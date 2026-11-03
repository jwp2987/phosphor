//! How much of the window a terminal wakeup has to redraw.
//!
//! A wakeup says the terminal model changed, usually because the pty delivered output. The
//! block list needs a full frame for that: its layout derives from block heights. The alt screen
//! does not: `AltScreenElement::paint` reads the grid from the model while painting, so once the
//! element tree exists, new content only needs paint to run again. Re-rendering the view (and the
//! footer views the wakeup used to notify) and laying out the whole window were pure overhead on
//! every keystroke echo in vim, less or htop.
//!
//! The block list is in between: `BlockListElement::layout` reads block heights and creates the
//! labels of newly visible blocks from the model, and its paint reads the blocks, so new output
//! needs layout and paint but not a rebuilt tree.
//!
//! The element tree does bake in some model state, though: whether the alt screen is active at
//! all, whether the input box or a footer is shown, which block is active, and so on.
//! [`RenderFingerprint`] is that state. `TerminalView::render` records the fingerprint it built
//! the tree from, and a wakeup may only skip the re-render while the fingerprint is unchanged.
//! The agent footer views have their own, smaller [`FooterFingerprint`].

use std::sync::OnceLock;

use warpui::color::ColorU;
use warpui::{AppContext, SingletonEntity};

use crate::terminal::TerminalModel;
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::terminal::model::terminal_model::BlockIndex;

use super::TerminalView;

/// What a wakeup asks the window for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WakeupRedraw {
    /// Re-render the terminal view and its footer, then lay out and paint the window.
    Full,
    /// Lay out and paint the existing element tree again; see `AppContext::request_redraw`.
    Relayout,
    /// Paint the existing element tree again; see `AppContext::request_paint_only_redraw`.
    PaintOnly,
}

/// The model-derived inputs `TerminalView::render` reads, other than the grid contents
/// themselves. Anything here that changes between a render and a wakeup means the element tree
/// is stale, so the wakeup must re-render.
///
/// Selections are reduced to their presence: pty output never moves one (the view's own
/// selection actions and resize do, and they notify); only eviction can invalidate one, and
/// `evicted_total` covers that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RenderFingerprint {
    pub(super) alt_screen_active: bool,
    pub(super) input_box_visible: bool,
    pub(super) footer_bar_has_content: bool,
    pub(super) remote_server_loading_footer: bool,
    pub(super) shared_session_view_pending: bool,
    pub(super) shared_session_active_viewer: bool,
    pub(super) shared_session_viewer: bool,
    pub(super) loading_conversation_transcript: bool,
    pub(super) agent_in_control_or_tagged_in: bool,
    pub(super) waterfall_gap_mode: bool,
    pub(super) long_running_command: bool,
    /// Read by the agent footer's render for its background. It is derived from the colors
    /// sampled during paint, so it changes *after* a paint, which is what the settle check in
    /// `TerminalView::handle_terminal_wakeup` is for.
    pub(super) inferred_bg_color: Option<ColorU>,
    /// Block-list inputs. The block list's layout and paint read the blocks live; these are
    /// what `render` and `BlockListElement::new` capture from them.
    pub(super) read_only: bool,
    pub(super) evicted_total: usize,
    pub(super) active_block_index: BlockIndex,
    pub(super) has_active_gap: bool,
    pub(super) has_block_banner: bool,
    pub(super) filtered_block_count: usize,
    pub(super) has_block_selection: bool,
    pub(super) overhanging_bottom_block: bool,
    pub(super) in_band_command: bool,
}

/// The model-derived inputs the agent footer views (`UseAgentToolbar` and its children) read
/// when they render. A wakeup that keeps the terminal view's tree re-renders them only when one
/// of these changed since they were last notified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FooterFingerprint {
    pub(super) alt_screen_active: bool,
    pub(super) inferred_bg_color: Option<ColorU>,
    pub(super) footer_bar_has_content: bool,
    pub(super) warpify_mode_set: bool,
    pub(super) cli_agent_present: bool,
    pub(super) cli_input_open: bool,
    pub(super) eligible_for_agent_handoff: bool,
    pub(super) has_requested_command_action: bool,
}

/// Decides the redraw for a wakeup from the fingerprint the current element tree was rendered
/// with (`None` before the first render) and the fingerprint right now.
pub(super) fn decide(
    last_rendered: Option<RenderFingerprint>,
    now: RenderFingerprint,
) -> WakeupRedraw {
    decide_with(
        last_rendered,
        now,
        alt_screen_paint_only_enabled(),
        block_list_relayout_enabled(),
    )
}

pub(super) fn decide_with(
    last_rendered: Option<RenderFingerprint>,
    now: RenderFingerprint,
    paint_only_enabled: bool,
    relayout_enabled: bool,
) -> WakeupRedraw {
    let tree_is_current = last_rendered == Some(now);
    match (now.alt_screen_active, tree_is_current) {
        (true, true) if paint_only_enabled => WakeupRedraw::PaintOnly,
        (false, true) if relayout_enabled => WakeupRedraw::Relayout,
        (true | false, true | false) => WakeupRedraw::Full,
    }
}

/// Paint-only alt-screen wakeups are the default. `PHOSPHOR_ALT_SCREEN_FULL_RENDER=1` restores
/// a full re-render on every wakeup, to rule the optimisation in or out when something on an
/// alt-screen frame looks stale.
fn alt_screen_paint_only_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        paint_only_unless_disabled(
            std::env::var("PHOSPHOR_ALT_SCREEN_FULL_RENDER")
                .ok()
                .as_deref(),
        )
    })
}

fn paint_only_unless_disabled(full_render_flag: Option<&str>) -> bool {
    full_render_flag.is_none_or(|flag| flag == "0")
}

/// Block-list wakeups that keep the element tree are the default. `PHOSPHOR_BLOCK_LIST_FULL_RENDER=1`
/// restores a full re-render on every wakeup.
fn block_list_relayout_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        paint_only_unless_disabled(
            std::env::var("PHOSPHOR_BLOCK_LIST_FULL_RENDER")
                .ok()
                .as_deref(),
        )
    })
}

impl TerminalView {
    /// The [`RenderFingerprint`] of the model and view state right now. `model` is the guard the
    /// caller already holds; nothing here locks the model again.
    pub(super) fn render_fingerprint(
        &self,
        model: &TerminalModel,
        app: &AppContext,
    ) -> RenderFingerprint {
        let shared_session_status = model.shared_session_status();
        let block_list = model.block_list();
        let active_block = block_list.active_block();
        let input_mode = self.current_input_mode(app);
        let overhanging_bottom_block = self
            .viewport_state(block_list, input_mode, app)
            .overhanging_bottom_block(app)
            .is_some();
        RenderFingerprint {
            alt_screen_active: model.is_alt_screen_active(),
            input_box_visible: self.is_input_box_visible(model, app),
            footer_bar_has_content: self
                .use_agent_footer_view_id_for_window_footer_bar(model, app)
                .is_some(),
            remote_server_loading_footer: self.show_remote_server_loading_footer(model, app),
            shared_session_view_pending: shared_session_status.is_view_pending(),
            shared_session_active_viewer: shared_session_status.is_active_viewer(),
            shared_session_viewer: shared_session_status.is_viewer(),
            loading_conversation_transcript: model.is_loading_conversation_transcript(),
            agent_in_control_or_tagged_in: active_block.is_agent_in_control_or_tagged_in(),
            waterfall_gap_mode: self.is_waterfall_gap_mode(model, app),
            long_running_command: active_block.is_active_and_long_running(),
            inferred_bg_color: model.alt_screen().inferred_bg_color(),
            read_only: model.is_read_only(),
            evicted_total: block_list.evicted_total(),
            active_block_index: block_list.active_block_index(),
            has_active_gap: block_list.active_gap().is_some(),
            has_block_banner: active_block.block_banner().is_some(),
            filtered_block_count: block_list.filtered_blocks().len(),
            has_block_selection: block_list.selection().is_some(),
            overhanging_bottom_block,
            in_band_command: block_list.is_writing_or_executing_in_band_command(),
        }
    }

    /// The [`FooterFingerprint`] right now; same locking contract as [`Self::render_fingerprint`].
    pub(super) fn footer_fingerprint(
        &self,
        model: &TerminalModel,
        app: &AppContext,
    ) -> FooterFingerprint {
        let active_block = model.block_list().active_block();
        let footer = self.use_agent_footer.as_ref(app);
        let cli_agent_sessions = CLIAgentSessionsModel::as_ref(app);
        FooterFingerprint {
            alt_screen_active: model.is_alt_screen_active(),
            inferred_bg_color: model.alt_screen().inferred_bg_color(),
            footer_bar_has_content: self
                .use_agent_footer_view_id_for_window_footer_bar(model, app)
                .is_some(),
            warpify_mode_set: footer.warpify_mode(app).is_some(),
            cli_agent_present: footer.has_cli_agent(app),
            cli_input_open: cli_agent_sessions.is_input_open(self.view_id),
            eligible_for_agent_handoff: active_block.is_eligible_for_agent_handoff(),
            has_requested_command_action: active_block.requested_command_action_id().is_some(),
        }
    }
}

#[cfg(test)]
#[path = "wakeup_redraw_tests.rs"]
mod tests;
