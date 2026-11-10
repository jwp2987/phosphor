# Round 11: the round 10 failures on Linux Wayland and in the lab

Branch `fix/r11-wl`, base integ/r11 `fb6236c58`. Run judged: build/r10a (integ/r10 `5dd50d49c`, pipeline 3048440714), jobs `app-test-linux-wayland`
(9 FAIL rows, 6 cases) and `app-test-lab-linux` (75 FAIL rows). Evidence: the jobs' per-case app-log slices and screenshots (job artifacts) and
`test-results/2026-11-10_build_r10a_3048440714/`. Nothing here was compiled or run against a build (laptop: rustfmt, `run.py --self-test`).
Wording: merged-untested work is "worked, unproven".

Result: two app defects (#2026 new, #2016 known and held out of round 10), six test faults (#2027). 69 of the lab's 75 rows are causes that integ/r11
already carries changes for (#2019 #2020 #2021); the lab job ran a build from before them. Three more things were seen and filed without a change:
#2028, #2029, #2030.

## 1. `agent_gate_deny_allow` (agent@fake): APP, #2026

**Rows.** "adding the origin to settings.toml with the app running: the request did not reach the listener", "... the gate refused it", and two
UNVERIFIABLE lines (see 2).

**Evidence.** The steps before it pass for the first time (the round 10 harness change works): denial, keyboard Allow / Remove / add, a live removal.
Then:

```
10:07:14 hop=network_gate_ui action=add_origin origin_hash=db3f053403ad937a     the app writes the file (W)
10:07:16 dispatching action for "root_view:add_file_pane"                        image d reaches the listener
         case: allowed_origins = "" written into the file (W')
10:07:20 network gate: denied "remote-asset" (host hash db3f053403ad937a)        reloaded
         case: the origin written back; the file is W again, byte for byte
10:07:26 network gate: denied "remote-asset" (host hash db3f053403ad937a)        not reloaded
```

**Cause.** `BackgroundWriter::is_recent_write` (`crates/warpui_extras/src/user_preferences/background_writer.rs:233`) says a file equal to the newest
remembered digest is the watcher's echo of our own write; `load_and_apply_unless_own_write` (`toml_backed.rs:374`) then skips the reload and
`handle_warp_config_change` (`app/src/settings/init.rs:571`) returns. Nothing forgets that digest when foreign content was loaded in between. For a
user: change a setting in the app, edit the file, undo the edit in the editor; the app keeps the edit and writes it back at the next change. Behind a
syntax error the undo also leaves the banner up and writes inhibited (`init.rs:571` returns before `reload_settings_after_file_change`).

**Fix.** `forget_recent_after_foreign_content`: when the store finds the file is not its own write, the writer drops the digests it remembers, all
of them, or all but the newest while a write is in flight (its rename is still coming). Called whether or not the foreign file parses.

**Rejected.** Dropping a digest when its echo is observed (the watcher may coalesce or repeat events: an own write would then be reloaded over newer
settings, the bug #1155 fixed). Comparing mtime or inode (an atomic save by an editor and by us look alike). A change in the case (it found the bug).

**Risk.** Low. The only new effect is a reload where there was none. A write in flight keeps its digest, so its echo is still skipped. If the
in-flight write lands after a foreign edit, the disk holds our stale write and memory the foreign edit: as before this change.

## 2. The toast's Allow, same case and `network_gate_notice`: TEST, #2027

`agent_gate_deny_allow` took its one screenshot at the denial log line: `agdn_find_allow` shows no toast (and the viewer's first, zero-width frame,
#2028), while `agdn_denied`, 3 s after an earlier denial, shows "Blocked a request for "remote-asset" to localhost:44535. Allow this origin".
`network_gate_notice` sleeps 4 s before looking; `ngn_denied` shows the toast and `ngn_find_allow`, a moment later, does not (the app log has the
toast stack updating 4 s after the denial). **Fix:** `find_toast_word` looks until the word shows, at most 3 s; `network_gate_notice` does not sleep
before its first look. "pointer Remove: ... not found" in the same row is a consequence of 1 (the page said "No extra origins.").

## 3. `agent_question_empty` (agent@fake, regressed): TEST, #2027

"neither answerable nor ended after 150 s", in a case that took 4.9 s. For a call the app rejects it logs `state=success` (t=...843.189),
`state=in_progress` (...846.704, the automatic follow-up) and `state=success` (...858.534). `run_variant` (`cases_agent_question.py`) stopped
polling at the first and then read the log again for the verdict it judged. **Fix:** `settled_verdict`: two polls 0.5 s apart must agree, and that
value is judged. **Rejected:** teaching `verdict()` about the follow-up (it would have to know app internals). The 3 ms `success` is filed (#2029).

## 4. `subshell_sh_probe_empty_ps1` (both modes, regressed): TEST, #2027

"this dash has an EMPTY PS1 ...: ['[$ ]']". The Linux leg typed `export PS1` in the pane's bash to hand the dash the blanked PS1. #1907 (`952fd184a`,
round 10; `bash_body.sh` `export -n PS1` in `__warp_update_prompt_vars_impl`, which runs at every prompt) un-exports it on purpose. The app was right:
`subshell_ready_judged source=armed decision=Write(None) kind=Cooked leader=Shell comm="dash" tail=ShellPrompt`. **Fix:** the `$ENV` file with `PS1=`
on every platform (macOS has used it since round 9); the second leg's rc gets `PS1=` too and checks its premise. **Rejected:** dropping the case (an
empty PS1 still reaches a dash through an rc file or `env PS1= sh`, and the app path is the same).

## 5. `subshell_ready_secret_after_probe` (both modes): TEST, #2023, not changed here

The log has `hop=subshell_stage_two_pending shell=Bash` before the Secret cancel, with no write between; `hops()` does not list that name.
`fix/r11-mac` (`e9bda59f9`) adds it. Changing the same lines here would only make a merge conflict.

## 6. `subshell_ready_zsh_newuser_install` (both modes): APP, #2016

The prediction made when the #2016 change was held out of round 10 came true, on exactly one check. The menu was shown and left alone
(`decision=Wait(InputPrompt) kind=LineReader ... tail=InputPrompt queue_empty=Some(true)`), no `.zshrc`; after the `q` every look is
`Wait(LineDirty)` at `kind=LineReader tail=ShellPrompt queue_empty=Some(true)` until the next Enter 12 s later. Those are the facts the rule reads.
**Fix:** `a037837d2` is back (revert of `5dd50d49c`), as its own commit. **Risk:** the one named on #2016 (a false "taken" lets the setup's
`^K ^U` kill typed text); its guards are unchanged. If it is to stay out, drop that commit: the case then fails on that one check.

## 7. `ui_vertical_tabs_many` (agent): TEST, #2027

"vt_many_closed: shell received 5 commands ... first mismatch: 'pwd >> .../vt_pwd_end.txt'". `tab_answer` (`cases_wrap_vtabs.py:354`) returns when the
probe's effect file exists; the rc hook logs the command with the next prompt. `effect_run` took `cmd_mark()` in that gap. **Fix:** `tab_answer`
waits for its own line in rec.log. (`fix/r11-mac` adds a `tab_answer` call right before `open_tabs`, which takes a mark too: the same race, closed by
the same change.)

## 8. Lab `lab_enter_start.{bash,zsh}.150.nohelper` (4 rows): TEST, #2027

"app median 140.7 ms vs plain ssh 88.2 ms" (zsh 115.8 vs 84.3). App samples are flat (140-142; zsh 115.3-118.7), which is not what a network wait
looks like. Two faults in the measure:

- **Start.** `t_enter` was taken before `ctx.be.key("Return")`. `WaylandBackend._wtype` puts `-k Shift_L -s 20` in front of every call. App log:
  Shift at t=...514.086, Enter (`core_enter stage=handle_window_event`) at ...538.847, `enter_begin` ...539.288, `pty_write` ...540.539. So about
  35 ms (spawn + prime) per Enter were the injector's, against a 15 ms budget; the plain control stamps right before its CR write.
- **Stop.** rec.log's C line is written by `PROMPT_COMMAND` / `precmd` (`profile.py:29-41, 98-105`): at the next prompt, after the command and the
  session's prompt hooks. Same line: echo back at +151.9 ms, `preexec_received` +18.5 ms later, `precmd_seen` +36.7 ms later.

**Fix.** Start = the app's receipt of the Enter from its own log (`lab.app_enter_epoch`; falls back to the old stamp, with a note, when the log has no
line). Stop for bash and zsh = the command's own `$EPOCHREALTIME` in both sessions (`lab.start_stamp_line`). The budget and `paired_verdict` are
unchanged; the old quantities stay as metrics. **Rejected:** `APPCI_WL_PRIME_MS=0` for this case (the prime exists because the first key of a call is
otherwise lost); raising the budget. **Not dismissed:** the remote bash hooks do cost about 18 ms before a command starts (#2030); with the corrected
measure the bash arm may still be over the budget, and that would then be an app finding.

## 9. Lab `lab_ctrl_c_no_esc_i.bash.150.nohelper` (2 rows): TEST, #2027

"[x2 gap 0 ms] prompt.log grew by 3". Two `ctrl_c_route route=pty long_running=true` 33 ms apart, both written; the first CommandFinished arrived
142 ms after the first key; then two CommandFinished / Precmd pairs. With a 200 ms gap the second key is `route=none` and the count is 2. The app
cannot know the remote prompt is back before the bytes arrive, and plain ssh sends both keys too; a bash at its prompt answers Ctrl-C with a prompt.
**Fix (expectation changed):** `2 <= n <= 2 + (Ctrl-Cs written - 1)`, from `hop=ctrl_c_pty_write`. Exactly 2 still holds wherever one Ctrl-C was
written. **Rejected:** dropping the count; a fixed "+1 at 150 ms".

## 10. The other lab rows

40 `harness_state_reset_after_*.helper` (#2021, no helper binary in the job), 24 fish / nu "no remote prompt" (#2019, #2021),
`lab_agent_remote_flow.bash`, `lab_agent_remote_drop.bash` with its `pane_not_local_after_*` row, `lab_fam_agent_delete_conflict.bash` (#2021),
`lab_fam_agent_edit_endings.bash` (#2020). `lab_enter_no_network_wait` and `lab_agent_remote_errors` fail only in their fish / nu arms.
40 + 24 + 5 + 4 + 2 = 75.

## Seen and filed, not changed

- #2028: the markdown viewer's first frame has no width (`image paint with suspicious size ... element_size=(-20, 208)`).
- #2029: the turn reads `success` for 1-4 ms before the follow-up to a rejected tool call.
- #2030: remote bash prompt hooks, about 18 ms before a command starts and 37 ms to the next prompt.
- A toast that carries the one-click "Allow this origin" is up for about 4 s. Whether that is long enough is the owner's call; the Network page's
  gate log has the same Allow.
