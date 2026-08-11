//! Ported narrowly from the pin (`02b53fcd8:app/src/ai/get_relevant_files/remote_search/native.rs`).
//!
//! The pin's `remote_search/native.rs` module as a whole is CLOUD: its
//! `execute_remote_codebase_search` drives an embedding-based rerank through
//! `StoreClient`/`ServerApi`, and the containing `get_relevant_files/`
//! directory was retired in this fork (see `codebase_retrieval.rs`'s module
//! doc), replaced by a daemon-side `SearchRemoteCodebase` RPC that returns
//! already-reranked results rather than raw fragments needing this
//! whole-file/fragment split.
//!
//! [`file_contents_from_response`] itself has none of that coupling -- it is
//! a pure filter over `remote_server::proto` wire types the fork already has
//! (`crates/remote_server/proto/remote_server.proto`'s `ReadFileContextFile`,
//! `LineRange`, `ReadFileContextResponse`, `FailedFileRead`, `FileContextProto`
//! match the pin's messages field-for-field). It currently has no caller in
//! this fork -- the fork's remote codebase-search leg doesn't do
//! fragment-vs-whole-file reconciliation the way the pin's did -- so this is
//! test/logic restoration, not a wired feature. See
//! `docs/sweep/outcome-tail.md`.

use std::collections::HashMap;
use std::path::PathBuf;

use remote_server::proto::{ReadFileContextResponse, file_context_proto};

/// Keeps only the whole-file entries (no line range) from a `ReadFileContext` RPC
/// response, discarding any fragment (ranged) entries, and returns their text content
/// keyed by file path.
pub(crate) fn file_contents_from_response(
    response: ReadFileContextResponse,
) -> HashMap<PathBuf, String> {
    let mut file_contents = HashMap::new();
    for file_context in response.file_contexts {
        if file_context.line_range_start.is_some() || file_context.line_range_end.is_some() {
            continue;
        }
        if let Some(file_context_proto::Content::TextContent(content)) = file_context.content {
            file_contents.insert(PathBuf::from(file_context.file_name), content);
        }
    }
    file_contents
}

#[cfg(test)]
#[path = "get_relevant_files_file_contents_tests.rs"]
mod tests;
