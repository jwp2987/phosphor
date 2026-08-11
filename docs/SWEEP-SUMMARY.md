# Pin-test sweep — consolidated result (2026-08-11)

Every pin test absent from this fork, hand-adjudicated. **1,841 tests across six
areas, 100% traced** — every mechanical `?` bucket in
[`SWEEP-INVENTORY.md`](SWEEP-INVENTORY.md) replaced with a verdict backed by
reading the pin source and the fork source.

Oracle: Warp `02b53fcd8`. Read it with `git show 02b53fcd8:<path>`. Never
`warp/master` — see [`../ORACLE.md`](../ORACLE.md).

## The ledger — read this before re-deriving anything below

The six prose files below are the **narrative record**: read them to
understand *why* a verdict was reached. They are not what a re-pin consumes.

[`../docs/sweep-verdict-ledger.tsv`](sweep-verdict-ledger.tsv) is the
**machine-readable** extraction of every row in those six files — one line
per pin test, TSV, greppable, diffable, matching the convention of
[`PIN-IDENTITY-MANIFEST-files.tsv`](PIN-IDENTITY-MANIFEST-files.tsv). Columns:
`test`, `pin_file`, `area`, `verdict`, `evidence`, `declined_ref`,
`pin_commit`, `sweep_date`, `confidence`, `source_doc`.

**1,843 rows** — two more than "1,841" above, because settings-workspace.md's
own per-file section headers sum to 290, not the 288 its own totals table
states; the ledger trusts the per-file sections, the same call that doc's own
per-file evidence makes about itself. Extraction fidelity: **1,661 rows
(90%) `clean`** — the source doc named this exact test under this exact
bucket; **177 (10%) `judgement`** — resolved by a documented, verified
inference (a stated bucket count with the remainder unnamed, a `grok_*`/
`geap_*`-style family, a fork-renamed test cross-matched by hand); **5
(0.3%) `unparsed`** — genuinely left unresolved, all five
`app/src/pane_group/mod_tests.rs` tests the sweep's own text called "needs a
second look, not re-verified this pass." No row was dropped to make a number
round. See [`script/extract_sweep_ledger.py`](../script/extract_sweep_ledger.py)'s
header for the full extraction method and every per-file special case.

### Re-pin procedure

1. Fetch the new pin, then run
   `script/generate_repin_queue <new-pin> <old-pin>`. It now cross-references
   the ledger and splits its output into three kinds of work instead of one:
   - **carried forward** — a ledger verdict whose pin file is untouched in
     the diff, whose cited `DECLINED.md` row (if any) is not struck, and
     whose named missing symbol (if checkable) still doesn't exist. Printed
     as a single count. This is the entire payoff: nothing to do.
   - **RE-EXAMINE, with a reason** — split into the three checkable
     invalidation rules below, each in its own section with the specific
     evidence that triggered it.
   - **genuinely new** — a test-bearing file with no ledger row and no
     `SCOPE-*.md` row either. Nobody has looked at it, full stop.
2. Read `script/generate_repin_queue`'s own header comment for the exact
   bucket list (it now includes `LEDGER RE-EXAMINE` sections alongside the
   pre-existing `DECLINED COLLISIONS`/`UNCLASSIFIED`/`ACTIONABLE` ones).
3. `script/check_sweep_ledger` runs continuously (wired into
   `script/precheck` and `pr-check.yml`), not just at re-pin — a
   `DECLINED.md` row can be struck at any time, and a ledger row still
   citing it is wrong from that moment, not from the next re-pin.

### The invalidation rules — when a carried-forward verdict must be re-examined

| # | rule | applies to | machine-checked? |
|---|---|---|---|
| 1 | The pin's test file changed between pin N and N+1. | every verdict | **Yes** — `generate_repin_queue`'s existing pin-to-pin diff; any changed file's ledger rows are flagged wholesale. The evidence was read against the OLD content, so it cannot be trusted against the new content sight unseen. |
| 2 | The cited `DECLINED.md` row is reversed or struck. | `DECLINED` verdicts | **Yes, conservatively** — `DECLINED.md` marks a reversed row `~~struck~~` rather than deleting it (2 rows do today: #440, and #304/#309/#310/#325/#329). An issue number counts as struck only when **every** row citing it is struck — #325 sits on both a struck row and a still-active one, and the ledger's `declined_ref` column can't tell which was meant, so a shared number is left alone rather than risk a false positive. This under-reports by construction; see `script/check_sweep_ledger`'s header for the full #325 case. |
| 3 | The fork gains the named subsystem. | `MISSING-SUBSYSTEM` verdicts | **Partially** — only when the evidence names a symbol immediately adjacent to "does not exist" / "needs `X`" / "lacks `X`" (23 of 195 rows qualify; the rest need a human). Tightened after the naive "first backtick token in the evidence" version produced real false positives in testing — see `generate_repin_queue`'s rule-3 comment for the two caught before ship. Any hit still says "VERIFY it is the same symbol, not a same-named unrelated one" — name collisions across files are the documented failure mode `check_declined_collisions` warns about for the identical `sym:` technique. |
| 4 | The fork test that covers it is deleted or renamed. | `COVERED-ELSEWHERE` verdicts | **No** — human-only. The covering fork test name lives in free-text evidence with no consistent syntax to extract; a wrong extraction here (silently trusting a deleted test) is worse than no check. Read the row. |
| 5 | The cloud backend is restored. | `CLOUD` verdicts | **No, and it shouldn't be per-test** — the most durable bucket by design (Phosphor dropping the cloud backend is not coming back), but if it ever changed it would be a repo-wide architecture decision invalidating potentially all 1,091 `CLOUD` rows at once. There is no cheaper per-test signal than that decision itself; don't build one. |

Rules 1 and 2 are enforced by script (`generate_repin_queue` for 1 live at
re-pin time and continuously via `check_sweep_ledger` for 2). Rule 3 is
enforced narrowly, on purpose. Rules 4 and 5 are stated, not automated —
an unenforceable rule presented as enforced is worse than none.

## Per-area detail

| area | tests | file |
|---|---:|---|
| `app/src/ai/**` | 917 | [`sweep/app-ai.md`](sweep/app-ai.md) |
| `app/src/settings_view`, `settings`, `workspace`, `pane_group`, `search` | 288 | [`sweep/settings-workspace.md`](sweep/settings-workspace.md) |
| `app/src/terminal/**` (excl. `ssh/`) | 263 | [`sweep/app-terminal.md`](sweep/app-terminal.md) |
| `crates/warp_cli`, `crates/onboarding` | 139 | [`sweep/warp-cli.md`](sweep/warp-cli.md) |
| `crates/ai`, `crates/build_cache` | 133 | [`sweep/crates-ai.md`](sweep/crates-ai.md) |
| `crates/warp_tui/**` | 101 | [`sweep/warp-tui.md`](sweep/warp-tui.md) |

## Totals

| bucket | count | share |
|---|---:|---:|
| CLOUD — needs dropped cloud plumbing | 1,204 | 65% |
| DECLINED — an existing `DECLINED.md` decision covers it | 284 | 15% |
| **MISSING-SUBSYSTEM — real non-cloud debt** | **209** | **11%** |
| DIVERGENT — fork API genuinely differs | ~60 | 3% |
| COVERED-ELSEWHERE — fork tests it under another name | ~26 | 1% |
| **PORTED this sweep** | **19** | **1%** |

## The headline: the mechanical estimate was wrong by an order of magnitude

`SWEEP-INVENTORY.md`'s mechanical pass bucketed ~340 tests `PORTABLE?`. Hand
tracing found **19 genuinely portable**. Two independent measurements of the
same slice disagreed by ~18x.

**Why the heuristic failed, stated precisely** (from the `warp_cli` pass, and it
generalises):

> the cloud coupling lives in **which subcommand a test parses**, not in the
> test body's own imports

A test that parses `oz agent run --share` imports `clap`, a struct and an
assertion — nothing cloud-shaped. The dependency is in the *semantics* of the
thing under test, which no import-based classifier can see. `app/terminal` had
the same shape: 32 tests bucketed `DIVERGENT?` are CLOUD, needing
`FeatureFlag::CloudMode` which no longer exists in `warp_features`.

**Do not re-derive the portable count from name-diffing.** It over-reports
by roughly 18x, not the "quarter" the inventory originally estimated.

## What the sweep was actually worth: six defects

Test-porting yield was ~1%. Defect yield is what justified the exercise — and
none of these would surface in any test-count metric.

### Fixed and merged

1. **Codex plugin events were never flag-gated.**
   `CodexSessionHandler::try_parse` accepted structured OSC 777 JSON even with
   `FeatureFlag::CodexPlugin` off, while `plugin_manager/codex.rs` checks that
   same flag at ~12 sites and the flag's own doc says "when disabled, Codex uses
   native OSC9 notifications."
2. **Tab could silently steal shell completion.**
   `keymap_context()` set `ATTACHMENTS_AVAILABLE_FLAG` without checking shell
   mode, so whenever image attachments were present Tab went to attachment focus
   instead of completing the command — contradicting the file's own adjacent
   comment about mutual exclusion. Fixed with the pin's
   `attachment_focus_available()`.
3. **`/theme` had no live-state suffix.** Every other stateful slash command
   (`/auto-approve`, `/natural-language-detection`, `/vim-mode`) shows
   `(currently …)`. Not a decision, an omission.

### Found, deliberately NOT fixed

Each needs a signature change touching passing tests, unverifiable without a
compiler. Reported under AGENTS §5.11 rather than attempted blind.

1. **`ReadSkillExecutor` ignores the session host**
   (`app/src/ai/blocklist/action_model/execute/read_skill.rs`) — a remote SSH
   session can read the **wrong bundled-skill catalog**. Everything needed
   already exists: `active_skill_by_reference_with_origin`,
   `SessionContext::skill_path_origin`, `ActiveSession`. Never wired.
2. **`ConversationUsageView::handle_action` is a literal no-op**
   (`usage/conversation_usage_view.rs:502`) — "View details" and "Show N more"
   clicks do nothing. No field and no test catches it.
3. **`classify_gui_list_entry` can never return `Unavailable`**
   (`blocklist/agent_view/conversation_selection.rs:99`) despite the variant
   existing — missing predicate parameter.

## The 209 MISSING-SUBSYSTEM — the real remaining debt

Ranked by tractability, not size.

- **`blocklist/usage/rollup.rs`** (8 tests) — its sole real dependency,
  `descendant_conversation_ids_in_spawn_order`, **already exists in the fork.**
  Closest thing to free work in the sweep.
- **Remote project rules have no `HostId` dimension**
  (`crates/ai/src/project_context/model.rs`) — `path_to_rules` /
  `ProjectRule::path` are host-blind, so project-rule resolution over SSH always
  returns `None` (6 tests). `global_rules.rs` (#575) **already solved the
  identical problem for global rules** — there is a working pattern in-tree.
- **`InputTypeAutoDetectionSource::AgentTerminalControl` + two hint strings**
  (15 tests, warp_tui) — "attach agent to running command" landed its mechanism
  but not its supporting pieces.
- **TUI renderer for `MessagesReceivedFromAgents` / `EventsFromAgents`**
  (9 tests) — **the GUI already has one.**
- **`skill_watcher.rs`** lacks the remote-project-skill refresh/fallback layer
  (13 tests).
- **The pin's `app/src/ai/orchestration/` config-picker layer** (39 tests).
  **CORRECTED 2026-08-11, maintainer: orchestration itself IS built here** — the
  sweep's "does not exist at all" phrasing was about the pin's *path*, and read
  as a claim about the subsystem, which was wrong and is the kind of error this
  document exists to stop.

  What the fork has, and it is substantial: `blocklist/orchestration_topology.rs`
  (26 tests), `blocklist/orchestration_events.rs` (10),
  `agent_view/orchestration_{pill_bar,pill_bar_model,avatar,conversation_links}.rs`
  (10 across them), `block/view_impl/orchestration.rs`, and
  `warp_tui/src/orchestration_{model,tab_bar}.rs`.

  What is genuinely absent is narrower: the pin's **config-picker layer** —
  `config_state.rs`, `edit_state.rs`, `providers.rs`, `remote_child.rs`,
  `snapshots.rs`, `validation.rs`. That is the surface for *choosing* harness /
  model / environment / host for a local orchestration run, not orchestration
  itself. Tracked #310/#304. It is also the only reachable consumer of
  `local_harness_setup.rs`'s `is_selectable` / `local_harness_is_product_enabled`,
  which is why those two remain `#[allow(dead_code)]`.

  Restated: **not a missing subsystem, a missing configuration UI in front of one
  that works.**
- **No `/index` slash command** — indexing is auto-only, a user cannot ask for
  it. Matters more now that the codebase index is actually wired to
  `get_relevant_files`.
- **MCP tool results render as a `serde_json::to_string_pretty` blob**, not a
  collapsible tree — no `McpRenderable` / `mcp_result_to_renderable`.
- **TUI selection** cannot trim trailing whitespace or select a styled word —
  `warpui_core` lacks `with_trimmed_selection_line_ends` /
  `with_semantic_selection_by_style`.
- **`languages::language_by_filename` has no `StandardizedPath` overload** —
  remote files resolve their language through a host-local `Path`.

Also relevant: **19 of `history_model_tests.rs`'s 27** mechanically-CLOUD tests
are actually local-orchestration debt calling fork methods that already exist.

## Scoping answer for #456 (`warp_tui` "a generation behind")

24 of 101 (24%) trace to **two coherent partial ports**, not diffuse per-test
debt — the two `AgentTerminalControl` and agent-message-renderer items above.
So #456 is not "the crate is behind"; it is two specific unfinished features.

## Methodology note

The `app/src/ai` pass ran **two independent research agents** over the 56
largest `CLOUD?` files, each told nothing about the other, both re-verifying
imports with `git show 02b53fcd8:<path>`. They found roughly a third
mis-bucketed **in both directions** — including files bucketed cloud that are
purely local. Convergent independent verification is the reason to trust these
numbers over the single-pass mechanical ones.

## Ledger corrections this sweep produced

- `SCOPE-REST.md`: the claim that the fork's `Harness` enum drops Codex and
  lacks `--bedrock-role-region` is **stale** — both gaps are closed.
- `SCOPE-*.md` verdict A is overstated **twice over**: MIXED files collapse to
  their majority bucket, *and* verdict A only asks whether a file of that name
  exists — not whether it is the same module, nor whether the API still exists.
- `ORACLE.md`'s "starfield" claim for `zero_state_animation_tests.rs` is stale
  post-#466 resync.
- `DECLINED.md` #174 undercounts its tie-break exclusions by one (4 in source,
  row says 3).
- `crates/build_cache` **does not exist in this fork**; the inventory said it
  ships the source.
- `agent_block.rs`'s "cloud" comment is stale post-reversal of the
  local-orchestration decline.
