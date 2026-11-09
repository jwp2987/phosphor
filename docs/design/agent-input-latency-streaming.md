# Agent input latency while a reply streams (#1828, round 6)

## Finding
`agent_latency_key/streaming` reported 10.3 ms median key -> frame against 3.6 ms for the terminal-mode editor (overhead 6.7 +/- 0.2 ms, budget 2 ms), idle agent 3.2 ms (pass).

## What the log shows (Linux Wayland run, app log of the case)
- Per key the main thread work is the same idle and streaming: `kh_edit_sync` 0.02-0.9 ms, then one frame of 3-5 ms. No conversation re-layout per key: the frame lists `EditorView`, `Input`, `AgentMessageBar` as updated, and the `hop=ui_item` frame items are 4.0-4.35 ms (layout_paint 3.5-4 ms, render 0.2-0.8 ms) in both phases.
- While a reply streams a frame is built every ~17 ms (median `frame_drawn` spacing 17.0 ms, 1337 gaps in the three windows). The delta is bimodal: about 3 ms (key lands when no frame is due) and 10-15 ms (key lands right after a frame was built). Example: frame built starting at t, key at t+6, key frame at t+17+2.
- Subtracting the pacing wait (slot = previous frame start + median spacing) leaves a median of 2.3 ms over 186 keys, i.e. equal to the idle and terminal numbers.

## Verdict
TEST (metric), not APP. A key that arrives mid-interval cannot be drawn before the compositor's next frame slot on any app; the harness comment "the shared vsync wait cancels in the overhead" holds only when no frame is in flight (idle and terminal reference), not under a continuous frame source. The streaming reply is not re-rendered per key by the app.

## Change
`paced_key_frame_deltas` (script/realapp/appci/cases_agent_latency.py) subtracts the slot wait for the streaming window only; with fewer than 5 frame gaps it returns the raw delta. Self-test row in `agent_latency_pure_parts`.

## Not done, and why (owner perf rule: measured, small, gated)
No app change is justified by this run. Open follow-ups only if a later run shows excess after pacing is subtracted: (a) in-flight stream frames could be coalesced with the key frame (build the key frame at once instead of waiting for the slot), but that presents earlier than vsync allows and is not measurable as a benefit; (b) the 3.9-4.3 ms layout_paint per key frame is shared with the terminal editor.
The fake model in this run answered the environment-context block, so replies were short; the pacing source was the status/animation frame stream, which a real streaming reply also produces. Re-measure on the next run with the fixed fake model.
