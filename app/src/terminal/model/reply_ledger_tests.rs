use super::*;

fn filtered(replies: &[&str], typed: &[&str], report: &str) -> String {
    let mut ledger = ReplyLedger::default();
    for reply in replies {
        ledger.note_replies(reply.as_bytes());
    }
    for bytes in typed {
        ledger.note_input(bytes.as_bytes());
    }
    ledger.filter_report(report.to_owned())
}

#[test]
fn a_bare_r_after_a_cursor_position_reply_is_dropped() {
    assert_eq!(filtered(&["\x1b[24;1R"], &[], "R"), "");
}

#[test]
fn typed_text_starting_with_r_is_kept_after_a_cursor_position_reply() {
    assert_eq!(
        filtered(&["\x1b[24;1R"], &["Rscript"], "RRscript"),
        "Rscript"
    );
    assert_eq!(
        filtered(&["\x1b[24;1R"], &["R --version"], "RR --version"),
        "R --version"
    );
}

#[test]
fn typed_text_alone_is_never_touched() {
    assert_eq!(
        filtered(&["\x1b[24;1R"], &["Rscript"], "Rscript"),
        "Rscript"
    );
    assert_eq!(filtered(&["\x1b[24;1R"], &["R"], "R"), "R");
    assert_eq!(filtered(&[], &[], "R"), "R");
    assert_eq!(filtered(&[], &["x"], "Rx"), "Rx");
}

#[test]
fn text_typed_in_pieces_is_tracked() {
    assert_eq!(
        filtered(&["\x1b[24;1R"], &["R", "script"], "RRscript"),
        "Rscript"
    );
}

#[test]
fn text_that_does_not_spell_a_reply_tail_is_kept() {
    assert_eq!(filtered(&["\x1b[24;1R"], &[], "Q"), "Q");
    assert_eq!(filtered(&["\x1b[24;1R"], &["x"], "Qx"), "Qx");
    assert_eq!(filtered(&["\x1b[24;1R"], &[], "RR"), "RR");
}

#[test]
fn longer_residues_and_several_replies_are_dropped() {
    assert_eq!(filtered(&["\x1b[?62;c"], &[], "62;c"), "");
    assert_eq!(filtered(&["\x1b[?62;c"], &[], "c"), "");
    assert_eq!(
        filtered(&["\x1b[24;1R", "\x1b[?62;c"], &["ls"], "R62;cls"),
        "ls"
    );
    assert_eq!(filtered(&["\x1b[24;1R\x1b[?62;c"], &[], "Rc"), "");
}

#[test]
fn an_xtversion_reply_residue_is_dropped() {
    let reply = "\x1bP>|Phosphor(0.1.12)\x1b\\";
    assert_eq!(filtered(&[reply], &[], ">|Phosphor(0.1.12)"), "");
    assert_eq!(
        filtered(&[reply], &["echo hi"], ">|Phosphor(0.1.12)echo hi"),
        "echo hi"
    );
    assert_eq!(filtered(&[reply], &[], "Phosphor(0.1.12)"), "");
}

#[test]
fn control_keys_typed_by_the_user_disable_filtering() {
    assert_eq!(filtered(&["\x1b[24;1R"], &["\x1b[D"], "R"), "R");
    assert_eq!(filtered(&["\x1b[24;1R"], &["a", "\t"], "R"), "R");
}

#[test]
fn text_after_an_enter_is_not_modelled_until_a_command_starts() {
    // The line that was run may have been put back (histverify); nothing is known yet.
    assert_eq!(filtered(&["\x1b[24;1R"], &["ls\r", "Rx"], "RRx"), "RRx");
    assert_eq!(filtered(&["\x1b[24;1R"], &["ls\r"], "R"), "R");
    // Once a command started the line is empty again.
    let mut ledger = ledger_with(&[]);
    ledger.note_input(b"ls\r");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    ledger.note_input(b"Rx");
    assert_eq!(ledger.filter_report("RRx".into()), "Rx");
}

#[test]
fn the_ledger_starts_over_after_each_report() {
    let mut ledger = ReplyLedger::default();
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
    assert_eq!(ledger.filter_report("R".into()), "R");
}

#[test]
fn zle_history_recall_followed_by_an_xtversion_payload_is_dropped() {
    // zle bound ESC P to history-search-backward: a recalled line, then the payload.
    let replies = ["\x1bP>|Phosphor(0.1.12)", "\x1b\\"];
    assert_eq!(
        filtered(&replies, &[], "bash /tmp/x.sh>|Phosphor(0.1.12)"),
        ""
    );
    assert_eq!(
        filtered(
            &replies,
            &["touch f"],
            "bash /tmp/x.sh>|Phosphor(0.1.12)touch f"
        ),
        "touch f"
    );
}

#[test]
fn a_recalled_line_without_a_reply_payload_is_kept() {
    let replies = ["\x1bP>|Phosphor(0.1.12)", "\x1b\\"];
    assert_eq!(filtered(&replies, &[], "bash /tmp/x.sh"), "bash /tmp/x.sh");
    assert_eq!(filtered(&["\x1b[24;1R"], &[], "ls>|x"), "ls>|x");
}

fn ledger_with(replies: &[&str]) -> ReplyLedger {
    let mut ledger = ReplyLedger::default();
    for reply in replies {
        ledger.note_replies(reply.as_bytes());
    }
    ledger
}

#[test]
fn a_stale_reply_and_an_unrecorded_write_keep_the_report() {
    // An escape sequence in a write (an agent's input, a bracketed paste, an arrow key) cannot
    // be modelled: whatever the report holds might be it.
    for write in ["\x1b[200~R\x1b[201~", "\x1b[D", "\x03", "\t"] {
        let mut ledger = ledger_with(&["\x1b[24;1R"]);
        ledger.note_input(write.as_bytes());
        assert_eq!(ledger.filter_report("R".into()), "R", "{write:?}");
    }
    // Invalid UTF-8 (a character split across writes) cannot be modelled either.
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(&[0xe6, 0x97]);
    assert_eq!(ledger.filter_report("R".into()), "R");
}

#[test]
fn bytes_typed_while_a_report_is_in_flight_are_not_lost_to_the_next_report() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"abc");
    // The shell reported "ab"; "c" was typed after it, so the report does not end with "abc".
    assert_eq!(ledger.filter_report("ab".into()), "ab");
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("c".into()), "c");
}

#[test]
fn a_kept_report_leaves_the_line_untracked_until_it_is_known_to_be_empty() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"abc");
    assert_eq!(ledger.filter_report("ab".into()), "ab");
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "R");
    // Enter, or the kill prefix every command is written with, empties the line.
    ledger.note_input(b" \x0b\x15echo hi\r");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn a_report_that_is_exactly_what_was_written_resets_the_ledger() {
    let mut ledger = ledger_with(&[]);
    ledger.note_input(b"abc");
    assert_eq!(ledger.filter_report("abc".into()), "abc");
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn an_empty_report_with_nothing_written_resets_the_ledger() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    assert_eq!(ledger.filter_report(String::new()), "");
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn a_flood_of_replies_is_bounded() {
    let reply = "\x1b[1;1R";
    let replies = vec![reply; 100];
    assert_eq!(filtered(&replies, &[], &"R".repeat(64)), "");
    // Only the newest 64 are remembered; six of 70 residues are unexplained, so none is dropped.
    assert_eq!(filtered(&replies, &[], &"R".repeat(70)), "R".repeat(70));
}

#[test]
fn a_backslash_after_an_xtversion_reply() {
    let replies = ["\x1bP>|Phosphor(0.1.12)", "\x1b\\"];
    // The ST's tail is residue when nothing was typed...
    assert_eq!(filtered(&replies, &[], "\\"), "");
    // ...but a backslash the user typed was written, so it is kept.
    assert_eq!(filtered(&replies, &["\\"], "\\"), "\\");
    assert_eq!(filtered(&replies, &["\\"], ">|Phosphor(0.1.12)\\"), "\\");
}

#[test]
fn multi_byte_text_is_tracked() {
    assert_eq!(
        filtered(
            &["\x1b[24;1R"],
            &["h\u{e9}llo \u{65e5}\u{672c}"],
            "Rh\u{e9}llo \u{65e5}\u{672c}"
        ),
        "h\u{e9}llo \u{65e5}\u{672c}"
    );
    assert_eq!(
        filtered(
            &["\x1b[24;1R"],
            &["\u{65e5}", "\u{672c}"],
            "R\u{65e5}\u{672c}"
        ),
        "\u{65e5}\u{672c}"
    );
}

#[test]
fn a_reply_continued_in_a_second_write_is_one_reply() {
    let mut ledger = ReplyLedger::default();
    ledger.note_replies(b"\x1b[24;");
    ledger.note_replies(b"1R");
    assert_eq!(ledger.filter_report("1R".into()), "");
}

#[test]
fn starting_a_command_forgets_replies_from_before_it() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.on_command_start();
    assert_eq!(ledger.filter_report("R".into()), "R");
}

// One case per way bytes reach the pty. All of them go through `note_input` (the event loop's
// message channel is the only way in), so each is either recorded as text or marks the line
// untracked, and none can leave a stale reply able to explain text the user wrote.

#[test]
fn keys_released_from_a_hold_are_recorded() {
    // `HeldKeysRelease::ToPty` -> `write_to_pty`: plain bytes, like typed keys.
    assert_eq!(filtered(&["\x1b[24;1R"], &["R"], "R"), "R");
    assert_eq!(filtered(&["\x1b[24;1R"], &["R", "c"], "Rc"), "Rc");
}

#[test]
fn a_command_written_with_the_kill_prefix_leaves_a_tracked_empty_line() {
    let mut ledger = ledger_with(&[]);
    ledger.note_input(b"x");
    ledger.note_input(b" \x0b\x15echo hi\r");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn a_lone_kill_key_marks_the_line_untracked() {
    for write in [&b"\x0b"[..], b"\x15", b"\x0bx\x15", b"abc\x15"] {
        let mut ledger = ledger_with(&["\x1b[24;1R"]);
        ledger.note_input(write);
        assert_eq!(ledger.filter_report("R".into()), "R", "{write:?}");
    }
}

#[test]
fn the_input_report_request_adds_nothing_to_the_line() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"\x1bi");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn an_enter_alone_does_not_recover_tracking_but_a_command_write_does() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"\x1b[D");
    ledger.note_input(b"\r");
    ledger.on_command_start();
    assert_eq!(ledger.filter_report("R".into()), "R");
    ledger.note_replies(b"\x1b[24;1R");
    ledger.note_input(b" \x0b\x15ls\r");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn an_agents_input_with_an_escape_is_untracked_and_plain_agent_input_is_recorded() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"y");
    assert_eq!(ledger.filter_report("Ry".into()), "y");
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"\x1b[A");
    assert_eq!(ledger.filter_report("R".into()), "R");
}

/// The combined query burst: `printf '\e[c\e[>c\e[6n\e[5n\e]10;?\a\e]11;?\a\e]4;1;?\a\e[?u\e[>q
/// \e[14t\e[18t\eP$qm\e\\\e[?1$p'`.
const BURST: [&str; 14] = [
    "\x1b[?62;c",
    "\x1b[>0;95;0c",
    "\x1b[1;1R",
    "\x1b[0n",
    "\x1b]10;rgb:ffff/ffff/ffff\x07",
    "\x1b]11;rgb:0000/0000/0000\x07",
    "\x1b]4;1;rgb:cdcd/0000/0000\x07",
    "\x1b[?0u",
    "\x1bP>|Phosphor()\x1b\\",
    "\x1b[4;720;992t",
    "\x1b[8;45;124t",
    "\x1bP1$r0m\x1b\\",
    "\x1b[?1;2$y",
    "\x1b[?62;c",
];

#[test]
fn a_combined_query_burst_is_accounted_for_in_order() {
    // What readline left behind in the field report.
    let residue = "Rn0u>|Phosphor()720;992t;45;124t";
    assert_eq!(filtered(&BURST, &[], residue), "");
    assert_eq!(
        filtered(&BURST, &["ls -la"], &format!("{residue}ls -la")),
        "ls -la"
    );
    assert_eq!(
        filtered(&BURST, &["Rscript"], &format!("{residue}Rscript")),
        "Rscript"
    );
    // Whatever subset of the tails the shell left, in order.
    assert_eq!(filtered(&BURST, &[], "Rn0u"), "");
    assert_eq!(filtered(&BURST, &[], "720;992t;45;124t"), "");
    // Out of order is not residue of these replies (without a DCS reply, no junk is tolerated).
    let no_dcs: Vec<&str> = BURST
        .iter()
        .copied()
        .filter(|r| !r.starts_with("\x1bP"))
        .collect();
    assert_eq!(filtered(&no_dcs, &[], "nR"), "nR");
}

#[test]
fn a_combined_query_burst_behind_a_zle_history_recall() {
    let residue = "Rn0u>|Phosphor()720;992t;45;124t";
    assert_eq!(
        filtered(&BURST, &[], &format!("bash /tmp/x.sh{residue}")),
        ""
    );
    assert_eq!(
        filtered(
            &BURST,
            &["touch f"],
            &format!("bash /tmp/x.sh{residue}touch f")
        ),
        "touch f"
    );
    // Junk is only tolerated ahead of a device control string reply's residue.
    let no_dcs: Vec<&str> = BURST
        .iter()
        .copied()
        .filter(|r| !r.starts_with("\x1bP"))
        .collect();
    assert_eq!(
        filtered(&no_dcs, &[], "bash /tmp/x.sh720;992t"),
        "bash /tmp/x.sh720;992t"
    );
}

#[test]
fn unseen_text_ending_in_a_one_character_payload_tail_is_kept() {
    let replies = ["\x1bP>|Phosphor(0.1.12)", "\x1b\\"];
    assert_eq!(filtered(&replies, &["foo"], "echo (a)foo"), "echo (a)foo");
    assert_eq!(filtered(&replies, &[], "echo (a)"), "echo (a)");
    assert_eq!(filtered(&replies, &[], "ls \\"), "ls \\");
}

#[test]
fn a_line_put_back_by_histverify_is_not_residue() {
    // `!!` + Enter: the expansion is back on the line after the ledger saw an Enter.
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"!!\r");
    assert_eq!(ledger.filter_report("R".into()), "R");
    let mut ledger = ledger_with(&["\x1b[?62;c"]);
    ledger.note_input(b"!!\r");
    assert_eq!(ledger.filter_report("c".into()), "c");
}

#[test]
fn a_prefilled_line_is_not_residue() {
    // `print -z`, `read -e -i`, `fc`: text the app never wrote.
    let replies = ["\x1b[24;1R", "\x1bP>|Phosphor(0.1.12)", "\x1b\\"];
    assert_eq!(filtered(&replies, &[], "echo hi"), "echo hi");
    assert_eq!(filtered(&replies, &[], "make (all)"), "make (all)");
    assert_eq!(filtered(&replies, &["x"], "echo hi)x"), "echo hi)x");
}

#[test]
fn a_vi_mode_ctrl_u_that_left_text_behind_is_not_trusted() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    // The kill pair is written; zsh viins ^U left "ab" in the line. No command started, so the
    // emptiness is unconfirmed.
    ledger.note_input(b" \x0b\x15");
    assert_eq!(ledger.filter_report("abR".into()), "abR");
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b" \x0b\x15");
    assert_eq!(ledger.filter_report("R".into()), "R");
}

#[test]
fn a_huge_paste_without_a_newline_is_not_held_in_memory() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    let chunk = "a".repeat(1000);
    for _ in 0..8_000 {
        ledger.note_input(chunk.as_bytes());
        assert!(ledger.tracked_len() <= 4096 + 1000);
    }
    ledger.note_input("b".repeat(8 * 1024 * 1024).as_bytes());
    assert!(ledger.tracked_len() <= 4096);
    assert_eq!(ledger.filter_report("R".into()), "R");
    // A command write recovers tracking.
    ledger.note_input(b" \x0b\x15ls\r");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn a_forgotten_reply_cannot_explain_text() {
    let mut ledger = ledger_with(&["\x1b[24;1R", "\x1b[?62;c"]);
    ledger.forget_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "R");
}

#[test]
fn a_command_written_as_a_bracketed_paste_leaves_a_tracked_line() {
    // How commands reach a shell with bracketed paste: kill prefix, paste markers, Enter.
    let mut ledger = ledger_with(&[]);
    ledger.note_input(b" \x0b\x15");
    ledger.note_input(b"\x1b[200~bash x.sh\x1b[201~\r");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    ledger.note_input(b"Rx");
    assert_eq!(ledger.filter_report("RRx".into()), "Rx");
}

#[test]
fn bracketed_paste_markers_around_typed_text_are_transparent() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"\x1b[200~abc\x1b[201~");
    assert_eq!(ledger.filter_report("Rabc".into()), "abc");
}

#[test]
fn a_command_written_with_ctrl_p_in_bracketed_paste_leaves_a_tracked_line() {
    // What `bytes_to_execute_command` writes for bash and zsh.
    let mut ledger = ledger_with(&[]);
    ledger.note_input(b"\x10\x1b[200~bash x.sh\x1b[201~\n");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    ledger.note_input(b"Rx");
    assert_eq!(ledger.filter_report("RRx".into()), "Rx");
    // Without bracketed paste, and fish's trailing space + backspace.
    let mut ledger = ledger_with(&[]);
    ledger.note_input(b"\x10bash x.sh \x7f\r");
    ledger.on_command_start();
    ledger.note_replies(b"\x1b[24;1R");
    assert_eq!(ledger.filter_report("R".into()), "");
}

#[test]
fn ctrl_p_empties_a_line_the_app_did_not_expect_but_must_be_confirmed() {
    let mut ledger = ledger_with(&["\x1b[24;1R"]);
    ledger.note_input(b"\x1b[D\x10");
    assert_eq!(ledger.filter_report("R".into()), "R");
}

#[test]
fn backspace_removes_tracked_text() {
    assert_eq!(filtered(&["\x1b[24;1R"], &["abc", "\x7f"], "Rab"), "ab");
}

#[test]
fn powershell_input_report_request_adds_nothing_to_the_line() {
    let mut ledger = ledger_with(&["\x1b[1;1R"]);
    ledger.note_input(b"\x1b1");
    assert_eq!(ledger.filter_report("1;1R".into()), "");
}

#[test]
fn a_powershell_command_write_leaves_a_tracked_line() {
    // ESC 2 (kill line), bracketed paste, Enter.
    for write in [
        &b"\x1b2\x1b[200~Write-Output a\x1b[201~\r"[..],
        b"\x1b2Write-Output a\r",
    ] {
        let mut ledger = ledger_with(&[]);
        ledger.note_input(write);
        ledger.on_command_start();
        ledger.note_replies(b"\x1b[1;1R");
        ledger.note_replies(b"\x1bP>|Phosphor()\x1b\\");
        assert_eq!(
            ledger.filter_report("1;1R>|Phosphor()".into()),
            "",
            "{write:?}"
        );
    }
    // Typed fragments that look like residue are kept.
    for typed in ["Rscript", "62c", "1R", "1;2R"] {
        let mut ledger = ledger_with(&["\x1b[1;1R"]);
        ledger.note_input(b"\x1b2\r");
        ledger.on_command_start();
        ledger.note_replies(b"\x1b[1;1R");
        ledger.note_input(typed.as_bytes());
        assert_eq!(ledger.filter_report(typed.into()), typed);
    }
}

#[test]
fn osc_colour_reply_residue_without_its_escape_and_bel_is_dropped() {
    // PSReadLine (pwsh) swallows the ESC ] prefix and the BEL terminator (#943).
    let replies = [
        "\x1b]10;rgb:ffff/ffff/ffff\x07",
        "\x1b]11;rgb:0000/0000/0000\x07",
    ];
    assert_eq!(
        filtered(&replies, &[], "10;rgb:ffff/ffff/ffff11;rgb:0000/0000/0000"),
        ""
    );
    assert_eq!(filtered(&replies, &[], "10;rgb:ffff/ffff/ffff"), "");
    assert_eq!(filtered(&replies[1..], &[], "11;rgb:0000/0000/0000"), "");
    assert_eq!(
        filtered(
            &replies,
            &["Write-Output a"],
            "10;rgb:ffff/ffff/ffff11;rgb:0000/0000/0000Write-Output a"
        ),
        "Write-Output a"
    );
    // The BEL, if the editor kept it, is explained too.
    assert_eq!(
        filtered(
            &replies,
            &[],
            "10;rgb:ffff/ffff/ffff\x0711;rgb:0000/0000/0000\x07"
        ),
        ""
    );
}

#[test]
fn osc_reply_ended_by_st_is_explained_with_or_without_the_terminator() {
    let replies = ["\x1b]11;rgb:0000/0000/0000\x1b\\"];
    assert_eq!(filtered(&replies, &[], "11;rgb:0000/0000/0000"), "");
    assert_eq!(filtered(&replies, &[], "11;rgb:0000/0000/0000\\"), "");
}

#[test]
fn typed_text_that_resembles_an_osc_reply_is_not_dropped_without_a_reply() {
    // Nothing recorded: nothing is dropped, however much the text looks like residue.
    assert_eq!(
        filtered(&[], &[], "10;rgb:ffff/ffff/ffff"),
        "10;rgb:ffff/ffff/ffff"
    );
    // Typed by the user (so it is in the typed text), it is kept even beside a reply.
    let replies = ["\x1b]10;rgb:ffff/ffff/ffff\x07"];
    assert_eq!(
        filtered(
            &replies,
            &["10;rgb:ffff/ffff/ffff"],
            "10;rgb:ffff/ffff/ffff"
        ),
        "10;rgb:ffff/ffff/ffff"
    );
    // A different colour than the one queried is not residue of it.
    assert_eq!(
        filtered(&replies, &[], "10;rgb:0000/0000/0000"),
        "10;rgb:0000/0000/0000"
    );
}
