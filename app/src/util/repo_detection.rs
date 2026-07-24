//! Thin convenience layer for repo-detection calls, so surfaces (GUI + `warp_tui`
//! TUI) share one entry point.
//!
//! Ported/adapted from warp/master `util/repo_detection.rs`. The core detection
//! logic lives on [`DetectedRepositories::detect_possible_git_repo`]. Zap's
//! method has no remote-detection parameter (warp's newer `remote_detect`
//! future), and the TUI only ever uses [`RepoDetectionSessionType::Local`], so
//! the `Remote` arm resolves to `None` here rather than delegating to a remote
//! server.

use std::future::Future;

#[cfg(any(target_family = "wasm", not(feature = "local_fs")))]
use futures::future::ready;
use repo_metadata::repositories::RepoDetectionSource;
use warp_core::SessionId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::AppContext;
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
use repo_metadata::repositories::DetectedRepositories;
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
use warpui::SingletonEntity;

/// Describes whether the active session is local or remote.
pub enum RepoDetectionSessionType {
    /// A local terminal session — repo detection runs on the local filesystem.
    Local,
    /// A remote SSH session. Zap's `DetectedRepositories` has no remote
    /// detection path yet, so this resolves to `None`.
    Remote { session_id: SessionId },
}

/// Detects the git repository root for the given working directory, delegating
/// to [`DetectedRepositories::detect_possible_git_repo`] for local sessions.
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
pub fn detect_possible_git_repo(
    session_type: RepoDetectionSessionType,
    active_directory: &str,
    source: RepoDetectionSource,
    ctx: &mut AppContext,
) -> impl Future<Output = Option<LocalOrRemotePath>> + use<> {
    let local_fut = match session_type {
        RepoDetectionSessionType::Local => Some(DetectedRepositories::handle(ctx).update(
            ctx,
            |repos, ctx| repos.detect_possible_git_repo(active_directory, source, ctx),
        )),
        // Zap's DetectedRepositories has no remote-detection path; the TUI only
        // uses Local. Remote sessions resolve to None (no misclassification).
        RepoDetectionSessionType::Remote { .. } => None,
    };
    async move {
        match local_fut {
            Some(fut) => fut.await.map(LocalOrRemotePath::Local),
            None => None,
        }
    }
}

/// Repository detection is unavailable in WASM / non-`local_fs` builds (which is
/// how `warp` is built for the `warp_tui` gate) because `DetectedRepositories`
/// is not registered there.
#[cfg(any(target_family = "wasm", not(feature = "local_fs")))]
pub fn detect_possible_git_repo(
    _session_type: RepoDetectionSessionType,
    _active_directory: &str,
    _source: RepoDetectionSource,
    _ctx: &mut AppContext,
) -> impl Future<Output = Option<LocalOrRemotePath>> + use<> {
    ready(None)
}
