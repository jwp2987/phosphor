//! Batch-reads text file contents from one remote host over the daemon's
//! `ReadFileContext` RPC.
//!
//! Ported from the pin (`02b53fcd8:app/src/ai/remote_context_files.rs`) as
//! part of #381. **Adapted, not a byte-for-byte port**: the pin calls
//! `RemoteServerManager::as_ref(ctx).host_request_handle(&host_id)`, which
//! does not exist on this fork. This fork's `RemoteServerManager`
//! (`crates/remote_server/src/manager.rs`) instead exposes
//! `client_for_host(&HostId) -> Option<&Arc<RemoteServerClient>>` (see its
//! use in `app/src/ai/blocklist/action_model/execute/read_files.rs`), so a
//! host with no live connection surfaces as `None` here rather than through
//! whatever internal fallback `host_request_handle` used on the pin.
//!
//! Also hits the fork's dual-`HostId` trap: paths carry
//! `warp_util::host_id::HostId`, but `RemoteServerManager` is keyed by
//! `warp_core::HostId` (aliased as `remote_server::HostId`). Bridged with
//! `crate::code::buffer_location::util_host_id_to_core`, the existing helper
//! for exactly this conversion direction.
//!
//! **Now wired to one of the pin's two call sites.**
//! `app/src/ai/skills/file_watchers/skill_watcher.rs`'s
//! `read_project_skill_contents` calls [`read_remote_text_file_contents`] for
//! remote project skills discovered via `RepoMetadataModel`'s standing-query
//! results (`SkillWatcher::refresh_project_skills_for_repo`). The pin's other
//! call site, `app/src/ai/metadata_project_rules.rs` (remote project-rule
//! content reading), still does not exist on this fork -- remote project-rule
//! *discovery* is a separate, comparably-sized feature of its own, not part
//! of #381's scope. This module is also **not** what populates
//! `RemoteAgentContextSnapshot`'s `global_rules` field -- that value arrives
//! pre-serialized in the snapshot proto (daemon-side, from
//! `ProjectContextModel::global_rules()`, which this fork's
//! `ProjectContextModel` does not have) and is consumed directly in
//! `app/src/ai/remote_agent_context.rs` without any additional RPC. See that
//! file's module doc comment for the real state of that gap.

use std::collections::HashMap;

use futures::future::{BoxFuture, FutureExt as _};
use remote_server::proto::{
    file_context_proto, FileContextProto, ReadFileContextFile, ReadFileContextRequest,
};
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::{AppContext, SingletonEntity};

use crate::remote_server::manager::RemoteServerManager;

pub(crate) const REMOTE_CONTEXT_MAX_FILE_BYTES: u32 = 1024 * 1024;
pub(crate) const REMOTE_CONTEXT_MAX_BATCH_BYTES: u32 = 5 * 1024 * 1024;

/// Reads text contents for exact paths on one remote host.
///
/// Responses may be reordered or omit unreadable files, and their file names do not include the
/// host ID. Pairing by path preserves the original host-qualified identities and request order.
pub(crate) fn read_remote_text_file_contents(
    paths: Vec<LocalOrRemotePath>,
    max_file_bytes: Option<u32>,
    max_batch_bytes: Option<u32>,
    ctx: &AppContext,
) -> BoxFuture<'static, anyhow::Result<Vec<(LocalOrRemotePath, String)>>> {
    let host_id = match remote_context_host_id(&paths) {
        Ok(Some(host_id)) => host_id,
        Ok(None) => return futures::future::ready(Ok(Vec::new())).boxed(),
        Err(error) => return futures::future::ready(Err(error)).boxed(),
    };

    let request = remote_text_file_read_request(&paths, max_file_bytes, max_batch_bytes);
    // Dual-HostId bridge (see module doc comment): RemoteServerManager is
    // keyed by warp_core::HostId, paths carry warp_util::host_id::HostId.
    let core_host_id = crate::code::buffer_location::util_host_id_to_core(&host_id);
    let client = RemoteServerManager::as_ref(ctx)
        .client_for_host(&core_host_id)
        .cloned();
    async move {
        let Some(client) = client else {
            anyhow::bail!("No connected remote server for host {host_id}");
        };
        let response = client.read_file_context(request).await?;
        Ok(pair_remote_text_file_contents(
            paths,
            response.file_contexts,
        ))
    }
    .boxed()
}

fn remote_context_host_id(paths: &[LocalOrRemotePath]) -> anyhow::Result<Option<HostId>> {
    let Some(first_path) = paths.first() else {
        return Ok(None);
    };
    let Some(first_remote) = first_path.as_remote() else {
        anyhow::bail!("Expected remote context paths");
    };
    if paths.iter().any(|path| {
        path.as_remote()
            .is_none_or(|remote| remote.host_id != first_remote.host_id)
    }) {
        anyhow::bail!("Remote context paths span multiple locations");
    }
    Ok(Some(first_remote.host_id.clone()))
}
fn remote_text_file_read_request(
    paths: &[LocalOrRemotePath],
    max_file_bytes: Option<u32>,
    max_batch_bytes: Option<u32>,
) -> ReadFileContextRequest {
    ReadFileContextRequest {
        files: paths
            .iter()
            .filter_map(|path| {
                let remote = path.as_remote()?;
                Some(ReadFileContextFile {
                    path: remote.path.as_str().to_string(),
                    line_ranges: Vec::new(),
                })
            })
            .collect(),
        max_file_bytes,
        max_batch_bytes,
    }
}

fn pair_remote_text_file_contents(
    paths: Vec<LocalOrRemotePath>,
    file_contexts: Vec<FileContextProto>,
) -> Vec<(LocalOrRemotePath, String)> {
    let content_by_path = file_contexts
        .into_iter()
        .filter_map(|file_context| {
            let file_context_proto::Content::TextContent(content) = file_context.content? else {
                return None;
            };
            Some((file_context.file_name, content))
        })
        .collect::<HashMap<_, _>>();
    paths
        .into_iter()
        .filter_map(|path| {
            let content = content_by_path
                .get(path.as_remote()?.path.as_str())?
                .clone();
            Some((path, content))
        })
        .collect()
}

#[cfg(test)]
#[path = "remote_context_files_tests.rs"]
mod tests;
