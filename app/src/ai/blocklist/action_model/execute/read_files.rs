use std::path::{Path, PathBuf};

use futures::{FutureExt, future::BoxFuture};
use warpui::{Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

use crate::{
    ai::{
        agent::{
            AIAgentAction, AIAgentActionResultType, AIAgentActionType,
            ReadFilesFailedFile, ReadFilesRequest, ReadFilesResult,
        },
        blocklist::BlocklistAIPermissions,
        paths::host_native_absolute_path_for,
    },
    terminal::model::session::{SessionType, active_session::ActiveSession},
};

use super::{
    AGENT_READ_FILES_BATCH_BYTES, AGENT_READ_FILES_MAX_FILES, ActionExecution, AnyActionExecution,
    ExecuteActionInput, PreprocessActionInput, ReadFileContextResult, cap_read_batch,
    convert_remote_file_contexts, describe_failed_files, read_local_file_context,
};

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

        // Fail closed: a `~` we cannot resolve on a remote session is judged against the wrong
        // host's path (#1288).
        if locations.iter().any(|file| {
            super::session_tilde_unresolved(self.active_session.as_ref(ctx), &file.name, ctx)
        }) {
            return false;
        }

        BlocklistAIPermissions::as_ref(ctx)
            .can_read_files_with_conversation(
                &conversation_id,
                locations
                    .iter()
                    .map(|file| {
                        PathBuf::from(host_native_absolute_path_for(
                            &super::expand_session_tilde(
                                self.active_session.as_ref(ctx),
                                &file.name,
                                ctx,
                            ),
                            &shell,
                            &current_working_directory,
                            super::session_path_host(self.active_session.as_ref(ctx), ctx),
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

        let expanded_names: Vec<String> = locations
            .iter()
            .map(|file| {
                super::expand_session_tilde(self.active_session.as_ref(ctx), &file.name, ctx)
            })
            .collect();
        BlocklistAIPermissions::handle(ctx).update(ctx, |model, _ctx| {
            model.add_temporary_file_read_permissions(
                conversation_id,
                // The resolved path, so the grant matches the absolute path later checks use.
                expanded_names.iter().map(Path::new),
            );
        });

        let current_working_directory = self
            .active_session
            .as_ref(ctx)
            .current_working_directory()
            .cloned();
        let shell = self.active_session.as_ref(ctx).shell_launch_data(ctx);

        // On a remote session `~` is the remote user's home, not ours (#1278).
        let locations: Vec<_> = locations
            .iter()
            .cloned()
            .map(|mut location| {
                location.name = super::expand_session_tilde(
                    self.active_session.as_ref(ctx),
                    &location.name,
                    ctx,
                );
                location
            })
            .collect();

        let path_host = super::session_path_host(self.active_session.as_ref(ctx), ctx);

        // Cap the batch; files past the cap are reported, not dropped (#1278).
        let (locations, overflow_note) = cap_read_batch(locations, AGENT_READ_FILES_MAX_FILES);

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
                    // Names the route that actually works instead of the pin's
                    // "Try using a different tool." — see the longer rationale on the
                    // matching message in
                    // `request_file_edits/diff_application.rs`. The read side gets
                    // `cat`/`sed -n` rather than the write side's heredoc.
                    "File read/edit tools need the Phosphor remote-server extension, \
                     which is not installed on this host, so nothing was read — do NOT \
                     treat this as an empty or missing file. Until it is installed, read \
                     files here with `run_shell_command` (`cat path`, or `sed -n \
                     '1,200p' path` for a slice) — that is the supported route on this \
                     session and overrides the general advice to prefer the file tools."
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
                                // A remote host's paths use its own (unix) rules, not the
                                // local OS's (#1278).
                                let absolute_path = host_native_absolute_path_for(
                                    &loc.name,
                                    &shell,
                                    &current_working_directory,
                                    path_host,
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
                        max_batch_bytes: Some(AGENT_READ_FILES_BATCH_BYTES as u32),
                    };

                    let response = client
                        .read_file_context(request)
                        .await
                        .map_err(|e| anyhow::anyhow!("Remote read failed: {e}"))?;

                    let failed_files = response
                        .failed_files
                        .into_iter()
                        .map(|f| ReadFilesFailedFile {
                            path: f.path,
                            message: f.error.map(|e| e.message).unwrap_or_else(|| {
                                "File not found or could not be read".to_string()
                            }),
                        })
                        .collect::<Vec<_>>();

                    let (file_contexts, contentless) =
                        convert_remote_file_contexts(response.file_contexts);
                    let mut failed_files = failed_files;
                    failed_files.extend(contentless);
                    failed_files.extend(overflow_note);

                    if file_contexts.is_empty() && !failed_files.is_empty() {
                        let failed = describe_failed_files(&failed_files);
                        return Ok(ReadFilesResult::Error(format!(
                            "Failed to read files: {failed}"
                        )));
                    }

                    Ok(ReadFilesResult::Success {
                        files: file_contexts,
                        failed_files,
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
                let mut result = read_local_file_context(
                    &locations,
                    current_working_directory,
                    shell,
                    None,
                    Some(AGENT_READ_FILES_BATCH_BYTES),
                )
                .await?;
                result.failed_files.extend(overflow_note);
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

/// Turns a local [`ReadFileContextResult`] into the action result handed to the
/// model.
///
/// Mirrors Warp's three-way split: everything read is a plain success, nothing
/// read is an error, and a mixed batch is a success that *also* reports what
/// failed. Before this split a single bad path threw away every file that had
/// been read successfully (issue #136).
///
/// This previously routed the failure list through a synthetic `Files Failed`
/// file entry, on the stated grounds that the fork's pinned `warp-proto-apis`
/// had no `failed_reads` field to put it in. **That premise was wrong** — the
/// generated proto does carry `read_files_result::FailedRead { path, message }`,
/// and `convert.rs` was filling it with an empty vec. The workaround, and the
/// deliberately vague "missing, too large, or unprocessable" wording it forced,
/// are gone; this now matches the pin exactly (#369).
fn local_read_files_result(result: ReadFileContextResult) -> ReadFilesResult {
    if result.failed_files.is_empty() {
        ReadFilesResult::Success {
            files: result.file_contexts,
            failed_files: Vec::new(),
        }
    } else if result.file_contexts.is_empty() {
        let failed_files = describe_failed_files(&result.failed_files);
        ReadFilesResult::Error(format!("Failed to read files: {failed_files}"))
    } else {
        ReadFilesResult::Success {
            files: result.file_contexts,
            failed_files: result.failed_files,
        }
    }
}

#[cfg(test)]
#[path = "read_files_tests.rs"]
mod tests;
