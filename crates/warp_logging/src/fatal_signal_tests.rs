use super::*;

#[test]
fn the_utc_date_matches_the_file_logs_timestamp() {
    // appci r18u26: t=1793888585127 ms was logged as 2026-11-05T14:23:05Z.
    assert_eq!(utc_civil(1_793_888_585), (2026, 11, 5, 14, 23, 5));
    assert_eq!(utc_civil(0), (1970, 1, 1, 0, 0, 0));
    // A leap day and the last second of a year.
    assert_eq!(utc_civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    assert_eq!(utc_civil(1_735_689_599), (2024, 12, 31, 23, 59, 59));
}

#[test]
fn the_line_names_the_signal_the_address_and_the_thread() {
    let mut line = LineBuf::new();
    format_line(
        &mut line,
        1_793_888_585,
        libc::SIGBUS,
        2,
        0x16f6_0bff0,
        b"clipboard-read",
        94_694,
    );
    let text = std::str::from_utf8(line.bytes()).unwrap();
    assert_eq!(
        text,
        format!(
            "2026-11-05T14:23:05Z [ERROR] fatal signal SIGBUS ({}) code=2 addr=0x16f60bff0 \
             thread='clipboard-read' pid=94694 (fault); the process is about to die\n",
            libc::SIGBUS
        )
    );
}

#[test]
fn an_unnamed_thread_and_a_sent_signal_are_said_so() {
    let mut line = LineBuf::new();
    format_line(&mut line, 0, libc::SIGSEGV, 0, 0, b"", 1);
    let text = std::str::from_utf8(line.bytes()).unwrap();
    assert!(text.contains("SIGSEGV"), "{text}");
    assert!(text.contains("thread='<unnamed>'"), "{text}");
    assert!(text.contains("addr=0x0 "), "{text}");
    assert!(text.contains("(sent by a process)"), "{text}");
}

#[test]
fn a_long_thread_name_cannot_overflow_the_line() {
    let mut line = LineBuf::new();
    let name = [b'x'; 2_000];
    format_line(&mut line, 0, libc::SIGABRT, 1, usize::MAX, &name, 1);
    assert_eq!(line.bytes().len(), 512);
}

#[test]
fn fault_codes_are_not_mistaken_for_sent_signals() {
    // BUS_ADRALN / SEGV_MAPERR / SEGV_ACCERR are small positive codes on both platforms.
    assert!(!sent_by_a_process(1));
    assert!(!sent_by_a_process(2));
    // Linux SI_USER / SI_QUEUE / SI_TKILL; macOS SI_USER.
    assert!(sent_by_a_process(0));
    assert!(sent_by_a_process(-1));
    assert!(sent_by_a_process(-6));
    assert!(sent_by_a_process(0x10001));
}

#[test]
fn the_kill_switch_turns_it_off() {
    assert!(enabled_by(None));
    assert!(enabled_by(Some("1")));
    assert!(!enabled_by(Some("0")));
    assert!(!enabled_by(Some(" OFF ")));
    assert!(!enabled_by(Some("false")));
}
