//! The markdown reader behind the markdown file pane, notebooks and AI answers. First byte picks
//! the entry point so one corpus covers them all: bit 0 selects GFM tables, bits 1-2 the API
//! (0 full parse, 1 raw text, 2 inline only, 3 streamed in two halves through the tail parser,
//! which must equal the full parse). Seeds: `fuzz/corpus/markdown_parser/` (copies of
//! `crates/markdown_parser/tests/fixtures/*.md`). Any panic, hang or OOM is a finding. #1460.
#![no_main]

use libfuzzer_sys::fuzz_target;
use md::{
    MarkdownResumePoint, parse_inline_markdown, parse_markdown, parse_markdown_tail,
    parse_markdown_with_gfm_tables,
};

fn full(text: &str, gfm: bool) -> Option<md::FormattedText> {
    if gfm {
        parse_markdown_with_gfm_tables(text).ok()
    } else {
        parse_markdown(text).ok()
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    // Cap the size: a quadratic parser is a finding, but a 100 MB input only measures the fuzzer.
    let rest = &rest[..rest.len().min(256 * 1024)];
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let gfm = sel & 1 == 1;
    match (sel >> 1) & 3 {
        0 => {
            let _ = full(text, gfm);
        }
        1 => {
            if let Some(doc) = full(text, gfm) {
                let _ = doc.raw_text();
            }
        }
        2 => {
            let _ = parse_inline_markdown(text);
        }
        _ => {
            // Split at a char boundary near the middle, parse the first half, then continue from
            // its resume point; the joined lines must equal one full parse.
            let mut cut = text.len() / 2;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            let (head, _) = text.split_at(cut);
            let Ok(first) = parse_markdown_tail(head, &MarkdownResumePoint::default(), gfm, cut)
            else {
                return;
            };
            let Ok(second) = parse_markdown_tail(text, &first.resume, gfm, text.len()) else {
                return;
            };
            let mut lines: Vec<_> = first.lines;
            lines.truncate(first.resume.line_count());
            lines.extend(second.lines);
            if let Some(md::FormattedTextLine::LineBreak) = lines.last() {
                lines.pop();
            }
            if let Some(doc) = full(text, gfm) {
                assert_eq!(lines, Vec::from(doc.lines), "streamed != full parse");
            }
        }
    }
});
