//! Gives a text back the line endings of the file it is written over (#1956).
//!
//! An editor buffer holds text LF-normalised and serialises it with ONE line ending, the file's
//! majority one. For a file that mixes CRLF and LF lines that serialisation rewrites every
//! minority-ending line, which are bytes nobody edited. [`text_with_line_endings_of`] takes the
//! file's raw text as the disk last had it and the text about to be written, and re-ends the new
//! text line by line: a line that is in the original keeps the original's line ending, a changed
//! or new line takes the ending of the line it replaces or sits next to.
//!
//! A file with a single line ending is not touched at all: the buffer's serialisation already is
//! the exact result there.

use std::collections::HashMap;

use super::lf_normalized;

/// One line of raw text: its content and the line break that ends it (`"\r\n"`, `"\n"`, `"\r"`,
/// or `""` for an unterminated last line).
type RawLine<'a> = (&'a str, &'static str);

/// Above this many (original line, new line) pairs in the changed middle of a file the exact
/// alignment table (four bytes a pair) is not built and a greedy forward match is used instead.
const MAX_ALIGNMENT_CELLS: usize = 1 << 20;

/// Splits `text` into lines the way an editor buffer counts them: at `\r\n`, `\n` and a lone
/// `\r`. The concatenation of every content and ending is `text`.
fn raw_lines(text: &str) -> Vec<RawLine<'_>> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let ending = match bytes[index] {
            b'\n' => "\n",
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => "\r\n",
            b'\r' => "\r",
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push((&text[start..index], ending));
        index += ending.len();
        start = index;
    }
    if start < bytes.len() {
        lines.push((&text[start..], ""));
    }
    lines
}

/// Whether `text` ends its lines in more than one way (any two of `\r\n`, `\n`, lone `\r`). An
/// unterminated last line is not an ending of its own.
pub fn has_mixed_line_endings(text: &str) -> bool {
    let bytes = text.as_bytes();
    let (mut lf, mut cr, mut crlf) = (false, false, false);
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => lf = true,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf = true;
                index += 1;
            }
            b'\r' => cr = true,
            _ => {}
        }
        index += 1;
    }
    usize::from(lf) + usize::from(cr) + usize::from(crlf) > 1
}

/// For every line of `new`, the index of the line of `old` it is (same content, same order), or
/// `None` for a line `old` does not have. The matched indices strictly increase.
///
/// The common prefix and suffix are matched directly, which is the whole job for the usual edit.
/// The middle is a longest common subsequence when its table fits [`MAX_ALIGNMENT_CELLS`], else a
/// greedy forward match (each new line takes the next unused original line with its content): a
/// valid alignment that may leave a line unmatched an exact one would have matched.
fn align(old: &[&str], new: &[&str]) -> Vec<Option<usize>> {
    let mut matches = vec![None; new.len()];
    let shortest = old.len().min(new.len());
    let mut prefix = 0;
    while prefix < shortest && old[prefix] == new[prefix] {
        matches[prefix] = Some(prefix);
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < shortest - prefix && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix] {
        matches[new.len() - 1 - suffix] = Some(old.len() - 1 - suffix);
        suffix += 1;
    }
    let old_middle = &old[prefix..old.len() - suffix];
    let new_middle = &new[prefix..new.len() - suffix];
    if old_middle.is_empty() || new_middle.is_empty() {
        return matches;
    }

    let (rows, columns) = (old_middle.len(), new_middle.len());
    if rows.saturating_mul(columns) <= MAX_ALIGNMENT_CELLS {
        // lengths[i * width + j]: the longest common subsequence of old_middle[i..] and
        // new_middle[j..].
        let width = columns + 1;
        let mut lengths = vec![0u32; (rows + 1) * width];
        for i in (0..rows).rev() {
            for j in (0..columns).rev() {
                lengths[i * width + j] = if old_middle[i] == new_middle[j] {
                    lengths[(i + 1) * width + j + 1] + 1
                } else {
                    lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < rows && j < columns {
            if old_middle[i] == new_middle[j] {
                matches[prefix + j] = Some(prefix + i);
                i += 1;
                j += 1;
            } else if lengths[(i + 1) * width + j] >= lengths[i * width + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    } else {
        let mut positions: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, line) in old_middle.iter().enumerate() {
            positions.entry(*line).or_default().push(i);
        }
        let mut next_unused = 0;
        for (j, line) in new_middle.iter().enumerate() {
            let Some(candidates) = positions.get(line) else {
                continue;
            };
            let at = candidates.partition_point(|&i| i < next_unused);
            if let Some(&i) = candidates.get(at) {
                matches[prefix + j] = Some(prefix + i);
                next_unused = i + 1;
            }
        }
    }
    matches
}

/// The line ending line `index` of `lines` lends to a line of the new text. Only the last line
/// can be unterminated; it lends the ending of the line before it, else `default`.
fn ending_to_lend(lines: &[RawLine<'_>], index: usize, default: &'static str) -> &'static str {
    match lines[index].1 {
        "" => index
            .checked_sub(1)
            .map_or(default, |before| lines[before].1),
        ending => ending,
    }
}

/// `text` with the line endings of `original`, the raw text of the file it is written over.
///
/// When `original` mixes line endings, every line of `text` that is also a line of `original`
/// (aligned by content, in order) is written with the ending that line has in `original`. A line
/// `original` does not have takes the ending of the original line at its position in the run of
/// lines it replaces, of the last replaced line when the run is longer than what it replaces,
/// and for a pure insertion of the original line before it, else after it. Whether the text ends
/// in a line break is the text's own choice and is kept.
///
/// `text` is returned as it is when `original` has a single line ending (or none): the caller's
/// serialisation is already exact there. It is also returned as it is if the result would not be
/// the same text, line endings aside, so this function can change line endings and nothing else.
pub fn text_with_line_endings_of(original: &str, text: String) -> String {
    if !has_mixed_line_endings(original) {
        return text;
    }
    let old = raw_lines(original);
    let Some(default) = old
        .iter()
        .map(|&(_, ending)| ending)
        .find(|ending| !ending.is_empty())
    else {
        return text;
    };
    let normalized = lf_normalized(&text);
    let ends_in_line_break = normalized.ends_with('\n');
    let new: Vec<&str> = normalized.split_terminator('\n').collect();
    let old_contents: Vec<&str> = old.iter().map(|&(content, _)| content).collect();
    let matches = align(&old_contents, &new);

    // For each new line, the original index of the next matched line at or after it.
    let mut next_matched = vec![old.len(); new.len()];
    let mut next = old.len();
    for (j, matched) in matches.iter().enumerate().rev() {
        if let Some(i) = matched {
            next = *i;
        }
        next_matched[j] = next;
    }

    let mut result = String::with_capacity(text.len() + new.len());
    let mut last_matched: Option<usize> = None;
    let mut run = 0;
    for (j, line) in new.iter().enumerate() {
        let source = match matches[j] {
            Some(i) => {
                last_matched = Some(i);
                run = 0;
                Some(i)
            }
            None => {
                // The original lines this run of new lines replaces: after the last matched
                // line, before the next matched one.
                let replaced_start = last_matched.map_or(0, |i| i + 1);
                let replaced_end = next_matched[j];
                let positional = replaced_start + run;
                run += 1;
                if positional < replaced_end {
                    Some(positional)
                } else if replaced_end > replaced_start {
                    Some(replaced_end - 1)
                } else if last_matched.is_some() {
                    last_matched
                } else if replaced_end < old.len() {
                    Some(replaced_end)
                } else {
                    None
                }
            }
        };
        result.push_str(line);
        if j + 1 == new.len() && !ends_in_line_break {
            break;
        }
        result.push_str(source.map_or(default, |i| ending_to_lend(&old, i, default)));
    }

    if lf_normalized(&result) == normalized {
        result
    } else {
        text
    }
}

#[cfg(test)]
#[path = "endings_tests.rs"]
mod tests;
