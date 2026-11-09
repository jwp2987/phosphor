//! Family c on the shell arm: buffer editing with NO server-side buffer (design ruling R4).
//!
//! * `open_buffer`: the file is read whole over the out-of-band channel, together with its state
//!   (SHA-256 when the host has a tool for it, else size and mtime). The state is remembered per
//!   path, per host, in [`ShellBuffers`].
//! * Edits (`BufferEdit`) never leave the app: the editor's text is the only copy.
//! * `save`: [`ShellBackend::save_buffer_content`] writes the editor's text through the guarded
//!   write ([`ShellBackend::write_file_outcome`]) with the remembered state as the precondition.
//!   A file that changed since is a [`BufferSaveOutcome::Conflict`], which the app shows with the
//!   same banner as the helper's `BufferConflictDetected`; the next save of that path (the
//!   banner's "overwrite") is unconditional, and a re-open (the banner's "discard") starts over.
//!
//! What this arm cannot do, by design: nothing pushes another editor's change into an open
//! buffer, and a conflict shows only when the user saves. `server_version` is a constant 1 (no
//! pushes carry one), so the app's version bookkeeping stays inert.

use std::collections::HashMap;
use std::sync::Mutex;

use super::super::{BackendError, BufferSaveOutcome, Family, ShellBackend};
use super::write::{WriteOutcome, op_failed};
use crate::proto::{OpenBufferResponse, ResolveConflictResponse, SaveBufferResponse};
use crate::proto::{ResolveConflictSuccess, resolve_conflict_response};
use crate::shellcmd::buffer::{parse_sha, parse_stamp, sha_command, stamp_command};
use crate::shellcmd::read::{self, FileProbe, GatherPlan};
use crate::shellcmd::write::Expect;
use crate::shellcmd::{Frame, Script, StderrMode};

/// The `server_version` every shell-mode open reports.
const SHELL_SERVER_VERSION: u64 = 1;

/// The most a buffer opens (the helper's limit too).
const READ_BYTES_CAP: usize = 7 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct Base {
    /// What the file looked like when the editor last loaded or saved it; `None` when the host
    /// would not say (the save is then unguarded).
    expect: Option<Expect>,
    /// A save found the file changed: the next one is the user's "overwrite".
    conflicted: bool,
}

/// The remembered state of every buffer one host's shell backend has opened, by path. Shared by
/// every backend the router builds for the host.
#[derive(Default)]
pub struct ShellBuffers {
    bases: Mutex<HashMap<String, Base>>,
}

impl std::fmt::Debug for ShellBuffers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellBuffers")
            .field("open", &self.len())
            .finish()
    }
}

impl ShellBuffers {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Base>> {
        self.bases.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, path: &str, base: Base) {
        self.lock().insert(path.to_owned(), base);
    }

    fn get(&self, path: &str) -> Option<Base> {
        self.lock().get(path).copied()
    }

    fn mark_conflicted(&self, path: &str) {
        if let Some(base) = self.lock().get_mut(path) {
            base.conflicted = true;
        }
    }

    /// The editor closed `path`: forget what it looked like.
    pub fn forget(&self, path: &str) {
        self.lock().remove(path);
    }

    /// How many buffers are remembered.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }
}

/// The precondition the two state sections describe: the hash when the host printed one, else
/// the stamp.
fn expect_from(frame: &Frame) -> Option<Expect> {
    if let Some(digest) = frame
        .section("sha")
        .filter(|s| s.exit == 0)
        .and_then(|s| parse_sha(&s.text()))
    {
        return Some(Expect::Sha256(digest));
    }
    frame
        .section("st")
        .filter(|s| s.exit == 0)
        .and_then(|s| parse_stamp(&s.text()))
        .map(Expect::Stamp)
}

impl ShellBackend {
    /// Reads the state sections (and the content when `read_plan` is given) in one round trip.
    async fn buffer_script(&self, path: &str, with_content: bool) -> Result<Frame, BackendError> {
        let executor = self.executor_for("buffer")?;
        let pwsh = executor.shell_kind().is_powershell();
        // The state is taken before the content: a change in between makes the next save report
        // a conflict, never overwrite one.
        let mut script = Script::new(Self::new_nonce())?
            .section("st", stamp_command(pwsh, path), StderrMode::Discard)?
            .section("sha", sha_command(pwsh, path), StderrMode::Discard)?;
        if with_content {
            let plan = GatherPlan {
                prefix_bytes: READ_BYTES_CAP,
                through_line: None,
            };
            let command = if pwsh {
                read::pwsh_file_section(path, &plan)
            } else {
                let tool = self.base64_tool().await?.ok_or_else(|| {
                    op_failed(
                        "the host has no base64, openssl or od, so files cannot be opened in shell mode",
                    )
                })?;
                read::posix_file_section(path, &plan, tool.encoder_filter())
            };
            script = script.section("r", command, StderrMode::Discard)?;
        }
        self.run_script(&script).await
    }

    /// `open_buffer` without a server buffer: the file's text now, and its state remembered for
    /// the save.
    pub(in crate::backend) async fn open_buffer_shell(
        &self,
        path: String,
        _force_reload: bool,
    ) -> Result<OpenBufferResponse, BackendError> {
        log::debug!("shell arm serving {:?}", Family::Buffers);
        let frame = self.buffer_script(&path, true).await?;
        let plan = GatherPlan {
            prefix_bytes: READ_BYTES_CAP,
            through_line: None,
        };
        let tool = if self.shell_kind().is_powershell() {
            None
        } else {
            self.base64_tool().await?
        };
        let Some(section) = frame.section("r") else {
            return Err(op_failed(format!(
                "Failed to load buffer {path}: no answer from the host"
            )));
        };
        let bytes = match read::parse_file_section(section, &plan, tool) {
            FileProbe::NotFound => {
                return Err(op_failed(format!(
                    "Failed to load buffer {path}: File does not exist"
                )));
            }
            FileProbe::Failed(reason) => {
                return Err(op_failed(format!("Failed to load buffer {path}: {reason}")));
            }
            FileProbe::Loaded(file) if file.complete => file.bytes,
            FileProbe::Loaded(_) => {
                return Err(op_failed(format!(
                    "Failed to load buffer {path}: the file is too large to edit remotely"
                )));
            }
        };
        let content = String::from_utf8(bytes).map_err(|_| {
            op_failed(format!(
                "Failed to load buffer {path}: the file is not valid UTF-8 text"
            ))
        })?;
        let expect = expect_from(&frame);
        if expect.is_none() {
            log::warn!("shell buffer: the host reports no file state, saves are unguarded");
        }
        self.buffers.set(
            &path,
            Base {
                expect,
                conflicted: false,
            },
        );
        Ok(OpenBufferResponse {
            content,
            server_version: SHELL_SERVER_VERSION,
        })
    }

    /// The plain `save_buffer(path)` has no text to write on this arm (the text lives in the
    /// editor): callers use [`Self::save_buffer_content`].
    pub(in crate::backend) async fn save_buffer_shell(
        &self,
        _path: String,
    ) -> Result<SaveBufferResponse, BackendError> {
        Err(self.not_implemented_save())
    }

    fn not_implemented_save(&self) -> BackendError {
        if self.is_available() {
            BackendError::Unsupported {
                family: Family::Buffers,
                what: "save_buffer without text (the shell arm saves the editor's text: save_buffer_content)",
            }
        } else {
            BackendError::Unavailable(
                "save_buffer: this session has no out-of-band channel for remote commands".into(),
            )
        }
    }

    /// Writes the editor's `content` to `path` if the file is still what it was when the editor
    /// loaded or last saved it. See the module docs for the conflict flow.
    pub async fn save_buffer_content(
        &self,
        path: String,
        content: String,
    ) -> Result<BufferSaveOutcome, BackendError> {
        log::debug!("shell arm serving {:?}", Family::Buffers);
        let expect = match self.buffers.get(&path) {
            // The user chose "overwrite" after a conflict.
            Some(Base {
                conflicted: true, ..
            }) => None,
            Some(Base { expect, .. }) => expect,
            None => {
                log::warn!("shell buffer saved without a remembered state: unguarded");
                None
            }
        };
        match self
            .write_file_outcome(path.clone(), content, expect)
            .await?
        {
            WriteOutcome::Conflict => {
                self.buffers.mark_conflicted(&path);
                Ok(BufferSaveOutcome::Conflict)
            }
            WriteOutcome::Written => {
                self.rebase(&path).await;
                Ok(BufferSaveOutcome::Saved)
            }
        }
    }

    /// Remembers the file as it is now (after our own write).
    async fn rebase(&self, path: &str) {
        let expect = match self.buffer_script(path, false).await {
            Ok(frame) => expect_from(&frame),
            Err(e) => {
                log::warn!("shell buffer: could not read the file state after a save: {e}");
                None
            }
        };
        self.buffers.set(
            path,
            Base {
                expect,
                conflicted: false,
            },
        );
    }

    /// "Keep mine": writes the client's text unconditionally. There is no server version to
    /// check on this arm.
    pub(in crate::backend) async fn resolve_conflict_shell(
        &self,
        path: String,
        _acknowledged_server_version: u64,
        client_content: String,
        _current_client_version: u64,
    ) -> Result<ResolveConflictResponse, BackendError> {
        log::debug!("shell arm serving {:?}", Family::Buffers);
        self.write_file_outcome(path.clone(), client_content, None)
            .await?;
        self.rebase(&path).await;
        Ok(ResolveConflictResponse {
            result: Some(resolve_conflict_response::Result::Success(
                ResolveConflictSuccess {},
            )),
        })
    }

    /// The editor closed `path`.
    pub fn close_buffer_shell(&self, path: &str) {
        self.buffers.forget(path);
    }
}
