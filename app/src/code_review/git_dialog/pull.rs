//! Pull mode for [`GitDialog`] — Stage 1 (fast-forward only) of git-pull
//! parity.
//!
//! Renders a simple branch confirmation (no expandable list: unlike push,
//! which already knows the exact local commits about to be sent, a pull's
//! incoming commits aren't known until the fetch/merge actually runs). On
//! confirm, spawns `run_pull`. Mirrors `push.rs`'s local/remote split at
//! `start_confirm_remote` exactly; the one divergence from push is that a
//! pull changes the working tree, so both the local and remote arms need the
//! post-pull diff refresh explained below.
//!
//! # Working-tree invalidation after a successful pull
//!
//! Push never touches the working tree, so `push.rs` has nothing to
//! invalidate. Pull does, and both arms already get a full refresh for free:
//!
//! - **Local**: `GitDialogEvent::Completed` (emitted below on both success and
//!   failure, same as every other mode) is handled by
//!   `CodeReviewView::open_git_dialog`'s subscription, which calls
//!   `refresh_after_git_operation`. That calls `load_diffs_for_active_repo`,
//!   which recomputes the diff/content cache for every file
//!   (`FileInvalidationTask`'s job, superseded by a full reload) rather than
//!   waiting on the filesystem watcher's own independent pickup of the
//!   pulled files.
//! - **Remote**: the daemon's per-repo `DiffStateWatch`
//!   (`app/src/remote_server/diff_state_tracker.rs`) is a real filesystem
//!   watcher already relied on to catch working-tree changes from any
//!   source (discard, an out-of-band `git pull` run in a terminal, etc.), so
//!   it picks up the daemon-run pull's changed files the same way and pushes
//!   `DiffStateFileDelta`/`DiffStateSnapshot` to subscribers on its own.
//!   `refresh_after_git_operation` still runs afterward on the client
//!   regardless, so the UI does not depend on the watcher's timing to show
//!   fresh content.

use warp_core::ui::appearance::Appearance;
use warpui::{
    elements::{Container, Element, Flex, ParentElement},
    ViewContext,
};

use crate::{
    code_review::git_dialog::{
        interactive_path_future, render_branch_section, show_toast, user_facing_git_error,
        GitDialog, GitDialogEvent, GitDialogMode,
    },
    ui_components::icons::Icon,
};

#[cfg(not(target_family = "wasm"))]
use crate::code::buffer_location::RemotePath;

pub struct PullState;

pub(super) fn new_state() -> PullState {
    PullState
}

pub(super) fn confirm_label() -> String {
    crate::t!("common-pull")
}

pub(super) fn confirm_icon() -> Icon {
    Icon::ArrowDown
}

fn loading_label() -> &'static str {
    "Pulling…"
}

pub(super) fn start_confirm(me: &mut GitDialog, ctx: &mut ViewContext<GitDialog>) {
    let GitDialogMode::Pull(_) = me.mode() else {
        return;
    };
    let repo_path = me.repo_path().clone();
    let branch = me.branch_name().to_string();

    me.set_loading(loading_label(), ctx);

    // Remote repos pull on the daemon over RPC (#116), same split as push.
    #[cfg(not(target_family = "wasm"))]
    if let Some(remote) = me.remote().cloned() {
        start_confirm_remote(remote, branch, ctx);
        return;
    }

    let path_future = interactive_path_future(ctx);

    ctx.spawn(
        async move {
            let path_env = path_future.await;
            crate::util::git::run_pull(&repo_path, &branch, path_env.as_deref()).await
        },
        move |me, result, ctx| {
            match result {
                Ok(_) => {
                    show_toast("Successfully pulled the latest changes.", ctx);
                }
                Err(e) => {
                    log::error!("Pull failed: {e}");
                    show_toast(user_facing_git_error(&e.to_string()), ctx);
                }
            }
            let _ = me;
            // Local case: the filesystem watcher will independently notice
            // the pulled files, but `Completed` also drives an explicit full
            // diff/content reload via `refresh_after_git_operation` (see the
            // module doc), so the panel doesn't wait on watcher timing.
            ctx.emit(GitDialogEvent::Completed);
        },
    );
}

/// Sends the pull to the remote host's daemon and surfaces the same toasts as
/// the local path.
#[cfg(not(target_family = "wasm"))]
fn start_confirm_remote(remote: RemotePath, branch: String, ctx: &mut ViewContext<GitDialog>) {
    use crate::code_review::git_dialog::remote_client_for;
    use crate::remote_server::proto;

    let Some(client) = remote_client_for(&remote, ctx) else {
        show_toast(user_facing_git_error("could not resolve host"), ctx);
        ctx.emit(GitDialogEvent::Completed);
        return;
    };
    let request = proto::GitPullRequest {
        repo_path: remote.path.to_string(),
        branch,
    };
    ctx.spawn(
        async move { client.git_pull(request).await },
        move |me, result, ctx| {
            match result {
                Ok(response) => match response.result {
                    Some(proto::git_pull_response::Result::Success(delta)) => {
                        // Fold the daemon's post-pull delta in before completing,
                        // matching push's behavior for the unpushed-commit count
                        // and upstream ref. The daemon's own `DiffStateWatch`
                        // (see module doc) independently pushes per-file deltas
                        // for the working-tree content that changed, so this
                        // doesn't need to enumerate changed files itself.
                        me.apply_git_op_delta(Some(delta), ctx);
                        show_toast("Successfully pulled the latest changes.", ctx);
                    }
                    Some(proto::git_pull_response::Result::Error(e)) => {
                        log::error!("Remote pull failed: {}", e.message);
                        show_toast(user_facing_git_error(&e.message), ctx);
                    }
                    None => show_toast(user_facing_git_error(""), ctx),
                },
                Err(e) => {
                    log::error!("Remote pull RPC failed: {e}");
                    show_toast(user_facing_git_error(&format!("{e}")), ctx);
                }
            }
            ctx.emit(GitDialogEvent::Completed);
        },
    );
}

pub(super) fn render_body(branch_name: &str, appearance: &Appearance) -> Box<dyn Element> {
    Flex::column()
        .with_child(
            Container::new(render_branch_section(branch_name, appearance))
                .with_margin_bottom(16.)
                .finish(),
        )
        .finish()
}
