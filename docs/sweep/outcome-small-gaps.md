# Outcome: three independent small gaps

Task package: three unrelated items queued together — (1) `language_by_filename`
takes the wrong type, (2) `TuiSelectable::with_semantic_selection_by_style` is
missing, (3) the AI-assistant panel's minimum width is too large (#324).

Base branch: `working`, reset to `8faf6002a`. Build freeze in effect (no
`cargo`/`rustc`/`nextest`/`script/precheck` in any form) — every claim below is
source comparison against the pin (`02b53fcd8`) plus `rustfmt --check`, not
compilation or test execution. **All three items: written, unverified.**

## 1. `language_by_filename` takes the wrong type

Premise confirmed real: `crates/languages/src/lib.rs:139` took `&Path`, not
`&StandardizedPath`. Confirmed by two independent signals — the pin's actual
signature, and a fork-authored comment already in
`crates/ai/src/index/full_source_code_embedding/chunker.rs` documenting the
exact drift and the failure mode it removed.

**What was ported**, matching the pin's `crates/languages/src/lib.rs` exactly:

- `pub fn language_by_filename(path: &StandardizedPath)`, delegating to a new
  private `language_by_filename_parts(filename: Option<&str>, extension:
  Option<&str>)`.
- `pub fn language_by_local_filename(path: &Path)` — the pin's sibling for
  callers that hold a plain local-filesystem path — delegating to the same
  `_parts` function.
- The fork's full 48-language extension/filename match arms were preserved
  verbatim inside `_parts` (the pin only supports a subset; the fork's extra
  languages — dart, zig, scss, r, julia, ocaml, erlang, nix, groovy, solidity,
  graphql, proto, clojure, elm, cmake — are a fork addition, not something to
  drop).
- Added `warp_util.workspace = true` to `crates/languages/Cargo.toml` (needed
  for `StandardizedPath`).

**Call sites** (all direct callers of the free function, found by grepping for
the definition, not the bare name):

| site | before | after | reasoning |
|---|---|---|---|
| `crates/ai/src/index/full_source_code_embedding/chunker.rs` | called `language_by_filename(path: &Path)` directly (the drift) | restored the pin's exact `StandardizedPath::try_from_local(path).ok()?` guard, falling back to naive chunking on failure | this is the one call site with a documented, real behavioral gap — the fork accepted non-absolute/non-UTF8 paths the pin would reject; now matches pin exactly |
| `app/src/terminal/view/inline_banner/open_in_warp.rs` | `language_by_filename(&openable_path.path)` (`path: PathBuf`) | `language_by_local_filename(...)` | `OpenablePath` is built from local terminal-output path resolution; no `StandardizedPath` in scope |
| `app/src/code/editor/model.rs` (`CodeEditorModel::set_language_with_path`) | `language_by_filename(path: &Path)` | `language_by_local_filename(path)` | kept the wrapper method's own `&Path` signature unchanged (see "not done" below); only its internal call was fixed |
| `app/src/workspace/view/vertical_tabs.rs` (`code_detail_kind_label`) | `language_by_filename(Path::new(file_name))` | `language_by_local_filename(...)` | takes a bare `&str` filename, no path type available |
| `app/src/util/openable_file_type.rs` (`is_supported_code_file`) | `language_by_filename(path: impl AsRef<Path>)` | `language_by_local_filename(...)` | signature is `AsRef<Path>`; no `StandardizedPath` in scope |
| `crates/ai/src/index/file_outline/native.rs` (`parse_file_outline`) | `language_by_filename(path: &Path)`, called with `metadata.path.to_local_path_lossy()` (the original `metadata.path` field is `repo_metadata::entry::FileMetadata.path: StandardizedPath`) | `language_by_local_filename(...)` | this one *does* discard a real `StandardizedPath` before language lookup, but the same converted local path is also used for `fs::read_to_string` right after — since filename/extension are unaffected by the lossy local re-encoding, using `language_by_local_filename` on the already-converted path is behaviorally identical to looking up the original `StandardizedPath`, so no ripple into `parse_file_outline`'s signature |
| tests: `crates/languages/src/lib_tests.rs`, `crates/ai/src/index/full_source_code_embedding/chunker/semantic_tests.rs`, `crates/syntax_tree/src/queries/indent_query_tests.rs` | all called the old single function on local `Path::new(...)` | switched to `language_by_local_filename` (or `language_by_filename` + `StandardizedPath::try_new` for `lib_tests.rs`'s pin-parity tests) | mechanical |

**Tests ported** (`crates/languages/src/lib_tests.rs`, matching the pin's
`html_extensions_resolve_to_html` / `command_extension_resolves_to_shell` /
`markdown_extensions_resolve_to_markdown` restructured around
`StandardizedPath`, plus a `local_*` sibling for each exercising
`language_by_local_filename`): also added
`foreign_encoded_path_resolves_language`, asserting a Windows-encoded
`StandardizedPath` (e.g. a remote SSH session on a Windows host) resolves by
filename/extension the same as a local path — this is exactly the case a bare
`&Path` cannot express, which is the whole reason this item exists. Kept the
fork's own `new_language_extensions_resolve` / `new_language_aliases_normalize`
tests (no pin equivalent, since those languages are a fork addition) adapted
to the new API.

**Not done / scope boundary**: the pin *also* splits several higher-level
wrapper methods that happen to share the same name coincidentally
(`CodeEditorModel::set_language_with_path`, and its callers in
`app/src/code/editor/view.rs`, `local_code_editor.rs`, `find_references_view.rs`,
`code_review/*.rs`, `ai/blocklist/*.rs`, `settings_view/mcp_servers/edit_page.rs`
— roughly 15 call sites) into a `StandardizedPath` version and a
`_with_local_path` version, mirroring the free-function split one level up.
That is a materially larger, independently-scoped port (a different struct's
API, not `languages::language_by_filename`), and under the build freeze I
cannot verify a multi-file signature change across that call graph compiles.
I left `set_language_with_path`'s own signature (`&Path`) untouched and only
fixed its internal call to the renamed free function, which preserves current
behavior exactly. Flagging this as follow-up work, not silently declaring it
in scope.

## 2. `TuiSelectable::with_semantic_selection_by_style` does not exist

Premise confirmed real by grep for the definition (`fn with_semantic_selection_by_style`):
zero hits tree-wide before this change; the only tree-wide references were two
comments in `crates/warp_tui/src/read_only_menu.rs` and `read_only_menu_tests.rs`
describing its absence. One of those two comments was itself stale — it also
claimed `TuiViewportedList::with_trimmed_selection_line_ends` was missing, but
that one was ported already (`crates/warpui_core/src/elements/tui/viewported_list.rs:471`,
wired at `read_only_menu.rs:226`) per a 2026-08-11 correction note already in
the file. Only `with_semantic_selection_by_style` was actually still missing.

**Ported from the pin** (`crates/warpui_core/src/elements/tui/selectable.rs` and
`selectable/cells.rs`):

- `TuiRowGlyph` gained a `style: TuiStyle` field, populated in `row_glyphs()`
  from `cell.style()` (the pin's exact change — this fork's `TuiRowGlyph` had
  no style tracking at all, which is *why* the function was missing, not just
  unported: there was nothing for it to compare). Confirmed the only
  `TuiRowGlyph { .. }` construction site is `row_glyphs()`, and the only
  `impl TuiSelectableElement` is `TuiViewportedList` (uses the shared
  `row_glyphs()` helper) — so this is not a breaking field addition anywhere
  else.
- `TuiSelectable` gained `semantic_selection_by_style: bool` (default `false`)
  and the builder method `with_semantic_selection_by_style()`.
- Added the `style_span` helper (resolves a semantic span from contiguous
  glyphs sharing the clicked style, trimming leading/trailing whitespace
  glyphs) and wired it into `selection_unit_span`'s `SelectionType::Semantic`
  arm, checked before falling back to `word_span` — byte-for-byte the pin's
  logic, including the `if cond && let Some(x) = ...` let-chain (the repo
  already pins edition 2024 / a 1.92+ toolchain that supports this).
- Confirmed `ratatui::style::Style` (aliased `TuiStyle`) derives `PartialEq`
  (checked the vendored `ratatui-core-0.1.2` source directly), which
  `style_span`'s glyph-style comparison depends on.

**Wired at the one call site the doc comments named**: `read_only_menu.rs`'s
`TuiReadOnlyMenu::render()` now calls `.with_semantic_selection_by_style()` on
the `TuiSelectable` it builds, matching the pin's construction exactly. Deleted
the now-false "intentionally omitted" doc comments on the module and on
`render()`.

**Tests**: the pin has no dedicated unit tests for `style_span` /
`with_semantic_selection_by_style` itself (checked via `git show 02b53fcd8`
across `*_test*.rs` — no hits), so there was nothing to port at that layer.
It *is* tested at the integration layer: `read_only_menu_tests.rs` already
carried two pin tests that had been left unported specifically because this
function (and, per its stale comment, the trim-line-ends function) didn't
exist — `selection_stops_at_trailing_whitespace` and
`double_click_selects_complete_styled_text`. Both are now portable; ported both
verbatim from the pin, plus the `left_down_with_click_count` helper the second
one needs. Updated the file's stale top-of-file comment accordingly.

## 3. Pane/panel minimum size too large (#324)

Both consts checked, as instructed, since they are not shared:

- **`app/src/ai_assistant/panel.rs:61` `MIN_PANEL_WIDTH`** — this is the one
  named in the report. **Lowered 300 → 240.** Chose 240 rather than an
  arbitrary smaller number because this codebase already has a working
  precedent for "narrowest usable width for a text-and-controls side panel":
  `DETAIL_SIDECAR_MIN_WIDTH = 240.` in `vertical_tabs.rs`. Checked the pin
  (`02b53fcd8:app/src/ai_assistant/panel.rs`) — it also hardcodes `300.`, so
  this is a deliberate departure from upstream Warp for a real usability
  report, not a parity fix; noted as such in a comment at the const
  definition. `DEFAULT_WARP_AI_WIDTH` (410, in `terminal/resizable_data.rs`)
  is unaffected, so the panel's *default* opened width does not change — only
  how far it can be dragged narrower. Spot-checked `render_title_bar()` (logo
  + heading text behind a `Shrinkable` spacer, then up to 3 fixed-size icon
  buttons) for anything that would visually break under 240px; nothing
  assumes more than icon-sized (≤44px) fixed widths, everything else is
  flex/shrinkable.
- **`app/src/workspace/view/vertical_tabs.rs:88` `MIN_PANEL_WIDTH` (200)** —
  this is a *different* constant (the tab sidebar, not the AI panel), already
  paired with a proportional max (`window_size.x() * MAX_PANEL_WIDTH_RATIO`,
  0.5) rather than panel.rs's fixed-offset max. Already lower than panel.rs's
  old 300 and already scales with window width. **No change made** — judged
  already reasonable, and the bug report as given names the AI panel
  specifically ("~300px floor").

**Explicitly out of scope, not touched**: the drag-latency half of #324, per
the task brief — it needs the app running.

**Test added**: factored the inline `with_bounds_callback` closure in
`panel.rs` out into a named, pure `fn panel_width_bounds(window_width: f32) ->
(f32, f32)` (previously untestable without a running window/`Resizable`
instance), and added `app/src/ai_assistant/panel_tests.rs` with three cases:
bounds reserve `MIN_REMAINING_WINDOW_SIZE` on a wide window, max never drops
below min on a narrow window (guards against handing `Resizable` an inverted
range), and a regression guard that the floor doesn't silently drift back
above `vertical_tabs.rs`'s `DETAIL_SIDECAR_MIN_WIDTH` precedent.

## Gates run

- `rustfmt --check --config-path .rustfmt.toml` on every touched file: no
  parse errors on any file; the only formatting *diffs* reported were
  pre-existing import-ordering churn unrelated to these changes (this rustfmt
  build disagrees with committed formatting repo-wide, per `HANDOFF.md`) —
  confirmed by re-running after reformatting only the lines this change
  touched, which then produced zero diff in every file except the
  pre-existing unrelated import blocks.
- `script/check_cloud_boundary`: ok (270 allowlisted import sites, unchanged).
- `script/check_stub_coverage`: ok (no test targets a gutted stub).
- `script/check_declined_collisions`: ok.
- `script/check_sweep_ledger`: ok.
- **Not run** (build freeze): `cargo check`/`test`, `nextest`, `script/precheck`.
  Cannot claim these three changes compile or their tests pass — only that
  they are sourced from the pin's real, compiling code, that every call site
  of a signature I changed was found and updated, and that formatting parses.

## Unfinished / follow-up

- The 15-site `set_language_with_path` / `set_language_with_local_path` split
  on `CodeEditorModel` (item 1's larger sibling) — see "Not done" above.
- Whether `MIN_PANEL_WIDTH = 240` is the *right* number, as opposed to *a*
  reasonable, precedented number — needs a human looking at the running app
  on the reporter's screen size, which the task explicitly scoped out.
