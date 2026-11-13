# Code editor: large-file memory (#1340)

Branch `fix/r23-editorrss`. Nothing compiled or run here (laptop: no cargo). Design and evidence only; no behaviour change.
Companion note with the per-platform table: `docs/design/r10-perf.md` ("#1340").

## Where the memory went (the 850 MB of r18av) and what already changed it

| cost | where | per-unit estimate | status |
|---|---|---|---|
| Eager layout of every line: one `Paragraph`/`TextFrame` per line, per-char `Glyph` + `CaretPosition` | `crates/editor/src/render/layout.rs`, `render/model/hydration.rs` | ~100+ B per char with usize offsets: the bulk (~600-1000 MB at 7 MB) | fixed by P1-P5: u32 offsets, hydrated/dehydrated paragraphs, lazy viewport layout, eviction, parked line chunks (`render/model/parked.rs`); kill switch `PHOSPHOR_EDITOR_EAGER_LAYOUT=1` |
| Per-style-run glyph Vec keeping line-sized capacity | `crates/warpui/src/windowing/winit/fonts/text_layout.rs` `flush_current_style_run` | slack of up to a line per run | fixed (shrunk) per the issue notes |
| Buffer sum-tree items | `crates/editor` buffer | 230,561 items x 136 B = 53.8 MB (txt), 23.9 MB (rs) | remaining, by design: the text itself plus tree overhead (~7.7x file size for the txt) |
| Syntax tree / highlighting | code editor model | skipped above 5 MB | capped |
| Closed pane held on the undo-close stack | `app/src/code/view.rs` (`PHOSPHOR_CODE_PANE_KEEP_ON_CLOSE`), `detach` | one file's worth | fixed; heap trim on real drop (`PHOSPHOR_HEAP_TRIM_ON_CLOSE=0` to disable) |
| Undo history | buffer edit log | proportional to edits, not to file size on open | not a cost on open |
| Glibc arena retention after drop | `app/src/alloc.rs` | looked like a leak in r18bb | handled by close trim; since #2201 the trim is held until the UI is calm (below) |

## Current measurement (last run of `editor_memory_7mb`, r18u28, 2026-11-05)

Linux: open +5..+65 MB (txt), +55..+75 MB (rs); targets 100 MB txt, 150 MB rs, 60 MB held after close: met in every cycle.
App breakdown (txt): buffer tree 53.8 MB, render tree 1.3 MB + parked chunks 9.9 MB, 4 hydrated paragraphs.
macOS: +217 MB accepted by the owner (#1607). Windows: the +775 MB row was an OS-trimmed working-set baseline, not editor memory; the
private-bytes correction (`3e64e7114`) is merged and has not run.

## Candidate further changes (none taken)

1. Compact buffer items (136 B/item for ~30 B of text): saves maybe 30-40 MB at 7 MB; touches the shared sum-tree, high blast radius. Not worth it. Kill switch would be impossible (data structure).
2. Defer `PHOSPHOR_EDITOR_TRIM_AFTER_OPEN` until after the load frame: default-off feature, needs a new event; not done.
3. Windows working-set trim: no evidence it is wanted; private bytes are the honest metric.

## Tests that measure it

- `crates/editor/tests/editor_memory.rs`: counting global allocator, generated large file, asserts bytes held after load and after drop
  (`script/agent-cargo <name> nextest run -p warp_editor --test editor_memory --no-capture`).
- appci `editor_memory_7mb` (`script/realapp/appci/cases.py`): real-app RSS for a 7 MB txt and rs file, 3 open/close cycles, held-after-close.

## Verdict

Target met on Linux by the last measurement; remaining items are measurement faults or owner decisions. The case needs a fresh run
on the current build (the fix tier now includes it, `a767eb69c`) before the issue can be called verified.

## Close trim and the UI thread (#2201, branch `fix/r29-closetrim`)

Nothing compiled or run here. Worked, unproven until `editor_memory_7mb` passes its trim A/B.

### Finding

r29a Wayland (job 17990069590): "trim A/B: the close trim put a 9.4 ms UI item on screen while it ran (budget 4 ms; control
no_trim: 0.0 ms)". The v0.1.17 acceptance of this pause has expired.

### Cause, from that job's app log

- The trim is not on the UI thread. `GlobalBufferModel::watch_buffer_drop` (`app/src/code/global_buffer_model.rs`) started
  `alloc::run_close_trims` on the background executor at the buffer's drop event; the worker calls glibc `malloc_trim(0)`
  (`app/src/alloc.rs` `trim_free_memory`).
- The drop event is in the middle of the close. A relayout frame follows it at once, and that frame is the 9.4 ms item:

  | close | trim (worker) | frame after the drop | frame end to trim end |
  |---|---|---|---|
  | trim A/B, default variant | start ..592, 9.52 ms, 130 MB | `ui_item kind=frame ms=9.44 render_ms=8.90` | 0.07 ms |
  | first close of the case | 11.37 ms, 152 MB | 9.63 ms (`render_ms=9.03`) | 1.8 ms |
  | fifth close | 6.69 ms, 75 MB | 6.03 ms | 0.7 ms |
  | `PHOSPHOR_HEAP_TRIM=0` | none | 0.6 ms | - |

  The frame ends when the trim does. `malloc_trim` takes each arena's lock for its whole walk of that arena, and the walk's time
  is the kernel taking back the pages (65-150 MB per trim here; 6.7-12 ms). The app runs on two arenas (`M_ARENA_MAX=2`,
  `tune_glibc_malloc`), so the UI thread's arena is one of the two being walked, and its next allocation that misses the
  thread cache waits for the walk to end. This is an inference from the timings (no lock trace), and every cycle fits it.
- In the cycles where no frame was over budget the frame still ended within 0.3 ms of the trim (11.39 ms trim, frame drawn
  0.24 ms before its end): the wait fell outside a timed item. The pause was there in every close.

### Options

| option | verdict |
|---|---|
| Run the trim off the UI thread | Already the case; it is not the cause. |
| Split the trim (per arena, incremental, `malloc_trim(pad)`) | glibc has no such call: `malloc_trim` walks every arena, each under its lock for the whole walk; `pad` only changes how much of the top is kept. |
| Trim only the arena that held the buffer | No API for one arena, and with two arenas the UI thread's arena is the one that matters anyway. |
| Another allocator / more arenas | Allocator work is deferred until after the next numbered release; more arenas is the RSS growth #816 removed. |
| Hold the trim until the UI is calm | Taken. Same call, same memory handed back, later. |

### What changed

`alloc::DeferredCloseTrim`. The drop event records the request (`buffer_dropped ... trim=deferred`). The app's frame-drawn
callback (`app/src/lib.rs`, main thread, one atomic load when nothing is held) starts the trim right after a drawn frame once two
frames in a row each followed a quiet gap of at least 50 ms. Measured on the eight closes of the r29a log: the close's frames
come 3-15 ms apart in the first 17 ms, then the UI draws at the cursor's 500 ms blink with one stray frame 74-484 ms after the
close. One calm frame is not enough: three closes had a frame 9-17 ms behind the first calm one, and a 10 ms trim started after
it would have run into the second. With two, every one of the eight closes starts its trim with the next frame 497 ms away.

Give-up net: `CLOSE_TRIM_GIVE_UP`, 3 s after the latest large drop, starts the held trim when no calm pair came (hidden or
unfocused window draws no frames; streaming output has no quiet gap). Only the latest drop's net may fire. A second large close
while a trim is held joins it; the `TrimCoalescer` still folds a request into a running trim.

Switches: `PHOSPHOR_HEAP_TRIM_ON_CLOSE=0` and `PHOSPHOR_HEAP_TRIM=0` turn the trim off as before (`trim=off`, nothing is held,
no net is armed). `PHOSPHOR_HEAP_TRIM_ON_CLOSE_DEFER=0` restores trim-on-drop. All platforms take the same path (macOS
`malloc_zone_pressure_relief` and Windows `HeapCompact` take allocator locks too); only Linux has a measured pause.

### What it does not do

- A key pressed, or a frame drawn, in the ~10 ms the trim runs still waits for it. The pause is no longer certain on every large
  close; it is a chance one, of at most the trim's length (under the 16 ms frame).
- When the net fires during streaming output, one frame of that output can wait for the trim.
- The freed memory is handed back up to 3 s later than before (0.5-1.5 s in a focused window by the r29a frame times). The case
  waits 15 s for the trim line and settles RSS over 4 s quiet, so its held-after-close figures are taken after the trim as before.

### Proof wanted

`editor_memory_7mb` on every platform: trim A/B close-trim item under 4 ms; `<kind>_app_heap_trim_freed_mb` and held-after-close
unchanged; `close_trim_start started_by=...` in the log says which of the two started each trim.
