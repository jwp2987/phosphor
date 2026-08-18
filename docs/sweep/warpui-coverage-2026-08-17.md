# `warpui_core` / `warpui` / `warpui_extras` — first sweep against the pin

**Date:** 2026-08-17 · **Oracle pin:** `42effe840` (2026.08.12 stable) · **Method:** static
comparison only. Nothing was built, run, fetched, or edited outside this file.

## Why this document exists

`docs/FLEET-ROUND.md`'s shard table names no `warpui_core` / `warpui` /
`warpui_extras` row, and neither did the 2026-08-15 audit's (unwritten) path set.
These three crates — 472 files, 40% of the `crates/` tree by file count — had
**never been compared against any pin**. `TODO.md` (~line 2009) records the hole.

This is the first pass. It produces **counts, test verdicts and evidence**.
Nothing was ported.

### Two things settled before this sweep started, re-confirmed in one command each

- `git diff 42effe840 -- crates/warpui_core/src/runtime/renderer.rs` emits **0
  lines**, and `crates/warpui_core/src/runtime/renderer_tests.rs:201` is
  `fn wide_grapheme_does_not_shift_following_cells()`. **#13591 is fixed here.**
  Stop citing it.
- The prior pass's "**356 of 479 files diverge**" is **not a divergence count**.
  It is `git diff --name-only 42effe840 -- <the three crates> | wc -l`, i.e. the
  number of changed *path entries* after rename collapsing — adds, deletes and
  modifications pooled together, against a denominator (479) that counts files.
  The raw status split is `84 A / 91 D / 262 M` = 437 entries, which rename
  detection collapses to 356. The corrected numbers are below.

---

## 1. File-level divergence census

**The headline number is dominated by two systematic path skews, not by content.**
Upstream keeps GUI elements under `crates/warpui_core/src/elements/gui/` and names
test files `*_tests.rs`; this fork flattened `elements/gui/*` into `elements/*`
(merging the pin's 10-line `elements.rs` and 799-line `elements/gui/mod.rs` into
one 827-line `elements/mod.rs`) and renamed 26 test files to `*_test.rs`. That is
**82 rename pairs**, which a naive comparison reports as 82 deletions plus 82
additions. They are counted as renames here.

| | `warpui_core` | `warpui` | `warpui_extras` | **total** |
|---|---:|---:|---:|---:|
| files here | 255 | 198 | 19 | **472** |
| files at pin | 262 | 198 | 19 | **479** |
| matched (same name) | 176 | 195 | 17 | 388 |
| matched (rename pair) | 78 | 2 | 2 | 82 |
| **byte-identical** | 79 | 51 | 7 | **137** |
| **differ in content** | 175 | 146 | 12 | **333** |
| **fork-only** | 1 | 1 | 0 | **2** |
| **pin-only (absent here)** | 7 | 1 | 0 | **8** |

*(`warpui_core`'s matched-pin-path count is 255, one more than its 254 matched
fork paths, because the pin's `elements.rs` and `elements/gui/mod.rs` both map
onto the single fork `elements/mod.rs`.)*

So: **333 of 479 pin files (69.5%) differ; 137 are byte-identical; 8 are absent
entirely; 2 are fork additions.**

### How much of the 333 is real

Exact, not sampled — each file compared against its pin counterpart with
whitespace stripped, then again with `use` statements removed:

| class | files |
|---|---:|
| identical once whitespace is stripped | **7** |
| identical once whitespace **and imports** are stripped | **75** |
| differ in actual code | **251** |

Of the 251, ranked by changed code lines (whitespace- and import-insensitive,
rename-aware): **104 change ≤10 lines**, 92 change 11–40, 35 change 41–100, and
only **20 exceed 100**. The two largest are structural rather than behavioural:
`crates/warpui_core/src/core/app.rs` (1,374) and the `elements/mod.rs` merge (786).

A further large slice of the 251 is **Rust-2024 idiom skew**, not behaviour: the
pin uses let-chains, the fork uses desugared nested `if let` / `match`. Measured
by `&& let` occurrences — `warpui_core` fork 8 vs pin 62, `warpui` fork 0 vs pin
18. This is **tree-wide, not specific to these crates** (`app/src` is 49 vs 668),
and `script/precheck:171` already records it: *"The repo has never been
rustfmt-clean (~1.6k files drift at either edition)"*, with the rustfmt gate
scoped to changed files only. It inflates every area's diff equally and is not a
finding.

### The 2 fork-only files

Both are vendored-attribution text, absent anywhere at the pin — fork-original
licence-compliance work, not debt:

- `crates/warpui_core/src/platform/LICENSE-WINIT` (Apache-2.0)
- `crates/warpui/src/rendering/LICENSE-CHROMIUM` (BSD-3-Clause)

### The 8 pin-only files

| pin path | what it is | verdict |
|---|---|---|
| `crates/warpui_core/src/telemetry/mod.rs` | `record_event` / `record_telemetry_from_ctx!` — the telemetry upload channel | DECLINED |
| `crates/warpui_core/src/telemetry/event_store.rs` | `EventStore`, session bucketing for the above | DECLINED |
| `crates/warpui_core/src/telemetry/event_store_tests.rs` | its 4 tests | DECLINED |
| `crates/warpui_core/src/app_focus_telemetry.rs` | "Daily App Focus Duration" event, keyed on `user_id`/`anonymous_id` | DECLINED |
| `crates/warpui_core/src/app_focus_telemetry_tests.rs` | its 1 test | DECLINED |
| `crates/warpui_core/src/elements/gui/new_scrollable/util_tests.rs` | 2 scroll-delta tests | COVERED-ELSEWHERE — the fork keeps both inline in `crates/warpui_core/src/elements/new_scrollable/util.rs:168,218` |
| `crates/warpui_core/examples/tui_file_viewer.rs` | an example binary | not a parity surface; noted, not debt |
| `crates/warpui/tests/headless_main_thread.rs` | a whole integration-test target | **real debt — see §3** |

---

## 2. Test-name comparison

Measured exactly as `script/state:65-70` measures it — unique `fn` names under
`#[test]` / `#[tokio::test]` / `#[async_std::test]`, pin names taken from these
three crates, fork names taken **tree-wide** so a relocated test counts as
present.

| | count |
|---|---:|
| pin tests in these three crates | **696** |
| fork tests in these three crates | **734** |
| pin tests absent from the whole fork | **7** |
| of those, present elsewhere in the fork but not in these crates | **0** |

That last row matters: there are no cross-crate name-collision false positives
here, the failure mode `DECLINED.md:94` warns about in the Grok row.

**The fork has 38 more tests in these crates than the pin does** — 45
fork-original tests against 7 absent. The unswept area is not undertested; it is
unaudited. Several of the 45 land squarely in the risk categories this sweep was
asked to watch:

- damage tracking — `present_reuses_the_cached_root_when_invalidate_reports_no_changes`,
  `present_reuses_the_view_invalidate_already_rendered`,
  `present_falls_back_to_a_direct_render_when_invalidate_was_never_called`,
  `invalidate_drops_removed_views_from_rendered_views`
- text shaping — `test_text_frame_line_y_offsets_accumulate_previous_line_heights`,
  `test_layout_text_chinese_soft_wrap_caret_indices`,
  `preferred_cjk_families_respects_cjk_locale`,
  `preferred_cjk_families_defaults_to_simplified_chinese_for_non_cjk_locale`
- input dispatch — the six `text_fallback_*` tests
- terminal probing — `da1_sentinel_outranks_the_background_reply` and three siblings
- fork-original shimmer maths — 12 `shimmer_*` / `intensity_at` / `value_at` tests

### The 7 absent tests, in full

| test | pin file |
|---|---|
| `safe_browser_open_url_accepts_warp_channel_urls` | `crates/warpui/src/browser_tests.rs` |
| `terminal_screen_lifecycle_reconfigures_modifier_reporting` | `crates/warpui_core/src/runtime/mod_tests.rs` |
| `test_app_active_after_activity` | `crates/warpui_core/src/telemetry/event_store_tests.rs` |
| `test_app_active_after_inactivity` | `crates/warpui_core/src/telemetry/event_store_tests.rs` |
| `test_event_queue_empty` | `crates/warpui_core/src/telemetry/event_store_tests.rs` |
| `test_initialize_session` | `crates/warpui_core/src/telemetry/event_store_tests.rs` |
| `test_daily_app_focus_duration_increase` | `crates/warpui_core/src/app_focus_telemetry_tests.rs` |

---

## 3. Triage

Ledger vocabulary, per `docs/sweep-verdict-ledger.tsv`.

| verdict | tests |
|---|---:|
| `DECLINED` | 6 |
| `COVERED-ELSEWHERE` | 1 |
| `CLOUD` | 0 |
| `DIVERGENT` | 0 |
| `MISSING-SUBSYSTEM` | 0 |
| `PORTABLE` | 0 |

**Zero portable test debt from the name-based measure.** One item of real debt
was found outside it (§3.4).

### 3.1 `COVERED-ELSEWHERE` — `safe_browser_open_url_accepts_warp_channel_urls`

The fork has the same test under a de-branded name,
`safe_browser_open_url_accepts_app_channel_urls`
(`crates/warpui/src/browser_tests.rs:20`), asserting over a strict **superset**
of the pin's scheme list — all six `warp*` schemes plus the fork's own `zap`.
`crates/warpui/src/browser.rs:30-31` allowlists all seven. Rename only; zero
coverage lost.

### 3.2 `DECLINED` — the 5 telemetry tests

`DECLINED.md:98-105`, "Telemetry and crash reporting — deliberately asymmetric":
*"Channel physically removed."* `crates/warpui_core/src/telemetry/` does not
exist in the fork. The pin's `telemetry/mod.rs` is a `record_event` façade over a
process-global `EventStore`, and `app_focus_telemetry.rs:17-27` calls
`crate::telemetry::record_event(user_id, anonymous_id, "Daily App Focus Duration
(seconds)", …)` — accounts plus an upload channel, neither of which exists here.
There is nothing to port these against. `declined_ref` **#165**. (`CLOUD` would
also be defensible; `DECLINED` is used because `DECLINED.md` carries an explicit
row.)

### 3.3 `DECLINED` — `terminal_screen_lifecycle_reconfigures_modifier_reporting`

This one is worth stating carefully, because it is input-dispatch code and reads
like debt.

The test drives `set_terminal_keyboard_enhancement_flags`, which the fork does
not have. At the pin
(`crates/warpui_core/src/runtime/mod.rs:850`) its **only** production caller is
`TuiTerminalGuard::set_modifier_key_lifecycle_enabled` (`:508-525`), reached only
via `TuiDriverHandle::set_modifier_key_lifecycle_enabled` (`:566-572`) →
`crates/warp_tui/src/session_registry.rs:612` → `crates/warp_tui/src/session.rs:300`.
That last hop is inside `#[cfg(feature = "voice_input")]`, subscribed to
`TuiVoiceSettingsChangedEvent::TuiVoiceInputHoldKeySetting` — the push-to-talk
hold key. `crates/warp_tui/Cargo.toml` at the pin declares
`voice_input = ["warp/voice_input"]`; **the fork's `crates/warp_tui/Cargo.toml`
declares no such feature.** Voice transcription is declined
(`DECLINED.md:118-123`, #389/#352/#11).

What is lost is only the *live reconfiguration* path — the absolute-set escape
`\x1b[={n};1u` written mid-session. The **enter-time** path (`\x1b[>3u` /
`\x1b[>15u`) is intact in the fork along with its tests
(`terminal_screen_lifecycle_can_skip_all_key_reporting`,
`terminal_screen_lifecycle_uses_baseline_keyboard_enhancement_when_unconfirmed`).
`declined_ref` **#389**. Revisit only if a hold-key setting ever returns.

**Residue this left, worth a `TODO.md` row rather than a ledger row:**
`TuiDriverHandle::modifier_key_lifecycle_enabled()` at
`crates/warpui_core/src/runtime/mod.rs:571` has **zero callers anywhere in the
fork** — a getter left behind when its setter was dropped. Its field
(`:500`, `:515`) is still initialised at `enter()`, so it is correct, just dead.

### 3.4 The one piece of real debt — and the measure that cannot see it

`crates/warpui/tests/headless_main_thread.rs` exists at the pin and **not here**.
It is an integration-test target with a hand-rolled `libtest_mimic` harness that
pins `test_threads = Some(1)` so the case runs on the process main thread, and
registers `Trial::test("services_main_dispatch_queue", …)` — a macOS run-loop /
`dispatch2::run_on_main` regression test built on
`AppBuilder::new_headless`.

**It contains no `#[test]` attribute at all.** `script/state`'s measure — and
therefore every test-name comparison in `docs/sweep-verdict-ledger.tsv`, this one
included — is structurally blind to it. It did not show up in the 7 because no
name-based method can see it. It is the only genuinely portable gap this sweep
found, and it was found by the *file* census, not the test census.

Practical caveat before anyone ports it: the whole body is
`#[cfg(target_os = "macos")]`, so on Linux CI the trial list is empty and the
target passes vacuously. Value is real but macOS-only.

---

### 3.5 Commit-level coverage — and what replaces the "~1,240 commits" figure

`TODO.md` claims *"~1,240 upstream commits touch `warpui_core` and `warpui` and
have never been swept at all."* **That figure is not measurable here and the
second half is wrong.** The clone is shallow, grafted at `02b53fcd8`; only 232
upstream commits exist locally. Any claim about commits beyond that horizon is
unverifiable in this tree and should not be repeated.

Inside the horizon the number *is* checkable. Of the **231** commits in
`02b53fcd8..42effe840` — the last re-pin step — exactly **22** touch these three
crates (9.5%). Of those 22:

- **19 were ported** (fork commits reference their upstream PR numbers).
- **1 is declined** — `82f3dce2b` "[APP-4988] Add configurable TUI push-to-talk
  key (#14307)", the voice hold-key work behind §3.3.
- **2 are a land-and-revert pair that nets to nothing**: `53411ef0a` (#14336,
  live-bg theme refresh) was reverted by `132db5c54` (#14651, "Prevent TUI
  terminal probe replies from leaking"). Their diffstats are exact negatives and
  `crates/warp_tui/src/terminal_background_tests.rs` exists at neither pin. The
  fork correctly has neither.

That matches `docs/FLEET-ROUND.md`'s shard D ("UI crates, 27 commits, 18 ports").
**So these crates were swept commit-by-commit by the re-pin round.** What was
never done is the *audit* — the test-parity and behavioural comparison every
other area got. That is the hole, and it is narrower than `TODO.md` implies.

---

## 4. Divergences ranked by risk

Test parity is effectively complete (§2), so the risk in these crates is not
missing tests — it is **behavioural drift in files both sides have**. Everything
below was verified directly against the pin. "Pin ahead" means the fork is
missing something upstream has; that is where the defects are.

### Context you need to read the ranking

The fork is on **wgpu/naga 29.0.3**, the pin on **30.0.0** (`Cargo.lock` on both
sides). Several GPU-layer diffs — `queue.present(t)` vs `t.present()`,
`&[Some(desc())]` vs `&[desc()]`, the missing `Error::BufferMap` variant — are
that version skew, not hand-written divergence. Do not file them.

### Tier 1 — file an issue

**R1. Both WGSL shaders lost `@interpolate(flat)` on their integer varying — PIN AHEAD**
`crates/warpui/src/rendering/wgpu/shaders/glyph_shader.wgsl:54` (`is_emoji: i32`)
and `crates/warpui/src/rendering/wgpu/shaders/image_shader.wgsl:32`
(`is_icon: u32`). The pin has `@interpolate(flat)` on both; the fork has **no
`interpolate` attribute anywhere in any shader**. `image_shader.wgsl`'s *entire*
diff against the pin is that one deleted attribute.

`is_emoji` selects the whole emoji-vs-text colour path
(`glyph_shader.wgsl:112,118`); `is_icon` gates the icon branch
(`image_shader.wgsl:74`). WGSL requires integral inter-stage IO to be flat —
interpolating an `i32`/`u32` across a triangle is not meaningful.

**Honest limit:** whether naga 29.0.3 rejects this at pipeline creation or
accepts it and mis-shades cannot be settled without building, which is forbidden
on this host. Those two outcomes are "the GPU renderer does not start" and
"emoji/icons shade wrong along a triangle" — both worth an issue, and the issue
can resolve which by compiling. Rust-side types are unchanged
(`renderer/glyph.rs:307,334` still writes `is_emoji: i32`), so there is no
compensating change.

**R2. `notify_model_observers` cannot reach TUI views — FORK-ORIGINAL DEFECT**
`crates/warpui_core/src/core/app.rs:4164-4177`. The fork split view storage into
`Window::views` + `Window::tui_views` (`core/window.rs:87-96`) where the pin has
one unified `StoredView` registry. Five analogous sites got a `tui_views`
fallback — `:1486-1498`, `:1609-1625`, `:1954-1969`, `:4081-4099`,
`:4273-4283`, several with comments spelling out why it is needed ("Without this
… an editor's `ContentChanged` never re-renders the view"). The
`Observation::FromView` arm of `notify_model_observers` did **not**. It does
`w.views.remove(view_id)` → `None` → `alive = false`, so the observation is
dropped permanently.

Effect: a TUI view observing a model stops re-rendering and never resumes. This
is a stale-frame defect the pin's unified registry cannot have. Highest-confidence
finding in the sweep — the omission is one site out of six, and the other five
document the requirement.

**R3. Start-clipped text draws its ellipsis on top of the last glyph — PIN AHEAD**
`crates/warpui_core/src/text_layout.rs:1546` and `:1615`. The pin hoists
`let start_ellipsis_offset = if is_start_clipping && ellipsis_width > 0. { ellipsis_width } else { 0. };`
with the comment *"so they stay flush with the right edge and do not overlap the
ellipsis"*, and adds it to every glyph x in both the measurement simulation and
the real paint. The fork deleted the binding and both uses. Everything else —
`remaining_width` initialised to `available_width - ellipsis_width`, the ellipsis
drawn at `x = remaining_width` — is byte-identical, so nothing compensates.

Effect: right-anchored truncation ("…tail of the string") paints the "…" over the
leftmost visible glyph and shifts the run left by one ellipsis width.

**R4. `Event::in_bounds` routes forward/back mouse buttons to every element — PIN AHEAD**
`crates/warpui_core/src/event.rs:260-276`. The pin's match is **exhaustive with
no wildcard**, listing `ForwardMouseDown` and `BackMouseDown` in the
position-testing group. The fork drops both from the group and adds `_ => true`,
so they report in-bounds for every element regardless of position.

Latent today — the only consumer
(`app/src/terminal/alt_screen/alt_screen_element.rs:847`) doesn't match those
variants. The durable cost is the lost exhaustiveness: the pin would fail to
compile when a new positional event is added; the fork will silently route it
everywhere.

### Tier 2 — worth an issue, narrower blast radius

**R5. macOS Metal presents synchronously on every frame — PIN AHEAD**
`crates/warpui/src/platform/mac/rendering/metal/renderer.rs:84`. The pin reads
`metal_layer.presentsWithTransaction()` (`:1071`), threads it into
`finish_with_capture` (`:78`), and when `!should_capture && !presents_with_transaction`
takes `presentDrawable(); commit(); return None` — never blocking the main
thread. The fork deleted the parameter and the branch: every frame is
`commit(); waitUntilCompleted(); present()`. Main-thread stall on GPU completion
per frame, plus a manual `present()` on a layer not configured for transactional
presentation. macOS-only.

**R6. Headless macOS event loop has no CFRunLoop pump — PIN AHEAD**
`crates/warpui/src/platform/headless/event_loop.rs:42`. The pin wraps the channel
in `EventSender`/`EventReceiver` backed by a `CFRunLoopSource` (`:35-81`) and on
macOS drives `CFRunLoop::run()` with a `try_recv` drain. The fork is a bare
`for event in receiver.iter()` on all platforms, so the main-thread run loop is
never serviced and anything dispatching through CFRunLoop / the main dispatch
queue never fires.

**This is the same gap as §3.4.** The pin's test for exactly this behaviour is
`crates/warpui/tests/headless_main_thread.rs`'s `services_main_dispatch_queue`,
and that file is one of the 8 absent. The fork is missing the mechanism *and* the
test that would have caught it. File them together.

**R7. `view_parents` is never pruned when a window closes — PIN AHEAD**
`crates/warpui_core/src/core/app.rs`. The pin removes it in the close-window path
alongside `window_invalidations` (`42effe840:crates/warpui_core/src/core/app.rs:2759`).
The fork has **no `view_parents` removal anywhere** — verified by grep. Entries
leak for the process lifetime, and because `reopen_closed_window` restores
`view_to_window`, a reopened window can inherit a stale parent graph and get the
wrong responder chain.

**R8. `Presenter`'s ancestor/descendant walkers have no cycle guard — PIN AHEAD**
`crates/warpui_core/src/presenter.rs:470-478` (`ancestors`) and `:482-497`
(`descendants`). The pin's `Presenter` has no such walkers at all — it only
*writes* the parent map (`42effe840:crates/warpui_core/src/presenter.rs:563`);
both walks live in `AppContext` and are guarded (`view_ancestors` with
`chain.contains(parent_id)` at pin `app.rs:1501-1507`, and a
`steps > parents.len()` bound at `:1539`).

The fork kept **both guarded copies** (`core/app.rs:2029-2038`,
`:3390-3406`) and added **two unguarded ones** on the GUI path. `ancestors` is on
the hot keystroke/responder-chain path; a cyclic parent link hangs the UI thread
instead of logging and continuing.

**R9. TUI focus-gain probe can eat keystrokes — FORK-ORIGINAL, needs a bound**
`crates/warpui_core/src/runtime/mod.rs:778-838` (`run_tui_input_reader`) with
`crates/warpui_core/src/runtime/terminal_probe.rs:440-497`. Fork-original (the
pin has no reader-thread probe). On every `FocusGained` under `TuiTheme::Auto` on
a tty, the reader thread stops pumping crossterm and does a raw
`libc::read(STDIN_FILENO, …)` loop for up to `LIVE_PROBE_DEADLINE` (50 ms),
keeping only the OSC 11 colour reply and discarding the rest. The
`!event::poll(Duration::ZERO)` guard at `:806` only proves the queue was empty
*before* the probe — anything typed *during* the window is swallowed, and a
half-consumed escape sequence can corrupt the following key.

This is the fork's own answer to the same problem upstream's reverted #14336/#14651
pair was chasing, and the fork has four fork-original tests covering the reply
parsing. What is untested is the discard path. Real, but fork-original design, so
it is a defect to file rather than a parity gap.

### Tier 3 — record, do not act

- **R10. Key-fallback trigger set widened** —
  `crates/warpui/src/windowing/winit/event_loop/key_events.rs:156-201`, call site
  `event_loop/mod.rs:1330-1345`. Fork ahead in intent (it adds a
  `ctrl||alt||super → None` guard the pin lacks and maps `\r\n\t\u{8}\u{7f}\u{1b}`
  to synthetic `KeyDown`), but it now fires whenever
  `convert_keyboard_input_event` returns `None` for *any* reason — including
  dead-key/compose intermediates via `convert_key`'s `_ => return None`
  (`key_events.rs:316`). The pin fired only on `is_unidentified_key && !is_synthetic && Pressed`.
  Covered by the six fork-original `text_fallback_*` tests. Worth a second pair of
  eyes on the dead-key path, not an issue on its own.
- **R11. IME preedit dedup drops identical `SetMarkedText`** —
  `event_loop/mod.rs:1578-1584`; and `update_ime_position()` no longer gated on
  `self.ime_enabled` (`:766-785`, deliberate, for Win11 Microsoft Pinyin). Both
  fork-ahead fixes for real bugs, chosen differently from the pin. Interaction
  between them is the thing to watch on Wayland/KDE.
- **R12. Fork-ahead rendering fixes worth keeping in mind when re-pinning** — the
  fork will keep conflicting with upstream here:
  `crates/warpui/src/windowing/winit/fonts.rs:523-562` (locale-aware `FontSystem`;
  the pin hardcodes `"en"`, which mis-picks Han forms),
  `crates/warpui_core/src/fonts.rs:485-491` (`glyph_for_char` no longer returns a
  stale cached `None`, which on the pin makes a char tofu permanently),
  `crates/warpui/src/windowing/winit/fonts/text_layout.rs:84` (paragraph byte
  offset threaded into `RunBuilder`; the pin mis-maps `Glyph::index` for every
  paragraph after the first), `crates/warpui_core/src/text_layout.rs:876`
  (`line_y_offsets`), `crates/warpui/src/rendering/wgpu/renderer.rs:195-212`
  (COPY_SRC capture guard), `crates/warpui/src/windowing/winit/window.rs:1605-1616`
  (Windows backdrop re-apply).
- **R13. Fork-introduced policy risk** —
  `crates/warpui/src/windowing/winit/fonts/windows.rs:405` makes every non-CJK
  locale prepend **Simplified Chinese** families for shared Han. Under `en-US`,
  Japanese and Traditional-Chinese text renders with Simplified forms — the same
  Han-unification error R12's locale work exists to fix, relocated to a different
  locale pair. Deliberate per the fork's comment, but it is a rendering-visible
  policy choice that should be written down.
- **R14. Hash-map downgrades** — `EntityIdMap`/`EntityIdSet` (FxHash) →
  `std::HashMap`/`HashSet` in `presenter.rs:36-37`, `core/app.rs:595,630-640`,
  `keymap/matcher.rs:14,54`. `EntityIdMap` still exists
  (`core/entity.rs:32`). Perf only, but `RandomState` also makes
  `Presenter::descendants` iteration order vary between runs, so notification
  ordering is no longer reproducible. Cheap to revert.
- **R15. Dead accessor** — `TuiDriverHandle::modifier_key_lifecycle_enabled()`
  (`crates/warpui_core/src/runtime/mod.rs:571`) has zero callers; residue of §3.3.
- **R16. Minor pin-ahead declines** — `should_terminate_app` lost its
  `TerminationRequestSource::User` argument (`event_loop/mod.rs:1544`,
  `headless/event_loop.rs:46-51`); nested-`Draggable` arbitration
  (`descendant_draggable_initiated`) removed, pin `presenter.rs:242,491,732-738`;
  enter/leave escape ordering swapped (`runtime/mod.rs:927-935,963-971`,
  byte-order only).

### Explicitly checked and clean

`crates/warpui_core/src/runtime/renderer.rs` (byte-identical),
`crates/warpui_core/src/runtime/event_conversion.rs` (zero diff),
`crates/warpui_core/src/presenter/tui.rs` (byte-identical),
`crates/warpui/src/platform/mac/text_layout.rs` and `platform/mac/event.rs` (no
semantic change — the macOS cluster→glyph mapping, ligature indices and advances
are untouched), `crates/warpui/src/rendering/glyph_cache.rs` (atlas packing,
subpixel keys, raster bounds identical), `crates/warpui_core/src/image_cache.rs`
(cache key, `should_cache_rendered_image`, eviction identical),
`crates/warpui_core/src/keymap/matcher.rs` (chord precedence and pending-clear
unchanged), and — checked line by line by the damage-tracking pass —
`build_scene`'s 3-iteration invalidation-coalescing loop, `update_windows` and all
six call sites, the repaint coalescing predicate, and `Presenter::build_scene` /
`paint` / `PaintContext` ordering. No changed numeric constant was found in any
shaping, layout or atlas path.

---

## 5. How much of the hole this closes

**Closed.**

- The file census is done and exact for all three crates (§1). The "356 of 479"
  figure is explained and replaced.
- The test census is done by `script/state`'s own method (§2), and the answer is
  unambiguous: **7 absent, 6 declined, 1 covered-elsewhere, 0 portable.** The
  crates carry **38 more tests than the pin**.
- Commit-level coverage inside the verifiable horizon is measured (§3.5): 22 of
  231, 19 ported, 1 declined, 2 a revert pair. The crates were swept by the
  re-pin round; only the audit was missing.
- Sixteen behavioural divergences are triaged, each verified against the pin,
  with four in Tier 1.

**Not closed.**

1. **Coverage of the risk ranking is deliberately partial.** §4 examined ~70
   files chosen for the four risk categories in the brief. **251 files differ in
   code** (§1); the remaining ~180 — element layout (`flex`, `stack`, `table`,
   `new_scrollable`, `uniform_list`, `viewported_list`), `ui_components`,
   `integration/`, `clipboard_utils`, `warpui_extras` — were counted but not
   read. `crates/warpui_core/src/clipboard_utils.rs` (215 changed code lines) and
   `crates/warpui/src/windowing/winit/delegate.rs` (113) are the largest unread
   files.
2. **`core/app.rs` was sampled, not exhausted.** ~1,374 changed code lines,
   assessed as roughly 60-65% noise / 30% behaviour-preserving architectural
   (the `views`+`tui_views` split, `view_parents` relocation) / ~5% semantic
   residue. R2, R7 and R8 came out of that 5%. A file that large deserves its own
   pass — R2 is proof that a single missed site in it is a live defect.
3. **Two Tier-1 findings cannot be resolved without building**, which this host
   forbids. R1 needs a compile to tell "renderer fails to start" from "emoji
   shade wrong". R3 and R5 want a visual check.
4. **Provenance of the oldest divergences is unrecoverable here.** `git log -S`
   for both R1 and R3 bottoms out at `02b53fcd8`, the graft boundary — they
   predate the 231-commit window and cannot be attributed to a commit in this
   clone. They are consistent with never having been swept.
5. **`SCOPE-*.md` still has no verdicts for these crates.** Those files cover 854
   test-bearing files measured at the *old* pin `02b53fcd8`; these three crates
   were never classified there either.
6. **The `_test.rs` / `_tests.rs` and `elements/` / `elements/gui/` skews are
   permanent re-pin friction.** 82 files whose paths will not match upstream at
   every future pin move. Worth a decision — adopt upstream's layout, or record
   the divergence in `DECLINED.md` so no future sweep re-derives it as debt.

### What a full sweep still needs

- Read the ~180 unexamined code-diverging files, element layout first.
- A dedicated `core/app.rs` pass, auditing every `views` access for a missing
  `tui_views` fallback (R2's failure mode).
- Build-backed resolution of R1, R3, R5.
- `SCOPE-*.md` verdicts for these crates at the current pin.
- A `DECLINED.md` decision on the path/naming skew.

---

## Proposed ledger rows

**Not written to `docs/sweep-verdict-ledger.tsv`** — proposed only. Suggested new
`area` value `warpui`, alongside the existing `app-ai` / `warp-tui` / etc.
Columns: `test  pin_file  area  verdict  evidence  declined_ref  pin_commit  sweep_date  confidence  source_doc`.

| test | pin_file | verdict | declined_ref | evidence (abridged) |
|---|---|---|---|---|
| `safe_browser_open_url_accepts_warp_channel_urls` | `crates/warpui/src/browser_tests.rs` | `COVERED-ELSEWHERE` | — | Renamed to `safe_browser_open_url_accepts_app_channel_urls`, `crates/warpui/src/browser_tests.rs:20`, asserting a strict superset (all 6 `warp*` schemes + `zap`); allowlist at `browser.rs:30-31`. |
| `terminal_screen_lifecycle_reconfigures_modifier_reporting` | `crates/warpui_core/src/runtime/mod_tests.rs` | `DECLINED` | #389 | Only pin caller chain is `runtime/mod.rs:850` ← `:508-525` ← `:566-572` ← `warp_tui/session_registry.rs:612` ← `warp_tui/session.rs:300`, which is `#[cfg(feature="voice_input")]` on the push-to-talk hold key. Fork's `crates/warp_tui/Cargo.toml` declares no `voice_input` feature. Enter-time path and its two tests retained. |
| `test_app_active_after_activity` | `crates/warpui_core/src/telemetry/event_store_tests.rs` | `DECLINED` | #165 | `crates/warpui_core/src/telemetry/` absent; `DECLINED.md:98-105` "Channel physically removed." |
| `test_app_active_after_inactivity` | `crates/warpui_core/src/telemetry/event_store_tests.rs` | `DECLINED` | #165 | as above |
| `test_event_queue_empty` | `crates/warpui_core/src/telemetry/event_store_tests.rs` | `DECLINED` | #165 | as above |
| `test_initialize_session` | `crates/warpui_core/src/telemetry/event_store_tests.rs` | `DECLINED` | #165 | as above |
| `test_daily_app_focus_duration_increase` | `crates/warpui_core/src/app_focus_telemetry_tests.rs` | `DECLINED` | #165 | `app_focus_telemetry.rs:17-27` calls `crate::telemetry::record_event(user_id, anonymous_id, …)`; neither the channel nor accounts exist here. |
| `services_main_dispatch_queue` | `crates/warpui/tests/headless_main_thread.rs` | `PORTABLE` | — | **Not a `#[test]`** — a `libtest_mimic` `Trial` in a custom harness pinning `test_threads = Some(1)`, so `script/state`'s measure cannot see it. Whole target absent here. Tests the CFRunLoop pump the fork also lacks (R6). `#[cfg(target_os = "macos")]`, so inert on Linux CI. |

`pin_commit` `42effe840`, `sweep_date` `2026-08-17`, `confidence` `clean`,
`source_doc` `docs/sweep/warpui-coverage-2026-08-17.md` for all eight.

## Proposed `TODO.md` amendments

**Not written to `TODO.md`** — proposed only.

1. **Amend the ~line 2009 entry.** The coverage hole is real but narrower than
   stated. Replace *"~1,240 upstream commits touch `warpui_core` and `warpui` and
   have never been swept at all"* with: *the crates were swept commit-wise by the
   re-pin round (19 of 22 in-window commits ported, 1 declined, 2 a revert pair);
   what was never done is the test-parity and behavioural audit. The "~1,240"
   figure is unverifiable in this shallow clone and should not be repeated.*
   Mark the test-parity half **closed** by this document, and the behavioural
   half **partially closed** (~70 of 251 code-diverging files read).
2. **New issues** for R1, R2, R3, R4 (Tier 1) and R5, R6, R7, R8, R9 (Tier 2).
   R6 should carry the `services_main_dispatch_queue` port with it.
3. **New row** for R15, the dead `modifier_key_lifecycle_enabled` accessor.
4. **A `DECLINED.md` decision** on the `_test.rs` / `elements/` path skew (82
   files), so future sweeps stop re-deriving it.
