//! What the app shows when an integration step fails (#2122).
//!
//! The driver writes the result as one `integ_failure_state` line into the scenario's log, so a
//! failed "menu is open" or "shell bootstrapped" assertion can be judged from the run alone:
//! the editor's text, which menu was up, whether the x-ray was set, and the active block.
//!
//! Everything here is what the scenario itself typed and ran (a scratch home, a throw-away
//! key), which is why the text is shown. Nothing in this file may panic: it runs on the failure
//! path, before the step's own failure is reported, and a window with no terminal (a settings
//! scenario) is an ordinary answer.

use warpui::{App, WindowId};

use crate::terminal::TerminalView;

/// At most this many characters of a block's command and of its output are kept.
const BLOCK_TEXT_LIMIT: usize = 1200;

/// The last `limit` characters of `text`, saying how many were cut.
fn tail(text: &str, limit: usize) -> String {
    let total = text.chars().count();
    if total <= limit {
        return text.to_owned();
    }
    let kept: String = text.chars().skip(total - limit).collect();
    format!("[{} chars cut]{kept}", total - limit)
}

/// One terminal pane's state, as `key=value` fields.
fn terminal_state(index: usize, view: &TerminalView, ctx: &warpui::AppContext) -> String {
    let (bootstrapped, read_only, pending_ssh, command, output) = {
        let model = view.model.lock();
        let block = model.block_list().active_block();
        (
            model.is_active_block_bootstrapped(),
            model.is_read_only(),
            model.has_pending_ssh_session(),
            tail(&block.command_to_string(), BLOCK_TEXT_LIMIT),
            tail(&block.output_to_string(), BLOCK_TEXT_LIMIT),
        )
    };
    let input = view.input().as_ref(ctx);
    let editor_text = input.buffer_text(ctx);
    let menu = input.suggestions_mode_model().as_ref(ctx).evidence();
    let xray = input.editor().as_ref(ctx).get_command_x_ray().is_some();
    format!(
        "terminal[{index}]: bootstrapped={bootstrapped} read_only={read_only} \
         pending_ssh={pending_ssh} helper_question={} menu=[{menu}] xray={xray} \
         editor={editor_text:?} block_command={command:?} block_output={output:?}",
        view.ssh_helper_question_showing()
    )
}

/// The state of every terminal pane of `window_id`, one per line.
pub fn failure_state(app: &mut App, window_id: WindowId) -> String {
    let app: &App = app;
    let views = app
        .views_of_type::<TerminalView>(window_id)
        .unwrap_or_default();
    if views.is_empty() {
        return "no terminal pane in the window".to_owned();
    }
    views
        .iter()
        .enumerate()
        .map(|(index, handle)| handle.read(app, |view, ctx| terminal_state(index, view, ctx)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::tail;

    #[test]
    fn the_tail_keeps_the_end_and_says_what_was_cut() {
        assert_eq!(tail("short", 10), "short");
        assert_eq!(tail("abcdefgh", 3), "[5 chars cut]fgh");
        // Counted in characters, never cut inside one.
        assert_eq!(tail("ααβγ", 2), "[2 chars cut]βγ");
    }
}
