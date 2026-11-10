# The per-key frame build: where the time goes, and what an incremental build would take

Status: **measurement hop merged (#1961); option A implemented (#1962, owner decision 2026-11-09),
section 8.** Neither is built or run yet (no cargo on the authoring machine). Options B to E are not
implemented: section 6 says why. Issues: #1961 (the measurement), #1962 (views re-rendered per key,
option A), #1993 / #1994 (two missing notifications option A uncovered), #1995 (what option A left
open), #1848 / #1843 (the latency cases this serves).

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
   **A: decided 2026-11-09, implemented ahead of the numbers (section 8); it is judged by its own
   same-run A/B. B is still open.**
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

With option A in the build (section 8), `key_frame_build_views` is no longer expected to stay at 6 /
2-4: section 8.5 gives the expected numbers per arm.

## 8. Option A as implemented (#1962)

Owner decision 2026-11-09: implement option A. Branch `perf/1962-key-view-skip`. Kill switch
`PHOSPHOR_KEY_VIEW_SKIP=0` (read once; `app/src/util/key_view_skip.rs`), default ON. Only
notifications are skipped: no key is read, held or routed differently, and no model changes what it
stores or emits.

### 8.1 Why each view was notified, and whether it showed anything new

Line numbers are those of `main` at `127332d33` (before the change).

| view | notified per key by | does its output depend on that change? |
|---|---|---|
| `AcceptAutosuggestionKeybinding` | `app/src/editor/view/mod.rs:8440`: every `EditorModelEvent::Edited` calls `close_menu` on the hint, and `close_menu` (`accept_autosuggestion_keybinding_view.rs:142`) notified whether or not the menu was open | no. The hint draws the keystroke, the menu and hover state; an edit changes none of them unless the menu was open. (It is not notified when an autosuggestion appears: the editor element mounts it then.) |
| `Input` (agent view) | `app/src/terminal/input.rs:10830`: an unconditional `ctx.notify()` at the end of the shell-prefix (`!`) check of the `EditorEvent::Edited` handler, reached for every edit in an agent view unless the input is locked to shell mode or the agent holds the command | almost never. `render_agent_input` branches on the buffer only through "is it empty" (the border color) and "is a slash command detected"; everything else it reads has its own notification |
| `AgentMessageBar` | `agent_message_bar.rs:199`: it notified on every event of the slash-command model, and that model emits on every buffer change, also when its state stays `None` (`slash_command_model.rs:460`). This also made #1491's shell-mode fix ineffective for this bar | no for plain text: the row reads the buffer only as "empty or not" |
| `TerminalInputMessageBar` | `terminal_message_bar.rs:103`: a buffer event whose `BufferSignature` changed, which includes empty <-> non-empty; `:120`: every suggestions-mode change (each as-you-type completion result) | rarely. Emptiness changes the row only with a misroute offer, a failed last block, an agent block last in the list, or an AI-autodetected buffer; the mode only through "is the inline history menu open" |
| `UniversalDeveloperInputButtonBar` | `app/src/terminal/view.rs:32488` -> `universal_developer_input.rs:599` `update_input_empty_state` -> `notify_and_notify_children` on every empty <-> non-empty change | not the bar: only its segmented control's options read the empty state (the "Auto" label and its colors) |
| `BlocklistAIStatusBar` | `status_bar.rs:293`: a shell-mode buffer changing between empty and non-empty | no: its `render` reads nothing of the buffer (the subscription predates the agent view) |
| `QueuedPromptsPanelView` | `queued_prompts_panel.rs:333`: the host input changing between empty and non-empty; `:253`: every suggestions-mode event | only while it is shown (the "⏎ to send" hint). With no queue it renders nothing |

**Why `latency_paint` re-renders six views.** It alternates a character and Delete
(`cases.paint_samples`), so every key changes the buffer between empty and non-empty, and four of the
six notifications above are keyed on exactly that. It measures the first character of a line, not
typing in general; a burst re-renders two (terminal) or four (agent) views.

### 8.2 The rule: compare with the last render, not with the last event

A notified view is rendered from all the state it reads, so a notification that looks redundant may
be the only thing that picks up a change that has no notification of its own. Two such cases were
found and are fixed where they belong (#1993: a changed accept-autosuggestion keybinding was drawn
only at the next edit; #1994: nothing told the input when the queued-prompts panel appeared or
disappeared). Others certainly exist (the agent message bar reads the shell's bootstrap state, the
plan count and the conversation length without a subscription for any of them).

So no skip is of the form "this event cannot change the view". Each view records, **in `render`**,
what it was built from, and an event that used to notify compares what the view would be built from
**now** with that record. Equal means a render now would build the same elements, whatever happened
in between; different or unknown means notify, as before. A change with no notification of its own is
therefore still corrected at the next key, exactly as the per-key render did.

| view | what is compared | unknown (= render) when |
|---|---|---|
| `AcceptAutosuggestionKeybinding` | "was the menu open" (closing a closed menu) | never |
| `Input` (agent view) | `AgentInputDisplayFacts`: every fact `render_agent_input` branches on (empty buffer, detected slash command, input type and lock, kind of open menu, banner, attachments, workflow info, voltron, focus, x-ray, shortcuts, inline view, queued-prompts panel, pane height) | the last render showed another input |
| `AgentMessageBar`, `TerminalInputMessageBar` | the row itself: `Message::display_fingerprint` (text, keystrokes, icons, colors; for the agent bar also its seven clickable hints, which dispatch fixed actions) | the row has a link, a chip, an image or any other clickable group (a click handler is not in what it shows); the terminal model is busy (`try_lock`) |
| `UniversalDeveloperInputButtonBar` | what the bar's own `render` reads (viewer, model selector, prompt alert, compact threshold); the segmented control is still notified | never rendered; the terminal model is busy |
| `BlocklistAIStatusBar` | "the last render drew nothing" and "a render now would draw nothing" (no agent view, no tagged-in block, no warping indicator) | anything else: the bar may draw something, so it renders |
| `QueuedPromptsPanelView` | hidden before and hidden now (`should_render`); a change of that re-renders the panel and tells the input | never |

Three facts of the agent input settle after the edit's own handler or change without an edit; each
now asks for the render itself: the slash-command model's handler, a replaced buffer, and the panel's
`ShownChanged` event. Those and the two fixes above are not behind the switch (they only add renders).

Not changed: the slash-command model still emits on every buffer change (the input's handler closes
the slash menu on those events), and every notification that reports a real change (input type, lock,
mode, context, conversation status, settings).

### 8.3 What a typed character is expected to re-render now

| typing | before (round 8) | expected |
|---|---|---|
| terminal, a character in a non-empty buffer | `EditorView`, `AcceptAutosuggestionKeybinding` | `EditorView` |
| terminal, with an as-you-type completion result in the frame | + `Input`, `InputSuggestions`, `QueuedPromptsPanelView`, `Terminal`, `TerminalInputMessageBar` | + `Input`, `InputSuggestions`, `Terminal` (the menu changed) |
| agent view, a character in a non-empty buffer | `EditorView`, `AcceptAutosuggestionKeybinding`, `Input`, `AgentMessageBar` | `EditorView` |
| terminal, the first character / Delete to empty (`latency_paint`) | six views | `EditorView` |
| agent view, the first character / Delete to empty | six or seven views | `EditorView`, `Input` (border color), `AgentMessageBar` (the hints are enabled only for an empty buffer); `BlocklistAIStatusBar` where it was the seventh (a shell-mode buffer in an agent view: the bar draws something there, so its notification is kept) |

These are the sets of the frame (`updated=[...]`, `views=`). A notified view that is not mounted and
whose parent is not mounted either is rendered at once, outside the frame, and is not in the set
(#805, `Presenter::render_unmounted_updates`): the button bar's segmented control and slash button
are such views in both modes (the bar itself is a child of the mounted input, so it was in the set),
and so is the agent message bar in terminal mode. Their render is then part of the key's handling
rather than of its frame build; the skips remove it there as well, which no `key_frame_build` number
shows. The segmented control and the slash button are still notified for empty <-> non-empty.

### 8.4 Not done

* **`Input` in terminal-mode typing (16-31 % of key frames in round 8), #1995.** None of the causes
  above explains a key frame whose set is `EditorView`, `AcceptAutosuggestionKeybinding`, `Input`
  and nothing else; the issue lists what was read and what would settle it.
* The agent view's first character still re-renders `Input` and `AgentMessageBar`: both show
  something different for an empty buffer.
* The segmented control is still notified for empty <-> non-empty even when its label does not
  change (a locked input). It is one small view.
* No view-level test asserts "typing does not notify view X" on a running window: the test harness
  builds no frames, and views that are not mounted are rendered and dropped from the set at once
  (#805). The decisions are pure functions with unit tests, three state tests run on a real terminal
  view, and the set itself is what `latency_key_views_ab` and `frame 1: updated=[...]` show.

### 8.5 How it is judged, and what the next run should show

`latency_key_views_ab` (PROVING; Linux and macOS; terminal and agent mode;
`script/realapp/appci/cases_viewskip_ab.py`): blocks alternate between the default environment and
`PHOSPHOR_KEY_VIEW_SKIP=0`, a relaunch per block, and `hop=key_frame_build` is read per arm for the
`latency_paint` pattern (`first`) and for a typing burst (`burst`). On macOS the `first` keys go
through kl3, which also gives key -> first changed pixel per arm. No verdict or budget is touched;
`latency_paint` and `agent_latency_key` judge the default arm as before.

Expected (`key_view_skip_on` against `key_view_skip_off`, medians):

* `burst`, terminal: views 1 against 2. `burst`, agent: 1 against 4. The view count of the off arm
  must match round 8; if it does not, the kill switch is not restoring the old notifications.
* `first`, terminal: 1 against 6. `first`, agent: 3 against 6 (4 against 7 where the status bar is
  in the set).
* `render_ms` lower in the on arm in every pattern; `total_ms` lower by the skipped views' render
  plus their cold layout. Section 1.3 puts that at about 0.3 ms (agent burst: `Input`) to 0.5 ms
  (agent burst: `Input` + `AgentMessageBar`) on the Mac, and less for the terminal burst (one small
  view). For `latency_paint` the round-8 gap between six views and a burst was 0.9 ms, of which an
  unknown part is cache warmth after the idle gap, which this change does not touch.
* macOS `key_to_paint` p50: lower in the on arm by about the `total_ms` difference, if that is above
  the 4.17 ms kl3 quantum's noise; no difference visible there is not evidence against the change.
* no WARN about the A/B itself, and never more views in the on arm (that is the case's one FAIL).
* `frame 1: updated=[...]` of key frames names the sets of section 8.3.
* every existing input, routing, completion and agent case unchanged: a stale label, hint, border or
  panel anywhere near the input is this change's failure mode. With `PHOSPHOR_KEY_VIEW_SKIP=0` the
  same case passing would confirm it.

If the on arm is not faster by more than the A/B's own noise, the change is still correct (fewer
renders of identical elements) but is not what closes #1848; option B and the key-handling share
(section 6) are then next.
