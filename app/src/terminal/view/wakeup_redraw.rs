//! How much of the window a terminal wakeup has to redraw.
//!
//! A wakeup says the terminal model changed, usually because the pty delivered output. The
//! block list needs a full frame for that: its layout derives from block heights. The alt screen
//! does not: `AltScreenElement::paint` reads the grid from the model while painting, so once the
//! element tree exists, new content only needs paint to run again. Re-rendering the view (and the
//! footer views the wakeup used to notify) and laying out the whole window were pure overhead on
//! every keystroke echo in vim, less or htop.
//!
//! The element tree does bake in some model state, though: whether the alt screen is active at
//! all, whether the input box or a footer is shown, and so on. [`RenderFingerprint`] is that
//! state. `TerminalView::render` records the fingerprint it built the tree from, and a wakeup may
//! only skip the re-render while the fingerprint is unchanged.

use std::sync::OnceLock;

use warpui::color::ColorU;
use warpui::AppContext;

use crate::terminal::TerminalModel;

use super::TerminalView;

/// What a wakeup asks the window for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WakeupRedraw {
    /// Re-render the terminal view and its footer, then lay out and paint the window.
    Full,
    /// Paint the existing element tree again; see `AppContext::request_paint_only_redraw`.
    PaintOnly,
}

/// The model-derived inputs `TerminalView::render` reads while the alt screen is active, other
/// than the grid contents themselves. Anything here that changes between a render and a wakeup
/// means the element tree is stale, so the wakeup must re-render.
///
/// Alt-screen selection is not included: pty output never changes it (it is set, moved and
/// cleared by the view's own selection actions and by resize, all of which notify).
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
}

/// Decides the redraw for a wakeup from the fingerprint the current element tree was rendered
/// with (`None` before the first render) and the fingerprint right now.
pub(super) fn decide(
    last_rendered: Option<RenderFingerprint>,
    now: RenderFingerprint,
) -> WakeupRedraw {
    decide_with(last_rendered, now, alt_screen_paint_only_enabled())
}

pub(super) fn decide_with(
    last_rendered: Option<RenderFingerprint>,
    now: RenderFingerprint,
    paint_only_enabled: bool,
) -> WakeupRedraw {
    if !paint_only_enabled || !now.alt_screen_active {
        return WakeupRedraw::Full;
    }
    match last_rendered {
        Some(rendered) if rendered == now => WakeupRedraw::PaintOnly,
        Some(_) | None => WakeupRedraw::Full,
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

impl TerminalView {
    /// The [`RenderFingerprint`] of the model and view state right now. `model` is the guard the
    /// caller already holds; nothing here locks the model again.
    pub(super) fn render_fingerprint(
        &self,
        model: &TerminalModel,
        app: &AppContext,
    ) -> RenderFingerprint {
        let shared_session_status = model.shared_session_status();
        let active_block = model.block_list().active_block();
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
        }
    }
}

#[cfg(test)]
#[path = "wakeup_redraw_tests.rs"]
mod tests;
