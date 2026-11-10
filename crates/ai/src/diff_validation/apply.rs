//! Applies resolved [`DiffDelta`]s to the raw text of a file (#1925, #1954, #1955, #1956).
//!
//! The one rule: an applied edit changes the edited lines and nothing else. Every line outside a
//! delta's range keeps its bytes, line ending included, so a CRLF file stays CRLF, a file that
//! mixes endings keeps each line's own, and a file with no final newline still has none.
//!
//! This is the byte-exact reference for what an accepted edit writes. The TUI writes its result
//! directly; the GUI shows the edit in an editor buffer (which stores text LF-normalised and can
//! only write one line ending) and writes this result whenever it is the same text as the buffer.
//! When it is not (the user changed the proposal before accepting it), the buffer's text is
//! written with the original's line endings put back line by line (`endings.rs`).

use super::DiffDelta;
use super::endings::text_with_line_endings_of;

/// The line ending `line` carries: `"\r\n"`, `"\n"`, or `""` for an unterminated last line.
fn terminator(line: &str) -> &'static str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

/// The line ending the lines a delta inserts are written with: that of the last line it replaces,
/// else of the line before it, else of the line after it, else of the first terminated line of
/// the file, else LF.
fn ending_for_insertion(lines: &[String], start: usize, end: usize) -> &'static str {
    let replaced = lines[start..end].last();
    let before = lines[..start].last();
    let after = lines.get(end);
    [replaced, before, after]
        .into_iter()
        .flatten()
        .chain(lines.iter())
        .map(|line| terminator(line))
        .find(|ending| !ending.is_empty())
        .unwrap_or("\n")
}

/// Applies line-range replacement deltas to `content` and returns the new text.
///
/// `replacement_line_range` is 1-indexed and end-exclusive, as the matchers in this module
/// produce it: `2..3` replaces line 2, `3..3` inserts before line 3, `n+1..n+1` appends after the
/// last of `n` lines, and `0..0` (like `1..1`) prepends. Lines are split on `\n` only, the way
/// `str::lines` numbers them, so a lone `\r` is ordinary text.
///
/// What is written for a delta:
/// - Each inserted line gets the line ending of the lines it replaces or sits next to
///   ([`ending_for_insertion`]); a line break inside the insertion (`\n` or `\r\n`) is rewritten
///   to it. An insertion that does not end in a line break is still a whole line.
/// - The exception is the end of a file whose last line has no line ending: an insertion that
///   reaches it and does not itself end in a line break leaves the new last line unterminated
///   too, so the file's final-newline state is kept. An insertion that does end in a line break
///   is taken at its word.
/// - Appending after such a last line gives that line the file's line ending first: the appended
///   text is a new line, not a continuation of the old one.
/// - An empty file takes the insertion literally.
///
/// Deltas are applied from the last range to the first. `Err` names a range outside the file.
pub fn apply_deltas_to_text(content: &str, deltas: &[DiffDelta]) -> Result<String, String> {
    let mut lines: Vec<String> = content.split_inclusive('\n').map(str::to_owned).collect();
    let mut deltas = deltas.to_vec();
    deltas.sort_by_key(|delta| delta.replacement_line_range.start);

    for delta in deltas.into_iter().rev() {
        let start = delta.replacement_line_range.start.saturating_sub(1);
        let end = delta.replacement_line_range.end.saturating_sub(1);
        if start > lines.len() || end > lines.len() || start > end {
            return Err(format!(
                "Diff range {:?} is out of bounds for file with {} lines",
                delta.replacement_line_range,
                lines.len()
            ));
        }

        if lines.is_empty() {
            lines = delta
                .insertion
                .split_inclusive('\n')
                .map(str::to_owned)
                .collect();
            continue;
        }

        let ending = ending_for_insertion(&lines, start, end);
        let unterminated_end =
            end == lines.len() && lines.last().is_some_and(|line| !line.ends_with('\n'));

        let mut replacement: Vec<String> = Vec::new();
        let mut pieces = delta.insertion.split_inclusive('\n').peekable();
        while let Some(piece) = pieces.next() {
            let piece_ending = terminator(piece);
            let text = &piece[..piece.len() - piece_ending.len()];
            let is_last = pieces.peek().is_none();
            let keep_unterminated = is_last && piece_ending.is_empty() && unterminated_end;
            let mut line = String::with_capacity(text.len() + ending.len());
            line.push_str(text);
            if !keep_unterminated {
                line.push_str(ending);
            }
            replacement.push(line);
        }

        if start == lines.len()
            && unterminated_end
            && !replacement.is_empty()
            && let Some(last) = lines.last_mut()
        {
            last.push_str(ending);
        }

        lines.splice(start..end, replacement);
    }

    Ok(lines.concat())
}

/// `text` with every `\r\n` and lone `\r` turned into `\n`: the form an editor buffer holds.
pub fn lf_normalized(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The text an accepted update writes, given the editor's serialisation of it (`buffer_text`),
/// the file's raw text as the edit was proposed against it, and the proposed deltas.
///
/// The editor buffer can write only one line ending, so on a file that mixes endings its text
/// rewrites lines the edit never touched (#1956). The deltas applied to the raw original
/// ([`apply_deltas_to_text`]) do not. That result is written when it is the same text as the
/// buffer's, line endings aside. It is not the same text when the user changed the proposal in
/// the diff view (or the two ever disagree): then the buffer's text is what the user saw and
/// accepted, and it is what is written, with each line that is still the original's given back
/// the original's line ending ([`text_with_line_endings_of`]; a no-op unless the original mixes
/// line endings).
///
/// An original with no line break at all is left to the buffer too: it has no line ending to
/// preserve, and the buffer knows the session platform's default for the lines being added.
pub fn accepted_update_text(buffer_text: String, original: &str, deltas: &[DiffDelta]) -> String {
    if !original.contains('\n') {
        return buffer_text;
    }
    match apply_deltas_to_text(original, deltas) {
        Ok(exact) if lf_normalized(&exact) == lf_normalized(&buffer_text) => exact,
        _ => text_with_line_endings_of(original, buffer_text),
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
