# #1795: ssh escape `\r` gaps, items 1-4 and 6

Status: approved by the owner 2026-11-14, owner question answered (a); implemented in one commit on
`fix/2209-no-dropped-keys`, written without a compiler, not yet run. See "As built" at the end.

Read at integ `d6c6b8d0b`; nothing compiled or run. `th` = `app/src/terminal/model/typeahead_handoff.rs`,
`el` = `local_tty/event_loop.rs`, `rl` = `model/reply_ledger.rs`, `pc` = `writeable_pty/pty_controller.rs`.
Background: `TerminalView::write_ssh_escape` (`view.rs:14405`) writes `\r~X` when
`TypeaheadHandoff::ssh_escape_line_start` (`th:1789`) answers `AddCr`. Two rules decide that: the
default line-start rule (#2186, `ssh_escape_cr_blocker_line_start`, `th:1843`) and the old proof
rule behind `PHOSPHOR_SSH_ESCAPE_CR_RULE=0` (`ssh_escape_cr_blocker`, `th:1872`).

## 1. A terminal-query reply nobody read can be run by the `\r` (real hole)

Root cause. A reply (cursor position, device attributes, ...) is queued by the event loop, never
by the controller: `ReplyWriter::write` (`el:1135`) -> `ReplyLedger::note_replies` (`rl:77`), which
since #2186 only moves the write TAIL (`PtyWriteCount::note_tail`, `th:591`). It is not a counted
write, so the proof rule's `line_empty_at_writes == writes` (`th:1897-1901`) still holds after it, and
the line-start rule has no reply check at all. If the program that asked has exited, the remote
line editor reads the reply and keeps a residue (`R`, `0;276;0c`); `\r~.` then RUNS that residue.
(Replies made while a command runs are gated and dropped when stale, `el:312`/`el:1077`, so the
window is a reply written while the asker still ran and never read.)

Change. `PtyWriteCount` gains a `replies` counter, bumped where the tail is (`rl:82`). The hand-off
records it as a baseline wherever the remote line is known empty: with `line_empty_at_writes` at
the report (`th:3169`) and at a command start (`on_preexec`; the ledger's own rule, `rl:231`).
Both blockers return `"reply_since_line_empty"` when the counter moved since: the escape goes as
typed, without `\r`, and the reason is logged like every other blocker's. No timer, no new state.

Cost. A shell that reports its line (bash, zsh, fish, PowerShell) re-baselines at every prompt,
so this only ever skips for a reply written after the prompt's report: practically never. A shell
that cannot report (nu, an unknown remote shell) re-baselines only at a command start, so after a
program that queried the terminal (vim, less, a TUI) the first `~.` at that prompt is typed
without `\r` and ssh may not take it; pressing Enter first makes it work. See the owner question.

Tests: a reply after the report -> `Skipped("reply_since_line_empty")` under both rules; a report
or a Preexec after the reply -> `AddCr`; a dropped reply (`forget_replies`) stays counted (conservative).

## 2. `~C`: the Enter typed at ssh's `ssh>` prompt is counted as a remote line

Root cause. After `\r~C` the pty owns input (`th:1812-1824`). The Enter that ends the line typed
at `ssh>` is counted by `route_user_bytes_unheld` (`th:5291`) but ssh eats it: two Enters, one
remote Precmd, so the hand-off stays `PtyOwned` until the stale-count give-up (5 s). Nothing is
lost, but a give-up timer is the normal end, which the owner's rule forbids.

Change. `ssh_escape_line_start` is told the escape byte. For `C` it sets `ssh_command_line_open`;
the next Enter-terminated user write is ssh's own and is not counted (flag cleared there, and by
a `^C`, a Precmd, or a session change). If the server has the command line disabled (OpenSSH
9.2+ default) ssh prints a notice and the Enter does reach the remote: then one Precmd more than
Enters arrives, which the hand-off already reads as "answered". Harmless both ways.

Tests: `\r~C`, `-L 1:h:2\r`, one Precmd -> no unanswered line, no stale-count wait; the
disabled-command-line order (two Precmds) -> same end state.

## 3. Continuation prompt (PS2)

Root cause. Neither blocker looks at `ShellActivity::Continuation` (`th:2740`). Requests are not
written at PS2, but under the line-start rule `EditorOwned` needs no report, and the stale-count
give-up can leave `EditorOwned` at PS2: `\r` would then add a line to an open quote.
Change. Both blockers return `"continuation_prompt"` first. Test: PS2 + `EditorOwned` -> skipped.

## 4. `AlreadyAtLineStart`: keep it, it is live again

The issue calls it near-dead because after `AddCr` the pty owns input and later keys are raw.
Since #2186 that is no longer the only way in: `TailIsNewline` (`th:1798-1802`) records the line
start and leaves `EditorOwned`, so `~?` then `~.` on a shell with no request at its prompt (nu)
takes this arm (`th:1791-1796`). Dropping it would write `\r~.` there (an extra empty line).
Change: none in code; fix its doc comment (`th:624-627`) and add the missing test (tail is a
newline, `~?`, then `~.` -> `AlreadyAtLineStart`, no `\r`).

## 6. tmux `Pending` flush is one counted write

Root cause. In tmux control mode `Pending`, writes are appended to one buffer (`pc:3919`) and
flushed as ONE `PtyWrite::Bytes` at `TmuxControlModeReady` (`pc:748-754`). A request and the keys
typed after it are then one counted write, so the proof rule's "nothing written since the
request" can hold with keys in the line. Only with the kill switch on, in tmux control mode, at
an ssh escape: practically unreachable.
Change. After that flush the controller tells the hand-off the count no longer describes the
line (`line_empty_at_writes = None`, `ssh_line_start_at_writes = None`): the proof rule answers
`no_report_since_prompt` until the next report. The buffer stays one buffer. Test: flush, then
escape under the proof rule -> skipped.

## Fit with #2209 / #2227 / #2182b

Independent of all three: held keys already block the `\r` (`"keys_held"`, `th:1863`); the escape
decision takes the hand-off's write lock, an exact read (#2227); a command parked behind a side
call (#2182b) makes the pty the owner, where the escape path is not taken. One commit, about 60
lines plus tests; lab check: `escape_connect_bare` and the `~?`/`~.` cases, three lab servers.

## Owner question (item 1)

On remote shells that cannot report their line (nu and unknown shells), after a full-screen
program (vim, less) has run, should the first `~.` at that prompt (a) be sent without the hidden
Enter, so it may not disconnect until you press Enter once (safe: a leftover terminal reply can
never be run as a command), or (b) keep today's behaviour (always works, with a very small chance
of running a stray one-letter command such as `R`)? Recommended: (a). Shells that do report
(bash, zsh, fish, PowerShell) are unaffected either way.

**Owner answer (2026-11-14): (a).** On shells that cannot report their line, after a full-screen
program the first `~.` goes without the hidden Enter; a leftover terminal reply is never run.

## As built

- (1) `PtyWriteCount::note_reply` / `replies`; `ReplyLedger::note_replies` calls it. Baselines:
  the report of the last request (as of that request), `on_preexec`, `on_new_session`. Reason
  `reply_since_line_empty`. Two existing #2186 expectations changed with the owner's answer: a
  reply written last used to get `\r~.`, now `~.` (`test_ssh_escape_cr_rule_skips_only_for_user_bytes`,
  case `reply`).
- (2) No signature change: the view calls `note_ssh_escape_written(byte)` after it wrote an
  escape that was not `Skipped` (and the #2207 deferred-Enter path does). The flag is cleared by
  the Enter it skips, a `^C`, a command written, or a new session; NOT by a Precmd as the text
  above says (the Precmd of the `\r` ahead of `~C` arrives before the user finishes the `ssh>` line).
- (3) `continuation_prompt`, first in both rules (`ssh_escape_cr_blocker_common`).
- (4) Doc comment only; the chain test is `escapes_chained_after_a_newline_stay_at_sshs_line_start`.
- (6) `TypeaheadHandoff::on_writes_flushed_as_one`, called after the tmux pending flush; it also
  voids the request's own count so its report cannot re-create the proof.
