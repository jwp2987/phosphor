# r10-unit-subshell: five failing Linux unit tests (subshell readiness, bash trap, secret-read prediction)

Source: pipeline 3048013125, job test-linux 17885063200, commit 413cd4241 (merge/w0 without the
round 10 branches): 11,818 tests, 40 failed; this note covers five of them. Verdicts are against
integ/r10 (this branch's base). Nothing was compiled or run: every verdict is from reading code.

Round 10 did not settle any of the five: `git log 41340e2fd..HEAD` shows no change to
`bash_body.sh`'s drain script, to the #1927 test, or to the three view tests' bodies (the round 10
subshell commits a086eee2d, 18baba61a, c6211d440, 1297b106f changed the judge elsewhere).

| test | verdict | cause | change |
|---|---|---|---|
| `bootstrap::tests::bash_body_does_not_trap_sighup` | APP (script breaks the #960 rule) | #1933's LocalCommand script traps `INT TERM HUP` | trap `INT TERM` only |
| `input::tests::editing_the_buffer_during_a_secret_read_never_requests_a_query_prediction` | TEST fault (racy observation) | the test counts a channel a background thread drains | the test owns the channel |
| `view::tests::the_readiness_tail_is_the_cursor_row_not_the_blank_rows_below_a_clear` | TEST fault (fixture) | the menu fixture needs 20 rows on the 7x10 mock grid | fixture fits the grid |
| `view::tests::an_idle_dash_prompt_with_nothing_typed_is_written_on_the_look` | TEST fault (assertion) | the final check counts the user's own `x` as a setup write | assert the pty got exactly `x` |
| `view::tests::a_users_own_command_cancels_the_armed_setup_and_nothing_is_written` | TEST fault (fixture, stale since 37295f950) | the line is ended with vim as the tty's leader, which re-arms by rule | the shell is the leader at the key |

## 1. bash_body_does_not_trap_sighup: APP

- Panic: `bash_body.sh must not trap HUP:   trap "command stty $__p_old 2>/dev/null; exit 130" INT TERM HUP`
  (`app/src/terminal/bootstrap_test.rs:2156-2170`).
- The line is in `WARP_SSH_MSYS_AUTH_DONE_SCRIPT` (`app/assets/bundled/bootstrap/bash_body.sh:2636-2692`,
  trap at 2667, its reset at 2674), added by 780972506 (#1933, one-connection ssh from Git Bash).
- The rule (#960, `bash_body.sh:201-206`): a trap on HUP replaces bash's default hang-up handling;
  bash runs a trap only between commands, so a trapped HUP is acted on late or never while a
  command is in flight, and orphaned shells stay behind.
- Is the test stale here? No. The script is a short-lived bash that ssh runs as its LocalCommand,
  and its handler does exit, so the #960 orphan is less likely than in the login shell. But the
  HUP entry buys nothing: it exists to restore the console's stty, and a hang-up means the console
  is gone (the stty call can only fail). Without the trap bash dies on HUP at once, read or no
  read, which is the behaviour #960 asks for. INT and TERM keep the restore (Ctrl-C at a stuck
  login is the documented way out, and the console lives on).
- Fix: `trap ... INT TERM` and `trap - INT TERM`. Rejected: exempting quoted script text in the
  test (weakens it; the scan is the only guard for scripts embedded in the body).
- Risk: none seen. On msys a closed ConPTY delivers SIGHUP; default action ends the script.

## 2. editing_the_buffer_during_a_secret_read_never_requests_a_query_prediction: TEST fault

- Panic at the CONTROL: `control: locked AI input asks for a prediction` (queued == 0) while
  `may_send` on the line before it passed (`app/src/terminal/input_test.rs:11321-11323`).
- The app still asks: `handle_editor_event_inner` sends on `debounce_ai_query_prediction_tx` when
  the event is an edit, `PredictAMQueries` is on (the test overrides it), NL autosuggestions are
  on (`settings/ai.rs:1992` default true, `is_any_ai_enabled` / `is_active_ai_enabled_internal`
  default true, test mode is `ExecutionMode::App`), the editor is not empty, and
  `may_send_buffer_as_partial_query` holds (`app/src/terminal/input.rs:10378-10390`). All true at
  the control.
- Why the count is 0: the receiver of that channel is inside `debounce(...)` handed to
  `ctx.spawn_stream_local` (`input.rs:3622-3631`), and `AppContext::spawn_stream_local` polls the
  stream on the BACKGROUND executor, a real tokio worker thread
  (`crates/warpui_core/src/core/app.rs:5609-5627`, `async/native/executor.rs:143-190`).
  `Debounce::poll_next` takes every queued item at once (`app/src/debounce.rs:101-113`). The test
  reads `tx.len()` after the edit: a race the worker normally wins, so the control read 0. The two
  "nothing is sent" assertions passed for the same reason they would pass with the privacy gate
  removed: they prove nothing.
- So #1927 (2fbc7bcab) did not lose the legitimate prediction; its test never could see one.
- Fix (test only, `input.rs` untouched): the test replaces `debounce_ai_query_prediction_tx` with
  the sender of a channel whose receiver it keeps, and counts on the receiver. Nothing else reads
  it, so 0 means "not sent" and the control cannot lose a send. The old channel closes, which
  ends the debounce stream; its callbacks are no-ops.
- Rejected: a `#[cfg(test)]` counter in `input.rs` (out of scope file, not needed); awaiting the
  250 ms debounce and checking the request (a timer in a test, and it needs a completed block).

## 3. the_readiness_tail_is_the_cursor_row_not_the_blank_rows_below_a_clear: TEST fault

- Panic: `blank rows sit below the cursor: (91, 90, 2)` = (grid len, cursor row, cursor col)
  (`view_test.rs:26562-26566` on this branch).
- The view tests' terminal is 7 columns by 10 rows (`view/testing.rs:44-50`:
  `SizeInfo::new(vec2f(7., 10.5), 1px, 1px, ..)`). The fixture's 40 `old screen N` lines fill rows
  0..=80 (history 71), `\e[H\e[J` homes to row 71, and the three menu lines (57, 25 and 44
  characters) wrap to 9 + 4 + 7 = 20 rows: twice the screen. The screen scrolls, the cursor ends on
  the grid's last row (90 of 91) at column 44 mod 7 = 2. No blank row is left below it, so the
  premise assertion fails before the judge is asked anything. The numbers match exactly.
- The app code under test (`take_subshell_pane_look`, `view.rs:23421-23464`, classifying
  `contents_through_row_to_string(cursor.row, ..)`) is not shown wrong by this. The test was added
  by 82724beb3 and had never been run (`docs/testing.md` row: "Not compiled or run yet").
- Fix: a menu that fits the mock grid: `(q)` (1 row) and the footer (7 rows) leave the cursor on
  row 78 of 81 with two blank rows below; the prompt after the answer is `% ` (row 79, one blank
  row still below, now asserted too); column 0 on row 80 is `Tail::None`. Same three tail
  assertions, same premise, plus the premise for the second look. `BlockGrid::len` counts to the
  furthest row the cursor reached (`blockgrid.rs:184-192`), and an erase does not pull it back, so
  the blank rows are in the grid as they are after a real `clear`.
- Rejected: resizing the mock terminal for this test (no test in the file does; the 7x10 size is
  shared by every view test).

## 4. an_idle_dash_prompt_with_nothing_typed_is_written_on_the_look: TEST fault

- Panic: `Legacy typed=true: left: true, right: false` at the last assertion: `written` is not
  empty although no setup was written (every `readiness_state` assertion inside passed: "armed").
- `written` (`subshell_pty_writes`, `view_test.rs:16673`) collects every `WriteBytesToPty` /
  `WriteHandoffUserBytesToPty`. In the `typed` leg the test itself types `x` with
  `write_user_bytes_to_pty`, and bytes typed while a setup is armed pass through to the pty by
  design (`view.rs:23978-23982`; the neighbouring test asserts `ls\r` "passes through as typed" in
  both modes and is green). So `written == b"x"`: the user's byte, nothing of the setup. The
  judge behaved as the test's name and its in-closure assertions say.
- The "Legacy" in the message is only the first mode of the loop; Table was never reached.
- Fix: the last assertion says what is meant: `typed` -> the pty got exactly `x`; not typed -> the
  probe was written (non-empty, as before). Stronger than before (a lost or doubled key fails it).
- Added by ee42c46b0, never run before this job.

## 5. a_users_own_command_cancels_the_armed_setup_and_nothing_is_written: TEST fault (stale fixture)

- Panic: `UserDrove: left "armed", right "idle"`.
- The test arms with `vim_in_front()` (kind Unknown, leader `Other`) so that nothing is written,
  draws `bash-5.1$ `, types `ls` + Enter and expects Cancel(UserDrove).
- 37295f950 (2026-11-09, Mac round 7: vim's `:wq` + Enter cancelled the attempt and the session
  was never Phosphorized) added `program_owns_tty` to `on_user_line_ended`
  (`view/subshell_ready.rs:517-540`): a non-empty line ended while the tty's leader is `Other` is
  the program's line and re-arms. #1936 made the view sample the tty AT THE KEY
  (`view.rs:24001-24040`). With the fixture's tty still saying "vim is the leader" at the Enter,
  ReArm is the rule working; that commit updated the pure tests and not this view test.
- The expectation (a command the user runs in the SHELL cancels) is unchanged and still what the
  code does when the leader is a shell. The fixture contradicts its own scenario.
- Fix: the scenario as it happens: armed while vim is in front (nothing written); vim is gone and
  bash's line editor reads (the tty cell changes, no output event, so no look sees it); `ls` +
  Enter in the same update: judged with the facts at the key (LineReader, leader Shell), the tail
  before the line was a shell prompt: Cancel, Idle. The rest of the test is as it was.
- No new test for the vim leg: `a_line_typed_into_the_program_does_not_hide_that_the_typed_ahead_line_already_ran`
  (`view_test.rs`, Legacy and Table) already holds it at the view level (`:wq` + Enter with vim in
  front stays armed, the keys reach the pty, the setup is written at the prompt that follows).
- Rejected: changing `on_user_line_ended` back (it would bring the Mac round 7 defect back);
  arming with `SubshellTtySample::unknown()` (leader Unreadable is the no-probe shape; it says
  less about the scenario than a shell leader at the key).

## Risk

Script: one word removed twice, in a path only msys Git Bash takes. Everything else is test
code. None of it was compiled; doubts are listed in the report.
