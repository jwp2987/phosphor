//! Owned, thread-safe copies of the grid cells a plain-text range read touches, so a huge
//! selection can be stringified off the UI thread (#1304).
//!
//! [`GridHandler::bounds_to_text_snapshot`] captures exactly what
//! [`GridHandler::bounds_to_string`] would read (only the cells inside the selected columns of
//! each selected row), under the caller's short model lock. [`GridTextSnapshot::render`] then
//! runs the same per-line logic as `line_to_string` without touching the model, so the text is
//! byte-identical to the inline path.
//!
//! [`GridHandler::bounds_to_text_snapshot`]: super::grid_handler::GridHandler::bounds_to_text_snapshot
//! [`GridHandler::bounds_to_string`]: super::grid_handler::GridHandler::bounds_to_string

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::terminal::model::cell::Flags;
use crate::util::extensions::TrimStringExt;

/// What `Cell::content_for_display` yields, owned.
pub(super) enum SnapshotContent {
    Char(char),
    Str(Box<str>),
}

pub(super) struct SnapshotCell {
    pub(super) content: SnapshotContent,
    pub(super) is_tab: bool,
    pub(super) flags: Flags,
}

/// One row of a [`GridTextSnapshot`]; mirrors the inputs of `GridHandler::line_to_string`.
pub(super) struct SnapshotRow {
    /// Column range after the wide-char-spacer adjustment `line_to_string` applies.
    pub(super) cols: Range<usize>,
    pub(super) row_length: usize,
    /// Cells for `cols.start..row_length`, stopping at the first missing cell.
    pub(super) cells: Vec<SnapshotCell>,
    /// Parallel to `cells` when secrets are shown obfuscated; empty otherwise.
    pub(super) obfuscated: Vec<bool>,
    /// Flags of the cell at `row_length - 1`.
    pub(super) last_flags: Option<Flags>,
    /// The first cell's raw char of the previous row, captured only when the leading wide-char
    /// spacer case needs it.
    pub(super) prev_first_char: Option<char>,
}

impl SnapshotRow {
    fn render_into(&self, tabs: &[bool], columns: usize, text: &mut String) {
        let mut tab_mode = false;
        for (index, cell) in self.cells.iter().enumerate() {
            let col = self.cols.start + index;
            if tab_mode {
                if tabs[col] {
                    tab_mode = false;
                } else {
                    continue;
                }
            }
            if cell.is_tab {
                tab_mode = true;
            }
            if !cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                if self.obfuscated.get(index).copied().unwrap_or(false) {
                    text.push('*');
                } else {
                    match &cell.content {
                        SnapshotContent::Char(c) => text.push(*c),
                        SnapshotContent::Str(s) => text.push_str(s),
                    }
                }
            }
        }

        if self.cols.end >= columns - 1
            && (self.row_length == 0
                || !self
                    .last_flags
                    .is_some_and(|flags| flags.contains(Flags::WRAPLINE)))
        {
            text.push('\n');
        }

        if let Some(c) = self.prev_first_char {
            text.push(c);
        }
    }
}

/// The text of one `bounds_to_string` call, ready to render on any thread.
#[derive(Default)]
pub struct GridTextSnapshot {
    pub(super) rows: Vec<SnapshotRow>,
    pub(super) tabs: Vec<bool>,
    pub(super) columns: usize,
}

impl GridTextSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Same text `GridHandler::bounds_to_string` returns for the captured range (plain text).
    pub fn render(&self) -> String {
        let mut res = String::new();
        for row in &self.rows {
            row.render_into(&self.tabs, self.columns, &mut res);
        }
        res.trim_trailing_newline();
        res
    }

    /// Like [`Self::render`], but checks `cancel` every [`CANCEL_CHECK_ROWS`] rows and returns
    /// `None` once it is set, so a superseded copy stops burning a core (#1382).
    pub fn render_cancellable(&self, cancel: &AtomicBool) -> Option<String> {
        let mut res = String::new();
        for (index, row) in self.rows.iter().enumerate() {
            if index % CANCEL_CHECK_ROWS == 0 && cancel.load(Ordering::Relaxed) {
                return None;
            }
            row.render_into(&self.tabs, self.columns, &mut res);
        }
        res.trim_trailing_newline();
        Some(res)
    }

    /// Number of captured rows.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

/// How many rows a cancellable render processes between checks of its cancel flag.
const CANCEL_CHECK_ROWS: usize = 1024;

/// The text of one block's selected range: one snapshot per grid it spans (prompt, rprompt,
/// command, output).
#[derive(Default)]
pub struct BlockTextSnapshot {
    pub pieces: Vec<GridTextSnapshot>,
}

impl BlockTextSnapshot {
    /// Same text `Block::bounds_to_string` returns: non-empty grid texts joined by newlines.
    pub fn render(&self) -> String {
        itertools::join(
            self.pieces
                .iter()
                .map(GridTextSnapshot::render)
                .filter(|content| !content.is_empty()),
            "\n",
        )
    }

    /// Cancellable [`Self::render`]; `None` when `cancel` was set.
    pub fn render_cancellable(&self, cancel: &AtomicBool) -> Option<String> {
        let mut out: Vec<String> = Vec::new();
        for piece in &self.pieces {
            let content = piece.render_cancellable(cancel)?;
            if !content.is_empty() {
                out.push(content);
            }
        }
        Some(out.join("\n"))
    }

    /// Number of captured rows across all grids.
    pub fn row_count(&self) -> usize {
        self.pieces.iter().map(GridTextSnapshot::row_count).sum()
    }
}

/// One piece of a selection's text: already built (rich content, small blocks) or still a grid
/// snapshot to render.
pub enum SelectionPart {
    Text(String),
    Block(BlockTextSnapshot),
}

impl From<String> for SelectionPart {
    fn from(text: String) -> Self {
        SelectionPart::Text(text)
    }
}

/// A selection's text for callers that only want a small, bounded amount of it (Find-bar prefill,
/// screen-reader announcements; #1304 follow-up). Never builds more than the caller's row cap.
#[derive(Debug, PartialEq, Eq)]
pub enum BoundedSelection {
    /// Nothing is selected.
    None,
    /// The whole selection, which is within the row cap.
    Text(String),
    /// The selection is over the row cap; nothing was built.
    TooLarge,
}

impl BoundedSelection {
    pub fn from_parts(parts: Option<SelectionParts>) -> Self {
        match parts {
            None => Self::None,
            Some(parts) if parts.has_snapshots() => Self::TooLarge,
            Some(parts) => Self::Text(parts.render()),
        }
    }
}

/// The whole selection's text, in output order. `Send`, so [`Self::render`] can run on a
/// background thread.
#[derive(Default)]
pub struct SelectionParts {
    parts: Vec<SelectionPart>,
}

impl SelectionParts {
    pub fn new(parts: Vec<SelectionPart>) -> Self {
        Self { parts }
    }

    /// Whether any part still has to be rendered from a snapshot.
    pub fn has_snapshots(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, SelectionPart::Block(_)))
    }

    /// The selection text; identical to the old inline `selected_texts.join("\n")`.
    pub fn render(self) -> String {
        self.parts
            .into_iter()
            .map(|part| match part {
                SelectionPart::Text(text) => text,
                SelectionPart::Block(snapshot) => snapshot.render(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Cancellable [`Self::render`]; `None` when `cancel` was set before it finished.
    pub fn render_cancellable(self, cancel: &AtomicBool) -> Option<String> {
        let mut out = Vec::with_capacity(self.parts.len());
        for part in self.parts {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            out.push(match part {
                SelectionPart::Text(text) => text,
                SelectionPart::Block(snapshot) => snapshot.render_cancellable(cancel)?,
            });
        }
        Some(out.join("\n"))
    }
}
