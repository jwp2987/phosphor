//! Truth tables for the subshell readiness judge (#1679 step 3). Every row of the redesign and the
//! review's corrections (R1-R9) is a case here; the exhaustive sweeps check the invariants over
//! every combination of the facts.

use std::time::Duration;

use super::{
    CancelKind, Decision, HeldKeysDest, LineEnd, LineEndVerdict, LineTracker, PRE_READY_CEILING,
    ReadinessSample, Tail, UserLedger, WaitWhy, WrittenCancel, WrittenSample, WrittenVerdict,
    cancel_flushes_unread_setup, held_keys_destination, judge_written, on_user_line_ended,
    subshell_ready,
};
use crate::terminal::model::subshell_tty::{LeaderKind, SubshellTtyKind, SubshellTtySample};
use crate::terminal::shell::ShellType;

fn tty(kind: SubshellTtyKind, leader: LeaderKind, comm: &str) -> SubshellTtySample {
    SubshellTtySample {
        kind,
        leader,
        leader_comm: (!comm.is_empty()).then(|| comm.to_owned()),
        canonical: Some(kind == SubshellTtyKind::Cooked),
        queue_empty: Some(true),
    }
}

fn sample(tty: SubshellTtySample) -> ReadinessSample {
    ReadinessSample {
        tty,
        block_unchanged: true,
        alt_screen: false,
        line_dirty: false,
        tail: Tail::ShellPrompt,
        prompt_moved_since_baseline: true,
        prompt_moved_since_line_end: true,
        elapsed: Duration::from_secs(1),
    }
}

fn no_bytes() -> UserLedger {
    UserLedger::default()
}

fn typed() -> UserLedger {
    UserLedger {
        bytes_since_arm: 5,
        lines_ended: 1,
        hop_changed_shell_to_relay: false,
    }
}

fn line_reader(comm: &str) -> ReadinessSample {
    sample(SubshellTtySample::line_reader(comm))
}

fn cooked(comm: &str) -> ReadinessSample {
    sample(tty(SubshellTtyKind::Cooked, LeaderKind::Shell, comm))
}

fn relay(comm: &str) -> ReadinessSample {
    let mut s = sample(tty(SubshellTtyKind::Relay, LeaderKind::Relay, comm));
    s.tty.canonical = Some(false);
    s
}

fn unknown(leader: LeaderKind, comm: &str) -> ReadinessSample {
    sample(tty(SubshellTtyKind::Unknown, leader, comm))
}

fn secret() -> ReadinessSample {
    sample(tty(SubshellTtyKind::Secret, LeaderKind::Unreadable, ""))
}

// ---- the redesign's rows ------------------------------------------------------------------

/// Row 1: a secret read is never ready, whatever else the sample says.
#[test]
fn row_1_secret_waits() {
    assert_eq!(
        subshell_ready(&secret(), &no_bytes(), Some(ShellType::Bash)),
        Decision::Wait(WaitWhy::Secret)
    );
    assert_eq!(
        subshell_ready(&secret(), &typed(), None),
        Decision::Wait(WaitWhy::Secret)
    );
}

/// Row 2: a local shell leader reading through its line editor, nothing passed through or the
/// queue proven empty: write, for the trigger's shell when the leader agrees with it.
#[test]
fn row_2_a_line_reader_with_nothing_unread_is_written() {
    assert_eq!(
        subshell_ready(&line_reader("bash"), &no_bytes(), Some(ShellType::Bash)),
        Decision::Write(Some(ShellType::Bash))
    );
    // Consumed: bytes passed through, the queue is empty (R1), the line is clean.
    assert_eq!(
        subshell_ready(&line_reader("-zsh"), &typed(), Some(ShellType::Zsh)),
        Decision::Write(Some(ShellType::Zsh))
    );
    // No new prompt is asked of a line editor: ^U redraws the line, not the prompt.
    let mut s = line_reader("zsh");
    s.prompt_moved_since_baseline = false;
    s.prompt_moved_since_line_end = false;
    s.tail = Tail::None;
    assert_eq!(
        subshell_ready(&s, &typed(), Some(ShellType::Zsh)),
        Decision::Write(Some(ShellType::Zsh))
    );
}

/// R1: the proof is the SLAVE INPUT QUEUE being empty; "unknown" is no proof.
#[test]
fn row_2_bytes_not_proven_consumed_wait() {
    for queue in [Some(false), None] {
        let mut s = line_reader("bash");
        s.tty.queue_empty = queue;
        assert_eq!(
            subshell_ready(&s, &typed(), Some(ShellType::Bash)),
            Decision::Wait(WaitWhy::NotConsumed),
            "{queue:?}"
        );
        // ... but with nothing typed the queue is irrelevant.
        assert_eq!(
            subshell_ready(&s, &no_bytes(), Some(ShellType::Bash)),
            Decision::Write(Some(ShellType::Bash)),
            "{queue:?}"
        );
    }
}

/// Row 3 (#1562 kept): a dirty line waits, whatever the fact.
#[test]
fn row_3_a_dirty_line_waits() {
    for mut s in [line_reader("bash"), cooked("dash"), relay("sudo")] {
        s.line_dirty = true;
        assert_eq!(
            subshell_ready(&s, &no_bytes(), None),
            Decision::Wait(WaitWhy::LineDirty)
        );
    }
}

/// Row 4 + R3: a cooked prompt is written into only for an editor-less shell, at a prompt drawn
/// since the baseline, through the probe path.
#[test]
fn row_4_a_cooked_editor_less_shell_at_a_new_prompt_is_written_via_the_probe() {
    for comm in ["dash", "sh", "ash", "busybox", "ksh", "mksh"] {
        assert_eq!(
            subshell_ready(&cooked(comm), &no_bytes(), Some(ShellType::Bash)),
            Decision::Write(None),
            "{comm}"
        );
        assert_eq!(
            subshell_ready(&cooked(comm), &typed(), None),
            Decision::Write(None),
            "{comm}: consumed behind the prompt"
        );
    }
}

/// #1847 (owner decision 2026-11-09): an idle editor-less shell with nothing typed is written to
/// without a moved prompt, and each condition alone, when false, restores the old wait.
#[test]
fn idle_untouched_editor_less_prompt_is_written_without_a_moved_prompt() {
    let mut s = cooked("dash");
    s.prompt_moved_since_baseline = false;
    s.prompt_moved_since_line_end = false;
    assert_eq!(subshell_ready(&s, &no_bytes(), None), Decision::Write(None));
    let waits = |s: &ReadinessSample, user: &UserLedger| {
        matches!(
            subshell_ready(s, user, None),
            Decision::Wait(WaitWhy::CookedNoPrompt | WaitWhy::InputPrompt)
        )
    };
    let mut t = s.clone();
    t.tail = Tail::None;
    assert!(waits(&t, &no_bytes()), "no prompt-shaped tail");
    let mut t = s.clone();
    t.tail = Tail::InputPrompt;
    assert!(waits(&t, &no_bytes()), "an input prompt");
    assert!(waits(&s, &typed()), "a byte was typed: today's rule");
    let mut t = s.clone();
    t.tty.leader = LeaderKind::Other;
    assert!(waits(&t, &no_bytes()), "a program, not the shell, in front");
    let mut t = s.clone();
    t.tty.queue_empty = Some(false);
    assert!(waits(&t, &no_bytes()), "unread bytes in the queue");
    let mut t = s.clone();
    t.tty.queue_empty = None;
    assert!(waits(&t, &no_bytes()), "queue not probed");
    let mut t = cooked("bash");
    t.prompt_moved_since_baseline = false;
    assert_eq!(
        subshell_ready(&t, &no_bytes(), None),
        Decision::Wait(WaitWhy::CookedReadBuiltin),
        "an editor shell at a cooked tty is a read builtin"
    );
    let mut t = s.clone();
    t.tty.kind = SubshellTtyKind::Relay;
    t.tty.canonical = Some(false);
    t.tty.leader = LeaderKind::Relay;
    assert!(
        matches!(
            subshell_ready(&t, &no_bytes(), None),
            Decision::Wait(WaitWhy::RelayNoPrompt)
        ),
        "a relay is not covered"
    );
}

/// R3: a cooked bash/zsh/fish/nu/pwsh is a `read` builtin: never written.
#[test]
fn r3_a_cooked_shell_with_a_line_editor_is_a_read_builtin_and_waits() {
    for comm in ["bash", "-zsh", "fish", "nu", "pwsh"] {
        assert_eq!(
            subshell_ready(&cooked(comm), &no_bytes(), Some(ShellType::Bash)),
            Decision::Wait(WaitWhy::CookedReadBuiltin),
            "{comm}"
        );
    }
}

/// Rows 5 and 6: a cooked shell at an input prompt, or with no new prompt, waits.
#[test]
fn rows_5_6_cooked_without_a_new_shell_prompt_waits() {
    let mut s = cooked("dash");
    s.tail = Tail::InputPrompt;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), None),
        Decision::Wait(WaitWhy::InputPrompt)
    );
    let mut s = cooked("dash");
    s.prompt_moved_since_baseline = false;
    // Owner decision 2026-11-09 (#1847): with nothing typed this idle prompt is written to, so
    // the wait is asserted for a ledger with typed bytes.
    assert_eq!(
        subshell_ready(&s, &typed(), None),
        Decision::Wait(WaitWhy::CookedNoPrompt)
    );
    let mut s = cooked("dash");
    s.tail = Tail::None;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), None),
        Decision::Wait(WaitWhy::CookedNoPrompt)
    );
}

/// Rows 7 and 8 + owner Q8: behind a cooked prompt or a relay, user bytes are consumed only when
/// the queue is empty AND a prompt was drawn since the last line end; either alone is not enough.
#[test]
fn rows_7_8_consumption_behind_a_prompt_needs_the_queue_and_a_new_prompt() {
    for base in [cooked("dash"), relay("sudo")] {
        let mut queue_only = base.clone();
        queue_only.prompt_moved_since_line_end = false;
        assert_eq!(
            subshell_ready(&queue_only, &typed(), None),
            Decision::Wait(WaitWhy::NotConsumed),
            "{:?}: a new prompt alone is not asked for, the queue alone is not enough",
            base.tty.kind
        );
        let mut prompt_only = base.clone();
        prompt_only.tty.queue_empty = None;
        assert_eq!(
            subshell_ready(&prompt_only, &typed(), None),
            Decision::Wait(WaitWhy::NotConsumed),
            "{:?}: a Mac with no queue probe never writes after bytes",
            base.tty.kind
        );
        assert!(matches!(
            subshell_ready(&base, &typed(), None),
            Decision::Write(_)
        ));
        // Nothing typed: the prompt shape alone writes.
        assert!(matches!(
            subshell_ready(&prompt_only, &no_bytes(), None),
            Decision::Write(_)
        ));
    }
}

/// Rows 9 and 10 + R4: a relay writes on the first new prompt shape, for the trigger's shell type,
/// only while the local tty is raw; otherwise it waits.
#[test]
fn rows_9_10_a_relay_writes_on_the_first_new_prompt_only_with_a_raw_local_tty() {
    for comm in ["sudo", "ssh", "docker", "su", "ksu", "podman"] {
        assert_eq!(
            subshell_ready(&relay(comm), &no_bytes(), Some(ShellType::Bash)),
            Decision::Write(Some(ShellType::Bash)),
            "{comm}"
        );
        assert_eq!(
            subshell_ready(&relay(comm), &no_bytes(), None),
            Decision::Write(None),
            "{comm}: the probe when the trigger named no shell"
        );
        let mut cooked_local = relay(comm);
        cooked_local.tty.canonical = Some(true);
        assert_eq!(
            subshell_ready(&cooked_local, &no_bytes(), None),
            Decision::Wait(WaitWhy::RelayCooked),
            "{comm}: docker exec -t without -i"
        );
        let mut unknown_local = relay(comm);
        unknown_local.tty.canonical = None;
        assert_eq!(
            subshell_ready(&unknown_local, &no_bytes(), None),
            Decision::Wait(WaitWhy::RelayCooked)
        );
        let mut no_prompt = relay(comm);
        no_prompt.prompt_moved_since_baseline = false;
        assert_eq!(
            subshell_ready(&no_prompt, &no_bytes(), None),
            Decision::Wait(WaitWhy::RelayNoPrompt)
        );
    }
}

/// Row 11: an unknown leader waits, and is given up as NotAShell at the ceiling; a shell or relay
/// leader at the ceiling is TooLong.
#[test]
fn row_11_13_unknown_waits_and_the_ceiling_names_the_cause() {
    for (leader, comm) in [
        (LeaderKind::Other, "vim"),
        (LeaderKind::Other, "fzf"),
        (LeaderKind::Parent, ""),
        (LeaderKind::Unreadable, ""),
    ] {
        let s = unknown(leader, comm);
        assert_eq!(
            subshell_ready(&s, &no_bytes(), None),
            Decision::Wait(WaitWhy::Unknown),
            "{leader:?}"
        );
        let mut at_ceiling = s.clone();
        at_ceiling.elapsed = PRE_READY_CEILING;
        assert_eq!(
            subshell_ready(&at_ceiling, &no_bytes(), None),
            Decision::Cancel(CancelKind::NotAShell),
            "{leader:?}"
        );
    }
    for mut s in [line_reader("bash"), cooked("bash"), relay("sudo"), secret()] {
        s.elapsed = PRE_READY_CEILING + Duration::from_millis(1);
        s.alt_screen = true;
        assert_eq!(
            subshell_ready(&s, &no_bytes(), None),
            Decision::Cancel(CancelKind::TooLong)
        );
    }
    let mut young = line_reader("bash");
    young.elapsed = PRE_READY_CEILING - Duration::from_millis(1);
    assert!(matches!(
        subshell_ready(&young, &no_bytes(), None),
        Decision::Write(_)
    ));
}

/// Row 14: the block changed beats everything.
#[test]
fn row_14_a_changed_block_is_gone() {
    for mut s in [
        line_reader("bash"),
        secret(),
        unknown(LeaderKind::Other, "vim"),
    ] {
        s.block_unchanged = false;
        s.elapsed = PRE_READY_CEILING * 2;
        assert_eq!(
            subshell_ready(&s, &typed(), None),
            Decision::Cancel(CancelKind::Gone)
        );
    }
}

/// Row 15: the alt screen waits (vim after `vim f` typeahead; fzf in the rc).
#[test]
fn row_15_alt_screen_waits() {
    for mut s in [line_reader("bash"), cooked("dash"), relay("sudo")] {
        s.alt_screen = true;
        assert_eq!(
            subshell_ready(&s, &no_bytes(), None),
            Decision::Wait(WaitWhy::AltScreen)
        );
    }
}

/// R5: a known-unsupported leader is given up at once, before any wait.
#[test]
fn r5_an_unsupported_leader_is_cancelled_at_once() {
    for comm in ["csh", "tcsh", "-tcsh", "xonsh", "elvish"] {
        let mut s = unknown(LeaderKind::Unsupported, comm);
        s.alt_screen = true;
        s.line_dirty = true;
        assert_eq!(
            subshell_ready(&s, &no_bytes(), Some(ShellType::Bash)),
            Decision::Cancel(CancelKind::Unsupported),
            "{comm}"
        );
    }
}

/// R5: `exec zsh` / `exec fish` in a `.bashrc`: the leader disagrees with the trigger, so the
/// probe is written and the shell names itself.
#[test]
fn r5_a_leader_that_disagrees_with_the_trigger_takes_the_probe_path() {
    assert_eq!(
        subshell_ready(&line_reader("zsh"), &no_bytes(), Some(ShellType::Bash)),
        Decision::Write(None)
    );
    assert_eq!(
        subshell_ready(&line_reader("fish"), &no_bytes(), Some(ShellType::Bash)),
        Decision::Write(None)
    );
    // bash started as `sh` reads through readline: the comm names no scripted shell, probe.
    assert_eq!(
        subshell_ready(&line_reader("sh"), &no_bytes(), Some(ShellType::Bash)),
        Decision::Write(None)
    );
    assert_eq!(
        subshell_ready(&line_reader("fish"), &no_bytes(), Some(ShellType::Fish)),
        Decision::Write(Some(ShellType::Fish))
    );
    assert_eq!(
        subshell_ready(&line_reader("nu"), &no_bytes(), Some(ShellType::Nu)),
        Decision::Write(Some(ShellType::Nu))
    );
    assert_eq!(
        subshell_ready(
            &line_reader("pwsh"),
            &no_bytes(),
            Some(ShellType::PowerShell)
        ),
        Decision::Write(Some(ShellType::PowerShell))
    );
}

/// R4 hop binding: a shell leader that gave way to a relay while the setup waited is not written
/// on the relay's shape.
#[test]
fn r4_an_unpredicted_shell_to_relay_hop_waits() {
    let hopped = UserLedger {
        hop_changed_shell_to_relay: true,
        ..no_bytes()
    };
    assert_eq!(
        subshell_ready(&relay("sudo"), &hopped, None),
        Decision::Wait(WaitWhy::Hop)
    );
    assert_eq!(
        subshell_ready(&line_reader("bash"), &hopped, None),
        Decision::Wait(WaitWhy::Hop)
    );
}

/// Precedence: Gone > ceiling > Unsupported > alt screen > Secret > dirty line > InputPrompt > hop
/// > the fact.
#[test]
fn the_preconditions_apply_in_order() {
    let mut s = secret();
    s.alt_screen = true;
    s.line_dirty = true;
    s.tail = Tail::InputPrompt;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), None),
        Decision::Wait(WaitWhy::AltScreen)
    );
    s.alt_screen = false;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), None),
        Decision::Wait(WaitWhy::Secret)
    );
    let mut s = line_reader("bash");
    s.line_dirty = true;
    s.tail = Tail::InputPrompt;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), None),
        Decision::Wait(WaitWhy::LineDirty)
    );
    s.line_dirty = false;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), None),
        Decision::Wait(WaitWhy::InputPrompt)
    );
}

// ---- the exhaustive sweep -----------------------------------------------------------------

fn every_sample() -> Vec<ReadinessSample> {
    let kinds = [
        (SubshellTtyKind::Secret, LeaderKind::Unreadable, ""),
        (SubshellTtyKind::LineReader, LeaderKind::Shell, "bash"),
        (SubshellTtyKind::LineReader, LeaderKind::Shell, "zsh"),
        (SubshellTtyKind::LineReader, LeaderKind::Shell, "sh"),
        (SubshellTtyKind::Cooked, LeaderKind::Shell, "bash"),
        (SubshellTtyKind::Cooked, LeaderKind::Shell, "dash"),
        (SubshellTtyKind::Relay, LeaderKind::Relay, "sudo"),
        (SubshellTtyKind::Unknown, LeaderKind::Other, "vim"),
        (SubshellTtyKind::Unknown, LeaderKind::Parent, ""),
        (SubshellTtyKind::Unknown, LeaderKind::Unsupported, "tcsh"),
        (SubshellTtyKind::Unknown, LeaderKind::Unreadable, ""),
    ];
    let mut out = Vec::new();
    for (kind, leader, comm) in kinds {
        for canonical in [Some(true), Some(false), None] {
            for queue_empty in [Some(true), Some(false), None] {
                for block_unchanged in [true, false] {
                    for alt_screen in [true, false] {
                        for line_dirty in [true, false] {
                            for tail in [Tail::None, Tail::ShellPrompt, Tail::InputPrompt] {
                                for moved in [true, false] {
                                    for moved_since_end in [true, false] {
                                        for elapsed in [Duration::from_secs(1), PRE_READY_CEILING] {
                                            out.push(ReadinessSample {
                                                tty: SubshellTtySample {
                                                    kind,
                                                    leader,
                                                    leader_comm: (!comm.is_empty())
                                                        .then(|| comm.to_owned()),
                                                    canonical,
                                                    queue_empty,
                                                },
                                                block_unchanged,
                                                alt_screen,
                                                line_dirty,
                                                tail,
                                                prompt_moved_since_baseline: moved,
                                                prompt_moved_since_line_end: moved_since_end,
                                                elapsed,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Over every combination of the facts: a write needs a shell leader at a raw tty or a new prompt
/// behind an editor-less cooked shell or a raw relay, the block unchanged, no alt screen, no
/// secret, a clean line, no input prompt, the user's bytes consumed, and a young attempt; nothing
/// else ever writes. The invariants, not the table rows.
#[test]
fn exhaustive_a_write_needs_every_precondition_and_a_proof() {
    let ledgers = [
        no_bytes(),
        typed(),
        UserLedger {
            hop_changed_shell_to_relay: true,
            ..typed()
        },
    ];
    let mut writes = 0;
    for s in every_sample() {
        for user in ledgers {
            for trigger in [None, Some(ShellType::Bash)] {
                let d = subshell_ready(&s, &user, trigger);
                if let Decision::Write(shell) = d {
                    writes += 1;
                    assert!(s.block_unchanged && !s.alt_screen && !s.line_dirty, "{s:?}");
                    assert_ne!(s.tail, Tail::InputPrompt, "{s:?}");
                    assert_ne!(s.tty.kind, SubshellTtyKind::Secret, "{s:?}");
                    assert_ne!(s.tty.kind, SubshellTtyKind::Unknown, "{s:?}");
                    assert!(s.elapsed < PRE_READY_CEILING, "{s:?}");
                    assert!(!user.hop_changed_shell_to_relay, "{s:?}");
                    let comm = s.tty.leader_comm.as_deref().unwrap_or_default();
                    match s.tty.kind {
                        SubshellTtyKind::LineReader => {
                            assert!(
                                user.bytes_since_arm == 0 || s.tty.queue_empty == Some(true),
                                "{s:?}"
                            );
                            if shell.is_some() {
                                assert_eq!(shell, trigger);
                            }
                        }
                        SubshellTtyKind::Cooked => {
                            assert!(["dash", "sh"].contains(&comm), "{s:?}");
                            assert_eq!(shell, None, "an editor-less shell takes the probe");
                        }
                        SubshellTtyKind::Relay => {
                            assert_eq!(s.tty.canonical, Some(false), "{s:?}");
                            assert_eq!(shell, trigger);
                        }
                        _ => unreachable!(),
                    }
                    if s.tty.kind != SubshellTtyKind::LineReader {
                        // #1847: the one write without a moved prompt is the idle, untouched,
                        // editor-less shell that is the tty's reader.
                        let idle = s.tty.kind == SubshellTtyKind::Cooked
                            && user.bytes_since_arm == 0
                            && s.tty.leader == LeaderKind::Shell
                            && s.tty.queue_empty == Some(true);
                        assert!(
                            s.tail == Tail::ShellPrompt && (s.prompt_moved_since_baseline || idle),
                            "{s:?}"
                        );
                        assert!(
                            user.bytes_since_arm == 0
                                || (s.tty.queue_empty == Some(true)
                                    && s.prompt_moved_since_line_end),
                            "{s:?}"
                        );
                    }
                }
                // The ceiling and a changed block always end the attempt.
                if !s.block_unchanged {
                    assert_eq!(d, Decision::Cancel(CancelKind::Gone));
                } else if s.elapsed >= PRE_READY_CEILING {
                    assert!(matches!(
                        d,
                        Decision::Cancel(CancelKind::TooLong | CancelKind::NotAShell)
                    ));
                } else if s.tty.leader == LeaderKind::Unsupported {
                    assert_eq!(d, Decision::Cancel(CancelKind::Unsupported));
                } else {
                    assert!(
                        !matches!(d, Decision::Cancel(_)),
                        "the judge never cancels a young attempt on a fact alone: {s:?} {d:?}"
                    );
                }
            }
        }
    }
    assert!(writes > 100, "the sweep reached the write rows: {writes}");
}

// ---- line ends (R2) -----------------------------------------------------------------------

#[test]
fn r2_only_an_empty_or_answering_line_re_arms() {
    use SubshellTtyKind::*;
    // Empty: always a ReArm.
    for kind in [Secret, LineReader, Cooked, Relay, Unknown] {
        assert_eq!(
            on_user_line_ended(false, kind, false, Tail::ShellPrompt, false),
            LineEndVerdict::ReArm
        );
    }
    // Non-empty answers.
    assert_eq!(
        on_user_line_ended(true, Secret, false, Tail::None, false),
        LineEndVerdict::ReArm
    );
    assert_eq!(
        on_user_line_ended(true, Cooked, true, Tail::None, false),
        LineEndVerdict::ReArm,
        "a cooked shell keeping up with its reads: a read builtin"
    );
    for kind in [LineReader, Relay, Unknown, Cooked] {
        assert_eq!(
            on_user_line_ended(true, kind, false, Tail::InputPrompt, false),
            LineEndVerdict::ReArm,
            "{kind:?} at an input prompt"
        );
    }
    // Non-empty lines that drove the shell.
    assert_eq!(
        on_user_line_ended(true, LineReader, false, Tail::ShellPrompt, false),
        LineEndVerdict::Cancel
    );
    assert_eq!(
        on_user_line_ended(true, Relay, false, Tail::ShellPrompt, false),
        LineEndVerdict::Cancel
    );
    assert_eq!(
        on_user_line_ended(true, Unknown, false, Tail::None, false),
        LineEndVerdict::Cancel
    );
    assert_eq!(
        on_user_line_ended(true, Cooked, false, Tail::ShellPrompt, false),
        LineEndVerdict::Cancel,
        "a cooked shell NOT keeping up: typeahead for its next prompt, the user drove"
    );
}

#[test]
fn r2_a_line_typed_into_a_program_that_owns_the_tty_re_arms() {
    use SubshellTtyKind::*;
    // vim's `:wq` + Enter: the line is the program's, the setup waits for the prompt after it.
    for tail in [Tail::None, Tail::ShellPrompt] {
        assert_eq!(
            on_user_line_ended(true, Unknown, false, tail, true),
            LineEndVerdict::ReArm
        );
    }
}

#[test]
fn the_line_tracker_counts_only_the_terminator_as_empty() {
    let mut t = LineTracker::default();
    assert_eq!(t.note(b"\r"), vec![LineEnd { non_empty: false }]);
    assert_eq!(t.note(b"ls"), vec![]);
    assert!(t.line_has_content());
    assert_eq!(t.note(b"\r"), vec![LineEnd { non_empty: true }]);
    assert!(!t.line_has_content());
    // ESC sequences, Tab, ^R, ^P, ^Y and a history recall are content (R2).
    for content in [
        b"\x1b[A\r".to_vec(),
        b"\t\r".to_vec(),
        b"\x12\r".to_vec(),
        b"\x10\r".to_vec(),
        b"\x19\r".to_vec(),
        b"!!\n".to_vec(),
        b"x\x7f\r".to_vec(),
    ] {
        let mut t = LineTracker::default();
        assert_eq!(
            t.note(&content),
            vec![LineEnd { non_empty: true }],
            "{content:?}"
        );
    }
    // ^U and ^C end the line empty; what was typed before them is gone.
    let mut t = LineTracker::default();
    assert_eq!(t.note(b"abc\x15"), vec![LineEnd { non_empty: false }]);
    assert_eq!(t.note(b"\r"), vec![LineEnd { non_empty: false }]);
    assert_eq!(t.note(b"abc\x03"), vec![LineEnd { non_empty: false }]);
    // Several ends in one chunk, in order.
    let mut t = LineTracker::default();
    assert_eq!(
        t.note(b"a\rb\r\r"),
        vec![
            LineEnd { non_empty: true },
            LineEnd { non_empty: true },
            LineEnd { non_empty: false }
        ]
    );
}

#[test]
fn the_line_tracker_understands_bracketed_paste_across_chunks() {
    let mut t = LineTracker::default();
    // A newline inside a paste is content, not a line end; the end bracket closes the paste.
    assert_eq!(t.note(b"\x1b[200~line one\nline two\x1b[201~"), vec![]);
    assert!(t.line_has_content());
    assert_eq!(t.note(b"\r"), vec![LineEnd { non_empty: true }]);
    // The brackets split across chunks.
    let mut t = LineTracker::default();
    assert_eq!(t.note(b"\x1b["), vec![]);
    assert_eq!(t.note(b"200~x\n"), vec![]);
    assert_eq!(t.note(b"y\x1b[2"), vec![]);
    assert_eq!(t.note(b"01~\r"), vec![LineEnd { non_empty: true }]);
    // An empty paste followed by Enter is an empty line.
    let mut t = LineTracker::default();
    assert_eq!(
        t.note(b"\x1b[200~\x1b[201~\r"),
        vec![LineEnd { non_empty: false }]
    );
    // A partial ESC that is not a paste bracket is content once completed.
    let mut t = LineTracker::default();
    assert_eq!(t.note(b"\x1b"), vec![]);
    assert_eq!(t.note(b"[A\r"), vec![LineEnd { non_empty: true }]);
}

// ---- Written (R7, R8, R9) -----------------------------------------------------------------

fn written() -> WrittenSample {
    WrittenSample {
        tty_kind: SubshellTtyKind::LineReader,
        block_unchanged: true,
        tail: Tail::None,
        prompt_moved_since_written: false,
        progress_seen: false,
        echo_ended: false,
        fresh_prompts_after_echo: 0,
        queue_empty: Some(true),
        relay: false,
    }
}

#[test]
fn judge_written_rows() {
    assert_eq!(judge_written(&written()), WrittenVerdict::Keep);
    // E-GONE beats everything.
    let mut s = written();
    s.block_unchanged = false;
    s.tty_kind = SubshellTtyKind::Secret;
    assert_eq!(judge_written(&s), WrittenVerdict::Gone);
    // #1840: once progress was seen, a changed block is the bootstrap's own InitShell block.
    let mut s = written();
    s.block_unchanged = false;
    s.progress_seen = true;
    assert_eq!(judge_written(&s), WrittenVerdict::Keep);
    // E-SECRET: in the same wakeup, whatever else.
    let mut s = written();
    s.tty_kind = SubshellTtyKind::Secret;
    s.progress_seen = true;
    assert_eq!(judge_written(&s), WrittenVerdict::Secret);
    // Progress clears every suspicion.
    let mut s = written();
    s.progress_seen = true;
    s.tail = Tail::InputPrompt;
    s.prompt_moved_since_written = true;
    s.echo_ended = true;
    s.fresh_prompts_after_echo = 3;
    assert_eq!(judge_written(&s), WrittenVerdict::Keep);
    // E-QUESTION: an input prompt drawn since the write, no marker.
    let mut s = written();
    s.tail = Tail::InputPrompt;
    s.prompt_moved_since_written = true;
    assert_eq!(judge_written(&s), WrittenVerdict::Question);
    // An input prompt that was already there (the baseline) is not the shell's answer.
    s.prompt_moved_since_written = false;
    assert_eq!(judge_written(&s), WrittenVerdict::Keep);
}

/// R7: E-UNSUPPORTED is stream-ordered: our echo ended, then a fresh prompt-shaped line, no marker
/// between. Local: confirmed by the queue-empty proof (no timer). Relay: by a second prompt.
#[test]
fn r7_unsupported_is_stream_ordered_and_confirmed_by_the_queue_or_a_second_prompt() {
    let mut s = written();
    s.tail = Tail::ShellPrompt;
    s.prompt_moved_since_written = true;
    // A prompt shape before our echo ended is the old prompt redrawn (a clock), not a verdict.
    assert_eq!(judge_written(&s), WrittenVerdict::Keep);
    s.echo_ended = true;
    assert_eq!(
        judge_written(&s),
        WrittenVerdict::Keep,
        "no fresh prompt after the echo yet"
    );
    s.fresh_prompts_after_echo = 1;
    assert_eq!(judge_written(&s), WrittenVerdict::Unsupported);
    s.queue_empty = Some(false);
    assert_eq!(judge_written(&s), WrittenVerdict::UnsupportedSuspect);
    s.queue_empty = None;
    assert_eq!(judge_written(&s), WrittenVerdict::UnsupportedSuspect);
    // Relay: the queue is the relay's; a second prompt confirms.
    let mut r = s;
    r.relay = true;
    r.queue_empty = Some(true);
    assert_eq!(judge_written(&r), WrittenVerdict::UnsupportedSuspect);
    r.fresh_prompts_after_echo = 2;
    assert_eq!(judge_written(&r), WrittenVerdict::Unsupported);
    // A finished line after the echo is not a prompt.
    let mut s = written();
    s.echo_ended = true;
    s.fresh_prompts_after_echo = 1;
    s.tail = Tail::None;
    assert_eq!(judge_written(&s), WrittenVerdict::Keep);
}

/// R8: where the held keys go, per cancel kind.
#[test]
fn r8_held_keys_go_to_the_editor_except_a_proven_unsupported_prompt() {
    for why in [
        WrittenCancel::Secret,
        WrittenCancel::Question,
        WrittenCancel::Net,
        WrittenCancel::Gone,
    ] {
        for proven in [true, false] {
            for consumed in [true, false] {
                assert_eq!(
                    held_keys_destination(why, proven, consumed),
                    HeldKeysDest::Editor,
                    "{why:?}"
                );
            }
        }
    }
    assert_eq!(
        held_keys_destination(WrittenCancel::Unsupported, true, true),
        HeldKeysDest::Pty
    );
    assert_eq!(
        held_keys_destination(WrittenCancel::Unsupported, true, false),
        HeldKeysDest::Editor
    );
    assert_eq!(
        held_keys_destination(WrittenCancel::Unsupported, false, true),
        HeldKeysDest::Editor
    );
}

/// R9: the flush happens only with the setup bytes provably unread.
#[test]
fn r9_a_cancel_flushes_only_provably_unread_setup_bytes() {
    assert!(cancel_flushes_unread_setup(Some(false), false));
    assert!(!cancel_flushes_unread_setup(Some(false), true));
    assert!(!cancel_flushes_unread_setup(Some(true), false));
    assert!(!cancel_flushes_unread_setup(None, false));
}
