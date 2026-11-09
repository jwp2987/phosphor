# #1838: Ctrl-C under flood waits behind frames blocked in nextDrawable (macOS)

Status: implemented behind a kill switch, not yet built or measured (no cargo on the authoring machine).

## The problem

Measured on the Mac (interleaved with Terminal.app in one session): Ctrl-C under a `yes` flood costs
about 100 ms more than the reference (budget 45 ms) and the figure has grown every run since Nov 3. The
plain-sleep control is unchanged, so it is specific to output frames. The app log shows the key event
dequeued, then a ~20 ms `drawable_wait`, then `ctrl_c_keydown`; the flood produces back-to-back
`redraw=true` frames each blocked 15-40 ms in `nextDrawable`.

## The render loop (what the code does)

* `WarpHostView displayLayer:` (host_view.m) -> `warp_update_layer` (window.rs) -> `Renderer::render`
  (rendering/metal/renderer.rs) -> `CAMetalLayer nextDrawable`, all on the main thread.
* The layer has `maximumDrawableCount = 2` (host_view.m, set for #1298). One drawable is on screen, at
  most one more can be queued, so a third request blocks the main thread until the queued one is
  presented. That is the 15-40 ms wait, and while it lasts the thread cannot dequeue a key.
* Redraws are requested with `setNeedsDisplayAsync` (a `dispatch_async` on the main queue). A flood
  keeps one pending at all times, so the main thread alternates frame / short gap / frame, and the gap
  is where a key is seen.
* Input is synchronous: `sendEvent:` -> `performKeyEquivalent:`/`keyDown:` -> `keyDownImpl` (which
  runs `interpretKeyEvents`) -> `warp_handle_view_event` -> `dispatch_event` -> `ctrl_c_keydown`.

What changed since v0.1.17 on these paths (`git log v0.1.17..HEAD` over warpui/warpui_core): only the
appci find hook (#1789, a cached env check) and the test-only changes. No render-loop change, so
nothing here explains a code regression; the growth is consistent with drift plus a loop that was
already exposed to this wait (the issue's own conclusion: re-run on both builds to separate them).

## What the existing hops cannot split

1. NSEvent hardware time -> `sendEvent:` is `os_key_event age_ms` (already logged).
2. `sendEvent:` -> `ctrl_c_keydown` has no hops, yet the log shows a `drawable_wait` completing inside
   it. A frame cannot run while the main thread is in `sendEvent:`, unless AppKit or the input method
   flushes display from inside `interpretKeyEvents`. Which stage it is, is unknown.
3. After the `^C` write the shell reacts in 2-8 ms (`CommandFinished`, `Precmd`), so the remaining
   unsplit time is probably the prompt waiting for a frame, or the reader draining flood bytes first.

## Options

(a) **Never block the main thread in nextDrawable (chosen).** Count drawables handed out whose
    presented handler has not run. With max 2 and one on screen, `unpresented >= 1` means the next
    `nextDrawable` would block until exactly that drawable is presented. Skip the frame instead,
    return to the run loop (input is dequeued at once), and let that presented handler ask the view to
    draw again. The frame is not lost and not delayed: the draw happens at the moment the blocking
    call would have returned, plus one main-queue hop. Fixes both the wait before the key and the
    wait inside any key-handling stage that triggers a draw. Local to the Mac renderer.
(b) Handle pending input before starting an output-caused frame. AppKit offers no cheap "peek the
    event queue" from inside `displayLayer:`; `nextEventMatchingMask` from there would re-enter event
    handling and reorder keys (forbidden). Rejected on risk.
(c) Pace flood frames to the display refresh (CADisplayLink). Larger change (a second frame clock,
    interaction with the display-sync-off-while-typing logic of #1298) and does not remove the
    blocking call, only makes it rarer. Candidate follow-up if (a) is not enough.

Ranking by risk/benefit: (a) > (c) > (b).

## Implementation of (a)

* `host_view.m`: `WarpFrameGate` per `CAMetalLayer` (associated object): `unpresented`,
  `presentedOnce`, `skipped`. `warp_frame_track_drawable` increments `unpresented`; the presented
  handler decrements it, sets `presentedOnce` for a real present, and if a frame was skipped posts
  `setNeedsDisplay:` to the view on the main queue. `warp_frame_gate_unpresented` / `warp_frame_gate_arm`
  are the two calls the renderer makes. `arm` re-checks after setting `skipped`, so a handler racing
  with the skip cannot leave the wake-up unarmed.
* `renderer.rs`: before `nextDrawable`, ask `frame_latency::gate_decision`. Skip returns early without
  rescheduling (same exit as the existing nil-drawable path). Frames with a capture callback pending
  or `presentsWithTransaction` (live resize) always draw. A skip run that ends logs
  `hop=frame_skip_run skipped=N held_ms=M`.
* `frame_latency.rs`: pure `gate_decision`, `SkipRun`, kill switch.
* Safety nets, so the window cannot be left stale: (1) a 100 ms `dispatch_after` armed with every skip
  requests a redraw if nothing woke the view; (2) a skip run older than 100 ms (`GATE_WATCHDOG_MS`)
  forces a draw even if it blocks, as today. A dropped drawable (presentedTime 0) still runs its handler.
* **Kill switch:** `PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES=0` restores the old behaviour exactly.
* **Default: ON.** The decision is conservative (skips only when the old code was certain to block
  under the max-2 model, and the wake fires at the instant the block would have ended), bounded by two
  nets, and Mac-only. The model is not verified on hardware, so the owner may prefer to flip the
  default for the first Mac run; that is a one-line change in `gate_enabled`.

## Added measurement (the unsplit ~60 ms)

* `hop=mac_ctrl stage=<key_down_impl_entry|after_interpret_key_events|after_handle_view_event>
  since_send_ms=` for every Control key-down: places the in-key-handling frame.
* `hop=ctrl_c_frame since_write_ms= since_precmd_ms= presented= after_precmd=` for each presented frame
  after the `^C` pty write until the first frame after the prompt hook: separates "shell reacted late"
  from "prompt waited for a frame".
* Existing: `os_key_event age_ms` (NSEvent arrival -> app dequeue), `drawable_wait`.

## What the next Mac run should show

* `drawable_wait` lines >= 2 ms should mostly disappear during the flood; `frame_skip_run` lines appear
  with `held_ms` around one frame interval.
* `os_key_event age_ms` p50 back near the plain figure (2 / 3.7 ms, was 9 / 19), `sendEvent -> ctrl_c_keydown`
  near 5 ms (was 17-19).
* If overhead stays high, `mac_ctrl` and `ctrl_c_frame` say which part is left; the next step is then
  option (c) or reader-side draining, not a guess.
* Throughput of the flood itself (lines painted per second) must not drop; compare `frame_skip_run`
  counts with the old frame rate. If it drops, set the kill switch and report.
