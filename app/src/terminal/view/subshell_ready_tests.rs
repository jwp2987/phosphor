//! Truth tables for the subshell readiness judge (#1679 step 3). Every row of the redesign and the
//! review's corrections (R1-R9) is a case here; the exhaustive sweeps check the invariants over
//! every combination of the facts.

use std::time::Duration;

use super::{
    CancelKind, Decision, HeldKeysDest, LastUserLine, LineEnd, LineEndVerdict, LineStart,
    LineTracker, PRE_READY_CEILING, ReadinessSample, StageTwoPending, Tail, TypedAhead, UserLedger,
    WaitWhy, WrittenCancel, WrittenSample, WrittenVerdict, cancel_flushes_unread_setup,
    held_keys_destination, judge_written, key_read_took_the_typed_keys, last_user_line_after_ends,
    line_editor_announcement_enabled_from_env, line_end_is_typed_ahead, on_user_line_ended,
    row_shows_typed_line, stage_two_pending, subshell_ready, typed_ahead_after_look,
    user_line_may_be_at_cursor,
};
use super::{
    LineEditorWrite, cancel_flushes_unread_line_editor_write, is_line_editor_write,
    line_editor_flush_needs_the_count, line_editor_took_the_line, row_shows_written_line,
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
        reader_blocked: None,
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
        newline_since_line_end: false,
        elapsed: Duration::from_secs(1),
        no_tty_probe: false,
        user_line_may_be_at_cursor: false,
        line_editor_announced: false,
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
        typed_ahead: TypedAhead::None,
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
    // No MOVED prompt is asked of a line editor: ^U redraws the line, not the prompt, and a click
    // at an idle prompt finds it where it was. The prompt shape itself is asked for (#1934).
    let mut s = line_reader("zsh");
    s.prompt_moved_since_baseline = false;
    s.prompt_moved_since_line_end = false;
    assert_eq!(
        subshell_ready(&s, &typed(), Some(ShellType::Zsh)),
        Decision::Write(Some(ShellType::Zsh))
    );
    assert_eq!(
        subshell_ready(&s, &no_bytes(), Some(ShellType::Zsh)),
        Decision::Write(Some(ShellType::Zsh))
    );
}

/// #1934 (review r8 D1a): a line-editor shell with NO prompt shape at the cursor waits. zsh's own
/// `read -k` / `read -q` / `vared` / `select` and zsh-newuser-install's menu are a Shell leader on
/// a raw tty too; the prompt on the cursor's row is the evidence that the line editor reads. An
/// input prompt there is the earlier veto; the queue proof still applies behind a prompt.
#[test]
fn d1_a_line_reader_without_a_prompt_shape_waits() {
    for user in [no_bytes(), typed()] {
        let mut s = line_reader("zsh");
        s.tail = Tail::None;
        assert_eq!(
            subshell_ready(&s, &user, Some(ShellType::Zsh)),
            Decision::Wait(WaitWhy::NoPrompt),
            "{user:?}"
        );
        s.tail = Tail::InputPrompt;
        assert_eq!(
            subshell_ready(&s, &user, Some(ShellType::Zsh)),
            Decision::Wait(WaitWhy::InputPrompt),
            "{user:?}"
        );
    }
    // A prompt at the cursor, unmoved since the baseline, nothing typed: the idle click writes.
    let mut s = line_reader("zsh");
    s.prompt_moved_since_baseline = false;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), Some(ShellType::Zsh)),
        Decision::Write(Some(ShellType::Zsh))
    );
    // Typed bytes behind a prompt need the queue proof, as before.
    s.tty.queue_empty = Some(false);
    assert_eq!(
        subshell_ready(&s, &typed(), Some(ShellType::Zsh)),
        Decision::Wait(WaitWhy::NotConsumed)
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
    // #1935 (review r8 D2), CHANGED EXPECTATION: no prompt-shaped tail (an empty PS1) used to
    // wait; the idle reader with nothing unread is written to without any prompt.
    let mut t = s.clone();
    t.tail = Tail::None;
    assert_eq!(subshell_ready(&t, &no_bytes(), None), Decision::Write(None));
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
    // #1935 (review r8 D2), CHANGED EXPECTATION: this row asserted Wait(CookedNoPrompt) for an
    // untouched dash with no prompt drawn; it is written to now (see d2_* below). With bytes
    // typed since arming the prompt is still asked for.
    let mut s = cooked("dash");
    s.tail = Tail::None;
    assert_eq!(subshell_ready(&s, &no_bytes(), None), Decision::Write(None));
    assert_eq!(
        subshell_ready(&s, &typed(), None),
        Decision::Wait(WaitWhy::CookedNoPrompt)
    );
}

/// #1935 (review r8 D2): the truth rows of an editor-less cooked shell that draws no prompt (dash
/// with an inherited empty PS1: cursor at column 0, `Tail::None`, nothing ever moves).
#[test]
fn d2_an_idle_editor_less_shell_needs_no_prompt_only_an_empty_queue() {
    for comm in ["dash", "sh", "ash", "busybox", "ksh"] {
        let mut s = cooked(comm);
        s.tail = Tail::None;
        s.prompt_moved_since_baseline = false;
        s.prompt_moved_since_line_end = false;
        assert_eq!(
            subshell_ready(&s, &no_bytes(), None),
            Decision::Write(None),
            "{comm}: idle reader, queue empty, nothing typed"
        );
        let mut t = s.clone();
        t.tail = Tail::InputPrompt;
        assert_eq!(
            subshell_ready(&t, &no_bytes(), None),
            Decision::Wait(WaitWhy::InputPrompt),
            "{comm}: a question at the tail"
        );
        for queue in [Some(false), None] {
            let mut t = s.clone();
            t.tty.queue_empty = queue;
            assert_eq!(
                subshell_ready(&t, &no_bytes(), None),
                Decision::Wait(WaitWhy::CookedNoPrompt),
                "{comm}: queue {queue:?}"
            );
        }
        let mut t = s.clone();
        t.tty.leader = LeaderKind::Other;
        assert_eq!(
            subshell_ready(&t, &no_bytes(), None),
            Decision::Wait(WaitWhy::CookedNoPrompt),
            "{comm}: a program in front"
        );
        // Bytes passed through: the accepted loss, there is no visible proof they were run.
        assert_eq!(
            subshell_ready(&s, &typed(), None),
            Decision::Wait(WaitWhy::CookedNoPrompt),
            "{comm}: typed since arming, no prompt"
        );
    }
    // Unread bytes with a NEW prompt and nothing typed since arming (typed before the click):
    // not consumed. The old rule wrote here.
    let mut s = cooked("dash");
    s.tty.queue_empty = Some(false);
    assert_eq!(
        subshell_ready(&s, &no_bytes(), None),
        Decision::Wait(WaitWhy::NotConsumed)
    );
    // The queue cannot be probed: the old proof (a new prompt) stands.
    let mut s = cooked("dash");
    s.tty.queue_empty = None;
    assert_eq!(subshell_ready(&s, &no_bytes(), None), Decision::Write(None));
}

/// #1935, "reader idle" (owner decision 2026-11-09): an editor-less cooked shell the user typed
/// into since arming, with no prompt to prove anything (an empty PS1), is written to when the OS
/// says it took every line and is blocked in its read again (Linux: `reader_blocked`), the queue is
/// empty and the last line end reached the tty (its newline was echoed). Any one fact missing
/// keeps the old answer; where the fact does not exist (`None`: macOS, no ptrace access) nothing
/// changes.
#[test]
fn reader_idle_an_editor_less_shell_blocked_in_read_is_written_after_the_users_lines() {
    for comm in ["dash", "sh", "ash", "busybox", "ksh"] {
        let mut s = cooked(comm);
        s.tail = Tail::None;
        s.prompt_moved_since_baseline = false;
        s.prompt_moved_since_line_end = false;
        s.newline_since_line_end = true;
        s.tty.reader_blocked = Some(true);
        assert_eq!(
            subshell_ready(&s, &typed(), None),
            Decision::Write(None),
            "{comm}: typed into, queue empty, blocked in read, the Enter echoed"
        );
        // The old answer for each missing fact.
        for blocked in [Some(false), None] {
            let mut t = s.clone();
            t.tty.reader_blocked = blocked;
            assert_eq!(
                subshell_ready(&t, &typed(), None),
                Decision::Wait(WaitWhy::CookedNoPrompt),
                "{comm}: reader_blocked {blocked:?} (running a command, or no such fact here)"
            );
        }
        for queue in [Some(false), None] {
            let mut t = s.clone();
            t.tty.queue_empty = queue;
            assert_eq!(
                subshell_ready(&t, &typed(), None),
                Decision::Wait(WaitWhy::CookedNoPrompt),
                "{comm}: queue {queue:?}"
            );
        }
        let mut t = s.clone();
        t.newline_since_line_end = false;
        assert_eq!(
            subshell_ready(&t, &typed(), None),
            Decision::Wait(WaitWhy::CookedNoPrompt),
            "{comm}: the line end is not known to have reached the tty"
        );
        let mut t = s.clone();
        t.tty.leader = LeaderKind::Other;
        assert_eq!(
            subshell_ready(&t, &typed(), None),
            Decision::Wait(WaitWhy::CookedNoPrompt),
            "{comm}: a program in front"
        );
        // Bytes with no line end at all (the ledger counts lines): the old rule.
        let no_line = UserLedger {
            lines_ended: 0,
            ..typed()
        };
        assert_eq!(
            subshell_ready(&s, &no_line, None),
            Decision::Wait(WaitWhy::CookedNoPrompt),
            "{comm}: typed, never ended"
        );
        // The vetoes that run before the rule still win.
        let mut t = s.clone();
        t.tail = Tail::InputPrompt;
        assert_eq!(
            subshell_ready(&t, &typed(), None),
            Decision::Wait(WaitWhy::InputPrompt),
            "{comm}: a question at the tail is the shell's own `read`"
        );
        let mut t = s.clone();
        t.line_dirty = true;
        assert_eq!(
            subshell_ready(&t, &typed(), None),
            Decision::Wait(WaitWhy::LineDirty),
            "{comm}: typed text sits in the line"
        );
        let mut t = s.clone();
        t.alt_screen = true;
        assert_eq!(
            subshell_ready(&t, &typed(), None),
            Decision::Wait(WaitWhy::AltScreen),
            "{comm}"
        );
    }
    // A shell with a line editor at a cooked tty is a `read` builtin whatever it is blocked in.
    for comm in ["bash", "zsh", "fish"] {
        let mut s = cooked(comm);
        s.tail = Tail::None;
        s.newline_since_line_end = true;
        s.tty.reader_blocked = Some(true);
        assert_eq!(
            subshell_ready(&s, &typed(), None),
            Decision::Wait(WaitWhy::CookedReadBuiltin),
            "{comm}"
        );
    }
    // The fact adds nothing to a line reader or a relay.
    let mut s = line_reader("bash");
    s.tail = Tail::None;
    s.newline_since_line_end = true;
    s.tty.reader_blocked = Some(true);
    assert_eq!(
        subshell_ready(&s, &typed(), Some(ShellType::Bash)),
        Decision::Wait(WaitWhy::NoPrompt)
    );
    let mut s = relay("sudo");
    s.tty.canonical = Some(false);
    s.tail = Tail::None;
    s.prompt_moved_since_baseline = false;
    s.newline_since_line_end = true;
    s.tty.reader_blocked = Some(true);
    assert_eq!(
        subshell_ready(&s, &typed(), None),
        Decision::Wait(WaitWhy::RelayNoPrompt)
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
                                                    reader_blocked: None,
                                                },
                                                block_unchanged,
                                                alt_screen,
                                                line_dirty,
                                                tail,
                                                prompt_moved_since_baseline: moved,
                                                prompt_moved_since_line_end: moved_since_end,
                                                newline_since_line_end: false,
                                                elapsed,
                                                no_tty_probe: false,
                                                user_line_may_be_at_cursor: false,
                                                line_editor_announced: false,
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

/// Over every combination of the facts: a write needs a shell leader at a raw tty with a prompt
/// shape at the cursor (#1934), or a new prompt behind an editor-less cooked shell or a raw relay, the block unchanged, no alt screen, no
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
                            // #1934: the prompt shape at the cursor, moved or not.
                            assert_eq!(s.tail, Tail::ShellPrompt, "{s:?}");
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
                        // #1847 / #1935: the one write without a moved prompt, or without any
                        // prompt, is the idle, untouched, editor-less shell that is the tty's
                        // reader with nothing unread.
                        let idle = s.tty.kind == SubshellTtyKind::Cooked
                            && user.bytes_since_arm == 0
                            && s.tty.leader == LeaderKind::Shell
                            && s.tty.queue_empty == Some(true);
                        assert!(
                            idle || (s.tail == Tail::ShellPrompt && s.prompt_moved_since_baseline),
                            "{s:?}"
                        );
                        if s.tty.kind == SubshellTtyKind::Cooked && user.bytes_since_arm == 0 {
                            assert_eq!(s.tty.leader, LeaderKind::Shell, "{s:?}");
                            assert_ne!(s.tty.queue_empty, Some(false), "{s:?}");
                        }
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
    // The rows this test had, for every cancel that may flush (the signature gained the cancel's
    // kind and the user's bytes, #1940; the expectations are the same).
    for why in [
        WrittenCancel::Question,
        WrittenCancel::Net,
        WrittenCancel::Gone,
        WrittenCancel::Unsupported,
    ] {
        assert!(
            cancel_flushes_unread_setup(why, Some(false), false, 0),
            "{why:?}"
        );
        assert!(
            !cancel_flushes_unread_setup(why, Some(false), true, 0),
            "{why:?}"
        );
        assert!(
            !cancel_flushes_unread_setup(why, Some(true), false, 0),
            "{why:?}"
        );
        assert!(!cancel_flushes_unread_setup(why, None, false, 0), "{why:?}");
    }
}

/// #1940 (review r8 D9): the flush never rests on the echo filter alone. Whatever the filter saw
/// (`echo_started` false is also what a MISSED echo looks like) and whatever the queue holds, a
/// Secret cancel never flushes (the unread bytes may be what the user types for the secret), and
/// no cancel flushes once a user byte went to the pty since the setup's first line.
#[test]
fn d9_a_secret_cancel_or_user_bytes_since_the_write_never_flush() {
    for queue in [Some(false), Some(true), None] {
        for echo_started in [false, true] {
            for user_bytes in [0, 1, 40] {
                assert!(
                    !cancel_flushes_unread_setup(
                        WrittenCancel::Secret,
                        queue,
                        echo_started,
                        user_bytes
                    ),
                    "Secret: queue {queue:?} echo {echo_started} bytes {user_bytes}"
                );
            }
            for why in [
                WrittenCancel::Question,
                WrittenCancel::Net,
                WrittenCancel::Gone,
                WrittenCancel::Unsupported,
            ] {
                for user_bytes in [1, 40] {
                    assert!(
                        !cancel_flushes_unread_setup(why, queue, echo_started, user_bytes),
                        "{why:?}: queue {queue:?} echo {echo_started} bytes {user_bytes}"
                    );
                }
            }
        }
    }
}

/// #2006, round 10 Windows Git Bash (`subshell_ready_rc_clear`, agent pass): the inner bash drew
/// its prompt before the setup was armed, so the cursor never moved "since the baseline" and
/// the probe-less judge waited until the user's own line cancelled the attempt. A line editor
/// that announced itself (bracketed paste on) under a shell-shaped tail is the positive fact:
/// it writes, but only while no user byte passed, never at a question, never with a probe's
/// sample, and the rules for user bytes are untouched.
#[test]
fn no_probe_writes_at_a_prompt_drawn_before_arming_when_the_line_editor_announced_itself() {
    let mut s = unknown(LeaderKind::Unreadable, "");
    s.no_tty_probe = true;
    s.prompt_moved_since_baseline = false;
    s.prompt_moved_since_line_end = false;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), Some(ShellType::Bash)),
        Decision::Wait(WaitWhy::Unknown),
        "no movement and no announcement: nothing says a line editor reads"
    );
    s.line_editor_announced = true;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), Some(ShellType::Bash)),
        Decision::Write(None),
        "the line editor announced itself at a shell-shaped tail"
    );
    assert_eq!(
        subshell_ready(&s, &typed(), Some(ShellType::Bash)),
        Decision::Wait(WaitWhy::Unknown),
        "after user bytes the announcement proves nothing about THEIR line: the old rules"
    );
    let mut question = s.clone();
    question.tail = Tail::InputPrompt;
    assert_eq!(
        subshell_ready(&question, &no_bytes(), None),
        Decision::Wait(WaitWhy::InputPrompt)
    );
    let mut blank = s.clone();
    blank.tail = Tail::None;
    assert_eq!(
        subshell_ready(&blank, &no_bytes(), None),
        Decision::Wait(WaitWhy::Unknown),
        "an announcement without a prompt on screen writes nothing"
    );
    let mut dirty = s.clone();
    dirty.line_dirty = true;
    assert_eq!(
        subshell_ready(&dirty, &no_bytes(), None),
        Decision::Wait(WaitWhy::LineDirty)
    );
    let mut alt = s.clone();
    alt.alt_screen = true;
    assert_eq!(
        subshell_ready(&alt, &no_bytes(), None),
        Decision::Wait(WaitWhy::AltScreen)
    );
}

#[test]
fn the_line_editor_announcement_is_on_unless_switched_off() {
    assert!(line_editor_announcement_enabled_from_env(None));
    assert!(line_editor_announcement_enabled_from_env(Some("1")));
    assert!(!line_editor_announcement_enabled_from_env(Some("0")));
    assert!(!line_editor_announcement_enabled_from_env(Some(" No ")));
}

/// #1886: with no tty probe (ConPTY) the screen facts decide: a shell prompt drawn since the
/// baseline writes; no prompt, an input prompt or an unproven user byte waits; the ceiling still
/// ends the attempt. A probe that exists but found nothing keeps waiting.
#[test]
fn no_probe_judges_from_screen_facts() {
    let mut s = unknown(LeaderKind::Unreadable, "");
    s.no_tty_probe = true;
    assert_eq!(
        subshell_ready(&s, &no_bytes(), Some(ShellType::Bash)),
        Decision::Write(None),
        "the leader names no shell, so the sh probe writes"
    );
    let mut moved_not = s.clone();
    moved_not.prompt_moved_since_baseline = false;
    assert_eq!(
        subshell_ready(&moved_not, &no_bytes(), None),
        Decision::Wait(WaitWhy::Unknown)
    );
    let mut input = s.clone();
    input.tail = Tail::InputPrompt;
    assert_eq!(
        subshell_ready(&input, &no_bytes(), None),
        Decision::Wait(WaitWhy::InputPrompt)
    );
    let mut unproven = s.clone();
    unproven.prompt_moved_since_line_end = false;
    assert_eq!(
        subshell_ready(&unproven, &typed(), None),
        Decision::Wait(WaitWhy::Unknown)
    );
    assert_eq!(
        subshell_ready(&s, &typed(), None),
        Decision::Write(None),
        "a prompt drawn since the user's last line end proves the bytes were read"
    );
    // #2006: the cursor's row may still show the user's own line (its late echo, the line
    // editor's redisplay): no proof that a prompt was drawn after the line was consumed.
    let mut echo = s.clone();
    echo.user_line_may_be_at_cursor = true;
    assert_eq!(
        subshell_ready(&echo, &typed(), None),
        Decision::Wait(WaitWhy::UserLineAtCursor),
        "`Name: me` after the Enter is the user's echo, not a prompt"
    );
    assert_eq!(
        subshell_ready(&echo, &no_bytes(), None),
        Decision::Write(None),
        "nothing typed: there is no user line to mistake"
    );
    let mut probed = s.clone();
    probed.no_tty_probe = false;
    assert_eq!(
        subshell_ready(&probed, &no_bytes(), None),
        Decision::Wait(WaitWhy::Unknown)
    );
    let mut late = s;
    late.elapsed = PRE_READY_CEILING;
    assert!(matches!(
        subshell_ready(&late, &no_bytes(), None),
        Decision::Cancel(_)
    ));
}

/// #1905: stage 2 follows a prompt drawn after the marker, never the marker alone, and never on a
/// tty a secret or a `read` builtin owns.
#[test]
fn stage_two_may_follow_only_a_prompt_after_the_marker_on_a_non_secret_tty() {
    use super::stage_two_may_follow;
    let reading = tty(SubshellTtyKind::LineReader, LeaderKind::Shell, "bash");
    let cooked = tty(SubshellTtyKind::Cooked, LeaderKind::Shell, "bash");
    let secret = tty(SubshellTtyKind::Secret, LeaderKind::Unreadable, "");
    let unknown = SubshellTtySample::unknown();
    // The ready row.
    assert!(stage_two_may_follow(
        Tail::ShellPrompt,
        false,
        true,
        &reading
    ));
    // No text since the marker (the old prompt is still the tail), no prompt, a question.
    assert!(!stage_two_may_follow(
        Tail::ShellPrompt,
        false,
        false,
        &reading
    ));
    assert!(!stage_two_may_follow(Tail::None, false, true, &reading));
    assert!(!stage_two_may_follow(
        Tail::InputPrompt,
        false,
        true,
        &reading
    ));
    assert!(!stage_two_may_follow(
        Tail::ShellPrompt,
        true,
        true,
        &reading
    ));
    // A secret read, or a `read` builtin of a line-editor shell.
    assert!(!stage_two_may_follow(
        Tail::ShellPrompt,
        false,
        true,
        &secret
    ));
    assert!(!stage_two_may_follow(
        Tail::ShellPrompt,
        false,
        true,
        &cooked
    ));
    // No OS fact: the prompt shape is the evidence there is.
    assert!(stage_two_may_follow(
        Tail::ShellPrompt,
        false,
        true,
        &unknown
    ));
    // Review r8 addendum 3: a program in front (vim, fzf) is never written to, prompt-shaped
    // last line or not.
    let program = tty(SubshellTtyKind::Unknown, LeaderKind::Other, "vim");
    assert!(!stage_two_may_follow(
        Tail::ShellPrompt,
        false,
        true,
        &program
    ));
    let cooked_program = tty(SubshellTtyKind::Cooked, LeaderKind::Other, "cat");
    assert!(!stage_two_may_follow(
        Tail::None,
        false,
        true,
        &cooked_program
    ));
    // Addendum 4 (#1935): an editor-less cooked shell with an empty PS1 draws nothing after the
    // marker; nothing unread in the queue is its proof, a question still vetoes, and so do unread
    // bytes without a prompt.
    let dash = tty(SubshellTtyKind::Cooked, LeaderKind::Shell, "dash");
    assert!(stage_two_may_follow(Tail::None, false, false, &dash));
    assert!(stage_two_may_follow(Tail::ShellPrompt, false, true, &dash));
    assert!(!stage_two_may_follow(Tail::InputPrompt, false, true, &dash));
    assert!(!stage_two_may_follow(Tail::None, true, false, &dash));
    let mut busy = dash.clone();
    busy.queue_empty = Some(false);
    assert!(!stage_two_may_follow(Tail::None, false, false, &busy));
    busy.queue_empty = None;
    assert!(!stage_two_may_follow(Tail::None, false, false, &busy));
    assert!(stage_two_may_follow(Tail::ShellPrompt, false, true, &busy));
}

/// #1905's first line (review r8 D3): the truth rows of a stage 2 waiting for its first line.
#[test]
fn stage_two_pending_rows() {
    use StageTwoPending::*;
    let reading = tty(SubshellTtyKind::LineReader, LeaderKind::Shell, "bash");
    let secret = tty(SubshellTtyKind::Secret, LeaderKind::Unreadable, "");
    let cooked = tty(SubshellTtyKind::Cooked, LeaderKind::Shell, "bash");
    let program = tty(SubshellTtyKind::Unknown, LeaderKind::Other, "fzf");
    let p = Tail::ShellPrompt;
    // A prompt drawn after the hook, the editor reading: one write.
    assert_eq!(stage_two_pending(p, false, true, false, &reading), Write);
    // Nothing drawn after the hook yet (the old prompt is still the tail), a finished line.
    assert_eq!(stage_two_pending(p, false, false, false, &reading), Wait);
    assert_eq!(
        stage_two_pending(Tail::None, false, true, false, &reading),
        Wait
    );
    // A secret read wins over everything, drawn prompt or not, alt screen or not.
    for (tail, text, alt) in [
        (p, true, false),
        (Tail::None, false, false),
        (p, true, true),
    ] {
        assert_eq!(stage_two_pending(tail, false, text, alt, &secret), Secret);
    }
    // A question drawn after the hook; the same shape from BEFORE the hook is not one.
    assert_eq!(
        stage_two_pending(Tail::InputPrompt, false, true, false, &cooked),
        Question
    );
    assert_eq!(
        stage_two_pending(Tail::InputPrompt, false, false, false, &cooked),
        Wait
    );
    assert_eq!(
        stage_two_pending(Tail::InputPrompt, true, true, false, &cooked),
        Wait
    );
    // The alt screen, a `read` builtin's cooked tty, a program in front: wait.
    assert_eq!(stage_two_pending(p, false, true, true, &reading), Wait);
    assert_eq!(stage_two_pending(p, false, true, false, &cooked), Wait);
    assert_eq!(stage_two_pending(p, false, true, false, &program), Wait);
}

/// #1941 (review r8 D4): a line typed AHEAD of the line editor. Round 8, agent pass: `vim f` +
/// Enter typed into a bash whose rc slept; readline came up, read the line, and for 0.7 ms the
/// facts were "line editor, prompt shape, queue empty": the probe was written there and went into
/// vim. The line editor row waits while such a line is outstanding.
#[test]
fn d4_a_line_typed_ahead_of_the_line_editor_blocks_the_write_until_it_was_accepted() {
    let rc = tty(SubshellTtyKind::Cooked, LeaderKind::Shell, "bash");
    // The line end: typed ahead only when non-empty, on a cooked line-editor shell, no question.
    assert!(line_end_is_typed_ahead(true, &rc, Tail::None));
    assert!(line_end_is_typed_ahead(true, &rc, Tail::ShellPrompt));
    assert!(!line_end_is_typed_ahead(false, &rc, Tail::None));
    assert!(!line_end_is_typed_ahead(true, &rc, Tail::InputPrompt));
    assert!(!line_end_is_typed_ahead(
        true,
        &tty(SubshellTtyKind::Cooked, LeaderKind::Shell, "dash"),
        Tail::None
    ));
    assert!(!line_end_is_typed_ahead(
        true,
        &SubshellTtySample::line_reader("bash"),
        Tail::ShellPrompt
    ));
    assert!(!line_end_is_typed_ahead(
        true,
        &tty(SubshellTtyKind::Unknown, LeaderKind::Other, "vim"),
        Tail::None
    ));
    // The looks. Unread: the line sits in the queue of the sleeping rc.
    let mut unread_rc = rc.clone();
    unread_rc.queue_empty = Some(false);
    let editor = SubshellTtySample::line_reader("bash");
    let vim = tty(SubshellTtyKind::Unknown, LeaderKind::Other, "vim");
    let p = Tail::ShellPrompt;
    let (none, unread, runs) = (TypedAhead::None, TypedAhead::Unread, TypedAhead::EditorRuns);
    let look = typed_ahead_after_look;
    let (shown, gone, opaque) = (Some(true), Some(false), Option::<bool>::None);
    assert_eq!(look(none, &editor, p, shown, 1), none);
    assert_eq!(look(unread, &unread_rc, Tail::None, gone, 1), unread);
    // A `read` builtin took it: nothing will run.
    assert_eq!(look(unread, &rc, Tail::None, gone, 1), none);
    // The editor came up with it outstanding, queue already empty or not; its echo on the prompt
    // row, not yet parsed (the bare prompt, one row below the Enter), or not predictable.
    let mut editor_behind = editor.clone();
    editor_behind.queue_empty = Some(false);
    for fact in [&editor, &editor_behind] {
        assert_eq!(look(unread, fact, p, shown, 1), runs);
        assert_eq!(look(unread, fact, p, gone, 1), runs);
        assert_eq!(look(unread, fact, p, shown, 4), runs);
        assert_eq!(look(unread, fact, p, opaque, 4), runs);
    }
    // One burst carried the editor's accept, the command and the next prompt: it ran already.
    assert_eq!(look(unread, &editor, p, gone, 2), none);
    // Still the prompt row with the echoed line (also soft-wrapped): it runs.
    assert_eq!(look(runs, &editor, p, shown, 1), runs);
    assert_eq!(look(runs, &editor, p, shown, 2), runs);
    assert_eq!(look(runs, &editor, p, gone, 1), runs);
    assert_eq!(look(runs, &editor, p, opaque, 9), runs);
    // Accepted: the newline, the cooked gap, the program, or the next prompt below.
    assert_eq!(look(runs, &editor, Tail::None, shown, 1), none);
    assert_eq!(look(runs, &rc, p, shown, 1), none);
    assert_eq!(look(runs, &vim, p, shown, 1), none);
    assert_eq!(look(runs, &editor, p, gone, 2), none);

    // The cursor row against the typed line.
    let typed_line = Some("vim -u NONE f.txt");
    let shows = |row: &str, continues: bool| row_shows_typed_line(row, continues, typed_line);
    assert_eq!(shows("LAB$ vim -u NONE f.txt", false), Some(true));
    assert_eq!(shows("LAB$ vim -u N", false), Some(true), "echo arriving");
    assert_eq!(shows("LAB$ v", false), Some(true));
    assert_eq!(shows("LAB$ ", false), Some(false), "a bare prompt");
    assert_eq!(shows("LAB$", false), Some(false));
    assert_eq!(shows("", false), Some(false));
    assert_eq!(shows("NE f.txt", true), Some(true), "the soft-wrapped end");
    assert_eq!(shows("NE f.txt", false), Some(false), "not a continuation");
    assert_eq!(row_shows_typed_line("LAB$ x", false, None), None);
    assert_eq!(row_shows_typed_line("LAB$ x", false, Some("  ")), None);

    // The tracker keeps the line's text only while its echo can be predicted, and hands it over
    // once.
    let mut tracker = LineTracker::default();
    tracker.note(b"vim -u NONE f.txt\r");
    assert_eq!(
        tracker.take_last_line().as_deref(),
        Some("vim -u NONE f.txt")
    );
    assert_eq!(tracker.take_last_line(), None, "taken once");
    let opaque_lines: [&[u8]; 4] = [
        b"vi\x7fm f\r",
        b"ls \t\r",
        b"\x1b[Aecho\r",
        b"\x1b[200~pasted\x1b[201~\r",
    ];
    for opaque_line in opaque_lines {
        tracker.note(opaque_line);
        assert_eq!(tracker.take_last_line(), None, "{opaque_line:?}");
    }
    tracker.note(b"abc\x15");
    assert_eq!(tracker.take_last_line(), None, "a killed line");
    tracker.note(b"next\r");
    assert_eq!(tracker.take_last_line().as_deref(), Some("next"));
    tracker.note(&[b'x'; 600]);
    tracker.note(b"\r");
    assert_eq!(tracker.take_last_line(), None, "too long to keep");

    // The judge: the round-8 sample (line editor, prompt shape, queue empty, 33 bytes, 1 line)
    // waits while the line is outstanding, in both states, and is written once it is not.
    let s = line_reader("bash");
    for state in [unread, runs] {
        let user = UserLedger {
            bytes_since_arm: 33,
            typed_ahead: state,
            ..typed()
        };
        assert_eq!(
            subshell_ready(&s, &user, None),
            Decision::Wait(WaitWhy::TypedLineRuns),
            "{state:?}"
        );
    }
    assert_eq!(subshell_ready(&s, &typed(), None), Decision::Write(None));
}

/// #2006 (round 9, Windows Git Bash, `subshell_ready_rc_questions`): on a pane with no tty probe
/// the user's own line on the cursor's row is never "a prompt drawn since the line end". Every
/// combination of the screen facts: with user bytes passed, a Write needs the fact false; the
/// fact never turns a Wait or a Cancel into a Write.
#[test]
fn no_probe_never_writes_on_the_users_own_line_sweep() {
    for tail in [Tail::None, Tail::ShellPrompt, Tail::InputPrompt] {
        for moved in [true, false] {
            for moved_since_end in [true, false] {
                for line_dirty in [true, false] {
                    for ledger in [no_bytes(), typed()] {
                        let mut clean = unknown(LeaderKind::Unreadable, "");
                        clean.no_tty_probe = true;
                        clean.tail = tail;
                        clean.prompt_moved_since_baseline = moved;
                        clean.prompt_moved_since_line_end = moved_since_end;
                        clean.line_dirty = line_dirty;
                        let mut shown = clean.clone();
                        shown.user_line_may_be_at_cursor = true;
                        let without = subshell_ready(&clean, &ledger, Some(ShellType::Bash));
                        let with = subshell_ready(&shown, &ledger, Some(ShellType::Bash));
                        let what = format!(
                            "{tail:?} moved={moved} since_end={moved_since_end} \
                             dirty={line_dirty} bytes={}",
                            ledger.bytes_since_arm
                        );
                        if ledger.bytes_since_arm > 0 {
                            assert!(!matches!(with, Decision::Write(_)), "{what}");
                        } else {
                            assert_eq!(with, without, "{what}");
                        }
                        if !matches!(without, Decision::Write(_)) {
                            assert_eq!(with, without, "{what}: only a Write is vetoed");
                        }
                    }
                }
            }
        }
    }
}

/// #2006: which rows may show the user's last line. The rows are those of the round 9 cases.
#[test]
fn user_line_may_be_at_cursor_rows() {
    let me = LastUserLine::Text("me".to_owned());
    // The late echo on the row the line was ended on, whatever it reads as.
    assert!(user_line_may_be_at_cursor(
        &me, false, false, "Name: me", false
    ));
    assert!(user_line_may_be_at_cursor(
        &me, false, false, "Name: m", false
    ));
    assert!(user_line_may_be_at_cursor(&me, false, false, "LAB$", false));
    // Another row: the next question, the next prompt.
    assert!(!user_line_may_be_at_cursor(
        &me, false, true, "Team:", false
    ));
    assert!(!user_line_may_be_at_cursor(&me, false, true, "LAB$", false));
    // A typed-ahead line the line editor redisplays at its prompt (a `clear` moved the row).
    let echo = LastUserLine::Text("echo T1 >> f.txt".to_owned());
    assert!(user_line_may_be_at_cursor(
        &echo,
        true,
        true,
        "LAB$ echo T1 >> f.txt",
        false
    ));
    assert!(user_line_may_be_at_cursor(
        &echo,
        true,
        true,
        "LAB$ echo T",
        false
    ));
    assert!(!user_line_may_be_at_cursor(
        &echo, true, true, "LAB$", false
    ));
    // Its echo wrapped onto the next row.
    assert!(user_line_may_be_at_cursor(
        &echo, false, true, ">> f.txt", true
    ));
    // An empty line has no echo to mistake, on any row.
    for row_changed in [true, false] {
        assert!(!user_line_may_be_at_cursor(
            &LastUserLine::None,
            true,
            row_changed,
            "LAB$",
            true
        ));
    }
    // An unpredictable line: the row it was ended on and a wrapped row may show it; typed
    // before any prompt was drawn it is never proof.
    let opaque = LastUserLine::Opaque;
    assert!(user_line_may_be_at_cursor(
        &opaque, false, false, "LAB$", false
    ));
    assert!(user_line_may_be_at_cursor(
        &opaque, false, true, "LAB$", true
    ));
    assert!(!user_line_may_be_at_cursor(
        &opaque, false, true, "LAB$", false
    ));
    assert!(user_line_may_be_at_cursor(
        &opaque, true, true, "LAB$", false
    ));
}

/// #2006: what a ReArm's line ends leave for the screen to show.
#[test]
fn last_user_line_after_ends_rows() {
    let none = LastUserLine::None;
    let text = |s: &str| LastUserLine::Text(s.to_owned());
    // One answer, predictable: its text, trimmed.
    assert_eq!(
        last_user_line_after_ends(&none, false, 1, true, Some(" me ")),
        text("me")
    );
    // Unpredictable (a control byte, a paste).
    assert_eq!(
        last_user_line_after_ends(&none, false, 1, true, None),
        LastUserLine::Opaque
    );
    // Blanks only run nothing.
    assert_eq!(
        last_user_line_after_ends(&none, false, 1, true, Some("  ")),
        none
    );
    // A bare Enter / ^C / ^U: nothing to show; it replaces an answered line.
    assert_eq!(
        last_user_line_after_ends(&text("me"), false, 0, false, Some("")),
        none
    );
    // ... but not a line typed before any prompt that is still outstanding.
    assert_eq!(
        last_user_line_after_ends(&text("vim f"), true, 0, false, Some("")),
        text("vim f")
    );
    // A second line behind one typed before any prompt, or two lines in one chunk: only the
    // last text is kept, so the pair is unprovable.
    assert_eq!(
        last_user_line_after_ends(&text("vim f"), true, 1, true, Some("ls")),
        LastUserLine::Opaque
    );
    assert_eq!(
        last_user_line_after_ends(&none, false, 2, true, Some("ls")),
        LastUserLine::Opaque
    );
    assert_eq!(
        last_user_line_after_ends(&none, false, 1, false, Some("")),
        LastUserLine::Opaque,
        "a non-empty line and then an empty one in one chunk"
    );
}

// ---- #2016: a one-key answer (zsh-newuser-install's menu, `read -k`) is not text in the line ----

const NEWUSER_FOOTER_ROW: &str = "--- Type one of the keys in parentheses --- ";

/// The look before the menu's `q`: a clean line, the footer on a raw tty under zsh, row 20.
fn at_the_menu() -> LineStart {
    LineStart {
        line_clean: true,
        raw_shell_tty: true,
        tail: Tail::InputPrompt,
        row: 20,
        row_text: NEWUSER_FOOTER_ROW.to_owned(),
    }
}

/// `key_read_took_the_typed_keys` for a look that shows `row_text` on `row` under a zle reading
/// with nothing unread, after the user typed `typed` at `before`.
fn taken(before: &LineStart, typed: Option<&str>, row: usize, row_text: &str) -> bool {
    key_read_took_the_typed_keys(
        before,
        typed,
        &SubshellTtySample::line_reader("zsh"),
        Tail::ShellPrompt,
        row,
        row_text,
        false,
    )
}

/// The case's path: `q` at the footer, the menu returns, zle draws `host% ` on the next row. The
/// key is gone; the judge then writes at that prompt (a line editor, a prompt, the queue empty).
#[test]
fn a_menu_key_answered_and_a_new_prompt_below_means_the_key_is_gone() {
    assert!(taken(&at_the_menu(), Some("q"), 21, "host% "));
    // Rows printed between the answer and the prompt ("Aborting. ...") change nothing.
    assert!(taken(&at_the_menu(), Some("x"), 25, "host% "));
    // Two menu keys (`1`, then `q` on a later screen), prompt further down.
    assert!(taken(&at_the_menu(), Some("1q"), 40, "~/src % "));
    // zsh's PROMPT_SP mark, seen before the prompt itself: still not the typed key.
    assert!(taken(&at_the_menu(), Some("q"), 21, "%"));
}

/// Each fact alone turns the rule off: the line stays dirty and the attempt waits, as before.
#[test]
fn a_key_is_not_gone_without_every_fact() {
    let before = at_the_menu();
    // Text typed before the question's key may sit in a line editor's buffer (zsh's "do you wish
    // to see all N possibilities?" after a line typed before Phosphorize).
    let mut b = before.clone();
    b.line_clean = false;
    assert!(!taken(&b, Some("n"), 21, "host% ls /usr/bin/"));
    // The key was not typed at a question ...
    let mut b = before.clone();
    b.tail = Tail::ShellPrompt;
    assert!(!taken(&b, Some("q"), 21, "host% "));
    let mut b = before.clone();
    b.tail = Tail::None;
    assert!(!taken(&b, Some("q"), 21, "host% "));
    // ... or not on a raw tty under a shell leader (a cooked `read -p` needs its Enter; a
    // program's own key read is the program's).
    let mut b = before.clone();
    b.raw_shell_tty = false;
    assert!(!taken(&b, Some("q"), 21, "host% "));
    // Nothing is known about the look before the line (no look since arming).
    assert!(!taken(&LineStart::unknown(), Some("q"), 21, "host% "));
    // The same row (the echo of the key behind the question) or a row above it.
    assert!(!taken(&before, Some("q"), 20, "host% "));
    assert!(!taken(&before, Some("q"), 3, "host% "));
    // The row shows what was typed: a line editor redrew its buffer behind its prompt.
    assert!(!taken(&before, Some("q"), 21, "host% q"));
    assert!(!taken(&before, Some("ls -l"), 21, "host% ls"));
    // The typed text cannot be predicted (an arrow key, a paste, nothing kept).
    assert!(!taken(&before, None, 21, "host% "));
    // The question drawn again on a lower row (the menu redisplayed, the same prompt redrawn).
    assert!(!taken(&before, Some("x"), 30, NEWUSER_FOOTER_ROW));
    assert!(!taken(
        &before,
        Some("x"),
        30,
        "--- Type one of the keys in parentheses --- 12:03"
    ));
}

/// The look's own facts: only a line editor's raw tty with nothing unread at a shell-shaped tail
/// on a row of its own counts.
#[test]
fn a_key_is_not_gone_on_a_look_that_is_not_a_line_editor_at_a_prompt() {
    let before = at_the_menu();
    let judge = |tty: &SubshellTtySample, tail: Tail, continues: bool| {
        key_read_took_the_typed_keys(&before, Some("q"), tty, tail, 21, "host% ", continues)
    };
    let zle = SubshellTtySample::line_reader("zsh");
    assert!(judge(&zle, Tail::ShellPrompt, false));
    // The cooked gap between the read and the line editor, a relay, a program, no probe.
    for kind in [
        SubshellTtyKind::Cooked,
        SubshellTtyKind::Relay,
        SubshellTtyKind::Secret,
        SubshellTtyKind::Unknown,
    ] {
        let mut other = zle.clone();
        other.kind = kind;
        assert!(!judge(&other, Tail::ShellPrompt, false), "{kind:?}");
    }
    assert!(!judge(
        &SubshellTtySample::unknown(),
        Tail::ShellPrompt,
        false
    ));
    // Something typed is still unread, or the queue cannot be probed.
    for queue in [Some(false), None] {
        let mut unread = zle.clone();
        unread.queue_empty = queue;
        assert!(!judge(&unread, Tail::ShellPrompt, false), "{queue:?}");
    }
    // The next question of the menu, or a finished line: no prompt of a line editor.
    assert!(!judge(&zle, Tail::InputPrompt, false));
    assert!(!judge(&zle, Tail::None, false));
    // The row continues the row above it (a soft wrap): not a prompt drawn on a row of its own.
    assert!(!judge(&zle, Tail::ShellPrompt, true));
}

/// The tracker's text of the line being typed: kept while its echo can be predicted, gone at a
/// line end, never for a paste or a control byte.
#[test]
fn the_tracker_knows_the_text_typed_so_far_only_while_it_is_plain() {
    let mut tracker = LineTracker::default();
    assert_eq!(tracker.typed_so_far(), None);
    assert!(tracker.note(b"q").is_empty());
    assert_eq!(tracker.typed_so_far(), Some("q"));
    assert!(tracker.note(b"1").is_empty());
    assert_eq!(tracker.typed_so_far(), Some("q1"));
    // An escape sequence (an arrow key) makes the echo unpredictable for the rest of the line.
    assert!(tracker.note(b"\x1b[A").is_empty());
    assert_eq!(tracker.typed_so_far(), None);
    assert!(tracker.line_has_content());
    // A line end starts a new line.
    assert_eq!(tracker.note(b"\r").len(), 1);
    assert_eq!(tracker.typed_so_far(), None);
    assert!(tracker.note(b"ls").is_empty());
    assert_eq!(tracker.typed_so_far(), Some("ls"));
    // ^U empties the line.
    assert_eq!(tracker.note(b"\x15").len(), 1);
    assert_eq!(tracker.typed_so_far(), None);
    // A paste is never predictable.
    assert!(tracker.note(b"\x1b[200~pasted\x1b[201~").is_empty());
    assert_eq!(tracker.typed_so_far(), None);
}

// ---- #1991: a setup line written to a line editor ----------------------------------------------

/// A setup-shaped line (the real ones begin with a space and an assignment).
const SETUP_LINE: &str = " __hc0=abc; eval \"$__hc0\"; printf '\\033]7771;7;0\\007'";

/// The line written at `bash-5.1$ ` (column 10) of row 4 of an 80-column pane.
fn editor_write() -> LineEditorWrite {
    LineEditorWrite {
        line: SETUP_LINE.to_owned(),
        row: 4,
        col: 10,
        columns: 80,
    }
}

/// #1991: only a write whose look said "line editor" is judged without the echo, and only while
/// the kill switch `PHOSPHOR_SUBSHELL_LINE_EDITOR_PROOF` is on.
#[test]
fn only_a_line_reader_write_is_a_line_editor_write_and_the_switch_turns_it_off() {
    for kind in [
        SubshellTtyKind::Secret,
        SubshellTtyKind::Relay,
        SubshellTtyKind::LineReader,
        SubshellTtyKind::Cooked,
        SubshellTtyKind::Unknown,
    ] {
        assert_eq!(
            is_line_editor_write(kind, true),
            kind == SubshellTtyKind::LineReader,
            "{kind:?}"
        );
        assert!(!is_line_editor_write(kind, false), "{kind:?}: switch off");
    }
}

/// #1991, the issue's R9 window: under a line editor the filter never matches, so "the queue
/// holds bytes and nothing was echoed" was true while the editor was half way through the line,
/// and a Question / Net / Gone cancel flushed the rest. The byte count decides instead: only the
/// WHOLE write, and nothing but it, is provably unread.
#[test]
fn a_line_editor_write_is_flushed_only_while_the_queue_holds_exactly_the_whole_write() {
    let write = 3 + SETUP_LINE.len() + 1;
    for why in [
        WrittenCancel::Question,
        WrittenCancel::Net,
        WrittenCancel::Gone,
        WrittenCancel::Unsupported,
    ] {
        assert!(
            cancel_flushes_unread_line_editor_write(why, Some(write), write, 0),
            "{why:?}: the whole write sits unread: kept out of whatever reads next"
        );
        // The editor is taking the line: one byte read is enough to forbid the cut.
        for unread in [write - 1, write / 2, 1, 0] {
            assert!(
                !cancel_flushes_unread_line_editor_write(why, Some(unread), write, 0),
                "{why:?}: {unread} of {write} bytes unread: the line is being read, never cut"
            );
        }
        // More than the write: something typed before the arming waits there too.
        assert!(
            !cancel_flushes_unread_line_editor_write(why, Some(write + 2), write, 0),
            "{why:?}: bytes that are not the setup's are never discarded"
        );
        // No count: nothing is proven.
        assert!(
            !cancel_flushes_unread_line_editor_write(why, None, write, 0),
            "{why:?}"
        );
        // #1940, unchanged: a user byte since the write.
        assert!(
            !cancel_flushes_unread_line_editor_write(why, Some(write), write, 1),
            "{why:?}: the unread bytes may be the user's"
        );
        assert!(line_editor_flush_needs_the_count(why, write, 0), "{why:?}");
        assert!(!line_editor_flush_needs_the_count(why, write, 1), "{why:?}");
        assert!(!line_editor_flush_needs_the_count(why, 0, 0), "{why:?}");
    }
    // #1940, unchanged: a Secret cancel never flushes, whatever the count says.
    assert!(!cancel_flushes_unread_line_editor_write(
        WrittenCancel::Secret,
        Some(write),
        write,
        0
    ));
    assert!(!line_editor_flush_needs_the_count(
        WrittenCancel::Secret,
        write,
        0
    ));
    // Nothing written (a stage 2 that still waited): nothing to flush, also with an empty queue.
    assert!(!cancel_flushes_unread_line_editor_write(
        WrittenCancel::Question,
        Some(0),
        0,
        0
    ));
}

/// #1991: the rows the editor's own drawing of the line can reach, and so the first row a
/// prompt drawn after accept-line can be on.
#[test]
fn the_first_row_after_accept_is_below_every_row_the_editor_draws_the_line_on() {
    let short = LineEditorWrite {
        line: "x".repeat(20),
        row: 4,
        col: 10,
        columns: 80,
    };
    assert_eq!(short.first_row_after_accept(), Some(5), "one row, then LF");
    // Ends exactly at the margin: the cursor wraps onto the next row before the accept.
    let margin = LineEditorWrite {
        line: "x".repeat(70),
        ..short.clone()
    };
    assert_eq!(margin.first_row_after_accept(), Some(6));
    // One column short of the margin: the column of slack counts it as wrapped too.
    let slack = LineEditorWrite {
        line: "x".repeat(69),
        ..short.clone()
    };
    assert_eq!(slack.first_row_after_accept(), Some(6));
    let long = LineEditorWrite {
        line: "x".repeat(200),
        ..short.clone()
    };
    assert_eq!(long.first_row_after_accept(), Some(4 + 2 + 1), "three rows");
    let unknown_width = LineEditorWrite {
        columns: 0,
        ..short
    };
    assert_eq!(unknown_width.first_row_after_accept(), None);
}

/// #1991: what the cursor's row shows while the editor draws the line, and what a prompt drawn
/// after it shows.
#[test]
fn a_row_of_the_drawn_line_is_told_from_a_fresh_prompt_by_its_text() {
    let line = SETUP_LINE;
    // The prompt's row, the line growing behind it.
    assert!(row_shows_written_line("bash-5.1$ __hc0=abc; ev", line));
    assert!(row_shows_written_line("bash-5.1$ _", line));
    assert!(row_shows_written_line(
        &format!("bash-5.1$ {}", line.trim()),
        line
    ));
    // A row of the wrapped line, wherever the editor cut it.
    assert!(row_shows_written_line("eval \"$__hc0\"; printf", line));
    assert!(row_shows_written_line("  7;0\\007'  ", line));
    // An empty row is never proof.
    assert!(row_shows_written_line("", line));
    assert!(row_shows_written_line("   ", line));
    // A prompt drawn after the line was accepted.
    assert!(!row_shows_written_line("bash-5.1$ ", line));
    assert!(!row_shows_written_line("user@host:~/src % ", line));
    assert!(!row_shows_written_line(
        "bash: __hc0=abc: command not found",
        line
    ));
    // Accepted, on the safe side: a prompt that occurs in the line, or ends in its first
    // character.
    assert!(row_shows_written_line("$ ", line), "`$` occurs in the line");
    assert!(row_shows_written_line("host_ ", line), "ends in `_`");
}

/// #1991, R7 for a line-editor write: the proof that the shell took the line is the grid's. Each
/// fact of the rule alone withholds it.
#[test]
fn a_fresh_prompt_below_the_drawn_line_proves_the_editor_took_it() {
    let write = editor_write();
    // Column 10, 53 characters and the slack: one row. The first row after accept is row 5.
    assert_eq!(write.first_row_after_accept(), Some(5));
    let took = |columns, row, tail, text: &str, continues| {
        line_editor_took_the_line(&write, columns, row, tail, text, continues)
    };
    // The error line was printed, the prompt drawn after it.
    assert!(took(80, 6, Tail::ShellPrompt, "bash-5.1$ ", false));
    // A shell that printed nothing: the prompt directly below the line.
    assert!(took(80, 5, Tail::ShellPrompt, "bash-5.1$ ", false));

    // The editor is still drawing on the prompt's row.
    assert!(!took(80, 4, Tail::ShellPrompt, "bash-5.1$ __hc0=ab", false));
    // The old prompt redrawn in place is not a prompt after the line.
    assert!(!took(80, 4, Tail::ShellPrompt, "bash-5.1$ ", false));
    // A finished line (the accept's line feed, the cursor at column 0), or a question.
    assert!(!took(80, 6, Tail::None, "", false));
    assert!(!took(80, 6, Tail::InputPrompt, "Continue? [y/N] ", false));
    // A soft-wrapped row is the same logical line.
    assert!(!took(80, 6, Tail::ShellPrompt, "bash-5.1$ ", true));
    // The row shows a piece of the line (an editor that redrew it lower).
    assert!(!took(
        80,
        6,
        Tail::ShellPrompt,
        "eval \"$__hc0\"; printf",
        false
    ));
    // The pane was resized since the write: the rows were reflowed.
    assert!(!took(100, 6, Tail::ShellPrompt, "bash-5.1$ ", false));

    // A line that wraps: rows 4..=6 are the editor's, so a prompt on row 6 proves nothing and
    // one on row 7 does.
    let long = LineEditorWrite {
        line: format!(" {}", "x".repeat(170)),
        ..editor_write()
    };
    assert_eq!(long.first_row_after_accept(), Some(7));
    assert!(!line_editor_took_the_line(
        &long,
        80,
        6,
        Tail::ShellPrompt,
        "bash-5.1$ ",
        false
    ));
    assert!(line_editor_took_the_line(
        &long,
        80,
        7,
        Tail::ShellPrompt,
        "bash-5.1$ ",
        false
    ));
}
