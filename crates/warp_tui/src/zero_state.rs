//! The pre-first-interaction "zero state" filling the transcript area: the
//! Phosphor Agent title and version, a "What's new" changelog section, and
//! the session's project context (rules and skills discovered), layered over
//! the rotating zero-state object animation.
//!
//! The session view owns visibility: the zero state fills the transcript
//! slot while the transcript has no visible content, so it dismisses once
//! the first accepted submission produces a block and returns whenever the
//! transcript empties out again.
//!
//! Layout: ported from Warp's `crates/warp_tui/src/zero_state.rs` at the
//! pinned oracle (`02b53fcd8` — see `ORACLE.md`) as part of #384, which
//! replaced this fork's side-by-side `TuiFlex::row(text_column,
//! flex_child(animation))` layout with a `TuiStack` — the animation fills the
//! whole view as a background layer, with the text column overlaid on top.
//! This matches the pin's actual rewrite (a full-bleed rotating object, not a
//! panel next to text) rather than just swapping the animation element in
//! place. See `zero_state_animation.rs` for the animation itself and why its
//! built-in mark isn't Warp's logo.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ai::project_context::model::{
    ProjectContextModel, ProjectContextModelEvent, ProjectRulesResult,
};
use warp::tui_export::{
    ActiveSession, ActiveSessionEvent, ChangelogModel, ChangelogModelEvent, ChangelogState,
    SkillManager, TuiMcpManager, TuiMcpServerStatus,
};
use warp_core::channel::ChannelState;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::SingletonEntity;
use warpui_core::elements::animation::AnimationClock;
use warpui_core::elements::tui::{Modifier, TuiConstrainedBox, TuiElement, TuiFlex, TuiStack, TuiText};
use warpui_core::{AppContext, Entity, ModelHandle, TuiView, ViewContext};

use crate::autoupdate::{TuiAutoupdateStatus, TuiAutoupdater, TuiAutoupdaterEvent};
use crate::tui_builder::TuiUiBuilder;
use crate::ui::abbreviate_home_prefix;
use crate::zero_state_animation::{
    ZeroStateAnimationConfig, ZeroStateAnimationConfigEvent, ZeroStateAnimationElement,
    ZeroStateMarkStyles,
};

/// Cap on "What's new" bullets, mirroring the compact zero-state mock.
const MAX_CHANGELOG_BULLETS: usize = 3;

/// Fixed width for the two constrained sub-sections of the text column (top:
/// title + version + changelog bullets; bottom: project context body + MCP).
/// Pinning both to the same value prevents the animation boundary from
/// shifting as content loads asynchronously at startup.
///
/// The project path *header* is rendered outside these constrained boxes so
/// it can use the column's full natural width instead of being capped by
/// this constant — trading a perfectly stable animation width for never
/// clipping the path: when the resolved header exceeds this constant and
/// still fits on one row, the text column can end up wider than
/// `LEFT_COLUMN_COLS`. See [`build_zero_state_text_column`] for the full
/// tradeoff.
const LEFT_COLUMN_COLS: u16 = 48;

// ---------------------------------------------------------------------------
// TuiZeroStateView
// ---------------------------------------------------------------------------

/// The zero-state view: displayed when the transcript is empty.
///
/// Owns the animation clock so the object's rotation remains continuous
/// across view re-renders (e.g. when MCP connects or a changelog loads), and
/// a snapshot of the animation's config (object shape, rotation period,
/// extrusion depth) refreshed whenever the backing settings/ASCII-art file
/// change (see `zero_state_animation::ZeroStateAnimationConfig`).
pub(crate) struct TuiZeroStateView {
    clock: AnimationClock,
    animation_config: Arc<ZeroStateAnimationConfig>,
    active_session: ModelHandle<ActiveSession>,
}

impl TuiZeroStateView {
    pub(crate) fn new(
        active_session: ModelHandle<ActiveSession>,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        // Subscribe to events that change what the zero state displays so
        // this view re-renders independently of its parent.
        ctx.subscribe_to_model(
            &ChangelogModel::handle(ctx),
            |_, _, event: &ChangelogModelEvent, ctx| {
                if let ChangelogModelEvent::ChangelogRequestComplete { .. } = event {
                    ctx.notify();
                }
            },
        );
        ctx.subscribe_to_model(
            &TuiAutoupdater::handle(ctx),
            |_, _, event: &TuiAutoupdaterEvent, ctx| {
                let TuiAutoupdaterEvent::StatusChanged = event;
                ctx.notify();
            },
        );
        ctx.subscribe_to_model(
            &ProjectContextModel::handle(ctx),
            |_, _, event: &ProjectContextModelEvent, ctx| {
                if let ProjectContextModelEvent::PathIndexed = event {
                    ctx.notify();
                }
            },
        );
        // Project-skill discovery completes asynchronously (after the repo
        // walk / directory watch settles), so the view must repaint on any
        // skill-inventory change or it can render before project skills
        // arrive and never be notified to update the discovered-skill count.
        ctx.subscribe_to_model(&SkillManager::handle(ctx), |_, _, _, ctx| ctx.notify());
        ctx.subscribe_to_model(&TuiMcpManager::handle(ctx), |_, _, _, ctx| ctx.notify());
        ctx.subscribe_to_model(&active_session, |_, _, event, ctx| {
            let ActiveSessionEvent::UpdatedPwd = event else {
                return;
            };
            ctx.notify();
        });
        let animation_config = ZeroStateAnimationConfig::handle(ctx);
        let animation_config_snapshot = Arc::new(animation_config.as_ref(ctx).clone());
        ctx.subscribe_to_model(
            &animation_config,
            |view, animation_config, event, ctx| match event {
                ZeroStateAnimationConfigEvent::Updated => {
                    view.animation_config = Arc::new(animation_config.as_ref(ctx).clone());
                    ctx.notify();
                }
                // The load-failure footer hint is shown by
                // `TuiTerminalSessionView`, which subscribes to the same
                // model directly (see terminal_session_view.rs); nothing
                // extra is needed here beyond the config snapshot itself
                // already reflecting the fallback shape.
                ZeroStateAnimationConfigEvent::LoadFailed(_) => {}
            },
        );

        Self {
            clock: AnimationClock::starting_at(Duration::ZERO),
            animation_config: animation_config_snapshot,
            active_session,
        }
    }
}

impl Entity for TuiZeroStateView {
    type Event = ();
}

impl TuiView for TuiZeroStateView {
    fn ui_name() -> &'static str {
        "TuiZeroStateView"
    }

    fn render(&self, ctx: &AppContext) -> Box<dyn TuiElement> {
        let builder = TuiUiBuilder::from_app(ctx);
        let session = self.active_session.as_ref(ctx);
        let cwd = session.current_working_directory().cloned().or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|cwd| cwd.to_string_lossy().into_owned())
        });
        let animation = ZeroStateAnimationElement::new(
            self.clock,
            self.animation_config.clone(),
            ZeroStateMarkStyles {
                front: builder.accent_text_style(),
                back: builder.primary_text_style(),
                side: builder.dim_text_style(),
                background: builder.muted_text_style(),
            },
        )
        .finish();
        let text_column = build_zero_state_text_column(cwd.as_deref(), &builder, ctx);
        TuiStack::new().child(animation).child(text_column).finish()
    }
}

/// Assembles the text column overlaid on top of the animation layer: the
/// top/bottom sections constrained to [`LEFT_COLUMN_COLS`], and the project
/// path header rendered between them at its natural (unconstrained) width so
/// a long path is never clipped: it wraps onto extra rows when the column is
/// narrow, or renders past 48 columns on a single row when there is enough
/// width, instead of losing content.
///
/// Both [`TuiZeroStateView::render`] and the regression tests call this
/// function so a change to how `render` composes the column is caught by the
/// test suite.
fn build_zero_state_text_column(
    cwd: Option<&str>,
    builder: &TuiUiBuilder,
    app: &AppContext,
) -> Box<dyn TuiElement> {
    // Compute project context once — find_applicable_project_rules walks the
    // directory tree and clones rule file contents, so resolving it once
    // avoids a redundant allocation on every zero-state re-render (pwd
    // change, changelog load, MCP update, PathIndexed, skill change).
    let (path_header_text, project_rules) = match cwd {
        Some(cwd) => {
            let cwd_path = LocalOrRemotePath::Local(PathBuf::from(cwd));
            let rules = ProjectContextModel::as_ref(app).find_applicable_project_rules(&cwd_path);
            let header_text = project_section_header_text(cwd, rules.as_ref());
            (Some(header_text), Some(rules))
        }
        None => (None, None),
    };

    // Title, version, and changelog — constrained to LEFT_COLUMN_COLS so
    // changelog bullets (which lack `.truncate()`) do not wrap against the
    // column's full natural width.
    let constrained_top = TuiConstrainedBox::new(render_top_section(builder, app).finish())
        .with_min_cols(LEFT_COLUMN_COLS)
        .with_max_cols(LEFT_COLUMN_COLS)
        .finish();

    // Project context body (rules / skills / placeholder) and MCP — also
    // constrained to LEFT_COLUMN_COLS, keeping those rows stable.
    let rules_ref = project_rules.flatten();
    let constrained_bottom = TuiConstrainedBox::new(
        render_bottom_section(cwd, rules_ref.as_ref(), builder, app).finish(),
    )
    .with_min_cols(LEFT_COLUMN_COLS)
    .with_max_cols(LEFT_COLUMN_COLS)
    .finish();

    // The project path header lives outside the LEFT_COLUMN_COLS-constrained
    // boxes so it can use the column's full natural width: it wraps onto
    // later rows when the column is narrow, or grows past 48 columns on one
    // row when there is enough width, rather than ever being clipped. See
    // the doc comment on this function for the full explanation.
    if let Some(path_header_text) = path_header_text {
        let header_style = builder.primary_text_style().add_modifier(Modifier::BOLD);
        let path_header = TuiText::new(path_header_text)
            .with_style(header_style)
            .finish();
        TuiFlex::column()
            .child(constrained_top)
            .child(blank_row())
            .child(path_header)
            .child(constrained_bottom)
            .finish()
    } else {
        TuiFlex::column()
            .child(constrained_top)
            .child(constrained_bottom)
            .finish()
    }
}

/// Top section of the text column: title, version, and changelog bullets.
///
/// Wrapped in a [`TuiConstrainedBox`] with `min = max = LEFT_COLUMN_COLS` by
/// the caller so that changelog bullets (which lack `.truncate()`) do not
/// word-wrap against the column's full natural width.
fn render_top_section(builder: &TuiUiBuilder, app: &AppContext) -> TuiFlex {
    let title_style = builder.accent_text_style().add_modifier(Modifier::BOLD);
    let header_style = builder.primary_text_style().add_modifier(Modifier::BOLD);
    let muted = builder.muted_text_style();

    let mut column = TuiFlex::column()
        .child(
            TuiText::new("Warp Agent")
                .with_style(title_style)
                .truncate()
                .finish(),
        )
        .child(render_version_line(builder, app));

    let bullets = changelog_bullets(app);
    if !bullets.is_empty() {
        column = column.child(blank_row()).child(
            TuiText::new("What's new")
                .with_style(header_style)
                .truncate()
                .finish(),
        );
        for bullet in bullets {
            // A fixed (non-flex) text child still wraps against the remaining
            // width while only reporting its natural width.
            column = column.child(
                TuiFlex::row()
                    .child(TuiText::new("• ").with_style(muted).truncate().finish())
                    .child(TuiText::new(bullet).with_style(muted).finish())
                    .finish(),
            );
        }
    }

    column
}

/// Bottom section of the text column: project context body (rules / skills /
/// placeholder) when a `cwd` is present, followed by the MCP section.
///
/// The project path *header* is intentionally omitted here — it is rendered
/// outside the constrained box so it can use the column's full natural width
/// (see [`build_zero_state_text_column`]).
///
/// `rules` must be the pre-computed [`ProjectRulesResult`] for `cwd`, resolved
/// once by the caller to avoid a duplicate upward directory walk.
fn render_bottom_section(
    cwd: Option<&str>,
    rules: Option<&ProjectRulesResult>,
    builder: &TuiUiBuilder,
    app: &AppContext,
) -> TuiFlex {
    let column = TuiFlex::column();
    let column = if let Some(cwd) = cwd {
        render_project_context_body(cwd, rules, column, builder, app)
    } else {
        column
    };
    render_mcp_section(column, builder, app)
}

/// Returns the abbreviated path text displayed as the project section header.
///
/// Uses the project root from `rules` when available, falling back to the raw
/// `cwd` string. This is the same text previously embedded inside the
/// 48-column constrained box; it is now computed separately so the caller can
/// render it outside that box.
///
/// `rules` must already be resolved by the caller (via [`ProjectContextModel`])
/// so the upward directory walk is not repeated for the project context body.
fn project_section_header_text(cwd: &str, rules: Option<&ProjectRulesResult>) -> String {
    let header = rules
        .map(|rules| rules.root_path.display().to_string())
        .unwrap_or_else(|| cwd.to_owned());
    abbreviate_home_prefix(&header)
}

fn render_mcp_section(mut column: TuiFlex, builder: &TuiUiBuilder, app: &AppContext) -> TuiFlex {
    let snapshot = TuiMcpManager::as_ref(app).snapshot();
    let header_style = builder.primary_text_style().add_modifier(Modifier::BOLD);
    let muted = builder.muted_text_style();
    column = column.child(blank_row()).child(
        TuiText::new("MCP")
            .with_style(header_style)
            .truncate()
            .finish(),
    );

    let (label, is_error) = mcp_status_label(snapshot);
    let style = if is_error {
        builder.error_text_style()
    } else {
        muted
    };
    column.child(TuiText::new(label).with_style(style).truncate().finish())
}

#[derive(Default)]
struct McpStatusCounts {
    running: usize,
    starting: usize,
    authenticating: usize,
    stopping: usize,
    failed: usize,
    offline: usize,
    available: usize,
}

impl McpStatusCounts {
    fn record(&mut self, status: &TuiMcpServerStatus) {
        match status {
            TuiMcpServerStatus::Available => self.available += 1,
            TuiMcpServerStatus::Offline => self.offline += 1,
            TuiMcpServerStatus::Starting => self.starting += 1,
            TuiMcpServerStatus::Authenticating => self.authenticating += 1,
            TuiMcpServerStatus::Running => self.running += 1,
            TuiMcpServerStatus::Stopping => self.stopping += 1,
            TuiMcpServerStatus::Failed { .. } => self.failed += 1,
        }
    }
}

/// The zero state's one-line MCP summary. There is no longer a single config
/// file whose health can be reported, so an unhealthy config is one more count
/// in the line rather than a state that replaces it.
fn mcp_status_label(snapshot: &warp::tui_export::TuiMcpSnapshot) -> (String, bool) {
    if snapshot.servers.is_empty() && snapshot.diagnostics.is_empty() {
        return ("No servers available · run /mcp".to_owned(), false);
    }
    let mut counts = McpStatusCounts::default();
    for server in &snapshot.servers {
        counts.record(&server.status);
    }
    let McpStatusCounts {
        running,
        starting,
        authenticating,
        stopping,
        failed,
        offline,
        available,
    } = counts;
    let mut parts = Vec::new();
    if running > 0 {
        parts.push(format!("{running} connected"));
    }
    if starting > 0 {
        parts.push(format!("{starting} starting"));
    }
    if authenticating > 0 {
        parts.push(format!("{authenticating} needs auth"));
    }
    if stopping > 0 {
        parts.push(format!("{stopping} stopping"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    if offline > 0 {
        parts.push(format!("{offline} offline"));
    }
    if available > 0 {
        parts.push(format!("{available} available"));
    }
    if !snapshot.diagnostics.is_empty() {
        parts.push(format!("{} config errors", snapshot.diagnostics.len()));
    }
    (
        format!("{} · /mcp", parts.join(" · ")),
        !snapshot.diagnostics.is_empty(),
    )
}

/// User-facing copy for each visible background updater status.
fn autoupdate_status_label(status: TuiAutoupdateStatus) -> Option<&'static str> {
    match status {
        TuiAutoupdateStatus::Idle => None,
        TuiAutoupdateStatus::Checking => Some("checking for updates…"),
        TuiAutoupdateStatus::Updating => Some("updating…"),
        TuiAutoupdateStatus::UpToDate => Some("up to date"),
        TuiAutoupdateStatus::Failed => Some("automatic update failed"),
        TuiAutoupdateStatus::PendingRestart => Some("update installed, restart to apply"),
    }
}

/// The version line: the release version (or "dev build"), with the
/// background auto-updater's status appended in parentheses. Dev builds
/// never run the updater (and have no version), so they render plain; the
/// `Idle` status (updater ineligible, or no stable check result yet) renders
/// no suffix either.
fn render_version_line(builder: &TuiUiBuilder, app: &AppContext) -> Box<dyn TuiElement> {
    let muted = builder.muted_text_style();
    let Some(version) = ChannelState::app_version() else {
        return TuiText::new("dev build")
            .with_style(muted)
            .truncate()
            .finish();
    };
    let status = TuiAutoupdater::as_ref(app).status();
    let Some(label) = autoupdate_status_label(status) else {
        return TuiText::new(version).with_style(muted).truncate().finish();
    };
    let style = match status {
        TuiAutoupdateStatus::Idle => unreachable!("idle status has no label"),
        TuiAutoupdateStatus::Checking
        | TuiAutoupdateStatus::Updating
        | TuiAutoupdateStatus::UpToDate => muted,
        TuiAutoupdateStatus::Failed => builder.error_text_style(),
        TuiAutoupdateStatus::PendingRestart => builder.success_glyph_style(),
    };
    // Like the bullet rows below: the version reports its natural width and
    // the suffix wraps against the remaining column width.
    TuiFlex::row()
        .child(
            TuiText::new(format!("{version} "))
                .with_style(muted)
                .truncate()
                .finish(),
        )
        .child(
            TuiText::new(format!("({label})"))
                .with_style(style)
                .finish(),
        )
        .finish()
}

/// Appends the project context body rows to `column`: the discovered rule
/// files and skill count (or a placeholder while discovery is still in
/// progress). Discovery is asynchronous, so a placeholder shows until results
/// land.
///
/// The project path *header* is intentionally omitted — it is rendered at the
/// outer level outside the constrained box so it can use the column's full
/// natural width (see [`build_zero_state_text_column`] and
/// [`project_section_header_text`]).
///
/// `rules` must be the pre-computed [`ProjectRulesResult`] for `cwd`, resolved
/// once by the caller to avoid a duplicate upward directory walk.
fn render_project_context_body(
    cwd: &str,
    rules: Option<&ProjectRulesResult>,
    mut column: TuiFlex,
    builder: &TuiUiBuilder,
    app: &AppContext,
) -> TuiFlex {
    let muted = builder.muted_text_style();
    let check = builder.success_glyph_style();

    // Use the pre-computed rules from build_zero_state_text_column —
    // find_applicable_project_rules is not called again here.
    let mut rule_files: Vec<String> = Vec::new();
    if let Some(rules) = rules {
        for rule in &rules.active_rules {
            if let Some(name) = rule.path.file_name().map(|n| n.to_string_lossy().into_owned())
                && !rule_files.iter().any(|file| *file == name)
            {
                rule_files.push(name);
            }
        }
    }

    let cwd_path = LocalOrRemotePath::Local(PathBuf::from(cwd));
    let project_skill_count = SkillManager::as_ref(app)
        .get_skills_for_working_directory(Some(&cwd_path), app)
        .iter()
        .filter(|skill| skill.is_project_skill())
        .count();

    if rule_files.is_empty() && project_skill_count == 0 {
        // Repo detection, metadata indexing, and skill scans are async, so
        // nothing may be known yet; this also covers projects with no
        // context at all.
        return column.child(
            TuiText::new("Discovering project context…")
                .with_style(builder.dim_text_style())
                .truncate()
                .finish(),
        );
    }

    let status_row = |column: TuiFlex, text: String| {
        column.child(
            TuiFlex::row()
                .child(TuiText::new("✓ ").with_style(check).truncate().finish())
                .child(TuiText::new(text).with_style(muted).truncate().finish())
                .finish(),
        )
    };
    for file in rule_files {
        column = status_row(column, format!("{file} loaded"));
    }
    if project_skill_count > 0 {
        let plural = if project_skill_count == 1 { "" } else { "s" };
        column = status_row(
            column,
            format!("{project_skill_count} skill{plural} discovered"),
        );
    }
    column
}

/// Up to [`MAX_CHANGELOG_BULLETS`] plain-text bullets for the current
/// version's changelog, or empty when no changelog is available (request
/// failed, still pending, or a channel without release changelogs).
fn changelog_bullets(app: &AppContext) -> Vec<String> {
    let ChangelogState::Some(changelog) = &ChangelogModel::as_ref(app).changelog else {
        return Vec::new();
    };
    let from_sections = changelog
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .take(MAX_CHANGELOG_BULLETS)
        .cloned()
        .collect::<Vec<_>>();
    if !from_sections.is_empty() {
        return from_sections;
    }
    // Newer payloads may only populate the markdown sections; fall back to
    // their top-level bullet lines.
    changelog
        .markdown_sections
        .iter()
        .flat_map(|section| section.markdown.lines())
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("* ").or_else(|| line.strip_prefix("- "))
        })
        .take(MAX_CHANGELOG_BULLETS)
        .map(ToOwned::to_owned)
        .collect()
}

/// A one-row spacer between sections.
fn blank_row() -> Box<dyn TuiElement> {
    TuiText::new(" ").truncate().finish()
}

#[cfg(test)]
#[path = "zero_state_tests.rs"]
mod tests;
