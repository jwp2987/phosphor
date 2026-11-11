//! App-test "grid snapshot hook" (#2075, r14-test-gaps T34).
//!
//! When `PHOSPHOR_APPCI_GRID_DIR` names a directory, the app reads marker tokens from the file
//! `request` in it (one token per line; the harness appends while the app runs). Whenever the app
//! PAINTS a terminal grid whose visible rows contain one of those tokens, it writes what it is
//! painting to `<dir>/<token>.grid`: the visible rows cell by cell (text with its combining
//! characters, the colour each cell asked for, its attributes, wide cells and their spacers) and
//! the cursor (row, column, shape) when the cursor of that grid is drawn. The harness prints a
//! fixed escape script that starts with a token and compares the file with the expected grid, on
//! every platform, without OCR.
//!
//! The file is one JSON object, see [`format_snapshot`]; `appci_grid_tests.rs` pins it.
//!
//! Cost and privacy rules (the same as `warpui::appci_find`):
//! - With the variable unset, [`active_tokens`] is one cached `OnceLock` read and nothing else
//!   runs. No release user has the variable set.
//! - Only a grid that shows a listed token is ever written, and only to the directory the
//!   variable names. Nothing goes to the log.
//! - A grid whose secrets are being redacted is never written.
//! - The file is rewritten only when the painted content changed (temporary file, then rename, so
//!   a reader never sees half a snapshot).

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _, Hasher as _};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;

use crate::terminal::model::ansi::{Color, CursorShape, NamedColor};
use crate::terminal::model::cell::{Cell, DEFAULT_CHAR, Flags};
use crate::terminal::model::grid::grid_source::GridSource;

use super::model::char_or_str::CharOrStr;

/// Environment variable naming the snapshot directory.
pub const ENV_GRID_DIR: &str = "PHOSPHOR_APPCI_GRID_DIR";
/// The token list inside that directory.
pub const REQUEST_FILE: &str = "request";
/// Format version written as `"v"`.
pub const FORMAT_VERSION: u32 = 1;
/// How often the request file is re-checked for new tokens.
const RECHECK: Duration = Duration::from_millis(200);
/// Bounds of one snapshot (a test grid is a screen, not a scrollback).
const MAX_ROWS: usize = 400;
const MAX_COLS: usize = 1000;

/// One painted cell that is not blank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellSnap {
    pub col: usize,
    /// The cell's character followed by its zero-width (combining) characters; empty for the
    /// spacer half of a wide cell.
    pub text: String,
    pub fg: String,
    pub bg: String,
    /// Attribute letters, see [`attr_letters`].
    pub attrs: String,
}

/// One painted row: its index among the grid's displayed rows and its non-blank cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSnap {
    pub row: usize,
    pub cells: Vec<CellSnap>,
}

/// The cursor of the painted grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorSnap {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
}

/// A token is 6 to 64 ASCII letters or digits: it is used as a file name.
pub fn valid_token(token: &str) -> bool {
    (6..=64).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Parse the request file: one token per line, invalid lines ignored, duplicates dropped.
pub fn parse_tokens(contents: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in contents.lines().map(str::trim) {
        if valid_token(token) && !out.iter().any(|t| t == token) {
            out.push(token.to_owned());
        }
    }
    out
}

/// The colour a cell asked for, as the escape sequence gave it (not resolved through the theme,
/// so the expected grid is the same under every theme): `named:<name>` (SGR 30-37, 90-97 and the
/// defaults, lower case: `named:red`, `named:brightblue`, `named:foreground`), `idx:<0-255>`
/// (SGR 38;5 / 48;5) or `rgb:<rrggbb>` (SGR 38;2 / 48;2).
pub fn color_name(color: &Color) -> String {
    match color {
        Color::Named(named) => format!("named:{}", format!("{named:?}").to_lowercase()),
        Color::Indexed(index) => format!("idx:{index}"),
        Color::Spec(rgb) => format!("rgb:{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b),
    }
}

/// Attribute letters in a fixed order: `b` bold, `d` dim, `i` italic, `u` underline, `U` double
/// underline, `s` strikeout, `r` reverse video, `h` hidden, `W` wide character, `S` the spacer
/// cell after a wide character, `L` the spacer left at the end of a row when a wide character
/// wrapped.
pub fn attr_letters(flags: Flags) -> String {
    const LETTERS: [(Flags, char); 11] = [
        (Flags::BOLD, 'b'),
        (Flags::DIM, 'd'),
        (Flags::ITALIC, 'i'),
        (Flags::UNDERLINE, 'u'),
        (Flags::DOUBLE_UNDERLINE, 'U'),
        (Flags::STRIKEOUT, 's'),
        (Flags::INVERSE, 'r'),
        (Flags::HIDDEN, 'h'),
        (Flags::WIDE_CHAR, 'W'),
        (Flags::WIDE_CHAR_SPACER, 'S'),
        (Flags::LEADING_WIDE_CHAR_SPACER, 'L'),
    ];
    LETTERS
        .iter()
        .filter(|(flag, _)| flags.contains(*flag))
        .map(|(_, letter)| *letter)
        .collect()
}

/// The cursor shape's name in the snapshot.
pub fn shape_name(shape: CursorShape) -> &'static str {
    match shape {
        CursorShape::Block => "block",
        CursorShape::Underline => "underline",
        CursorShape::Beam => "beam",
        CursorShape::HollowBlock => "hollow_block",
        CursorShape::Hidden => "hidden",
    }
}

/// The text a cell shows: its character and combining characters; a never-written cell is a
/// space; the spacer half of a wide cell is empty.
fn cell_text(cell: &Cell) -> String {
    if cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        return String::new();
    }
    match cell.raw_content() {
        CharOrStr::Char(DEFAULT_CHAR) => " ".to_owned(),
        CharOrStr::Char(c) => c.to_string(),
        CharOrStr::Str(s) => s.to_owned(),
    }
}

/// A cell that paints nothing: a space (or never written), not a wide-cell spacer, on the default
/// background, with no attribute that draws on an empty cell.
fn is_blank(cell: &Cell, text: &str) -> bool {
    text == " "
        && cell.bg == Color::Named(NamedColor::Background)
        && !cell.flags.intersects(
            Flags::INVERSE | Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::STRIKEOUT,
        )
}

/// The non-blank cells of one row.
pub fn snap_cells(cells: &[Cell]) -> Vec<CellSnap> {
    cells
        .iter()
        .enumerate()
        .filter_map(|(col, cell)| {
            let text = cell_text(cell);
            if is_blank(cell, &text) {
                return None;
            }
            Some(CellSnap {
                col,
                text,
                fg: color_name(&cell.fg),
                bg: color_name(&cell.bg),
                attrs: attr_letters(cell.flags),
            })
        })
        .collect()
}

/// The row as plain text: every column in order, a blank for a cell that is not listed, nothing
/// for a wide cell's spacer; trailing blanks removed.
pub fn row_text(cells: &[CellSnap]) -> String {
    let mut out = String::new();
    let mut next_col = 0;
    for cell in cells {
        while next_col < cell.col {
            out.push(' ');
            next_col += 1;
        }
        out.push_str(&cell.text);
        next_col = cell.col + 1;
    }
    out.trim_end_matches(' ').to_owned()
}

/// The snapshot file's content: one JSON object,
/// `{"v":1,"token":T,"cols":N,"cursor":null|{"row":R,"col":C,"shape":S},"rows":[{"row":R,"text":X,"cells":[[col,text,fg,bg,attrs],..]},..]}`.
/// Rows are the grid's displayed rows that were painted, top to bottom; a cell that paints nothing
/// is not listed.
pub fn format_snapshot(
    token: &str,
    cols: usize,
    rows: &[RowSnap],
    cursor: Option<CursorSnap>,
) -> String {
    let rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let cells: Vec<serde_json::Value> = row
                .cells
                .iter()
                .map(|cell| serde_json::json!([cell.col, cell.text, cell.fg, cell.bg, cell.attrs]))
                .collect();
            serde_json::json!({"row": row.row, "text": row_text(&row.cells), "cells": cells})
        })
        .collect();
    let cursor = cursor.map(|cursor| {
        serde_json::json!({"row": cursor.row, "col": cursor.col, "shape": shape_name(cursor.shape)})
    });
    serde_json::json!({
        "v": FORMAT_VERSION,
        "token": token,
        "cols": cols,
        "cursor": cursor,
        "rows": rows,
    })
    .to_string()
}

/// The listed tokens that occur in the text of one of `rows`.
pub fn tokens_shown<'a>(rows: &[RowSnap], tokens: &'a [String]) -> Vec<&'a str> {
    let texts: Vec<String> = rows.iter().map(|row| row_text(&row.cells)).collect();
    tokens
        .iter()
        .filter(|token| texts.iter().any(|text| text.contains(token.as_str())))
        .map(String::as_str)
        .collect()
}

struct TokenCache {
    tokens: Arc<Vec<String>>,
    checked: Option<Instant>,
    sig: Option<(u64, SystemTime)>,
}

static GRID_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
static CACHE: Mutex<Option<TokenCache>> = Mutex::new(None);
/// Hash of the last snapshot written per token.
static WRITTEN: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

fn grid_dir() -> Option<&'static PathBuf> {
    GRID_DIR
        .get_or_init(|| {
            std::env::var_os(ENV_GRID_DIR)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .as_ref()
}

/// The tokens to look for, or `None` when the hook is off or the request file lists none yet. The
/// file is re-read when its size or mtime changed, at most every 200 ms.
pub fn active_tokens() -> Option<Arc<Vec<String>>> {
    let dir = grid_dir()?;
    let now = Instant::now();
    let mut guard = CACHE.lock();
    let cache = guard.get_or_insert_with(|| TokenCache {
        tokens: Arc::new(Vec::new()),
        checked: None,
        sig: None,
    });
    if cache
        .checked
        .is_none_or(|t| now.duration_since(t) >= RECHECK)
    {
        cache.checked = Some(now);
        let path = dir.join(REQUEST_FILE);
        if let Ok(meta) = std::fs::metadata(&path) {
            let sig = (
                meta.len(),
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            );
            if cache.sig != Some(sig) {
                cache.sig = Some(sig);
                if let Ok(contents) = std::fs::read_to_string(&path) {
                    cache.tokens = Arc::new(parse_tokens(&contents));
                }
            }
        }
    }
    (!cache.tokens.is_empty()).then(|| cache.tokens.clone())
}

/// Called by `render_grid` for the grid it is about to paint, only when [`active_tokens`] returned
/// tokens. `rows` yields `(displayed row, row index in the grid)` for the rows being painted;
/// `cursor_shape` is `Some` when the cursor of this grid is drawn.
pub fn report_painted_grid<G: GridSource>(
    grid: &G,
    rows: impl Iterator<Item = (usize, usize)>,
    cursor_shape: Option<CursorShape>,
    redacting_secrets: bool,
    tokens: &[String],
) {
    if redacting_secrets {
        return;
    }
    let Some(dir) = grid_dir() else {
        return;
    };
    let cols = grid.columns().min(MAX_COLS);
    let mut snaps = Vec::new();
    for (displayed_row, row_idx) in rows.take(MAX_ROWS) {
        let Some(row) = grid.row(row_idx) else {
            continue;
        };
        snaps.push(RowSnap {
            row: displayed_row,
            cells: snap_cells(&row[0..cols.min(row.len())]),
        });
    }
    let shown = tokens_shown(&snaps, tokens);
    if shown.is_empty() {
        return;
    }
    let cursor = cursor_shape.map(|shape| {
        let point = grid.cursor_render_point();
        CursorSnap {
            row: point.row,
            col: point.col,
            shape,
        }
    });
    for token in shown {
        let text = format_snapshot(token, cols, &snaps, cursor);
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();
        let mut written = WRITTEN.lock();
        let written = written.get_or_insert_with(HashMap::new);
        if written.get(token) == Some(&hash) {
            continue;
        }
        let tmp = dir.join(format!("{token}.grid.tmp"));
        let path = dir.join(format!("{token}.grid"));
        if std::fs::write(&tmp, text.as_bytes()).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            written.insert(token.to_owned(), hash);
        }
    }
}

#[cfg(test)]
#[path = "appci_grid_tests.rs"]
mod tests;
