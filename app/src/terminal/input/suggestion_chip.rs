//! Where a prompt suggestion is drawn in the input (#2138, owner decision 2026-11-11).
//!
//! A model's prompt suggestion arrives seconds after the command it is about. It used to be a
//! banner the input grew for, so every visible row of output moved under the pointer and a click
//! meant for the output landed on the banner and started an agent conversation. The suggestion is
//! now a chip at the right end of the editor's first line: the one row every input layout has. It
//! is a positioned child of a stack, which adds nothing to the stack's size, so the input's height
//! is the same with and without it. This file is the pure part: the kill switch, the placement and
//! the room the chip gets on a line that also holds what the user types.
//! Design: `docs/design/r21-chip.md`.

use std::sync::OnceLock;

use unicode_width::UnicodeWidthChar;

/// Kill switch: `PHOSPHOR_PROMPT_SUGGESTION_CHIP=0` (also `off`, `false`, `no`) draws the
/// suggestion as the banner above the prompt, as before #2138: the input grows and shrinks with it.
pub(super) const SUGGESTION_CHIP_ENV: &str = "PHOSPHOR_PROMPT_SUGGESTION_CHIP";

/// Whether a suggestion is drawn as the chip: it is, unless the kill switch says off. Read once:
/// the layout must not change between two frames of one run.
pub(super) fn suggestion_chip_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        suggestion_chip_enabled_from(std::env::var(SUGGESTION_CHIP_ENV).ok().as_deref())
    })
}

/// [`suggestion_chip_enabled`] with the variable's value passed in (tests).
pub(super) fn suggestion_chip_enabled_from(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// Where the input draws a visible prompt suggestion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SuggestionPlacement {
    /// On the editor's first line; the input's height does not change.
    Chip,
    /// The banner above the prompt; the input grows by it (the layout before #2138).
    Banner,
}

impl SuggestionPlacement {
    /// The `place=` word of the `hop=prompt_chip` log line.
    pub(super) fn log_word(self) -> &'static str {
        match self {
            Self::Chip => "chip",
            Self::Banner => "banner",
        }
    }
}

/// The placement for one render of the input.
///
/// * `chip_enabled`: [`suggestion_chip_enabled`];
/// * `first_line_known`: the input can tell what the editor's first line holds besides the buffer.
///   It cannot for a same-line prompt that is not the shell's own prompt grid (context chips drawn
///   inside the editor, only without the agent view feature): that layout keeps the banner.
pub(super) fn suggestion_placement(
    chip_enabled: bool,
    first_line_known: bool,
) -> SuggestionPlacement {
    if chip_enabled && first_line_known {
        SuggestionPlacement::Chip
    } else {
        SuggestionPlacement::Banner
    }
}

/// What a layout tells [`super::Input::render_input_box`] about the editor's first text line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ChipLine {
    /// Columns at the start of the line that are not the buffer's: the shell's prompt when it is
    /// drawn inside the editor, else 0.
    pub(super) prefix_cols: usize,
    /// Pixels at the right end of the line that are taken: a right prompt and its gap, else 0.
    pub(super) right_inset_px: f32,
    /// How far under the editor's top edge the line is: the rows of a prompt drawn inside the
    /// editor above its last one, else 0.
    pub(super) y_offset_px: f32,
    /// See [`suggestion_placement`].
    pub(super) first_line_known: bool,
}

impl Default for ChipLine {
    /// The editor's first line holds the buffer only (terminal, agent and universal layouts, and
    /// the classic layout with the prompt on its own row).
    fn default() -> Self {
        Self {
            prefix_cols: 0,
            right_inset_px: 0.,
            y_offset_px: 0.,
            first_line_known: true,
        }
    }
}

/// Cells kept free between the end of the line's text and the chip.
pub(super) const CHIP_GAP_CELLS: f32 = 2.;
/// Under this many cells the chip is not drawn: an icon and too little of the label to read.
pub(super) const CHIP_MIN_CELLS: f32 = 16.;
/// From this many cells on the chip also shows the accept keystroke.
pub(super) const CHIP_KEYSTROKE_MIN_CELLS: f32 = 32.;
/// The chip never takes more than this part of the editor's width.
pub(super) const CHIP_MAX_FRACTION: f32 = 0.5;
/// Columns a tab is counted as (the widest it can be drawn).
const TAB_COLS: usize = 8;

/// The room on the editor's first line, in pixels and columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ChipRoom {
    /// The editor's width (an estimate at or under the real one).
    pub(super) editor_width_px: f32,
    /// The width of one cell of the editor's font.
    pub(super) cell_width_px: f32,
    /// Columns of the line that are taken from its start: prefix, buffer text, ghost text.
    pub(super) used_cols: usize,
    /// Pixels taken at the line's right end.
    pub(super) right_inset_px: f32,
}

/// The widest the chip may be drawn, or `None` when it is not drawn at all.
///
/// The chip gets what the line's text leaves free, less a gap, and never more than
/// [`CHIP_MAX_FRACTION`] of the editor. Under [`CHIP_MIN_CELLS`] it is not drawn: the suggestion
/// stays pending (the accept key and Escape work as before), it comes back when the line is
/// shorter.
pub(super) fn chip_max_width(room: ChipRoom) -> Option<f32> {
    let used_px = room.used_cols as f32 * room.cell_width_px;
    let free_px = room.editor_width_px
        - room.right_inset_px.max(0.)
        - used_px
        - CHIP_GAP_CELLS * room.cell_width_px;
    let width = free_px.min(room.editor_width_px * CHIP_MAX_FRACTION);
    // A size that is not known yet (zero, negative or NaN) draws no chip: every comparison with
    // NaN is false.
    (room.cell_width_px > 0. && width >= CHIP_MIN_CELLS * room.cell_width_px).then_some(width)
}

/// Whether a chip of this width also shows the accept keystroke.
pub(super) fn chip_shows_keystroke(width_px: f32, cell_width_px: f32) -> bool {
    width_px >= CHIP_KEYSTROKE_MIN_CELLS * cell_width_px
}

/// Display columns of the first line of `text` (up to its first line break).
pub(super) fn first_line_cols(text: &str) -> usize {
    text.chars()
        .take_while(|c| *c != '\n' && *c != '\r')
        .map(|c| {
            if c == '\t' {
                TAB_COLS
            } else {
                c.width().unwrap_or(0)
            }
        })
        .sum()
}

/// Columns the editor's first line holds from its start: the prefix, the buffer's first line,
/// and the ghost text of an autosuggestion when the buffer is a single line (the ghost text is
/// drawn after it; counting all of it can only make the chip narrower).
pub(super) fn used_cols(prefix_cols: usize, buffer: &str, ghost_text: Option<&str>) -> usize {
    let single_line = !buffer.contains(['\n', '\r']);
    let ghost = match ghost_text {
        Some(ghost) if single_line => first_line_cols(ghost),
        _ => 0,
    };
    prefix_cols
        .saturating_add(first_line_cols(buffer))
        .saturating_add(ghost)
}

#[cfg(test)]
#[path = "suggestion_chip_tests.rs"]
mod tests;
