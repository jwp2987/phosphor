use super::*;

fn toks(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn finds_every_occurrence_with_char_offsets() {
    let t = toks(&["msel42", "zz"]);
    let r = find_token_ranges("xx msel42 yy msel42", &t);
    assert_eq!(r, vec![(0, 3, 9), (0, 13, 19)]);
    assert!(find_token_ranges("nothing here", &t).is_empty());
}

#[test]
fn offsets_are_chars_not_bytes() {
    let t = toks(&["tok"]);
    assert_eq!(find_token_ranges("é→ tok", &t), vec![(0, 3, 6)]);
}

#[test]
fn empty_and_overlong_tokens_never_match() {
    let t = toks(&["", "longer than the text"]);
    assert!(find_token_ranges("short", &t).is_empty());
}

#[test]
fn extent_needs_first_and_last_glyph() {
    // glyphs for "ab tok": indexes 0..6, 10 px wide
    let g: Vec<_> = (0..6).map(|i| (i, i as f32 * 10., 10.)).collect();
    assert_eq!(glyph_extent(&g, 3, 6), Some((30., 60.)));
    assert_eq!(glyph_extent(&g, 3, 8), None);
    assert_eq!(glyph_extent(&g, 4, 4), None);
}

#[test]
fn hits_carry_token_range_and_extent() {
    let t = toks(&["tok"]);
    let g: Vec<_> = (0..6).map(|i| (i, 5. + i as f32 * 10., 10.)).collect();
    let h = hits_for_line("ab tok", &t, &g);
    assert_eq!(
        h,
        vec![FindHit {
            token: "tok".into(),
            start: 3,
            end: 6,
            x0: 35.,
            x1: 65.
        }]
    );
}

#[test]
fn token_file_parsing() {
    assert_eq!(parse_tokens("a\n\n  b  \na\r\n"), toks(&["a", "b"]));
    let long = "x".repeat(MAX_TOKEN_CHARS + 1);
    assert!(parse_tokens(&long).is_empty());
}

#[test]
fn logs_once_per_position_change() {
    let mut r = Reporter::default();
    let t0 = Instant::now();
    assert!(r.should_log("t", [1, 2, 3, 4], t0));
    assert!(!r.should_log("t", [1, 2, 3, 4], t0));
    assert!(r.should_log("t", [1, 9, 3, 4], t0));
    // an older position coming back is held back until RELOG has passed
    assert!(!r.should_log("t", [1, 2, 3, 4], t0));
    assert!(r.should_log("t", [1, 2, 3, 4], t0 + RELOG));
    // tokens are independent
    assert!(r.should_log("u", [1, 2, 3, 4], t0));
}

#[test]
fn line_is_device_pixels_in_the_harness_format() {
    assert_eq!(
        format_line("t", [1., 2., 3.5, 4.], 2.),
        "appci_find token=t x0=2.0 y0=4.0 x1=7.0 y1=8.0"
    );
}
