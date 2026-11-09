//! Writes, deletes and whole-file reads on the shell arm (`WriteFile`, `DeleteFile`,
//! `read_file_bytes`), for the agent's `apply_file_diffs` and the file model's guarded saves.
//!
//! Failures are reported the way the helper reports them (a file-operation failure with the
//! host's reason), so a call site cannot tell the arms apart.

use super::super::{BackendError, Family, ShellBackend};
use crate::client::ClientError;
use crate::shellcmd::read::{self, FileProbe, GatherPlan};
use crate::shellcmd::write::{
    ChunkPosition, EXIT_CONFLICT, Expect, posix_delete_command, posix_write_command,
    pwsh_delete_command, pwsh_write_command, split_chunks, write_failure_message,
};
use crate::shellcmd::{Script, StderrMode};

/// The most a whole-file read returns: what base64 of it still fits one command's output.
const READ_BYTES_CAP: usize = 7 * 1024 * 1024;

/// What a guarded write did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    Written,
    /// The target no longer matched the expectation; nothing was changed.
    Conflict,
}

pub(super) fn op_failed(message: impl Into<String>) -> BackendError {
    BackendError::Client(ClientError::FileOperationFailed(message.into()))
}

impl ShellBackend {
    pub(in crate::backend) async fn write_file_shell(
        &self,
        path: String,
        content: String,
    ) -> Result<(), BackendError> {
        self.write_file_expecting(path, content, None).await
    }

    /// Writes `content` to `path` (creating its directories), but only if the file is still in
    /// the state `expect` names; `None` writes unconditionally.
    pub async fn write_file_expecting(
        &self,
        path: String,
        content: String,
        expect: Option<Expect>,
    ) -> Result<(), BackendError> {
        match self
            .write_file_outcome(path.clone(), content, expect)
            .await?
        {
            WriteOutcome::Written => Ok(()),
            WriteOutcome::Conflict => {
                Err(op_failed(write_failure_message(&path, EXIT_CONFLICT, "")))
            }
        }
    }

    /// [`Self::write_file_expecting`], but a target that no longer matches `expect` is an
    /// outcome the caller can act on (a buffer save shows its conflict banner), not an error.
    pub async fn write_file_outcome(
        &self,
        path: String,
        content: String,
        expect: Option<Expect>,
    ) -> Result<WriteOutcome, BackendError> {
        log::debug!("shell arm serving {:?}", Family::FileWrite);
        let executor = self.executor_for("write_file")?;
        let pwsh = executor.shell_kind().is_powershell();
        // POSIX hosts decode base64 when they can; with no decoder the bytes travel as printf
        // octal escapes (smaller chunks).
        let decoder = if pwsh {
            None
        } else {
            self.decode_tool().await?
        };
        let printf = !pwsh && decoder.is_none();
        let nonce = Self::new_nonce();
        let bytes = content.into_bytes();
        let chunks = split_chunks(&bytes, printf);
        let count = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            let position = ChunkPosition {
                first: i == 0,
                last: i + 1 == count,
            };
            let command = if pwsh {
                pwsh_write_command(&path, &nonce, chunk, position, expect)
            } else {
                posix_write_command(&path, &nonce, chunk, position, decoder, expect)
            };
            let script =
                Script::new(Self::new_nonce())?.section("w", command, StderrMode::Merge)?;
            let frame = self.run_script(&script).await?;
            let section = frame.section("w").ok_or_else(|| {
                op_failed(format!("Failed to write {path}: no answer from the host"))
            })?;
            if section.exit == EXIT_CONFLICT {
                return Ok(WriteOutcome::Conflict);
            }
            if section.exit != 0 {
                return Err(op_failed(write_failure_message(
                    &path,
                    section.exit,
                    &section.text(),
                )));
            }
        }
        Ok(WriteOutcome::Written)
    }

    pub(in crate::backend) async fn delete_file_shell(
        &self,
        path: String,
    ) -> Result<(), BackendError> {
        let executor = self.executor_for("delete_file")?;
        let command = if executor.shell_kind().is_powershell() {
            pwsh_delete_command(&path)
        } else {
            posix_delete_command(&path)
        };
        let script = Script::new(Self::new_nonce())?.section("d", command, StderrMode::Merge)?;
        let frame = self.run_script(&script).await?;
        match frame.section("d") {
            Some(section) if section.exit == 0 => Ok(()),
            Some(section) => {
                let detail = section.text();
                let detail = detail.trim();
                Err(op_failed(if detail.is_empty() {
                    format!("Failed to delete {path}")
                } else {
                    format!("Failed to delete {path}: {detail}")
                }))
            }
            None => Err(op_failed(format!(
                "Failed to delete {path}: no answer from the host"
            ))),
        }
    }

    /// The whole file's bytes (up to [`READ_BYTES_CAP`]), as the helper's chunked read returns
    /// them.
    pub(in crate::backend) async fn read_file_bytes_shell(
        &self,
        path: String,
    ) -> Result<Vec<u8>, BackendError> {
        let executor = self.executor_for("read_file_bytes")?;
        let pwsh = executor.shell_kind().is_powershell();
        let plan = GatherPlan {
            prefix_bytes: READ_BYTES_CAP,
            through_line: None,
        };
        let tool = if pwsh {
            None
        } else {
            match self.base64_tool().await? {
                Some(tool) => Some(tool),
                None => {
                    return Err(op_failed(
                        "the host has no base64, openssl or od, so files cannot be read in shell mode",
                    ));
                }
            }
        };
        let command = if pwsh {
            read::pwsh_file_section(&path, &plan)
        } else {
            let encoder = tool.map_or("base64", |t| t.encoder_filter());
            read::posix_file_section(&path, &plan, encoder)
        };
        let script = Script::new(Self::new_nonce())?.section("r", command, StderrMode::Discard)?;
        let frame = self.run_script(&script).await?;
        let Some(section) = frame.section("r") else {
            return Err(op_failed(format!(
                "Failed to read {path}: no answer from the host"
            )));
        };
        match read::parse_file_section(section, &plan, tool) {
            FileProbe::NotFound => Err(op_failed(format!(
                "Failed to read {path}: File does not exist"
            ))),
            FileProbe::Failed(reason) => Err(op_failed(format!("Failed to read {path}: {reason}"))),
            FileProbe::Loaded(file) if file.complete => Ok(file.bytes),
            FileProbe::Loaded(_) => Err(op_failed(format!(
                "Failed to read {path}: the file is too large to read in shell mode"
            ))),
        }
    }
}
