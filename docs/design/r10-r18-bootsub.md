# Round 18: two round-14 unit failures (r18-bootsub)

Branch `fix/r18-bootsub`, from integ/r14 `42ef67839`. Run judged: build/r14a (the same commit),
pipeline 3052606915, job test-linux. Nothing here was compiled or run; Rust is `rustfmt --check`
only. Both verdicts are **TEST** (the fixture), reasoned from the trace and the code; "worked,
unproven" until a run shows the two tests green. No app code changed.

## A. `terminal::bootstrap::tests::msys_ssh_wrapper_sends_the_bootstrap_in_the_remote_command` (#2051)

**What the trace shows.** `bootstrap_test.rs:5414`, `in-command: zsh login`: left is the
`PlainSSHFallback` hook, then the CI host's message of the day, then `LOGIN zsh -l`; right is the
hook and `LOGIN zsh -l`. Nothing else differs.

**No ssh ran and nobody logged in.** The first reading of the trace was that the stub was bypassed
and a real ssh reached the host. The evidence says otherwise:

- The wrapper stage of the same case had already passed its assertions with the stub `ssh`
  (two `-G`, one `-V`, two connecting calls recorded in the stub's own file, one `KEYSCAN`).
- The failing stage starts no ssh at all. It runs the remote command the wrapper built with
  `bash -c` on the test machine (`run_far`), with stubs for the remote `bash` and the login
  shells. `LOGIN zsh -l` in the output is the stub's line: the stub login shell was used.
- The banner is printed by the remote command itself. Its plain login is
  `if test ! -e "$HOME/.hushlogin"; then <cat the first readable of /etc/motd, /run/motd,
  /run/motd.dynamic, /usr/lib/motd, /usr/lib/motd.dynamic>; fi; exec "$SHELL" -l`
  (`bash_body.sh:2748-2761`, and `2683-2698` for the mode-byte arm). The test's remote HOME
  (`far`) has no `.hushlogin`, so on a machine with one of those files the file is printed.
  The CI host has one; the machine the test was written on has none of the five (checked), which
  is why the expectation was written without it.

**Verdict: TEST.** The "remote" of this test is the machine it runs on, and the expected text
assumed that machine has no MotD file. The two assertions behind the failing one (`mode byte P`,
`no mode byte`) have the same fault and would have failed next.

**The expectation, judged.** The app behaviour is right and is not changed: sshd prints no MotD
for a session with a command, so a wrapper that turns the login into a command prints it itself
before the plain login; the path for a login shell that is neither bash nor zsh does the same
(`bash_body.sh:2794-2817`), and `r13-gbssh.md` writes the plain login as `<MotD>; exec "$SHELL" -l`.
A managed bash login prints nothing there (its bootstrap prints the MotD, `bash_body.sh:3398`);
the test's managed run already proves the output starts with the SSH hook and the stub's line.

**Fix (test only).** Each plain login is run twice:

1. with `.hushlogin` in the remote HOME: the exact old expectation, the same on every machine;
2. without it: the same text with this machine's MotD (the first readable file of the same list,
   possibly none) between the hook and the login shell's line. This is a NEW assertion: on a
   machine with a MotD file it proves the MotD is printed, once, after the hook and before the
   shell; on a machine without one it equals run 1.

`/run/motd.dynamic` is rewritten by a login on the machine; the MotD is read before and after the
run and the run repeated (at most 5 times) if it changed meanwhile. No timer.

**Rejected.** Only adding `.hushlogin` (drops the MotD from what is tested). Comparing with
`ends_with` / `contains` (weaker than the exact text). Pointing the command at other MotD paths
through a variable (app change for a test).

**Isolation otherwise.** Read for the task's question "can this unit test reach a real ssh":
the wrapper is run with the stub directory first in PATH and calls `ssh` / `ssh-keyscan` by name
(`command ssh`), never by absolute path; HOME is the temp directory. No path to a real ssh found.

## B. `terminal::view::tests::e_unsupported_is_stream_ordered_and_delivers_the_held_keys_at_the_fresh_prompt`, `LineEditor` arm (#1991)

**What the trace shows.** `view_test.rs:35267`: after the error line and a fresh prompt the state
is `written`, expected `idle`. Every earlier assertion of the arm passed (the write recorded as a
line-editor write, the filter unarmed, nothing concluded while the line is drawn).

**Cause: the fixture's pane is 7 columns wide and its prompt is 10.** `TerminalView::new_for_test`
(`view/testing.rs:43`) builds the pane from 7 x 10.5 px with 1 px cells: 7 columns, 10 rows. The
arm's prompt `bash-5.1$ ` soft-wraps there (`bash-5.` / `1$ `), so at the final look the cursor's
row continues the row above it (`take_subshell_pane_look`: `row_soft_wraps(cursor.row - 1)`).
`line_editor_took_the_line` (`view/subshell_ready.rs:1082`) refuses such a row on purpose ("the
row does not continue the row above it (a soft wrap is one logical line, still the editor's)"),
so `fresh_prompts_after_echo` stays 0 and `judge_written` keeps the attempt. The other four
conditions hold at that look (prompt-shaped tail, same width, the row far enough below the write,
the row's text not the line).

Checked with a cursor model of the grid (pending wrap at the last column, the wrap flag set when
the wrap is executed, as `ansi_handler.rs:1782`), for every length the setup line can have (the
token is a random u64, 1 to 20 digits): with the 10-character prompt the final look is never
proof; with a prompt that fits it always is, and the look while the line is drawn never is.
The `CookedTty` arm passes because its proof is the echo filter, which does not ask about rows.
The pure rule's own tests (`subshell_ready_tests.rs`) passed in the same run.

**Verdict: TEST.** The rule did what its design says (`r10-r17-queue3.md` section 3, item 3). The
arm was written without a build and assumed a pane wider than its prompt.

**Fix (test only).** The arm's prompt is `sh$ ` (fits; not `$ `, whose whole text occurs in the
setup line and is never proof, an accepted loss of #1991), and the test asserts that its prompt
fits the pane before drawing it, so a change of the test pane's size fails with that reason. No
assertion removed or changed.

**Rejected.** Resizing the test pane to 80 columns (no view test does; a resize is an event of
its own for the judge, unrun). Relaxing the rule so the test passes (see below).

**For the owner, not decided here.** A prompt that soft-wraps (a long path in a narrow pane) is
never the grid's proof, so an unsupported line-editor shell is not noticed at that prompt and the
quiet net ends the attempt, as before #1991. That is the safe side and follows from the rule as
designed, but the note's "Accepted losses" does not list it. Judging the whole logical line
(its first row against `first_row_after_accept`, its joined text against the written line) would
cover it; it is a rule change with its own risk and was not asked for.

## What the next run should show

Both tests PASS on Linux (test-linux). On the CI host test A now also proves the MotD placement
(run 2 differs from run 1 there). The macOS trace of round 14 has no line for either test (not run there
or not reached); on macOS A's run 2 reads `/etc/motd` if present.
