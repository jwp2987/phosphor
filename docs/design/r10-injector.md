# Round 10: "N of M injected keys never reached the app's key trace"

Verdict: **TEST-ACCOUNTING.** The count was wrong; no key loss is shown by any of the rows. Nothing in the app was changed.
Status: worked, unproven (self-test only; no build, no real run).

## The pattern

Round 9 macOS (pipeline 3047937983, job 17884561117) and the committed rounds 7/8 make most rows `UNVERIFIABLE` with
"injector accounting: N of M injected key(s) never reached the app's key trace".

1. **The same counts on two unrelated injectors.** Round 8 (`test-results/2026-11-09_build_r18w8_3047198675/app/*/results.json`),
   lost-before-app, macOS (CGEvent, htype2) vs Wayland (wtype virtual keyboard):

   | case (mode) | macOS | Wayland |
   |---|---|---|
   | cadence (terminal, agent) | 4, 4 | 4, 4 |
   | fast_typing (terminal) | 2 | 2 |
   | tab_enter (terminal, agent) | 14, 14 | 14, 14 |
   | ctrl_c_d (terminal, agent) | 92, 85 | 92, 85 |
   | ctrl_c_no_esc_i_local (terminal) | 20 | 20 |
   | agent_mode_switch (terminal) | 18 | 18 |
   | routing_table (terminal) | 69 | 70 |
   | latency_key_log (terminal) | 6 | 6 |
   | agent_stream_cancel, agent_input_exact | 6, 6 | 6, 6 |

   A compositor, focus or injector loss does not repeat to the key on a different OS with a different injector. A deterministic
   count does.
2. **Every SKIP row loses exactly 2 of 2** ("injector sent 2; app saw 0", Windows too): the only keys a skipped case sends are
   `Ctx.reset()`'s Ctrl-C and Ctrl-U (`script/realapp/appci/cases.py:1466-1475`).
3. **The counts are the bound keys.** tab_enter: 12 Tab + the reset's 2 = 14. paste_instant_enter on macOS: 6 Cmd-V + 2 = 8
   (Wayland, where the 6 paste chords were already taken out of `sent`: 2). fast_typing: 2. routing_table: one Ctrl-C / reset per row.
4. **All Enters arrive and every effect is exact** in those rows (coordinator's lead, round 9: routing_table 28/28,
   agent_mode_switch 4/4, ctrl_c_no_esc_i_local 18/18).
5. **macOS's own tap saw every key.** `run_slim/logs/enter_route.txt`, rounds 7 and 8: "ALL KEYS: injector sent 124897; macOS
   delivered 92818 to Phosphor, 34204 to other processes; never in macOS (sent - delivered) 0" and "LOST with neighbours logged: 0".

Modifier presses are not the cause: the senders never count them (`backend.parse_injector`, htype2 posts flags, not modifier keys)
and the reader skips modifier keydowns (`injacct.app_keys`).

## Cause

`input_order keydown key=... seq=N` is written in ONE place, `App::handle_non_keybound_event`
(`crates/warpui_core/src/core/app.rs:4985-4999`). `handle_window_event` (`app.rs:4441-4470`) offers a key-down to the keybinding
matcher first and only calls `handle_non_keybound_event` when no binding handled it. A key a binding takes logs
`input_order hop=keydown_keybound dispatch_seq=... key=...` instead (`app.rs:2517-2534`, #1127, and `2549-2563` for the first key
of a pending chord), and a bound Enter logs `hop=core_enter stage=dispatch_result handled=true` (`app.rs:2539-2547`).

`injacct.py` read only `keydown` / `typed_characters` plus the input editor's `hop=enter_begin deferred=false`. So every bound
non-Enter key (Ctrl-C `terminal/view/init.rs:394`, Ctrl-U `editor/view/mod.rs:614`, Tab `terminal/input.rs:2645`, Escape, Ctrl-D,
paste chords) was "lost", and an Enter taken by any binding other than the input editor's was a missing Enter (Wayland round 8:
agent_gate_deny_allow "Enter 13/2").

macOS has three more differences, all in the count:

- A Cmd chord that is a main-menu key equivalent (Cmd-V) is taken by AppKit after `-[NSApplication sendEvent:]` and never reaches
  the view (`objc/window.m:542-596`: `triggersCustomAction` skips `keyDownImpl`), so it has no Rust key line at all. mode_proof
  "1 of 1" and the 6 pastes are this.
- `MacBackend` records no `pastes`, so the Cmd-V chords stayed in `sent`.
- Returns posted with the `k:` op (`keys(["Return"])`) are logged by htype2 as `KEY36`, not `ENTER`
  (`mac/tools/htype2.swift` `case "k"`), so "Enter: sent 0, app saw 12": the Enter comparison could not see a lost one.

## Fix (harness only)

`script/realapp/appci/injacct.py`:

- One physical key the app dispatched = exactly one of `keydown` (unbound), `keydown_keybound` (bound / pending chord),
  `core_enter dispatch_result handled=true` (bound Enter). The three are disjoint by construction in `handle_window_event`.
  A trace without any `core_enter` line keeps the old `enter_begin` count.
- macOS: `hop=os_key_event` (`objc/app.m:503-522`, one per key-down NSApplication dequeues, before window/menu/IME) is "reached
  the app": lost-before-app = sent - os_key_event, Enter = `enter=true`. The dispatch lines give a second figure, key-downs that
  reached NSApplication and were never dispatched. That is allowed only up to the Cmd chords the injector posted in the case
  (`MacBackend.cmd_chords`); past it the row is `UNVERIFIABLE` with its own sentence ("stopped INSIDE the app").
- A paste chord is one key on both sides. `KEY36`/`KEY76` count as Enters sent.
- The row says how the app's count splits (unbound / bound / Enter by a binding; macOS reached / dispatched).

Not blind: the self-test `r10_accounting_counts_bound_keys_and_still_fails_a_really_lost_key` takes each single key's line away
from a clean trace (typed key, bound chord, Tab, editor Enter, pty Enter; macOS: any key-down) and requires lost 1 and `noref`
each time; a key-down that reached NSApplication and was not dispatched is `noref` too; the loss cases keep their own verdict.
`LOSS_CASES`, `loss_verdict`'s rule and `key_loss_control` / `enter_loss_control` are unchanged.

One expectation changed in an existing self-test (`injector_accounting_parse_and_join`): a paste-only call was "n/a"; it is now a
key that must have its `keydown_keybound` line (lost 1 without it). Stricter, not looser.

## Rejected

- **Exempt the reset's keys / known chords on the sender side.** Hides a really lost Ctrl-C or Tab.
- **Move the `keydown` line before the matcher in the app.** Changes a line many cases parse (`key_event_audit`, latency splits);
  the existing hops already carry the fact.
- **A new Objective-C hop for "key-down not handed to the view".** Would make the Cmd-chord allowance exact, but it cannot be
  compiled here and the allowance only matters in cases that post Cmd chords. Left as a follow-up if a macOS row ever trips it.
- **Use `os_key_event` alone on macOS.** Would no longer flag a key the app received and dropped before dispatch.

## Risk

- Not run on a build. If a platform's bound keys do not log `keydown_keybound` as read here, rows stay `noref` (the safe side).
- macOS `undispatched` allowance: a case that posts Cmd chords which ARE dispatched (bound, not menu) leaves that many unused
  allowances that could cover the same number of undelivered plain keys in that case. `from_native` returning `None`
  (`mac/event.rs:76-84`: a key-down with no characters) is the known way a key-down can stop before dispatch.
- `keydown_keybound` and `core_enter dispatch_result` are formatted inline in `app.rs` with no Rust test pinning their shape. A
  wording change would turn rows `noref` (loud), not hide a loss.

## What the next run must show

Every row, all three platforms: `lost-before-app 0`, Enter sent = seen, and the split naming the bound keys (a SKIP row:
"injector sent 2; app saw 2; ... 0 unbound keydown/typed, 2 taken by a key binding"). tab_enter: 14 bound of which 12 Tab.
macOS paste_instant_enter: "6 not (6 Cmd chord(s) injected)". Any row that still loses a key after this is a real signal and must
be read with its app-log slice: `os_key_event` missing = before the app; present with no dispatch line = inside the app.

## Windows giveup_keys_reach_shell (round 9, coordinator's question)

"2/20 lines exact (got 19 lines) ... line 3 is t253 where t252 was typed ... no hold gave up". **Cannot be decided from the job
log**: the row is cut before its accounting note and the job has no artifacts while it runs. What the log does show: "enter_route:
20 Enter(s) reached the input editor: Shell=20" for a case that posts 40 Returns (20 line Enters, 20 bare ones), so that line
does not say which 20. To decide, from the finished job's `results.json` and the case's app-log slice:

1. `metrics.injector`: sent vs seen, Enter 40 vs seen. Under the OLD count expect "lost 2" (the reset) and nothing else; a larger
   number or a short Enter = before the app (SendInput / focus).
2. The slice, third `hop=enter_begin`: `buffer=<N chars>` with N = the length of the t252 line means the app held the whole line
   and its Enter. Then follow it: a `pty_write_bytes` carrying it with a CR and no command in PowerShell = lost between the pty
   write and the shell (ConPTY / PSReadLine); no pty write = the app dropped a complete line, an APP bug and a release blocker.
3. `buffer=<0 chars>` for t252's Enter and its characters in `keydown`/`typed_characters` lines = the app routed the text
   somewhere else (also APP).
