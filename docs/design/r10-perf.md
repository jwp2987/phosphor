# Round 10, perf: output frames (#1979) and editor memory (#1340)

Branch `fix/r10-perf`, base merge/w0 `41340e2fd`. Nothing here was compiled or run (laptop: rustfmt and the appci
self-test only). Both issues turned out to describe an older build than the code under test, so this round adds the
measurement that proves or refutes each claim on the next run and changes no behaviour.

## #1979: "a visible busy tab drives ~1 frame per output line; hidden views are re-rendered ~60/s per pane"

### What the issue text is based on

Both sentences are the residue paragraph of the #805 closing note (2026-10-12, fix `52a379f24`, 2026-10-11) and of
#812, copied into `docs/design/user-annoyance-review.md` item 10. Two later changes are not in that text:

- `eb2968c3f` (2026-10-31, #1256) and `789cdaea4` (2026-10-31, #1282): `wakeup_redraw.rs`. A wakeup compares a
  fingerprint of the model state the element tree was built from (`RenderFingerprint`) with the state now
  (`app/src/terminal/view/wakeup_redraw.rs` `decide_with`). Unchanged means the tree is reused: `Relayout` in the block
  list, `PaintOnly` in the alt screen. Only a changed fingerprint is `Full` (`ctx.notify()`, a re-render).
- The hidden branch: `app/src/terminal/view.rs` `handle_terminal_wakeup`, the arm
  `WakeupRedraw::Relayout | WakeupRedraw::PaintOnly if !ctx.is_view_reachable(..)` asks for nothing at all (no notify,
  no frame); the footer views are notified only when `FooterFingerprint` changed.

### What the code does now (by reading)

- Visible tab. Wakeups are throttled to 60/s per view (`view.rs` `MAX_WAKEUPS_PER_SECOND`, `throttle_with_bypass` at
  the `spawn_stream_local` call). Each delivered wakeup with an unchanged fingerprint asks for one layout+paint of the
  existing tree (`ctx.request_redraw`), no view render. So "one frame per output line" holds only below 60 lines/s, and
  then it is the terminal doing its job: a line that arrived is shown. #812 measured this on 2026-10-19 (44 wakeups/s
  -> 45 frames/s, 1:1, no coalescing headroom) and the owner closed it: the only remaining lever is renderer work
  (retained target + damage rectangles), which touches every frame.
- Hidden tab. A wakeup whose fingerprint is unchanged does not re-render the view and builds no frame. A `Full` wakeup
  of a hidden view still calls `notify`; the presenter re-renders it in place and builds no frame
  (`crates/warpui_core/src/presenter.rs` `render_unmounted_updates`, `core/app.rs` `prepare_window_invalidations`).
  That happens when a fingerprint field changes (block start/end, long-running flip, a block evicted, footer state),
  not per output chunk.
- Measured so far: `agent_multitab_streaming`, r18w8 Wayland 3.7 frames/s with 6 hidden streaming tabs vs 4.0 idle,
  r18w8 and r18w7 macOS 4.0 vs 4.0 (`test-results/2026-11-09_build_r18w8_3047198675/app/*/summary.md`). That is frames
  with hidden AGENT replies. Nothing committed counts in-place re-renders, and nothing measures hidden PTY output since
  #1256.

So neither claim is supported by the current code, and neither is disproven by a committed number.

### Alternative B (skip the in-place re-render of unmounted views): reviewed, not taken

B means: a notified view that is not in the element tree is marked stale and rendered when a parent next mounts it.

- It was drafted for #805 and dropped: rendering inside layout deadlocks on tab switch (the block list holds the
  terminal model lock across child layout; `AIBlock::render` takes it). #805 closing note.
- It was audited again for #1213 and left as is; the reasons are the doc comment on `render_unmounted_updates`
  ("Why this re-renders eagerly ..."): a deferred render runs inside `layout`, a new calling context for every
  `View::render`; views notify from observers and rely on a current element tree for `after_layout` /
  `dispatch_event` before they are next shown. Not provable by reading across some hundreds of view types.
- A terminal-view-only variant (skip `notify` for a hidden view on `Full`, notify once when shown) needs a "this view
  is now shown" event that never misses. A miss is a stale screen after a tab switch. There is no such event today;
  showing a tab currently works because the tree is already fresh.
- The expected gain is unmeasured and, by reading, small: `Full` is not per chunk.

Rule applied: performance work only if measured, small and safe. B is none of the three today. Dropped.

### What this branch does instead

Measurement, numbers only, nothing computed while `warpui::frame_cause` debug logging is off:

- `wakeup: redraw=<Full|Relayout|PaintOnly> reachable=<bool> view=<id>` per delivered wakeup
  (`handle_terminal_wakeup`). `redraw=Full reachable=false` lines per second per view IS "hidden views re-rendered per
  pane".
- `unmounted_render: n=<count> views=[<type names>]` per flush that re-rendered hidden views in place
  (`render_unmounted_updates`). It names every view type, not only `TerminalView`, so an unexpected notifier shows up
  by name.
- appci case `output_frames` (PROVING, both modes, all platforms): idle window, shown tab streaming, 4 hidden tabs
  streaming. Verdicts in `docs/testing.md`. If the issue's claims are true of this build it FAILS and the message
  carries the rate and the view types; if they are stale it PASSES. Either way the next step is decided by a number.

Rejected: a frame-rate cap or a batching delay for the visible tab (adds latency to keystroke echo, #812's numbers show
no headroom); a "visible region dirty" bit in the parser (the parked `fix/812-visible-output-frames` measured no gain
and has a wide stale-screen surface).

Risk: two `log_enabled!` checks per wakeup / per flush with hidden notified views; the formatting runs only in a
debug-logging run. The wakeup line adds up to 60 lines/s per streaming pane to a test log.

## #1340: "code editor: a 7 MB file costs ~850 MB RSS"

### What the numbers say now

The 850 MB figure is r18av (2026-11-01), before P1-P5 (u32 offsets, hydrated/dehydrated paragraphs, lazy layout,
eviction, parked line chunks), the syntax cap at 5 MB and the close-release work. The last run of `editor_memory_7mb`
is r18u28 (2026-11-05, `test-results/2026-11-05_ssh_r18u28_*/results.json`); the case is in the candidate tier, so the
r18w1-w8 fix-tier runs did not run it.

| platform | txt open (3 cycles) | rs open | held 75 s after close | why the row was FAIL |
|---|---|---|---|---|
| Linux Wayland | +8, +49, +54 MB | +55 MB | txt -334 MB, rs +15 MB | opt-in open trim: load frame 15.2 ms vs 10.2 ms control |
| Linux X11 | +5, +42, +65 MB | +75 MB | txt -668 MB, rs +3 MB | harness: opt-in trim variant logged nothing, 274 of 595 keys lost before the app |
| macOS | +217 MB rss, +222 MB footprint | (accepted) | footprint below baseline | PASS; open cost accepted by the owner (#1607 closed "not a bug") |
| Windows | +86 MB (base 1029), then +258, +289 | +775 / +790 MB (base 87 / 50 MB) | +41 / +71 MB | working set against an OS-trimmed baseline; trim A/B attribution |

The app's own breakdown is the same on every platform: buffer tree 53.8 MB for the txt file (230,561 items of 136
bytes), render tree 1.3 MB + parked chunks 9.9 MB, 4 hydrated paragraphs; rs: 23.9 MB + 0.4 MB + 8.0 MB.

- Linux: inside the #1340 targets (100 MB txt, 150 MB rs, 60 MB after close) in every cycle of both sessions.
- Windows +775 MB is not editor memory. The baseline before the rs file was 87 MB of WORKING SET for an app that was
  1029 MB a few minutes earlier: the OS had trimmed it, and opening the file faulted the app's own pages back in. The
  app has no working-set call (`app/src/alloc.rs`: `HeapCompact` only). `3e64e7114` (2026-11-09, merged, not run yet)
  already makes Windows judge private bytes against a warm baseline and report the working set as a note.
- The "layout 15 ms" in the annoyance review is the Wayland row: 15.2 ms is the load's frame WITH the opt-in
  `PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1`; the default is 10.2 ms, no trim 10.1 ms.

### Verdict

- Memory: no app change. Linux is within target by measurement; macOS is an owner decision already made; Windows was a
  measurement fault whose correction is merged and unrun.
- Open trim (opt-in, default off): the A/B answered its own question on Linux. The trim is spawned from
  `CodeEditorModel::after_large_layout` (`app/src/code/editor/model.rs`). By reading, for the code pane's editor that
  is before the frame that shows the file, so the worker's `malloc_trim` (4.75 ms in that run, holding each arena's
  lock in turn) overlaps that frame's allocations; the measured +5.1 ms on a 10.2 ms frame fits that and nothing in
  the run contradicts it, but it is an inference, not a trace. Linux does not need the trim (open cost is within target without it). The leg keeps failing the
  whole case on a feature that is off. Owner decision, not taken here: delete the opt-in and its A/B leg, or keep it
  as an experiment and stop it failing the row.
- Deferring the trim until that frame has been drawn would remove the overlap, but the model has no window to ask for
  "next frame drawn"; it would need a new event through the editor view for a default-off path, uncompiled. Not done.
- The 10 ms default load frame (once per 7 MB open; budget 4 ms, warn 16) has no attribution yet. The `load_stage`
  lines of `37089d6af` / `3e64e7114` exist for that and have not run.

### What this branch does

- `editor_memory open ... walk_ms=<ms>`: the breakdown's two tree walks run on the main thread right after the
  load's layout, only under `editor_memory=debug` (which appci sets). They are inside no `hop=ui_item`, so the time was
  invisible; the case now notes it. Measurement only.

Rejected for this round: shrinking the buffer item (136 bytes x 2 items per line is the 54 MB; a sum-tree redesign,
P5's "buffer-backed source" follow-up) and any allocator change (compiler/allocator work is deferred until after the
next numbered release).

## What the next run should show

- Fix tier, every platform, both modes: `output_frames` PASS with, shown tab streaming, frames/s about equal to
  wakeups/s and under 66 + idle, almost no `redraw=Full`; hidden tabs streaming, frames/s equal to the idle window,
  `Full_hidden` near 0, `unmounted_views` near 0. A FAIL names the view types to look at.
- Candidate tier: `editor_memory_7mb` Windows rows judged on private bytes (the first honest Windows figure; no
  prediction beyond "not the working-set 775"); a `walk_ms` note per kind; `<kind>_load_stages` metrics to attribute the 10 ms frame. The
  Wayland row stays FAIL on the opt-in trim leg until the owner decides.
