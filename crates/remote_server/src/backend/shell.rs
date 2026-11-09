//! The no-helper arm ("shell mode"): features served by running ordinary commands on the host
//! over an [`OutOfBandExecutor`]. Never the user's pty (ruling R1).
//!
//! P0 ships the skeleton: availability, running a framed [`Script`], and the cached base64 tool
//! probe. Every family's request methods answer `Unsupported` until its package fills them in
//! (P1..P9), or `Unavailable` when the session has no out-of-band channel.

use std::sync::{Arc, Mutex};

use super::executor::{ChannelKind, ExecRequest, MAX_EXEC_OUTPUT_BYTES, OutOfBandExecutor};
use super::{BackendError, Family};
use crate::shellcmd::base64::{self, Base64Tool};
use crate::shellcmd::{Frame, Script, StderrMode};

#[derive(Clone, Default)]
pub struct ShellBackend {
    executor: Option<Arc<dyn OutOfBandExecutor>>,
    /// The host's base64 tool, probed once: `None` not probed yet, `Some(None)` probed and the
    /// host has none.
    base64_tool: Arc<Mutex<Option<Option<Base64Tool>>>>,
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
        }
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
}
