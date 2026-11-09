//! Reading past the first section's byte cap on the shell arm (#1873): a file larger than one
//! command's output is read as consecutive `dd skip/count` sections, each fitting the cap, so
//! `read_file_bytes` and `open_buffer` do not refuse big files only because there is no helper
//! (the helper's `ReadFileChunk` loops without a limit).

use super::super::{BackendError, ShellBackend};
use super::write::op_failed;
use crate::shellcmd::read::{
    RANGE_CHUNK_BYTES, parse_range_section, posix_range_section, pwsh_range_section,
};
use crate::shellcmd::{Script, StderrMode};

/// The largest file a chunked shell read assembles in memory.
pub(super) const CHUNKED_READ_LIMIT: u64 = 1024 * 1024 * 1024;

impl ShellBackend {
    /// `have` holds the first `have.len()` bytes of `path` (a multiple of the range block size)
    /// and the file is `size` bytes: reads the rest in chunks and returns the whole file.
    /// `what` names the operation in a refusal (`read`, `load buffer`).
    pub(super) async fn read_remaining_bytes(
        &self,
        path: &str,
        mut have: Vec<u8>,
        size: u64,
        limit: u64,
        what: &str,
    ) -> Result<Vec<u8>, BackendError> {
        let limit = limit.min(CHUNKED_READ_LIMIT);
        if size > limit {
            return Err(op_failed(format!(
                "Failed to {what} {path}: the file is too large ({size} bytes, the limit is {limit})"
            )));
        }
        let pwsh = self.shell_kind().is_powershell();
        let tool = if pwsh {
            None
        } else {
            Some(self.base64_tool().await?.ok_or_else(|| {
                op_failed(
                    "the host has no base64, openssl or od, so files cannot be read in shell mode",
                )
            })?)
        };
        while (have.len() as u64) < size {
            let offset = have.len() as u64;
            if offset % crate::shellcmd::read::RANGE_BLOCK_BYTES != 0 {
                return Err(op_failed(format!(
                    "Failed to {what} {path}: the host returned a partial block"
                )));
            }
            let len = RANGE_CHUNK_BYTES.min(size - offset);
            let command = if pwsh {
                pwsh_range_section(path, offset, len)
            } else {
                let encoder = tool.map_or("base64", |t| t.encoder_filter());
                posix_range_section(path, offset, len, encoder)
            };
            let script =
                Script::new(Self::new_nonce())?.section("r", command, StderrMode::Discard)?;
            let frame = self.run_script(&script).await?;
            let section = frame.section("r").ok_or_else(|| {
                op_failed(format!("Failed to {what} {path}: no answer from the host"))
            })?;
            let chunk = parse_range_section(section, tool)
                .map_err(|reason| op_failed(format!("Failed to {what} {path}: {reason}")))?;
            if chunk.is_empty() {
                // The file shrank while it was read.
                return Err(op_failed(format!(
                    "Failed to {what} {path}: the file changed while it was being read"
                )));
            }
            have.extend_from_slice(&chunk);
        }
        have.truncate(size as usize);
        Ok(have)
    }
}
