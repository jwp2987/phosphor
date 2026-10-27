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

/// The first setup line is written behind ` ^K ^U`. What a cooked tty echoes for it, captured
/// byte for byte from zsh 5.9 under a pty on macOS 26 and on Linux 7.1 (#1038): identical on both.
const CAPTURED_LINE_CLEAR_ECHO: &str = " ^K\u{8} \u{8}\u{8} \u{8}\u{8} \u{8}";

#[test]
fn the_line_clear_prefix_echo_matches_the_captured_tty_echo() {
    assert_eq!(super::LINE_CLEAR_ECHO, CAPTURED_LINE_CLEAR_ECHO);
}

#[test]
fn swallows_the_line_clear_echo_before_the_first_line() {
    let mut filter = SetupEchoFilter::default();
    filter.expect_line_clear_prefix();
    filter.expect_line(" [ -n \"${__whi-}\" ] || { __whi=0; }");
    filter.expect_line(" _wbs='a'");
    let output = format!(
        "{CAPTURED_LINE_CLEAR_ECHO} [ -n \"${{__whi-}}\" ] || {{ __whi=0; }}\r\n _wbs='a'\r\n"
    );
    assert_eq!(visible(&mut filter, &output), "");
    assert!(!filter.is_armed());
}

#[test]
fn without_the_prefix_expectation_the_echo_is_shown() {
    // The pre-#1038-fix arming: the leading space matched the line, `^` diverged, the filter
    // stood down and everything after it was shown.
    let mut filter = SetupEchoFilter::default();
    filter.expect_line(" [ -n x ]");
    let output = format!("{CAPTURED_LINE_CLEAR_ECHO} [ -n x ]\r\n");
    assert_ne!(visible(&mut filter, &output), "");
    assert!(!filter.is_armed());
}

#[test]
fn a_stall_part_way_through_a_line_is_mid_line() {
    // macOS: the tty input queue (1024 bytes) fills part-way through the second line, so the echo
    // stops there for as long as the shell sleeps.
    let mut filter = SetupEchoFilter::default();
    filter.expect_line_clear_prefix();
    filter.expect_line(" first");
    filter.expect_line(" _wbs='export WARP_HONOR_PS1=0; _x=$(( [##16] _i ))'");
    filter.expect_line(" last");
    assert!(!filter.is_mid_line());
    let output =
        format!("{CAPTURED_LINE_CLEAR_ECHO} first\r\n _wbs='export WARP_HONOR_PS1=0; _x=$(( [##");
    assert_eq!(visible(&mut filter, &output), "");
    assert!(filter.is_mid_line());
    // Between lines nothing is half matched.
    let mut filter = SetupEchoFilter::default();
    filter.expect_line(" first");
    filter.expect_line(" last");
    assert_eq!(visible(&mut filter, " first\r\n"), "");
    assert!(!filter.is_mid_line());
}

#[test]
fn a_diverging_line_clear_echo_stands_the_filter_down() {
    // A shell whose line editor already runs echoes the prefix itself, differently: nothing
    // after the divergence is hidden.
    let mut filter = SetupEchoFilter::default();
    filter.expect_line_clear_prefix();
    filter.expect_line(" x");
    assert_eq!(visible(&mut filter, " \u{8} x\r\n"), "\u{8} x\r\n");
    assert!(!filter.is_armed());
    assert!(!filter.is_mid_line());
}
