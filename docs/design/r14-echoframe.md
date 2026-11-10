# Round 14: the echo frame of a key typed into a running program is drawn at once on macOS (#2039), and the lone `Input` re-render (#1995)

Status: implemented behind a kill switch, **not built and not measured** (no cargo on the authoring machine). Judged by a same-run
A/B (`latency_echo_ab`, PROVING). Follows `docs/design/1843-1848-latency.md` (#1848) and `docs/design/r12-mackey.md` (#2037).

## 1. Cause (#2039)

A key typed into a running program (`cat`, vim, ssh) does not change the window while it is handled: it is written to the pty. The
change comes back as the program's echo, about 0.4 ms later, on the pty read:

* the read ends in `WindowState::request_redraw` (`crates/warpui/src/platform/mac/window.rs:1238`) ->
  `set_needs_display_async` (`window.rs:1139`) -> `setNeedsDisplayAsyncForRequest:` (`objc/host_view.m:523`): a `dispatch_async` to
  the main queue whose block calls `setNeedsDisplay:YES`;
* AppKit serves `setNeedsDisplay:` on its next display pass. `hop=key_frame_sched` (`frame_latency.rs:251`, logged at
  `window.rs:1463`) is that wait.

#1848 (`drawKeyFrameNowSince:`, `host_view.m:501`) only covers redraw requests made *while the key press is handled*
(`RedrawRequests::begin_key_frame`, `frame_latency.rs:347`: `latest > at_key_start`). The echo's request is made after the press has
ended, so it still waits.

Numbers, round 12 Mac run (`test-results/2026-11-10_build_r12a_3050758880/app/macos`, same shape as round 11 in `r12-mackey.md`):

| measure | key into a running `cat` (`latency_key_log`) | key into the editor (`latency_paint`, #1848 on) |
|---|---|---|
| `hop=key_frame_sched` p50 (n) | 4.36 ms (53), max 9.96 | 0.08 ms (46) |
| key -> frame built, median | 7.2-7.3 ms (round 11) | 1.46 ms (`agent_latency_key`, round 11) |
| `hop=key_presented key_to_presented_ms` p50 | 28.46 ms | 30.36 ms (under ScreenCaptureKit capture) |

Does starting the frame earlier reach the glass earlier? `r12-mackey.md` left that open ("the frame still reaches the screen on the
next refresh either way"). The #1848 A/B of the same round answers it for the same mechanism: `latency_paint_ab`, key -> first changed
pixel (kl3), path on vs off in one run: **-4.61 ms** (terminal mode, 29.56 vs 34.17) and **-2.41 ms** (agent mode). The frames present
with `displaySyncEnabled = NO` (`host_view.m:355-365`), so a frame handed over 4-6 ms earlier is latched one refresh earlier for a
good share of the keys. The echo frame has the same present path; only its start differs.

## 2. Design

**The first frame a window is asked for after a key press has been handled is drawn when the request's posted block runs, instead of
at AppKit's next display pass. Once per key press. Every other frame keeps its pacing.**

### 2.1 Where

The request is made inside an app update (the app is mutably borrowed), so it cannot draw from `request_redraw` itself. The block that
`setNeedsDisplayAsyncForRequest:` already posts runs on the main queue after that update has returned. There, instead of
`setNeedsDisplay:YES`, the view calls `displayLayer:` (the method AppKit would call later, and the one #1848 calls). Nothing new is
posted and no timer exists: the event is the redraw request, carried by the block it always posted.

### 2.2 The rule (event-based, `RedrawRequests` in `frame_latency.rs`, pure and unit-tested)

* `keyDownImpl` end (after the #1848 key frame, if any): `note_key_handled(key time)` stores `(latest request number, key time)` for
  the window. The press's own requests are numbered at or below that mark: they are #1848's business and never an echo.
* A posted block whose request is numbered above the mark is the echo candidate. `begin_echo_frame` takes the mark (so **one attempt
  per key press**) and answers yes only when: the switch is on, the key is at most `ECHO_FRAME_KEY_AGE_MAX_NS` (250 ms) old, the
  request is not already shown, and no key frame is being drawn.
* Any frame that starts with a request above the mark outstanding (`note_frame_started`, in `warp_update_layer`) clears the mark: the
  display pass got there first (a frame was in flight, see below), so the next output frame is not mistaken for the echo.
* If the frame went to the compositor (`note_frame_submitted`, as for #1848) the requests made before it began are covered and their
  blocks skip `setNeedsDisplay:`: one frame per echo, not two. If it did not (the #1838 gate skipped it, the app was borrowed, no
  drawable), nothing is covered and the block falls through to `setNeedsDisplay:` exactly as before. No frame can be lost.

The age bound is a comparison of two timestamps taken at events (key press, block run), not a timer: nothing fires after 250 ms. It
only stops a key that got no echo (a password prompt, a key the program ignored) from making an unrelated frame minutes later the
"echo". 250 ms covers a slow ssh round trip; a larger value would cost nothing because of the one-per-key rule.

The state is per window (unlike the `warpKeyFrameTag` statics), so output in another window cannot spend this window's mark.

### 2.3 What AppKit allows, and what is not done

| option | verdict |
|---|---|
| `setNeedsDisplay:` (today) | served by the display cycle: the 4-6 ms |
| `displayLayer:` called directly from the main-queue block | **taken.** Same call #1848 makes from `keyDown:`; the layer is a `CAMetalLayer` that presents outside the CA transaction, so the draw needs no display pass. |
| `-[NSView displayIfNeeded]` / `-display` after `setNeedsDisplay:` | rejected: goes through AppKit's view display machinery for the whole view tree for the same result, and marks nothing as covered |
| a `CADisplayLink` / `CVDisplayLink` that draws every refresh | rejected: it is the display pass again (a wait for the next tick), and a free-running link costs frames when nothing changes |
| `presentsWithTransaction` for the echo frame | rejected for the same reason as in #1848 (option E): the present stage is shared with Terminal.app |

Not drawn at once (left to the display pass, as today), the guards #1848 has, now shared in `layerForFrameNow`: test mode, no window
state or window, a layer that is not a `CAMetalLayer`, a live resize or a layer presenting with the CA transaction, and **a frame
still in flight** (`warp_frame_gate_unpresented >= 1`).

### 2.4 Frame pacing, vsync, tearing

* Present mode is unchanged. The first `displayLayer:` after a key already presents with display sync off (`warpNextFrameFromKey`,
  #1379); the echo frame is that frame today and stays that frame. No frame that presented synced now presents unsynced.
* No extra frames. The frame drawn at once replaces the display-pass frame for the same requests (they are covered). A request made
  while it draws is not covered and takes the display pass.
* A burst of keys faster than a refresh into a running program could draw one frame per key instead of one per refresh. The in-flight
  guard bounds it: while the previous frame has not been presented, the echo goes to the display pass as today. So at most one frame
  is in flight, as now.

### 2.5 Output floods, busy tabs, hidden tabs (#1979, #1838, #805)

One frame per key press, and never with a frame in flight:

* Enter on `cat bigfile`: the first output frame may be drawn at once; every frame after it has no mark and is paced by the display
  pass and the #1838 gate, unchanged.
* A tab that floods while the user types elsewhere in the window: frames are in flight almost all the time, so the guard sends the
  echo to the display pass; when it does draw at once it is one frame per key press, a frame that was going to be drawn 4-6 ms later.
* A hidden tab whose output asks for a window redraw after a key: that request spends the mark (one frame, earlier than it would have
  been drawn anyway). The frame count does not change, so the hidden-tab and busy-tab frame rates of #1979 do not.
* Cost on the request path: each posted block makes one more call into Rust that reads one `Cell` (`echo_frame_due`); the guards
  and the clock are read only when a mark is armed.

### 2.6 Kill switch and default

`PHOSPHOR_MAC_ECHO_FRAME_NOW=0` (read once, `echo_frame_now_enabled`): `begin_echo_frame` is never true, no request is covered, every
block ends in `setNeedsDisplay:`: the old path exactly. Default **ON**, the 9 Nov pattern (key-frame default on with a same-run A/B).
Independent of `PHOSPHOR_MAC_KEY_FRAME_NOW`.

### 2.7 How the hops show it

* `hop=echo_frame_now drawn=<0|1> ms=<draw time> key_age_ms=<key press -> block run>`: new, one per attempt.
* `hop=key_frame_sched ms=`: for an echo drawn at once it reads the main-queue hop (expected under 0.3 ms) instead of 4-6 ms.
* `hop=key_frame_build`: unchanged content (the same scene is built), earlier `t=`.
* `hop=key_presented key_to_presented_ms`: the on-glass figure from the presented handler; expected lower by a part of a refresh.
* `latency_key_log`: `key_frame_display_wait_ms` falls to about 0, so the median key -> frame approaches `key_to_frame_own_ms`
  (1.5-1.7 ms in rounds 11/12). Its gate (#2037) is on the app's share and the p95, so it needs no change; the baseline's p95 (9.3 ms)
  will read as an improvement.

### 2.8 Same-run A/B: `latency_echo_ab` (PROVING, macOS)

`appci/cases_keyframe_ab.py`, relaunch per block with `PHOSPHOR_MAC_ECHO_FRAME_NOW=0` for the off arm (the variable is read once),
blocks interleaved (`cases_ctrlc_ab.ab_schedule`). Each block starts `cat > /dev/null`, types keys 80-180 ms apart (the
`latency_key_log` method) and, where kl3 is available, also samples key -> first changed pixel into the running `cat`
(`cases.paint_samples`). Per arm: key -> frame p50 / p95, `key_frame_sched` p50, `key_to_presented` p50, `echo_frames_now` /
`_drawn`, kl3 key -> paint p50. Notes and WARNs only (the switch did not reach the app, the path never ran, most attempts did not go
out); it fails on harness problems. The budget verdicts stay with `latency_key_log` and `latency_paint`.

## 3. Rejected

* **Drawing from `request_redraw` synchronously.** The app is borrowed there; `warp_update_layer` would drop the draw.
* **Treating every frame requested within N ms of a key as urgent.** That is a time window deciding pacing for many frames (the 150 ms
  window #1379 removed). The rule here is one frame, armed by the key event.
* **Arming only when the key press wrote to the pty.** The view layer does not know where a key went, and the first frame after an
  editor key (the autosuggestion or completion result arriving a few ms later) gains the same way at the same cost: one frame that
  was going to be drawn anyway, and usually refused by the in-flight guard because the key's own frame is still on its way.
* **A key-aware exception to the #1838 gate.** Same as #1848: drawing with a frame in flight blocks in `nextDrawable`.

## 4. Risk

* macOS only, small: one ObjC method, four FFI entry points, pure state in `RedrawRequests`. Not compiled on Linux, not compiled at
  all yet. The #1848 guards moved into a shared helper (`layerForFrameNow`): a mistake there would change #1848 too.
* A direct `displayLayer:` from a main-queue block is new for output-driven frames (until now only from `keyDown:`). If AppKit has a
  display pass pending for an earlier request, that pass still runs and draws the same scene again: one redundant frame, as #1848
  already has.
* The on-glass gain is inferred from #1848's A/B; the echo path's own A/B has not run. If `latency_echo_ab` shows no gain on glass
  (kl3 and `key_to_presented`), the switch default should go to OFF: that is an owner call on the measured numbers.

## 5. #1995: the cause of the lone `Input` re-render

Named from the committed round 12 logs (`hop=notify_cause` before each `hop=key_frame_build`, terminal mode;
`test-results/2026-11-10_build_r12a_3050758880/app/{macos,wayland}/run_slim/logs/terminal/`):

| log | key frame builds | builds with `Input` notifies before them | causes (builds) |
|---|---|---|---|
| wayland `latency_enter` | 161 | 158 | `handle_completion_suggestions_results_3` 149, `new_10` 75 |
| macOS `latency_enter` | 73 | 32 | `handle_completion_suggestions_results_3` 32, `new_10` 19 |
| wayland `latency_key_views_ab` | 39 | 30 | `handle_completion_suggestions_results_3` 26, `new_10` 12 |
| macOS `latency_key_views_ab` | 25 | 19 | `handle_completion_suggestions_results_3` 16, `new_10` 5 |

With only `handle_completion_suggestions_results_3` before the build, the build re-renders 2 views (wayland `latency_enter`: 74 of 74
such builds have `views=2`); with `new_10` as well, 3-4. Builds with no notify before them have `views=1` (`EditorView`).

**Cause:** `Input::handle_completion_suggestions_results` (`app/src/terminal/input.rs:13641`) ends in an unconditional `ctx.notify()`
(`input.rs:13803`). With completions-as-you-type on, every typed character whose completion result comes back for the current buffer
runs it. When the result does not change the menu (closed stays closed: `InputSuggestionsModeModel::set_mode` returns early on an
equal mode, `suggestions_mode_model.rs:32`), nothing else is notified and `Input` is re-rendered alone with the editor: the 16-31 %
set of the issue. When the mode does change, the model's `ModeChanged` subscription notifies `Input` anyway (`new_10`,
`input.rs:3732`) along with the menu views. The share follows how often a completion result lands before the next key (higher on the
slower Linux VM).

**Not changed here.** `input.rs` belongs to another agent's area this round, and the change wants its own check: the trailing notify
is redundant when the mode changed (`new_10` covers it) and when nothing changed, but `completion_menu_stale` is cleared just above
it and `set_prepared_tab_completions` runs in the open branch, so the proposed change is "notify only in the branches that return
early today or changed something `render` reads", with a unit test that a no-change result does not notify. Recorded on the issue.

**Harness:** `latency_key_log` now records the `render_hops` metrics (among them `input_notify_causes` / `input_notify_frames`) over
its whole typing window, which includes the command typed into the editor, and notes the causes.
