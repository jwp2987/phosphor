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
| Glibc arena retention after drop | `app/src/alloc.rs` | looked like a leak in r18bb | handled by close trim |

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
