use settings::Setting as _;
use warpui::{AppContext, SingletonEntity};

use super::{
    alt_screen_reporting::AltScreenReporting, model::grid::grid_handler::TermMode, TerminalModel,
};

pub mod alt_screen_element;

/// Whether file-path detection runs while the alt screen has SGR mouse reporting on (#1347).
/// Fullscreen TUIs such as Claude Code enable SGR mouse, and used to be treated as editors
/// (no file hover scan). `PHOSPHOR_ALT_FILE_LINKS=0` restores that.
pub fn file_links_with_mouse_reporting() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("PHOSPHOR_ALT_FILE_LINKS").map_or(true, |v| v != "0"))
}

/// True when the alt-screen hover must NOT scan for file paths.
pub fn skip_file_link_scan(sgr_mouse: bool, file_links_enabled: bool) -> bool {
    sgr_mouse && !file_links_enabled
}

/// Cmd (macOS) / Ctrl (elsewhere) held: the same modifier that opens a link on click.
pub fn link_menu_modifier_held(modifiers: &warpui::event::ModifiersState) -> bool {
    super::links::should_directly_open_link(modifiers)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

/// Determines if mouse event is intercepted based on SGR_MOUSE mode and mouse reporting setting.
pub fn should_intercept_mouse(model: &TerminalModel, shift: bool, ctx: &AppContext) -> bool {
    // Always intercept mouse for a shared session reader since their mouse events
    // will not be processed by the sharer's running terminal app.
    if model.shared_session_status().is_reader() || shift {
        return true;
    }
    // Require some level of mouse tracking to be enabled when the block list is active.
    let mouse_tracking = model.is_alt_screen_active()
        || model.is_term_mode_set(TermMode::MOUSE_REPORT_CLICK)
        || model.is_term_mode_set(TermMode::MOUSE_DRAG)
        || model.is_term_mode_set(TermMode::MOUSE_MOTION);
    let mouse_reporting_enabled = *AltScreenReporting::as_ref(ctx)
        .mouse_reporting_enabled
        .value();
    !(model.is_term_mode_set(TermMode::SGR_MOUSE) && mouse_tracking && mouse_reporting_enabled)
}

/// Determines if scroll event is intercepted. SGR_mouse and mouse reporting must be enabled to
/// report scroll events, otherwise, always intercept scroll.
pub fn should_intercept_scroll(model: &TerminalModel, ctx: &AppContext) -> bool {
    let scroll_reporting_enabled = *AltScreenReporting::as_ref(ctx)
        .scroll_reporting_enabled
        .value();
    should_intercept_mouse(model, false, ctx) || !scroll_reporting_enabled
}
