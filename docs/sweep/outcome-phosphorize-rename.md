# Outcome: finish the "Phosphorize" terminology rename (burn-in cleanup)

Branch: `phosphorize/burn-in-rename`, based on `origin/main` @ `206799556`.

## Summary

The "Warping..." status-text concept (upstream Warp) had already been renamed
to "Phosphorize"/"Phosphorizing" almost everywhere, but a "burn in" variant had
leaked into the same UI status-text call sites and their surrounding comments.
This is the same concept in every case (the shimmering "in progress" label
shown while an AI conversation/exchange is running, both in the GUI status bar
and the TUI indicator row), confirmed against the pin
(`02b53fcd8:app/src/ai/blocklist/block/status_bar.rs` and
`.../view_impl/common.rs`, `crates/warp_tui/src/warping_indicator_tests.rs`),
where the equivalent strings are `"Warping..."` / `"Warping with {name}."`.
All of it has now been converted to the Phosphorize family.

## Sites changed

| file:line | before | after |
|---|---|---|
| `app/src/ai/blocklist/block/status_bar.rs:996` | doc comment: `(e.g. "Burning in with Claude 3.5 Haiku.")` | `(e.g. "Phosphorizing with Claude 3.5 Haiku.")` |
| `app/src/ai/blocklist/block/status_bar.rs:999` | doc comment: `flicker from "Burning in..." to "Burning in with {name}."` | `flicker from "Phosphorizing..." to "Phosphorizing with {name}."` |
| `app/src/ai/blocklist/block/status_bar.rs:1034` | `format!("Burning in with {name}.")` | `format!("Phosphorizing with {name}.")` |
| `app/src/ai/blocklist/block/status_bar.rs:1035` | `"Burning in with another model.".to_owned()` | `"Phosphorizing with another model.".to_owned()` |
| `app/src/ai/blocklist/block/view_impl/common.rs:132` | `pub const LOAD_OUTPUT_MESSAGE: &str = "Burning in...";` | `"Phosphorizing..."` |
| `crates/warp_tui/src/terminal_session_view.rs:5029` | `"Burning in"` (indicator-row label, mirrors `LOAD_OUTPUT_MESSAGE`) | `"Phosphorizing"` |
| `crates/warp_tui/src/warping_indicator_tests.rs:50,92` | `"Burning in"` (test input label, 2 sites) | `"Phosphorizing"` |
| `crates/warp_tui/src/warping_indicator_tests.rs:72` | `line.contains(" Burning in... (0s)")` (test assertion) | `line.contains(" Phosphorizing... (0s)")` |
| `app/src/terminal/view.rs:13617` | comment: `status bar's "Burning in..." disappears immediately` | `status bar's "Phosphorizing..." disappears immediately` |
| `app/src/terminal/view.rs:13655` | comment: `would get stuck on "Burning in..." yet again` | `would get stuck on "Phosphorizing..." yet again` |
| `app/src/ai/agent/api/convert_from.rs:568` | comment: `exchange would be stuck at "Burning in..." forever` | `exchange would be stuck at "Phosphorizing..." forever` |
| `app/i18n/en/warp.ftl:3665` | typo `Phosphorizeing` in `terminal-shell-premature-subtext` | `Phosphorizing` |

The `warping_indicator_tests.rs` changes update code and assertions together —
`render_warping_indicator_row("Burning in", ...)` and the rendered-text
assertion `line.contains(" Burning in... (0s)")` both moved to `"Phosphorizing"`
so the tests still assert the string the production code (now also changed)
actually emits, rather than the assertions and the code drifting apart.

## i18n misspelling sweep

Checked `app/i18n/en/*.ftl` and `app/i18n/ja/*.ftl` for other misspellings of
the Phosphorize family (`grep -o "Phosphoriz[a-zA-Z]*" ... | sort -u`). Found
only the one typo listed above (`Phosphorizeing` → `Phosphorizing`), on line
3665 of `app/i18n/en/warp.ftl`. No other malformed forms exist in either file.
Per `CLAUDE.local.md`, `app/i18n/zh-CN/*.ftl` and `app/i18n/ja/*.ftl` were left
untouched — not edited or translated.

## Deliberately left alone (burn-in occurrences that are a different sense of "burn")

These matched the `Burning`/`burn-in`/`burning`/`burn in` grep sweep but are
**not** part of the Phosphorize/Warpify naming family — they use "burn(ing)"
in its literal/idiomatic English sense, unrelated to the shell-startup status
text. Renaming these would misrepresent quoted upstream source text or change
plain English prose to something incorrect:

- `docs/sweep/computer-use.md:95` ("confirming the overlay burn-in has no
  producer"), `docs/sweep/computer-use.md:113` ("pass can burn in click/drag
  annotations"), `docs/sweep/crates-ai.md:235` ("overlay burn-in) is
  declined"), `DECLINED.md:183` ("`overlay::build_overlay_ass` which burns
  click/drag annotations into the video" / "the overlay burn-in stay out") —
  all four describe the **video overlay burn-in** feature of the declined
  computer-use session-recording subsystem (`crates/computer_use/src/overlay.rs`,
  `PointerSink`/`PointerSession`). This is literally pixels burned into a
  video frame (an `.ass` subtitle overlay of click/drag annotations), quoting
  or paraphrasing the pin's own doc comments (`overlay.rs`: *"burned-in
  recording annotations"*; `PointerSink`: *"burn in click/drag
  annotations"*). Changing "burn-in" here to "Phosphorize" would corrupt a
  direct quote from upstream source and describe the wrong feature.
- `HANDOFF.md:181` ("It sits there burning nothing and reporting nothing") —
  idiomatic English for an idle/stalled agent, unrelated to any UI string.
- `app/src/ai/agent_providers/tools/coerce.rs:156` ("burning a whole
  tool-call round-trip for nothing") — idiomatic English for "wasting",
  unrelated to any UI string.

None of these are load-bearing identifiers either way (they're prose in docs
and comments), but they are correctly *not* part of this rename — converting
them would be a factual/meaning change, not a terminology cleanup.

## `warpify`/`Warpify` sites — surveyed, none renamed (out of scope per task)

Per the task boundary, `warpify`/`Warpify` occurrences were left untouched.
Surveyed all 58 `.rs` files containing the term (plus the `.ftl` files) to
look for any that were purely cosmetic and safe to rename in isolation.

**Finding: none qualify as safely renameable in isolation.** Every occurrence
is either a load-bearing identifier itself, or prose that names a load-bearing
identifier/feature by its actual name. The `warpify` name is structural, not
incidental:

- **Module/file identity**: `app/src/terminal/warpify/` (module directory,
  `mod.rs`, `settings.rs`, `settings_test.rs`, `success_block.rs`,
  `trigger_state.rs`, `render.rs`), `app/src/terminal/ssh/warpify.rs`,
  `app/src/terminal/ssh/warpify_test.rs`, `app/src/settings_view/warpify_page.rs`,
  `app/src/terminal/view/block_banner/warpify.rs`,
  `app/src/terminal/view/use_agent_footer/warpify_footer.rs`.
- **Type/const identifiers**: `WarpifySettings`, `WarpifySuccessBlock`,
  `WarpifySettingsChangedEvent`, `SshWarpifyCommand`.
- **Script identifiers embedded in generated shell scripts** (cross a wire —
  these get shipped to and executed inside a subshell/SSH session, so
  renaming is a behavior change, not cosmetic): `warpify_ssh_session`,
  `begin_warpify_ssh_session` (`app/src/terminal/ssh/warpify.rs`,
  `app/src/terminal/model/ansi/handler.rs`).
- **Settings keys** (TOML, persisted/migrated user config — a wire format):
  `warpify.ssh.enable_legacy_ssh_wrapper` and the `WarpifySettings` group
  (`app/src/terminal/warpify/settings.rs`).
- **Fluent message IDs** (already noted in the main task's boundary list):
  `terminal-warpify-subshell`, `settings-section-warpify`,
  `settings-warpify-*`, `keybinding-desc-*-warpify-*`,
  `agent-tip-warpify-ssh`, `app-menu-*-warpified-ssh-blocks`, etc.
- **Prose comments** (e.g. `app/src/root_view.rs:2120`,
  `app/src/terminal/view_test.rs:8254`, `app/src/terminal/view.rs` — many
  sites) all refer to the feature or its identifiers by their real name
  (`warpify()`, `WarpifySettings`, "the Warpify settings page"). Renaming the
  prose alone while the type/module/script keeps the name `warpify` would
  reproduce exactly the inconsistency this task exists to fix (comment says
  one word, code says another) — so these are not "safe", they're just the
  visible tip of the same larger rename this task's boundary explicitly
  excludes.

No file was found where `warpify`/`Warpify` appears **only** as incidental
prose disconnected from the module/type/script family. Recommendation for a
future, separately-scoped decision: renaming this family requires deciding
what happens to the settings TOML key, the DCS/script identifiers shipped
into remote shells, and the fluent message IDs — i.e. it is a protocol/config
migration, not a text edit, exactly as the task brief anticipated.

## Verification

- `rustfmt --check --config-path .rustfmt.toml <file>` run individually on
  each of the 6 touched `.rs` files — all parse cleanly (no
  `error: expected`/`error: unexpected`), matching the parse-only gate
  `script/precheck` actually runs (the repo is not full-file rustfmt-clean
  project-wide, so a whole-file diff-clean check is not the real gate; see
  `script/precheck`'s own comment on this).
- `./script/check_channel_command_names` → ok.
- `./script/check_settings_registry` → ok (50 groups registered in both).
- **Not built or run** — no `cargo`/`nextest` per the task's hard rules.
  Written, unverified beyond the above static checks.
