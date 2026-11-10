# Round 10, group A: #1966 #1985 #2004 #1907 #1938

Branch `fix/r10-groupa`, base integ/r10 `cfe7249e8`. Nothing here was compiled or run (laptop: rustfmt, `run.py --self-test`
and `script/check_input_path_timers` only). Wording: merged-untested work is "worked, unproven".

## 1. #1966 Windows Git Bash agent question cards read as hung

**Evidence.** `cases_agent_question.verdict` (`script/realapp/appci/cases_agent_question.py:34-70`) calls `AR.action_hops`, i.e. it reads
`hop=agent_action` lines. They come from `app/src/ai/blocklist/action_model.rs:761` (`log::info!`, module
`warp::ai::blocklist::action_model`). Linux and macOS launch with no base level (`app.py` INNER script, `warp::input_order=debug,...`), and
env_logger's default with no bare level shows only what a directive names, but the app's own default applies there; on Windows
`WindowsApp.env` sets `RUST_LOG="warn,winit=info,...`, a `warn` base that hides every `info` line from a module with no directive of its own.
Only `agent_providers` and `blocklist::controller` had one, so a card that WAS shown reads as `hung` ("no hop=agent_action").

Other hidden modules, found by scanning the Rust sources for plain `log::info!/debug!` calls whose string carries a `hop=` name some appci case
reads (13 hops in 13 modules): `cli_controller` (`handover`), `queued_query` (`queued_row`), `diff_state_remote`, `git_status_update_remote`,
`github_repo_model::remote` (`diff_state`, `git_chip`, `gh_chip`), `pushed_listing` (`cwd_listing`, `path_detect`), `remote_index_driver`
(`remote_index_sync`, `remote_index_embed`), `network_page` (`network_gate_ui`), `crates/remote_server/src/manager.rs` (`agent_context`,
`file_tree`) and `router.rs` (`backend_route`, logged through `route_log_line`). Every `input_order` hop (the other ~80 names) goes through
`warp::input_order=debug`, already enabled.

**Change.** `app.WINDOWS_RUST_LOG` (one constant the launcher uses) now has `=info` directives for all of those modules. `app.rust_log_enabled`
implements the env_logger prefix rule. Self-test `windows_rust_log_enables_every_hop_module_1966` rescans the sources and fails if any such module
is hidden (so a new plain-log hop forces a new directive), and proves the matcher can detect the original defect (the pre-fix string hides
`action_model`).

**Rejected.** Dropping the `warn` base on Windows (it exists because Windows runs were flooded; r18d2 comment in `app.py`). Making the app log
`hop=` lines at `warn` (changes product log levels for a harness problem). Moving the hop to the `input_order` target (app change, and the
module-enabled check is the more general guard).

**Risk.** Log volume on Windows grows by the info lines of 13 modules: low rate (state changes). A module path derived from the file path is wrong
for a `#[path]` or re-exported module; the scan would then pass a hidden module. None of the 13 uses either.

## 2. #1985 flood_scrollback_find false-FAILs when OCR is blind

**Evidence.** `cases_robust.py` `c_flood_find`: the final `r.check(seen and scrolled, ...)` needed OCR to read the marker. `seen` is False only
when OCR read the marker nowhere, not even in the Find field it was typed into (the r18u28 case: "OCR read the marker nowhere"), i.e. OCR is blind. The
case already parses the app's own `hop=find_scroll result=scroll ... moved=... viewport_lines= match_line=` (`find_scroll_geometry`) and notes "the app
placed the match ON screen", but only as a note: the verdict stayed a FAIL.

**Change.** Pure `find_verdict(seen, scrolled, hops, geo)`. OCR read the marker: unchanged (pass iff the view left the flood's tail). OCR blind:
geometry ON screen and `moved=true` -> pass, noted as resting on the app log; geometry OFF screen -> fail; only `result=no_match` hops -> fail (the app
found nothing); no hop, no/`busy` geometry, or ON screen with `moved=false` (self-contradicting: the marker was off screen before) -> `r.unverifiable`
(NOREF, never a pass). The agent-mode Skip for an all-`no_match` log still comes first. Selftest rows cover each of those; the old structural assertion
now orders the Skip before `find_verdict(`.

**Rejected.** Treating ON-screen geometry as a pass when OCR DID read the marker but the tail is still visible (that is the r18u19 false pass; stays a
fail). Failing an OCR-seen run whose geometry says OFF screen (a stricter check than asked; the Find-field hit is weak evidence either way, so left
for the owner). Removing the "no OCR engine" Skip at the top: with no engine the control (marker not on screen before the Find) cannot be checked.

**Risk.** A wrong `match_line` in the app (the rendered vs scroll mode disagreement the case already notes) would now pass a blind-OCR run; the
note says so and the mode mismatch note is kept.

## 3. #2004 big_paste checks bytes, #1191 is about time

**Evidence.** `cases.py` `c_big`: pastes ~1 MB into `cat > big.out`, waited for the file size to be stable (1.5 s polls), THEN sent Return, so a
frozen UI or a slow Enter was absorbed by the wait; only `got == want` was a check. `ui_item_metrics` records `hop=ui_item` as a metric, "never a
check". #1191: a 1 MB paste froze the UI 6-7 s and Enter took 1.4-5 s.

**Deviation from the brief, on purpose.** The brief says to time the paste end -> the command's rec.log timestamp. Here the paste goes to a running
`cat`, so rec.log (`profile.commands()`, the rc hook's `$EPOCHREALTIME`) stamps `cat > big.out` at the NEXT prompt, after Ctrl-D: that is the end of the
command, not the Enter. The Enter's own effect is observable instead: the paste ends without a newline, the tty holds that last line until Return, so
`big.out` reaches its full size (data + 1 byte) exactly when the Enter has taken effect. The case now sends Return at once behind the paste (as a user
does; `paste + instant Enter` already relies on that ordering) and polls the file every 20 ms (`poll_file_size`): time-to-Enter = that moment minus the
end of `ctx.be.paste()`. If the file never gets there (the ConPTY shortfall) it falls back to the old stable-size wait and nothing is timed.

**Budgets (PROPOSED, owner to confirm).** time-to-Enter <= 1500 ms (`APPCI_BIGPASTE_ENTER_MS`); largest single `hop=ui_item` <= 250 ms
(`APPCI_BIGPASTE_UI_ITEM_MS`; the 4 ms owner budget stays a metric, 250 ms is the "UI froze" line). Enter is judged only on a byte-exact paste; the
UI item is judged whenever an app log exists. The byte check, the Windows control and both session modes are untouched. Pure judging
`judge_big_paste_time`, selftest `big_paste_time_budgets_2004`.

**Rejected.** Waiting for stable size before Return (cannot see #1191). Timing via rec.log (wrong event, above). Scaling the budgets with
`APPCI_BIG_MB` (set the env vars for a bigger paste).

**Risk.** Return now races the paste's tail on slow platforms: a real ordering bug shows as a byte mismatch (correct), but a platform that merely
drains slowly could exceed 1500 ms on a first run; the numbers are proposals for that reason.

## 4. #1907 child shells inherit an empty PS1

**Evidence (reproduced).** `bash_body.sh` `__warp_update_prompt_vars_impl` ends with `PS1=""` whenever `WARP_HONOR_PS1 != 1` (the app exports
`WARP_HONOR_PS1=0`, `local_tty/unix.rs:481-483`). PS1 is not exported by default, so only a user who `export`s it or runs `set -a` leaks the
blank. `script/test_child_ps1` (new, real bootstrap in a pty, a child's `printenv PS1`) showed `empty` for both before any change, `unset` after.

**Options.**
- (B) Restore the user's PS1 in preexec for the command's lifetime and re-blank at precmd. Closest to a plain terminal (the child gets the user's own
  prompt) but REJECTED: the generator-command precmd (`_WARP_GENERATOR_COMMAND`, ~line 650) returns before `update_prompt_vars`, so a preexec-restored PS1
  would stay visible after an in-band command; preexec is the performance-sensitive path (fork-free budget); and the coupling to every early return in
  precmd cannot be proven by reading.
- (A) CHOSEN: when blanking, un-export PS1 if it was exported (`compgen -A export -X '!PS1'` is a builtin whose status says so), remember it in
  `__warp_ps1_was_exported`, and have `warp_change_prompt_modes_to_ps1` export it again. A child then finds no PS1 (dash/ash/busybox use their own default
  `$ `), not an empty one. Nothing runs in preexec; the blank path already ran once per prompt.

**Not achieved.** The child does not get the user's own PS1 (a plain terminal would). That needs (B). A script that tests `[ -z "$PS1" ]` sees the same
answer as before; only `${PS1+x}` distinguishes unset from empty.

**Risk.** bash 3.2 (macOS /bin/bash): `compgen -A export -X` and `export -n` exist there (not run here; only bash 5.3 was available). zsh does not blank PS1 (it wraps
`PROMPT`), so it is unaffected. Remote bash sessions use the same body, so they get the same fix.

## 5. #1938 comment only

`view.rs` `clear_line_editor_and_write_to_pty` now carries the D7 comment (accepted loss, pointer to `idle_reader_untouched`). No code change.
