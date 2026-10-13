//! Tests for [`parse_markdown_tail`]: re-parsing a growing document from a resume point must
//! produce exactly what a full parse produces, however the document was chunked (#849).

use super::*;

/// Documents covering every block construct, plus the ones that read past their own line.
const CORPUS: &[&str] = &[
    "",
    "plain",
    "one line\n",
    "first paragraph\n\nsecond paragraph\nwith two lines\n",
    "# Heading\n\nSome *emphasis* and **strong** and `inline code` and a [link](https://x.y).\n",
    "## Sub heading\ntext right after\n### Third\n",
    "- a\n- b\n  - nested\n    - deeper\n- back\n\n- after blank\n",
    "* star\n* items\n\nparagraph resets\n  - indented start\n",
    "1. one\n2. two\n3. three\n\n4. four after blank\n10. ten\n   1. nested\n   2. nested two\n",
    "1. a\nnot a list\n1. restart\n",
    "- [ ] todo\n- [x] done\n  - [ ] nested todo\n",
    "---\n***\n___\n- - -\n",
    "```rust\nfn main() {}\n```\nafter\n",
    "```\nunterminated fence\nmore code\n\nstill code?\n",
    "  ```py\n  indented\n  ```\n",
    "text\n```\ncode\n```\n\n```\nsecond\n```\n",
    "| a | b |\n| --- | :---: |\n| 1 | 2 |\n| 3 | 4 |\n\nafter table\n",
    "| a | b |\n| --- | --- |\n| row |\nnot a row\n",
    "| header only |\nplain next\n",
    "| a | b |\n|---|---|\n",
    "![alt](https://img.png)\n\n![two](a.png \"title\")\n",
    "![multi\nline alt](x.png)\nafter\n",
    "![never closed\nmore\n\nand more\n",
    "<!-- comment -->\nvisible\n",
    "<!-- multi\nline\ncomment -->\nvisible\n",
    "<!-- unterminated\nstill hidden?\n\nmore\n",
    "<!-- a --> trailing text\n",
    "inline <!-- hidden --> text\n",
    "`code span\nacross lines`\n",
    "**bold\nacross lines**\n",
    "line one\r\nline two\r\n\r\nline three\r\n",
    "lone\rcarriage\rreturns\r",
    "trailing blank lines\n\n\n",
    "\n\n\nleading blank lines\n",
    "unicode \u{1f600} text\n- \u{00e9}l\u{00e8}ve\n\n\u{4e2d}\u{6587}\n",
    "> not a quote construct\n>> nested\n",
    "mix\n- list\ntext\n1. ordered\n- [ ] task\n# head\n---\n| t | u |\n| - | - |\n| 1 | 2 |\n```\nc\n```\n",
];

fn full_lines(markdown: &str, parse_gfm_tables: bool) -> Option<Vec<FormattedTextLine>> {
    parse_markdown_internal::<nom::error::Error<&str>>(markdown, parse_gfm_tables)
        .ok()
        .map(|(_, lines)| lines)
}

/// Streams `markdown` in the given chunk boundaries, resuming at every step, and checks every
/// intermediate result against a full parse. Returns the number of bytes re-parsed.
fn stream_and_check(markdown: &str, boundaries: &[usize], parse_gfm_tables: bool) -> usize {
    let mut kept: Vec<FormattedTextLine> = Vec::new();
    let mut resume = MarkdownResumePoint::default();
    let mut bytes_parsed = 0;
    for &end in boundaries {
        let text = &markdown[..end];
        let Some(expected) = full_lines(text, parse_gfm_tables) else {
            assert!(parse_markdown_tail(text, &resume, parse_gfm_tables, end).is_err());
            continue;
        };
        bytes_parsed += end - resume.offset();
        let tail = parse_markdown_tail(text, &resume, parse_gfm_tables, end)
            .expect("a document that parses in full parses from a resume point");
        assert!(
            kept.len() >= resume.line_count(),
            "resume point {resume:?} is past the lines kept for {text:?}"
        );
        kept.truncate(resume.line_count());
        kept.extend(tail.lines);
        assert_eq!(
            kept, expected,
            "incremental parse diverged at byte {end} of {markdown:?}"
        );
        assert!(tail.resume.offset() <= end);
        resume = tail.resume;
    }
    bytes_parsed
}

fn char_boundaries(markdown: &str) -> Vec<usize> {
    (0..=markdown.len())
        .filter(|i| markdown.is_char_boundary(*i))
        .collect()
}

#[test]
fn resuming_from_any_prefix_matches_a_full_parse() {
    for parse_gfm_tables in [false, true] {
        for markdown in CORPUS {
            let Some(full) = full_lines(markdown, parse_gfm_tables) else {
                continue;
            };
            for split in char_boundaries(markdown) {
                let prefix = &markdown[..split];
                let Ok(first) = parse_markdown_tail(
                    prefix,
                    &MarkdownResumePoint::default(),
                    parse_gfm_tables,
                    split,
                ) else {
                    continue;
                };
                let resume = first.resume;
                // What a resume point vouches for is the same in the prefix and the whole.
                assert_eq!(
                    first.lines[..resume.line_count()],
                    full[..resume.line_count()],
                    "prefix {prefix:?} of {markdown:?}"
                );
                let rest = parse_markdown_tail(markdown, &resume, parse_gfm_tables, markdown.len())
                    .expect("full document parses");
                let mut joined = full[..resume.line_count()].to_vec();
                joined.extend(rest.lines);
                assert_eq!(joined, full, "split at {split} of {markdown:?}");
            }
        }
    }
}

#[test]
fn streaming_byte_by_byte_matches_a_full_parse_at_every_step() {
    for parse_gfm_tables in [false, true] {
        for markdown in CORPUS {
            stream_and_check(markdown, &char_boundaries(markdown), parse_gfm_tables);
        }
    }
}

#[test]
fn streaming_the_whole_corpus_in_random_chunks_matches_a_full_parse() {
    let document = CORPUS.join("\n");
    // A fixed-seed LCG keeps the test deterministic without a dependency.
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    for parse_gfm_tables in [false, true] {
        for _ in 0..20 {
            let mut boundaries = Vec::new();
            let mut end = 0;
            while end < document.len() {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                end = (end + 1 + (state >> 33) as usize % 64).min(document.len());
                while !document.is_char_boundary(end) {
                    end += 1;
                }
                boundaries.push(end);
            }
            stream_and_check(&document, &boundaries, parse_gfm_tables);
        }
    }
}

#[test]
fn trimmed_result_matches_parse_markdown() {
    // `parse_markdown` trims one trailing line break; a caller splicing tail lines must do the
    // same to the joined lines and get the same `FormattedText`.
    for markdown in CORPUS {
        let Ok(expected) = parse_markdown_with_gfm_tables(markdown) else {
            continue;
        };
        let mut lines = parse_markdown_tail(markdown, &MarkdownResumePoint::default(), true, 0)
            .expect("parses")
            .lines;
        if let Some(FormattedTextLine::LineBreak) = lines.last() {
            lines.pop();
        }
        assert_eq!(FormattedText::new(lines), expected, "{markdown:?}");
    }
}

#[test]
fn resume_point_never_passes_final_len() {
    let markdown = "a\nb\nc\nd\n";
    for final_len in 0..=markdown.len() {
        let tail = parse_markdown_tail(markdown, &MarkdownResumePoint::default(), true, final_len)
            .expect("parses");
        assert!(tail.resume.offset() <= final_len);
    }
}

#[test]
fn unfinished_multi_line_constructs_hold_the_resume_point_back() {
    // Each of these can still turn into something else when more text arrives, so nothing after
    // its opening line may be vouched for.
    for (markdown, opener) in [
        ("before\n<!-- open\nx\ny\n", "<!--"),
        ("before\n![alt\nx\ny\n", "!["),
        ("before\n```\nx\ny\n", "```"),
        ("before\n| a | b |\n", "| a"),
    ] {
        let tail = parse_markdown_tail(
            markdown,
            &MarkdownResumePoint::default(),
            true,
            markdown.len(),
        )
        .expect("parses");
        let opener_at = markdown.find(opener).expect("opener present");
        assert!(
            tail.resume.offset() <= opener_at,
            "{markdown:?} resumed at {} past its open {opener:?}",
            tail.resume.offset()
        );
    }
}

#[test]
fn a_long_flat_list_keeps_a_small_indentation_stack() {
    let markdown = "- item\n".repeat(10_000);
    let tail = parse_markdown_tail(
        &markdown,
        &MarkdownResumePoint::default(),
        true,
        markdown.len(),
    )
    .expect("parses");
    assert_eq!(tail.resume.line_count(), 10_000);
    assert!(tail.resume.indentation_stack.len() <= 1);
}

#[test]
fn streaming_a_megabyte_in_4k_chunks_parses_each_byte_a_bounded_number_of_times() {
    // Before #849 every chunk re-parsed the whole answer: 1 MB in 4 KB chunks was ~128 MB of
    // parsing. Resuming must keep it proportional to the answer.
    let paragraph = "Some prose with *emphasis*, `code`, and a [link](https://example.com).\n\
                     - a list item\n  - nested item\n1. ordered\n2. ordered\n\n\
                     | a | b |\n| --- | --- |\n| 1 | 2 |\n\n";
    let mut markdown = String::new();
    while markdown.len() < 1 << 20 {
        markdown.push_str(paragraph);
    }
    let mut kept: Vec<FormattedTextLine> = Vec::new();
    let mut resume = MarkdownResumePoint::default();
    let mut bytes_parsed = 0;
    let mut end = 0;
    while end < markdown.len() {
        end = (end + 4096).min(markdown.len());
        bytes_parsed += end - resume.offset();
        let tail = parse_markdown_tail(&markdown[..end], &resume, true, end).expect("parses");
        kept.truncate(resume.line_count());
        kept.extend(tail.lines);
        resume = tail.resume;
    }
    assert_eq!(Some(kept), full_lines(&markdown, true));
    assert!(
        bytes_parsed <= 2 * markdown.len(),
        "parsed {bytes_parsed} bytes to stream {} bytes",
        markdown.len()
    );
}
