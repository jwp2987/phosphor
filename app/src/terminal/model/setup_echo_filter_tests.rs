use super::SetupEchoFilter;

/// What the grid would see after the filter: the characters not swallowed.
fn visible(filter: &mut SetupEchoFilter, output: &str) -> String {
    output.chars().filter(|c| !filter.swallow(*c)).collect()
}

#[test]
fn swallows_the_echo_of_every_line_and_its_crlf() {
    let mut filter = SetupEchoFilter::default();
    filter.expect_line(" __hc0=1; x");
    filter.expect_line(" _wbs='a'");
    assert_eq!(visible(&mut filter, " __hc0=1; x\r\n _wbs='a'\r\n"), "");
    assert!(!filter.is_armed());
}

#[test]
fn passes_output_before_the_echo_and_stays_armed() {
    let mut filter = SetupEchoFilter::default();
    filter.expect_line(" _wbs='a'");
    // An rc banner, then the echo, then the shell's own output after it.
    assert_eq!(
        visible(&mut filter, "Loading...\r\n _wbs='a'\r\nafter"),
        "Loading...\r\nafter"
    );
}

#[test]
fn stands_down_when_a_half_matched_line_diverges() {
    let mut filter = SetupEchoFilter::default();
    filter.expect_line(" _wbs='abc'");
    // The shell redraws the line differently from the plain echo: what follows is not filtered.
    assert_eq!(visible(&mut filter, " _wbs='aXbc'\r\n"), "Xbc'\r\n");
    assert!(!filter.is_armed());
    assert_eq!(visible(&mut filter, " _wbs='abc'\r\n"), " _wbs='abc'\r\n");
}

#[test]
fn disarmed_filter_swallows_nothing() {
    let mut filter = SetupEchoFilter::default();
    filter.expect_line("x");
    filter.disarm();
    assert_eq!(visible(&mut filter, "x\r\n"), "x\r\n");
}

#[test]
fn an_unrelated_line_with_the_same_first_character_is_not_swallowed_past_it() {
    let mut filter = SetupEchoFilter::default();
    filter.expect_line(" _wbs='a'");
    // Output that starts like the echo but is not it loses at most the matching prefix.
    assert_eq!(visible(&mut filter, " hello\r\n"), "hello\r\n");
    assert!(!filter.is_armed());
}
