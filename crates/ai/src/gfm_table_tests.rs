use super::format_gfm_table;

#[test]
fn format_gfm_table_normalizes_column_widths() {
    let lines = vec![
        "| Short | Medium Length | This is much longer |".to_owned(),
        "| --- | --- | --- |".to_owned(),
        "| A | Hello world | X |".to_owned(),
    ];
    let result = format_gfm_table(&lines);
    let result_lines: Vec<&str> = result.lines().collect();

    assert_eq!(result_lines.len(), 3);
    // All rows should have the same length due to padding
    assert_eq!(result_lines[0].len(), result_lines[1].len());
    assert_eq!(result_lines[1].len(), result_lines[2].len());
    // Check content is preserved
    assert!(result_lines[0].contains("Short"));
    assert!(result_lines[0].contains("Medium Length"));
    assert!(result_lines[0].contains("This is much longer"));
    assert!(result_lines[2].contains("A"));
    assert!(result_lines[2].contains("Hello world"));
}

#[test]
fn format_gfm_table_preserves_alignment_markers() {
    let lines = vec![
        "| Left | Center | Right |".to_owned(),
        "| :--- | :---: | ---: |".to_owned(),
        "| A | B | C |".to_owned(),
    ];
    let result = format_gfm_table(&lines);
    let sep_line = result.lines().nth(1).unwrap();

    // Extract separator cells (trim leading/trailing pipes and split)
    let cells: Vec<&str> = sep_line
        .trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim())
        .collect();

    assert_eq!(cells.len(), 3);
    // Left alignment: starts with dashes (no leading colon after trimming)
    assert!(
        cells[0].starts_with('-'),
        "Left column should be left-aligned"
    );
    // Center alignment: starts and ends with colon
    assert!(
        cells[1].starts_with(':') && cells[1].ends_with(':'),
        "Center column should be center-aligned"
    );
    // Right alignment: ends with colon but doesn't start with one
    assert!(
        !cells[2].starts_with(':') && cells[2].ends_with(':'),
        "Right column should be right-aligned"
    );
}

#[test]
fn format_gfm_table_handles_rows_with_fewer_columns() {
    let lines = vec![
        "| A | B | C |".to_owned(),
        "| --- | --- | --- |".to_owned(),
        "| X |".to_owned(), // Missing columns
    ];
    let result = format_gfm_table(&lines);
    let result_lines: Vec<&str> = result.lines().collect();

    // Should still produce valid output
    assert_eq!(result_lines.len(), 3);
    // Last row should be padded to have same structure
    assert_eq!(result_lines[0].len(), result_lines[2].len());
}

#[test]
fn format_gfm_table_handles_empty_cells() {
    let lines = vec![
        "| A | | C |".to_owned(),
        "| --- | --- | --- |".to_owned(),
        "| | B | |".to_owned(),
    ];
    let result = format_gfm_table(&lines);

    // Should produce aligned output with empty cells preserved
    assert!(result.contains("| A"));
    assert!(result.contains("| B"));
    assert!(result.contains("| C"));
}

// A pipe inside an inline code span is cell content, not a column delimiter.

/// The verbatim row from a 2026-08-15 `gpt-oss:20b` answer, which rendered as a lone boxed
/// header followed by every data row as raw `| … |` prose. The row carries two unescaped pipes
/// inside one code span, so the old splitter counted 5 cells against a 3-cell header and
/// `maybe_collect_gfm_table_lines` ended the table on it.
const SCREENSHOT_ROW: &str = "| **Verify where the VM's disk lives** | `virsh dumpxml HA | grep -E '(source|node-name)'` | Shows the exact path. |";

#[test]
fn pipes_inside_a_code_span_do_not_split_cells() {
    let cells = super::split_cells_escaped(SCREENSHOT_ROW);
    assert_eq!(
        cells.len(),
        3,
        "the code span holds the row to the header's 3 columns: {cells:?}"
    );
    assert_eq!(
        cells[1], "`virsh dumpxml HA | grep -E '(source|node-name)'`",
        "the command must survive intact in one cell"
    );
}

#[test]
fn a_row_with_a_code_span_pipe_no_longer_ends_the_table() {
    let lines = [
        "| --- | --- | --- |",
        SCREENSHOT_ROW,
        "| **Check host disk usage** | `df -h` | Look for the filesystem. |",
    ];
    let mut rest = lines.into_iter().peekable();
    let collected = super::maybe_collect_gfm_table_lines(
        "| Step | Command / Tool | Why |",
        &mut rest,
        |_| false,
    )
    .expect("header + separator form a table");

    assert_eq!(
        collected.len(),
        4,
        "header, separator and BOTH data rows: {collected:?}"
    );
    assert_eq!(rest.next(), None, "every row was consumed by the table");
}

#[test]
fn an_unterminated_backtick_falls_back_to_the_naive_split() {
    // One stray backtick is not a code span; without the fallback it would swallow every
    // remaining delimiter and collapse the row to a single cell.
    let cells = super::split_cells_escaped("| a ` b | c | d |");
    assert_eq!(cells, vec!["a ` b", "c", "d"]);
}

#[test]
fn a_line_that_is_not_pipe_delimited_still_ends_the_table() {
    let lines = ["| --- | --- |", "| 1 | 2 |", "Ordinary prose after the table"];
    let mut rest = lines.into_iter().peekable();
    let collected = super::maybe_collect_gfm_table_lines("| A | B |", &mut rest, |_| false)
        .expect("header + separator form a table");

    assert_eq!(collected.len(), 3, "header, separator, one row: {collected:?}");
    assert_eq!(
        rest.next(),
        Some("Ordinary prose after the table"),
        "the prose line must be left for the caller, not swallowed"
    );
}
