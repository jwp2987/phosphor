//! A narrow driver for fuzzing the pty-output path, shared by the in-tree
//! `#[ignore]`d harness (`fuzz_tests.rs`) and the out-of-tree libFuzzer target
//! in `app/fuzz/`.
//!
//! Only compiled for tests and with the opt-in `fuzzing` cargo feature (which
//! implies `test-util`, for `TerminalModel::mock`). With the feature off --
//! every normal build -- this module does not exist.
//!
//! [`FuzzTerminal`] wraps a [`TerminalModel`] plus a persistent ANSI
//! [`Processor`] and exposes the operations a pty and the app perform on it;
//! [`check_invariants`] checks the invariants the model's own code relies on;
//! [`run_fuzz_input`] decodes one libFuzzer input into those operations.

use std::any::Any;
use std::io::sink;

use warp_core::command::ExitCode;
use warp_core::features::FeatureFlag;

use super::TerminalModel;
use super::ansi::{
    ClearValue, CommandFinishedValue, CompletionMetadata, Handler, PrecmdValue, PreexecValue,
    Processor, PromptMetadata,
};
use super::block::BlockId;
use super::find::{FindConfig, RegexDFAs};
use super::grid::Dimensions;
use super::grid::grid_handler::GridHandler;
use super::index::Point;
use crate::terminal::SizeUpdate;

/// The session ID `TerminalModel::new_for_test` registers, and so the one DCS
/// hooks must carry to pass `Performer::validate_hook_session_id`.
pub const SESSION_ID: u64 = 123;

/// Runtime feature flags that gate pty-output parsing paths.
#[derive(Clone, Copy, Debug)]
pub struct FuzzFlags {
    /// `FeatureFlag::KittyImages`: kitty graphics APC handling.
    pub kitty_images: bool,
    /// `FeatureFlag::OscHyperlinks`: OSC 8 handling.
    pub osc_hyperlinks: bool,
    /// `FeatureFlag::SSHTmuxWrapper`: `DCS 1000p` enters tmux control mode.
    pub tmux_control_mode: bool,
}

/// A [`TerminalModel`] driven the way the pty event loop drives it.
pub struct FuzzTerminal {
    model: TerminalModel,
    processor: Processor,
    /// Thread-local feature-flag overrides, held for the terminal's lifetime.
    _flag_overrides: Vec<Box<dyn Any>>,
}

impl FuzzTerminal {
    /// A bootstrapped model (`TerminalModel::mock`) with the given flags.
    ///
    /// The flag overrides are thread-local, so the terminal must be driven on
    /// the thread that created it.
    pub fn new(flags: FuzzFlags) -> Self {
        let flag_overrides: Vec<Box<dyn Any>> = vec![
            Box::new(FeatureFlag::KittyImages.override_enabled(flags.kitty_images)),
            Box::new(FeatureFlag::OscHyperlinks.override_enabled(flags.osc_hyperlinks)),
            Box::new(FeatureFlag::SSHTmuxWrapper.override_enabled(flags.tmux_control_mode)),
        ];
        Self {
            model: TerminalModel::mock(None, None),
            processor: Processor::new(),
            _flag_overrides: flag_overrides,
        }
    }

    pub fn model(&self) -> &TerminalModel {
        &self.model
    }

    /// One pty read. Parser state persists across calls, so sequences may be
    /// split between reads.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.processor
            .parse_bytes(&mut self.model, bytes, &mut sink());
    }

    /// A pane resize to `rows` x `cols` cells (clamped by `SizeInfo`).
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let last = *self.model.block_list().size();
        self.model
            .resize(SizeUpdate::from_cell_dimensions(last, rows, cols));
    }

    /// Ends a synchronized update, as the event loop does on its timeout.
    pub fn finish_sync_output(&mut self) {
        self.processor
            .finish_sync_output(&mut self.model, &mut sink());
    }

    /// Drops all parser state (as `TerminalModel::process_bytes` does per call).
    pub fn reset_parser(&mut self) {
        self.processor = Processor::new();
    }

    /// The user submits a command (`TerminalModel::start_command_execution`).
    pub fn start_command(&mut self) {
        let _ = self.model.start_command_execution();
    }

    // The hook calls below go straight to the `Handler`, bypassing the parser
    // and session validation; the parser path is reached by feeding the
    // framed bytes from `dcs_hook`/`osc_hook` instead.

    pub fn preexec(&mut self, command: &str) {
        Handler::preexec(
            &mut self.model,
            PreexecValue {
                command: command.to_owned(),
                session_id: Some(SESSION_ID),
            },
        );
    }

    pub fn command_finished(&mut self, exit: i32, next_block_id: &str) {
        Handler::command_finished(
            &mut self.model,
            CommandFinishedValue {
                completion_metadata: completion(exit, next_block_id),
                session_id: Some(SESSION_ID),
            },
        );
    }

    pub fn precmd(&mut self, exit: i32, next_block_id: &str) {
        Handler::precmd_with_completion_metadata(
            &mut self.model,
            PrecmdValue {
                completion_metadata: completion(exit, next_block_id),
                prompt_metadata: prompt_metadata(),
            },
        );
    }

    pub fn prompt_only_precmd(&mut self) {
        Handler::prompt_only_precmd(&mut self.model, prompt_metadata());
    }

    pub fn clear_hook(&mut self) {
        Handler::clear(
            &mut self.model,
            ClearValue {
                session_id: Some(SESSION_ID),
            },
        );
    }

    pub fn clear_visible_screen(&mut self) {
        self.model.clear_visible_screen();
    }

    pub fn find(&self, query: &str, regex: bool, case_sensitive: bool) -> Result<(), String> {
        run_find(&self.model, query, regex, case_sensitive)
    }

    pub fn check_invariants(&self) -> Result<(), String> {
        check_invariants(&self.model)
    }
}

fn completion(exit: i32, next_block_id: &str) -> CompletionMetadata {
    CompletionMetadata {
        exit_code: ExitCode::from(exit),
        next_block_id: BlockId::from(next_block_id.to_owned()),
    }
}

fn prompt_metadata() -> PromptMetadata {
    PromptMetadata {
        session_id: Some(SESSION_ID),
        ..Default::default()
    }
}

/// A hex-encoded hook framed as the bootstrap scripts send it on a pty:
/// `ESC P $ d <hex JSON> 0x9c`.
pub fn dcs_hook(json: &[u8]) -> Vec<u8> {
    let mut out = b"\x1bP$d".to_vec();
    out.extend(hex::encode(json).into_bytes());
    out.push(0x9c);
    out
}

/// A hex-encoded hook framed as the ConPTY path sends it: `OSC 9278;d;<hex> BEL`.
pub fn osc_hook(json: &[u8]) -> Vec<u8> {
    let mut out = b"\x1b]9278;d;".to_vec();
    out.extend(hex::encode(json).into_bytes());
    out.push(0x07);
    out
}

/// An unencoded hook: `ESC P $ f <JSON> 0x9c` (accepted for the RC-file and
/// init hooks only).
pub fn unencoded_dcs_hook(json: &[u8]) -> Vec<u8> {
    let mut out = b"\x1bP$f".to_vec();
    out.extend_from_slice(json);
    out.push(0x9c);
    out
}

/// Runs find over every block's prompt/command and output grids and the alt
/// screen -- the same per-grid calls `terminal::find::model::block_list` and
/// `::alt_screen` make -- and checks each match is a forward range
/// (`RegexDFAs::regex_search_*` build `left..=right` by scanning back from the
/// match end).
pub fn run_find(
    model: &TerminalModel,
    query: &str,
    regex: bool,
    case_sensitive: bool,
) -> Result<(), String> {
    let Ok(dfas) = RegexDFAs::new_with_config(
        query,
        FindConfig {
            is_regex_enabled: regex,
            is_case_sensitive: case_sensitive,
        },
    ) else {
        // An invalid regex is rejected the same way the find bar rejects it.
        return Ok(());
    };
    let check =
        |label: &str, matches: Vec<std::ops::RangeInclusive<Point>>| -> Result<(), String> {
            for range in matches {
                let (start, end) = (range.start(), range.end());
                if (start.row, start.col) > (end.row, end.col) {
                    return Err(format!(
                        "find {query:?}: {label} returned a backwards match {start:?}..={end:?}"
                    ));
                }
            }
            Ok(())
        };
    for (i, block) in model.block_list().blocks().iter().enumerate() {
        check(
            &format!("block {i} prompt/command grid"),
            block.find_prompt_and_command_grid_matches(&dfas),
        )?;
        check(
            &format!("block {i} output grid"),
            block.find_output_grid_matches(&dfas),
        )?;
    }
    check("alt screen", model.alt_screen().find(&dfas))
}

/// Invariants the model's own code relies on. Each check names the code that
/// establishes it.
pub fn check_invariants(model: &TerminalModel) -> Result<(), String> {
    let block_list = model.block_list();
    let blocks = block_list.blocks();
    // `BlockList::active_block` is `blocks.last().expect("at least one block should exist")`.
    if blocks.is_empty() {
        return Err("block list has no blocks".to_owned());
    }
    // The block-height SumTree holds exactly one `Block` item per block: its
    // `block_count` dimension is used directly as a block index
    // (`BlockIndex::from(cursor.start().block_count)` in `BlockList`).
    let summary = block_list.block_heights().summary();
    if summary.block_count != blocks.len() {
        return Err(format!(
            "block_heights has {} Block items but the block list has {} blocks",
            summary.block_count,
            blocks.len()
        ));
    }
    let total = summary.height.as_f64();
    if !total.is_finite() || total < -1e-6 {
        return Err(format!("block_heights total height is {total}"));
    }
    let scope = block_list.transcript_scope();
    for (i, block) in blocks.iter().enumerate() {
        let height = block.height(scope).as_f64();
        if !height.is_finite() || height < -1e-6 {
            return Err(format!("block {i} has height {height}"));
        }
        check_grid(
            &format!("block {i} output grid"),
            block.output_grid().grid_handler(),
            false,
        )?;
    }
    check_grid("alt screen grid", model.alt_screen().grid_handler(), true)
}

fn check_grid(label: &str, grid: &GridHandler, region_within_rows: bool) -> Result<(), String> {
    let total_rows = grid.total_rows();
    let visible_rows = grid.visible_rows();
    let columns = grid.columns();
    // `InitialCursorState::new` (grid/resize.rs) treats a cursor outside the
    // grid as a bug ("cursor should not be outside the bounds of the grid!")
    // and every cursor writer (`goto`, `input`, ...) clamps to the visible
    // region. (Not checked on prompt grids: rendering truncates their columns
    // to the cursor on purpose.)
    let cursor = grid.cursor_point();
    if cursor.row >= total_rows || cursor.col >= columns {
        return Err(format!(
            "{label}: cursor {cursor:?} outside grid ({total_rows} rows, {visible_rows} visible, {columns} columns)"
        ));
    }
    // `set_scrolling_region` rejects top >= bottom and clamps both ends to
    // the visible rows; resize and reset re-derive it from the visible rows.
    // Only the alt screen's visible rows never shrink outside a resize (a
    // finished block grid truncates rows without touching the region).
    let region = grid.scroll_region();
    if region.start > region.end || (region_within_rows && region.end.0 > visible_rows) {
        return Err(format!(
            "{label}: scroll region {:?}..{:?} invalid for {visible_rows} visible rows",
            region.start, region.end
        ));
    }
    // `GridHandler::row` documents that it returns a row for every index
    // within the bounds of the grid; this also exercises flat-storage row
    // reconstruction for every row.
    for row in 0..total_rows {
        if grid.row(row).is_none() {
            return Err(format!(
                "{label}: row {row} of {total_rows} could not be read"
            ));
        }
    }
    Ok(())
}

/// Decodes one libFuzzer input into terminal operations and runs them,
/// panicking on an invariant violation.
///
/// Layout: byte 0 selects the feature flags, bytes 1-2 the initial size
/// (255 means 300 rows / 1000 columns), then a sequence of records, each
/// starting with a control byte:
///
/// | control | record |
/// |---|---|
/// | `0x00..=0xDF` | feed the next `control + 1` bytes as one pty read |
/// | `0xE0` | feed the next `u16` (LE) bytes as one pty read |
/// | `0xE1` | resize: rows byte, cols byte |
/// | `0xE2` | finish sync output |
/// | `0xE3` | reset the parser |
/// | `0xE4` | start command |
/// | `0xE5` | preexec: length byte, command bytes |
/// | `0xE6` / `0xE7` | command finished / precmd: exit byte (signed), block-id byte |
/// | `0xE8` | prompt-only precmd |
/// | `0xE9` | clear hook |
/// | `0xEA` | clear visible screen |
/// | `0xEB` | find: flags byte, length byte, query bytes |
/// | `0xEC` | check invariants |
/// | `0xED` | feed a framed shell hook: kind byte, framing byte, exit byte, block-id byte |
/// | `0xEE..=0xFF` | feed the whole remainder as one pty read |
pub fn run_fuzz_input(data: &[u8]) {
    let mut input = Input { data, pos: 0 };
    let flag_byte = input.byte();
    let flags = FuzzFlags {
        kitty_images: flag_byte & 1 != 0,
        osc_hyperlinks: flag_byte & 2 != 0,
        tmux_control_mode: flag_byte & 0x0c == 0x0c,
    };
    let mut terminal = FuzzTerminal::new(flags);
    let rows = size_from_byte(input.byte(), 300);
    let cols = size_from_byte(input.byte(), 1000);
    terminal.resize(rows, cols);

    while !input.is_empty() {
        let control = input.byte();
        match control {
            0x00..=0xDF => terminal.feed(input.take(control as usize + 1)),
            0xE0 => {
                let len = u16::from_le_bytes([input.byte(), input.byte()]) as usize;
                terminal.feed(input.take(len));
            }
            0xE1 => {
                let rows = size_from_byte(input.byte(), 300);
                let cols = size_from_byte(input.byte(), 1000);
                terminal.resize(rows, cols);
            }
            0xE2 => terminal.finish_sync_output(),
            0xE3 => terminal.reset_parser(),
            0xE4 => terminal.start_command(),
            0xE5 => {
                let len = input.byte() as usize;
                let command = String::from_utf8_lossy(input.take(len)).into_owned();
                terminal.preexec(&command);
            }
            0xE6 | 0xE7 => {
                let exit = input.byte() as i8 as i32;
                let block_id = format!("lf-{}", input.byte());
                if control == 0xE6 {
                    terminal.command_finished(exit, &block_id);
                } else {
                    terminal.precmd(exit, &block_id);
                }
            }
            0xE8 => terminal.prompt_only_precmd(),
            0xE9 => terminal.clear_hook(),
            0xEA => terminal.clear_visible_screen(),
            0xEB => {
                let find_flags = input.byte();
                let len = input.byte() as usize;
                let query = String::from_utf8_lossy(input.take(len)).into_owned();
                if let Err(e) = terminal.find(&query, find_flags & 1 != 0, find_flags & 2 != 0) {
                    panic!("invariant violated: {e}");
                }
            }
            0xEC => {
                if let Err(e) = terminal.check_invariants() {
                    panic!("invariant violated: {e}");
                }
            }
            0xED => {
                let kind = input.byte();
                let framing = input.byte();
                let exit = input.byte() as i8 as i32;
                let block_id = format!("lf-{}", input.byte());
                let json = hook_json(kind, exit, &block_id);
                let bytes = match framing % 3 {
                    0 => dcs_hook(json.as_bytes()),
                    1 => osc_hook(json.as_bytes()),
                    _ => unencoded_dcs_hook(json.as_bytes()),
                };
                terminal.feed(&bytes);
            }
            _ => {
                let rest = input.take(usize::MAX);
                terminal.feed(rest);
            }
        }
    }
    terminal.finish_sync_output();
    if let Err(e) = terminal.check_invariants() {
        panic!("invariant violated at end of input: {e}");
    }
}

/// A valid hook payload; corruption is left to the fuzzer, which can feed any
/// bytes directly.
fn hook_json(kind: u8, exit: i32, block_id: &str) -> String {
    let session_id = SESSION_ID;
    match kind % 6 {
        0 => format!(
            r#"{{"hook":"Precmd","value":{{"exit_code":{exit},"next_block_id":"{block_id}","pwd":"/tmp","session_id":{session_id}}}}}"#
        ),
        1 => format!(r#"{{"hook":"Precmd","value":{{"pwd":"/tmp","session_id":{session_id}}}}}"#),
        2 => {
            format!(r#"{{"hook":"Preexec","value":{{"command":"ls","session_id":{session_id}}}}}"#)
        }
        3 => format!(
            r#"{{"hook":"CommandFinished","value":{{"exit_code":{exit},"next_block_id":"{block_id}","session_id":{session_id}}}}}"#
        ),
        4 => format!(
            r#"{{"hook":"InitShell","value":{{"session_id":{session_id},"shell":"zsh","user":"u","hostname":"h"}}}}"#
        ),
        _ => format!(r#"{{"hook":"Clear","value":{{"session_id":{session_id}}}}}"#),
    }
}

fn size_from_byte(byte: u8, max: usize) -> usize {
    if byte == u8::MAX {
        max
    } else {
        byte as usize % 200 + 1
    }
}

struct Input<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Input<'a> {
    fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// The next byte, or 0 once the input is exhausted.
    fn byte(&mut self) -> u8 {
        let byte = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos = self.pos.saturating_add(1);
        byte
    }

    /// Up to `len` bytes.
    fn take(&mut self, len: usize) -> &'a [u8] {
        let start = self.pos.min(self.data.len());
        let end = start.saturating_add(len).min(self.data.len());
        self.pos = end;
        &self.data[start..end]
    }
}
