//! #1033: the tty's echo of typed-ahead keys must not stay in the blocks it landed in.
//!
//! The scenario tests replay byte streams captured from a real bash 5.2 running the Phosphor
//! bootstrap under a pty, with keys written to the pty while commands ran, the way the
//! typeahead hand-off writes them. Hook DCS sequences are replaced by the matching handler calls
//! at the exact place they appeared in the stream; window-title OSCs are left out. Every
//! `typed` step is a write the app made (recorded as typeahead, then echoed by the tty when the
//! capture shows the echo).

use std::io;

use crate::terminal::SizeInfo;
use crate::terminal::model::ansi::{self, Handler as _, InputBufferValue, PreexecValue, Processor};
use crate::terminal::model::blocks::BlockList;
use crate::terminal::model::session::SessionInfo;
use crate::terminal::model::test_utils::{TestBlockListBuilder, block_size};
use crate::terminal::shell::ShellType;

use super::{EchoGrid, TypedEcho};

// ---------------------------------------------------------------------------------------------
// The ledger on its own.

fn id() -> crate::terminal::model::BlockId {
    crate::terminal::model::BlockId::new()
}

fn ledger_with_line_editor_seen() -> TypedEcho {
    let mut echo = TypedEcho::new();
    echo.set_line_editor_active(true);
    echo.set_line_editor_active(false);
    echo
}

#[test]
fn nothing_is_claimed_before_the_shell_was_seen_using_bracketed_paste() {
    let mut echo = TypedEcho::new();
    echo.record_typed(b"e");
    assert_eq!(echo.classify('e', None, (0, 0), true), None);
}

#[test]
fn nothing_is_claimed_while_the_line_editor_is_active() {
    let mut echo = ledger_with_line_editor_seen();
    echo.record_typed(b"e");
    echo.set_line_editor_active(true);
    assert!(!echo.may_match('e'));
    assert_eq!(echo.classify('e', None, (0, 0), true), None);
}

#[test]
fn an_echo_starts_a_line_or_continues_a_run() {
    let mut echo = ledger_with_line_editor_seen();
    let block = id();
    echo.record_typed(b"ech");
    // Not at a line start and not after a run: program output, not echo.
    assert_eq!(
        echo.classify('e', Some((block.clone(), EchoGrid::Output)), (0, 3), false),
        None
    );
    let m = echo
        .classify('e', Some((block.clone(), EchoGrid::Output)), (1, 0), true)
        .unwrap();
    echo.commit(m, block.clone(), EchoGrid::Output, 'e', (1, 0), (1, 1));
    // Continues only right after the run, in the same grid.
    assert_eq!(
        echo.classify('c', Some((block.clone(), EchoGrid::Command)), (1, 1), false),
        None
    );
    assert_eq!(
        echo.classify('c', Some((block.clone(), EchoGrid::Output)), (1, 2), false),
        None
    );
    let m = echo
        .classify('c', Some((block.clone(), EchoGrid::Output)), (1, 1), false)
        .unwrap();
    echo.commit(m, block.clone(), EchoGrid::Output, 'c', (1, 1), (1, 2));
    let runs: Vec<_> = echo.runs().cloned().collect();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "ec");
}

#[test]
fn an_enter_echoed_as_a_line_break_is_skipped_at_the_next_line_start() {
    let mut echo = ledger_with_line_editor_seen();
    let block = id();
    echo.record_typed(b"\rx");
    assert_eq!(
        echo.classify('x', Some((block.clone(), EchoGrid::Output)), (0, 4), false),
        None
    );
    assert!(
        echo.classify('x', Some((block, EchoGrid::Output)), (1, 0), true)
            .is_some()
    );
}

#[test]
fn editing_keys_stop_the_tracking() {
    let mut echo = ledger_with_line_editor_seen();
    echo.record_typed(b"ab\x7f");
    assert!(!echo.may_match('a'));
    echo.record_typed(b"c");
    assert!(echo.may_match('c'));
}

#[test]
fn only_the_line_still_being_typed_is_taken_when_a_command_exits() {
    let mut echo = ledger_with_line_editor_seen();
    let block = id();
    // "hi" + Enter went to the program (e.g. `cat`), "ls" is still being typed.
    echo.record_typed(b"hi\rls");
    for (c, point, next) in [('h', (0, 0), (0, 1)), ('i', (0, 1), (0, 2))] {
        let m = echo
            .classify(
                c,
                Some((block.clone(), EchoGrid::Output)),
                point,
                point.1 == 0,
            )
            .unwrap();
        echo.commit(m, block.clone(), EchoGrid::Output, c, point, next);
    }
    for (c, point, next) in [('l', (2, 0), (2, 1)), ('s', (2, 1), (2, 2))] {
        let m = echo
            .classify(
                c,
                Some((block.clone(), EchoGrid::Output)),
                point,
                point.1 == 0,
            )
            .unwrap();
        echo.commit(m, block.clone(), EchoGrid::Output, c, point, next);
    }
    let taken = echo.take_runs_of_unterminated_line(&block);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].text, "ls");
    assert_eq!(echo.runs().count(), 1, "the program's line stays recorded");
}

#[test]
fn a_shell_run_line_is_matched_by_its_text() {
    let mut echo = ledger_with_line_editor_seen();
    let block = id();
    echo.record_typed(b"hi\rls\r");
    let m = echo
        .classify('h', Some((block.clone(), EchoGrid::Output)), (0, 0), true)
        .unwrap();
    echo.commit(m, block.clone(), EchoGrid::Output, 'h', (0, 0), (0, 1));
    let m = echo
        .classify('i', Some((block.clone(), EchoGrid::Output)), (0, 1), false)
        .unwrap();
    echo.commit(m, block.clone(), EchoGrid::Output, 'i', (0, 1), (0, 2));
    let m = echo
        .classify('l', Some((block.clone(), EchoGrid::Output)), (1, 0), true)
        .unwrap();
    echo.commit(m, block.clone(), EchoGrid::Output, 'l', (1, 0), (1, 1));
    // The shell ran `ls`, not `hi`: only the echo of `ls` is the shell's.
    let taken = echo.take_runs_of_shell_run_line("ls");
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].text, "l");
    // A command nobody typed takes nothing.
    assert!(echo.take_runs_of_shell_run_line("pwd").is_empty());
}

#[test]
fn a_report_that_differs_from_the_typed_line_takes_nothing_but_resets() {
    let mut echo = ledger_with_line_editor_seen();
    let block = id();
    echo.record_typed(b"ls");
    let m = echo
        .classify('l', Some((block.clone(), EchoGrid::Output)), (0, 0), true)
        .unwrap();
    echo.commit(m, block, EchoGrid::Output, 'l', (0, 0), (0, 1));
    assert!(echo.take_runs_of_reported_line("cd").is_empty());
    assert!(!echo.may_match('s'));
}

// ---------------------------------------------------------------------------------------------
// Replays of real bash byte streams through the block list.

enum Step<'a> {
    /// Bytes the shell or the tty wrote to the pty.
    Out(&'a [u8]),
    /// Bytes the app wrote to the pty as typeahead.
    Typed(&'a [u8]),
    /// The input editor submits a command (the block starts before its bytes are written).
    EditorCommand,
    Preexec(&'a str),
    CommandFinished,
    Precmd,
    Report(&'a str),
}
use Step::*;

/// A bootstrapped bash-like session in shell-reported typeahead mode, 80 columns wide.
fn new_session() -> BlockList {
    let mut sizes = block_size();
    sizes.size = SizeInfo::new_without_font_metrics(10, 80);
    let mut block_list = TestBlockListBuilder::new().with_block_sizes(sizes).build();
    let init_shell_value = ansi::InitShellValue {
        shell: "zsh".into(),
        ..Default::default()
    };
    let bootstrapped_value = ansi::BootstrappedValue {
        shell: "zsh".into(),
        shell_version: Some("5.0".into()),
        ..Default::default()
    };
    let session_info = SessionInfo::create_pending(
        ShellType::from_name("zsh").unwrap(),
        init_shell_value,
        None,
        None,
        None,
        false,
        None,
    )
    .merge_from_bootstrapped_value(bootstrapped_value.clone(), false);
    block_list.bootstrapped(bootstrapped_value);
    block_list.early_output_mut().init_session(&session_info);
    block_list.command_finished(Default::default());
    block_list.prompt_only_precmd(Default::default());
    assert!(block_list.is_bootstrapping_precmd_done());
    block_list
}

fn replay(block_list: &mut BlockList, steps: &[Step]) {
    let mut processor = Processor::new();
    for step in steps {
        match step {
            Out(bytes) => processor.parse_bytes(block_list, bytes, &mut io::sink()),
            Typed(bytes) => block_list.record_typed_ahead_bytes(bytes),
            EditorCommand => block_list.start_active_block(),
            Preexec(command) => block_list.preexec(PreexecValue {
                command: (*command).into(),
                session_id: None,
            }),
            CommandFinished => block_list.command_finished(Default::default()),
            Precmd => block_list.prompt_only_precmd(Default::default()),
            Report(buffer) => block_list.input_buffer(InputBufferValue {
                buffer: (*buffer).into(),
                session_id: None,
            }),
        }
    }
}

/// (command, output) of every finished command block, in order.
fn finished_commands(block_list: &BlockList) -> Vec<(String, String)> {
    block_list
        .blocks()
        .iter()
        .filter(|block| block.finished() && !block.is_background())
        .map(|block| (block.command_to_string(), block.output_to_string()))
        .filter(|(command, _)| !command.is_empty())
        .collect()
}

/// Text of every background (early output) block that kept any.
fn background_text(block_list: &BlockList) -> Vec<String> {
    block_list
        .blocks()
        .iter()
        .filter(|block| block.is_background())
        .map(|block| block.output_to_string())
        .filter(|text| !text.trim().is_empty())
        .collect()
}

/// `true` from the editor, then `echo LOCAL-OK-42` typed at once (3 ms a key), its Enter held
/// by the hand-off (#933) so the line comes back by report and is submitted from the editor.
/// Captured: "ech" is echoed before preexec (into the header), "o" while `true` runs (its
/// output), " LOC" between its exit and the prompt (early output, then erased by the report).
#[test]
fn typeahead_typed_during_a_short_editor_command_leaves_no_echo_behind() {
    let mut block_list = new_session();
    replay(
        &mut block_list,
        &[
            Out(b"\x1b[?2004h"),
            EditorCommand,
            Out(b"\x1b[7mtrue\x1b[27m\rtrue\r\n\x1b[?2004l\r"),
            Typed(b"e"),
            Out(b"e"),
            Typed(b"c"),
            Out(b"c"),
            Typed(b"h"),
            Out(b"h"),
            Preexec("true"),
            Typed(b"o"),
            Out(b"o"),
            CommandFinished,
            Typed(b" "),
            Out(b" "),
            Typed(b"L"),
            Out(b"L"),
            Typed(b"O"),
            Out(b"O"),
            Precmd,
            Typed(b"C"),
            Out(b"C"),
            Out(b"\x1b[?2004h"),
            Out(b"echo LOC"),
            Out(b"\r\x1b[K\r"),
            Report("echo LOC"),
            EditorCommand,
            Out(b"\x1b[7mecho LOCAL-OK-42\x1b[27m\recho LOCAL-OK-42\r\n\x1b[?2004l\r"),
            Preexec("echo LOCAL-OK-42"),
            Out(b"LOCAL-OK-42\r\n"),
            CommandFinished,
            Precmd,
            Out(b"\x1b[?2004h"),
        ],
    );
    assert_eq!(
        finished_commands(&block_list),
        vec![
            ("true".to_owned(), "".to_owned()),
            ("echo LOCAL-OK-42".to_owned(), "LOCAL-OK-42".to_owned()),
        ]
    );
    assert!(background_text(&block_list).is_empty());
}

/// 50 x `echo BURSTn` + Enter typed at 5 ms a key straight into the pty (captured; the first
/// three commands): the shell runs each typed line itself. The next line's first keys are
/// echoed into the header and output of the running command, and the keys typed between a
/// command's exit and the prompt end up in front of the next command line.
#[test]
fn typeahead_run_by_the_shell_itself_leaves_no_echo_behind() {
    let mut block_list = new_session();
    replay(
        &mut block_list,
        &[
            Out(b"\x1b[?2004h"),
            EditorCommand,
            Out(b"\x1b[7mecho BURST1\x1b[27m\recho BURST1\r\n"),
            Out(b"\x1b[?2004l\r"),
            Typed(b"e"),
            Out(b"e"),
            Typed(b"c"),
            Out(b"c"),
            Preexec("echo BURST1"),
            Out(b"BURST1\r\n"),
            CommandFinished,
            Typed(b"h"),
            Out(b"h"),
            Typed(b"o"),
            Out(b"o"),
            Precmd,
            Typed(b" "),
            Out(b" "),
            Out(b"\x1b[?2004h"),
            Out(b"echo "),
            Typed(b"B"),
            Out(b"B"),
            Typed(b"URST2"),
            Out(b"URST2"),
            Typed(b"\r"),
            Out(b"\r\n\x1b[?2004l\r"),
            Typed(b"e"),
            Out(b"e"),
            Preexec("echo BURST2"),
            Out(b"BURST2\r\n"),
            Typed(b"c"),
            Out(b"c"),
            CommandFinished,
            Typed(b"h"),
            Out(b"h"),
            Typed(b"o"),
            Out(b"o"),
            Precmd,
            Out(b"\x1b[?2004h"),
            Out(b"echo"),
            Typed(b" "),
            Out(b" "),
            Typed(b"BURST3"),
            Out(b"BURST3"),
            Typed(b"\r"),
            Out(b"\r\n"),
            Out(b"\x1b[?2004l\r"),
            Typed(b"e"),
            Out(b"e"),
            Preexec("echo BURST3"),
            Out(b"BURST3\r\n"),
            Typed(b"c"),
            Out(b"c"),
            CommandFinished,
            Typed(b"h"),
            Out(b"h"),
            Typed(b"o"),
            Out(b"o"),
            Typed(b" "),
            Out(b" "),
            Precmd,
            Out(b"\x1b[?2004h"),
            Out(b"echo "),
            Out(b"\r\x1b[K\r"),
            Report("echo "),
        ],
    );
    assert_eq!(
        finished_commands(&block_list),
        vec![
            ("echo BURST1".to_owned(), "BURST1".to_owned()),
            ("echo BURST2".to_owned(), "BURST2".to_owned()),
            ("echo BURST3".to_owned(), "BURST3".to_owned()),
        ]
    );
    assert!(background_text(&block_list).is_empty());
}

/// Program output that starts with the next typed key is not mistaken for its echo: the
/// output keeps every character (the late echo stays visible rather than any output going).
#[test]
fn output_that_looks_like_the_next_key_is_kept() {
    let mut block_list = new_session();
    replay(
        &mut block_list,
        &[
            Out(b"\x1b[?2004h"),
            EditorCommand,
            Out(b"echo BURST4\r\n\x1b[?2004l\r"),
            Preexec("echo BURST4"),
            // "BU" was written before the program's output was read, its echo after it.
            Typed(b"BU"),
            Out(b"BURST4\r\n"),
            Out(b"BU"),
            CommandFinished,
            Precmd,
        ],
    );
    let commands = finished_commands(&block_list);
    assert_eq!(commands.len(), 1);
    assert!(
        commands[0].1.starts_with("BURST4"),
        "program output must never be removed: {:?}",
        commands[0].1
    );
}

/// A whole line typed into a program that reads it (`cat`) keeps its echo: the command
/// consumed it, the shell never did.
#[test]
fn a_line_a_program_read_keeps_its_echo() {
    let mut block_list = new_session();
    replay(
        &mut block_list,
        &[
            Out(b"\x1b[?2004h"),
            EditorCommand,
            Out(b"cat\r\n\x1b[?2004l\r"),
            Preexec("cat"),
            Typed(b"hi\r"),
            Out(b"hi\r\n"),
            Out(b"hi\r\n"),
            CommandFinished,
            Precmd,
            Out(b"\x1b[?2004h"),
            Report(""),
        ],
    );
    assert_eq!(
        finished_commands(&block_list),
        vec![("cat".to_owned(), "hi\nhi".to_owned())]
    );
}

/// A shell that never enables bracketed paste gives no way to tell its own line drawing from
/// the tty's echo: nothing is removed.
#[test]
fn without_the_line_editor_marker_nothing_is_removed() {
    let mut block_list = new_session();
    replay(
        &mut block_list,
        &[
            EditorCommand,
            Out(b"true\r\n"),
            Typed(b"ech"),
            Out(b"ech"),
            Preexec("true"),
            CommandFinished,
            Precmd,
        ],
    );
    let commands = finished_commands(&block_list);
    assert_eq!(commands.len(), 1);
    assert!(commands[0].0.ends_with("ech"), "{:?}", commands[0].0);
}
