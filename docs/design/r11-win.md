# Round 11: the Windows failures of round 10 (design note)

Base: integ/r11 fb6236c58. Round 10: build/r10a, integ/r10 5dd50d49c, pipeline 3048440714, job
app-test-windows 17887888287. Evidence: the job's artifacts (per-case app-log slices under
`run_slim/logs/<mode>/case_<name>.log`, screenshots under `run_slim/shots/`); the committed copy in
`test-results/2026-11-10_build_r10a_3048440714/app/windows/` has the rows but not the slices. Round
9 for comparison: `test-results/2026-11-10_build_r18w9_3047937983`. Nothing was compiled or run.
Every change is "worked, unproven".

Ten failing rows, seven cases. Verdicts: five APP causes (two new issues, three on open issues),
two TEST.

| case | passes | verdict | issue |
|---|---|---|---|
| `giveup_keys_reach_shell` | terminal, agent (PowerShell) | APP | #2024 (new) |
| `gitbash_precmd_secret` | terminal@gitbash | APP, the #1923 fix read the wrong grid | #1923 |
| `subshell_ready_rc_questions` | terminal@gitbash, agent@gitbash | APP, a second defect behind #2006 | #2025 (new) |
| `subshell_ready_rc_clear` | terminal@gitbash | APP, same | #2025 |
| `subshell_ready_rc_clear` | agent@gitbash | APP, the probe-less judge has no positive fact | #2006 |
| `agent_fault_queued_rows` | agent@fake, agent@gitbash+fake | TEST, no OCR on the host | none |
| `ctrl_c_d` | terminal@gitbash | TEST, noise inside the gate's threshold | none (owner question) |

## 1. giveup_keys_reach_shell: APP (#2024)

Not the #2012 reorder of round 9 and not a give-up path (#1996, #1997): `give_ups=0`,
`lost-before-app 0`. The first characters of a typed line stay in PowerShell's own line.

Log (terminal mode, the seventh round), in order:

    hop=precmd_seen consumed=false seq_mode=true enters=2 precmds=0
    hop=preexec_received signal=preexec                       t=...224.013
    hop=line_editor_reading_activates released_queued_hold=false  t=...224.276   (no decision follows)
    hop=pty_write_bytes class=UserKey ... bytes="A"
    hop=pty_write_bytes class=UserKey ... bytes="d"
    hop=precmd_seen consumed=false seq_mode=true enters=2 precmds=1   t=...249.158
    hop=handoff_request_skipped reason=line_editor_not_proven         t=...254.786
    hop=handoff_decision decision=Committed retry=false
    hop=line_editor_reading_activates released_queued_hold=false      t=...261.352   (this prompt's own signal, too late)
    hop=enter_begin deferred=false buffer="d-Content gup_... t217_..." input_type=AI

Agent mode, same signature: `buffer="ntent gup_... t129_..."`. In the rounds that passed, the
second prompt reads `precmd_seen`, `line_editor_reading_activates`, `decision=SendRequest`,
`input_report text="Add-..."`, `handoff_insert`.

Cause, `app/src/terminal/line_editor_status.rs`:

- The marker of prompt 1 reached the main thread after the model had parsed the Preexec of the
  typed-ahead empty line. `activate_line_editor_on_ready_signal_inner` found a line queued ahead
  (`has_unanswered_lines_for_activation`; `counts_provably_stale` is false once the last parsed hook
  is a Preexec), so it only remembered the signal (`expedite_pending_activation`:
  `signal_activation_pending = true`, `awaiting_signal` stays set).
- The Preexec event then ran `set_line_editor_inactive`, which recorded `signal_missed` because
  `awaiting_signal` was set.
- `on_prompt_hook` of prompt 2 demoted the session (`ready_signal_seen.remove`, `signal_wait =
  None`) and emitted `Active` at the Precmd, unproven. Unproven means no line-report request
  (#1798); `on_line_editor_active(at_prompt, can_request = false)` with user bytes written commits
  to the editor. `A`, `d` stay in the shell.
- The damaged line is not a command; with the owner's settings its Enter goes to the agent
  (`enter: Undecided by model`), the view switches to the conversation (`hop=view_mode mode=agent`
  in the terminal pass), and the following lines are steers (#851, by design). That is the whole
  6/20.

Fix: `set_line_editor_inactive` records a miss only when no signal was remembered
(`awaiting_signal && !signal_activation_pending`). The session stays a signalling one, prompt 2
opens its gate and waits for its own signal, which arrives 12 ms later and sends the request.

Rejected:

- Sending the request unproven for a PowerShell session. #1798 forbids it (a reader that is not
  reading echoes `^[i`); the proof arrives a few ms later anyway.
- Not demoting PowerShell sessions at all. The marker has a kill switch and Unix pwsh can lose
  it; the #1825 demotion is the net for that.
- Making `Committed` with `can_request = false` keep the pty as owner when bytes were written.
  That is the rule for a shell with no binding; changing it reaches every such shell.

Risk: low. One more condition on the miss, which can only keep a session signalling. A session
that really stops signalling while a signal sits remembered is impossible: the remembered signal
is the signal.

## 2. gitbash_precmd_secret: APP, #1923's screen fact read the wrong grid

Log, after `touch gbgo.txt`:

    hop=precmd_seen consumed=true seq_mode=true enters=1 precmds=0
    hop=commit_deferred_awaiting_line_editor
    hop=hand_route reason=held_prompt_hold_started state=pty_owned awaiting_ready=true ... bytes="p"
    hop=kh_iq_queue ... gate_open kind=line_editor_active key=44 ... opened_by=precmd_on_conpty
    [WARN] typeahead prompt hold 44: no line-editor ready event within the cap; 6 held entr(ies) wait for the shell
    hop=prompt_hold_kept_overdue hold=44 entries=6

Screenshot `terminal@gitbash_gbps_after_giveup.jpg`: `pin: ` with the cursor behind it, under the
toast "Your typed input is waiting for the shell to be ready". So #1923's first half works (the
Precmd no longer hands input to the editor) and its second half does not: at the cap
`screen_tail_asks_for_input()` was false with the question on screen.

Cause, `app/src/terminal/view/paste_queue.rs`: the fact read `active_block().output_grid()`. The
question is printed by a precmd function, after the Precmd, into a block that has not started:
`Block::active_grid_type()` is `PromptAndCommand` then, the output grid is empty, cursor column 0.

Fix: `key_facts::active_block_tail_asks_for_input(block)` reads the grid of
`block.active_grid_type()`; a running command's question is read from the output grid as before.

Rejected: reading both grids (a finished command's output is not the tail); an OS fact (none on
ConPTY); a bash-side marker for `read` (a larger design, #1973).

Risk: low. The fact is consulted in one place, at a cap, and can only turn "keep holding" or "hand
back" into "write the held keys to the pty in typed order", which is what a plain terminal does.
A shell prompt that looks like a question (ends in `:` or `?`, or names a secret word) at a prompt
whose line editor did not signal within 1.5 s now takes that path.

Open, read from the code and not proven: the Enter that ends the `read` is counted as a line by the
hand-off and no Precmd answers it. The cases after this one in the same pane show whether the
editor takes input again at the next prompt.

## 3. subshell_ready_rc_questions (both modes), subshell_ready_rc_clear (terminal): APP (#2025)

The #2006 note said: "If the next run shows the setup written at the prompt AFTER the inner line ran
and the row still fails, the tail is a second defect and needs the app-log slice." This is it.

What held (rc_questions, terminal): `verdict=ReArm` twice, `decision=Wait(UserLineAtCursor) ...
user_line_at_cursor=true` after each, `decision=Write(None)`, one `subshell_setup_ready_write`, no
cancel, the answers check passed.

What failed, same slice:

    hop=subshell_setup_progress kind=InitShell gen=2 seq=7 age_ms=2760
    hop=pty_write_bytes class=SetupChunked wrote=2048 ...        (41 of them, 83 KB in 1.1 s)
    hop=decset_2004 on=false / on=true                            (32 to 49 pairs per second for 13 s)
    hop=ctrl_c_route route=pty long_running=true                  (the harness, 15 s after the write)
    hop=subshell_setup_given_up why=quiet_net gen=2
    [WARN] Abandoning a subshell bootstrap that never reported Bootstrapped (#1014)

Cause, `pty_controller.rs` `write_bootstrap_script_to_shell`: the pane's Git Bash is sourced a
temporary file because its launch data is MSYS2. A subshell has no launch data
(`pending_shell_launch_data` was taken by the pane's shell), so the inner bash takes the generic
subshell branch: the here-document typed through readline. msys bash behind ConPTY takes it at
about 40 lines per second.

Fix: a bash subshell started by plain `bash`, local, no ssh / container / tmux / WSL, whose chain
of spawning sessions leads to an MSYS2 shell through plain-bash subshells only, takes the existing
rc-file branch: one `source '<temp file>'` line. The decision is pure
(`bootstrap::msys2_bash_subshell_sources_file`); the chain walk is
`PtyController::spawned_from_msys2_through_plain_bash`. Switch `PHOSPHOR_MSYS2_SUBSHELL_SOURCE=0`.

Rejected:

- `cfg!(windows) && bash && subshell` (the zsh rule's shape). The bash init payload carries no
  `wsl_name`, so a WSL bash would be told to source a Windows path.
- Adding `wsl_name` (or `$OSTYPE`) to `bash_init_subshell.sh`. Right in the long run; it changes a
  script every platform types, whose length is checked by the setup. Not without a run.
- A counted raw read (`head -c`, #1226). Not validated on msys behind ConPTY.
- Raising the nets. 30 to 90 s of held input is the defect.

Risk: medium, Windows only. The branch is the one the pane's own Git Bash and every Windows zsh
subshell already take. If the temp file cannot be read the shell prints one error and the quiet net
abandons the setup as it does today.

Not covered: Git Bash started from a PowerShell pane (`bash` may be the WSL launcher there).

## 4. subshell_ready_rc_clear, agent@gitbash: APP (#2006 follow-up)

    hop=decset_2004 on=true                                  t=...558.560
    hop=subshell_setup_armed trigger=banner                  t=...558.592
    hop=subshell_ready_judged source=armed decision=Wait(Unknown) ... tail=ShellPrompt ... bytes=0
    hop=subshell_ready_line_end ... tail=ShellPrompt at_key=false verdict=Cancel
    hop=subshell_setup_cancelled why=UserDrove trigger=banner bytes=33 lines=1

The rc had finished and bash had drawn `LAB$ ` 32 ms before the arming. The probe-less branch
writes only at a prompt drawn since the baseline, so nothing could ever make it write; the user's
line then cancelled it (correctly: a line at a shell prompt is the user driving). The terminal pass
armed 20 ms before the prompt and wrote. A race the judge had no fact for.

Owner decision 2 of 2026-11-10 (in `r10-subshell-gitbash.md`): build a "line editor reads" fact from
DECSET 2004 only if a Windows run shows it in stream order before the line editor's prompt and not
before a `read -p` prompt. Round 10 shows both, in both modes: `on=true` directly before every look
at `LAB$ `; between the outer shell's `on=false` and the looks at `Name: ` / `Team: ` none.

Fix, narrow: `ReadinessSample::line_editor_announced` (no tty probe, and bracketed paste is on in
the running block). With no user byte passed, a shell-shaped tail plus the announcement writes.
After user bytes nothing changes. Switch `PHOSPHOR_SUBSHELL_READY_DECSET_2004=0`.

Rejected: REQUIRING the announcement for every probe-less write (closes the `read -s -p "pin> "`
hole of #1973 for bash 5.1+, but breaks readline with `enable-bracketed-paste off`: owner
question); using it after user bytes (the echo rules of #2006 are the proof there).

Risk: low. One more way to reach Write, in a state that waited for the 60 s ceiling before. Same
class of false positive as the existing rule (a readline `read -e` with a shell-shaped prompt).

Third change on the probe-less branch (#1886, #2006, this). Per the "design after the second patch"
rule: the branch now has one positive fact and two shape rules; the next change there should be
the owner's answer on requiring the fact, not another shape rule.

## 5. agent_fault_queued_rows: TEST

    [INFO] hop=queued_row id=6194... state=queued
    [INFO] hop=queued_row id=12c2... state=queued
    hop=turn conv=... state=error
    [INFO] hop=queued_row id=6194... state=popped
    [DEBUG] keystroke: Agent by request-shape (#834/#965) (AgentConversation, head "please", 40 bytes)

40 bytes is exactly `please answer with the word <token>`: the first row is in the input, the second
is stored (`queued_rows_stored`). The app did what it is designed to do (`pop_front` restores one
row to an empty input after an error). The case sees the input only through OCR, the Windows VM has
no OCR engine (`mode_proof`: "no OCR engine"), so `on_screen` was empty. macOS passes the case.

Fix: the app logs `input_order hop=queued_row_restored id= buffer=` with what the input editor
holds after the restore (text only with `WARP_INPUT_ORDER_LOG_TEXT`, like `enter_begin`); the case
counts a token found there as held unless an Enter followed. Not weakened: a row seen nowhere is
still "lost".

## 6. ctrl_c_d, terminal@gitbash: TEST (noise); no change

`REGRESSION Ctrl-C: median 88.01 vs baseline build +7.77 ms (+9.7%); re-measured once: median 84.91`.

- The app's own share is unchanged and tiny: `ctrl_c_keydown->ctrl_c_to_event_loop` 0.41,
  `->ctrl_c_received` 0.08, `->ctrl_c_pty_write` 0.16 ms (p50, n=30). Round 9, which passed:
  0.42 / 0.09 / 0.17.
- The other 87 ms are the input tool's spawn, ConPTY and bash's trap. Its spread: SE of the median
  4.72 ms in round 10 and 7.32 ms in round 9; the series drifted -15.08 ms between its halves; the
  reference (conhost with bash) measured 73.5 against 76.8 in round 9.
- The gate fails at +5 % and +0.5 ms of the median. +4.67 ms on the re-measure is one SE.
- The PowerShell pane's Ctrl-C in the same run: 4.26 ms against a 4.61 baseline, PASS.

Not an app regression. The gate was not changed: making it take the measurement's own error into
account is a policy change (owner question).

## Owner questions (not decided here)

1. Should the regression gate (`lat.regress`: +5 % and +0.5 ms of the median) take the standard
   error of the median into account, or should Ctrl-C be judged on the app's own segments
   (`ctrl_c_app_segments`, 0.65 ms, stable to 0.02 ms across rounds)? Until then
   `ctrl_c_d` on Git Bash can fail on noise.
2. Should a probe-less pane REQUIRE the line editor's announcement before any setup write? It would
   close the `read -s -p "pin> "` hole (#1973) for readline with bracketed paste and stop auto-setup
   for a readline with `enable-bracketed-paste off`.
3. Should the bash subshell init script report `wsl_name` as the zsh one does? It would let a Git
   Bash started from a PowerShell pane be sourced its bootstrap too (#2025's uncovered case).
