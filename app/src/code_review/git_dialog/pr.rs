//! Create-PR mode for [`GitDialog`].
//!
//! Renders the branch's PR diff (what would be included in the pull request)
//! with expandable per-file stats. On confirm, spawns `create_pr` and shows
//! a toast with a clickable "Open PR" link.

use std::path::Path;

use warp_core::ui::appearance::Appearance;
use warpui::{
    elements::{
        ClippedScrollStateHandle, Container, Element, Flex, MouseStateHandle, ParentElement, Text,
    },
    SingletonEntity, ViewContext,
};

use crate::{
    code_review::git_dialog::{
        interactive_path_future, render_branch_section, render_file_changes_box, show_toast,
        user_facing_git_error, GitDialog, GitDialogAction, GitDialogEvent, GitDialogMode,
    },
    ui_components::icons::Icon,
    util::git::{create_pr, get_branch_diff_entries, FileChangeEntry, PrInfo},
    view_components::{DismissibleToast, ToastLink},
    workspace::ToastStack,
};

use crate::code::buffer_location::RemotePath;

/// PR-mode sub-actions, dispatched wrapped in `GitDialogAction::Pr`.
#[derive(Clone, Debug, PartialEq)]
pub enum PrSubAction {
    ToggleChangesExpanded,
}

pub struct PrState {
    base_branch_name: Option<String>,
    file_changes: Vec<FileChangeEntry>,
    changes_expanded: bool,
    summary_mouse_state: MouseStateHandle,
    changes_scroll_state: ClippedScrollStateHandle,
}

pub(super) fn confirm_label_for() -> String {
    crate::t!("code-review-create-pr")
}

pub(super) fn confirm_icon_for() -> Icon {
    Icon::Github
}

fn loading_label_for() -> &'static str {
    "Creating\u{2026}"
}

/// PR mode has no prerequisites beyond a branch with commits; confirm is
/// always enabled when not loading.
pub(super) fn is_ready_to_confirm(_state: &PrState) -> bool {
    true
}

pub(super) fn new_state(
    repo_path: &Path,
    base_branch_name: Option<String>,
    remote: Option<&RemotePath>,
    ctx: &mut ViewContext<GitDialog>,
) -> PrState {
    spawn_load_file_changes(repo_path, remote, ctx);

    PrState {
        base_branch_name: base_branch_name.map(|name| {
            let name = name.trim();
            name.strip_prefix("origin/").unwrap_or(name).to_string()
        }),
        file_changes: Vec::new(),
        changes_expanded: false,
        summary_mouse_state: MouseStateHandle::default(),
        changes_scroll_state: ClippedScrollStateHandle::default(),
    }
}

pub(super) fn handle_sub_action(
    me: &mut GitDialog,
    action: &PrSubAction,
    ctx: &mut ViewContext<GitDialog>,
) {
    match action {
        PrSubAction::ToggleChangesExpanded => {
            if let GitDialogMode::CreatePr(state) = me.mode_mut() {
                state.changes_expanded = !state.changes_expanded;
            }
            ctx.notify();
        }
    }
}

pub(super) fn start_confirm(me: &mut GitDialog, ctx: &mut ViewContext<GitDialog>) {
    let GitDialogMode::CreatePr(_) = me.mode() else {
        return;
    };
    let repo_path = me.repo_path().clone();

    me.set_loading(loading_label_for(), ctx);

    // Remote repos create the PR on the daemon over RPC (#116).
    #[cfg(not(target_family = "wasm"))]
    if let Some(remote) = me.remote().cloned() {
        let branch = me.branch_name().to_string();
        start_confirm_remote(remote, branch, ctx);
        return;
    }

    let path_future = interactive_path_future(ctx);

    ctx.spawn(
        async move {
            let path_env = path_future.await;
            create_pr(&repo_path, None, None, path_env.as_deref()).await
        },
        move |_me, result, ctx| {
            match result {
                Ok(pr_info) => {
                    show_pr_created_toast(&pr_info, ctx);
                }
                Err(err) => {
                    log::error!("Failed to create PR: {err}");
                    show_toast(user_facing_git_error(&err.to_string()), ctx);
                }
            }
            ctx.emit(GitDialogEvent::Completed);
        },
    );
}

/// Sends the create-PR request to the remote host's daemon and surfaces the
/// same toast as the local path. The daemon runs `gh pr create --fill` (BYOP:
/// no daemon-side autogen, #116).
#[cfg(not(target_family = "wasm"))]
fn start_confirm_remote(remote: RemotePath, branch: String, ctx: &mut ViewContext<GitDialog>) {
    use crate::code_review::git_dialog::remote_client_for;
    use crate::remote_server::proto;

    let Some(client) = remote_client_for(&remote, ctx) else {
        show_toast(user_facing_git_error("could not resolve host"), ctx);
        ctx.emit(GitDialogEvent::Completed);
        return;
    };
    let request = proto::GitCreatePrRequest {
        repo_path: remote.path.to_string(),
        branch,
        autogenerate_content: false,
    };
    ctx.spawn(
        async move { client.git_create_pr(request).await },
        move |_me, result, ctx| {
            match result {
                Ok(response) => match response.result {
                    Some(proto::git_create_pr_response::Result::Success(pr)) => {
                        let pr = PrInfo {
                            number: pr.number,
                            url: pr.url,
                        };
                        show_pr_created_toast(&pr, ctx);
                    }
                    Some(proto::git_create_pr_response::Result::Error(e)) => {
                        log::error!("Remote create PR failed: {}", e.message);
                        show_toast(user_facing_git_error(&e.message), ctx);
                    }
                    None => show_toast(user_facing_git_error(""), ctx),
                },
                Err(e) => {
                    log::error!("Remote create PR RPC failed: {e}");
                    show_toast(user_facing_git_error(&format!("{e}")), ctx);
                }
            }
            ctx.emit(GitDialogEvent::Completed);
        },
    );
}

/// Loads the Changes box: the branch's committed-only file list
/// (`main...HEAD`). Local reads it from git directly; remote asks the daemon's
/// `GetCommittedBranchFiles` for the same set, so both surfaces show the same
/// files. (Deriving it from the loaded diff state instead would show whichever
/// file set the *current diff mode* holds, which is a different set whenever
/// the mode isn't branch-vs-main.)
#[cfg_attr(target_family = "wasm", allow(unused_variables))]
fn spawn_load_file_changes(
    repo_path: &Path,
    remote: Option<&RemotePath>,
    ctx: &mut ViewContext<GitDialog>,
) {
    #[cfg(not(target_family = "wasm"))]
    if let Some(remote) = remote {
        spawn_load_remote_file_changes(remote.clone(), ctx);
        return;
    }

    let diff_repo_path = repo_path.to_path_buf();
    ctx.spawn(
        async move { get_branch_diff_entries(&diff_repo_path).await },
        |me, result, ctx| {
            if let GitDialogMode::CreatePr(state) = &mut me.mode {
                match result {
                    Ok(entries) => {
                        state.file_changes = entries;
                        ctx.notify();
                    }
                    Err(err) => {
                        log::error!("Failed to load branch diff entries: {err}");
                    }
                }
            }
        },
    );
}

/// Remote arm of [`spawn_load_file_changes`]: the working tree lives on the
/// daemon, so the committed branch file list comes over RPC.
#[cfg(not(target_family = "wasm"))]
fn spawn_load_remote_file_changes(remote: RemotePath, ctx: &mut ViewContext<GitDialog>) {
    use crate::code_review::git_dialog::remote_client_for;
    use crate::remote_server::diff_state_proto::proto_to_file_change_entry;
    use crate::remote_server::proto;

    let Some(client) = remote_client_for(&remote, ctx) else {
        log::error!("Create PR dialog: no connected session for the code-review host");
        return;
    };
    let request = proto::GetCommittedBranchFilesRequest {
        repo_path: remote.path.to_string(),
    };
    ctx.spawn(
        async move { client.get_committed_branch_files(request).await },
        |me, result, ctx| {
            let GitDialogMode::CreatePr(state) = &mut me.mode else {
                return;
            };
            match result {
                Ok(response) => match response.result {
                    Some(proto::get_committed_branch_files_response::Result::Success(success)) => {
                        state.file_changes = success
                            .files
                            .iter()
                            .map(proto_to_file_change_entry)
                            .collect();
                        ctx.notify();
                    }
                    Some(proto::get_committed_branch_files_response::Result::Error(e)) => {
                        log::error!("Failed to load remote branch file changes: {}", e.message);
                    }
                    None => {
                        log::error!("Empty GetCommittedBranchFiles response");
                    }
                },
                Err(e) => {
                    log::error!("GetCommittedBranchFiles RPC failed: {e}");
                }
            }
        },
    );
}

/// Shows a toast announcing PR creation with a clickable "Open PR" link.
pub(super) fn show_pr_created_toast(pr_info: &PrInfo, ctx: &mut ViewContext<GitDialog>) {
    let window_id = ctx.window_id();
    let url = pr_info.url.clone();
    ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
        let link = ToastLink::new(crate::t!("code-review-open-pr")).with_href(url);
        let toast =
            DismissibleToast::default(crate::t!("code-review-pr-created-toast")).with_link(link);
        toast_stack.add_ephemeral_toast(toast, window_id, ctx);
    });
}

pub(super) fn render_body(
    state: &PrState,
    branch_name: &str,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let base_branch = state
        .base_branch_name
        .as_deref()
        .unwrap_or("default branch");
    let branch_name = format!("{branch_name} \u{2192} {base_branch}");
    Flex::column()
        .with_child(
            Container::new(render_branch_section(branch_name, appearance))
                .with_margin_bottom(16.)
                .finish(),
        )
        .with_child(render_changes_section(state, appearance))
        .finish()
}

fn render_changes_section(state: &PrState, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let main_color = theme.main_text_color(theme.surface_1()).into_solid();

    let label = Text::new(
        "Changes",
        appearance.ui_font_family(),
        appearance.ui_font_size(),
    )
    .with_color(main_color)
    .finish();

    let changes_box = render_file_changes_box(
        &state.file_changes,
        state.changes_expanded,
        &state.summary_mouse_state,
        &state.changes_scroll_state,
        GitDialogAction::Pr(PrSubAction::ToggleChangesExpanded),
        appearance,
    );

    Flex::column()
        .with_child(Container::new(label).with_margin_bottom(8.).finish())
        .with_child(changes_box)
        .finish()
}
