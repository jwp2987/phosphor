//! Entity submodel that encapsulates all filesystem access for diff application.
//!
//! The executor holds a [`ModelHandle<ApplyDiffModel>`] and calls
//! [`ApplyDiffModel::apply_diffs`] without knowing whether the session is local
//! or remote. Internally the method resolves the session context and remote
//! client from the model context, then dispatches:
//!
//! - **Local**: calls [`apply_edits`] with a `std::fs`-backed closure.
//! - **Remote**: calls [`apply_edits`] with a router-backed closure (helper or shell mode).

use ai::diff_validation::AIRequestedCodeDiff;
use futures::FutureExt;
use remote_server::backend::RemoteFeatures as _;
use vec1::Vec1;
use warpui::r#async::BoxFuture;
use warpui::{Entity, ModelContext, ModelHandle, SingletonEntity as _};

use crate::ai::agent::{AIIdentifiers, FileEdit};
use crate::ai::blocklist::SessionContext;
use crate::auth::AuthStateProvider;
use crate::terminal::model::session::active_session::ActiveSession;

use super::diff_application::{DiffApplicationError, FileReadResult, apply_edits_strict};

/// Entity submodel that encapsulates filesystem access for diff application.
///
/// Held as a [`ModelHandle`] by the [`super::RequestFileEditsExecutor`].
pub(crate) struct ApplyDiffModel {
    active_session: ModelHandle<ActiveSession>,
}

impl Entity for ApplyDiffModel {
    type Event = ();
}

impl ApplyDiffModel {
    pub fn new(active_session: ModelHandle<ActiveSession>) -> Self {
        Self { active_session }
    }

    /// Resolves session context and remote client from the model context, then
    /// returns a future that applies the edits locally or remotely.
    pub fn apply_diffs(
        &self,
        edits: Vec<FileEdit>,
        ai_identifiers: &AIIdentifiers,
        passive_diff: bool,
        ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, Result<Vec<AIRequestedCodeDiff>, Vec1<DiffApplicationError>>> {
        let session_context = SessionContext::from_session(self.active_session.as_ref(ctx), ctx);
        let background_executor = ctx.background_executor();
        let auth_state = AuthStateProvider::as_ref(ctx).get().clone();
        let ai_identifiers = ai_identifiers.clone();

        // Reads go through the router: the helper, or shell mode over the session's
        // out-of-band channel. Neither available: the file tools stay unsupported here.
        let remote_backend = self
            .active_session
            .as_ref(ctx)
            .session_id(ctx)
            .filter(|_| session_context.is_remote())
            .map(|session_id| {
                remote_server::manager::RemoteServerManager::as_ref(ctx)
                    .backend_for_session_with_host(session_id, session_context.host_id())
            })
            .filter(|backend| backend.is_available(remote_server::backend::Family::FileContext));

        let is_remote = session_context.is_remote();
        let fut = async move {
            if is_remote {
                match remote_backend {
                    Some(backend) => {
                        apply_edits_strict(
                            edits,
                            &session_context,
                            &ai_identifiers,
                            background_executor,
                            auth_state,
                            passive_diff,
                            |path| {
                                let backend = backend.clone();
                                async move { read_remote_file(&backend, &path).await }
                            },
                        )
                        .await
                    }
                    None => Err(vec1::vec1![
                        DiffApplicationError::RemoteFileOperationsUnsupported
                    ]),
                }
            } else {
                apply_edits_strict(
                    edits,
                    &session_context,
                    &ai_identifiers,
                    background_executor,
                    auth_state,
                    passive_diff,
                    |path| async move { FileReadResult::from(std::fs::read_to_string(path)) },
                )
                .await
            }
        };
        cfg_if::cfg_if! {
            if #[cfg(target_family = "wasm")] {
                fut.boxed_local()
            } else {
                fut.boxed()
            }
        }
    }
}

// ── Remote file reading ──────────────────────────────────────────────────────────

/// Per-file byte limit for remote diff application (10 MB).
const MAX_DIFF_READ_BYTES: u32 = 10_000_000;

async fn read_remote_file(
    backend: &remote_server::backend::RoutedBackend,
    path: &str,
) -> FileReadResult {
    let request = remote_server::proto::ReadFileContextRequest {
        files: vec![remote_server::proto::ReadFileContextFile {
            path: path.to_string(),
            line_ranges: vec![],
        }],
        max_file_bytes: Some(MAX_DIFF_READ_BYTES),
        max_batch_bytes: None,
    };
    match backend.read_file_context(request).await {
        Ok(response) => {
            if let Some(fc) = response.file_contexts.into_iter().next() {
                // A whole-file read that was truncated by the byte limit will
                // have line_range_start/end set even though no ranges were
                // requested. Detect this and fail explicitly rather than
                // applying the diff to partial content.
                if fc.line_range_start.is_some() || fc.line_range_end.is_some() {
                    return FileReadResult::ReadError(format!(
                        "File exceeds the {MAX_DIFF_READ_BYTES}-byte limit for remote diff \
                         application and was truncated. The diff cannot be applied safely."
                    ));
                }
                match fc.content {
                    Some(remote_server::proto::file_context_proto::Content::TextContent(
                        content,
                    )) => {
                        // `ReadFileContext` hands the model LF-normalised text. The diff is
                        // applied to, shown from and written back as this text, so the file's
                        // own line endings have to come from its bytes (see
                        // `with_the_files_own_line_endings`). A failed raw read keeps the
                        // normalised text, as before.
                        let raw = if content.is_empty() {
                            None
                        } else {
                            backend.read_file_bytes(path.to_string()).await.ok()
                        };
                        FileReadResult::Found(with_the_files_own_line_endings(content, raw))
                    }
                    Some(remote_server::proto::file_context_proto::Content::BinaryContent(_)) => {
                        // apply-diff only works with text files
                        FileReadResult::ReadError("File is binary".to_string())
                    }
                    None => FileReadResult::Found(String::new()),
                }
            } else if let Some(failed) = response.failed_files.into_iter().next() {
                let message = failed
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string());
                if message.contains("not found") || message.contains("Not found") {
                    FileReadResult::NotFound
                } else {
                    FileReadResult::ReadError(message)
                }
            } else {
                FileReadResult::NotFound
            }
        }
        Err(err) => FileReadResult::ReadError(format!("{err}")),
    }
}

/// The text of a remote file as the diff application must see it: `raw` (the file's bytes)
/// when it is the same text as `normalised` apart from `\r\n` line endings, `normalised`
/// otherwise.
///
/// `normalised` is what `ReadFileContext` returned: both arms (the helper and the shell
/// backend, `remote_server::shellcmd::read`) split on `\n` and drop one `\r` in front of it,
/// which is right for the model and wrong for an edit. The local path reads the file with
/// `std::fs::read_to_string` and so keeps every ending; with the normalised text a remote CRLF
/// file got an LF diff base and an LF buffer, and the accept was then refused by the write
/// guard ("line endings were converted", `warp_files::compare_pre_image`) while a file mixing
/// CRLF and LF lines was written back with LF only (lab job 17885063245,
/// `lab_fam_agent_edit_endings`).
///
/// `raw` is only ever used when it is provably the same file: valid UTF-8 that normalises to
/// exactly `normalised`. Bytes that differ (the file changed between the two reads, a read
/// that was cut short) or that are missing leave today's behaviour.
fn with_the_files_own_line_endings(normalised: String, raw: Option<Vec<u8>>) -> String {
    let Some(raw) = raw.and_then(|bytes| String::from_utf8(bytes).ok()) else {
        return normalised;
    };
    if !raw.contains('\r') {
        return normalised;
    }
    if as_read_file_context_normalises(&raw) == normalised {
        raw
    } else {
        normalised
    }
}

/// `text` with one `\r` dropped at the end of every line, the way
/// `remote_server::shellcmd::read::read_text` builds the lines of a `ReadFileContext` answer
/// (a line is what `\n` ends, or the unterminated rest of the file).
fn as_read_file_context_normalises(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let (line, tail, has_newline) = match rest.find('\n') {
            Some(index) => (&rest[..index], &rest[index + 1..], true),
            None => (rest, "", false),
        };
        out.push_str(line.strip_suffix('\r').unwrap_or(line));
        if has_newline {
            out.push('\n');
        }
        rest = tail;
    }
    out
}

#[cfg(test)]
#[path = "apply_diff_model_tests.rs"]
mod tests;
