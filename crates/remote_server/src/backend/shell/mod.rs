//! The no-helper arm ("shell mode"): features served by running ordinary commands on the host
//! over an [`OutOfBandExecutor`]. Never the user's pty (ruling R1).
//!
//! P0 ships the skeleton: availability, running a framed [`Script`], and the cached base64 tool
//! probe. Every family's request methods answer `Unsupported` until its package fills them in
//! (P1..P9), or `Unavailable` when the session has no out-of-band channel.

mod agent_context;
mod buffer;
mod diff;
mod git_batch;
mod git_read;
mod git_write;
mod host_store;
mod index;
mod read;
mod read_bytes;
mod status;
mod tree;
mod write;

use std::sync::{Arc, Mutex};

pub use agent_context::AgentContextCache;
pub use buffer::ShellBuffers;
pub use git_batch::GitBatchStore;
pub use index::ShellIndex;
pub use status::PrLookup;
pub use tree::{ShellPush, TreeCache};

use super::executor::{ChannelKind, ExecRequest, MAX_EXEC_OUTPUT_BYTES, OutOfBandExecutor};
use super::shell_git_runner::ShellGitRunner;
use super::{BackendError, Family};
use crate::proto::{
    ListDirectoryResponse, LoadRepoMetadataDirectoryResponse, NavigatedToDirectoryResponse,
};
use crate::shellcmd::base64::{self, Base64Tool, DecodeTool};
use crate::shellcmd::{Frame, FrameError, Script, ShellKind, StderrMode};
use git_batch::BatchedGitRunner;

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
    /// The host's app-side codebase index state; `None` when no storage is configured (tests,
    /// callers that never index).
    index: Option<Arc<ShellIndex>>,
    /// The agent context this host's shell backend last published, with its revision. Shared by
    /// every backend the router builds for the host.
    agent_cache: Arc<AgentContextCache>,
    /// What the buffers this host's editors opened looked like, for the guarded save (family c).
    /// Shared by every backend the router builds for the host.
    buffers: Arc<ShellBuffers>,
    /// The login-shell wrapping that worked after the default one did not (an old nushell
    /// without raw strings), so later calls skip the failed attempt.
    kind_override: Arc<Mutex<Option<ShellKind>>>,
    /// The batched read-only git answers of this host's refreshes (#2156), shared by every
    /// backend the router builds for the host so the producers of one precmd find each other.
    git_batch: Arc<GitBatchStore>,
    /// `PHOSPHOR_SHELL_BATCH_GIT=0` (or a test): every git call is its own exec again.
    batch_git_off: bool,
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
            index: None,
            agent_cache: Arc::default(),
            buffers: Arc::default(),
            kind_override: Arc::default(),
            git_batch: Arc::default(),
            batch_git_off: !git_batch::batching_enabled(),
        }
    }

    /// Shares `store` (the host's batched git answers) with this backend.
    pub fn with_git_batch(mut self, store: Arc<GitBatchStore>) -> Self {
        self.git_batch = store;
        self
    }

    /// Turns the one-exec-per-refresh git batching on or off for this backend, overriding the
    /// `PHOSPHOR_SHELL_BATCH_GIT` kill switch (tests).
    pub fn with_git_batching(mut self, on: bool) -> Self {
        self.batch_git_off = !on;
        self
    }

    /// Shares `cache` (the host's agent-context state) with this backend.
    pub fn with_agent_cache(mut self, cache: Arc<AgentContextCache>) -> Self {
        self.agent_cache = cache;
        self
    }

    /// Shares `buffers` (the host's remembered buffer states) with this backend.
    pub fn with_buffers(mut self, buffers: Arc<ShellBuffers>) -> Self {
        self.buffers = buffers;
        self
    }

    /// Shares `cache` (the host's file-tree state) with this backend.
    pub fn with_tree_cache(mut self, cache: Arc<TreeCache>) -> Self {
        self.tree_cache = cache;
        self
    }

    /// Gives this backend the host's codebase-index state (its store root and text cache).
    pub fn with_index(mut self, index: Arc<ShellIndex>) -> Self {
        self.index = Some(index);
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

    /// The login-shell wrapping to use now: the executor's, unless an earlier call found another
    /// one that works (an old nushell).
    pub(super) fn login_kind(&self) -> ShellKind {
        (*self.kind_override.lock().unwrap_or_else(|e| e.into_inner()))
            .unwrap_or_else(|| self.shell_kind())
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
        self.run_script_stdin(script, Vec::new()).await
    }

    /// [`Self::run_script`] with `stdin` on the script's standard input: how user data (file
    /// chunks, vector records) reaches the host without appearing in any command line (#1864).
    /// Retried under another login-shell wrapping only if the first one fails to run the script.
    pub async fn run_script_stdin(
        &self,
        script: &Script,
        stdin: Vec<u8>,
    ) -> Result<Frame, BackendError> {
        let executor = self.executor.as_ref().ok_or_else(|| {
            BackendError::Unavailable("no out-of-band channel for remote commands".into())
        })?;
        let mut kind = (*self.kind_override.lock().unwrap_or_else(|e| e.into_inner()))
            .unwrap_or_else(|| executor.shell_kind());
        let mut retried = false;
        loop {
            let started = std::time::Instant::now();
            let result = executor
                .execute(ExecRequest {
                    command: script.render(kind),
                    stdin: stdin.clone(),
                    max_output_bytes: MAX_EXEC_OUTPUT_BYTES,
                })
                .await;
            // One line per exec channel (#2156): the sections name the producer.
            log::debug!(
                "shell exec: producer=run_script sections=[{}] elapsed_ms={} ok={}",
                script.section_names().join(","),
                started.elapsed().as_millis(),
                result.is_ok()
            );
            let output = result?;
            if output.truncated {
                return Err(BackendError::Unavailable(format!(
                    "remote output over the {} byte cap",
                    crate::shellcmd::MAX_OUTPUT_BYTES
                )));
            }
            match Frame::parse(&output.stdout, script.nonce()) {
                Ok(frame) => return Ok(frame),
                Err(FrameError::NoFrame) if kind == ShellKind::Nu && !retried => {
                    // Probably a nushell without raw strings: retry once with the escaped form.
                    retried = true;
                    kind = ShellKind::NuLegacy;
                    *self.kind_override.lock().unwrap_or_else(|e| e.into_inner()) = Some(kind);
                }
                Err(FrameError::NoFrame) if kind == ShellKind::NuLegacy && retried => {
                    // The escaped form failed too: the raw form was not the problem.
                    *self.kind_override.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    return Err(BackendError::Unavailable(
                        "the host's nushell did not run the remote script".into(),
                    ));
                }
                Err(FrameError::NoFrame) if foreign_shell_complaint(&output.stderr) => {
                    return Err(BackendError::Unavailable(
                        "the host's login shell is not POSIX, fish, nushell or PowerShell \
                         (csh or tcsh?), so remote commands cannot run there; the remote helper \
                         is needed on this host"
                            .into(),
                    ));
                }
                Err(e) => return Err(e.into()),
            }
        }
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

    /// A [`ShellGitRunner`] over this session's channel, or `Unavailable` naming `what`.
    pub(super) fn git_runner(&self, what: &'static str) -> Result<ShellGitRunner, BackendError> {
        Ok(ShellGitRunner::new(self.executor_for(what)?))
    }

    /// A git runner for one read-only refresh of `repo_path` (status chip, PR and repository
    /// chips, diff state): the answers one batched script gives for the whole refresh (shared
    /// with the other producers of the same precmd), and one exec for any call it does not
    /// cover. Plain per-call exec on PowerShell hosts and with the kill switch off.
    pub(super) async fn batched_git_runner(
        &self,
        what: &'static str,
        repo_path: &str,
    ) -> Result<BatchedGitRunner, BackendError> {
        let inner = self.git_runner(what)?;
        let data = if self.batch_git_off || self.shell_kind().is_powershell() {
            None
        } else {
            self.git_batch.get(self, &inner, repo_path).await
        };
        Ok(BatchedGitRunner::new(inner, repo_path, data))
    }

    /// The batched answers of this refresh of `path`, whose tree sections (`canon`, `git`, `files`,
    /// `ignoredpaths`, see [`git_batch::Prefetched::tree`]) the file tree reads instead of running
    /// its own script. `None`: no batch (kill switch, PowerShell host, the script failed), so the
    /// tree runs its own commands.
    pub(super) async fn batched_tree(&self, path: &str) -> Option<Arc<git_batch::Prefetched>> {
        if self.batch_git_off || self.shell_kind().is_powershell() {
            return None;
        }
        let inner = self.git_runner("file tree").ok()?;
        self.git_batch.get(self, &inner, path).await
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

/// Whether a login shell's stderr is the complaint of a shell that could not parse our POSIX
/// script (csh and tcsh reject its subshells, `$(..)`, `${..%..}` and multi-line quotes).
fn foreign_shell_complaint(stderr: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stderr);
    [
        "Unmatched",
        "Illegal variable name",
        "Undefined variable",
        "Badly placed",
        "Missing }",
        "Too many ('s",
        "Invalid null command",
        "Variable syntax",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::future::BoxFuture;

    use super::super::executor::{ChannelKind, ExecError, ExecOutput};
    use super::*;

    #[test]
    fn csh_complaints_are_recognised() {
        assert!(foreign_shell_complaint(b"Unmatched '.\n"));
        assert!(foreign_shell_complaint(b"Illegal variable name.\n"));
        assert!(!foreign_shell_complaint(
            b"Permission denied (publickey).\n"
        ));
        assert!(!foreign_shell_complaint(b""));
    }

    /// An executor with a scripted answer per attempt; it records every command it was given.
    #[derive(Debug)]
    struct Scripted {
        kind: ShellKind,
        seen: Mutex<Vec<String>>,
        /// Answers the Nth call (the last repeats): `None` stdout means "frame the section".
        answers: Vec<(Option<Vec<u8>>, Vec<u8>)>,
    }

    impl OutOfBandExecutor for Scripted {
        fn channel_kind(&self) -> ChannelKind {
            ChannelKind::LocalSh
        }
        fn shell_kind(&self) -> ShellKind {
            self.kind
        }
        fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
            let mut seen = self.seen.lock().unwrap();
            let n = seen.len();
            seen.push(request.command.clone());
            let (stdout, stderr) = self.answers[n.min(self.answers.len() - 1)].clone();
            let stdout = stdout.unwrap_or_else(|| {
                let nonce = request
                    .command
                    .split("@@PHOSPHOR-")
                    .nth(1)
                    .and_then(|rest| rest.split(' ').next())
                    .unwrap_or_default()
                    .to_string();
                format!("@@PHOSPHOR-{nonce} BEGIN x\nok\n@@PHOSPHOR-{nonce} END x exit=0\n")
                    .into_bytes()
            });
            Box::pin(async move {
                Ok(ExecOutput {
                    stdout,
                    stderr,
                    exit_code: Some(0),
                    truncated: false,
                })
            })
        }
    }

    fn one_section() -> Script {
        Script::new("n0nce")
            .unwrap()
            .section("x", "echo ok", StderrMode::Discard)
            .unwrap()
    }

    #[tokio::test]
    async fn an_old_nushell_without_raw_strings_gets_the_escaped_wrapping_and_keeps_it() {
        let exec = Arc::new(Scripted {
            kind: ShellKind::Nu,
            seen: Mutex::default(),
            // The raw-string form prints nothing usable; the escaped form works.
            answers: vec![
                (
                    Some(Vec::new()),
                    b"Error: nu::parser::parse_mismatch".to_vec(),
                ),
                (None, Vec::new()),
            ],
        });
        let shell = ShellBackend::new(Some(exec.clone()));
        let frame = shell.run_script(&one_section()).await.unwrap();
        assert_eq!(frame.section("x").unwrap().text(), "ok");
        {
            let seen = exec.seen.lock().unwrap();
            assert_eq!(seen.len(), 2);
            assert!(seen[0].starts_with("^sh -c r#'"), "{}", seen[0]);
            assert!(seen[1].starts_with("^sh -c \""), "{}", seen[1]);
        }
        // The next call goes straight to the form that worked.
        shell.run_script(&one_section()).await.unwrap();
        let seen = exec.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert!(seen[2].starts_with("^sh -c \""), "{}", seen[2]);
    }

    #[tokio::test]
    async fn a_login_shell_that_cannot_parse_the_script_is_unavailable_not_unreadable() {
        let exec = Arc::new(Scripted {
            kind: ShellKind::Posix,
            seen: Mutex::default(),
            answers: vec![(Some(Vec::new()), b"Unmatched '.\n".to_vec())],
        });
        let err = ShellBackend::new(Some(exec))
            .run_script(&one_section())
            .await
            .unwrap_err();
        match err {
            BackendError::Unavailable(why) => assert!(why.contains("csh"), "{why}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
}
