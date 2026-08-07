use std::path::{Path, PathBuf};

use futures::{future::BoxFuture, FutureExt};
use warpui::{Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

use crate::{
    ai::{
        agent::{
            AIAgentAction, AIAgentActionResultType, AIAgentActionType, AnyFileContent, FileContext,
            ReadFilesRequest, ReadFilesResult,
        },
        blocklist::BlocklistAIPermissions,
        paths::host_native_absolute_path,
    },
    terminal::model::session::{active_session::ActiveSession, SessionType},
};

use super::{
    read_local_file_context, ActionExecution, AnyActionExecution, ExecuteActionInput,
    PreprocessActionInput, ReadFileContextResult,
};

/// Reason reported for every path in [`ReadFileContextResult::missing_files`].
///
/// `read_local_file_context` collapses "not found", "over the byte limit" and
/// "could not be decoded" into a single list of paths, so the fork cannot name
/// the individual cause. It must therefore not claim the file does not exist —
/// the previous wording ("These files do not exist") was simply wrong for an
/// oversized binary or an unprocessable image. See the tracking note in
/// [`local_read_files_result`].
const FAILED_FILE_REASON: &str =
    "File does not exist or could not be read (missing, too large, or unprocessable)";

/// File name of the synthetic entry that carries the failure list back to the
/// model. See [`local_read_files_result`] for why the list travels as a file.
const FAILED_FILES_ENTRY_NAME: &str = "Files Failed";

pub struct ReadFilesExecutor {
    active_session: ModelHandle<ActiveSession>,
    terminal_view_id: EntityId,
}

impl ReadFilesExecutor {
    pub fn new(active_session: ModelHandle<ActiveSession>, terminal_view_id: EntityId) -> Self {
        Self {
            active_session,
            terminal_view_id,
        }
    }

    pub(super) fn should_autoexecute(
        &self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let ExecuteActionInput {
            action:
                AIAgentAction {
                    action: AIAgentActionType::ReadFiles(ReadFilesRequest { locations }),
                    ..
                },
            conversation_id,
        } = input
        else {
            return false;
        };

        // TODO: figure out how to avoid constructing the full paths in `should_execute`
        // and then again in `execute`, and then again on every render.
        let current_working_directory = self
            .active_session
            .as_ref(ctx)
            .current_working_directory()
            .cloned();
        let shell = self.active_session.as_ref(ctx).shell_launch_data(ctx);

        BlocklistAIPermissions::as_ref(ctx)
            .can_read_files_with_conversation(
                &conversation_id,
                locations
                    .iter()
                    .map(|file| {
                        PathBuf::from(host_native_absolute_path(
                            &file.name,
                            &shell,
                            &current_working_directory,
                        ))
                    })
                    .collect(),
                Some(self.terminal_view_id),
                ctx,
            )
            .is_allowed()
    }

    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> impl Into<AnyActionExecution> + use<> {
        let ExecuteActionInput {
            action,
            conversation_id,
            ..
        } = input;
        let AIAgentAction {
            action: AIAgentActionType::ReadFiles(ReadFilesRequest { locations }),
            ..
        } = action
        else {
            return ActionExecution::InvalidAction;
        };

        BlocklistAIPermissions::handle(ctx).update(ctx, |model, _ctx| {
            model.add_temporary_file_read_permissions(
                conversation_id,
                locations.iter().map(|file| Path::new(&file.name)),
            );
        });

        let current_working_directory = self
            .active_session
            .as_ref(ctx)
            .current_working_directory()
            .cloned();
        let shell = self.active_session.as_ref(ctx).shell_launch_data(ctx);

        let locations = locations.clone();

        // Check if this is a remote session with a connected host.
        let session_type = self.active_session.as_ref(ctx).session_type(ctx);
        let remote_client = match &session_type {
            Some(SessionType::WarpifiedRemote {
                host_id: Some(host_id),
            }) => remote_server::manager::RemoteServerManager::as_ref(ctx)
                .client_for_host(host_id)
                .cloned(),
            _ => None,
        };

        // Remote session without a usable remote server client. File reading
        // requires either local access or a connected remote server, neither
        // of which is available.
        if matches!(session_type, Some(SessionType::WarpifiedRemote { .. }))
            && remote_client.is_none()
        {
            return ActionExecution::Sync(AIAgentActionResultType::ReadFiles(
                ReadFilesResult::Error(
                    "The file read/edit tool is not available on this remote session. \
                     Try using a different tool."
                        .to_string(),
                ),
            ));
        }

        if let Some(client) = remote_client {
            return ActionExecution::Async {
                execute_future: Box::pin(async move {
                    let request = remote_server::proto::ReadFileContextRequest {
                        files: locations
                            .iter()
                            .map(|loc| {
                                let absolute_path = host_native_absolute_path(
                                    &loc.name,
                                    &shell,
                                    &current_working_directory,
                                );
                                remote_server::proto::ReadFileContextFile {
                                    path: absolute_path,
                                    line_ranges: loc
                                        .lines
                                        .iter()
                                        .map(|r| remote_server::proto::LineRange {
                                            start: r.start as u32,
                                            end: r.end as u32,
                                        })
                                        .collect(),
                                }
                            })
                            .collect(),
                        max_file_bytes: None,
                        max_batch_bytes: None,
                    };

                    let response = client
                        .read_file_context(request)
                        .await
                        .map_err(|e| anyhow::anyhow!("Remote read failed: {e}"))?;

                    if !response.failed_files.is_empty() && response.file_contexts.is_empty() {
                        let failed = response
                            .failed_files
                            .iter()
                            .map(|f| {
                                let reason = f
                                    .error
                                    .as_ref()
                                    .map(|e| e.message.as_str())
                                    .unwrap_or("unknown error");
                                format!("{}: {reason}", f.path)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Ok(ReadFilesResult::Error(format!(
                            "Failed to read files: {failed}"
                        )));
                    }

                    let file_contexts = response
                        .file_contexts
                        .into_iter()
                        .filter_map(|fc| {
                            let content = match fc.content? {
                                remote_server::proto::file_context_proto::Content::TextContent(
                                    text,
                                ) => crate::ai::agent::AnyFileContent::StringContent(text),
                                remote_server::proto::file_context_proto::Content::BinaryContent(
                                    bytes,
                                ) => crate::ai::agent::AnyFileContent::BinaryContent(bytes),
                            };
                            let line_range = match (fc.line_range_start, fc.line_range_end) {
                                (Some(start), Some(end)) => Some(start as usize..end as usize),
                                _ => None,
                            };
                            let last_modified = fc.last_modified_epoch_millis.map(|ms| {
                                std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms)
                            });
                            Some(crate::ai::agent::FileContext {
                                file_name: fc.file_name,
                                content,
                                line_range,
                                last_modified,
                                line_count: fc.line_count as usize,
                            })
                        })
                        .collect();

                    Ok(ReadFilesResult::Success {
                        files: file_contexts,
                    })
                }),
                on_complete: Box::new(|res: Result<ReadFilesResult, anyhow::Error>, _ctx| {
                    let action_result =
                        res.unwrap_or_else(|e| ReadFilesResult::Error(e.to_string()));
                    AIAgentActionResultType::ReadFiles(action_result)
                }),
            };
        }

        // Local path.
        ActionExecution::Async {
            execute_future: Box::pin(async move {
                let result = read_local_file_context(
                    &locations,
                    current_working_directory,
                    shell,
                    None,
                    None,
                )
                .await?;
                Ok(local_read_files_result(result))
            }),
            on_complete: Box::new(|res: Result<ReadFilesResult, anyhow::Error>, _ctx| {
                let action_result = res.unwrap_or_else(|e| ReadFilesResult::Error(e.to_string()));
                AIAgentActionResultType::ReadFiles(action_result)
            }),
        }
    }

    pub(super) fn preprocess_action(
        &mut self,
        _input: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }
}

impl Entity for ReadFilesExecutor {
    type Event = ();
}

/// Renders a batch of read failures as one `path: reason` entry per file, the
/// shape Warp's `describe_failed_files` produces for its error message.
fn describe_failed_files(failed_files: &[String]) -> String {
    failed_files
        .iter()
        .map(|path| format!("{path}: {FAILED_FILE_REASON}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Builds the synthetic [`FileContext`] that carries the failure list, using
/// the same per-file lines Warp renders under its `**Files Failed:**` section.
fn failed_files_context(failed_files: &[String]) -> FileContext {
    let body = failed_files
        .iter()
        .map(|path| format!("- **{path}**: {FAILED_FILE_REASON}"))
        .collect::<Vec<_>>()
        .join("\n");
    FileContext::new(
        FAILED_FILES_ENTRY_NAME.to_string(),
        AnyFileContent::StringContent(body),
        None,
        None,
    )
}

/// Turns a local [`ReadFileContextResult`] into the action result handed to the
/// model.
///
/// Mirrors Warp's three-way split: everything read is a plain success, nothing
/// read is an error, and a mixed batch is a success that *also* reports what
/// failed. Before this split a single bad path threw away every file that had
/// been read successfully (issue #136).
///
/// ## Sanctioned divergence from Warp (refs #136, #11)
///
/// Warp's `ReadFilesResult::Success` carries `failed_files` next to `files` and
/// renders it as a `**Files Failed:**` section. The fork's variant is
/// `Success { files }` only, because its pinned `warp-proto-apis` has no
/// `failed_reads` field on `ReadFilesResult` — and that wire format is exactly
/// what the BYOP model reads (`ai::agent_providers::tools` serializes the proto
/// into the `role=tool` JSON). There is therefore no field, on the wire or off
/// it, to put the failure list in.
///
/// So the list travels as a trailing synthetic text entry named
/// [`FAILED_FILES_ENTRY_NAME`], whose body is Warp's per-file failure lines.
/// This is acceptable because the property that matters — the model learns both
/// the content it got *and* which paths it did not get — is preserved on every
/// model-facing path (proto → BYOP JSON, `MarkdownActionResult`,
/// `ReadFilesResult`'s own `Display`). The alternative of returning
/// `Success { files }` and dropping the list would hide the failures entirely,
/// i.e. trade issue #136's loud data loss for the silent truncation the same
/// issue files against the remote path. The cost is cosmetic: the GUI lists the
/// entry among the read files instead of styling it as a separate failure
/// section.
///
/// Revisit when the proto is re-pinned (#11): this collapses onto Warp's
/// `Success { files, failed_files }` and the synthetic entry goes away.
fn local_read_files_result(result: ReadFileContextResult) -> ReadFilesResult {
    if result.missing_files.is_empty() {
        ReadFilesResult::Success {
            files: result.file_contexts,
        }
    } else if result.file_contexts.is_empty() {
        let failed_files = describe_failed_files(&result.missing_files);
        ReadFilesResult::Error(format!("Failed to read files: {failed_files}"))
    } else {
        let mut files = result.file_contexts;
        files.push(failed_files_context(&result.missing_files));
        ReadFilesResult::Success { files }
    }
}

#[cfg(test)]
#[path = "read_files_tests.rs"]
mod tests;
