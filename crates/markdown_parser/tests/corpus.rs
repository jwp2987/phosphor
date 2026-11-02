//! Markdown corpus: every file in `tests/fixtures/` is parsed the way the app parses markdown (the
//! markdown file pane, notebooks and AI answers all go through `parse_markdown`; GFM tables are on
//! via `parse_markdown_with_gfm_tables`, which is the runtime switch behind the `markdown_tables`
//! flag) and the resulting document model (`FormattedText`) is snapshotted with insta, so any
//! change in how a construct is read shows up as a reviewable diff (#1460).
//!
//! Update: `INSTA_UPDATE=always cargo nextest run -p markdown_parser --test corpus` on the build
//! box, then `cargo insta review` / commit the `tests/snapshots/*.snap` diff. See docs/testing.md.
//!
//! Besides the snapshots every corpus file is held to the invariants the app relies on: the parse
//! never panics, is deterministic, and a tail re-parse from any recorded resume point equals the
//! full parse (the streaming path used for AI answers).

use std::path::PathBuf;

use markdown_parser::markdown_parser::parse_markdown_to_raw_text;
use markdown_parser::{
    FormattedText, FormattedTextLine, MarkdownResumePoint, parse_inline_markdown, parse_markdown,
    parse_markdown_tail, parse_markdown_with_gfm_tables,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// (file stem, contents) of every fixture, sorted so the order is stable.
fn corpus() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(fixtures_dir()).expect("fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).expect("read fixture");
        let text = String::from_utf8(bytes).unwrap_or_else(|e| panic!("{stem}: not UTF-8: {e}"));
        out.push((stem, text));
    }
    out.sort();
    out
}

/// Parse errors print without the (debug-only) nom detail so the snapshot is the same in debug and
/// release builds.
fn render(result: anyhow::Result<FormattedText>) -> String {
    match result {
        Ok(doc) => format!("{doc:#?}"),
        Err(_) => "Err(parse failed)".to_string(),
    }
}

#[test]
fn corpus_is_not_empty() {
    let names: Vec<_> = corpus().into_iter().map(|(n, _)| n).collect();
    for expected in [
        "headings",
        "lists_nested",
        "task_lists",
        "tables",
        "code_fenced",
        "inline",
        "links_images",
        "blockquote",
        "html_blocks",
        "footnotes",
        "unicode_rtl",
        "malformed",
        "crlf",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing fixture {expected}"
        );
    }
}

#[test]
fn corpus_snapshots_plain() {
    for (name, text) in corpus() {
        insta::assert_snapshot!(format!("plain__{name}"), render(parse_markdown(&text)));
    }
}

#[test]
fn corpus_snapshots_gfm_tables() {
    for (name, text) in corpus() {
        insta::assert_snapshot!(
            format!("gfm__{name}"),
            render(parse_markdown_with_gfm_tables(&text))
        );
    }
}

#[test]
fn corpus_parse_is_deterministic_and_never_panics() {
    for (name, text) in corpus() {
        for gfm in [false, true] {
            let parse = |t: &str| {
                if gfm {
                    parse_markdown_with_gfm_tables(t)
                } else {
                    parse_markdown(t)
                }
            };
            let a = parse(&text).ok();
            let b = parse(&text).ok();
            assert_eq!(a, b, "{name} (gfm={gfm}) parses differently twice");
            // Raw-text extraction and inline parsing are used for search and tab titles.
            let _ = parse_markdown_to_raw_text(&text);
            let _ = parse_inline_markdown(&text);
        }
    }
}

/// CRLF must read as the same document as LF, except where the fixture itself is the CRLF file.
#[test]
fn crlf_parses_like_lf_for_text_content() {
    for (name, text) in corpus() {
        if text.contains('\r') {
            continue;
        }
        let crlf = text.replace('\n', "\r\n");
        let lf_raw = parse_markdown_to_raw_text(&text);
        let crlf_raw = parse_markdown_to_raw_text(&crlf);
        if let (Ok(lf), Ok(crlf)) = (lf_raw, crlf_raw) {
            assert_eq!(
                lf.replace('\r', ""),
                crlf.replace('\r', ""),
                "{name}: CRLF input reads differently from LF input"
            );
        }
    }
}

fn long_line_doc() -> String {
    let word = "wordy ";
    let long = word.repeat(50_000); // 300 KB on one line
    let long_code = "x".repeat(200_000);
    format!(
        "# Long\n\n{long}\n\n```text\n{long_code}\n```\n\n| a | b |\n|---|---|\n| {long} | 2 |\n\n- {long}\n"
    )
}

#[test]
fn very_long_lines_parse_without_blowup() {
    let doc = long_line_doc();
    let start = std::time::Instant::now();
    let _ = parse_markdown(&doc);
    let _ = parse_markdown_with_gfm_tables(&doc);
    // Budget note: this is a smoke bound, not a benchmark. A quadratic regression on a ~600 KB
    // document takes minutes; a healthy parse takes well under a second even in debug builds.
    assert!(
        start.elapsed() < std::time::Duration::from_secs(30),
        "long-line document took {:?}",
        start.elapsed()
    );
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let quotes = format!("{} deep\n", "> ".repeat(500));
    let list: String = (0..300)
        .map(|i| format!("{}- item\n", "  ".repeat(i)))
        .collect();
    let emphasis = format!("{}x{}\n", "*".repeat(2000), "*".repeat(2000));
    for doc in [quotes, list, emphasis] {
        let _ = parse_markdown(&doc);
        let _ = parse_markdown_with_gfm_tables(&doc);
    }
}

/// The streaming path (AI answers grow chunk by chunk): feeding a corpus file line by line through
/// `parse_markdown_tail`, keeping the lines before each resume point, ends at exactly the document
/// one full parse produces.
#[test]
fn streamed_tail_parse_matches_full_parse() {
    for (name, text) in corpus() {
        for gfm in [false, true] {
            let full = if gfm {
                parse_markdown_with_gfm_tables(&text)
            } else {
                parse_markdown(&text)
            };
            let Ok(full) = full else { continue };

            let mut lines: Vec<FormattedTextLine> = Vec::new();
            let mut resume = MarkdownResumePoint::default();
            let mut seen = String::new();
            let mut ok = true;
            for chunk in text.split_inclusive('\n') {
                // Everything before this chunk is final; the chunk itself may still change.
                let final_len = seen.len();
                seen.push_str(chunk);
                let Ok(tail) = parse_markdown_tail(&seen, &resume, gfm, final_len) else {
                    ok = false;
                    break;
                };
                lines.truncate(resume.line_count());
                lines.extend(tail.lines);
                resume = tail.resume;
            }
            if !ok {
                continue;
            }
            if let Some(FormattedTextLine::LineBreak) = lines.last() {
                lines.pop();
            }
            assert_eq!(
                lines,
                Vec::from(full.lines),
                "{name} (gfm={gfm}): streamed parse differs from full parse"
            );
        }
    }
}
