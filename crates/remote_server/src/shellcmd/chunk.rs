//! The line/byte chunker for codebase indexing, shared by the helper's scan (`daemon::chunk`) and
//! the app's no-helper path (Wave H parity, P9a). Pure `std`: no I/O, no dependencies.
//!
//! It is the app's *naive* chunker (`crates/ai/.../chunker/naive.rs`) ported rule for rule, so a
//! fragment here has the same `start_line`/`end_line` (0-based, inclusive) and byte range
//! (`byte_end` exclusive) the app would persist in its `FragmentMetadata`:
//!
//! * lines come from [`str::lines`] (`\n` and `\r\n` end a line, the terminator is not part of the
//!   line span);
//! * every [`LINES_PER_CHUNK`] lines form a chunk, spanning the first line's start to the last
//!   line's end;
//! * a chunk longer than [`MAX_BYTES_PER_CHUNK`] is cut line by line, a line longer than that is
//!   cut by bytes (backing up to a UTF-8 boundary, or forward when no boundary lies behind), and
//!   the pieces are re-joined from the END of the chunk while they still fit.
//!
//! What it does NOT do: the app first tries a tree-sitter ("semantic") split for files whose
//! language it recognises and only falls back to this chunker otherwise. The helper cannot carry
//! the grammars, so for recognised languages its boundaries (and so its hashes) differ from the
//! app's local index. They are consistent between the helper and the shell path, which is what a
//! remote index needs.

use std::ops::Range;

/// Lines per chunk (the app's `LINES_PER_CHUNK`).
pub const LINES_PER_CHUNK: usize = 200;
/// Average characters per line (the app's `AVG_CHAR_PER_LINE`).
pub const AVG_CHAR_PER_LINE: usize = 60;
/// Byte cap of one fragment (the app's `MAX_BYTES_PER_CHUNK`).
pub const MAX_BYTES_PER_CHUNK: usize = LINES_PER_CHUNK * AVG_CHAR_PER_LINE;
/// Files of this size or more are not indexed (the app's `repo_metadata` `MAX_FILE_SIZE`; it
/// accepts `len < 3_000_000`).
pub const MAX_FILE_BYTES: u64 = 3 * 1000 * 1000;

/// Where one fragment lies in its file. Lines are 0-based and inclusive; the byte range is
/// half-open. The same facts as the app's `FragmentMetadata` minus the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}

impl Span {
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_start..self.byte_end
    }
}

/// Chunks `code` with the app's constants.
pub fn chunk_text(code: &str) -> Vec<Span> {
    chunk_with(code, MAX_BYTES_PER_CHUNK, LINES_PER_CHUNK)
}

/// Chunks `code` into spans of at most `lines_per_chunk` lines and `max_bytes` bytes (a single
/// character wider than `max_bytes` is the one exception).
pub fn chunk_with(code: &str, max_bytes: usize, lines_per_chunk: usize) -> Vec<Span> {
    let base = code.as_ptr() as usize;
    let lines: Vec<(usize, Range<usize>)> = code
        .lines()
        .enumerate()
        .map(|(n, line)| {
            let start = line.as_ptr() as usize - base;
            (n, start..start + line.len())
        })
        .collect();

    let mut out = Vec::new();
    for chunk in lines.chunks(lines_per_chunk.max(1)) {
        let (start_line, first) = &chunk[0];
        let (end_line, last) = &chunk[chunk.len() - 1];
        if last.end - first.start > max_bytes {
            let pieces = chunk
                .iter()
                .flat_map(|(n, r)| chunk_line_by_bytes(code, max_bytes, *n, r.clone()))
                .collect::<Vec<_>>();
            out.extend(coalesce(pieces, max_bytes));
        } else {
            out.push(Span {
                start_line: *start_line,
                end_line: *end_line,
                byte_start: first.start,
                byte_end: last.end,
            });
        }
    }
    out
}

fn chunk_line_by_bytes(code: &str, max_bytes: usize, line: usize, r: Range<usize>) -> Vec<Span> {
    let (line_start, line_end) = (r.start, r.end);
    if line_end - line_start <= max_bytes {
        return vec![Span {
            start_line: line,
            end_line: line,
            byte_start: line_start,
            byte_end: line_end,
        }];
    }
    let mut out = Vec::new();
    let mut current = line_start;
    while current < line_end {
        let size = (line_end - current).min(max_bytes);
        let mut end = current + size;
        while end > current && !code.is_char_boundary(end) {
            end -= 1;
        }
        if end <= current {
            end = current + size;
            while end < line_end && !code.is_char_boundary(end) {
                end += 1;
            }
        }
        out.push(Span {
            start_line: line,
            end_line: line,
            byte_start: current,
            byte_end: end,
        });
        current = end;
    }
    out
}

/// Joins neighbouring spans from the end backwards while the joined span fits `max_bytes`.
fn coalesce(spans: Vec<Span>, max_bytes: usize) -> Vec<Span> {
    let mut acc: Vec<Span> = Vec::new();
    for mut span in spans.into_iter().rev() {
        match acc.last_mut() {
            Some(last) if last.byte_end - span.byte_start <= max_bytes => {
                span.end_line = last.end_line;
                span.byte_end = last.byte_end;
                *last = span;
            }
            _ => acc.push(span),
        }
    }
    acc.reverse();
    acc
}

#[cfg(test)]
#[path = "chunk_tests.rs"]
mod tests;
