# Pin identity manifest

Which fork `.rs` files under `app/src` and `crates` are byte-identical to
the pinned oracle right now, versus touched (`DIFFERS`) or fork-original
(`FORK-ONLY`). Identical files are the ones that can be fast-forwarded to the
next pin with zero judgment -- see the "RE-PIN AUTOMATION" section of
`TODO.md` for why this number matters and `ORACLE.md` for the pin policy.

Generated **2026-08-11**, pin `02b53fcd8`, fork `03772a004`.

**To regenerate:** `script/generate_pin_identity_manifest`. It needs the pin
commit fetched (`git fetch warp 02b53fcd8`) and writes this file plus the
per-file evidence in `docs/PIN-IDENTITY-MANIFEST-files.tsv`. It is a
generated snapshot, not a live gate -- nothing in CI runs it, so it drifts
between runs; re-run it after a porting pass, and always at a re-pin.

## Headline

| bucket | files | % of fork |
|---|---:|---:|
| **IDENTICAL** — fast-forwardable, zero judgment | 572 | 16% |
| **DIFFERS** — fork touched this file | 2334 | 69% |
| **FORK-ONLY** — no path match at the pin | 460 | 13% |
| **total fork .rs files scanned** | 3366 | 100% |

**Pin-only** (a path the pin has that this scan found no fork counterpart
for): **997** files. Not itemised here — this is exactly what
SCOPE-AI.md / SCOPE-TERMINAL.md / SCOPE-REST.md already classify file-by-file,
with the caveats those files carry (read their staleness banners before
treating either count as a fact rather than a verdict).

**FORK-ONLY is not the same as "fork-original".** This is a raw path match:
the fork's own `*_tests.rs` -> `*_test.rs` rename and `a/b/c_tests.rs` ->
`a/b_c_tests.rs` flattening (documented in ORACLE.md) means some FORK-ONLY
paths here are renamed pin files, not new ones. Cross-check against
SCOPE-*.md before calling a FORK-ONLY file "ours".

## By component

Top-level `app/src/<area>` or `crates/<crate>`, sorted by file count.

| component | files | identical | differs | fork-only |
|---|---:|---:|---:|---:|
| `app/src/ai` | 446 | 35 | 297 | 114 |
| `app/src/terminal` | 425 | 40 | 334 | 51 |
| `crates/warpui_core` | 250 | 65 | 107 | 78 |
| `app/src/search` | 177 | 46 | 112 | 19 |
| `crates/warp_tui` | 163 | 49 | 95 | 19 |
| `crates/warpui` | 158 | 17 | 139 | 2 |
| `crates/ai` | 92 | 32 | 49 | 11 |
| `crates/editor` | 85 | 9 | 72 | 4 |
| `app/src/integration_testing` | 79 | 27 | 51 | 1 |
| `crates/warp_completer` | 67 | 9 | 46 | 12 |
| `app/src/(root files)` | 63 | 11 | 43 | 9 |
| `app/src/workspace` | 60 | 5 | 51 | 4 |
| `app/src/code` | 56 | 10 | 46 | 0 |
| `app/src/settings` | 55 | 4 | 47 | 4 |
| `crates/warp_core` | 55 | 9 | 33 | 13 |
| `crates/integration` | 46 | 3 | 42 | 1 |
| `app/src/code_review` | 45 | 1 | 35 | 9 |
| `app/src/settings_view` | 45 | 5 | 33 | 7 |
| `app/src/drive` | 42 | 2 | 36 | 4 |
| `app/src/util` | 39 | 7 | 26 | 6 |
| `app/src/editor` | 38 | 3 | 25 | 10 |
| `crates/warp_terminal` | 38 | 13 | 23 | 2 |
| `crates/onboarding` | 35 | 1 | 33 | 1 |
| `app/src/pane_group` | 33 | 0 | 29 | 4 |
| `crates/computer_use` | 33 | 20 | 13 | 0 |
| `app/src/notebooks` | 31 | 1 | 29 | 1 |
| `crates/warp_cli` | 28 | 11 | 15 | 2 |
| `app/src/context_chips` | 24 | 2 | 19 | 3 |
| `crates/repo_metadata` | 24 | 3 | 19 | 2 |
| `app/src/remote_server` | 23 | 3 | 15 | 5 |
| `crates/remote_server` | 23 | 5 | 17 | 1 |
| `app/src/workflows` | 22 | 0 | 16 | 6 |
| `crates/lsp` | 22 | 17 | 5 | 0 |
| `crates/warp_util` | 22 | 7 | 13 | 2 |
| `app/src/plugin` | 21 | 8 | 13 | 0 |
| `app/src/view_components` | 21 | 0 | 21 | 0 |
| `app/src/ui_components` | 20 | 3 | 16 | 1 |
| `crates/vim` | 20 | 7 | 13 | 0 |
| `crates/warpui_extras` | 18 | 5 | 11 | 2 |
| `app/src/cloud_object` | 15 | 1 | 10 | 4 |
| `app/src/local_control` | 15 | 9 | 6 | 0 |
| `app/src/tab_configs` | 15 | 1 | 14 | 0 |
| `crates/input_classifier` | 15 | 2 | 12 | 1 |
| `app/src/env_vars` | 14 | 1 | 13 | 0 |
| `crates/local_control` | 13 | 4 | 9 | 0 |
| `app/src/themes` | 12 | 1 | 9 | 2 |
| `app/src/persistence` | 11 | 0 | 9 | 2 |
| `app/src/server` | 11 | 0 | 7 | 4 |
| `app/src/autoupdate` | 10 | 1 | 7 | 2 |
| `app/src/resource_center` | 10 | 0 | 10 | 0 |
| `crates/ui_components` | 10 | 0 | 10 | 0 |
| `app/src/ai_assistant` | 9 | 1 | 8 | 0 |
| `app/src/app_services` | 9 | 0 | 6 | 3 |
| `crates/warp_search_core` | 9 | 4 | 4 | 1 |
| `app/src/notifications` | 8 | 0 | 0 | 8 |
| `app/src/uri` | 8 | 0 | 5 | 3 |
| `crates/ipc` | 8 | 0 | 8 | 0 |
| `crates/managed_secrets` | 8 | 0 | 8 | 0 |
| `crates/settings` | 8 | 1 | 7 | 0 |
| `app/src/experiments` | 7 | 2 | 5 | 0 |
| `crates/command` | 7 | 2 | 5 | 0 |
| `crates/websocket` | 7 | 0 | 7 | 0 |
| `app/src/test_util` | 6 | 1 | 5 | 0 |
| `app/src/user_config` | 6 | 1 | 4 | 1 |
| `crates/channel_versions` | 6 | 1 | 5 | 0 |
| `crates/isolation_platform` | 6 | 4 | 2 | 0 |
| `crates/markdown_parser` | 6 | 1 | 3 | 2 |
| `crates/warp_js` | 6 | 4 | 2 | 0 |
| `crates/warp_logging` | 6 | 3 | 3 | 0 |
| `app/src/coding_entrypoints` | 5 | 1 | 4 | 0 |
| `app/src/workspaces` | 5 | 0 | 5 | 0 |
| `crates/prevent_sleep` | 5 | 2 | 3 | 0 |
| `crates/syntax_tree` | 5 | 1 | 4 | 0 |
| `crates/warp_files` | 5 | 2 | 2 | 1 |
| `app/src/launch_configs` | 4 | 1 | 3 | 0 |
| `app/src/login_item` | 4 | 0 | 4 | 0 |
| `crates/handlebars` | 4 | 0 | 2 | 2 |
| `crates/jsonrpc` | 4 | 2 | 2 | 0 |
| `crates/persistence` | 4 | 2 | 2 | 0 |
| `crates/simple_logger` | 4 | 1 | 3 | 0 |
| `crates/warp_ripgrep` | 4 | 1 | 3 | 0 |
| `app/src/antivirus` | 3 | 1 | 2 | 0 |
| `app/src/banner` | 3 | 2 | 1 | 0 |
| `app/src/completer` | 3 | 0 | 3 | 0 |
| `app/src/crash_reporting` | 3 | 0 | 2 | 1 |
| `app/src/system` | 3 | 0 | 3 | 0 |
| `app/src/tips` | 3 | 0 | 2 | 1 |
| `app/src/undo_close` | 3 | 0 | 3 | 0 |
| `crates/http_client` | 3 | 0 | 1 | 2 |
| `crates/sum_tree` | 3 | 0 | 2 | 1 |
| `crates/usage_suite` | 3 | 0 | 0 | 3 |
| `app/src/auth` | 2 | 0 | 1 | 1 |
| `app/src/bin` | 2 | 0 | 1 | 1 |
| `app/src/chip_configurator` | 2 | 0 | 2 | 0 |
| `app/src/default_terminal` | 2 | 0 | 2 | 0 |
| `app/src/platform` | 2 | 0 | 2 | 0 |
| `app/src/prompt` | 2 | 1 | 1 | 0 |
| `app/src/skill_manager` | 2 | 0 | 0 | 2 |
| `app/src/suggestions` | 2 | 1 | 1 | 0 |
| `app/src/tui` | 2 | 0 | 2 | 0 |
| `app/src/voice` | 2 | 1 | 1 | 0 |
| `crates/asset_cache` | 2 | 1 | 1 | 0 |
| `crates/command-signatures-v2` | 2 | 1 | 1 | 0 |
| `crates/fuzzy_match` | 2 | 0 | 1 | 1 |
| `crates/ipynb_parser` | 2 | 2 | 0 | 0 |
| `crates/languages` | 2 | 0 | 2 | 0 |
| `crates/managed_secrets_wasm` | 2 | 1 | 1 | 0 |
| `crates/natural_language_detection` | 2 | 2 | 0 | 0 |
| `crates/node_runtime` | 2 | 1 | 1 | 0 |
| `crates/settings_value` | 2 | 1 | 1 | 0 |
| `crates/string-offset` | 2 | 2 | 0 | 0 |
| `crates/voice_input` | 2 | 0 | 2 | 0 |
| `crates/warp_features` | 2 | 0 | 1 | 1 |
| `crates/watcher` | 2 | 0 | 2 | 0 |
| `app/src/external_secrets` | 1 | 0 | 1 | 0 |
| `app/src/pricing` | 1 | 0 | 1 | 0 |
| `app/src/quit_warning` | 1 | 0 | 1 | 0 |
| `app/src/tui_export` | 1 | 0 | 1 | 0 |
| `crates/app-installation-detection` | 1 | 0 | 1 | 0 |
| `crates/asset_macro` | 1 | 0 | 1 | 0 |
| `crates/field_mask` | 1 | 1 | 0 | 0 |
| `crates/http_server` | 1 | 0 | 1 | 0 |
| `crates/serve-wasm` | 1 | 0 | 1 | 0 |
| `crates/settings_value_derive` | 1 | 1 | 0 | 0 |
| `crates/virtual_fs` | 1 | 0 | 1 | 0 |
| `crates/warp_web_event_bus` | 1 | 0 | 1 | 0 |

## Raw data

Full per-file list (path, bucket, pin blob hash, fork blob hash):
`docs/PIN-IDENTITY-MANIFEST-files.tsv` (3366 rows, tab-separated,
header row included).
