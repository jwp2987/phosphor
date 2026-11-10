# The per-key frame build: where the time goes, and what an incremental build would take

Status: **design + measurement only.** The measurement hop of section 5 is implemented on this branch
(no behaviour change, **not built and not run**: no cargo on the authoring machine). No incremental
build is implemented: section 6 says why and what the owner has to decide. Issues: #1961 (this work),
#1962 (views re-rendered per key), #1848 / #1843 (the latency cases this serves).

Follows `docs/design/1843-1848-latency.md`, which ends with "the remainder is the per-key scene build,
about 4 ms". **That figure is wrong**, and correcting it changes the recommendation, so it comes first.

Evidence: app logs in the job artifacts of pipeline 3047198675 (`app-test-linux-wayland` 17879373014,
`app-test-macos` 17879373016; they expire 2026-11-17), results in
`test-results/2026-11-09_build_r18w8_3047198675/`. Lines used: `frame 1: updated=[...]`
(`warpui::frame_cause`), `hop=key_frame`, `hop=key_frame_sched`, `hop=key_presented`, `hop=frame_drawn`,
`hop=ui_item kind=frame`, `keydown`.

## 1. What a key's frame costs

### 1.1 The build is 0.8-2.0 ms, not 4 ms

"About 4 ms" came from `hop=ui_item kind=frame ms=4.0-4.35`. That line is only written for a frame of
4 ms or more (`UI_ITEM_BUDGET`), so it describes the slow tail: 8 of 12,222 key frames in the Wayland
terminal-mode log, 83 of 1,001 in agent@fake, 13 of 14,453 on the Mac. The median is what every key
pays:

| run (first frame after a key-down) | n | frame build p50 | p75 | p95 |
|---|---|---|---|---|
| macOS, terminal mode, whole log | 14,230 | 0.79 ms | 1.16 | 1.85 |
| macOS, agent@fake, whole log | 970 | 1.39 | 1.86 | 2.82 |
| macOS, `latency_key_log` terminal (a typing burst) | 68 | 0.96 | 1.28 | 1.36 |
| macOS, `latency_paint` terminal (**the judged case**) | 20 | 1.90 | 2.00 | 2.66 |
| macOS, `latency_paint` agent | 20 | 2.08 | 2.14 | 2.30 |
| Wayland, terminal mode, whole log | 12,222 | 0.76 | 1.03 | 1.81 |
| Wayland, agent@fake (a conversation on screen) | 1,001 | 2.02 | 2.79 | 4.54 |

How it is read. macOS: `hop=key_frame_sched` is logged on entry to `warp_update_layer` and
`hop=key_frame` when the presenter has built the scene, so the difference is take-invalidations + view
renders + layout + after-layout + paint. Linux: `hop=frame_drawn total_ms` covers the build and the
render; `frame_drawn.t - key_frame.t` is what follows the build, and the rest is the build.

### 1.2 The judged case on the Mac, hop by hop

`latency_paint`, terminal mode, 20 keys, keyDown -> frame handed to the compositor
(`key_to_presented_ms - submit_to_presented_ms`), mean 8.50 ms (p50 9.35):

| part | mean | how measured |
|---|---|---|
| key handling before the redraw request | 1.44 ms | `keydown` line -> `key_frame_sched` entry, minus `key_frame_sched ms` |
| `key_frame_sched` (AppKit's display pass) | 3.52 | the hop; **#1848 removes this** |
| frame build | 1.83 | `key_frame_sched` -> `key_frame` |
| `nextDrawable` wait | 0.55 | `key_presented wait_ms` (p50 0.08, a quarter of the keys 1.2-1.9) |
| not covered by any hop | 1.16 | the remainder; 0.32 ms in `latency_key_log` |

The uncovered 1.16 ms sits between "scene built" and `nextDrawable`. In code that is the tail of
`build_scene_inner` (`crates/warpui_core/src/core/app.rs`): the text-layout cache generation, **the
synthetic mouse-move dispatched through the whole element tree after every build** (hover state for the
new layout), the second pass of the build loop that it can cause, the cursor-position report; then the
renderer's gate. Nothing times any of it.

After #1848 the app-side time before submit is therefore about 5 ms in three parts of similar size
(handling 1.4, build 1.8, after-build 1.2 + drawable 0.5), against a predicted overhead of about 3.3 ms
over Terminal.app (7.45 - 4.2; Terminal.app pays its own handling and drawing). **Deleting the whole
frame build would save 1.8 ms and still not reach the 2 ms budget by itself;** a realistic incremental
build saves a part of it.

### 1.3 Which views a key re-renders (#1962)

A view is re-rendered when it is notified; the rest of the tree keeps its elements but is laid out and
painted again. Most common `updated=[...]` sets of key frames:

| run | share | views re-rendered | build p50 |
|---|---|---|---|
| terminal, typing (Mac / Wayland) | 43 % / 40 % | `EditorView`, `AcceptAutosuggestionKeybinding` | 0.65 / 0.67 ms |
| | 16 % / 31 % | + `Input` | 0.98 / 0.78 |
| | 10 % / 16 % | + `Input`, `InputSuggestions`, `QueuedPromptsPanelView`, `Terminal`, `TerminalInputMessageBar` | 0.87 / 1.03 |
| agent@fake, typing (Mac / Wayland) | 79 % / 72 % | `EditorView`, `AcceptAutosuggestionKeybinding`, `Input`, `AgentMessageBar` | 1.32 / 1.78 |
| | 14 % / 23 % | + `Terminal`, `AIBlock`, `BlocklistAIStatusBar` (a streamed chunk in the same frame) | 2.19 / 3.56 |
| Mac `latency_paint` terminal | 20 of 20 | `EditorView`, `AcceptAutosuggestionKeybinding`, `TerminalInputMessageBar`, `UniversalDeveloperInputButtonBar`, `BlocklistAIStatusBar`, `QueuedPromptsPanelView` | 1.90 |

Two things follow. The judged case is the expensive one: a single character after seconds of idle
re-renders six views and runs on cold caches (1.90 ms against 0.96 ms for the same editor in a burst).
And the cost tracks the number and kind of views re-rendered as much as the size of the window, which
is something an incremental *layout* does not touch (a re-rendered view is laid out in every design).

### 1.4 What the round-8 logs cannot say

* layout vs paint vs view render inside the build: `render_ms` / `layout_paint_ms` exist only on the
  >= 4 ms tail (there: render 0.57 ms, layout + paint 4.1 ms, agent@fake Wayland);
* which views the layout and paint time is in;
* the cost of the hover pass, and how often it makes the frame lay out and paint twice;
* on Linux, how "scene built -> drawn" (0.81 ms terminal, 2.26 ms agent@fake) divides between that pass
  and the GPU encode. Paint-only frames (cursor blink) total 2.8 ms in agent@fake against 4.0 ms for a
  two-pass-free key frame, which bounds layout + render of four views at roughly 1.2 ms there, but
  blink frames run on an idle machine and the comparison is not clean.

Choosing between the options below needs exactly these numbers.

## 2. How a frame is built today (what any incremental scheme has to respect)

`AppContext::build_scene_inner` -> `Presenter::invalidate` (render the notified views into
`rendered_views`) -> `Presenter::build_scene_impl`: `layout` from the root, `after_layout`, `paint` into
a **new** `Scene`, then the synthetic mouse-move and up to two more passes.

* Elements are retained per view between frames; a re-rendered view gets a fresh, un-laid-out tree.
* `Element::layout(constraint, ctx, app)` gets the whole `AppContext`: an element may read any model
  while laying out (the block list reads the terminal model; `repaint_after` exists for elements that
  rebuild content in layout). A view's layout is not a function of its own state and its constraint.
* Layout rebuilds `mounted_views`, `parents` and `lazy_children` from nothing each pass (#805, #842);
  focus reachability and the hidden-tab frame skip read them.
* Paint has side effects besides the scene: the position cache (single-frame positions are cleared
  before every build and must be registered again), repaint timers and their layout flag (#703),
  pending assets and rasters, `views_painted` (cursor reset), each element's stored origin (event
  dispatch and hit-testing use it), and the scene's layers and hit records (`is_covered`,
  `visible_rect`).
* The one existing shortcut is the paint-only frame (#703): no view notified, nothing removed, no
  plain redraw request. A key always notifies a view, so a key frame never qualifies.

The same audit was made once for a smaller question (#1213, the comment on
`Presenter::render_unmounted_updates`): deferring a hidden view's render was judged "not provably safe
across the hundreds of view types" and the cost was addressed at its sources instead.

## 3. Options

| | option | what it saves | size | risk |
|---|---|---|---|---|
| A | **Fewer views re-rendered per key** (#1962): stop notifying views whose output did not change for one more character | their `render`, and their share of layout (a fresh tree is laid out cold) | small per view, in app code | low: each is a local "did anything I show change" check with its own test; a miss is a stale label, found by the view's test |
| B | **Hover pass after the frame, not inside it**: dispatch the synthetic mouse-move after the scene is handed to the renderer, and only when a layout ran; a hover change then invalidates and draws the next frame | the pass itself for every frame (up to the 1.2 ms of 1.2), and the same-frame second layout + paint when it invalidates | small (one function), kill switch | medium: a hover highlight can lag one frame behind a layout change; anything that relies on the second pass happening before the first present has to be found (the measurement says how often a second pass happens at all) |
| C | **Layout reuse for clean view subtrees** (a relayout boundary at `ChildView`): a view not re-rendered since its last layout, with no re-rendered descendant and the same constraint, returns its stored size; its mounted / parent / lazy records are replayed | layout of everything but the notified views and their ancestors | large: per-view records in the presenter, dirty-ancestor marking, replay of three sets, opt-in per view type, a verify mode | high: layout reads models (section 2), so a view type is only safe when audited and opted in, and the views that cost most (terminal block list, agent blocks) are the ones that read models in layout. A wrong reuse is a stale or mis-sized view until the next full layout |
| D | **Scene fragments per view** (a repaint boundary): keep each clean view's primitives, positions, timers and hit records and splice them into the new scene | paint of clean views | very large: the scene is flat layers in paint order; every paint side effect of section 2 has to be recorded and replayed, and translated when an origin moves | high: stale pixels, stale hit-testing, lost repaint timers |
| E | **Editor-only repaint**: when the editor's size did not change, patch only its region of the previous scene | layout and paint of the rest | C + D for one view, plus proof that nothing else depends on the buffer (suggestions, message bars, footers do: section 1.3) | high, and the frames it could serve are the 40 % with two views re-rendered, already the cheapest (0.65 ms) |
| F | **Damage tracking** (present only the changed rectangle) | GPU fill, not the CPU build | n/a | wgpu and `CAMetalLayer` expose no partial present; the present is not where the time is (`present_ms` 0.1) |

Not offered: any change to a budget, a verdict or a reference.

## 4. Risks common to C, D and E

* **Missed invalidation = stale frame.** Today a missed notification costs nothing visible as long as
  *any* frame is built, because every frame lays out and paints everything. With reuse, the same miss
  shows old content until that view is notified. The fork has several fixes of exactly that shape for
  the one existing shortcut (#841, #842, #703 follow-ups).
* **Hidden inputs.** Window size, zoom, theme, font fallback arriving, focus, the position cache read in
  layout: each must invalidate every cache. One missed input is a bug that appears once a week.
* **Verification.** The only reliable check is running both paths and comparing (sizes for C, scenes
  for D), which costs more than the saving, so it can run in tests and a debug mode, not in the field.
* **Gain is bounded by section 1.** At most the non-editor share of 0.8-1.9 ms, unknown until measured;
  the notified views and their ancestors are laid out and painted in every design.

## 5. Measurement plan

### 5.1 Implemented here: `hop=key_frame_build` (no behaviour change)

For the first frame built after a key-down, with input-order logging on:

```
input_order hop=key_frame_build passes=1 layout_passes=1 views=4 mounted=57 render_ms=0.21
  layout_ms=0.63 after_layout_ms=0.02 paint_ms=0.48 hover_ms=0.31 total_ms=1.80
  layout_top=Terminal:0.31,Workspace:0.10,EditorView:0.08 paint_top=Terminal:0.22,... t=...
```

* `passes` / `layout_passes`: passes of the build loop (2 or more = the hover pass invalidated and the
  frame was laid out and painted again);
* `views`: views re-rendered; `mounted`: views in the tree;
* the stages of the build, `hover_ms` (the synthetic mouse-move), `total_ms` (the whole
  `build_scene_inner`; total minus the stages is the untimed rest);
* `layout_top` / `paint_top`: the three view types with the largest **self** time (their own elements,
  without the child views inside them). Type names only, like the `frame_cause` line; no text.

Code: `crates/warpui_core/src/frame_split.rs` (pure, unit-tested), `presenter.rs` (stage times; a
per-view clock that exists only while such a frame is built), `core/app.rs` (`build_scene_inner`),
`input_order.rs`. Cost on a frame that is not logged: six `Instant` reads and one atomic load. There is
no kill switch because nothing a frame shows, or when it is drawn, depends on it; it is silent without
input-order logging, like every other hop.

Harness: `appci/render_hops.py` turns the line into `key_frame_build_*` metrics in every case that
already records render hops through `render_hops.record` (`latency_paint`, `agent_latency_key`,
`ctrl_c_flood_ab`). Metrics only; no verdict reads them.

### 5.2 How each option would be judged

Like #1838 / #1848: an environment kill switch read at start-up, and both arms in the same run,
interleaved, relaunch per block (`latency_paint_ab` on the Mac with kl3 key -> first changed pixel;
an arm of `agent_latency_key` on Linux). Per arm: key -> paint p50 / p95, `key_frame_build_total_ms`,
and for B `key_frame_builds_multi_pass` and `key_frame_build_hover_ms`. The verdicts stay with
`latency_paint` and `agent_latency_key` on the default arm. An option ships default-on only when its
arm is faster by more than the A/B's own noise and its unit tests cover the invalidation rules.

## 6. Recommendation

1. **Take the measurement (5.1) into the next build.** It is the only part implemented.
2. **Then A and B, each as its own small change with a kill switch where behaviour changes (B), if
   the numbers are there:** A when `render_ms` plus the notified views' share of `layout_top` is
   0.3 ms or more in `latency_paint`; B when `hover_ms` is 0.3 ms or more or a second pass is common.
   Together with the 1.4 ms of key handling (not a scene-build question; #1848's issue text names an
   "edit sync" of about that size) these are where the remaining 3.3 ms is, in three roughly equal
   parts. None alone reaches 2 ms.
3. **Do not build C, D or E for v0.1.18.** They are the "incremental scene build" that was asked for,
   and they are neither small nor safe: the frame they would shorten is 0.8-1.9 ms, their saving is an
   unknown fraction of it, and their failure mode is a stale window. If, after A and B, `layout_ms`
   is still above about 0.5 ms and `layout_top` shows it concentrated in one or two view types, C
   limited to those types (opt-in, verify mode in tests, kill switch, default off until its A/B) is the
   next step, with its own design review.
4. F is closed.

### Owner decisions

1. Is the measurement hop acceptable as the whole of "step 2" for this round? (No kill switch, because
   it changes nothing; say so if one is wanted anyway.)
2. Schedule A (#1962) and B for v0.1.18 once the next run's numbers are in? B changes when a hover
   highlight follows a layout change (one frame later) and needs a yes before it is written.
3. C: agree that it waits for the numbers and is not v0.1.18 work?
4. #1848 is expected to stay a FAIL after its own fix (about 3.3 ms against 2). No budget change is
   proposed; the question is only whether the release waits for A and B.

## 7. What the next real run should show

* every key frame in the latency cases logs one `hop=key_frame_build`; results carry
  `key_frame_builds` about equal to the keys typed and the `key_frame_build_*` metrics;
* `key_frame_build_total_ms` p50 near section 1.1 (Mac `latency_paint` about 1.9 ms plus the hover
  pass, which 1.1 does not include; terminal typing about 0.8-1.0 ms). A p50 near 4 ms would contradict
  this document;
* `key_frame_build_hover_ms`: the 1.16 ms unknown of section 1.2 is either mostly here (then B is
  worth it) or not (then the renderer's side before `nextDrawable` is the next thing to time);
* `key_frame_builds_multi_pass`: how many key frames were laid out and painted twice;
* `key_frame_build_views` p50 6 in `latency_paint` (2-4 while typing), unchanged until #1962 is worked;
* no change in any latency number from the hop itself: `latency_key_log` key -> frame within its
  baseline. If it moves by more than the baseline's noise, the per-view clock is too expensive and
  should be dropped from the hop (the stage times alone are six clock reads).
