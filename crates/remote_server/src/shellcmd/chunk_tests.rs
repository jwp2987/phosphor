//! Golden tests: the expectations are the ones in the app's `chunker/naive_tests.rs`, plus
//! hand-computed spans for the multi-chunk case.

use super::*;

fn text(code: &str, spans: &[Span]) -> Vec<String> {
    spans
        .iter()
        .map(|s| code[s.byte_range()].to_string())
        .collect()
}

fn joined(code: &str, spans: &[Span]) -> String {
    text(code, spans).join("")
}

const SAMPLE: &str = "This is some text content\nthat should be chunked\nusing the naive chunker\nbecause the language isn't recognized.";

#[test]
fn one_line_per_chunk_gives_one_span_per_line() {
    let spans = chunk_with(SAMPLE, 10000, 1);
    assert_eq!(spans.len(), SAMPLE.lines().count());
    for (i, line) in SAMPLE.lines().enumerate() {
        assert_eq!(&SAMPLE[spans[i].byte_range()], line);
        assert_eq!((spans[i].start_line, spans[i].end_line), (i, i));
    }
}

#[test]
fn a_large_chunk_is_one_span() {
    let spans = chunk_with(SAMPLE, 10000, 100);
    assert_eq!(spans.len(), 1);
    assert_eq!(&SAMPLE[spans[0].byte_range()], SAMPLE);
    assert_eq!((spans[0].start_line, spans[0].end_line), (0, 3));
}

#[test]
fn max_bytes_cuts_the_long_line_and_keeps_the_short_ones_together() {
    let code = "line1\nline2\nline3\nline4abcdefghijklmnopqrstuvwxyz";
    let spans = chunk_with(code, 25, 1000);
    let t = text(code, &spans);
    assert_eq!(t[0], "line1\nline2\nline3");
    assert_eq!(t[1], "line4abcdefghijklmnopqrst");
    assert_eq!(t[2], "uvwxyz");
    assert_eq!(t.len(), 3);
    assert_eq!((spans[0].start_line, spans[0].end_line), (0, 2));
    assert_eq!((spans[1].start_line, spans[1].end_line), (3, 3));
    assert_eq!((spans[2].start_line, spans[2].end_line), (3, 3));
    assert_eq!(spans[0].byte_range(), 0..17);
    assert_eq!(spans[1].byte_range(), 18..43);
    assert_eq!(spans[2].byte_range(), 43..49);
}

#[test]
fn multibyte_text_is_cut_on_char_boundaries_and_loses_nothing() {
    for (code, max) in [
        ("Hello 🦀 Rust\nWorld 🌍 Test\n🚀 Rocket 🎯 Target", 15),
        ("Café résumé naïve\nÉlève découvrir\nMañana piñata", 10),
        ("ASCII text 中文 🦀 résumé ℘ math symbols", 8),
        ("ab🦀cd", 3),
        ("Hi🦀Test", 3),
    ] {
        let spans = chunk_with(code, max, 1000);
        for s in &spans {
            assert!(code.is_char_boundary(s.byte_start) && code.is_char_boundary(s.byte_end));
            assert!(s.byte_end - s.byte_start <= max.max(4), "{code:?}");
        }
        let no_nl = |s: &str| s.replace('\n', "");
        assert_eq!(no_nl(&joined(code, &spans)), no_nl(code));
    }
}

#[test]
fn one_wide_char_over_the_cap_is_one_span() {
    let spans = chunk_with("🦀", 2, 1000);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].byte_range(), 0..4);
}

#[test]
fn multibyte_lines_one_per_chunk() {
    let code = "Hello🌍\nWorld🦀\nTest🎯";
    let spans = chunk_with(code, 10, 1);
    assert_eq!(spans.len(), 3);
    for (i, s) in spans.iter().enumerate() {
        assert_eq!((s.start_line, s.end_line), (i, i));
    }
}

#[test]
fn the_real_constants_chunk_every_200_lines() {
    // 450 lines "l<n>": spans of lines 0..=199, 200..=399, 400..=449.
    let code: String = (0..450).map(|n| format!("l{n}\n")).collect();
    let spans = chunk_text(&code);
    assert_eq!(spans.len(), 3);
    assert_eq!((spans[0].start_line, spans[0].end_line), (0, 199));
    assert_eq!((spans[1].start_line, spans[1].end_line), (200, 399));
    assert_eq!((spans[2].start_line, spans[2].end_line), (400, 449));
    assert_eq!(spans[0].byte_start, 0);
    // The span ends at the last line's END: its terminator is not inside the fragment.
    assert_eq!(
        &code[spans[0].byte_range()][code[spans[0].byte_range()].len() - 4..],
        "l199"
    );
    assert_eq!(spans[1].byte_start, code.find("l200").unwrap());
    assert_eq!(spans[2].byte_end, code.trim_end().len());
}

#[test]
fn crlf_and_empty_input() {
    assert!(chunk_text("").is_empty());
    let code = "a\r\nb\r\n";
    let spans = chunk_text(code);
    assert_eq!(spans.len(), 1);
    assert_eq!(&code[spans[0].byte_range()], "a\r\nb");
    assert_eq!((spans[0].start_line, spans[0].end_line), (0, 1));
}
