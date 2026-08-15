use std::iter::Peekable;

use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
enum ColumnAlignment {
    Left,
    Center,
    Right,
}

/// Split a table row into cells, handling escaped pipes (`\|`) as literal pipe characters and
/// treating a pipe inside an inline code span as content rather than a column delimiter.
///
/// The code-span rule is a deliberate deviation from GFM, which requires `\|` even inside
/// backticks and would split `` `a | b` `` into two cells. Models do not escape them: observed
/// 2026-08-15 from `gpt-oss:20b`, a row reading
/// `` | **Verify the disk** | `virsh dumpxml HA | grep -E '(source|node-name)'` | … | ``
/// counted 5 cells against a 3-cell header. Because [`maybe_collect_gfm_table_lines`] ends the
/// table at the first row whose cell count disagrees with the header, that single row did not
/// merely render oddly — it terminated the table, and every data row after it fell through to
/// the caller as raw `| … |` prose under a lone header. Honouring the code span keeps the
/// command in one cell, which is also what the author meant.
///
/// An unterminated backtick run is not a code span, so the naive split is used for that line;
/// otherwise a single stray backtick would swallow every remaining delimiter on the row.
fn split_cells_escaped(line: &str) -> Vec<String> {
    let trimmed = line.trim().trim_matches('|');
    match split_cells_code_span_aware(trimmed) {
        Some(cells) => cells,
        None => split_cells_naive(trimmed),
    }
}

/// Splits on every unescaped `|`, with no notion of inline code spans.
fn split_cells_naive(trimmed: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current_cell = String::new();
    let mut chars = trimmed.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            current_cell.push('|');
            chars.next();
        } else if c == '|' {
            cells.push(current_cell.trim().to_string());
            current_cell = String::new();
        } else {
            current_cell.push(c);
        }
    }
    cells.push(current_cell.trim().to_string());
    cells
}

/// Splits on unescaped `|` outside inline code spans. Returns `None` when a backtick run is
/// never closed, since there is then no code span and the caller should fall back.
fn split_cells_code_span_aware(trimmed: &str) -> Option<Vec<String>> {
    let mut cells = Vec::new();
    let mut current_cell = String::new();
    let mut chars = trimmed.chars().peekable();
    // Length of the backtick run that opened the span currently being scanned, if any. A span
    // is closed only by a run of exactly the same length, per CommonMark's code-span rule.
    let mut open_run: Option<usize> = None;

    while let Some(c) = chars.next() {
        if c == '`' {
            let mut run = 1;
            while chars.peek() == Some(&'`') {
                chars.next();
                run += 1;
            }
            match open_run {
                Some(open) if open == run => open_run = None,
                None => open_run = Some(run),
                // A run of a different length inside a span is ordinary content.
                Some(_) => {}
            }
            for _ in 0..run {
                current_cell.push('`');
            }
        } else if c == '\\' && chars.peek() == Some(&'|') {
            current_cell.push('|');
            chars.next();
        } else if c == '|' && open_run.is_none() {
            cells.push(current_cell.trim().to_string());
            current_cell = String::new();
        } else {
            current_cell.push(c);
        }
    }

    if open_run.is_some() {
        return None;
    }
    cells.push(current_cell.trim().to_string());
    Some(cells)
}

/// Returns true if the line looks like a GFM pipe-table separator row,
/// e.g. `| --- | ---: | :---: |`.
fn is_gfm_table_separator_row(row: &str) -> bool {
    let trimmed = row.trim();
    if trimmed.is_empty() || !trimmed.contains('|') {
        return false;
    }

    let mut contains_separator_cell = false;
    // Split row into individual cells.
    for cell in trimmed.split('|').map(|c| c.trim()) {
        if cell.is_empty() {
            continue;
        }

        // `:` are used to indicate a column's horizontal alignment (e.g. `:--:` for center).
        let dashes = cell.trim_matches(':').trim();
        if dashes.is_empty() {
            return false;
        }
        if !dashes.chars().all(|c| c == '-') {
            return false;
        }
        contains_separator_cell = true;
    }
    contains_separator_cell
}

/// Attempts to parse a GFM table starting from `header_line`.
///
/// If the next line in `lines` is a valid GFM separator row, this consumes all
/// subsequent table rows and returns the raw table lines.
/// The `should_stop` predicate is called on each candidate row to allow the caller
/// to halt parsing early (e.g., when encountering a fenced code block).
///
/// Returns `None` if `header_line` and the next line don't form a valid table start.
pub fn maybe_collect_gfm_table_lines<'a, I>(
    header_line: &str,
    lines: &mut Peekable<I>,
    should_stop: impl Fn(&str) -> bool,
) -> Option<Vec<String>>
where
    I: Iterator<Item = &'a str>,
{
    let header_trimmed = header_line.trim();
    let has_leading_or_trailing_pipe =
        header_trimmed.starts_with('|') || header_trimmed.ends_with('|');
    let has_at_least_two_pipes = header_trimmed.matches('|').count() >= 2;
    if !has_leading_or_trailing_pipe || !has_at_least_two_pipes {
        return None;
    }

    let separator = lines
        .next_if(|line| is_gfm_table_separator_row(line))?
        .to_owned();

    let header_column_count = split_cells_escaped(header_trimmed).len();
    let separator_column_count = split_cells_escaped(&separator).len();
    if header_column_count != separator_column_count {
        return None;
    }

    let mut table_lines = vec![header_line.to_owned(), separator];

    while let Some(next_line) = lines.peek() {
        let is_blank = next_line.trim().is_empty();
        let is_end_of_section = should_stop(next_line);
        // GFM does NOT end a table at a row whose cell count differs from the header's: "if a
        // row has fewer cells than the header, empty cells are inserted; if more, the excess is
        // ignored." A count mismatch is therefore only usable as an end-of-table signal for a
        // line that does not otherwise look like a row at all. A pipe-delimited line is still a
        // row, however its cells land — ending the table there is what turned the rest of an
        // agent's table into raw prose, and code-span-aware splitting alone would not have
        // saved a row that mismatched for some other reason.
        let trimmed = next_line.trim();
        let looks_like_row = trimmed.starts_with('|') || trimmed.ends_with('|');
        let count_ends_table =
            !looks_like_row && split_cells_escaped(next_line).len() != header_column_count;
        if is_blank || is_end_of_section || count_ends_table {
            break;
        }
        table_lines.push(lines.next().expect("peeked line must exist").to_owned());
    }

    Some(table_lines)
}

/// Attempts to parse a GFM table starting from `header_line`.
///
/// If the next line in `lines` is a valid GFM separator row, this consumes all
/// subsequent table rows and returns the formatted table as a `String`.
/// The `should_stop` predicate is called on each candidate row to allow the caller
/// to halt parsing early (e.g., when encountering a fenced code block).
///
/// Returns `None` if `header_line` and the next line don't form a valid table start.
pub fn maybe_parse_gfm_table<'a, I>(
    header_line: &str,
    lines: &mut Peekable<I>,
    should_stop: impl Fn(&str) -> bool,
) -> Option<String>
where
    I: Iterator<Item = &'a str>,
{
    maybe_collect_gfm_table_lines(header_line, lines, should_stop)
        .map(|table_lines| format_gfm_table(&table_lines))
}

/// Formats a GFM table with normalized column widths.
pub fn format_gfm_table(rows: &[String]) -> String {
    // A valid GFM table must consist of at least two rows
    // (a header row and a separator row).
    if rows.len() < 2 {
        return rows.join("\n");
    }

    // Parse all rows into cells, handling leading/trailing pipes
    let parsed_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let trimmed = row.trim();
            if trimmed.is_empty() {
                return vec![];
            }

            // Split into cells, handling escaped pipes.
            split_cells_escaped(trimmed)
        })
        .collect();

    let num_columns = parsed_rows.first().map_or(0, |r| r.len());
    if num_columns == 0 {
        return rows.join("\n");
    }

    // Calculate max display width for each column
    // (3 is the minimum width for the separator row).
    let mut column_widths = vec![3usize; num_columns];
    for (row_idx, row) in parsed_rows.iter().enumerate() {
        if row_idx == 1 {
            continue;
        }
        for (col_idx, cell) in row.iter().enumerate() {
            if col_idx < num_columns {
                column_widths[col_idx] = column_widths[col_idx].max(cell.width());
            }
        }
    }

    // Parse alignments from separator row
    let alignments: Vec<ColumnAlignment> = parsed_rows
        .get(1)
        .map(|sep_row| {
            (0..num_columns)
                .map(|i| {
                    sep_row.get(i).map_or(ColumnAlignment::Left, |cell| {
                        let cell = cell.trim();
                        match (cell.starts_with(':'), cell.ends_with(':')) {
                            // :---: => center aligned
                            (true, true) => ColumnAlignment::Center,
                            // ---: => right aligned
                            (false, true) => ColumnAlignment::Right,
                            // :--- or --- => left aligned
                            _ => ColumnAlignment::Left,
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_else(|| vec![ColumnAlignment::Left; num_columns]);

    // Build formatted rows
    let mut result = Vec::with_capacity(rows.len());
    for (row_idx, row) in parsed_rows.iter().enumerate() {
        let formatted_cells: Vec<String> = (0..num_columns)
            .map(|col_idx| {
                let width = column_widths[col_idx];
                let alignment = alignments[col_idx];
                if row_idx == 1 {
                    // Use format padding to generate repeated dashes for the separator rows
                    // (e.g. `{:-<width$}` pads "-" on the right with `-` chars to reach `width`).
                    match alignment {
                        ColumnAlignment::Left => format!("{:-<width$}", "-"),
                        ColumnAlignment::Right => {
                            let dashes = width.saturating_sub(1);
                            format!("{:-<dashes$}:", "-")
                        }
                        ColumnAlignment::Center => {
                            let dashes = width.saturating_sub(2);
                            format!(":{:-<dashes$}:", "-")
                        }
                    }
                } else {
                    // Data row: pad manually since format! doesn't account for
                    // Unicode display width (e.g. emojis are wider than 1 char).
                    let cell = row.get(col_idx).map_or("", |s| s.as_str());
                    let display_width = cell.width();
                    let padding = width.saturating_sub(display_width);
                    match alignment {
                        ColumnAlignment::Left => format!("{cell}{:padding$}", ""),
                        ColumnAlignment::Right => format!("{:padding$}{cell}", ""),
                        ColumnAlignment::Center => {
                            let left_pad = padding / 2;
                            let right_pad = padding - left_pad;
                            format!("{:left_pad$}{cell}{:right_pad$}", "", "")
                        }
                    }
                }
            })
            .collect();
        result.push(format!("| {} |", formatted_cells.join(" | ")));
    }

    result.join("\n")
}

#[cfg(test)]
#[path = "gfm_table_tests.rs"]
mod tests;
