//! Family a on the shell arm: `ReadFileContext` over the out-of-band channel.
//!
//! Files are fetched in framed batches (one section per file, see
//! [`crate::shellcmd::read::posix_file_section`]) and handed to the same
//! [`crate::shellcmd::read::assemble`] the helper uses, so the two arms cannot differ in what the
//! model sees. `~` is expanded on the host. A failure of the whole batch (the host has no base64
//! tool, the command could not run) is the whole-request error shape the app daemon uses: one
//! failure with an empty path.

use super::super::{BackendError, ShellBackend};
use crate::proto::{ReadFileContextRequest, ReadFileContextResponse};
use crate::shellcmd::read::{self, FileProbe, GatherPlan};
use crate::shellcmd::{Script, StderrMode};

/// The most encoded output one command is asked for. A file that alone exceeds it still runs in
/// a command of its own.
const GROUP_OUTPUT_BUDGET: usize = 8 * 1024 * 1024;

impl ShellBackend {
    pub(in crate::backend) async fn read_file_context_shell(
        &self,
        request: ReadFileContextRequest,
    ) -> Result<ReadFileContextResponse, BackendError> {
        let Some(executor) = self.executor.clone() else {
            return Err(BackendError::Unavailable(
                "read_file_context: this session has no out-of-band channel for remote commands"
                    .into(),
            ));
        };
        if request.files.is_empty() {
            return Ok(ReadFileContextResponse::default());
        }
        let pwsh = executor.shell_kind().is_powershell();
        let tool = if pwsh {
            None
        } else {
            match self.base64_tool().await {
                Ok(Some(tool)) => Some(tool),
                Ok(None) => {
                    return Ok(read::whole_request_error(
                        "The host has no base64, openssl or od, so files cannot be read in shell mode.",
                    ));
                }
                Err(e) => return Ok(read::whole_request_error(e.to_string())),
            }
        };

        let plans: Vec<GatherPlan> = request
            .files
            .iter()
            .map(|file| read::plan(file, &request))
            .collect();
        let mut probes: Vec<FileProbe> = Vec::with_capacity(plans.len());

        let mut start = 0;
        while start < plans.len() {
            // As many files as fit one command's output budget (at least one).
            let mut end = start;
            let mut estimate = 0usize;
            while end < plans.len() {
                let cost = plans[end].prefix_bytes.saturating_mul(2);
                if end > start && estimate + cost > GROUP_OUTPUT_BUDGET {
                    break;
                }
                estimate += cost;
                end += 1;
            }
            let mut script = match Script::new(Self::new_nonce()) {
                Ok(script) => script,
                Err(e) => return Ok(read::whole_request_error(e.to_string())),
            };
            for i in start..end {
                let path = &request.files[i].path;
                let command = if pwsh {
                    read::pwsh_file_section(path, &plans[i])
                } else {
                    let encoder = tool.map_or("base64", |t| t.encoder_filter());
                    read::posix_file_section(path, &plans[i], encoder)
                };
                script = match script.section(format!("f{i}"), command, StderrMode::Discard) {
                    Ok(script) => script,
                    Err(e) => return Ok(read::whole_request_error(e.to_string())),
                };
            }
            let frame = match self.run_script(&script).await {
                Ok(frame) => frame,
                Err(e) => return Ok(read::whole_request_error(e.to_string())),
            };
            for i in start..end {
                probes.push(match frame.section(&format!("f{i}")) {
                    Some(section) => read::parse_file_section(section, &plans[i], tool),
                    None => FileProbe::Failed("the host returned nothing for this file".into()),
                });
            }
            start = end;
        }
        Ok(read::assemble(&request, probes))
    }
}
