# Round 12, Mac: where the 3.6-3.75 ms of Enter -> command start goes (#2031)

Run: round 10 Mac app run, pipeline 3048440714 (`test-results/2026-11-10_build_r10a_3048440714/app/macos`), with r18w9 (3047937983)
and r18w8 (3047198675) as cross-checks. Case `latency_enter`, interleaved A/B with Terminal.app, n=60 kept per side.
Issues: #2031 (this split), #2032 (agent-mode effect flush), #2035 (archive lost the hop lines).

Nothing was built or run on a Mac for this note. The Mac's run directory had already been cleared by the next job, and the archive
kept 2 of 52555 hop lines of the case (#2035), so every Mac number below is a median from `results.json`. Numbers marked "replica"
come from zsh 5.9 on the Linux laptop (a real interactive zsh on a pty); they size the shell-side parts, they do not prove the Mac.

## Verdict first

- The case does not judge this number on macOS: Enter overhead is REPORT-ONLY since #1606 was closed (owner 2026-11-05,
  `cases.lat_checks(report_only_macos=True)`). The `FAIL` in `summary.md` is the raw budget column of the latency table. The case
  status `noref` comes from the injector accounting note (17 key-downs not dispatched), which is another matter.
- The overhead itself is APP, not TEST: 2.1 ms of it is spent by the app's zsh integration after the line is accepted, and about
  1.1-1.8 ms before that, in the app's key path and in the shell ingesting the line the app writes at Enter.
- No app code is changed in this round. Reasons are per candidate below; in short, the parts that can be sized here are either
  too small to be worth a change in the hook every command runs through, or change what the user's shell does with its history
  (an owner decision), and the largest part cannot be attributed without one more Mac run. The harness now takes the stamps that
  attribute it.

## The split (terminal mode, r10a, medians in ms)

| hop | Phosphor | Terminal.app | delta | source |
|---|---|---|---|---|
| injector post -> OS event timestamp | 45.69 | 46.25 | -0.56 | `enter_os_split_ms.post_to_evts_ms` |
| OS event -> app dequeues it | 0.89 | | | `app_segments_ms.event_age_ms` (`hop=mac_enter stage=app_send_event age_ms=`) |
| dequeue -> `hop=enter_begin` | 0.75 | | | `event_to_enter_ms` |
| `enter_begin` -> `pty_write` | 0.77 | | | `enter_to_write_ms` (0.55 try_execute, 0.02 `submit_bookkeeping`, 0.20 `effect_flush`) |
| `pty_write` -> zle-line-finish | ~1.36 | | | `evts_to_lf` minus the three rows above |
| OS event -> zle-line-finish | 3.77 | 2.00 | **+1.77** | `enter_os_split_ms.evts_to_lf_ms` |
| zle-line-finish -> rc preexec hook | 2.34 | 0.22 | **+2.12** | `enter_split_ms.lf_to_px_ms` |
| rc preexec hook -> stamp | 0.04 | 0.05 | -0.01 | `px_to_stamp_ms` |

On the wall clock: `post_to_lf` +1.14, `lf_to_px` +2.12, `post_to_stamp` +3.49; the headline 3.75 is injector spawn -> stamp
(`inj_to_post` -0.01). App log: `pty_write` -> first `pty_read` 1.80, `pty_write` -> `preexec_received` 3.36, `pty_lock_wait` 0.00.

Agent mode (3.58 ms): the same shape; `enter_to_write` 1.43 (of it `effect_flush` 0.88), `lf_to_px` 1.84 vs 0.21.
r18w9: 3.18 / 3.95 ms, `lf_to_px` 2.20 / 1.90 vs 0.21. r18w8: 4.24 / 3.11, `lf_to_px` 1.74 / 1.71 vs 0.21.

Harness asymmetry found in #1606 (the guard's `ps` spawn, `t0_to_inj` +5.16) is outside the sample since the rediag.

## After the line is accepted: +2.12 ms

Between zle-line-finish and the rc's preexec stamp a Phosphor pane does three things Terminal.app's zsh does not:

1. `_warp_zshaddhistory` (`app/assets/bundled/bootstrap/zsh_body.sh:2013`), which runs `_warp_repair_hooks` (`:2160`).
2. `setopt share_history` (`zsh_body.sh:2318`, "Set history to flush after every command", inherited from the pin). With it zsh, in
   `hend()` and so before preexec, locks the history file with a symlink (`$HISTFILE.LOCK`), reads what other shells added, appends the
   line and unlinks the lock (seen with strace on zsh 5.9: `symlink`, `openat(O_APPEND)`, `unlink`).
3. `warp_preexec` (`zsh_body.sh:306`): JSON-escape the command, hex-encode the message, write the DCS to `/dev/tty`.

Replica, n=80-120 per arm, plain zsh `lf_to_px` 0.22-0.24 ms (the Mac reference: 0.22, so the two machines run plain zsh alike):

| arm | lf -> px | over plain |
|---|---|---|
| plain | 0.22 | |
| hooks, no share_history | 0.72-0.74 | +0.50 |
| share_history, no hooks | 0.53 | +0.28 |
| both | 0.93 | +0.69 |

Inside the hooks (the 95-character stamp line, in the live session): hex encode 0.17-0.18, `_warp_zshaddhistory` 0.075, JSON escape
0.04-0.05, the write to `/dev/tty` 0.03; an empty `warp_preexec` still leaves 0.15 (function calls, `setopt localoptions`).

So Linux reproduces about 0.7 of the Mac's 2.12 ms. The other ~1.4 ms is macOS-specific and NOT attributed. Suspects, in order:

- `share_history` on APFS: one symlink, one append, one unlink per command, in the home directory. 0.28 ms on btrfs.
- zsh ends every line edit with `tcsetattr(TCSADRAIN)`, which on macOS returns only when the terminal has read everything the shell
  wrote. Phosphor's first read is logged ~0.44 ms after zle-line-finish (1.80 after the write, the line being accepted ~1.36 after
  it). The reader thread reads before it takes the model lock (`app/src/terminal/local_tty/event_loop.rs:1265`) and
  `pty_lock_wait` is 0, so this would be thread wake-up time, not a lock.
- the same hooks simply costing more under macOS zsh.

### What was considered and not done

- **Trim the hooks.** The largest piece, hex, is already one `printf -v` (#1606). Caching the hex of the message's constant prefix
  saves about 0.03 ms of 3.75. Rejected: 1 % of the gap, in the hook every command runs through.
- **Drop or replace `share_history`** (plain history, `inc_append_history_time`, or `hist_fcntl_lock` to avoid the symlink).
  Each changes what the user's shell does: whether a running command is on disk yet, whether other sessions' lines are imported,
  the lock other zsh sessions honour, the file's timestamp format. Not decided here: an owner question, and its Mac cost is not
  measured yet (the new `ah_to_p0` number is exactly that cost).
- **Raise the pty reader's thread priority on macOS.** Only if `lf_to_ah` shows a drain wait. No number yet, nothing changed.

## Before the line is accepted: +1.14 (wall) to +1.77 (OS clock)

Phosphor's 3.77 ms from OS event to zle-line-finish is 0.89 (the event waiting to be dequeued) + 0.75 (AppKit routing, the input
context, keystroke match) + 0.77 (submit) + ~1.36 (pty transit and zle ingesting the line). Terminal.app does all of its part in 2.00.

- **Ingest is by design and depends on the line.** Phosphor's editor owns the line and writes it at Enter; Terminal.app's zle already
  has it. Replica: zle takes 0.36 ms from a bracketed 26-character burst to line-finish, 0.49 ms for 95 characters (unbracketed:
  0.38 and 0.75). The stamp line is 95 characters, so the case shows roughly the long-command end of this cost. This is the one
  TEST-shaped part, about 0.1-0.15 ms; it is real behaviour and is left as it is.
- **0.75 ms dequeue -> enter_begin** was one number. The app already logs its stages (`hop=mac_enter stage=perform_key_equivalent`,
  `key_down_impl_entry`, `after_interpret_key_events`: `crates/warpui/src/platform/mac/objc/window.m:568`, `host_view.m:426,440`);
  the harness now reports them.
- **Agent mode pays 0.68 ms more before the write** (`effect_flush` 0.88 vs 0.20; Wayland 1.01 vs 0.21). The command reaches
  `PtyController::write_command` through three queued events (`input.rs` `try_execute_command` -> `view.rs` `handle_input_event` ->
  the pty controller), and the AI input-model updates the submit makes after `hop=try_execute_done` are delivered in between.
  Filed as #2032. Not changed: it is an ordering change in the Enter path, in a file another agent owns this round, with no build
  to measure it on.
- Debug logging is on during the case (about 20 `input_order` lines per Enter on the main thread). Its cost is not known; no control.

## What changed (harness only)

- `profile.ZSHRC`: two more shell times, `__pa_ah` (first `zshaddhistory` function) and `__pa_p0` (a plain `preexec()`, which zsh runs
  before `warp_preexec`; re-defined from `precmd` because the bootstrap moves a `preexec()` it finds, #974). They ride in `$__pa_px`,
  so the typed stamp line is unchanged byte for byte. `enter_split_ms` gains `lf_to_ah` (line editor teardown, tty drain), `ah_to_p0`
  (history: `share_history`) and `p0_to_px` (`warp_preexec`). Checked in a real zsh with a stand-in bootstrap, in both orders.
- `cases.enter_segment_samples`: `event_to_enter` split into `appkit_route`, `key_equivalent`, `interpret_keys`, `dispatch_to_enter`.
- `script/ci/lat_evidence.py`: the platform's evidence budget is shared between its logs (#2035).

Risk: the rc's `preexec()` runs before `warp_preexec` in every pane of every case. It assigns one variable and prints nothing; the
reference terminal's rc is the same file.

## What the next Mac run should show

`enter_split_ms.delta` with `lf_to_ah_ms`, `ah_to_p0_ms`, `p0_to_px_ms` summing to `lf_to_px_ms` (~2.1):

- `ah_to_p0` large (over ~0.6): `share_history` on APFS is the cost. Owner question: keep it, or move the write off the Enter path.
- `lf_to_ah` large: the tty drain. Look at the reader thread's wake-up on macOS.
- `p0_to_px` large (over ~0.6): the hooks cost more on macOS zsh than on Linux; profile them there.

`app_segments_ms` with the four stage keys summing to `event_to_enter_ms` (~0.75), and `logs/<mode>/case_latency_enter.lat.txt`
holding hundreds of hop lines instead of two.
