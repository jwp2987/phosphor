# TODO — Phosphor: Warp parity ledger (#11) + code-review debt

## ACTIVE WORK QUEUE (2026-08-08) — read this first

**Process, agreed with the maintainer:**
- ONE sonnet agent at a time, ONE issue at a time.
- All work happens on branch `working`. The agent touches nothing else.
- After each issue: run the build check, and if green, merge `working` into
  local `main` and move to the next.
- Work the tiers in order: trivial -> small -> medium -> large.
- Update this section as each issue lands, so it survives context compaction.

**Verification rule learned the hard way (2026-08-08):** when a signature in
`app/src` changes, verify the DEPENDENTS too, not just the crate edited.
`warp_tui` consumes `Block::{is_visible,height}` via `warp::tui_export` and was
broken by #423 because the verification run covered only the edited crates.
Build check that catches this: `-p warp -p warp_tui -p remote_server -p repo_metadata`.

**This rule bit a SECOND time the same day, in a place that check does not
reach.** `crates/integration` also consumed the old `Block::is_empty` signature
and did not compile. Nothing caught it for hours because `warp` alone does not
enable the `integration_tests` feature — `crates/integration` only compiles when
`warp` and `integration` are checked in the SAME cargo invocation, so
`cargo check -p warp` is structurally blind to it and the suite was 5614/5614
green throughout. **Only `script/precheck` runs the feature-unified check. Run
`script/precheck`, not a hand-rolled `cargo check`, before calling anything
done.** Fixed in `0aee67c11`.

**Corollary — never `| tail -N` a gate script.** The first precheck run of this
work was piped to `tail -30`, which discarded every step above the test summary.
It printed `precheck: FAILED` with the failing step scrolled off, and the visible
tail was all green. Redirect the whole log to a file and grep it.

**Third rule — WIRE WHAT YOU PORT.** If a symbol is implemented but has no
production call site, that is a defect, not a done item. Fix it in the same
change, or say so loudly with file:line evidence. Never silently accept "ported
but never wired". #207 tracks this class (12 known instances). #334 is the
worked example: `reset_pane_sizes` landed in PR #515 with passing tests and
nothing ever called it, so the issue read as done while the feature did not
exist for a user. It was closed on that basis and had to be reopened.
Every agent brief must carry this rule.

**Second rule:** VERIFY EVERY ISSUE'S PREMISE BEFORE ESTIMATING OR IMPLEMENTING.
Of 11 issues examined closely on 2026-08-08, four stated the opposite of the code
(#437, #418, #532, #548-partly) and three more were already partly done. Estimating
from titles here is unreliable.

## RE-PIN AUTOMATION -- build during catch-up, pays off at pin N+1

Decided 2026-08-08. The catch-up against `02b53fcd8` is the FIRST pass and is
expensive by nature. Moving the pin later repeats tonight's motions, and most of
it can be mechanised -- but only if the inputs are recorded WHILE the first pass
happens. Retrofitting them afterwards costs as much as the pass itself.

**Mechanisable, worth building:**
- [ ] **Identical-to-pin manifest.** Per fork file, record whether it is
      byte-identical to the pin. Files that are identical can be fast-forwarded
      at the next re-pin with zero judgment. This single number tells us how
      cheap re-pinning actually gets. Cheap to generate as a one-off measurement
      (same method as the 2026-08-08 coverage measurement).
- [ ] **Re-pin work queue generator.** `git diff <pin N> <pin N+1>` over
      test-bearing files, bucketed by the existing `SCOPE-*.md` verdicts and
      `script/check_cloud_boundary`, so cloud-touching changes drop out
      automatically and what remains is a triaged list.
- [ ] **Divergence-collision guard.** THE ONE TONIGHT PROVED WE NEED. Flag when
      an incoming pin test collides with a deliberate fork divergence. Requires
      `DECLINED.md` entries to carry machine-checkable markers (symbol names or
      file paths), not just prose. Change how entries are written NOW, while they
      are being created anyway.
- [x] **Gates that actually run.** DONE 2026-08-08: `script/precheck` now covers
      8,342 tests across 43 packages, up from 6,181 across 3.

**Deliberately NOT automatable -- do not try:**
- Cloud-vs-local calls on ambiguous subsystems. `CLAUDE.md` already warns that
  `SCOPE-AI.md`'s verdict A is overstated (MIXED files collapse to their majority
  bucket), so a script reading those verdicts will confidently mis-bucket.
- Product divergences (e.g. the 2026-08-08 double-click decision). Maintainer's
  call, every time.
- This fork's own seams. Both focus bugs fixed tonight came from the GUI/TUI
  storage split THIS fork introduced; the skills-path issues come from cloud
  removal. Warp will never fix those, and they are where bugs concentrate.

**The discipline that keeps re-pinning cheap:** record every intentional
divergence in `DECLINED.md` the day you make it. Tonight's double-click
collision -- a July divergence contradicted by an August parity port, discovered
in neither -- cost real time purely because nobody wrote it down. `DECLINED.md`
already existed; it was not the tooling that failed.

### DECISION 2026-08-08 late — the SSH half of `ai/skills/remote.rs` is UN-DROPPED

**This partially reverses the #11 maintainer decision of 2026-08-02** ("AI skills:
build `bundled` + `global` (local); DROP the `remote` daemon-sync / cloud-repo
arm"). That verdict put two unrelated things under one label.

What the file actually is: `02b53fcd8:app/src/ai/skills/remote.rs` is **59 lines,
two functions** (`mcp_integration_wire_id`, `bundled_skill_snapshot_protos`), with
**zero cloud imports** — no `warp_graphql`, no `server_api`, no
`ServerApiProvider`, no `warp_server_client`. Its only external import is
`remote_server::proto`, which is **Phosphor's own daemon**. Its doc comment
describes the SSH path outright: *"Serializes a daemon-side bundled catalog for
the aggregate remote Agent Mode snapshot — the daemon owns the files."*

There is no cloud-repo resolution arm in that file.

**It is also a hard dependency of #353**, approved for build the same evening: the
pin's `app/src/remote_server/server_model.rs:91` imports
`bundled_skill_snapshot_protos` and calls it at `:348` to build the snapshot that
`refresh_remote_agent_context_snapshot` broadcasts at `:692`.

**CORRECTED AGAIN, later the same evening — `BundledSkills` IS needed, build it.**
I issued three different boundaries before getting this right. The final one:

| verdict | shape |
|---|---|
| **BUILD** | anything keyed by `HostId` that stores or routes context for hosts we are SSH-connected to |
| **DROPPED** | genuine cloud only — in this whole surface that is just `is_cloud_environment` |

**The proof was in the FORK, not the pin**, which is why reasoning from #11's label
kept failing. `crates/remote_server/src/manager.rs` already has, landed under #438:
- `:619` `remote_agent_context_snapshots: HashMap<HostId, RemoteAgentContextSnapshot>`
  — one snapshot PER HOST, with revision-based conflict resolution at `:2073`
- `:588` `host_to_sessions: HashMap<HostId, HashSet<SessionId>>` — multiple
  simultaneous hosts, each with multiple sessions

and `crates/remote_server/proto/remote_server.proto:294` defines
`RemoteAgentContextSnapshot { revision, home_dir, repeated RemoteSkillProto skills,
repeated RemoteContextFileProto global_rules }`.

So once #353's producer fills those snapshots, the client holds N hosts' skill
catalogs at once. `BundledSkills { local, remote_by_host: HashMap<HostId,
BundledSkill> }` is exactly the structure to hold and route them; without it a
conversation on host B resolves against host A's catalog. `remote_home_directories:
HashMap<HostId, LocalOrRemotePath>` is what proto field 2 (`home_dir`) is for, by
the same argument. Singular `BundledSkill` survives as the per-host inner type.

**The reusable lesson, which cost three reversals:** I reasoned downward from a
decision's *label* ("drop the remote arm") instead of checking what the codebase
already models. Every correction came from reading the fork's own structures. When
a scope decision seems to say "we do not support X", check whether the fork
already has data structures for X — if it does, the decision was about something
narrower than its wording.

**How this was nearly missed, which is the reusable lesson.** #487 raised exactly
this — *"the cloud-repo resolution arm is likely out of scope; the SSH/daemon arm
may not be... needs classification per-arm rather than a blanket verdict"* — and
was then closed citing #11's blanket verdict without performing the split. A
correct doubt was recorded and then overridden by a broader statement that had
never been checked against the code. **When a decision names two things with a
slash, check whether the file actually contains both.**

Porting notes for `remote.rs` (easy to lose in a port): `TuiOnly` skills are
omitted (a daemon cannot expose client-local migration behaviour); `RequiresFile`
and `RequiresFeature` are evaluated DAEMON-SIDE so the client only ever receives
`Always` or `RequiresMcp`; results are sorted by skill path so pushes are
deterministic across daemon restarts. `BundledSkill` needs an `iter_definitions()`
yielding `(id, skill, activation)` — the fork's current `iter()` drops activation.

### RECONCILIATION 2026-08-08 late — every open issue is tiered

Checked both directions programmatically (`gh issue list --state open` against
the tier lists): **0 untracked, 0 listed-but-closed.**

| bucket | count |
|---|---|
| tier 2 (in flight) | 2 — #205, #299 |
| absorbed into tier 2 | 2 — #353, #388 |
| tier 3 | 14 |
| tier 4 | 10 |
| maintainer decision, not code | 7 |
| **open total** | **35** |

Started the day at 63. The drop is **not** mostly fixes: roughly half were closed
because the premise did not hold — six were symbols the pin does not call either
(#552, #555, #547, #554, #536, #553), and several were records of completed work
that nobody closed (#523, #4, #208, #338).

**Re-run this reconciliation after any closing spree.** Eight issues were found
untracked by the tiers on 2026-08-08 — five already done and simply never closed,
#405 never tiered at all, and #4/#208 stale-open. A tier list nobody reconciles
drifts silently, and the drift always reads as "more work remaining than there is".

### RECOVERED WORK from closed-unmerged PRs (2026-08-08)

Nine PRs were closed without merging. When the workflow switched away from PRs
that morning, the in-flight ones were never triaged -- so real work sat on local
branches while the same ground was covered again. All branches survive locally.

| PR | branch | issue | status |
|---|---|---|---|
| #480 | feat/wire-local-control-cli | #216 | RECOVERED, landed `974cb9cc4` |
| #529 | ci/208-run-integration-tests | #208 | RECOVERED, landed `974cb9cc4` |
| #538 | fix/422-419-grid-clear-and-dcs | #422,#419 | RECOVERED -- real bug fixed (`reset_invalid_trailing_wide_char` now preserves `bg`, matching the oracle) |
| #546 | feat/394-411-288-cli-agent-variants | #411 | RECOVERED -- semantic conflict resolved (parse accepts `Harness::Codex`; local launch still rejects it, test relocated to prove both) |
| #565 | test/418-399-terminal-view | #418 | SUPERSEDED (my port covers it) |
| #566 | ci/multi-package-feature-check | -- | SUPERSEDED (precheck has it) |
| #489 | fix/373-ask-user-question-auto-approve | #373 | maintainer chose to leave as-is |
| #198 | chore/governor-disk-hygiene | -- | stale docs, 335 commits behind |
| #1 | review/oss-sync-shared | -- | review-only, never for merge |

**Every one of these predates the compiler-in-the-loop policy.** PR #480's own
body says "No compiler has touched this diff". They merge cleanly and compile,
but running the tests found two real defects nobody had ever seen:
- `FullGridClearBehavior` loses cell attributes across a shrink-resize (#538's
  OWN test caught it)
- `Harness::Codex` now parses where an existing test asserts it must not (#546
  vs the local-child-harness contract)

**Corrections this sweep forced, all one root cause -- treating `main` as the
only reality and never looking at the branches:**
- #208 was closed on faulty analysis (wrong directory: `src/test/` is the bin's
  scenarios; `tests/` is the real cargo test target). REOPENED.
- #532's premise was called false; it was actually written against #538's work,
  which was closed rather than merged.
- #401's "blocked by in-flight PR #480" was stale -- #480 was already closed.
- The "5 cloud_boundary_allowlist entries" figure was mine and wrong: it is ONE
  entry (4 of the lines were comments), and it is justified-local.

### Tier 1 — trivial (< 1h each)
- [x] #334 pane divider double-click -- DONE 2026-08-08 (`315cfbb57`). Data layer was
      already in (PR #515); the gesture was never wired. Ported the pin's
      `divider_mouse_down_action` into both divider variants + `PaneGroupAction::ResetPaneSizes`.
- [x] #401 warpctrl symlink installer -- DONE 2026-08-08 (`693046e02`). Also had to add
      `Channel::warpctrl_command_name()`, which the issue did not mention.
      NOTE the distinction for the 'wire what you port' rule: #334 was unwired with NO
      blocker (fix it); #401 was unwired with a DOCUMENTED blocker owned elsewhere (accept
      and record it). The rule must not force collisions.
      **The #401 blocker is now GONE** and its note above was stale in two ways: PR #480
      was closed, not in flight, and `FeatureFlag::WarpControlCli` has since arrived on
      main by another route. Both stale comments corrected in `d16d7261b`, which also
      swapped the `read_skill_tests` stand-in flag back to the real one, matching the pin
      exactly. If #401 still wants palette wiring, nothing blocks it now.

**Premises for all of tier 1 were verified against the pin on 2026-08-08 (all 8 real).**
- [ ] #342 port `repository_gated_command_{drops_when_leaving,stays_within}_repository`.
      Blocker removed: `simulate_directory_for_completion` exists at `app/src/terminal/input_test.rs:515`.
      Pin source: `app/src/terminal/input/slash_command_model_tests.rs:556,627`.
      NB the issue title garbles the first test name.
- [ ] #410 util/bindings: two editable-binding regressions vs the pin.
      Verified: fork declares AND registers `TOGGLE_MAXIMIZE_PANE_BINDING_NAME`
      (`pane_group/mod.rs:184,434`) but never uses it at the pin's second site,
      `terminal/view/pane_impl.rs:692`.
- [ ] #436 warpui_core TuiViewportedList: no trimmed-selection-line-ends option.
      Verified absent; pin has `trim_selection_line_ends` + `trimmed_selection_row_end`
      in `crates/warpui_core/src/elements/tui/viewported_list.rs:21,168,438`.
- [ ] #498 file tree: `show_hidden_files` has no Settings toggle / palette action.
      Verified: setting IS read (`code/file_tree/view.rs:357,418,726,1704`), no UI entry.
- [ ] #549 duplicate dead test-fixture helpers. Verified: `app/src/test_util/virtual_fs.rs`
      and `crates/virtual_fs/src/lib.rs` both define `git_repository_fixture`/`executable`/
      `fixtures`; the ONLY callers are each file's own `git_repository_fixture` calling its
      own `fixtures()`. Trap: delete inner-first or you break the self-reference.
- [ ] #547 view_components: ActionButton.callout / AlertConfig::success / Dropdown::Naked unwired.
      `AlertConfig::success` verified at zero uses; confirm the other two individually.
- [ ] #552 search/ai_context_menu: `render_search_bar` never called. Verified: defined at
      `app/src/search/ai_context_menu/view.rs:1656`, no call site. (The same-named methods in
      command_palette/welcome_palette/theme_chooser ARE called — do not confuse them.)
- [ ] #555 prompt/editor_modal: same-line-prompt toggle UI missing. Verified:
      `render_same_line_prompt_section` defined once at `app/src/prompt/editor_modal.rs:592`,
      never called.
- [x] #532 CLOSED 2026-08-08: #419 has now landed (recovered from PR #538) and
      `requires_registered_session`, `is_registered_session`, and
      `should_validate_dcs_hook_session_id` are present in
      `app/src/terminal/model/ansi/{dcs_hooks,mod}.rs` and `terminal_model.rs`. The
      original premise ("its premise is false, #419 hasn't landed") is now moot.

### Tier 2 — small (~half a day each)
- [ ] #523 cmd-k: `try_clear_buffer_in_agent_view` still checks only `is_agent_monitoring`
      (`clear_buffer` was fixed; this one guard remains)
- [ ] #545 CLI-agent image paste: keystroke is still agent-agnostic. Pin sends `ESC v`
      ONLY for `CLIAgent::Claude` on Windows; fork sends it for every agent, in BOTH
      `cli_agent_paste_keystroke_bytes` and `TerminalView::paste`.
- [ ] #205 skill path classification uses client home dir, misclassifies remote skills
- [ ] #299 SkillReference lacks remote/SSH path support
- [ ] #300 Mermaid code block does not defer to code-block rendering while loading/failed
- [ ] #313 BlocklistAIInputModel does not take an injected InputModePolicy
- [ ] #342 cannot port repository_gated_command_* without simulate_directory_for_completion
- [ ] #396 forking a conversation starts the new pane in the wrong working directory
- [ ] #403 notebooks/editor: mermaid asset-load relayout tracking missing
- [x] #411 warp_cli: Harness has no Codex variant -- DONE 2026-08-08. Recovered from
      closed PR #546 (`feat/394-411-288-cli-agent-variants`); `Harness::Codex` parses
      everywhere including local-child-harness normalization, but local launch still
      returns "Local Codex child harness support is not yet implemented." (that gap
      is #323's scope, not #411's). Test relocated in
      `local_harness_launch_tests.rs` to assert both halves of the contract.
- [x] #422 terminal/grid: FullGridClearBehavior missing -- DONE 2026-08-08. Recovered
      from closed PR #538 (`fix/422-419-grid-clear-and-dcs`); fixed a real bug the
      port's own test caught (shrink-resize was losing cell `bg` via
      `Cell::default()` instead of preserving it -- see
      `reset_invalid_trailing_wide_char` in
      `app/src/terminal/model/grid/grid_storage/resize.rs`, matching the oracle).
- [ ] #552 search/ai_context_menu: render_search_bar never called
- [ ] #554 code/editor_management: CodeManagerEvent::EditCompleted has no subscriber

### Tier 3 — medium (1-3 days each)

**Fully re-audited against the pin 2026-08-08.** All 20 prior entries were
verified with file:line evidence; none came back uncertain. Result: 4 closed, 2
absorbed into the tier-2 batch, 14 remain — and most of those are NARROWER than
their titles claim. Read the issue's latest comment, not its title.

CLOSED 2026-08-08 on evidence:
- #536, #553 — dead code AT THE PIN TOO (`snapshot.rs`, `for_update`). Not gaps.
- #548 — the only `impl Slide for` in all of Warp is `oz_launch.rs`, pure cloud
  marketing. Scaffolding is faithfully ported; its one implementor is declined.
- #338 — composite; every sub-item already done, declined, or never a pin feature.

ABSORBED into the tier-2 batch (maintainer decision 2026-08-08):
- #353 — the skills full-parity work includes `remote_agent_context.rs` and the
  daemon-side producer, which IS #353's scope.
- #388 — its 3 real sub-items touch the same proto/daemon files, so folded in to
  avoid a second proto-regeneration cycle. (Sub-item 3, `GetCommittedBranchFiles`,
  is NOT a gap — the fork uses direct RPC, functionally equivalent.)

**REOPENED 2026-08-08 late — #440, and it is a HARD DEPENDENCY of in-flight work:**
- [ ] #440 remote_server: bundled global skills/resources install mechanism.
      **Reopened not because the decline was mislabelled** (a full 30-row
      `DECLINED.md` audit confirms it never claimed cloud — it is an honest
      packaging decision) **but because it became incoherent with the #487 SSH
      un-drop made the same evening.** The pin's daemon
      (`02b53fcd8:app/src/remote_server/server_model.rs:724`) gates its whole
      bundled-skill catalog on `daemon_bundled_resources_dir()`; with #440
      declined it takes the `else` branch forever, so `bundled_skill_snapshot_protos`
      — un-dropped tonight — serializes an empty catalog and #353's broadcast
      carries no skills. We would ship the entire chain inert, knowingly.
      Scope: `BUNDLED_RESOURCES_DIR_NAME` / `remote_server_bundled_resources_dir()`
      / `remote_server_removal_command()` in `crates/remote_server/src/setup.rs`,
      `daemon_bundled_resources_dir()` + the spawn in `server_model.rs`, removal
      wiring in `ssh_transport.rs:289`, **plus the packaging half** — the
      remote-server artifact must actually ship a `bundled_resources/` tree, which
      touches the release pipeline, not just Rust. Tier 3 because of that packaging
      half; the Rust side alone is small. **Do it with or before #353 ships**, or
      #353 ships degraded (it still carries `home_dir` and `global_rules`, which
      have separate sources — so degraded, not dead).

      **Reusable lesson:** the audit that cleared this row answered *"is the stated
      reason true?"* — and it was. It did not answer *"is this still consistent with
      what we decided since?"* **A decline can be individually sound and
      collectively wrong.** Re-check declines against decisions made after them.

**REAL as filed:**
- [ ] #284 no `received_rich_notification` latch on `CLIAgentSession`; fork derives
      rich-status statically per agent type (`listener/mod.rs:36-38`) vs the pin's
      per-event latch (`cli_agent_sessions/mod.rs:153,412,441`). 3 pinned tests.
      **Touches the same struct as tier-2 #545** — adjacent, low risk.
- [ ] #343 `BlocklistAIContextModel` has no `try_start_new_conversation` for TUI;
      fork hard-codes the GUI path and always errors on TUI (`context_model.rs:1184`).
      **BLOCKED on #316** — needs a real `AgentViewConversationSelection` to inject.
- [ ] #316 `AgentViewConversationSelection` never ported. Delegation half is real,
      portable debt (the `AgentViewController` it needs already exists at
      `agent_view/controller.rs:778`). **The `classify_entry` half is entangled with
      the #418 DECLINED decision** — it calls `ActiveAgentViewsModel`, permanently
      deleted here; needs a `BlocklistAIHistoryModel`-based substitute, not a port.
- [ ] #256 no persisted prompt-history snapshot / `prompt_history_candidates`
      (pin `history_model.rs:331-333,2370`). Items 1/3/4 of the original issue are
      superseded by #336/#337/#331; only item 2 remains.
- [ ] #431 no lazy metadata-only conversation read + summary backfill. Fork reads
      eagerly on every startup path (`sqlite.rs:3347`). 4 pinned tests. Real perf
      AND correctness gap.
- [ ] #217 Zap -> Phosphor rename incomplete: **361** `"Zap"` Rust literals on main
      (issue said 357; drift, not error). Fork-internal, no pin comparison applies.
      NOTE: renaming risks breaking persisted keybindings — see the open
      `zapctrl` vs `warpctrl` maintainer decision.
- [ ] #254 NARROWED to two items: `Input::unfreeze_agent_input` (pin
      `input.rs:7625`) and `CommandExecutionSource::SharedSession`'s `preserve_input`
      field. Items b/c are already ported (`input.rs:2037,2064`) via #399.
- [ ] #323 NARROWED: `Harness::Codex` now exists (landed under #411), but local
      Codex launch still returns "not yet implemented" (`local_harness_launch.rs:145-148`),
      and `ANTHROPIC_MODEL` merge, `normalize_orchestrator_agent_name`, and the
      OZ_CLI *prompt-text* augmentation (`local_claude_child_prompt`) are all absent.

**PARTLY REAL — scope narrowed, see each issue's re-scope comment:**
- [ ] #147 ONLY `/theme` remains. `/clear`+`/set-tab-color` done; `/rename-conversation`
      is genuinely cloud-coupled; `/reset-statusline`+`/copy-debugging-id` never existed
      at the pin — **that issue cited `warp/master`, the exact ORACLE.md trap.**
- [ ] #341 prompt-attachment plumbing DONE (`29049f4f8`); `register_mock_stream_for_test`
      exists. Remaining: `schedule_auto_resume_after_error`, `fail_conversation_due_to_shell_exit`,
      `emit_response_event_for_test`.
- [ ] #389 voice half DECLINED. Menu half is **ported but NOT WIRED** — `TuiReadOnlyMenuKind`
      has zero call sites. Also: `status_menu.rs` landed at the WRONG PATH (top-level
      instead of nested under `terminal_session_view/`); move it, do not re-port.
- [ ] #390 `state.rs` done. Remaining: `completions.rs`, `shortcuts.rs`, the
      attach/detach running-command API, and `terminal_use.rs`'s missing 6th param
      `agent_owns_alt_screen_input`. **`completions.rs` is BLOCKED on #395's
      completion-menu API.**
- [ ] #395 footer wording FIXED. Remaining: ask-question multiselect, blocked-action
      presentation, completion-menu API shape. File-edits expand/collapse: API landed
      but the DEFAULT still diverges (fork collapses, pin expands).
- [ ] #397 error tone FIXED. Remaining: statusline datetime/footer grouping
      (`format_statusline_*`, `render_statusline_datetime`,
      `TuiUiBuilder::shell_command_accent_style` — all absent).

**SEQUENCING — the warp_tui cluster is NOT parallelisable.** #389/#390/#395/#397
all touch `crates/warp_tui/src/terminal_session_view.rs`; #390 depends on #395's
completion-menu API; #390 and #397 both need `TuiUiBuilder::shell_command_accent_style`.
Work them as one ordered sequence. Likewise #343 is blocked on #316 — one pair.

### Tier 4 — large (a week+)
- [ ] #210 · #252 · #289 · #381 · #382 · #236 · #349 · #324 · #142
- [ ] #405 notebooks/file: Jupyter (`.ipynb`) rendering missing
      (`FeatureFlag::JupyterNotebooks`). Added to this tier 2026-08-08 — it had
      never been tiered at all, found by reconciling every open issue against the
      tier lists. A whole feature, hence tier 4.

**Being audited against the pin (2026-08-08), same treatment tier 3 got.** For
this tier the TEST COUNTS are the main claim -- five of these issues assert a
number of blocked tests (~733 total between #210/#381/#252/#289/#382), and that
number is what sets their priority. Verify claimed-vs-actual before acting.
Suspected double-counting: #252 vs #289 (both agent_sdk) and #381 vs #382 (both
app/src/ai). Tier 3's audit found 4 of 20 closeable and 8 more narrower than
filed, so treat these numbers as unverified until the audit lands.

Audit landed 2026-08-08 late. Results below.

- **#142 — CLOSEABLE, already done. My earlier "pull it forward, BYOP is
  untested" note here was WRONG and is retracted.** I saw `api_key` in two
  absent pin filenames and assumed BYOP. They are absent, but they are Warp's
  *cloud team API-key management* (`agent_sdk/api_key.rs` imports
  `warp_graphql::mutations::{expire_api_key,generate_api_key}` and
  `ServerApiProvider`) — programmatic tokens for Warp's own cloud API, a
  different concept from BYOP provider keys. PR #189/#227 already reconciled the
  real file: 12 ported / 3 blocked on pin-side dead code / 16 superseded by
  `AgentProviderSecrets` (the fork's actual BYOP store, 19 fork-original tests) /
  36 cloud. The "7 of 82" figure conflated four differently-scoped
  `api_key`-named files. **Lesson: a filename is not a scope.**
- **#324 overlaps work in flight.** Its `diff_state_tracker.rs`
  (`RemoteDiffStateManager`, ~472 lines) sits beside the `diff_state_remote.rs` /
  `diff_state_proto.rs` / proto files the current tier-2 batch is editing for
  #388/#353. Cheaper to do while that area is open.
- **#349 is macOS-only** — cannot be built or verified on this host regardless of
  verdict.

### Needs a maintainer decision, not code
- [ ] #149 · #150 · #203 (design decision) · #206 · #207 · #279 · #312

### Landed 2026-08-08
#191, #251, #253, #570, #437, #438, #439, #399, #418, #423, #208, #537 — plus
`main` going from 65 failing tests to 0, and three CI gates repaired
(`check_test_failures` was blind to `TRY n FAIL`; `precheck` compiled nothing and
ignored uncommitted work).


Reconciled 2026-08-04; **#11 section re-verified against code on `main` 2026-08-06.**
**Reconciled again 2026-08-07 (#148) — `main` = `2e7d6eb2f` (194 commits past the
`af79b705d` HANDOFF.md snapshot).** Every checkbox and issue reference below was
re-verified against `origin/main`, `gh issue view`, and `DECLINED.md` on that date;
see the commit that made this edit for the full classification.
`[x]` items in issue #11 = "keep/restore" (maintainer wants them in the fork). This
file is the live tracker: **mark an item `- [x]` the moment it's verified done.**

> **Issue #11 itself is now CLOSED (2026-08-07, `COMPLETED`).** It was fully
> reconciled: 44 of its 56 ticked items were already implemented on `main` (verified
> symbol-by-symbol), and the 10 genuinely absent are each now tracked by a specific
> issue. See the "#11 status" section below for the current table — the old
> "10 remain / 7 buildable" framing is superseded.
>
> **The old "`main` is red" story (PRs #140/#181, issue #171) is also resolved** —
> #171 is closed; PR #224 repaired the underlying regressions. `main`'s current
> red-test state is a different, smaller, fully-attributed set; see
> [Red on main](#red-on-main--2026-08-06) below, which now points at `HANDOFF.md`
> for the live count instead of embedding a number that will go stale again.

## Rules (apply to every item — same as the whole project)
- **`warp/master` is the behavioral oracle.** Port faithfully; adapt only for
  BYOP/local (no cloud) — never silently simplify away Warp behavior (AGENTS §5.10).
- **Tests-first, never defer.** Port Warp's oracle tests with each feature; a red
  test gets fixed now, never parked (AGENTS §5.6). Never weaken an assertion to go green.
- **Run all cargo through the governor:** `script/agent-cargo <agent-name> <cargo-args>`.
  It bounds how many compiles run at once and gives each agent its own target dir.
  Never invoke cargo bare while another agent is running (AGENTS §5.8).
- **English only** (code, comments, tests, docs). Exception: `app/i18n/zh-CN|ja/*.ftl`.
- **Central verification:** the owner re-runs the suite before marking done — don't
  trust an agent's self-report.
- **No CI builds as a discovery loop.** Local `cargo`/user's `script/run --release`
  is the verification; a release build happens once at the end to confirm.

---

## What's in this file

Two separate concerns, kept distinguishable:

- **[Part 1 — Warp parity restore ledger (#11)](#part-1--warp-parity-restore-ledger-11)** —
  the issue #11 keep/restore ledger plus the other outstanding parity work.
- **[Part 2 — Code-review debt](#part-2--code-review-debt)** — actionable findings from the
  code reviews and the security/performance audit, grouped by review.

Part 2 was consolidated in from a separate lowercase `todo.md` on 2026-08-06: two tracked
files differing only by case collide on case-insensitive filesystems (macOS/Windows), so
only this `TODO.md` remains. Nothing was dropped — items since verified as landed on `main`
are marked `- [x]` with the evidence inline rather than deleted.

---

# Part 1 — Warp parity restore ledger (#11)

## #11 status — CLOSED 2026-08-07 (full reconciliation)

Issue #11 was reconciled at *definition level* (symbol-by-symbol against
`origin/main`, excluding comments/binaries — a first pass with loose greps
produced false positives and was discarded) and closed. **44 of its 56 ticked
items were already implemented on `main`**; nothing was unticked (the ticks stand
as the historical record of intent, per maintainer instruction). The **10
genuinely absent items are now each tracked by a specific issue**, superseding
the old "3 decisions/holds + 7 buildable" framing:

| item | tracked by |
|---|---|
| Size-based / `warp_logging` rotation | DONE — see "Log-rotation" below |
| `SettingSurfaces` / `SettingsMode` | declined, `DECLINED.md` |
| CJK link-boundary mechanism | #223 (open) |
| `local_control` / `warpctrl` | #216 (open, comprehensive); #401 (installer sliver, open); #184/#200/#183 closed as subsets |
| Banner-immune PATH capture | #481 (closed — done) |
| TUI live background re-probe | #482 (open) |
| CDPATH-aware `cd` completion | #483 (closed — done) |
| Launch-at-login | #484 (closed — done, see "Requires macOS/Windows" below) |
| NLD heuristic feature flags | #485 (closed — done) |
| AI bundled + global skills | #487 (open — this ledger's old "AI global skills" entry below was **wrong**; see correction) |

(The 13 unchecked `[ ]` items are all keep-dropped/cloud — OTEL, VoiceInputLifecycle,
semantic-search, RunAgents orchestration, computer-use recording, cloud-mode-v2,
product-analytics telemetry, `IsCloudConversationStorageEnabled`, etc. — not work,
by decision. One open question remains outside the 10/13 split: theme syncability
*portable-path* machinery, since "syncable" there means syncable to Warp's cloud —
computes a correct answer to a question nothing currently asks unless local theme
portability is wanted.)

### Merged this session
- [x] **Pending-edit-batch conflict-discard** — CORE MERGED (targeted issue #101):
  `PendingEditBatch` 200 ms debounce + push-conflict-discard + save-flush; 3 oracle
  tests green in isolation. Deferred sub-part `BufferConflictDetected` server→client
  push (**#102**, blocked `handle_buffer_conflict_detected` + its 4th test) is now
  **DONE too** — fixed by commit `78b66b6b2` ("feat(remote_server): BufferConflictDetected
  push + git write-op RPC surface"), issue closed 2026-08-07. Assessment:
  `specs/pending-edit-batch/ASSESS.md`.

### Requires macOS / Windows — cannot be built or verified on this host
Not deferred for lack of intent: this box is Linux and these cannot be compiled or
exercised here at all. They need a macOS or Windows machine (or CI) to progress.
**Do not mark any of these done from a Linux build.**

- [ ] **WSLENV passthrough vars** *(Windows)* — **STALE: this claimed absent, but it
  is DONE.** `wsl_env_allowlist` exists at
  `app/src/terminal/local_tty/windows/environment.rs:202` (commit `17ee390a2`, PR #119,
  targeted issue #117). Compile-only port, per the commit's own note — still not
  runtime-verified on an actual WSL/Windows host, which is the real remaining item.
- [ ] **Launch-at-login** *(macOS + Windows)* — **STALE: this claimed absent, but it
  is DONE.** `app/src/login_item/` exists (`mod.rs`, `macos.rs`, `windows.rs`,
  `windows_tests.rs`; commit `17ee390a2`, PR #119, targeted issue #118). Same caveat:
  compile-only on this Linux host, not runtime-verified on macOS/Windows.
- [ ] **Edition-2024 release verification** *(macOS)* — the **code work is done and on
  `main`** (commit `48bc21cb9`, PR #53). Only a macOS release build remains unverified.
- [ ] **pwsh `-EncodedCommand` at 2 call sites** *(Windows)* — the fix is ported to
  `local_command_executor.rs:55` and `msys2_command_executor.rs:67`, matching the
  already-verified `shell.rs` site (commit `5365c62a`). Needs a Windows run to confirm.

### STALE-WRONG — corrected 2026-08-07
- [ ] **AI global skills** — **this entry previously said "WON'T DO (maintainer,
  2026-08-06)" and stated the opposite of the actual decision.** #11's 2026-08-07
  closing comment quotes the ledger's own "Maintainer BYOP decisions — 2026-08-02"
  section, settled before the WON'T-DO note was ever written: *"AI skills: build
  `bundled` + `global` (local); DROP the `remote` daemon-sync / cloud-repo arm."*
  Verified against `origin/main` today: `app/src/ai/skills/` is missing exactly
  `bundled.rs`, `bundled_tests.rs`, `global_skills.rs`, `global_skills_tests.rs`
  (plus `remote.rs`/`remote_tests.rs`, which stay dropped per the decision above).
  This is real open work, tracked at **#487** (open) — do not treat it as done or
  declined.

### Not started — true gaps
- [ ] **Skill remote-path** — now **#205**. Promoted out of this ledger after finding a
  real correctness bug rather than a missing feature: `get_provider_for_path` **and**
  `get_scope_for_path` both resolve `home_skills_path` against the *client's* home, so
  a remote skill under a same-named home dir is silently misclassified as local.
  Latent only because #170 means no remote path reaches them yet — **fix with or
  before #170.** Note this ledger previously claimed `get_scope_for_path` was migrated
  by #59; it was not (still `&Path`). Related but distinct from **#487** (AI global
  skills, above): #205 is the path-*typing* half of remote skills, #487 is the
  missing-*modules* half.

### Keep-dropped (decided this session)
- [x] **history_model reconciliation** — non-cloud parts DONE (optimistic rename /
  event-sequence / child-index cleanup + `TransientError` recovery). Remaining
  `WaitForEvents`/orchestration part is **KEEP-DROPPED (maintainer 2026-08-06)**: it
  is cloud orchestration (only a Warp-server tool call triggers it; RunAgents /
  OrchestrationEventStreamer are the dropped cloud surface). The BYOP recovery
  equivalent (`recovery_pending`→`TransientError`) already covers the local case, so
  `WaitingForEvents` never firing is correct, not a bug. The constructor-arity bits
  (`start_new_conversation`/`prompt_history_candidates`) have no consumer (tie to the
  undecided NLD-flags item). Recorded on #11; tracking issue #107 closed.

### Core landed — sub-part / wiring still outstanding
- [x] **`remote_server_controller` connection-label helpers** — DONE. This entry was
  **false**: `connection_label_for_session_info` is called in production at
  `remote_server_controller.rs:290` and `:526`, not only from its own tests.
  Re-verified against `main` `8c1841a94` on 2026-08-06.
- [ ] **`local_control` / `warpctrl` app-side** — **#200 is now CLOSED**, as a subset
  of **#216** (open), the comprehensive tracking issue: app-side module (23+2+2+1
  tests) + CLI-side module (19 tests) + settings group (6 tests, already landed via
  PR #472) = 53 tests. `crates/local_control` exists (14 source files);
  `app/src/local_control/` and `crates/warp_cli/src/local_control/` are still absent
  — PR #480 is open, wiring the app-side surface. The `install_warpctrl`/
  `uninstall_warpctrl` installer sliver in `app/src/workspace/cli_install.rs` is
  NOT covered by #216 and stays tracked separately at **#401** (open).
- [x] **Pinned-tabs / tab-groups remaining GUI surfaces** — **DONE, #146 closed
  2026-08-07.** Fixing commit `ababc7f07` ("feat(tabs): move-to-group submenu,
  multi-tab menu and modifier selection") ported the vertical-tabs group-header
  row, tab-group right-click menu, inline group-rename editor, and group-aware
  drag-and-drop reordering — the four items this entry previously listed as
  outstanding. Verified: `git merge-base --is-ancestor ababc7f07 origin/main`.
- [x] **repo_metadata standing-queries wiring** — **DONE, #201 closed 2026-08-07.**
  Wired by commit `0d345486f` (PR #121): `app/src/ai/skills/file_watchers/skill_watcher.rs`
  subscribes to `RepoMetadataEvent::StandingQueryResultsUpdated`, and `app/src/lib.rs`
  calls `set_project_skill_provider_paths`/`register_force_included_paths` at
  startup. (The old repro — `grep -rn standing_queries app/src` — returns zero hits
  because the driving symbols were renamed; search for the concept, not the name.)
  The **remote** half is genuinely missing, now tracked at **#296** (PR #526 open).
- [x] **Log-rotation deferred wiring** — **DONE, #202 closed 2026-08-07 — premise was
  already false when filed.** `crates/warp_logging`'s `LogConfig` already carries
  both `frontend` and `max_file_size_bytes`, and `app/src/lib.rs::init_common`
  already threads `launch_mode.log_frontend()` through — landed in the same commit
  `0d345486f` (PR #121) as the standing-queries wiring above. `max_file_size_bytes`
  staying `None` at call sites is not a fork gap either: the pin does the identical
  `..Default::default()` at every one of its own call sites.
- [x] **code_review over SSH — git write-ops** — DONE, merged 2026-08-06 (PR #125,
  issue #116). Commit / push / create-PR RPCs over SSH, plus a
  `git_operation_in_progress` guard on all three mutating handlers. Verified
  109/109 on `code_review` before merge. **Remaining sub-part:** AI commit-message
  autogen is still local-only and calls `generate_for_local_repo` with no
  `is_remote()` check — see #126.

### Done — 44 of 56 (present on `main`)
Verified by spot-check (all present): `is_jupyter_notebook_file`, `sorted_cd_directories`,
`LLMContextWindow`, `safe_browser_open_url`, `remote_matches_to_global`,
`GitBranchTrackingStatus`, `seal_with_context`, `SshRemoteServerSupport`,
`soft_wrapped_row_bounds`. The remainder are the earlier ~24 keeps (theme
syncability, relative line numbers, mermaid config, OSC-52/OSC-8, hyperlink
registry, tmux DCS, link-punct strip, CJK boundary, box-drawing, block-lifecycle,
code-symbol source, `approve` keyword, `sync::Condition`, `file_uri_drive_path`,
NLD flags, CDPATH, SettingSurfaces, browser allowlist, content-version assets,
image fallback, `TuiStack`, soft-wrap Home/End, tab-drag collapse, oversized
data-URI, editable bindings, autoupdate per-channel, `external_control_master`,
async find, banner-immune PATH capture, terminal-background reprobe, managed-secrets
BYO, `ModelEventDispatcher` SSH gate, URI deep-links) plus the PRs #58–97 items
(queued-prompts panel, remote/SSH global search, diff-state-over-SSH read path,
skill-scope `Home`, WSL program translation, Windows PATHEXT, `nld_heuristic_v2`,
mermaid fallback, focus-URL env, `standing_queries`, pinned-tabs storage).
(Sampled, not each of 44 exhaustively grepped.)

---

## Other outstanding (non-#11)
- [x] ⭐ **SMOKE TESTS** — on merged main (2026-08-05, after the 6 diff-state PRs #59-#64):
  `./script/usage-test --surface both` = **12 pass / 0 fail / 7 skip** (skips are
  needs-real-shell / needs-byop / needs-desktop — environmental), EXIT 0. Full warp lib
  sanity: `cargo test -p warp --lib` = **3910 pass / 0 fail / 33 ignored**. App boots and
  behaves with all parity + diff-state-over-SSH changes in.
- [ ] **Edition-2024 cross-platform build — macOS release verification only.** The code work
  is DONE and on `main`: the mac/wasm/windows `unsafe`-syntax fixes from branch
  `fix/edition-2024-native-targets` are merged (commit `48bc21cb9`, via PR #53) — verified
  2026-08-06 with `git merge-base --is-ancestor 48bc21cb9 main`, and the remote branch has
  been deleted. **All that remains is a local macOS `script/run --release` run**, which
  cannot be done on this Linux host — it needs a Mac (no CI-discovery builds). That run may
  still surface further latent mac-only errors.
- [ ] **#4 warp_tui suite** — **STALE, corrected 2026-08-07.** The deadlock this
  entry describes (`tui_generic_tool_call_view::…_completes_the_executor`) is FIXED
  — see Part 2 below, PR #124, commit `87d06d179` — do not re-investigate it. #4
  itself stays open, but its scope moved: CI now gates the `warp_tui` crate at all
  (it previously didn't — issue #465 covered that gap; PR #469 addressed it), and the remaining gap
  is understood as `warp_tui` trailing the pin by a generation, tracked with a full
  root-cause map at **#456**, with #384/#387/#389/#390/#392/#395 as siblings tracing
  to the same cause. Treat the old 18-failure nextest breakdown above as historical
  context for how this was first noticed, not as the current state.
- [ ] **#2 sweep** — the 2 missing GUI auto-resume oracle tests
  (`completed_user_controlled_lrc_{resumes_when_not_suppressed,skips_resume_when_suppressed}`)
  are now PORTED to `terminal/view_test.rs` (2/0; the resumes case needed a
  `GlobalResourceHandlesProvider` mock for the subagent-sidecar persist path; the fork's
  teardown method is `set_user_control_with_stop_reason`, Warp's is `set_user_control_for_teardown`).
  Broader 379-module sweep still ongoing. (Anchor Stop/auto-resume regression already code-fixed.)
- [x] **#5 deferred low-sev** — **STALE-WRONG, corrected: #5 is CLOSED (2026-08-05),
  not "all still present."** All 5 findings were dispositioned: mouse-wheel scroll
  reuse was FIXED (#78); the other 4 (multi-cursor selection span, footer statusline
  recompute, `first_rendered_line_width` paint-to-measure, `vim_visual_selection_ranges`
  duplication) were explicitly won't-fixed as either feature-gated, negligible, or
  folded into `specs/tui-render-perf/SCOPE.md`. Nothing actionable remains here.
- [x] **warp-suite i18n test-isolation** (found 2026-08-04) — the 3 deterministically-red
  tests (`drive::export::test_export_untitled_notebook`, `search::…::test_directory_search_support`,
  `workspace::…::terminal_primary_line_falls_back_to_new_session`) were the localized-`t!()`
  case: `App::test` never globally inits i18n, so the key only resolved when an earlier test
  triggered init. FIXED per-test and **LANDED ON `main` via PR #103** (commit `3150a17b9`) —
  verified 2026-08-06 with `git merge-base --is-ancestor 3150a17b9 main`; issue #98 is closed.
  All 3 green in isolation, no assertions changed. NOTE: the same class likely
  still affects #4's `slash_commands` tests; a test-binary-global i18n init would close those too.
- [ ] **get_relevant_files live smoke** — now **#206**. Unit + lib green (4 tests in
  `get_relevant_files_tests.rs`, 4 in `get_relevant_files_runtime_tests.rs`), but never
  run against a real BYOP provider. Matters because the tool is intercepted by name and
  bypasses the protobuf executor, so no other integration coverage touches its path.
  Manual verification item — needs provider credentials.
- [x] **Vertex provider bugs** — DONE, on `main`. Empty-project silent-drop (`#99`) +
  8-field payload struct (`#100`), fixed on `fix/vertex-provider-bugs` (commit `a08b52777`)
  and **merged via PR #104** — verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`. `AgentProvider::validation_error()` and
  `ProviderEditFields` are present on `main`; issues #99/#100 are closed. Nothing to review
  or merge.

## Issue reconciliation status
- **#37** SSH ControlMaster guard — DONE and CLOSED. **Correction:** the previous
  line here said the `external_control_master` refinement was "still open" — it is
  not; it is plumbed DCS hook → session → controller and covered by tests
  (`owns_control_master_accessor_reflects_constructor`,
  `parse_dcs_ssh_with_external_control_master`). Both halves closed together.
- **#4** — still OPEN, but **PR #124 already repaired the deadlock** this entry
  describes (see Part 2 below) — do not read "#4 open" as "deadlock reproduces". The remaining
  scope is now understood as one symptom of a broader gap: `warp_tui` trails the
  pin by a generation, with #384/#387/#389/#390/#392/#395 tracing to the same root
  cause. Full map: **#456**.
- **#98/#99/#100/#101/#102** — all MERGED and CLOSED. (#102, the deferred
  `BufferConflictDetected` push, is now also done — see "Merged this session" above.)
- **#2** — tracking issue for the broader Warp-test-parity sweep; stays OPEN (this is
  now the umbrella for the much larger `SCOPE-*.md`-driven fleet effort — see
  `HANDOFF.md`, not this file, for its live numbers).
- **#5** — **CLOSED 2026-08-05, not open.** All 5 deferred low-severity findings were
  triaged: 1 fixed (mouse-wheel scroll reuse, in #78), 4 explicitly won't-fixed
  (deferred to `specs/tui-render-perf/SCOPE.md` or judged not worth doing standalone).
  The "Other outstanding" entry below previously said "5 latent items, all still
  present" — that was stale; see the correction there.
- **#11** — **CLOSED 2026-08-07.** Fully reconciled; see "#11 status" above.

### Closed 2026-08-06 late
#129 (mermaid flake) · #131 (MCP redaction gate) · #135 (PR lookup) · #137 (empty
branch dropdown over SSH) · #138 (watch filter) · #143 (Privacy page) · #145 (editor
parity) · #152 (`/usage` + `/cost`) · #156 (`PrInfo` fields) · #157 (gh-auth) ·
#185 (WSL paths) · #196 (WCAG chip labels).

### Deliberately left open — partially resolved, remainder is real
- **#126** — still OPEN, still real. BYOP commit-message gen: local path shipped
  (PR #130); #125 landed and the wiring still is not done —
  `maybe_start_commit_message_autogen` (`app/src/code_review/git_dialog/commit.rs:295`)
  calls `generate_for_local_repo` with **no `is_remote()` check**, so on an SSH repo
  it runs `git` against a path that does not exist locally and silently produces no
  draft (no toast — the empty editor is the only symptom). This is the same defect
  class as #188 (local diff-state model used on a path that may be remote); #126 is
  reportedly instance twelve of that class.
- **#136** — **DONE, closed 2026-08-07.** Fixed by PR #468, commit `5b83a8ee8`. Both
  halves verified: the local path's `local_read_files_result` now returns
  `Success { files, failed_files }` instead of discarding successful reads on any
  failure; the remote path threads `failed_files` through instead of flattening
  every failure into one hardcoded string. (The proto-field premise in the old
  entry was also wrong the other way — `AnyFilesSuccess.failed_reads` was already on
  the wire; `convert.rs` was just populating it with an empty vec.)
- **#142** — still OPEN (left to a maintainer to close/retitle), but nothing further
  to port: PR #189 and PR #227 are merged, `api_keys.rs`/`api_keys_tests.rs` carry
  the full ported/blocked/superseded/cloud breakdown, and `git grep CustomEndpoint`
  across the tree is empty. The "superseded by `AgentProviderSecrets`" decision is
  now recorded in `DECLINED.md` (PR #486) — see that file's "Divergences where the
  fork deliberately differs" section (`CustomEndpoint` row) rather than treating
  this as open parity work.
- **#146** — **DONE, closed 2026-08-07** (commit `ababc7f07`); see the remaining GUI
  surfaces item above, now also marked done.

### New issues filed 2026-08-06 late
#183, #184 (`warp_cli` gaps) — **both now closed as subsets**: #183 into #411
(`Harness::Codex` variant; the `config_name`/`from_config_name` half of #183 was
separately done, PR-added additively), #184 into #216 (see `local_control` above)
· #188 (3 more local-model-on-remote-path sites — still OPEN) ·
#191 (`.rustfmt.toml` pins edition 2018 while all 64 crates are 2024 — still OPEN) ·
#194 (BYOP token accounting was dead, which disabled auto-compaction — **now
CLOSED/fixed**) · #196 (closed).

---

## Red on main — 2026-08-06 (superseded, see correction)

`main` carried knowingly-failing tests as of 2026-08-06, per a maintainer decision to
consolidate all work onto one branch and fix afterwards. **That specific episode is
now resolved:**

- [x] **#171 — 9 ported Warp terminal tests fail** — **CLOSED; PR #224 repaired it**
  ("fix(terminal): repair the 9 ported Warp terminal regressions"), including
  both security-relevant fixes (the OSC 1337 parser panic and the unquoted
  `cat {history_file}`).
- [x] **`warpui` / `warpui_core` suite** — no longer a distinct red item. PR #181,
  which introduced it, was itself a test-porting pass that found and ported real
  gaps (word/punctuation selection expansion, hyperlink click-handling, one
  macOS-only font-identity file); nothing in the current comprehensive test count
  (see below) attributes failures to `warpui`/`warpui_core`.
- [x] **Baseline established** — but do not trust a number written here; it goes
  stale within a day at the current merge rate (194 commits landed between
  `HANDOFF.md`'s last snapshot and this reconciliation). **`HANDOFF.md` is the live
  source for `main`'s current red-test count** — as of its last rewrite: 4946 tests
  batch-run, 4935 passed, 11 failed, all attributed (5 deliberate — PR #259 pinning
  real `history_model` rewind/fork divergences #251/#253 rather than shipping
  unverified fixes; 6 from two PRs' uncompiled hunks in `vim_handler_tests.rs` and
  `app/src/terminal/input.rs`, not deliberate, need fixing). Re-read `HANDOFF.md`
  rather than updating a count here.

**Method note (still valid):** always state which commit a test count belongs to —
a branch number was once mistaken for `main`'s and made a clean proto re-pin look
like a 22-test loss.

---

# Part 2 — Code-review debt

Actionable items from the code reviews run on 2026-07-26 (and later). Grouped by review.
Each item notes `file:line`, the problem, and the suggested fix.

Consolidated here from the former lowercase `todo.md` on 2026-08-06. Every item was carried
over. Items re-verified against `main` during the consolidation and found already landed were
flipped to `- [x]` with the evidence inline — none were deleted. Note that several `file:line`
references below predate later refactors (e.g. `app/src/settings_view/about_page.rs` is now the
`about_page/` module); the original paths are kept as written so the findings stay traceable.

## warp_tui test suite health (found 2026-07-29, commits `5b2d600f`/`eaabdc36`)

Discovered while verifying the #328 fix + TUI allow/reject keybindings.
Confirmed via `git stash` that both issues below reproduce identically on
clean HEAD — pre-existing, unrelated to either of those changes. Not fixed
here to keep those changes scoped; `cargo build`/`cargo check` (the actual
release gates) are unaffected either way.

- [x] **`cargo test -p warp_tui --lib` deadlocks partway through a full serial run**
  — RESOLVED on `main` (PR #124, commit `87d06d179`, "fix(warp_tui): stop cargo test
  --lib deadlock"; verified 2026-08-06 with
  `git merge-base --is-ancestor 87d06d179 main`), which reworked
  `tui_generic_tool_call_view_tests.rs` and `test_fixtures.rs`.
  Hung at `tui_generic_tool_call_view::tests::accepting_new_conversation_suggestion_completes_the_executor`
  — 39 threads, all blocked in `futex_do_wait`, zero CPU progress for 20+
  minutes. Reproduced twice. Scoping the test filter away from this module
  avoided it (used for verification of the allow/reject work), so the full
  crate suite may simply have never been run to completion before.
  **NOTE:** only the *deadlock* is closed out here. The remaining `warp_tui`
  suite work — the 18 `nextest` failures — is tracked in Part 1 under
  **`#4 warp_tui suite`**, whose text predates PR #124.

- [x] **3 tests in `terminal_session_view_tests.rs` fail even run alone/serially**
  — RESOLVED on `main` (PR #73, commit `b7c6012ce`, "test(tui): drive footer/zero-state
  trio into AI input mode"; verified 2026-08-06 with
  `git merge-base --is-ancestor b7c6012ce main`), which made the implicit setup explicit
  per-test exactly as the fix below proposed. All three tests still exist in
  `crates/warp_tui/src/terminal_session_view_tests.rs`.
  `agent_hint_tracks_transcript_emptiness_without_input_invalidation`,
  `footer_conversations_callout_no_longer_renders`,
  `footer_model_label_is_a_bounded_click_target` — all failed with a
  default/empty-looking footer ("shell mode", "No custom provider
  configured" not found) even filtered down to just this one test module
  with `--test-threads=1`, meaning they depended on setup that normally
  happened as a side effect of some other test module running first in the
  full suite — not truly hermetic.
  **Fix (applied):** find whatever global/singleton setup they implicitly depend on
  and make it explicit per-test (matching the pattern already used to fix
  `warp`'s own historical test-isolation issues — see settings.toml
  hermetic-path fix, ssh-onekey singleton, etc.).

---

## Follow-up code-review fixes (2026-07-29, commit `fddc193a`)

Dev machine is Linux; nothing below has been run against a real Windows
`pwsh.exe`. Verified only via `cargo check`/`cargo test` (static + unit-level).

- [ ] **NEEDS WINDOWS VERIFICATION: pwsh `-EncodedCommand` at 2 more call sites**
  — `app/src/terminal/model/session/command_executor/local_command_executor.rs:55`,
  `app/src/terminal/model/session/command_executor/msys2_command_executor.rs:67`
  Ported the same fix as the interactive-session-launch site (`shell.rs`,
  commit `5365c62a`) to `LocalCommandExecutor`'s generator/login-shell command
  path and `MSYS2CommandExecutor`'s Windows-native-shell path, both of which
  built `pwsh ... -c <command>` as a plain string — open to the same PS 7.6
  `-Command` quoting-parser crash on any command containing a `"`. Shared the
  encode logic into `util::encode_pwsh_command`.
  Regression tests (`encode_pwsh_command_round_trips_without_trailing_nul` in
  `util/mod.rs`, plus the existing `shell_tests.rs` one) only check the
  base64/UTF-16LE encoding itself round-trips correctly — they don't spawn a
  real `pwsh.exe` and confirm it accepts the argv or that a generator command
  containing a quote actually executes.
  **To verify:** on a Windows box with PowerShell 7.6, run a generator/BYOP
  local command whose text contains a `"` (e.g. a quoted path) through both
  executors and confirm it executes instead of erroring; also sanity-check a
  plain no-quotes command still works end-to-end (stdout/exit code correct).

---

## Security / performance audit — non-Warp code (2026-07-26)

Parallel audit (6 agents) of the fork's own code (fork-specific additions + newer work;
boundary = Warp merge-base `c325d146`). Upstream Warp treated as trusted/out of
scope. Ranked most-actionable first. No CRITICAL/HIGH security issues; **crash
sweep found zero reachable panics** (BYOP/AI stack is well-hardened). Duplicates
across agents have been merged.

> **Scope note (2026-08-06):** Track 3 (merge `fad390189`, on `main`) deleted
> `app/src/ssh_manager/`, `crates/warp_ssh_manager`, `crates/zap_sync` and
> `crates/zap_sftp` outright. Every finding below that names one of those paths is
> therefore moot as live code — the fixes are recorded for history, and the one
> remaining follow-up they carried (decoupling the cloud-sync DEK from the PAT) has
> no code left to apply to.

### Security

- [x] **[MED] SSH-sync payload integrity → RCE-on-connect** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  Now seals the entire `SshSyncData` in a single AES-GCM envelope (`seal_payload`
  / `unseal_payload`, v2 format), so every field — `host`, `key_path`,
  `startup_command`, `notes`, node structure — is covered by the GCM auth tag.
  Tampered payloads fail authentication and are rejected; legacy v1
  (unauthenticated) payloads are refused with a "re-upload to upgrade" message.
  Tests: `seal_roundtrip_*`, `tampered_sealed_payload_is_rejected`,
  `legacy_unauthenticated_payload_is_rejected`.
  *(original location note: sync_provider.rs:174,332)*
  On download, only the encrypted secret fields were authenticated; `host`,
  `key_path`, and `startup_command` came from the gist JSON integrity-unprotected,
  and `startup_command` was written verbatim to the PTY on connect. A tampered gist
  (writable with a `gist`-scoped or leaked token that can't read the encrypted
  blob) → command execution on connect, or connection/key redirect.
  **Fix:** authenticate the whole payload (HMAC/sign all fields, or wrap the entire
  JSON in the AES-GCM envelope), and confirm changes pulled from sync on apply.

- [x] **[MED] SSH destination argument injection (leading-dash host → local RCE)** — FIXED
  — `crates/warp_ssh_manager/src/ssh_command.rs` *(crate since removed by Track 3)*
  Added a `--` option terminator before the destination in all three argv paths
  (`build_ssh_args`, `test_key_auth`, `build_password_auth_cmd_args`) via a shared
  `push_destination` helper, so a `-o…` host/username can't be parsed as an ssh
  option. Regression tests: `build_ssh_args_guards_leading_dash_host`,
  `password_auth_args_guard_leading_dash_host`.
  *(original location note: ssh_command.rs:50-55)*
  (also `test_key_auth:118`, password path `:307-309`, PTY `build_ssh_command_line:59-65`)
  The `host` / `user@host` target was appended as the final `ssh` argv with no `--`
  separator. A host beginning with `-` (e.g. `-oProxyCommand=touch /tmp/pwned`)
  is parsed as an option → local command execution before any connection.
  `shell_escape` does NOT neutralize a leading-dash flag. Self-inflicted today
  (own config), but reachable if a host was ever imported/synced from `~/.ssh/config`
  or a shared profile.
  **Fix:** insert a literal `--` before the target in all four paths; reject
  host/username values starting with `-`.

- [x] **[MED] Cloud-sync key is unsalted, token-coupled, not a real KDF** — PARTIALLY FIXED,
  remainder now MOOT (crate removed by Track 3)
  — `crates/zap_sync/src/crypto.rs` *(crate since removed by Track 3)*
  Replaced `SHA256(SHA256(token))` with **Argon2id** over a random 16-byte
  per-message salt (embedded in the blob as `salt || nonce || ciphertext`). This
  closed the "not a real KDF / unsalted / brute-forceable low-entropy token"
  weakness; API unchanged so all callers were untouched. It remained **token-derived**
  (not decoupled from gist access) — full decoupling would have needed an independent
  user passphrase (larger UX change), left as a follow-up. That follow-up is now moot:
  there is no `zap_sync` crate on `main`.
  The AES-256-GCM key was `SHA256(SHA256(PAT))` — derived from the same GitHub/Gitee
  token that also fetched the ciphertext gist, with no salt/work factor/domain
  separation. Token compromise yielded both ciphertext and key; low-entropy
  (self-hosted/Gitee/custom) tokens became brute-forceable against the public gist.
  **Fix:** derive the DEK from an independent user passphrase (or a random per-user
  key kept only in the OS keychain, never uploaded) via Argon2id + stored random
  salt. **Availability footgun:** rotating the PAT silently made all synced data
  undecryptable — document it.

- [x] **[LOW] `http://` provider base_url sends the API key as cleartext Bearer**
  — RESOLVED on `main` (PR #114, commit `74e365635`; verified 2026-08-06 with
  `git merge-base --is-ancestor 74e365635 main`)
  — `app/src/ai/agent_providers/openai_compatible.rs:61` (and `chat_stream.rs`
  `normalize_endpoint_url:3344`)
  `http://` was permitted and `Authorization: Bearer <key>` was attached anyway.
  Intended for local Ollama, but a plaintext/MITM'd provider leaked the key.
  **Fix (applied):** `is_loopback_host` / the cleartext-risk check in
  `app/src/ai/agent_providers/mod.rs` now gate the bearer to `https://` or a
  loopback `http://` endpoint; `chat_stream.rs` strips the key and warns otherwise.
  The gate matches on the literal host (`localhost`, `127.0.0.0/8`, `::1`) rather
  than on DNS resolution, since a name that merely resolves to loopback today can
  be repointed tomorrow.

- [x] **[LOW] Unbounded response/stream reads (DoS)** — ACCEPTED RISK (stock upstream, unfixed upstream; documented in SECURITY.md)
  — `lib/rust-genai/src/webc/web_client.rs:113,128`, `models_dev.rs:254`
  (`res.text()`/`bytes()` with no cap) and `web_stream.rs:~168` (SSE
  `partial_message` grows unbounded if the delimiter never arrives).
  A malicious/compromised provider endpoint can OOM the client. (gzip is off, so
  not a decompression bomb — just raw size.)
  **Fix:** size-limited streamed reads; cap the SSE buffer and error past a limit.

- [x] **[LOW] SSH sync uploads structural fields to the gist in plaintext** — RESOLVED (by the v2 seal)
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  Mooted by the payload-integrity fix: the whole `SshSyncData` (host, username,
  port, startup_command, notes, key_path, node tree) went inside the v2 AES-GCM
  seal, so nothing structural was on the wire in plaintext anymore.

- [x] **[LOW] Bearer token forwarded to `raw_url` taken from response JSON** — FIXED
  — `crates/zap_sync/src/gist_client.rs` *(crate since removed by Track 3)*
  The truncated-gist path only attached the `Authorization` header when
  `raw_url_is_trusted(platform, raw_url)` — HTTPS + a per-platform content-host
  allowlist (`gist.githubusercontent.com` etc. for GitHub, `*.gitee.com` for
  Gitee). A tampered `raw_url` was fetched without credentials, so the token
  couldn't be exfiltrated. Tests: `raw_url_trusted_*`, `raw_url_rejected_*`.

- [x] **[LOW] Decrypted secrets held in non-zeroized `String`** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  `PendingSecret.value` became `Zeroizing<String>` and both per-field decrypts were
  wrapped in `Zeroizing::new(...)`, so decrypted passwords/passphrases were zeroed
  on drop after being written to the keychain — consistent with
  `WrittenSecret.prior_value`.

- [x] **[LOW] SSRF IPv4-compatible IPv6 gap** — FIXED (to_ipv4 covers ::a.b.c.d); WASM DNS-filter gap noted (cloud target only)
  — `app/src/ai/agent_providers/tools/web_runtime.rs:110-155`
  `is_blocked_ip` handled `::ffff:a.b.c.d` but not the deprecated `::a.b.c.d`
  form; `SsrfSafeResolver` is `cfg(not(wasm32))` so the WASM build only checks IP
  literals. Marginal on desktop; noted for completeness.
  **Fix:** also reject embedded-IPv4 IPv6 / `v6.to_ipv4()`; document the WASM gap.

- [x] **[LOW] Defense-in-depth: unvalidated inputs to sensitive sinks**
  — RESOLVED on `main`, all three sub-parts:
  - `vertex_auth.rs:89` — gcloud `--impersonate-service-account` SA email was only
    checked non-empty (argv-safe, no injection, but wanted an email format check).
    **Done:** `is_plausible_service_account_email` now gates the flag (PR #114,
    commit `74e365635`), with unit tests.
  - `app/src/ssh_manager/su_password_injector.rs` + `secret_injector.rs:107` — raw
    secret + `\n` written to PTY, so an embedded newline injected trailing bytes as
    commands. **Moot:** the whole `app/src/ssh_manager/` directory was deleted by
    Track 3 (merge `fad390189`, on `main`); there are no injectors left in the tree.
  - prompt custom-file loader `prompt_renderer.rs:278` — blocked `..`/absolute but
    followed symlinks out of the dir. **Done:** `canonicalize` + `starts_with`
    containment on both loader paths (PR #114), with a regression test.

### Performance (new TUI rendering — all HIGH, same trigger: per-streamed-chunk / per-frame)

- [x] **[HIGH] `sync_code_block_views` reclones every code block each streamed chunk** — FIXED
  — `crates/warp_tui/src/agent_block.rs`
  The reconciler now compares the borrowed `&str` against the retained view's
  content (`TuiCodeBlockView::matches`) and only clones new/changed sections (in
  practice just the streaming block). `sync()` already no-ops on an equal payload,
  so this elides only redundant allocation. Verified: builds; code_block (8) +
  agent_block (51) tests pass.

- [x] **[HIGH] `sync_action_views` re-clones actions each chunk** — FIXED (matches-skip for shell+plan; plan re-resolves presentation to catch model state). Commit e77659f7
  — `crates/warp_tui/src/agent_block.rs:498-541`
  Same trigger; cloned every plan/shell/generic action every chunk.
  **Analysis:** *Shell* is safe to skip-when-unchanged — `update_action` is a pure
  function of `(action, output_streaming)` and shell action payloads are small
  (just the command string; live output is reactive from `terminal_model`), so the
  payoff is small. *Plan* (`CreateDocuments`/`EditDocuments`, the larger payloads)
  is NOT safe to skip: `sync_action` → `sync_documents` re-resolves per-document
  state from `action_model`, which changes independently of the action. A correct
  plan fix needs to fold that model-derived state into the change key.
  **Recommend:** do plan properly with a running-TUI check + a streaming snapshot
  test; shell-only is low value.

- [ ] **[HIGH] Full-document rebuild on every layout pass, not viewport-gated** — now **#203**. — NEEDS REFACTOR (deferred)
  — `crates/warp_tui/src/editor_element.rs:351-401` (`build`) +
  `crates/editor/src/render/model/char_cell_display.rs:257-334` (`display_rows`)
  `layout()` unconditionally rebuilds: `text.chars().collect()` + a full-buffer
  `display_lattice` walk even when `with_viewport_rows` is set; any animated
  element (shimmer, ~10 Hz) re-layouts the whole retained tree.
  **Analysis:** `build()` can't be memoized wholesale — it has essential
  per-layout side effects (`try_layout_pending_edits`, scroll clamp/follow_cursor,
  `set_terminal_width`); skipping it breaks editing/scroll. The real fix is to
  separate the pure projection from the side effects and/or make `display_lattice`
  viewport-windowed in shared `crates/editor` code — an intricate change that
  **must** be verified in a running TUI. Deferred to a focused, harness-backed
  session rather than shipped blind.

### INFO / noted (not action items)

- Linux `secure_storage` fallback uses a hardcoded embedded key
  (`secure_storage/linux.rs:95-113`) → fallback files are effectively plaintext.
  This is **upstream Warp** code, but the fork now routes far more sensitive
  secrets through it (BYOP API keys, proxy password) on
  headless-Linux/WSL/no-Secret-Service boxes, amplifying blast radius. Escalate
  upstream or override in the fork. (The cloud-sync PAT and SSH passwords that
  originally widened this blast radius are gone with Track 3.)
- genai logs full response bodies at `tracing::trace` (no secrets/`Authorization`).
- LLM file tools (`tools/files.rs`, `edit.rs`) add no extra sandboxing beyond
  upstream's executor + block-UI approval.
- **Crash sweep: 0 findings.** BYOP/AI stack uses checked slicing, `saturating_sub`,
  `.get()`, `from_utf8_lossy`, `to_ascii_lowercase`, division-by-zero guards
  throughout; one `crates/editor` diff is itself a panic fix.

---

## About page + Phosphor theme (commits `41a77348`, `472a339b`)

- [x] **Search terms advertise now-hidden autoupdate controls** — FIXED (trimmed)
  — `app/src/settings_view/about_page.rs:138` (now `about_page/mod.rs:152`)
  `search_terms` still listed "automatic updates auto update check for updates
  new version", but `SHOW_AUTOUPDATE_UI = false` hides those controls. Settings
  search for "automatic updates" led to the About page with no such control.
  **Fix:** trim the autoupdate vocabulary from `search_terms` while the UI is
  hidden.

- [x] **JPEG logo: opaque background + baked-in text, illegible at ~100px** — FIXED, **#204**.
  — `app/src/settings_view/about_page.rs:187` (now `about_page/mod.rs:167`)
  The 1024×1024 badge was downscaled to ~100px (its "PHOSPHOR TERMLNK / CRT
  TERMINAL" lettering became noise), and being an opaque JPEG it rendered as a
  dark box on a light-themed About page.
  **Fix:** point the About page at the existing vector Phosphor mark
  (`app/channels/oss/icon/AppIcon.icon/Assets/logo.svg`, already the source of
  truth for the app icon) copied to `bundled/svg/phosphor-logo.svg` — no baked
  text, alpha background, crisp at any size. The old
  `bundled/jpg/phosphor-logo.jpeg` (unused elsewhere) was removed; the
  README/marketing badge at repo-root `assets/phosphor-logo.jpeg` is untouched.

- [x] **Autoupdate observer now gated** — FIXED (subscribe only when SHOW_AUTOUPDATE_UI)
  — `app/src/settings_view/about_page.rs:61` (now `about_page/mod.rs:72`)
  `new()` still subscribed to `AutoupdateState` (`ctx.observe(... ctx.notify())`)
  and all autoupdate `handle_action` arms remained. While disabled, any autoupdate
  stage change re-rendered the About page for no visible effect; the controller
  half was left half-wired.
  **Fix:** gate the subscription alongside the render (ideally derive the flag
  from real release-channel availability).

- [x] **~200 lines reachable only via the const-false branch** — RESOLVED via the
  "extract so it's clearly parked" option
  — `app/src/settings_view/about_page.rs:303`
  `render_update_status` + `UpdateAction` + `format_bytes` +
  `format_download_progress` were only reachable through
  `SHOW_AUTOUPDATE_UI` (compile-time `false`). Deliberate/reversible, but the
  dead branch would bit-rot and was untested while disabled.
  **Fix (applied):** they now live in their own module,
  `app/src/settings_view/about_page/autoupdate_ui.rs`, whose header documents it as
  "**Parked, not wired up.**" and states that re-enabling means flipping
  `SHOW_AUTOUPDATE_UI` back to `true`; `autoupdate_ui_tests.rs` covers `format_bytes`
  and `format_download_progress` so the parked code no longer rots untested.

- [x] **Amber theme duplicated in Rust const + yaml, hand-synced** — RESOLVED via the
  "add a test asserting the two stay in sync" option
  — `themes/phosphor_amber.yaml:24`
  Phosphor Amber is defined twice — the bundled Rust `AnsiColors` const (the
  actual default) and this copy-in yaml — with no shared source. The change that
  raised this had to edit identical blue/cyan values in both, and nothing prevented
  future drift.
  **Fix (applied):** `app/src/themes/default_themes_tests.rs` now has
  `phosphor_amber_yaml_matches_builtin_theme` (and a green counterpart) asserting the
  YAML round-trips to exactly the built-in `WarpTheme`, with a failure message telling
  you to re-sync. The two are still hand-synced duplicates by design — the test is the
  guard rail.

---

## Vertex AI provider (merge `fae32e14`)

- [x] **Empty project builds a malformed URL + silent picker drop** — RESOLVED on `main`
  (issue `#99`, commit `a08b52777`, PR #104; verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`)
  — `app/src/settings/ai.rs:924`
  There was no save-time validation, so a Vertex provider could be saved with an empty
  project. `build_byop_llm_infos` (`mod.rs:83`) then silently skipped it (models
  never appeared, no feedback), and `vertex_endpoint_url("", "global")` yielded
  `.../projects//locations/global/` if any path resolved it.
  **Fix (applied):** `AgentProvider::validation_error()` rejects a Vertex provider with
  an empty `vertex_project` at save time and `save_agent_provider_edits` surfaces it as
  an error toast.

- [x] **Vertex location not case-normalized** — FIXED (vertex_endpoint_url lowercases location)
  — `app/src/settings/ai.rs:927`
  The `location == "global"` check was case-sensitive and the raw location was
  interpolated into the hostname, so "Global" → `Global-aiplatform...` and
  "US-EAST5" → `US-EAST5-aiplatform...` — both invalid hosts.
  **Fix:** `location.to_ascii_lowercase()` before the global check and host
  interpolation.

- [x] **Cold-start token mint has no in-flight coalescing** — FIXED (MINT_LOCK single-flight)
  — `app/src/ai/agent_providers/vertex_auth.rs:47`
  On a cold cache, concurrent first requests (main stream + title gen +
  active-AI) each missed and spawned their own `gcloud auth print-access-token`
  subprocess.
  **Fix:** single-flight the mint per credential (per-credential async lock or
  in-flight map) so only one `gcloud` runs.

- [x] **8-field positional provider-edit payload duplicated ~4×** — RESOLVED on `main`
  (issue `#100`, commit `a08b52777`, PR #104; verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`)
  — `app/src/settings_view/ai_page.rs:2425`
  `SaveAgentProviderEdits` / `SaveAgentProviderEditsThen` / the
  `to_save_action_with` closure type / `save_agent_provider_edits` all carried the
  same 8 positional fields, kept in lockstep by hand (needing
  `#[allow(clippy::too_many_arguments)]`). A mismatched order silently swapped
  values.
  **Fix (applied):** collapsed into a single `ProviderEditFields` struct passed by
  value (now in `app/src/settings_view/ai_page.rs` +
  `app/src/settings_view/agent_providers_widget.rs`).

- [x] **Vertex family routing duplicated** — FIXED (shared vertex_model_family())
  — `app/src/ai/agent_providers/reasoning.rs:100`
  (and `app/src/ai/agent_providers/attachment_caps.rs:225`)
  The `contains("claude") ? Anthropic : Gemini` dispatch was implemented verbatim
  in both; a change to the heuristic had to touch both or the surfaces disagreed.
  **Fix:** extract `fn vertex_model_family(model_id: &str) -> AgentProviderApiType`
  and call it from both.

---

## warp-oss-sync / TUI port (range `ab207e20..7accb626`)

Scale: ~150 commits, 20k+ lines across 207 shared files (plus the isolated
`warp_tui` crate + test churn). Too large for a faithful inline line-by-line
pass — run `/code-review ultra josh/warp-oss-sync` for full coverage.

A **focused GUI-regression review of the two biggest GUI-facing keystones** was
done inline and both came back **clean**:

- [x] **View→Entity relaxation + `tui_views` routing** (`core/view/context.rs`,
  `core/view/handle.rs`) — GUI-safe. All `T: View` → `T: Entity` changes are
  widenings (`View: Entity`), method bodies unchanged; the `tui_views` fallback
  in `WeakViewHandle::upgrade` (and view/try_view/update_view) is
  `#[cfg(feature = "tui")]`-gated, so GUI builds behave identically. The change
  also fixes a latent bug where weak handles to TUI views failed to upgrade.
- [x] **`TerminalManager<S>` genericization** (`terminal/local_tty/terminal_manager.rs`)
  — structurally sound. GUI wiring stays in a concrete `impl
  TerminalManager<TerminalView>`; the generic `impl<S>` path is additive; GUI
  downcast site (`pane_group/mod.rs:2314`) is consistent. Full line-by-line of
  the 1079-line body extraction was not done (defer to ultra); the green test
  suite covers terminal behavior.

Reviewed 2026-07-26 (the three previously-unreviewed files) — all CLEAN:
- [x] `crates/warpui_core/src/core/app.rs` — GUI-safe. Same shape as the cleared
  keystones: View→Entity widenings, `tui_views` fallbacks all `#[cfg(feature =
  "tui")]`-gated (compiled out of GUI builds; the GUI `views` map is always
  checked first with unchanged behavior), and the `&mut dyn Any` downcast
  refactor is consistent throughout. No regression.
- [x] `crates/editor/src/render/model/mod.rs` — char-cell render model, no
  reachable panic: `opportunities` is sized `count+1` (never empty), row/char
  indexing relies on sentinel invariants that are `debug_assert`-checked and
  maintained by `rebuild`, byte-offset math is `.min()`/`.get()`/`saturating_sub`
  clamped. Internal-invariant-guarded, not untrusted-triggerable.
- [x] `app/src/ai/agent_providers/prompt_renderer.rs` — no SSTI: templates are
  pre-registered by name; LLM/user values flow in only as context DATA
  (`Value::from_serialize`), never compiled as templates. minijinja is sandboxed
  (no eval/shell/fs from templates), no `render_str`, no command exec.
  `custom_prompt_raw` blocks absolute/`..` paths (input is user config, not LLM).
  Only residual: symlink-follow — since fixed, see the defense-in-depth item above.
