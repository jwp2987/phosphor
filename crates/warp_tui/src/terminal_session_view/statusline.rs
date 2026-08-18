//! Configurable terminal-session statusline formatting, rendering, and
//! metadata subscriptions.
//!
//! Extracted out of `terminal_session_view.rs` following the pin's
//! `62a6b083b` ("TUI statusline polish"), which split the same code into
//! `terminal_session_view/statusline.rs` upstream. The extraction is a move:
//! the resolution order, separator rules and hint priority are this fork's,
//! not upstream's -- see `FooterSegment::separator_to` and `render_footer`.

use std::time::Duration;

use chrono::{Local, NaiveDateTime};
use warp::settings::{AISettings, TuiStatuslineConfig, TuiStatuslineItem};
use warp::tui_export::{GitRepoModels, GitStatusMetadata, LLMPreferences};
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::SingletonEntity;
use warpui_core::elements::tui::{
    Modifier, TuiAnimated, TuiElement, TuiFlex, TuiHoverable, TuiStyle, TuiText,
};
use warpui_core::{AppContext, ViewContext};

use super::{
    CTRL_C_EXIT_HINT, CTRL_C_KILL_CHILD_HINT, ConversationRestoreState,
    LOADING_CONVERSATION_HINT, RUNNING_COMMAND_DETACH_HINT, SHELL_MODE_HINT,
    TuiConversationRestoreOrigin, TuiTerminalSessionAction, TuiTerminalSessionView,
    render_mcp_install_footer, render_mcp_menu_footer,
};
use crate::transient_hint::TransientHintTone;
use crate::tui_builder::TuiUiBuilder;
use crate::ui::compact_footer_path;
use crate::usage::render_context_usage_entry;

/// How often a statusline date/time segment repaints itself.
const STATUSLINE_DATETIME_REPAINT_INTERVAL: Duration = Duration::from_secs(60);

pub(super) fn format_statusline_date(now: NaiveDateTime) -> String {
    now.format("%B %-d, %Y").to_string()
}
pub(super) fn format_statusline_time_12_hour(now: NaiveDateTime) -> String {
    now.format("%-I:%M%P").to_string()
}
pub(super) fn format_statusline_time_24_hour(now: NaiveDateTime) -> String {
    now.format("%H:%M").to_string()
}
/// Formats the active AI conversation's to-do progress for the statusline:
/// `❒` while items remain pending, `✓` once the list is finished.
pub(super) fn format_todo_progress(completed: usize, total: usize, finished: bool) -> String {
    let marker = if finished { "✓" } else { "❒" };
    format!("{marker} {completed}/{total}")
}
/// Renders a self-repainting statusline datetime segment: `formatter` maps
/// the current local time to display text, and the element schedules its own
/// repaint every [`STATUSLINE_DATETIME_REPAINT_INTERVAL`] so the footer stays
/// current without the whole session view re-rendering on a timer.
pub(super) fn render_statusline_datetime(
    formatter: fn(NaiveDateTime) -> String,
    style: TuiStyle,
) -> Box<dyn TuiElement> {
    TuiAnimated::new(STATUSLINE_DATETIME_REPAINT_INTERVAL, move || {
        TuiText::new(formatter(Local::now().naive_local()))
            .with_style(style)
            .truncate()
            .finish()
    })
    .finish()
}

/// One resolved item in the footer's configured presentation order.
pub(super) enum FooterSegment {
    /// Vim mode label (NOR/INS/VIS/V-L/REP), shown when vim mode is enabled.
    /// Always the leading segment when present, ahead of shell-mode/model.
    Vim(&'static str),
    ShellMode,
    ActiveIndicator(&'static str),
    /// The footer's auto-approve control. This replaces the plain
    /// `ActiveIndicator("Auto-approve")` label with the pin's clickable `▶▶`
    /// toggle (`render_auto_approve_statusline`): it is present whenever the
    /// item is enabled and the session is not in shell mode, and carries its
    /// *state* in colour (muted = off, success = on) rather than by
    /// appearing/disappearing -- otherwise there would be nothing to click
    /// while auto-approve is off. Carries a rendered element because it owns
    /// hover state on a retained handle.
    AutoApproveIndicator(Box<dyn TuiElement>),
    Model(Box<dyn TuiElement>),
    WorkingDirectory(String),
    GitBranch(String),
    /// Composite branch item (`⊢ main • ↑1 ↓2`): the branch name plus its
    /// upstream tracking state. Carries a rendered element rather than a
    /// string because the counts are accent-styled against a muted branch
    /// name -- see `render_git_branch_status`.
    GitBranchStatus(Box<dyn TuiElement>),
    /// The selected conversation's context-window usage. BYOP has no cloud
    /// credits/cost, so unlike upstream's clickable credits⇄cost toggle this
    /// wraps Phosphor's informational context-% entry (`crate::usage`).
    ContextWindowUsage(Box<dyn TuiElement>),
    GitDiff {
        files_changed: usize,
        additions: usize,
        deletions: usize,
    },
    /// A configured date/time item (`Date`, `Time12Hour`, `Time24Hour`).
    DateTime(Box<dyn TuiElement>),
    /// The selected AI conversation's active to-do list progress
    /// (`format_todo_progress`), shown while that list is non-empty. Carries a
    /// rendered element rather than a string because the segment is clickable
    /// (it toggles the to-do menu) and tracks its own hover state.
    AgentTodoList(Box<dyn TuiElement>),
    /// The current branch's GitHub pull request, rendered as a clickable
    /// link. Backed by the local `gh` CLI (`GitHubRepoModel`), so no Warp
    /// backend is involved.
    GitHubPullRequest(Box<dyn TuiElement>),
}

impl FooterSegment {
    /// Two-tier separator, matching the pin's Figma-group structure: " • "
    /// joins segments within the same group (working-directory/git-branch;
    /// date/time; consecutive active indicators; anything touching
    /// shell-mode, which always stays plain), " | " joins segments across
    /// different groups. Every other pairing among the "big set" of
    /// unrelated single-segment groups (active indicator, model, working
    /// directory, git branch, context-window usage, git diff, date/time,
    /// agent to-do list -- each its own group when not paired with its
    /// git-branch/date-time sibling) therefore falls through to " | ".
    fn separator_to(&self, next: &Self) -> &'static str {
        match (self, next) {
            // A leading vim indicator is joined to the shell-mode/model
            // segment right after it with a plain space, matching the
            // shell-mode-to-cwd relationship below.
            (Self::Vim(_), Self::ShellMode | Self::Model(_)) => " ",
            // Only a shell-mode label directly preceding the working directory
            // gets a plain space; a leading Model label falls through to the
            // default " • " below so model and cwd don't visually run
            // together (fixes a missing divider present in an earlier
            // revision of this port — see warp upstream 311deab98).
            (Self::ShellMode, Self::WorkingDirectory(_)) => " ",
            (Self::WorkingDirectory(_), Self::GitBranch(_)) => " ↬ ",
            // The composite branch status owns its own `⊢` relationship glyph
            // (see `render_git_branch_status`), so it follows the working
            // directory with a plain space rather than this fork's `↬` marker.
            // Matches the pin.
            (Self::WorkingDirectory(_), Self::GitBranchStatus(_)) => " ",
            // The auto-approve control stays in the same Figma group as the
            // remaining plain active indicator ("Queued"), so the pairing
            // keeps this fork's " • " rather than dropping to " | ".
            (
                Self::ActiveIndicator(_) | Self::AutoApproveIndicator(_),
                Self::ActiveIndicator(_) | Self::AutoApproveIndicator(_),
            ) => " • ",
            (
                Self::WorkingDirectory(_) | Self::GitBranch(_) | Self::GitBranchStatus(_),
                Self::WorkingDirectory(_) | Self::GitBranch(_) | Self::GitBranchStatus(_),
            )
            | (Self::DateTime(_), Self::DateTime(_))
            | (Self::ShellMode, _)
            | (_, Self::ShellMode) => " • ",
            (
                Self::Vim(_)
                | Self::ActiveIndicator(_)
                | Self::AutoApproveIndicator(_)
                | Self::Model(_)
                | Self::WorkingDirectory(_)
                | Self::GitBranch(_)
                | Self::GitBranchStatus(_)
                | Self::ContextWindowUsage(_)
                | Self::GitDiff { .. }
                | Self::DateTime(_)
                | Self::AgentTodoList(_)
                | Self::GitHubPullRequest(_),
                Self::Vim(_)
                | Self::ActiveIndicator(_)
                | Self::AutoApproveIndicator(_)
                | Self::Model(_)
                | Self::WorkingDirectory(_)
                | Self::GitBranch(_)
                | Self::GitBranchStatus(_)
                | Self::ContextWindowUsage(_)
                | Self::GitDiff { .. }
                | Self::DateTime(_)
                | Self::AgentTodoList(_)
                | Self::GitHubPullRequest(_),
            ) => " | ",
        }
    }
}

/// Resolved segments for the footer's left-aligned status row.
pub(super) struct FooterSegments {
    pub(super) ordered: Vec<FooterSegment>,
}

/// Builds the status row from resolved segments. Working directory follows a
/// leading shell-mode or model label with a plain space; an immediately
/// following branch uses ` ↬ ` as the relationship marker. Items in
/// different Figma groups use ` | `; other adjacent pairs use ` • `. The
/// first item never receives a separator. Every child truncates to a single
/// row, so the row lays out one row tall.
pub(super) fn render_status_footer_row(
    segments: FooterSegments,
    builder: &TuiUiBuilder,
) -> TuiFlex {
    let muted = builder.muted_text_style();
    let mut row = TuiFlex::row();
    let mut segments = segments.ordered.into_iter().peekable();
    while let Some(segment) = segments.next() {
        let separator = segments.peek().map(|next| segment.separator_to(next));
        match segment {
            FooterSegment::Vim(label) => {
                row = row.child(
                    TuiText::new(label)
                        .with_style(builder.accent_border_style())
                        .truncate()
                        .finish(),
                );
            }
            FooterSegment::ShellMode => {
                row = row.child(
                    TuiText::new(SHELL_MODE_HINT)
                        .with_style(builder.shell_command_accent_style())
                        .truncate()
                        .finish(),
                );
            }
            FooterSegment::ActiveIndicator(label) => {
                row = row.child(
                    TuiText::new(label)
                        .with_style(builder.success_glyph_style())
                        .truncate()
                        .finish(),
                );
            }
            FooterSegment::AutoApproveIndicator(model)
            | FooterSegment::Model(model)
            | FooterSegment::ContextWindowUsage(model)
            | FooterSegment::DateTime(model)
            | FooterSegment::AgentTodoList(model)
            | FooterSegment::GitBranchStatus(model)
            | FooterSegment::GitHubPullRequest(model) => {
                row = row.child(model);
            }
            FooterSegment::WorkingDirectory(cwd) | FooterSegment::GitBranch(cwd) => {
                row = row.child(TuiText::new(cwd).with_style(muted).truncate().finish());
            }
            FooterSegment::GitDiff {
                files_changed,
                additions,
                deletions,
            } => {
                // The file count leads and is always present, so a binary or
                // whitespace-only change (no counted lines) still shows up.
                let mut spans = vec![(format!("☰ {files_changed}"), muted)];
                if additions > 0 || deletions > 0 {
                    spans.push((" •".to_owned(), muted));
                }
                if additions > 0 {
                    spans.push((format!(" +{additions}"), builder.diff_added_style()));
                }
                if deletions > 0 {
                    spans.push((" ".to_owned(), muted));
                    spans.push((format!("-{deletions}"), builder.diff_removed_style()));
                }
                row = row.child(TuiText::from_spans(spans).truncate().finish());
            }
        }
        if let Some(separator) = separator {
            row = row.child(
                TuiText::new(separator)
                    .with_style(muted)
                    .truncate()
                    .finish(),
            );
        }
    }

    row
}

/// Renders the composite branch item: `⊢ main • ↑1 ↓2`.
///
/// The branch name and the counts are muted; only the direction glyphs are
/// accented, so the row reads as one unit and the arrows stay findable. The
/// trailing group is omitted entirely when the branch has no upstream (or the
/// counts are unavailable), leaving a bare `⊢ main`. A rebase in progress
/// replaces both counts with `⇅` — `ahead`/`behind` are already `None` in that
/// case (see `GitBranchTrackingStatus::ahead_display_count`), so `rebased`
/// is checked first rather than combined with them.
pub(super) fn render_git_branch_status(
    branch: &str,
    rebased: bool,
    ahead: Option<String>,
    behind: Option<String>,
    builder: &TuiUiBuilder,
) -> Box<dyn TuiElement> {
    let muted = builder.muted_text_style();
    let accent = builder.accent_text_style();
    let has_ahead = ahead.is_some();
    let has_behind = behind.is_some();
    let mut spans = vec![(format!("⊢ {branch}"), muted)];
    if rebased || has_ahead || has_behind {
        spans.push((" • ".to_owned(), muted));
    }
    if rebased {
        spans.push(("⇅".to_owned(), accent));
    } else {
        if let Some(ahead) = ahead {
            spans.push(("↑".to_owned(), accent));
            spans.push((
                format!("{ahead}{}", if has_behind { " " } else { "" }),
                muted,
            ));
        }
        if let Some(behind) = behind {
            spans.push(("↓".to_owned(), accent));
            spans.push((behind, muted));
        }
    }
    TuiText::from_spans(spans).truncate().finish()
}

/// Whether the plain `GitBranch` item should render. `GitBranchStatus` is a
/// superset of it — same branch name, plus tracking counts — so enabling both
/// would print the branch twice; the composite item wins.
pub(super) fn should_render_plain_git_branch(config: &TuiStatuslineConfig) -> bool {
    config.is_enabled(TuiStatuslineItem::GitBranch)
        && !config.is_enabled(TuiStatuslineItem::GitBranchStatus)
}

impl TuiTerminalSessionView {
    /// Builds the configured statusline under the input box. Normal mode uses
    /// the persisted item order and visibility (`/statusline`); shell mode
    /// always leads with its mode label and only resolves configured
    /// working-directory and git items. A replacing hint — the ctrl-c exit
    /// confirmation while armed, the conversation-list loading hint, or an
    /// active transient notice — occupies the whole row instead. An empty
    /// resolved configuration consumes no row.
    pub(super) fn render_footer(&self, ctx: &AppContext) -> TuiFlex {
        let builder = TuiUiBuilder::from_app(ctx);
        let muted = builder.muted_text_style();

        // Replacing hints occupy the entire status row, in the existing
        // priority order: ctrl-c → loading → transient.
        if self.exit_confirmation.is_armed() {
            // When the kill-child window is armed, show the child-specific hint
            // so the user knows the next ctrl-c will kill the child agent rather
            // than exiting the whole TUI.
            let hint = if self.child_kill_armed_conversation.is_some() {
                CTRL_C_KILL_CHILD_HINT
            } else {
                CTRL_C_EXIT_HINT
            };
            return TuiFlex::row().child(TuiText::new(hint).with_style(muted).truncate().finish());
        }
        if matches!(
            &self.conversation_restore_state,
            ConversationRestoreState::Loading {
                origin: TuiConversationRestoreOrigin::ConversationList,
                ..
            }
        ) {
            return TuiFlex::row().child(
                TuiText::new(LOADING_CONVERSATION_HINT)
                    .with_style(muted)
                    .truncate()
                    .finish(),
            );
        }
        if let Some((transient, tone)) = self.transient_hint.current() {
            let style = match tone {
                TransientHintTone::Muted => muted,
                TransientHintTone::Success => builder.success_glyph_style(),
                TransientHintTone::Error => builder.error_text_style(),
            };
            return TuiFlex::row().child(
                TuiText::new(transient)
                    .with_style(style)
                    .truncate()
                    .finish(),
            );
        }
        // While the agent is manually tagged into a running command, the
        // detach hint replaces the normal statusline the same way the hints
        // above do. Ported from the pin's `footer_hint` priority order
        // (`02b53fcd8`, `RUNNING_COMMAND_DETACH_HINT`) for #390.
        if self
            .terminal_model
            .lock()
            .block_list()
            .active_block()
            .is_agent_tagged_in()
        {
            return TuiFlex::row().child(
                TuiText::new(RUNNING_COMMAND_DETACH_HINT)
                    .with_style(muted)
                    .truncate()
                    .finish(),
            );
        }
        // The open `/mcp` install flow or menu replaces the statusline with its
        // own controls, the same way the replacing hints above do.
        if self.mcp_install_flow.as_ref(ctx).is_open(ctx) {
            return render_mcp_install_footer(
                &builder,
                self.mcp_install_flow.as_ref(ctx).primary_action_hint(),
            );
        }
        if self.mcp_menu.as_ref(ctx).is_open(ctx) {
            let menu = self.mcp_menu.as_ref(ctx);
            return render_mcp_menu_footer(
                &builder,
                menu.selected_primary_action(ctx),
                menu.can_log_out_selected(ctx),
            );
        }
        let shell_mode = self.is_shell_mode(ctx);
        let config = AISettings::as_ref(ctx).tui_statusline.normalized();
        let git_metadata = self.git_status_metadata(ctx);
        let mut ordered = Vec::new();
        if let Some(vim_label) = self.vim_mode_indicator(ctx) {
            ordered.push(FooterSegment::Vim(vim_label));
        }
        if shell_mode {
            ordered.push(FooterSegment::ShellMode);
        }
        for item in config.order.iter().copied() {
            if !config.is_enabled(item) {
                continue;
            }
            let segment = match item {
                // The footer auto-approve entry is a clickable toggle, not a
                // presence-only label: it renders in every non-shell-mode
                // session so it can be clicked to turn auto-approve *on*, and
                // reports the current state through its colour instead.
                TuiStatuslineItem::AutoApprove => (!shell_mode).then(|| {
                    FooterSegment::AutoApproveIndicator(
                        self.render_auto_approve_statusline(&builder, ctx),
                    )
                }),
                // Zap has no persistent "auto-queue" mode (`/queue` holds a
                // single specific prompt instead — see `TuiStatuslineItem`'s
                // doc comment), so this indicates a queued follow-up prompt.
                TuiStatuslineItem::AutoQueue => (!shell_mode && self.queued_follow_up.is_some())
                    .then_some(FooterSegment::ActiveIndicator("Queued")),
                TuiStatuslineItem::Model => (!shell_mode).then(|| {
                    let model_name = LLMPreferences::as_ref(ctx)
                        .get_active_base_model(ctx, Some(self.terminal_surface_id))
                        .display_name
                        .clone();
                    // The active-model label is clickable: a left click
                    // toggles the inline model picker (the same menu
                    // `/model` surfaces). The hover state lives on a
                    // retained [`MouseStateHandle`] so it survives
                    // element-tree rebuilds, and the click dispatches a
                    // typed action since the element pass only has an
                    // immutable [`AppContext`] — mirroring the usage entry.
                    let model_label_hovered = self
                        .model_label_hover
                        .lock()
                        .is_ok_and(|state| state.is_hovered());
                    let model_label_style = if model_label_hovered {
                        builder.primary_text_style()
                    } else {
                        builder.muted_text_style()
                    };
                    FooterSegment::Model(
                        TuiHoverable::new(
                            self.model_label_hover.clone(),
                            TuiText::new(model_name)
                                .with_style(model_label_style)
                                .truncate()
                                .finish(),
                        )
                        .on_click(|event_ctx, _| {
                            event_ctx
                                .dispatch_typed_action(TuiTerminalSessionAction::ToggleModelMenu);
                        })
                        .finish(),
                    )
                }),
                TuiStatuslineItem::WorkingDirectory => self
                    .current_working_directory(ctx)
                    .map(|cwd| FooterSegment::WorkingDirectory(compact_footer_path(&cwd))),
                TuiStatuslineItem::GitBranch => should_render_plain_git_branch(&config)
                    .then(|| {
                        git_metadata.map(|metadata| {
                            FooterSegment::GitBranch(metadata.current_branch_name.clone())
                        })
                    })
                    .flatten(),
                TuiStatuslineItem::GitBranchStatus => git_metadata.map(|metadata| {
                    let tracking = &metadata.branch_tracking_status;
                    FooterSegment::GitBranchStatus(render_git_branch_status(
                        &metadata.current_branch_name,
                        tracking.is_rebased(),
                        tracking.ahead_display_count(),
                        tracking.behind_display_count(),
                        &builder,
                    ))
                }),
                TuiStatuslineItem::GitDiffStatus => git_metadata.and_then(|metadata| {
                    let stats = metadata.stats_against_head;
                    // Gated on the file count, not the line counts: a binary or
                    // whitespace-only change has files but no counted lines and
                    // must still be visible.
                    (stats.files_changed > 0).then_some(FooterSegment::GitDiff {
                        files_changed: stats.files_changed,
                        additions: stats.total_additions,
                        deletions: stats.total_deletions,
                    })
                }),
                // Current-branch PR, resolved through the local `gh` CLI. The
                // link is clickable: `TuiLink` keeps hover state on a retained
                // handle and the click dispatches a typed action, since the
                // element pass only holds an immutable `AppContext`.
                TuiStatuslineItem::GitHubPullRequest => (!shell_mode)
                    .then_some(self.github_repo.as_ref())
                    .flatten()
                    .and_then(|repo| repo.as_ref(ctx).pr_info(ctx))
                    .map(|pr| {
                        let url = pr.url.clone();
                        FooterSegment::GitHubPullRequest(self.github_pr_link.render(
                            format!("PR #{}", pr.number),
                            ctx,
                            move |event_ctx, _| {
                                event_ctx.dispatch_typed_action(TuiTerminalSessionAction::OpenUrl(
                                    url.clone(),
                                ));
                            },
                        ))
                    }),
                // Selected conversation's context-window occupancy, hidden
                // until any usage has been reported (and hidden in shell
                // mode, where it is stale AI-conversation metadata). BYOP
                // has no cloud credits/cost, so this reuses Zap's existing
                // informational context-% entry (`crate::usage`) rather than
                // upstream's clickable credits⇄cost toggle.
                TuiStatuslineItem::ContextWindowUsage => (!shell_mode)
                    .then(|| self.selected_conversation_context_usage(ctx))
                    .flatten()
                    .map(|fraction| {
                        FooterSegment::ContextWindowUsage(render_context_usage_entry(fraction, ctx))
                    }),
                TuiStatuslineItem::Date => Some(FooterSegment::DateTime(
                    render_statusline_datetime(format_statusline_date, builder.muted_text_style()),
                )),
                TuiStatuslineItem::Time12Hour => {
                    Some(FooterSegment::DateTime(render_statusline_datetime(
                        format_statusline_time_12_hour,
                        builder.muted_text_style(),
                    )))
                }
                TuiStatuslineItem::Time24Hour => {
                    Some(FooterSegment::DateTime(render_statusline_datetime(
                        format_statusline_time_24_hour,
                        builder.muted_text_style(),
                    )))
                }
                TuiStatuslineItem::AgentTodoList => (!shell_mode)
                    .then(|| {
                        self.conversation_selection
                            .as_ref(ctx)
                            .selected_conversation(ctx)
                    })
                    .flatten()
                    .and_then(|conversation| conversation.active_todo_list())
                    .filter(|todo_list| !todo_list.is_empty())
                    .map(|todo_list| {
                        let hovered = self
                            .todo_list_mouse
                            .lock()
                            .is_ok_and(|state| state.is_hovered());
                        let style = if hovered {
                            builder.primary_text_style()
                        } else {
                            builder.muted_text_style()
                        };
                        let progress = format_todo_progress(
                            todo_list.completed_items().len(),
                            todo_list.len(),
                            todo_list.is_finished(),
                        );
                        FooterSegment::AgentTodoList(
                            TuiHoverable::new(
                                self.todo_list_mouse.clone(),
                                TuiText::new(progress).with_style(style).truncate().finish(),
                            )
                            .on_click(|event_ctx, _| {
                                event_ctx.dispatch_typed_action(
                                    TuiTerminalSessionAction::ToggleTodoMenu,
                                );
                            })
                            .finish(),
                        )
                    }),
            };
            if let Some(segment) = segment {
                ordered.push(segment);
            }
        }
        render_status_footer_row(FooterSegments { ordered }, &builder)
    }

    /// Renders the footer's clickable auto-approve control: a `▶▶` toggle
    /// styled success when the selected conversation's pending-query
    /// autoexecute override approves any action and muted when it does not,
    /// bolded while hovered. Clicking dispatches
    /// [`TuiTerminalSessionAction::ToggleAutoApprove`] with `show_feedback:
    /// false` -- the control itself already shows the new state, so the
    /// transient footer confirmation the keybinding/slash-command path uses
    /// would be redundant.
    ///
    /// Hover state lives on the retained `footer_auto_approve_mouse` handle so
    /// it survives element-tree rebuilds, and stays distinct from the warping
    /// indicator's own `warping_auto_approve_mouse`. Purely local state: the
    /// override lives on the selected conversation, so no account or
    /// shared-session lookup is involved.
    pub(super) fn render_auto_approve_statusline(
        &self,
        builder: &TuiUiBuilder,
        ctx: &AppContext,
    ) -> Box<dyn TuiElement> {
        let enabled = self
            .conversation_selection
            .as_ref(ctx)
            .pending_query_autoexecute_override(ctx)
            .is_autoexecute_any_action();
        let hovered = self
            .footer_auto_approve_mouse
            .lock()
            .is_ok_and(|state| state.is_hovered());
        let mut style = if enabled {
            builder.success_glyph_style()
        } else {
            builder.muted_text_style()
        };
        if hovered {
            style = style.add_modifier(Modifier::BOLD);
        }
        TuiHoverable::new(
            self.footer_auto_approve_mouse.clone(),
            TuiText::new("▶▶").with_style(style).truncate().finish(),
        )
        .on_click(|event_ctx, _| {
            event_ctx.dispatch_typed_action(TuiTerminalSessionAction::ToggleAutoApprove {
                show_feedback: false,
            });
        })
        .finish()
    }

    /// Returns a brief vim mode label for the footer (NOR/INS/VIS/V-L/REP)
    /// when vim mode is enabled, or `None` when vim mode is disabled.
    pub(super) fn vim_mode_indicator(&self, ctx: &AppContext) -> Option<&'static str> {
        use vim::vim::{MotionType, VimMode};
        let mode = self.input_view.as_ref(ctx).vim_mode(ctx)?;
        match mode {
            VimMode::Normal => Some("NOR"),
            VimMode::Visual(MotionType::Charwise) => Some("VIS"),
            VimMode::Visual(MotionType::Linewise) => Some("V-L"),
            VimMode::Replace => Some("REP"),
            // Insert mode is shown with a label, matching the GUI vim status indicator.
            VimMode::Insert => Some("INS"),
        }
    }

    /// Updates the watcher-backed git-status subscription after repository
    /// detection completes for the active working directory.
    pub(super) fn update_git_status_subscription(
        &mut self,
        repo_path: Option<LocalOrRemotePath>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.current_repo_path == repo_path && self.git_repo_status.is_some() {
            self.update_github_status_subscription(ctx);
            return;
        }
        self.current_repo_path = repo_path.clone();
        self.git_repo_status = None;
        self.github_repo = None;

        let Some(repo_path) = repo_path else {
            ctx.notify();
            return;
        };
        // The git-status singleton now keys on `LocalOrRemotePath` and serves
        // remote repos through a push receiver, so the previous narrowing to
        // `LocalOrRemotePath::Local` (and the early return for remote repos)
        // is gone.
        match GitRepoModels::handle(ctx).update(ctx, |models, ctx| models.subscribe(&repo_path, ctx))
        {
            Ok(handle) => {
                ctx.subscribe_to_model(&handle, |_, _, _, ctx| ctx.notify());
                self.git_repo_status = Some(handle);
            }
            Err(error) => {
                log::warn!("Unable to subscribe TUI footer to git status: {error}");
            }
        }
        self.update_github_status_subscription(ctx);
        ctx.notify();
    }

    /// Retains GitHub metadata only while the `GitHubPullRequest` statusline
    /// item is enabled, so a session that never shows the PR entry never runs
    /// `gh`. Re-run whenever the statusline configuration changes.
    pub(super) fn update_github_status_subscription(&mut self, ctx: &mut ViewContext<Self>) {
        let enabled = AISettings::as_ref(ctx)
            .tui_statusline
            .normalized()
            .is_enabled(TuiStatuslineItem::GitHubPullRequest);
        if !enabled {
            self.github_repo = None;
            ctx.notify();
            return;
        }
        if self.github_repo.is_some() {
            return;
        }
        let Some(repo_path) = self.current_repo_path.clone() else {
            return;
        };
        match GitRepoModels::handle(ctx).update(ctx, |models, ctx| {
            models.subscribe_github_repo(&repo_path, ctx)
        }) {
            Ok(handle) => {
                ctx.subscribe_to_model(&handle, |_, _, _, ctx| ctx.notify());
                self.github_repo = Some(handle);
            }
            Err(error) => {
                log::warn!("Unable to subscribe TUI footer to GitHub status: {error}");
            }
        }
        ctx.notify();
    }

    fn git_status_metadata<'a>(&self, ctx: &'a AppContext) -> Option<&'a GitStatusMetadata> {
        self.git_repo_status.as_ref()?.as_ref(ctx).metadata(ctx)
    }
}
