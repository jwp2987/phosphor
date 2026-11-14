# #2211: remote native Tab in one round trip

Status: implemented on `fix/2211-one-rtt-tab`; Rust tests written, NOT compiled or run here (no cargo on the
laptop); `script/test_native_completions` run locally against real bash, zsh and fish.

## Problem

The remote native Tab (#1801, #2196: bash, zsh with compsys, fish; nu keeps the app completer, #2210) took two
round trips: `^Y` out, the shell's prompt OSC (`9280;P;<token>`) back, the answer frame out, the answer back. On a
150 ms link that is 300 ms before the first candidate.

## Owner answers (2026-11-14)

- Send the line with the Tab request in one write.
- Target: Tab answer within 1 RTT + 100 ms.
- Ship enabled, with a kill switch `PHOSPHOR_REMOTE_NATIVE_COMPLETIONS_V2=0` that forces the old protocol.
- nu is out (#2210 closed; nu keeps the app completer). zsh without compsys stays on #2225 (the Precmd
  `compsys_loaded=false` report makes the app skip the in-band request and use the completer).

## Design: the pipelined frame (v2)

1. **Advertise.** bash (`bash_body.sh`, next to the v1 option), zsh (`zsh_body.sh`) and fish (`fish.sh`) report
   `!phosphor_nc_frames_v2` in their shell options in addition to `!phosphor_nc_frames_v1`
   (`NATIVE_COMPLETIONS_PIPELINED_OPTION`). `Shell::supports_pipelined_native_completions()` is true only when
   the shell supports the exchange at all (v1 rules, bash version, kill switch of #1969) AND advertised v2 AND
   `PHOSPHOR_REMOTE_NATIVE_COMPLETIONS_V2` is not `0`. An old remote bootstrap (v1 only) gets the old protocol.
2. **App.** `NativeCompletionsRequestOptions::pipelined` selects the request. The pipelined request is ONE write:
   `^Y` + `RS <nonce> : <%HH of the line> US`. The nonce is app-generated, `n` + per-pane counter (hex) + 32
   random bits (hex), so it matches `[A-Za-z0-9]+`, is at most 32 characters and is new for every exchange. The
   exchange goes straight to `AwaitingResults` (no `AwaitingPrompt`). The 15 s results net is unchanged. The 2 s
   `NATIVE_COMPLETIONS_PROMPT_TIMEOUT` becomes an ACK watchdog on the shell's prompt OSC: with no ack the app gives
   up (closes the requester's channel, the completer answers, a toast for a non-remote-first request as before) and
   WRITES NOTHING, owes nothing: the frame is already in the tty. The Enter-after-Tab hold and later bytes behind
   the frame are unchanged (nothing is ever queued behind the exchange; later writes go straight out behind the
   frame and are NOT cancelled).
3. **Hooks.** bash and zsh run `stty -echo` on the tty first (the shell's tty is cooked while a `bind -x`
   function runs, so a frame byte arriving then would be echoed); all three print the prompt OSC (now only an
   ack) and read the frame already in the tty with the same by-content reader as v1. The frame reader accepts
   the app's nonce shape (`n` + letters and digits) next to the shell's own `<digits>x<digits>x<digits>` token and
   the cancel `-`. Tty order is the guard: a hook consumes exactly one frame, and a stale frame is consumed by the
   RS discard widget before the next `^Y` is read. The hook echoes the frame's token in the answer's start mark,
   `9280;A;incrementally_typed;<nonce>` (fish: `__warp_nc_answer line nonce`), and the app drops an answer whose
   start mark names another exchange (`Event::CompletionsStarted`, `ModelEvent::CompletionsStarted`,
   `Handler::completions_started_nonce`; the nonce rides ahead of the answer's end mark).
4. **Frame at a prompt.** A frame that finds the shell at its prompt (the hook is not running) is dropped by the
   RS widget (`warp_native_discard` / `__warp_native_discard`) exactly as in v1. A frame followed by typed bytes
   leaves those bytes in the line.
5. **No cancel in v2** between `^Y` and the answer. The cancel frame stays for the v1 path (a refused or given-up
   `^Y`).

## Tab timing

v1: 2 round trips + hook work. v2: 1 round trip + hook work (a `stty` fork on bash/zsh, the candidate
generation). Target: Tab answer within 1 RTT + 100 ms.

## Tests

- `crates/warp_terminal/src/shell/mod_tests.rs`:
  `pipelined_native_completions_need_the_v2_option_and_the_kill_switch_off`.
- `app/src/terminal/writeable_pty/pty_controller_tests.rs` (the exchange's own harness):
  `a_pipelined_request_is_one_write_and_awaits_results_at_once`, `a_write_after_the_pipelined_frame_is_not_cancelled`,
  `a_pipelined_ack_writes_nothing_and_keeps_the_exchange`, `a_missed_pipelined_ack_gives_up_without_a_write`,
  `a_stale_nonce_answer_is_dropped`, `a_non_pipelined_request_is_the_old_protocol`.
- `script/test_native_completions` (real bash, zsh, fish in a pty): v2 advertised; a pipelined frame is answered with
  the nonce echoed in the start mark and no echo of the frame on the terminal; the tty echo is not left off; a
  command behind the pipelined frame runs; a pipelined frame at a prompt is dropped and the command behind it runs.
- Lab expectations (`lab_tab_remote_dir.{bash,zshc,fish}.150.nohelper` and
  `lab_tab_no_helper.{bash,zshc,fish}.{lan,150}.nohelper`): the app log has "N candidates from the remote shell"
  within 1 RTT + 100 ms of the Tab (the `tab_native_ms` metric), the completed `cd` is what the shell received,
  and no stray control characters reach the screen. zsh (no compinit) keeps the empty answer; nu is the completer.
  Not run here (no lab access from this worktree).

## Kill switch

`PHOSPHOR_REMOTE_NATIVE_COMPLETIONS_V2=0` forces the old two-step protocol (read once per process). The older
switches (`PHOSPHOR_REMOTE_NATIVE_COMPLETIONS=0`, `PHOSPHOR_BASH3_NATIVE_COMPLETIONS=0`) still turn the whole
in-band path (or bash below 4.0) off.
