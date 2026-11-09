//! The no-helper arm ("shell mode"): features served by running ordinary commands on the host
//! over an [`OutOfBandExecutor`]. Never the user's pty (ruling R1).
//!
//! P0 ships the skeleton: availability, running a framed [`Script`], and the cached base64 tool
//! probe. Every family's request methods answer `Unsupported` until its package fills them in
//! (P1..P9), or `Unavailable` when the session has no out-of-band channel.

mod read;
mod tree;
mod write;

use std::sync::{Arc, Mutex};

pub use tree::{ShellPush, TreeCache};

use super::executor::{ChannelKind, ExecRequest, MAX_EXEC_OUTPUT_BYTES, OutOfBandExecutor};
use super::{BackendError, Family};
use crate::proto::{
    ListDirectoryResponse, LoadRepoMetadataDirectoryResponse, NavigatedToDirectoryResponse,
};
use crate::shellcmd::base64::{self, Base64Tool, DecodeTool};
use crate::shellcmd::{Frame, Script, ShellKind, StderrMode};

#[derive(Clone, Default)]
pub struct ShellBackend {
    executor: Option<Arc<dyn OutOfBandExecutor>>,
    /// The host's base64 tool, probed once: `None` not probed yet, `Some(None)` probed and the
    /// host has none.
    base64_tool: Arc<Mutex<Option<Option<Base64Tool>>>>,
    /// The file trees this host's shell backend has reported, kept between calls so a refresh can
    /// tell the app only what changed. Shared by every backend the router builds for the host.
    tree_cache: Arc<TreeCache>,
    /// Pushes (file-tree snapshots and updates) the last calls produced, for the caller to drain
    /// with [`Self::take_pushes`]. The helper sends these unprompted; the shell backend has no
    /// connection to push on, so it hands them back.
    pushes: Arc<Mutex<Vec<ShellPush>>>,
    /// The host's base64 decoder (for writes), probed once like the encoder.
    decode_tool: Arc<Mutex<Option<Option<DecodeTool>>>>,
}

impl std::fmt::Debug for ShellBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellBackend")
            .field("channel", &self.channel_kind())
            .finish()
    }
}

impl ShellBackend {
    /// `executor` is `None` for a session with no out-of-band channel; every call is then
    /// `Unavailable`.
    pub fn new(executor: Option<Arc<dyn OutOfBandExecutor>>) -> Self {
        Self {
            executor,
            base64_tool: Arc::default(),
            tree_cache: Arc::default(),
            pushes: Arc::default(),
            decode_tool: Arc::default(),
        }
    }

    /// Shares `cache` (the host's file-tree state) with this backend.
    pub fn with_tree_cache(mut self, cache: Arc<TreeCache>) -> Self {
        self.tree_cache = cache;
        self
    }

    /// Drains the pushes produced since the last call, oldest first.
    pub fn take_pushes(&self) -> Vec<ShellPush> {
        std::mem::take(&mut *self.pushes.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub(super) fn navigate_to_directory_shell(
        &self,
        path: String,
    ) -> impl std::future::Future<Output = Result<NavigatedToDirectoryResponse, BackendError>> + Send + '_
    {
        tree::navigate_to_directory(self, path)
    }

    pub(super) fn load_repo_metadata_directory_shell(
        &self,
        repo_path: String,
        dir_path: String,
    ) -> impl std::future::Future<Output = Result<LoadRepoMetadataDirectoryResponse, BackendError>>
    + Send
    + '_ {
        tree::load_repo_metadata_directory(self, repo_path, dir_path)
    }

    pub(super) fn list_directory_shell(
        &self,
        path: String,
    ) -> impl std::future::Future<Output = Result<ListDirectoryResponse, BackendError>> + Send + '_
    {
        tree::list_directory(self, path)
    }

    /// Re-reads the indexed root `repo_path` and queues the difference as a push (see
    /// [`ShellPush`]). Called on prompt/cwd/user-action events, never on a timer.
    pub async fn refresh_repo_tree(&self, repo_path: &str) -> Result<(), BackendError> {
        tree::refresh_repo_tree(self, repo_path).await
    }

    /// The roots whose tree this backend has reported.
    pub fn indexed_roots(&self) -> Vec<String> {
        self.tree_cache.roots()
    }

    pub(super) fn push(&self, push: ShellPush) {
        self.pushes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(push);
    }

    /// The host's login shell family (POSIX when there is no channel; every call then fails
    /// before using it).
    pub(super) fn shell_kind(&self) -> ShellKind {
        self.executor
            .as_ref()
            .map_or(ShellKind::Posix, |e| e.shell_kind())
    }

    pub fn channel_kind(&self) -> Option<ChannelKind> {
        self.executor.as_ref().map(|e| e.channel_kind())
    }

    pub fn is_available(&self) -> bool {
        self.executor.is_some()
    }

    /// The error for a request this backend does not implement (yet).
    pub(super) fn not_implemented(&self, family: Family, what: &'static str) -> BackendError {
        if self.executor.is_none() {
            BackendError::Unavailable(format!(
                "{what}: this session has no out-of-band channel for remote commands"
            ))
        } else {
            BackendError::Unsupported { family, what }
        }
    }

    /// Runs `script` on the host and parses the framed result.
    pub async fn run_script(&self, script: &Script) -> Result<Frame, BackendError> {
        let executor = self.executor.as_ref().ok_or_else(|| {
            BackendError::Unavailable("no out-of-band channel for remote commands".into())
        })?;
        let output = executor
            .execute(ExecRequest {
                command: script.render(executor.shell_kind()),
                stdin: Vec::new(),
                max_output_bytes: MAX_EXEC_OUTPUT_BYTES,
            })
            .await?;
        if output.truncated {
            return Err(BackendError::Unavailable(format!(
                "remote output over the {} byte cap",
                crate::shellcmd::MAX_OUTPUT_BYTES
            )));
        }
        Ok(Frame::parse(&output.stdout, script.nonce())?)
    }

    /// A fresh per-call nonce for [`Script::new`].
    pub fn new_nonce() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    /// The host's base64 tool (`None`: it has none), probed on first use and cached.
    pub async fn base64_tool(&self) -> Result<Option<Base64Tool>, BackendError> {
        if let Some(cached) = *self.base64_tool.lock().unwrap_or_else(|e| e.into_inner()) {
            return Ok(cached);
        }
        let script = Script::new(Self::new_nonce())?.section(
            "b64",
            base64::probe_command(),
            StderrMode::Discard,
        )?;
        let frame = self.run_script(&script).await?;
        let tool = frame
            .section("b64")
            .and_then(|s| Base64Tool::from_probe_output(&s.text()));
        *self.base64_tool.lock().unwrap_or_else(|e| e.into_inner()) = Some(tool);
        Ok(tool)
    }

    /// The host's base64 decoder (`None`: it has none), probed on first use and cached.
    pub async fn decode_tool(&self) -> Result<Option<DecodeTool>, BackendError> {
        if let Some(cached) = *self.decode_tool.lock().unwrap_or_else(|e| e.into_inner()) {
            return Ok(cached);
        }
        let script = Script::new(Self::new_nonce())?.section(
            "dec",
            base64::decode_probe_command(),
            StderrMode::Discard,
        )?;
        let frame = self.run_script(&script).await?;
        let tool = frame
            .section("dec")
            .and_then(|s| DecodeTool::from_probe_output(&s.text()));
        *self.decode_tool.lock().unwrap_or_else(|e| e.into_inner()) = Some(tool);
        Ok(tool)
    }

    /// The out-of-band executor, or `Unavailable` naming `what`.
    pub(super) fn executor_for(
        &self,
        what: &'static str,
    ) -> Result<Arc<dyn OutOfBandExecutor>, BackendError> {
        self.executor.clone().ok_or_else(|| {
            BackendError::Unavailable(format!(
                "{what}: this session has no out-of-band channel for remote commands"
            ))
        })
    }
}
