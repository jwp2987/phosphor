use std::collections::VecDeque;

use ratatui::crossterm::event::{KeyEventState, MouseEvent, MouseEventKind};

use super::*;

#[test]
fn parses_bel_terminated_replies() {
    let replies = b"\x1b]10;rgb:ffff/ffff/ffff\x07\x1b]11;rgb:0000/0000/0000\x07\x1b[?6c";
    let colors = parse_replies(replies);
    assert_eq!(
        colors.fg,
        Some(ProbedRgb {
            r: 255,
            g: 255,
            b: 255
        })
    );
    assert_eq!(colors.bg, Some(ProbedRgb { r: 0, g: 0, b: 0 }));
}

#[test]
fn parses_st_terminated_replies() {
    let replies = b"\x1b]11;rgb:fdfd/e3e3/d2d2\x1b\\";
    let colors = parse_replies(replies);
    assert_eq!(colors.fg, None);
    assert_eq!(
        colors.bg,
        Some(ProbedRgb {
            r: 253,
            g: 227,
            b: 210
        })
    );
}

#[test]
fn parses_two_digit_components() {
    let replies = b"\x1b]11;rgb:28/2a/36\x07";
    assert_eq!(
        parse_replies(replies).bg,
        Some(ProbedRgb {
            r: 40,
            g: 42,
            b: 54
        })
    );
}

#[test]
fn parses_rgba_payload_ignoring_alpha() {
    let replies = b"\x1b]11;rgba:ffff/0000/8080/cccc\x07";
    assert_eq!(
        parse_replies(replies).bg,
        Some(ProbedRgb {
            r: 255,
            g: 0,
            b: 128
        })
    );
}

#[test]
fn scales_single_and_mixed_width_components() {
    // 1-digit components scale by 0xf, e.g. `f` -> 255, `8` -> 136.
    assert_eq!(parse_scaled_component("f"), Some(255));
    assert_eq!(parse_scaled_component("8"), Some(136));
    assert_eq!(parse_scaled_component("ffff"), Some(255));
    assert_eq!(parse_scaled_component("808"), Some(128));
    assert_eq!(parse_scaled_component(""), None);
    assert_eq!(parse_scaled_component("fffff"), None);
    assert_eq!(parse_scaled_component("zz"), None);
}

#[test]
fn ignores_malformed_payloads() {
    assert_eq!(parse_replies(b"\x1b]11;?\x07").bg, None);
    assert_eq!(parse_replies(b"\x1b]11;rgb:ff/ff\x07").bg, None);
    assert_eq!(parse_replies(b"\x1b]11;#282a36\x07").bg, None);
    assert_eq!(parse_replies(b"").bg, None);
}

#[test]
fn classifies_luminance_with_rec601_luma() {
    let white = ProbedRgb {
        r: 255,
        g: 255,
        b: 255,
    };
    let black = ProbedRgb { r: 0, g: 0, b: 0 };
    // Saturated blue is dark despite its high blue channel; saturated green
    // is light despite two zero channels — the luma weights dominate.
    let blue = ProbedRgb { r: 0, g: 0, b: 255 };
    let green = ProbedRgb { r: 0, g: 255, b: 0 };
    assert!(white.is_light());
    assert!(!black.is_light());
    assert!(!blue.is_light());
    assert!(green.is_light());
}

#[test]
fn background_luminance_prefers_probed_background() {
    let light = ProbedTerminalColors {
        fg: None,
        bg: Some(ProbedRgb {
            r: 253,
            g: 246,
            b: 227,
        }),
    };
    assert_eq!(light.background_luminance(), BackgroundLuminance::Light);

    let dark = ProbedTerminalColors {
        fg: None,
        bg: Some(ProbedRgb {
            r: 40,
            g: 42,
            b: 54,
        }),
    };
    assert_eq!(dark.background_luminance(), BackgroundLuminance::Dark);
}

#[test]
fn colorfgbg_heuristic() {
    assert_eq!(colorfgbg_luminance("15;0"), BackgroundLuminance::Dark);
    assert_eq!(colorfgbg_luminance("0;15"), BackgroundLuminance::Light);
    assert_eq!(
        colorfgbg_luminance("15;default;0"),
        BackgroundLuminance::Dark
    );
    assert_eq!(colorfgbg_luminance("8"), BackgroundLuminance::Dark);
    assert_eq!(colorfgbg_luminance("7"), BackgroundLuminance::Light);
    assert_eq!(
        colorfgbg_luminance("15;default"),
        BackgroundLuminance::Unknown
    );
    assert_eq!(colorfgbg_luminance(""), BackgroundLuminance::Unknown);
}

// ---- the reply filter -------------------------------------------------------------
//
// An "event" in these tests is its own wire image (`Vec<u8>`): that is what the filter
// looks at, and it lets the tests assert exactly which events came out and in what order.

const BG_REPLY: &str = "rgb:1e1e/1e1e/2e2e";
const BG: ProbedRgb = ProbedRgb {
    r: 0x1e,
    g: 0x1e,
    b: 0x2e,
};

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// The events crossterm produces for `bytes` arriving in one read: an ESC followed by a
/// byte is one Alt event, every other byte is its own event.
fn events(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] != 0x1b {
            out.push(vec![0x1b, bytes[i + 1]]);
            i += 2;
        } else {
            out.push(vec![bytes[i]]);
            i += 1;
        }
    }
    out
}

fn reply(code: u8, payload: &str, terminator: &[u8]) -> Vec<u8> {
    let mut bytes = format!("\x1b]{code};{payload}").into_bytes();
    bytes.extend_from_slice(terminator);
    bytes
}

/// Feeds each event; returns (passed-through wire images in order, replies).
fn run(filter: &mut ReplyFilter<Vec<u8>>, events: Vec<Vec<u8>>) -> (Vec<Vec<u8>>, Vec<Reply>) {
    let (mut passed, mut replies) = (Vec::new(), Vec::new());
    for event in events {
        for emit in filter.push(event.clone(), Some(event), Instant::now()) {
            match emit {
                Emit::Pass(event) => passed.push(event),
                Emit::Reply(reply) => replies.push(reply),
            }
        }
    }
    (passed, replies)
}

fn expecting_background() -> ReplyFilter<Vec<u8>> {
    let mut filter = ReplyFilter::new();
    filter.expect_background(later());
    filter
}

fn typed(text: &str) -> Vec<Vec<u8>> {
    text.bytes().map(|byte| vec![byte]).collect()
}

#[test]
fn user_bytes_before_between_and_after_replies_keep_their_order() {
    let mut filter = expecting_background();
    filter.expect_foreground(later());
    let mut stream = typed("ab");
    stream.extend(events(&reply(10, "rgb:ffff/ffff/ffff", b"\x07")));
    stream.extend(typed("cd"));
    stream.extend(events(&reply(11, BG_REPLY, b"\x07")));
    stream.extend(typed("ef"));
    let (passed, replies) = run(&mut filter, stream);
    assert_eq!(
        passed,
        typed("abcdef"),
        "typed bytes arrive whole and in order"
    );
    assert_eq!(
        replies,
        [
            Reply::Foreground(ProbedRgb {
                r: 255,
                g: 255,
                b: 255
            }),
            Reply::Background(BG)
        ]
    );
    assert!(!filter.is_holding());
}

#[test]
fn a_reply_split_across_reads_still_matches_bel_and_st() {
    for terminator in [&b"\x07"[..], &b"\x1b\\"[..]] {
        let mut filter = expecting_background();
        let bytes = reply(11, BG_REPLY, terminator);
        // One read per byte: every split point is exercised, ESC separate from `]`.
        let one_by_one: Vec<Vec<u8>> = bytes.iter().map(|byte| vec![*byte]).collect();
        let (passed, replies) = run(&mut filter, one_by_one);
        assert!(passed.is_empty(), "{passed:?}");
        assert_eq!(replies, [Reply::Background(BG)]);
        // Crossterm's own grouping (Alt-], Alt-\\ for ST) matches too.
        let mut filter = expecting_background();
        let (passed, replies) = run(&mut filter, events(&bytes));
        assert!(passed.is_empty(), "{passed:?}");
        assert_eq!(replies, [Reply::Background(BG)]);
    }
}

#[test]
fn alt_bracket_then_text_is_not_eaten() {
    let mut filter = expecting_background();
    let mut stream = events(b"\x1b]");
    stream.extend(typed("hello"));
    let (passed, replies) = run(&mut filter, stream);
    assert!(replies.is_empty());
    let mut expected = vec![vec![0x1b, b']']];
    expected.extend(typed("hello"));
    assert_eq!(passed, expected, "Alt-] and the text after it, in order");
    assert!(!filter.is_holding());

    // Also with digits after it that begin like a reply but are not one.
    let mut filter = expecting_background();
    let (passed, replies) = run(&mut filter, events(b"\x1b]12;rgb:00/00/00\x07"));
    assert!(replies.is_empty());
    assert_eq!(
        passed.concat(),
        b"\x1b]12;rgb:00/00/00\x07".to_vec(),
        "OSC 12 is not a reply we asked for"
    );
}

#[test]
fn an_unfinished_match_is_released_in_order_by_the_hold_net() {
    let mut filter = expecting_background();
    let (passed, _) = run(&mut filter, events(b"\x1b]11;rgb:1e"));
    assert!(passed.is_empty());
    assert!(filter.is_holding());
    let released: Vec<_> = filter
        .release_held()
        .into_iter()
        .map(|emit| match emit {
            Emit::Pass(event) => event,
            Emit::Reply(reply) => panic!("not a reply: {reply:?}"),
        })
        .collect();
    assert_eq!(released.concat(), b"\x1b]11;rgb:1e".to_vec());
    assert!(!filter.is_holding());
}

#[test]
fn a_late_reply_is_swallowed_once_and_only_while_outstanding() {
    let bytes = reply(11, BG_REPLY, b"\x07");
    // Nothing outstanding: the lookalike is typed input and passes through whole.
    let mut filter = ReplyFilter::new();
    let (passed, replies) = run(&mut filter, events(&bytes));
    assert!(replies.is_empty());
    assert_eq!(passed.concat(), bytes);

    // Outstanding: swallowed once; a second copy is typed input.
    let mut filter = expecting_background();
    let (passed, replies) = run(&mut filter, events(&bytes));
    assert!(passed.is_empty());
    assert_eq!(replies, [Reply::Background(BG)]);
    let (passed, replies) = run(&mut filter, events(&bytes));
    assert!(replies.is_empty(), "no longer outstanding");
    assert_eq!(passed.concat(), bytes);

    // Expired: a reply after its window is typed input again.
    let mut filter = ReplyFilter::new();
    filter.expect_background(Instant::now() - Duration::from_secs(1));
    assert!(!filter.is_expecting(Instant::now()));
    let (passed, replies) = run(&mut filter, events(&bytes));
    assert!(replies.is_empty());
    assert_eq!(passed.concat(), bytes);
}

#[test]
fn only_the_asked_for_osc_code_is_swallowed() {
    // Only the background is outstanding: an OSC 10 reply is not ours.
    let mut filter = expecting_background();
    let fg = reply(10, "rgb:ffff/ffff/ffff", b"\x07");
    let (passed, replies) = run(&mut filter, events(&fg));
    assert!(replies.is_empty());
    assert_eq!(passed.concat(), fg);
}

#[test]
fn an_event_that_is_not_a_key_breaks_a_partial_match_and_nothing_is_lost() {
    let mut filter = expecting_background();
    let mut emitted = Vec::new();
    for event in events(b"\x1b]11;rgb:") {
        emitted.extend(filter.push(event.clone(), Some(event), Instant::now()));
    }
    // A mouse event (no wire image) arrives mid-match.
    emitted.extend(filter.push(b"MOUSE".to_vec(), None, Instant::now()));
    let passed: Vec<Vec<u8>> = emitted
        .into_iter()
        .map(|emit| match emit {
            Emit::Pass(event) => event,
            Emit::Reply(reply) => panic!("not a reply: {reply:?}"),
        })
        .collect();
    let mut expected = events(b"\x1b]11;rgb:");
    expected.push(b"MOUSE".to_vec());
    assert_eq!(passed, expected, "everything out, in the order it came in");
}

#[test]
fn key_events_map_to_the_bytes_crossterm_parsed_them_from() {
    let key = |code, modifiers| {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    };
    assert_eq!(
        event_wire(&key(KeyCode::Char(']'), KeyModifiers::ALT)),
        Some(vec![0x1b, b']'])
    );
    assert_eq!(
        event_wire(&key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(vec![0x1b])
    );
    assert_eq!(
        event_wire(&key(KeyCode::Char('g'), KeyModifiers::CONTROL)),
        Some(vec![0x07]),
        "BEL"
    );
    assert_eq!(
        event_wire(&key(KeyCode::Char('A'), KeyModifiers::SHIFT)),
        Some(vec![b'A'])
    );
    assert_eq!(event_wire(&key(KeyCode::Enter, KeyModifiers::NONE)), None);
    assert_eq!(
        event_wire(&key(KeyCode::Char('x'), KeyModifiers::CONTROL)),
        None
    );
    let release = Event::Key(KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    });
    assert_eq!(event_wire(&release), None);
    let mouse = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(event_wire(&mouse), None);
}

// ---- the probe window -------------------------------------------------------------

/// Drives `run_probe_window` from a script of events. Returns the outcome and what was
/// forwarded, in order.
fn window(
    filter: &mut ReplyFilter<Vec<u8>>,
    script: Vec<Vec<u8>>,
    deadline: Duration,
) -> (ProbeEnd, Vec<Vec<u8>>, usize) {
    let script = std::cell::RefCell::new(VecDeque::from(script));
    let forwarded = std::cell::RefCell::new(Vec::new());
    let end = run_probe_window(
        filter,
        deadline,
        |_| Ok(!script.borrow().is_empty()),
        || Ok(script.borrow_mut().pop_front().expect("polled readable")),
        |event: &Vec<u8>| Some(event.clone()),
        |emit| {
            if let Emit::Pass(event) = emit {
                forwarded.borrow_mut().push(event);
            }
            true
        },
    )
    .expect("no io error");
    let unread = script.borrow().len();
    (end, forwarded.into_inner(), unread)
}

#[test]
fn the_window_stops_when_the_reply_completes_and_forwards_keys_typed_inside_it() {
    let mut filter = ReplyFilter::new();
    let mut script = typed("ls");
    script.extend(events(&reply(11, BG_REPLY, b"\x07")));
    script.extend(typed("zz"));
    let (end, forwarded, unread) = window(&mut filter, script, Duration::from_secs(5));
    assert!(
        matches!(end, ProbeEnd::Done(Some(rgb)) if rgb == BG),
        "{end:?}"
    );
    assert_eq!(
        forwarded,
        typed("ls"),
        "keys typed in the window are forwarded"
    );
    assert_eq!(
        unread, 2,
        "stopped at the reply; the keys after it are left to the main loop"
    );
}

#[test]
fn a_missed_window_gives_up_without_losing_keys_and_the_reply_stays_outstanding() {
    let mut filter = ReplyFilter::new();
    // An Alt-] withheld when the deadline passes is handed on, not dropped.
    let mut script = typed("a");
    script.extend(events(b"\x1b]"));
    let (end, forwarded, _) = window(&mut filter, script, Duration::from_millis(30));
    assert!(matches!(end, ProbeEnd::Done(None)), "{end:?}");
    let mut expected = typed("a");
    expected.push(vec![0x1b, b']']);
    assert_eq!(forwarded, expected);
    assert!(
        filter.is_expecting(Instant::now()),
        "a reply that lands late is still recognised"
    );
    // ...and is then swallowed, once.
    let (passed, replies) = run(&mut filter, events(&reply(11, BG_REPLY, b"\x07")));
    assert!(passed.is_empty());
    assert_eq!(replies, [Reply::Background(BG)]);
}
