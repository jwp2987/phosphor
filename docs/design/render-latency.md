<!-- Fable 5.2 read-only design pass, 2026-11-09 (#1848 #1843 #1838). Saved verbatim by the coordinator; first step = option A (measurement hops). -->

# Render latency: frame pipeline, hypotheses, options (design pass, read-only)

Scope: crates/warpui frame path (mac Metal, winit/wgpu), warpui_core invalidation + hops, appci harness. No cargo, no edits.

## 1. Frame pipeline per platform, with the measured milliseconds placed

### macOS (Metal, main thread does everything)

```
HID/CGEventPost -> NSEvent (timestamp)                                 | os_key_event age_ms p50 3.4 (idle!)
 -> WarpApp sendEvent: (app.m:503)  [os_key_event logged, warpCtrlSendMark]
 -> WarpWindow sendEvent:/performKeyEquivalent: -> keyDownImpl (host_view.m:376)
      sets warpNextFrameFromKey=YES, warpPendingKeyNs                   |
      interpretKeyEvents -> warp_handle_view_event -> dispatch_event    | kh_* hops; edit sync ~1.4 ms
      -> on_window_invalidated -> request_redraw (window.rs:1216)       |
         = next_scene.take() + setNeedsDisplayAsync                     | ~7 ms total
 -> [hop A] dispatch_async main queue -> setNeedsDisplay:YES (window.m:518) | os_key_event -> key_frame
 -> [hop B] CA transaction commit (runloop BeforeWaiting) -> displayLayer: (host_view.m:324)
      flips displaySyncEnabled (key frame: OFF) -> warp_update_layer (window.rs:1413)
      build_scene: full layout+paint for one char                      | ui_item frame ~4.0-4.35 ms (#1843, same code path)
      Renderer::render (metal/renderer.rs:1067): #1838 gate -> nextDrawable | drawable_wait (>=2 ms logged)
      warp_frame_track_drawable: attaches pending key, adds presentedHandler
      encode, presentDrawable + commit (async; renderer.rs:98-102)      | "submit"
 -> GPU -> WindowServer composite -> panel scanout                     | submit->presented ~20-28 ms INFERRED, NOT LOGGED
 -> presentedHandler (host_view.m:164) -> warp_frame_presented (frame_latency.rs:230)
      key_to_presented_ms exists but only as log::debug "frame presented=", not on the input_order target
 -> frame_drawn callback
```
Streaming/flood frames: pty reader thread -> model notify -> request_redraw -> setNeedsDisplayAsync, one per read, no coalescing. The only pacing is the #1838 gate (skip when unpresented>=1, redraw from the presented handler). The gate is NOT key-aware: a key frame arriving while a drawable is in flight is also skipped (must_draw only covers capture / presentsWithTransaction). Measured "never skipped a key's own frame" is luck of timing, not design.

Where the 34.5 ms p50 goes: 3.4 (queue) + ~7 (dispatch + two hops + 4 ms build + encode) + 20-28 (present stage, unmeasured) + kl3 quantisation (240 Hz, ±4). Terminal.app pays the WindowServer stage too, so the 5-7 ms overhead must sit in the first ~10 ms (the client stages) unless the present stage differs by mechanism (see H4/H5).

### Linux (winit + wgpu; GL software in the #1843 run)

```
evdev/Wayland -> winit event -> handle_event (event_loop/mod.rs:767): control flow reset, retries, owed redraws
 -> key dispatch -> request_redraw (winit/window.rs:1966) -> winit coalesces into ONE RedrawRequested at the END of this loop iteration
 -> redraw_window (event_loop/mod.rs ~1286-1360): build_scene if !has_scene (4 ms) -> window.render
      get_surface_texture (resources.rs:190, last_acquire_wait recorded) -> draw -> submit -> pre_present_notify -> surface_texture.present()
      present_mode AutoNoVsync -> Immediate/Mailbox/Fifo by support (resources.rs:904,947); PHOSPHOR_PRESENT_MODE override exists
      desired_maximum_frame_latency: wgpu default 2, DX12 forced to 1 (resources.rs:1011); PHOSPHOR_FRAME_LATENCY override exists
 -> hop=frame_drawn (includes acquire_wait; present() duration NOT split out)
```
Streaming cadence ~17 ms (#1843) = one frame per loop iteration, where each iteration is bounded by whatever blocks inside render (acquire or present under software GL / Wayland frame callbacks) plus the 32 ms shimmer repaint timer (shimmering_text.rs:231) and cursor blink (editor render/element/mod.rs:978). A key that lands during that frame is dispatched only after it, and its RedrawRequested comes at the end of the next iteration: bimodal 3 / 10-15 ms, exactly as measured.

### Windows
Same winit path, DX12 with desired_maximum_frame_latency=1: streaming 1.5 ms vs idle 3.6 (no penalty). This is the strongest clue in the data set: with queue depth 1 the main thread never sits behind a queued frame, and the key frame is the next thing presented.

## 2. Root-cause hypotheses, ranked, each with the one measurement

H1 (mac, #1848, likely 1-3 ms): two asynchronous hops between the key's invalidation and the frame build (dispatch_async turn + CA commit observer). Measure: `hop=key_frame_sched ms=` = time from request_redraw (stamp an atomic in window.rs:1216 when warpNextFrameFromKey is set) to displayLayer: entry (host_view.m:324 / warp_update_layer). Confirms if p50 >= 1 ms at idle.

H2 (mac+linux, ~4 ms, shared with terminal editor): full layout+paint per keystroke (build_scene_inner app.rs:3463; skip_layout only for paint-only timers). Already measured (ui_item frame 4.0-4.35). Not render-pipeline; note only. Confirm on mac by `ui_item kind=frame` lines in the latency_paint window. **Corrected 2026-11-09:** `ui_item` only logs frames of 4 ms or more, so 4.0-4.35 is the slow tail; the round-8 median per key is 0.8-2.0 ms (`docs/design/key-scene-build.md` section 1).

H3 (mac, 3.4 ms at idle): os_key_event age at idle is too large for an idle main thread; either blink paint-only frames (paint ~0.5-1 ms + nextDrawable) occupy the thread when the key arrives, or it is WindowServer->app IPC Terminal.app pays too. Measure: A/B latency_paint with cursor blink disabled (settings) vs enabled, reading `key_event_age_ms` (already a metric, cases.py:4177). If age drops to <1 ms with blink off, blink frames are stealing the dequeue.

H4 (mac, up to one composite period 8-16 ms): async presentDrawable lands after the WindowServer's composite deadline for the transaction the key's CA commit belongs to; Terminal.app's CA-drawn content is in the transaction and composites one period earlier. Measure: `hop=key_presented submit_to_presented_ms= key_to_presented_ms= sync=<displaySyncEnabled> wait_ms=` emitted on the input_order target from warp_frame_presented (frame_latency.rs:230; add a submit timestamp in warp_frame_track_drawable host_view.m:155 and pass it through). If submit->presented p50 is ~1 composite period (8.3 at 120, 16.7 at 60) and not ~GPU time (1-3 ms), H4 holds and option E below is the lever.

H5 (mac): toggling displaySyncEnabled per key frame costs the layer a resync. Measure: same key_presented hop, A/B PHOSPHOR_MAC_DISPLAY_SYNC_OFF_WHILE_TYPING=1 vs 0 (switch exists), compare submit_to_presented and kl3 overhead. #1298 measured the toggle as a win on ProMotion; re-check only if H4 shows an odd distribution.

H6 (linux, #1843): main thread blocks inside present()/acquire for most of the 17 ms (software GL swap or Wayland frame callback), the same shape as mac nextDrawable. Measure: add `present_ms=` and `acquire_ms=` fields to `hop=frame_drawn` (event_loop/mod.rs ~1350, time around surface_texture.present() in wgpu/renderer.rs:146). If present_ms p50 >= 8 during streaming, the fix is queue depth / present mode (option F), free to A/B with the existing env overrides.

H7 (mac Ctrl-C under flood, ~60 ms unexplained): (a) the gate skips the frame carrying the prompt and the wake adds one present + one hop (8-16 ms); (b) the reader drains flood bytes before the prompt is parsed; (c) shell latency. Hops mac_ctrl and ctrl_c_frame are in place; no new hop needed until that data is read. Add only `pty_read` count/bytes between the ^C write and precmd (derivable from existing pty_read lines in the harness).

## 3. Design options, ranked against budgets (key->paint <=2 ms overhead, UI <=4 ms, Ctrl-C <=20) and the safety rule

A. Measurement patch only (H1, H4, H6 hops). Risk: none (3 log lines, atomics). Gain: splits the 20-28 ms inferred stage and the 7 ms stage; makes every later option judgeable. Kill switch: not needed (input_order target gating). A/B: n/a; latency_paint and agent_latency read the new hops (regex additions in cases.py/cases_agent_latency.py).

B. Key-driven immediate frame (mac). After keyDownImpl's warp_handle_view_event returns and warpNextFrameFromKey is set and the window has a pending invalidation, call `[self.layer setNeedsDisplay]; [self.layer displayIfNeeded]` (synchronous displayLayer: -> warp_update_layer) instead of waiting for hops A+B; the async setNeedsDisplay still fires, so a per-window "frame generation" (stamp in request_redraw, compare in warp_update_layer) must turn the second displayLayer into a no-op when no new invalidation arrived (otherwise +4 ms rebuild per key). warp_update_layer already defers safely when the app is borrowed (DrawOwed). Gain: H1's 1-3 ms. Risk: low-medium (re-entrancy with IME is avoided by running after interpretKeyEvents; the dedupe is the only new logic). Kill switch: PHOSPHOR_MAC_KEY_FRAME_IMMEDIATE=0. A/B: clone cases_ctrlc_ab.py's relaunch-per-block pattern (`_arm_env`, `ab_schedule`) onto latency_paint, metric key_to_paint_ms per arm.

C. Key frames bypass the gate's skip (input preempts the streaming cadence, mac). When warpNextFrameFromKey is set and unpresented>=1: today the key frame is skipped and redrawn on present (+1 hop). Alternative is to draw anyway and block (forbidden), or to raise maximumDrawableCount to 3 for the key frame (adds a stale frame ahead: worse). Correct form: keep the skip but make the wake synchronous-ish (presented handler calls setNeedsDisplay already; the hop is dispatch_async + CA commit, same as H1), so B covers this too. No separate option; log `key_frame_skipped=` in frame_skip_run to quantify how often a key frame is skipped under flood.

D. Present-time pacing / coalescing output frames to present (mac). The gate already does this de facto (one in-flight drawable, redraw on present). Making it explicit (defer output-caused request_redraw while unpresented>=1, instead of building a scene that is then skipped) saves the 4 ms build of each skipped frame, i.e. main-thread time a key could use. Gain: only under floods; expected Ctrl-C-under-flood p50 improvement a few ms. Risk: medium (stale-window nets must be kept: watchdog, dispatch_after). Kill switch: PHOSPHOR_MAC_DEFER_OUTPUT_FRAMES=0. A/B: ctrl_c_flood_ab as is, new env.

E. presentsWithTransaction for key frames only (mac, tests H4). In displayLayer: when the frame is from a key and unpresented==0, set layer.presentsWithTransaction=YES for this frame (renderer already takes the sync path: commit, waitUntilCompleted, present inside the transaction; renderer.rs:105-116), reset afterwards. Gain: up to one composite period (8-16 ms) if H4 holds; this is the only option that can reach the <=2 ms overhead if the present stage is where Phosphor loses to Terminal.app. Risk: medium (main thread waits for GPU ~1-2 ms per key frame; must_draw forces the draw, so only when unpresented==0 or it blocks). Kill switch: PHOSPHOR_MAC_KEY_FRAME_TRANSACTION=0 (default OFF until measured). A/B: relaunch-per-block on latency_paint.

F. Linux/Windows queue depth and present mode (#1843). Zero-code A/B first: PHOSPHOR_PRESENT_MODE=mailbox and PHOSPHOR_FRAME_LATENCY=1 arms of agent_latency_key/streaming, reading paced and raw deltas plus the new present_ms. If an arm removes the penalty, promote it to the default for that backend in desired_frame_latency / resolve_auto_no_vsync with the env var as the kill switch (same shape as DX12=1 today). Risk: low (Vulkan Fifo-only compositors serialise CPU/GPU with 2 images, noted in resources.rs:1009; measure throughput).

G. CADisplayLink / CVDisplayLink pacing of output frames. Second frame clock, interacts with displaySync toggling; larger. Defer unless D is insufficient.

H. maximumDrawableCount=3 (triple buffering). Rejected: adds a queued stale frame in front of the key frame; the gate would skip more, not less.

## 4. Recommended first step

Land A (three hops, no behaviour change) and run latency_paint + ctrl_c_flood_ab on the Mac and agent_latency on Linux with the existing env A/Bs from F:
- mac: `hop=key_presented` (frame_latency.rs:230 + host_view.m:155), `hop=key_frame_sched` (window.rs:1216 -> host_view.m:324).
- linux/windows: `present_ms`/`acquire_ms` on `hop=frame_drawn` (event_loop/mod.rs ~1350, wgpu/renderer.rs:146).
Decision rule after that run: submit->presented ~ one composite period -> E; key_frame_sched >= 1 ms -> B; Linux present_ms >= 8 under streaming -> F default change. Nothing else is justified yet by the numbers.

### Critical Files for Implementation
- /home/winters/git/phosphor-merge/crates/warpui/src/platform/mac/objc/host_view.m (keyDownImpl 376, displayLayer 324, warp_frame_track_drawable 155, presented handler 164, layer setup 516)
- /home/winters/git/phosphor-merge/crates/warpui/src/platform/mac/frame_latency.rs (warp_frame_presented 230, gate_decision 176, kill switches)
- /home/winters/git/phosphor-merge/crates/warpui/src/platform/mac/window.rs (request_redraw 1216, schedule_redraw 1123, warp_update_layer 1413)
- /home/winters/git/phosphor-merge/crates/warpui/src/platform/mac/rendering/metal/renderer.rs (render 1067, finish_with_capture 87)
- /home/winters/git/phosphor-merge/crates/warpui/src/windowing/winit/event_loop/mod.rs (redraw_window ~1286-1360, frame_drawn hop) with /home/winters/git/phosphor-merge/crates/warpui/src/rendering/wgpu/resources.rs (present mode 904, frame latency 1011)
- Harness: /home/winters/git/phosphor-merge/script/realapp/appci/cases_ctrlc_ab.py (A/B pattern), /home/winters/git/phosphor-merge/script/realapp/appci/cases.py (c_lat_paint 4133)
