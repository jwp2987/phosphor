# r10-wl-zsh-newuser: `subshell_ready_zsh_newuser_install` on Linux Wayland (round 9b)

Rows (build/r18w9b 413cd4241, pipeline 3048013125, job app-test-linux-wayland 17885063240),
terminal AND agent mode, same text in rounds 7, 8 and 9b:

- `subshell_ready_zsh_newuser_install` FAIL: "nothing was written into the newuser-install menu;
  zshnew: the line typed into the inner shell ran (file []); the menu footer read as an InputPrompt
  in at least one sample; zshnew: the shell in front after leaving is the pane's ROOT shell (got
  none)".
- `pane_not_local_after_subshell_ready_zsh_newuser_install` FAIL (the app is relaunched).

Nothing was compiled or run for this note. Evidence: the two case logs, the FAIL screenshots, the
case and harness source, and a real zsh 5.9 (Ubuntu, the box's version) driven on a pty on the
laptop (no app involved).

## Verdict

| failing check | verdict | why |
|---|---|---|
| nothing was written into the menu | TEST fault | there was no menu in the pane: the zsh sat at its own prompt, and writing the setup there is right |
| the typed line ran (file []) | TEST fault, consequence | the case's `q` (meant for the menu) went into the app's editor in front of the line: `qecho ...: command not found` |
| the footer read as an InputPrompt | TEST fault, consequence | no footer was ever on screen |
| root shell in front (got none), `pane_not_local_after_*` | TEST fault, second cause | the PID probe writes `~/.appci_pid`; the inner zsh had another HOME, so the answer landed where the harness does not read |

The app did what it should in that run. #1934's fix (cursor-row tail, LineReader needs a prompt
shape) is not disproved and not proven by it: the menu path has never been reached in the app, in
any round.

Issues: #2015 (the harness faults), #2016 and #2017 (two APP gaps found by reading the path the
repaired case will reach for the first time; both worked on this branch, unproven). #1934 got a
note.

## Cause 1 (TEST): the pane's zsh never ran zsh-newuser-install

- The Linux app is started with `export SHELL=/bin/bash ZDOTDIR="<scratch home>"`
  (`script/realapp/appci/app.py:9`; macOS `app.py:256`), and the scratch home has a `.zshrc`
  (`profile.py:242`). The pane's bash hands ZDOTDIR on.
- The case typed `HOME=~/w/lab_zsh_home zsh` (`cases_subshell_ready.py:538` before this branch).
  zsh looks for its startup files in `$ZDOTDIR` first (`zsh-newuser-install:31`:
  `zd=${ZDOTDIR:-$HOME}`; the zsh/newuser module tests the same directory), finds the scratch
  `.zshrc`, and starts at its `%~%# ` prompt. No menu.
- The case's guard, `zsh_runs_newuser_install(home)`, ran `zsh -i` with only HOME set, in the
  HARNESS's environment, which has no ZDOTDIR: there the menu does show, so the case did not skip.
- Reproduced on the laptop with a real zsh on a pty: `HOME=<empty> ZDOTDIR=<dir with .zshrc>`:
  no menu, prompt is the full path followed by `% `; `HOME=<empty>` alone: menu;
  `ZDOTDIR=<empty dir>`: menu.

Hops of the terminal-mode log (agent mode is the same):

- `enter_begin buffer="HOME=~/w/lab_zsh_home zsh"` t=...894204
- `subshell_setup_armed trigger=banner` t=...895962, then at once
  `subshell_ready_judged source=armed decision=Write(None) kind=LineReader leader=Shell comm="zsh"
  tail=ShellPrompt queue_empty=Some(true) bytes=0 lines=0 age_ms=0`: a line editor at a prompt it
  drew, nothing typed. That is the rule for an idle zle prompt, applied correctly.
- `subshell_setup_ready_write`, marker 6 ms later, `InitSubshell` 23 ms, stage 2, `Bootstrapped`
  at 919 ms, `subshell_setup_net_cleared setup over`. The setup ran cleanly: a zle read every line.
- `enter_begin buffer="qecho T284_y52tty >> lab_zshnew.txt"`: the case's `q` and its line, in the
  app's editor (`kh_pane_key ... target=Editor`), run by zsh as one command.
- Screenshot: "Session Phosphorized", then the block `qecho ...` with `zsh: command not found:
  qecho`. The blocks show the working directory as a full path, as a `%~` prompt does when HOME is
  another directory.

Why the "menu aborted by the setup's first byte" reading was ruled out: it produces the same hops
(the first byte of the setup is a space, `bootstrap.rs` "every line starts with a space", and any
key but q/0/1/2 makes the menu print "Aborting." and return, after which zle reads the rest:
reproduced on the laptop). What decides is the environment: with ZDOTDIR pointing at a directory
that has a `.zshrc` the menu cannot run at all.

## Cause 2 (TEST): the PID probe could not answer from that zsh

`leave_nested` (`cases.py:1021`) proves which shell is in front with
`sh -c 'echo tok:$PPID >> ~/.appci_pid'` and reads the file in the scratch home
(`where_homes`, `cases.py:975`). In a zsh started with `HOME=~/w/lab_zsh_home` the token goes to
`lab_zsh_home/.appci_pid`. Two probes (`pT285`, `pT286` in the log and the screenshot) ran in the
zsh, none was seen, `exit` was rightly not typed, the result was "none" and the pane was relaunched.
This would have failed the case even with a menu on screen.

## Fix (TEST, #2015), `script/realapp/appci/cases_subshell_ready.py`

- The case types `ZDOTDIR=~/w/lab_zsh_home zsh`: the empty directory is the one zsh looks in, and
  HOME stays the scratch home, so `~/.appci_pid` and `~/w` keep working. One change removes both
  causes. The banner still shows: the matcher strips leading assignments
  (`warpify/mod.rs:37 strip_env_prefix`).
- The guard starts its zsh the same way (ZDOTDIR = the empty directory, HOME = the scratch home, a
  24x80 pty: the menu refuses a terminal under 15x72) and looks for the footer text, not the word
  "newuser" (which is also in the function's error messages).
- New precondition, read from the OS and not from the screen (Linux; elsewhere a note): the zsh
  under the pane's root shell has `ZDOTDIR` = the empty directory in `/proc/<pid>/environ`, and
  its tty is at least 15x72. A wrong environment FAILS with "TEST precondition", a small pane
  skips. The case can no longer run its checks against a zsh that has no menu.
- No check was removed or loosened. Self-test: `subshell_ready_newuser_guard` (the environment the
  guard builds, the menu marker, the `/proc` environ parser, the precondition verdicts).

Rejected: unsetting ZDOTDIR for the whole app (other cases start `zsh` and need the scratch rc);
`env -u ZDOTDIR HOME=... zsh` (keeps cause 2); a symlink `lab_zsh_home/.appci_pid` (works, but
keeps a second HOME for no gain); OCR of the menu (slower and less exact than `/proc`).

## What the repaired case reaches next (read from code, unproven)

With the menu on screen: the footer is on the cursor's row, `take_subshell_pane_look`
(`view.rs:23436`) reads `InputPrompt`, the judge waits (`subshell_ready.rs:352`). The real `clear`
is `ESC[H ESC[2J ESC[3J` (captured from zsh on the laptop), not the `ESC[H ESC[J` of the view test;
in a block with no output yet `ESC[2J` clears in place (`ansi_handler.rs:943-960`, #986) and
`ESC[3J` has no history to drop, so the cursor row is the footer row either way.

Then the user answers `q`. Two gaps:

### APP gap A (#2016): a one-key answer leaves the setup armed until the user ends a line

- `q` is one byte with no line end. `note_user_bytes_for_subshell_readiness` (`view.rs:24011-24015`)
  counts it and returns; the hand-off marks the shell's line dirty
  (`typeahead_handoff.rs:4133-4136`, `wrote_user_bytes`), and only Enter, ^C or ^U clear that.
- The menu takes the key and returns; zle draws its prompt. Every look from then on is
  `Wait(LineDirty)` (`subshell_ready.rs:349`): the app believes `q` sits in the shell's line.
- The setup is written only after the user's next Enter (whose line is judged as "answering" the
  menu's question, since `tail_before_line` is still the footer), or never (60 s ceiling, toast).
  The case's check "the setup was written at the zle prompt after the menu" would fail.

Fix: a pure rule, `subshell_ready::key_read_took_the_typed_keys`. The keys of the line being typed
are known to be gone, taken by a key read, when ALL hold:

1. before the first of them the line was clean (the hand-off said so at the last look);
2. that look showed a question on a raw tty under a shell leader (`InputPrompt`, `LineReader`);
3. now: `LineReader`, queue empty, a shell-shaped tail;
4. on a row BELOW the row of the question, not a soft-wrap continuation;
5. the row does not show what was typed (`row_shows_typed_line == Some(false)`; a line whose echo
   cannot be predicted never qualifies);
6. the row does not begin with the question's own text (the same prompt drawn again).

A line editor that holds typed text redraws it with its prompt; it does not draw a different prompt
on a lower row without the text. The view (`judge_armed_subshell_setup`) then forgets the typed
line for THIS attempt only (`key_read_took_line`, cleared by the next user byte), logs
`subshell_ready_key_read`, and judges with a clean line: LineReader + ShellPrompt + queue empty =
Write. The hand-off's own flag is left alone (it is what every raw program that takes a key leaves
behind today; the next prompt's report corrects it).

Risk: a false "taken" would let the setup's `^K ^U` kill typed text. It needs a line editor whose
own prompt reads as a question, a different prompt text drawn on a lower row without a line end,
and the typed text hidden on that row, with a clean line before the first key. Condition 1 exists
for the one realistic case found: text typed BEFORE Phosphorize, then zsh's "do you wish to see all
N possibilities?" answered with one key (the buffer is redrawn on a lower row); the line was dirty
before the key, so the rule does not apply. Windows (no tty probe: never `LineReader`) is untouched.

Rejected: clearing the hand-off's `wrote_user_bytes` (more state, other readers of that flag);
treating any key at an `InputPrompt` as a line end (a `read -p` answer typed without Enter would
count as consumed); waiting for a cooked look between the read and zle (not guaranteed to be
sampled: looks happen at output events only).

### APP gap B (#2017): the menu's other prompts read as a shell prompt

`output_tail_is_input_prompt` (`key_facts.rs:372`) knows the footer "Type one of the keys in
parentheses". zsh-newuser-install also asks `--- Type a key ---`, `--- Edit abandoned, type a key
---` and `--- Type a key in forlorn hope ---` (`zsh-newuser-install:42,231,467,544,603`), raw, one
key, under the same shell leader. Those tails classify as `ShellPrompt`: a Phosphorize click there
(and, with gap A's rule, the look after an earlier menu key) writes the setup into the menu. Fix:
"type a key" is an input prompt too. Pure; rows added to
`choice_groups_and_the_zsh_newuser_footer_are_input_prompts`.

## Tests

- appci self-test `subshell_ready_newuser_guard` (harness, no app).
- `key_facts_tests.rs`: the three other menu prompts are input prompts; a shell prompt is not.
- `subshell_ready_tests.rs`: truth rows of `key_read_took_the_typed_keys` (each condition alone
  turns it off), `LineTracker::typed_so_far`.
- `view_test.rs`: `a_menu_key_answer_does_not_leave_the_setup_armed`: menu, Phosphorize (armed,
  nothing written), `q` (line dirty: asserted), prompt on the next row: written; and the same with
  the key shown at the new prompt: still armed.
- appci case unchanged in what it demands; stays in `cases.PROVING`.

## What the next Linux run must show

- `subshell_ready_judged ... kind=LineReader ... tail=InputPrompt decision=Wait(InputPrompt)` at
  Ctrl-I, no `subshell_setup_ready_write` before the `q`.
- After the `q`: `subshell_ready_key_read`, then `subshell_setup_ready_write`, `Bootstrapped`.
- `echo tok >> lab_zshnew.txt` runs in zsh, no `.zshrc` in `lab_zsh_home`, the probes answer, one
  `exit`, the root shell is in front; no `pane_not_local_after_*` row.
- If the row says "TEST precondition", the harness is wrong again, not the app.
