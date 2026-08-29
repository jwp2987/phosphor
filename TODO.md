# TODO — Phosphor: Warp parity ledger (#11) + code-review debt

**Checkbox key:** `- [ ]` open · `- [>]` **IN FLIGHT, agent assigned** · `- [~]` partial · `- [x]` done.
Added 2026-08-10 after a status report listed four in-flight items as unstarted:
the assignment lived in the operator's head and not in this file. **Record the
assignment here when you start work, not when you finish it.**

## RE-PIN SCOPING ROUND 2026-08-29 — `42effe840` -> `4111d08f9` (CANDIDATE pin)

**This is scope, not work done. Nothing here was compiled.** A 10-agent fleet
classified all 171 commits in the range by reading diffs and grepping the fork;
no `cargo` of any kind was run, so every verdict below is a reading, not a
verified result. Treat each as a hypothesis strong enough to plan against and
weak enough to re-check before you act on it.

**Pin identity** (both verified `git cat-file -t` = `commit`):

| | commit | date |
|---|---|---|
| old (current oracle) | `42effe840` | 2026-08-11 17:51 -0700 |
| new (candidate) | `4111d08f9` | 2026-08-26 04:48 +0000 |

`4111d08f9` is the **dated** cut for the `2026.08.26` stable, not a tag — tag
publication stopped after 2026-06-09, so this is Phase 1's documented
approximation. Confirm it against the real release build's version string
before recording it in `ORACLE.md`. The intervening `2026.08.19` stable sits
inside the range at commit 73, so nothing is skipped by pinning straight to
`08.26`.

Range is a clean linear ancestry (`merge-base --is-ancestor` = yes), 0 merge
commits, 0 empty commits, `--first-parent` count == full count == 171. Shards
partition it exactly: every commit assigned once, none orphaned.

### FILE MOVE `21f413b7` — PARTIAL. The full boundary is blocked BY THE TMUX DIVERGENCE.

Three safe, independent commits landed (22 files, +429/-394): `completions`/`iterm_image`/
`kitty` into `warp_terminal::model`, `Side`/`Direction` into `warp_terminal::model::indexing`,
and executable-path resolution into `warp_util`. The agent then **stopped rather than
reshape a divergence**, which is what it was told to do and the right call.

#### The blocker — coordinator-verified

`app/src/terminal/model/ansi/mod.rs` IS in the move set, and in this fork it does
`use crate::terminal::model::tmux::{…}` (`:33`) and `::ControlModeEvent` (`:39`).
**Upstream has zero `terminal/model/tmux` files** — that directory is fork-only, retained
under `DECLINED.md:218` (#322) because this fork keeps the SSH tmux wrapper that the pin
deleted.

So moving `ansi` into `warp_terminal` forces a choice: relocate fork-only `model/tmux` into
`warp_terminal` (a layout upstream does not have, i.e. **inventing** a divergence rather
than adopting parity — and it drags `terminal::event` and `util::{AsciiDebug, parse_ascii_u32}`
with it), or leave `ansi` in `app` and strand everything above it.

**The terminal crate boundary cannot be fully adopted while the tmux wrapper is kept,
without first deciding where fork-only tmux code lives in the new layout.** That is a
maintainer decision and it was not visible before this attempt.

Everything in `model/**` is one atomic unit gated on that, plus the `SizeInfo`/`Vector2F`
extraction, the `event`/`event_listener` split, and `secrets`/`session`/`block_filter`.
Separately, `model/grid`'s internals are private/`pub(super)` with consumers that cannot
move — moving them alone would mean widening visibility past upstream, a deviation.

**Also newly known: this fork's `warp_terminal` is behind upstream's PARENT commit.**
Upstream's pre-move `Cargo.toml` already had `warp_errors`, `warpui_core` and
`session-sharing-protocol`; the fork has none (no `crates/warp_errors` exists; it deps
`warpui`, not `warpui_core`) and its `lib.rs` lacks `mod shared_session`. Full adoption
must close that gap first.

#### SEVENTH COORDINATOR ERROR — three wrong facts in one brief

- **"`local_tty/terminal_manager.rs` IS in the move set"** — false. It is **`M`**, modified
  in place, never renamed. It does not move, and the `use_ssh_tmux_wrapper` call site I
  warned about was never at risk. All six call sites are untouched by this commit.
- **"49 removed / 43 exist here / 6 fork-only"** — false. Reality: **73 deletions**, 72 of
  which exist here, **0 fork-only**; the single absentee is
  `model/ansi/dcs_hooks_tests.rs`. So there was no fork-only decision to make.
- **"96 files added under `crates/`"** — false, it is **83**.

Cause: I measured with `git show --name-only`, which lists every path in the commit
including modifications, then filtered by prefix — conflating deletes with modifies. The
agent used `--name-status --no-renames`. **Use `--name-status` when the question is what
moved.**

#### Open risk for the build

`Cargo.lock` was **not** regenerated (agents cannot run cargo). A `--locked` CI job fails
until it is. The agent's own least-confident list is led by
`crates/warp_util/src/path/windows.rs` — **`cfg(windows)`-only, so a Linux build cannot
check it at all**; it will pass here and break Windows CI if wrong.

### TEST-PORT WAVE — 3 launched, 1 landed, 1 refused, 1 pending refutation

**`port/tests-orphan-icon` (`a9552f503`) — landed, under refutation.** The one cluster
needing no production port. Rather than copy the pin's predicate into a new file (no
caller, `check_stub_coverage` bait), the porter **extracted the decision out of the live
`action_icon`** in `block/view_impl/output.rs` and tested the shipped path. It landed
**five tests from the pin's four**: the pin's card predicate is `Cancelled | Failed`
while `action_icon` keys off `!is_streaming()`, so `Complete` orphans here too — and it
established the pin's own `action_icon` is byte-identical to the fork's, making that a
divergence between **the pin's two predicates**, not pin-vs-fork. It split the test
rather than weaken it. Under refutation, including a claim that
`block/model/debug_model_impl.rs` is dead code that would not compile.

**`port/tests-exit-commit` (`d24a50d84`) — the agent REFUSED to port, and was right.**

#### SIXTH COORDINATOR ERROR — and this one contradicted my own ledger row

My brief asserted "nothing in this repo currently tracks this work — `exit_commit_handle`
has zero hits in `TODO.md`." **The symbol has zero hits; the work has a row.**
`TODO.md:1280` carries `60d602df6` marked "**BLOCKED, not schedulable yet**", naming the
same missing host function — **a row I wrote earlier in this same session.** I searched by
symbol name; the ledger tracks by upstream sha.

**Lesson, and it generalises:** grep the ledger by **sha**, not by symbol. A symbol that
does not exist in the fork cannot appear in a row describing why it does not exist.

The agent then established the deeper reason the work is not schedulable:
**`OrchestrationEventService` is an island.** Coordinator-verified: **zero** registrations
in `app/src/lib.rs` (the pin registers it), and it appears in exactly two files — itself
and its tests. `enqueue_event_batch`, `drain_events_for_request`, `requeue_awaiting_events`
have no production caller anywhere. **So the race these tests guard cannot occur here**,
and 5 of the 7 tests are not even writable (they need `emit_*_for_test`,
`skip_initial_turn`, `run_conversation_id` — all absent).

It committed a ledger correction instead, widening the BLOCKED row from the 2 pieces it
named to **8 measured**, and recording two traps for whoever unblocks it:
- The fork's `conversation_id_cell` (`driver.rs:1511`) holds the **server conversation
  token string**, not the `AIConversationId` that `on_commit` must seed. Silent
  wrong-value trap.
- The pin's `IdleTimeoutSender` grew `on_commit`/`IdleWait` **alongside** the
  failure-side `arm_refreshable`/`refresh`/`idle_window_for_terminal_status`. Taking the
  struct wholesale **re-imports the declined half**.

**Standing decision: `60d602df6` stays BLOCKED.** It unblocks with the
orchestration-consumer increment, not before. Cluster A and shard E's
`drop_pending_events_...` are the **same work item** as `60d602df6` — not three.

### ACCOUNTING CORRECTION — the test debt and the port queue are mostly THE SAME WORK

An earlier revision of this file reported "52 commits **plus** 60 tests of real work."
**That double-counted.** Coordinator-verified: **all nine blocked test clusters map to
commits already in the port queue** —

| test cluster | tests | queue commit |
|---|---|---|
| TUI disconnect resilience | 5 | `ee351a0e7` |
| shared recovery budget | 6 | `1a29f680d` (PARTIAL) |
| right-click paste | 6 | `c25ac4070` |
| `#` AI-search trigger | 3 | `94daf47f3` |
| TUI focus ownership | 7 | `69254d73` |
| Ctrl-C cancel window | 2 | `9921300b7` + `6696954c` |
| tab shortcut hints | 1 | `8b88df98` |
| settings empty-category | 2 | `3a7a4a5b3` |
| `is_passive_conversation` | 3 | `63a17a50a` |

These tests are not blocked by something else — they are blocked by **the exact commits
already counted**. Porting a queued commit **includes porting its tests**; that is what a
complete port is. This round already proved the cost of the opposite: the coordinator
told the grep porter that `fbbfc41f3`'s own 25 tests were "a separate work item", and
following that would have shipped a two-layer shell-quoting surface with zero coverage.

**Rule for this and every future round: a port-queue row is not done until its tests
land with it. Do not schedule the tests as separate work.**

#### Genuinely additional work — ~18 tests, no queue commit covers them

- **`classify_family_event` (7 tests)** — the parent/child demultiplexer for the agent
  event stream. Needs ~60 lines of production (`classify_family_event`, `enum FamilyEvent`,
  `lifecycle_event_type_from_wire`, two constants). The fork already defines
  `AgentRunEvent` and open-codes one arm by hand.
- **exit-commit ordering (6 tests)** — `exit_commit_handle` has **0 hits in TODO.md**;
  nothing tracks it. Needs `IdleTimeoutSender::with_on_commit` plus three
  `OrchestrationEventService` methods.
- **`drop_pending_events_for_exiting_conversation` (1)** — a 12-line `HashMap::remove`.
  Overlaps the exit-commit cluster; port together.
- **4 `is_orphaned_by_finished_output` tests** — the only cluster needing **no production
  port at all**: the `action_icon` mirror is already live at
  `block/view_impl/output.rs:3175` with zero coverage.

### TEST PORT ROUND — 9 shards launched 2026-08-29

**The work-list was re-derived mechanically rather than taken from the prose.** Every
one of the 228 queue entries was joined against the set of every `fn` defined in the
tree today, then filtered by staged verdict:

| | count |
|---|---|
| queue entries | 228 |
| already present in the fork by exact name | 10 |
| absent, and carrying a disposition that means do-not-port (DECLINED 42 / CLOUD 15 / COVERED-ELSEWHERE 7 / NOT-A-TEST 1) | 65 |
| **candidate port set** | **153** |

**114 of the 153 carry NO staged verdict.** Shards A, B and E reported inline and never
wrote their TSVs, so only C (69) and D (35) staged rows — 104 of the 128 the prose
claims. For those 114 this round is the *first adjudication of record*, not a re-check.
That is the real reason the number is larger than "60 tests of real work": the 60 counted
only what had already been adjudicated as portable.

Shards, each with its own worktree and branch `port/t-<shard>`, none over 29 tests:

| shard | n | content |
|---|---|---|
| S1 settings-cli | 12 | empty-category guard (7), slug migration (3, decision), ADD_MCP half, `WARP_*` aliases |
| S2 rightclick-ctrlc | 8 | right-click-paste setting (new at this pin) + ctrl-C cancel window |
| S3 passive | 7 | `#` trigger gate (live defect) + `is_passive_conversation` |
| S4 tui-disconnect-focus | 12 | **highest-value**: driver disconnect resilience + focus ownership |
| S5 sdk-environment | 20 | repo head overrides / blobless clone + 3 path-traversal guards |
| S6 sdk-driver-retry | 20 | exit-commit (BLOCKED, confirm), installation id, retry classifier |
| S7 blocklist | 28 | shared recovery budget (REMOTE-2269), streamer drain, autoexecute denylist |
| S8 misc | 17 | artifacts, skills filtering, cost accounting, project-context coalescing |
| S9 cloud-confirm | 29 | **refutation shard** — assume portable, prove otherwise |

**S9 is a refutation shard, not a porting shard.** The previous not-portable pass was
wrong 27% of the time (12 of 51 overturned), every failure tracing to a file-name or
symbol-name match against a declined family without reading the body. S9 re-attacks the
29 that still look cloud-shaped on exactly that suspicion.

Standing orders unchanged and restated in `BRIEF-COMMON.md`, which every agent reads
before its shard brief: **no agent compiles anything** (12c/22GB laptop; the coordinator
is the sole builder), no test is ever weakened to go green, no PRs, and every agent is
told to **refute its own brief** — coordinator briefs have been wrong seven times this
round and every one was caught by the agent, not by me.

#### Results — S1, S5, S8 (3 of 9 in)

| shard | ported | covered-elsewhere | not-portable | blocked |
|---|---|---|---|---|
| S1 settings-cli (12) | **3** (+1 prod fix) | 1 | 8 | — |
| S5 sdk-environment (20) | **2** (adapted) | 1 | 17 | — |
| S8 misc (17) | **2** (adapted) | 3 | 11 | 1 |

**The briefs were wrong again — 9 claims across 3 shards, every one caught by the agent.**
The pattern is now unmistakable: my shard lists were generated from an adjudication pass
that **pre-dates the maintainer's 2026-08-29 decline block**, so I briefed agents to port
work that had been declined the previous day.

- **`a18026275` (trailing-element infra) was declined** — I called its 5 tests "the real
  work here". They do not even compile against this fork: there is no `CategoryHeader`, no
  `PageTitle`, no `TrailingElementRenderer`, and `PageType`'s title is `Option<&'static str>`,
  not `Option<PageTitle<V>>`.
- **`0e075a072` (`WARP_*` env aliases) was declined** — I briefed *two* shards (S1, S6) to
  treat it as an open maintainer decision and prepare a decision package. Wasted work;
  S6 was corrected mid-flight.
- **`d019ddfe9` was declined only in part** — S8 was corrected mid-flight to split the
  declined variants from the still-queued `log::warn!` half rather than collapse the test.

**Process defect, filed against myself:** a shard list must be regenerated against
`TODO.md` and `DECLINED.md` at the moment of launch, not inherited from an adjudication
pass of unknown age. Add to `docs/pin-migration.md` Phase 2.6.

**S1 — one real fork defect, found and fixed.** `PageType::new_categorized` seeds every
category's filter with every widget index, and `update_filter` is the only place
`SettingsWidget::should_render` is consulted. So a categorized page that has never been
searched draws a category sub-header whose every widget is gated off, and the header
vanishes the moment the user types. Fixed by porting `categories_with_visible_content`
(upstream `3a7a4a5b3`) into `render_page`'s `Categorized` branch.
**Latent, not user-visible today** — every sole-widget category in the tree uses the
default `should_render`. `TODO.md`'s "do not sell this as a live bug" was right, and the
agent verified it rather than repeating it. Ledger row `3a7a4a5b3` can be ticked.

**S5 — the security half survived where the pipeline did not.** All 3 attachment tests and
all 17 environment tests are cloud (`cloud_object_models` deleted; `server_api` absent;
`DECLINED.md:81` #211 names `environment.rs` explicitly). But the *property* the traversal
tests exist for is enforced by this fork's `sanitized_basename`, byte-identical to the
pin's — and it had **no traversal case and no no-basename case**. Both now ported against
the real guard. The agent then swept the tree for a remote-supplied name reaching a
`Path::join` and found none unguarded; it also correctly declined to port 3 "pure" helpers
whose subjects have no caller here, on the grounds that inventing production code so a test
can exercise it is not debt repayment.
**It also refuted my "asserts the shape of the git command" claim:**
`blobless_clone_walks_path_limited_history_without_network` calls **no product code at all** —
it seeds a real repo and characterises the `git` binary. Ported here it would pass no matter
what this fork does.

**S8 — 12 of 17 not portable, and the refutations mattered more than the port.** The 4
`filter_skills_by_spec_*` are declined outright (`DECLINED.md:88` #487 deleted
`global_skills.rs` on 2026-08-10) — I had briefed "verify the fork has it under some name".
The cost trio is governed by `DECLINED.md:215`, which names
`app/src/ai/agent/conversation_tests.rs` *verbatim*; my hedge that BYOP usage might make it
live does not hold. One test ported: `artifact_round_trips_all_variants`, adapted to the
fork's 4 variants, and it is **not** redundant with the per-variant tests — none of them
covers `Artifact::Screenshot`, which shares the hand-written `Deserialize`. Artifacts are
persisted locally, so a dropped field corrupts stored screenshots silently.

**Two findings recorded, neither fixed, both correctly:** the fork's
`credits_spent_for_last_block` reset sits inside `if let Some(request_cost)`, which is the
pin's exact bug — but `request_cost` is `None` at every production construction site, so
fixing it is a no-op behind a dead branch and testing it would be stub coverage. And the
`pending_updates`/`drain_pending_updates` coalescing invariant is genuinely untested, with
no seam to drive it from a unit test.

**Coordinator verification (I compile; agents do not).** All three commits reviewed by
hand: `Artifact` does derive `PartialEq`, `NotebookId::from(String)` is already used at
`mod_tests.rs:203`, `mod_tests` already imports from `settings_page`, and `app: &AppContext`
is in scope at S1's insertion point. **My `BRIEF-COMMON.md` named the wrong rustfmt edition** —
`.rustfmt.toml` pins `edition = "2024"`, not 2021 (caught by S1).

#### Results — S6 (4 of 9 in)

**2 ported, 6 BLOCKED, 12 not portable.** The two ports closed a real production
capability gap rather than just adding coverage: the fork's `with_bounded_retry`
hardcoded both `MAX_ATTEMPTS` and `is_transient_http_error`. It now delegates to a new
`with_bounded_retry_using(operation, max_attempts, is_transient, attempt_fn)`
(`app/src/util/retry_strategies.rs:151`), plus `backoff_after_attempts` with a
`BACKOFF_MAX_EXPONENT` cap so a budget larger than `MAX_ATTEMPTS` cannot grow the
interval without bound. **Not a dead leaf** — every existing caller and all four existing
`with_bounded_retry` tests now run through the new path.
**Coordinator-verified behaviourally equivalent for existing callers:** the old loop's
cumulative `delay` after attempt *n* was `INITIAL_BACKOFF * 2^(n-1)`, which is exactly what
`backoff_after_attempts(n)` computes; `Duration` is already imported at `:2`.

**`60d602df6` re-verified independently and stays BLOCKED**, with counts rather than
assertions: `OrchestrationEventService` registered **0** times in `app/src/lib.rs`;
`EventsReady` emitted 3× and subscribed nowhere; `conversation_ready_for_pending_events`
is a *comment*, not code; `exit_commit_handle`, `run_conversation_id` and three
`*_for_test` seams are **0 hits each**; four test scaffolding helpers absent entirely; the
fork's `IdleTimeoutSender` has no `on_commit` and no `IdleWait`. The agent explicitly
declined to port `with_on_commit` standalone to rescue one test, because it would have no
production caller — the unreachable-surface shape the ledger reserves for the unblocking
increment. Nothing re-filed as untracked.

**Five more brief claims refuted, and two invert my reasoning:**
- I said the `ephemeral_installation_id_*` four were "likely local — MCP identity, not Warp
  cloud". Wrong: upstream's inline `MCPSpec::Json` arm leaves ids **random**, exactly as the
  fork already does; the deterministic id is reached *only* from the managed-MCP arms and
  exists only so a **rebuilt cloud sandbox** re-resolves to an id already in history.
- I said `well_known_resolution_*` was "public-spec discovery, not Warp-specific". Wrong:
  `MCPSpec::WellKnown("linear")` is a bare id whose meaning is owned by Warp's server, and
  `mcp_config.rs:40-48` already carries a standing "do not port the well-known variant".
- I said the two `sandbox_deadline`/`terminated_by_signal` siblings were "plain error
  classification, probably portable" — **the file-level-verdict trap running the other way.**
  All three need `classify_driver_error`, whose signature takes `warp_graphql::ai` and
  `server_api::ai` types; `SCOPE-AI.md:128` already had this right at verdict C.
- I pointed at `agent_sdk/retry.rs`; at the pin that is a 17-line re-export shim and the
  implementation lives in `server/retry_strategies.rs`. Creating the shim here would be
  dead surface — the fork's `agent_sdk` has no retry call sites.

**A correction to my own mid-flight correction.** I told S6 that `0e075a072`'s consumers
were "harness plugins already removed under #595". Imprecise: #595 removed the *Oz platform
plugin* install methods; the **notification** plugins were kept and still work
(`DECLINED.md:182`). The decline holds on a stronger fact — nothing in this fork consumes
those vars at all, and the notification plugins reach the terminal over OSC 777 /
`WARP_CLI_AGENT_PROTOCOL_VERSION`, not task env vars. That rationale row should be amended.

Adjudication for the managed-MCP and `ephemeral_installation_id` families is now recorded
as a module doc comment in `driver_tests.rs` itself, so the next sweep reads it in-tree
rather than re-deriving it.

#### S8 re-adjudication — my mid-flight correction paid off

I warned S8 that its `ambient_agent_task_deserializes_orchestration_source` verdict looked
like the collapse-to-the-bucket error. It re-adjudicated **from the sha** and the warning
was right, but for a sharper reason than I gave:

**`Orchestration` is not one of `d019ddfe9`'s five declined variants at all.** That row
covers `Jira`, `GitLabWebhook`, `RunScorer`, `Autofix`, `BenchmarkTrial`. The test actually
arrives with **`d15645c77`** ("Add ORCHESTRATION variant to client AgentSource", APP-5412),
which is **recorded in neither `TODO.md` nor `DECLINED.md`**. So the original verdict cited
a decline that does not reach the test.

Split verdict, and a third port for the round: the `Orchestration` variant and
`blocks_cloud_followups` are both genuinely absent (warp-server run source; **every**
`AmbientAgentTask` value in this fork is built inside a test, so `DECLINED.md:213`'s local
reversal does not make it reachable — local children never round-trip through this JSON).
But the property upstream's defect was *about* — quoting its own root-cause analysis, "the
task still loads but its source is lost" — is **live here and had zero coverage**:
`deserialize_ambient_agent_source` (`task.rs:242-266`) was entirely untested, `source`
appearing in that test file only as a struct-literal field.
`task_with_unrecognized_source_still_deserializes_with_no_source` now covers it, and
**guards the queued portable half of `d019ddfe9`** (`report_error!` → `log::warn!` on this
same arm): that port must change the log macro without changing the deserialize outcome.
**Coordinator-verified:** `GITHUB_ACTION` → `AgentSource::GitHubAction` at `task.rs:258`,
the `_` arm returns `None` without failing the record, and five existing
`from_str::<AmbientAgentTask>` tests establish the JSON shape. The recognised-source case
is asserted alongside the unknown one specifically so the test cannot pass against a
degenerate always-`None` deserializer.

**Five more unrecorded shas surfaced by grepping the ledger by sha rather than by symbol:**
`9e8ba7341` and `1c6708dde` (the cost tests — covered only by `DECLINED.md:215` prose, no
sha row), `bbec37f3d` and `a45466be8` (Factory skills), `a9c0a1ebd` (project-context
refresh — the NOT-PORTABLE call rests on in-code evidence at
`crates/ai/src/project_context/model.rs:355-372`, which records that the fork
*deliberately did not adopt* the pin's generation-counter design, rather than on any ledger
row). `19dc50535` is recorded only partially, for its `script/presubmit` hunk.

#### Results — S9, the refutation shard (5 of 9 in)

**0 refutations. All 29 confirmed NOT-PORTABLE, nothing committed, branch clean.**

This is the honest negative result the shard was for. Last round's equivalent pass
overturned 12 of 51; this one overturned none, and said so rather than manufacturing a
port. The three families that looked most refutable on paper each dissolved on reading
the body:

- **Local pane bookkeeping.** The fork *does* have `child_agent_panes` and
  `restore_missing_child_agent_panes_for_parent` — so the name-grep looks promising. But
  that path is **synchronous and disk-backed**: it reads `child_conversation_ids_of`
  directly, with no fetch, no in-flight state and no completion callback. There is
  therefore no dispatch to coalesce and no staleness window, so the bug class the 7 seed
  tests guard **cannot occur here**.
- **Shared onboarding widget navigation.** `move_selection` is a **private inherent method
  of `OfferSlide`**, not trait-provided; `OnboardingSlide::on_up/on_down` are bare hooks
  that default to `{}` and every fork slide implements its own. Nothing is shared.
- **The pure staleness predicate.** `is_stale_ancestor_list_completion` takes no `App` and
  no network and is portable *in mechanism* — but nothing in the fork would ever construct
  a `PendingParentChildSeed`, because it exists only to track an ancestor-list fetch the
  fork never makes. Porting it is the exact unreachable-code-guarded-by-a-test-that-can-
  only-pass shape `script/check_stub_coverage` forbids.

**Four of my brief's claims were wrong; all four verified by the coordinator:**
1. `build_cache` is documented after all — see the corrected bullet above.
2. **Wrong `DECLINED.md` row for onboarding.** I cited `:215` (provider-cost baselines on
   restored conversations); the row that governs the offer slide is **`:85` — account-first
   onboarding, billing, paid tiers (#11)**, which is what both in-code decline comments
   already cite. (`:215` *was* the right row for S8's cost trio, and S8 cited it correctly.)
3. **Wrong struct for the billing tests.** I named `TeamMember` (`team.rs:28`); the upstream
   test uses **`WorkspaceMember`** (`workspace.rs:115-120`). Both lack `is_disabled` so the
   verdict survives, but my cited evidence would not have supported it.
4. My shard brief was written against the **old** pin `42effe840` while `BRIEF-COMMON.md`
   names `4111d08f9`. The agent used the new pin throughout; no verdict turned on it.

**Three new findings, none of them in the 29:**
- **`crates/onboarding` has zero tests** (`grep -c '#[test]'` = 0) while **five live slides**
  carry hand-rolled clamped two-option arrow navigation (`intention_slide.rs:575/584`,
  `third_party_slide.rs:392/402`, `agent_slide.rs:1081/1106`, `theme_picker_slide.rs:598/614`,
  `customize_slide.rs:689/699`). That is precisely the bug class the declined
  `arrow_keys_move_through_both_options` guards — on live fork code, uncovered. **Correctly
  not written:** `crates/onboarding/Cargo.toml` has **no `[dev-dependencies]` section at
  all**, so the crate cannot host a test today; adding one is a manifest change no agent can
  compile-verify. Follow-up ticket, not a port.
- **`crates/onboarding/src/telemetry_tests.rs`** is unported and unmentioned anywhere.
- **An entire upstream module is absent and untracked:**
  `app/src/pane_group/child_agent/{mod,hydration,materialization,materialization_tests,restoration}.rs`,
  from `8eb52216f` (QUALITY-928 orchestration unified stack) and `73bd01431`.
  **Coordinator-verified: neither sha appears in `TODO.md`, `DECLINED.md`, `docs/`, or any
  `SCOPE-*.md`.** Correctly unported (cloud), but nothing records the decision — the same
  shape as the `build_cache` gap, and this one is real.

### TEST ADJUDICATION — Phase 2.6 COMPLETE (all 5 shards, 228 queue entries)

Every verdict read from the test BODY at `4111d08f9`. Ledger-ready TSV rows are in
the round scratchpad; they are NOT yet appended to `docs/sweep-verdict-ledger.tsv`.

| shard | tests | ledger rows | no applicable verdict |
|---|---|---|---|
| A — agent_sdk / retry | 46 | 33 | **13** |
| B — cost / local models | 27 | 26 | 1 |
| D — settings / CLI | 35 | 26 | **8** (+1 not-a-test) |
| E — verify NOT-PORTABLE | 51 | — | **12 REFUTED, 2 partial** |
| C — terminal / TUI | 69 (not 76) | 43 | **26** |

**Totals: 128 ledger-ready rows, 48 tests of portable debt with no applicable verdict,
plus 12 more freed by shard E's refutations = 60 tests of real work.** Rows are staged
in the round scratchpad and are NOT yet appended to `docs/sweep-verdict-ledger.tsv`.

#### Shard C — the largest, and where the value was expected to be

39 DECLINED, 3 CLOUD, 1 DIVERGENT, **26 portable debt**. Ranked:

1. **TUI driver disconnect resilience (5).** Highest value in the round. `is_terminal_disconnect`,
   `TuiDriverStartupError`, `fail_tui_driver` all absent; the fork's
   `draw_and_schedule_repaint` logs once and **reschedules the timer anyway**, so a
   dropped SSH connection leaves it drawing forever into a dead terminal. Pure
   `std::io`/errno/crossterm. **This is the test coverage for `ee351a0e7`, already in the
   port queue** — one work item, not two.
2. **TUI focus ownership (7).** One coherent cluster across four files. Would guard that
   clicking the input focuses it, a background session cannot steal focus, and focus
   survives a redraw. Note `empty_background_attachment_update_*` is a **near-miss, not
   COVERED-ELSEWHERE** — the fork guards the same invariant via a different door
   (`update_process_input_focus`), leaving the attachment-bar path unguarded.
3. **Right-click paste (6)** and **`#` AI-search trigger (3)** — both settings that are
   new at this pin. The `#` one matters: the fork's trigger is **ungated**, so a user
   cannot type a literal `#` at line start without the AI panel opening.
4. **Ctrl-C cancel window (2)** — viewer side, in scope; the fork has the write path but
   as a bare delegate.
5. **`is_passive_conversation` (3 of 4).** The earlier claim was independently
   re-verified and holds — live at **9** fork call sites, named by **no** fork test. But
   the fourth, `does_not_re_derive_from_history_after_construction`, is **DIVERGENT and
   would FAIL here**: the pin caches `is_passive`; the fork re-derives every call, so a
   stripped exchange falls back to `Active`. **Possible fork-only defect the adjudicator
   did not file:** `drop_hidden_passive_ai_blocks` (`view.rs:13989`) uses this in a
   `retain`, so a hidden passive block whose exchange left history stops classifying as
   passive and leaks into `rich_content_views`.

**The `is_ai_allowed_in_remote_sessions` trap fired as briefed, and worsened:** the pin's
version now takes a `&TeamContext` scope, and `org_command_patterns_*` additionally needs
`remote_session_regex_list`, which the fork reads off a hard-`None` `current_team()`.
Three tests correctly DECLINED. No SSH-tmux-deprecation test appeared in this shard.

#### The "not portable" calls were wrong 27% of the time

Shard E attacked 51 calls that would have permanently deleted work and **broke 12**,
plus 2 partial. All trace to one root error: the earlier pass matched a **file name**
(`run_agents_card_view_tests.rs`, `orchestration_event_streamer_tests.rs`) or a
**symbol name** (`AgentRunEvent`) against a declined family, without reading the body
or checking whether the fork defines the symbol under a different path.

- **7 `classify_family_event` tests.** The premise that `AgentRunEvent` is a deleted
  `server_api` type is **false** — the fork defines it itself at
  `app/src/ai/agent_events/mod.rs:49` and feeds it from a **BYOP provider SSE stream**
  (`claude_code/parent_bridge.rs`), not from Warp. The fork already open-codes one arm
  of the routing table by hand. Port cost ~60 lines, no `AIClient`, no `warp_graphql`.
- **4 `is_orphaned_by_finished_output` tests.** They import nothing from RunAgents. The
  predicate's own doc says it mirrors `block/view_impl/output.rs`'s `action_icon` — and
  the fork has that mirror **live at `output.rs:3175`, with zero coverage**
  (coordinator-verified: 0 hits in `output_tests.rs`). DECLINED #290 over-applied.
- **`drop_pending_events_for_exiting_conversation`.** Pure local queue; everything it
  needs exists in `orchestration_events.rs` except a 12-line `HashMap::remove`. Local
  orchestration was explicitly **reversed back into scope** by `DECLINED.md:213`.

**Coordinator error while verifying this:** I grepped `pub struct AgentRunEvent`, found
nothing, and nearly discounted a correct refutation. It is `pub(crate) struct`. Too
narrow a pattern — the same failure mode as citing a line without reading the attribute
above it.

#### Real portable debt found: 34 tests, none of which had a verdict to sit in

- **~~Cluster A (6) — QUALITY-1801 exit-commit ordering.~~ NOT PORTABLE YET —
  re-verified 2026-08-29 by the port agent assigned to it, which ported nothing.**
  Once an ambient run commits to exiting, a buffered child message must not restart MAA
  and flip a terminal conversation back to `InProgress`. Fully local, and the two
  decisions that *look* like they cover it genuinely do not (both re-confirmed
  independently — see below). **But the defect cannot occur in this fork, because the
  code path it guards does not exist here.** The upstream commit is `60d602df6`, which is
  ALREADY in the port queue at `TODO.md:1297` marked **BLOCKED**; that row is correct and
  under-states the blocker. Cluster A and that row are one work item. Full missing-piece
  inventory now recorded there. **Do not schedule Cluster A independently, and do not
  re-file it as untracked** — the adjudication note that `exit_commit_handle` has "0 hits
  in TODO.md" was true of the *symbol* and false of the *work*; the ledger tracks this by
  upstream sha, not by symbol name.
  - **Both look-alike decisions re-verified and neither applies, as the adjudicator
    said.** (1) The 2026-08-17 idle-window decline is `TODO.md:6267-6296`, not a
    `DECLINED.md` row; it removed the *failure-side* `SETUP_FAILED_IDLE_TIMEOUT` deferral
    and says in its own text "the surviving `idle_on_complete` branch". `idle_on_complete`
    is live: `--idle-on-complete` at `crates/warp_cli/src/agent.rs:353`, plumbed at
    `agent_sdk/mod.rs:511`, consumed at `driver.rs:1669` and `:1807` via
    `complete_with_optional_idle`. (2) `DECLINED.md:213` does reverse the orchestration
    decline for the local half (#304/#309/#310/#325/#329), leaving only the cloud-runner
    half (#290) declined. Nothing in `60d602df6` touches `server_api`,
    `ServerApiProvider`, `MockAIClient` or `warp_graphql`.
  - **Overlaps shard E's `drop_pending_events_...`** — same commit, blocked with it.
- **Cluster B (6) — REMOTE-2269 shared recovery budget.** Retries and resumes must draw
  from one counter. The fork still has the pre-fix 5-bool `recovery_action`, and its own
  test comment states the exact behaviour the fix removes. **This is the same defect as
  the `1a29f680d` PARTIAL already in the port queue** — the tests and the commit are one
  work item.
- **Settings empty-category guard (2).** One is the cheapest item found all round —
  **zero production change**, the fork already behaves correctly. The other names a live
  defect: a category whose only widget cannot render draws a bare header until the user
  types.
- **`ADD_MCP` GUI-only registration (1)** — live and untested.
- **12 refuted above.**

#### Maintainer decisions surfaced

- **The slug migration is not portable as tests.** Adopting `from_slug` means adopting
  upstream's page split — it asserts `"AI"→WarpAgent`, `"Code"→CodeIndexing`,
  `"MCP Servers"→AgentMCPServers`, and all three are **live, distinct, shipping sections
  here**. And `slugs_were_seeded_from_the_display_labels_they_replaced` would be an
  outright regression: it welds the stored key to the `Display` label, which upstream can
  afford because its labels are English literals and the fork cannot because its are i18n
  keys. The fork's deliberate inverse is `persistence_keys_are_unique_and_not_localized`.
- **`WARP_*` env aliases.** Mechanism is plainly portable; the question is whether a
  de-Warped fork advertises `WARP_*` names to third-party agents. `DECLINED.md:182`
  covers not *renaming* existing wire tokens — a different question from adding aliases.
- **~~`crates/build_cache`'s absence is undocumented.~~ WRONG — corrected 2026-08-29.**
  The claim checked only `DECLINED.md`, `TODO.md` and `docs/STATE.md` (0/0/0, accurate as
  far as it went) and concluded "nothing records whether it was a decision". It is in fact
  documented with a rationale at **`SCOPE-REST.md:214,328`** (verdict `C`, cloud) and
  `docs/SWEEP-SUMMARY.md:299`: sole consumer at the pin is
  `app/src/ai/agent_sdk/driver/cache_setup.rs`, whose imports open with
  `use build_cache::...; use cloud_object_models::SourceRepo;` — and `cloud_object_models`
  is declined outright at `DECLINED.md:81` (#211). So the absence is a documented,
  defensible consequence of an existing decline. What is genuinely missing is only a
  `DECLINED.md` row of its own — a much weaker gap than "undocumented".
  **Lesson: three files is not "the docs".** `SCOPE-*.md` carries per-file verdicts and was
  not consulted.

#### TWO SEPARATE EXTRACTION DEFECTS — corrected 2026-08-29

An earlier revision of this section blamed both on the coordinator. **That was wrong**;
they are distinct and only one is mine.

**Mine — the 235-vs-228 overcount.** My `awk` range extraction leaked 7 lines of the
queue's own reconciliation prose into shard C's file, where they read as test names.
Coordinator-verified: `235 - 7 = 228`, matching the queue exactly. Shard C caught it and
adjudicated 69 real tests rather than the 76 it was handed.

**The generator's — nested helpers counted as tests.** `generate_repin_queue` collects
`fn <name>(` without requiring a preceding `#[test]` or tracking nesting depth, so it
lists helpers declared inside test bodies. Confirmed: the queue lists `is_listed`, which
at `4111d08f9:app/src/settings_view/mod_tests.rs:161` is a nested helper inside
`#[test] fn all_sections_list_is_exhaustive()`. **So the 228 is itself inflated.** Pin
`mod_tests.rs` has six more such helpers; pin `lib_tests.rs` has 148 top-level `fn`
against 144 `#[test]`.

**Consequence: every queue count needs re-deriving with an attribute + depth check** —
the 228, and the 1,100 tree-wide figure. Fix the generator before the next round.

#### (superseded heading kept for diff clarity)

My extraction script matched `fn <name>(` **without requiring a preceding `#[test]` and
without tracking nesting depth**, so it collected helpers declared inside test bodies.
Confirmed: `is_listed` at `4111d08f9:app/src/settings_view/mod_tests.rs:161` is a nested
helper inside `#[test] fn all_sections_list_is_exhaustive()`. **Do not insert it into the
ledger.** Pin `mod_tests.rs` has six more such helpers; pin `lib_tests.rs` has 148
top-level `fn` against 144 `#[test]`.

**Re-run the extraction with an attribute + depth check before trusting any queue count**,
including the 228 and the 1,100 tree-wide figure.

### TEST ADJUDICATION — shard detail

235 tests across 42 files, sharded 5 ways. **Every verdict is read from the test
BODY at `4111d08f9`**, not from its name — the characterisation pass that preceded
this read no bodies and said so.

**Shard B (cost / local models) — 27 of 27 adjudicated.** 26 ledger-ready rows
(6 DECLINED, 8 CLOUD, 3 COVERED-ELSEWHERE, 1 DIVERGENT, 1 MIXED, plus siblings) and
1 test with **no applicable verdict**.

- **Real portable debt found:** `reveals_shortcut_hints_requires_overlap_with_binding_modifiers`
  (`app/src/workspace/view/vertical_tabs_tests.rs`). Pure local UI logic, zero cloud
  surface; `reveals_shortcut_hints` / `TabShortcutModifierState` are absent fork-wide.
  **It is already tracked** as `8b88df98` in the port queue above — filing it DECLINED
  would have retired a commit the ledger is still following.
- **Two slivers inside otherwise-retired rows**, flagged so they are not lost:
  `Artifact::Screenshot` has no round-trip test though the variant exists here (~12
  lines against existing code; artifacts are persisted locally, so a serde change
  silently corrupts stored screenshots); and the orchestration **token** rollup shape
  is expressible here — the fork has real provider token counts
  (`total_token_usage_by_model`) and a priced model (`usage_cost.rs`) — but the pin
  sources tokens *exclusively* from warp-server `RequestCharges`, so porting means
  building a feature on a different pipe. **Reopen as a feature request, not as pin
  parity.**

#### FIFTH COORDINATOR ERROR — on the call flagged as the shard's highest-stakes

The brief told the adjudicator that `DECLINED.md`'s "credits round to 1dp" row pins
only the rounding and "does not decline the cost dimension", and that provider-reported
cost "is arguably exactly what a BYOP fork wants."

**Half right, and wrong where it counts.** A *different* row governs:
**`DECLINED.md:215` "Provider-cost baselines on restored conversations" (DECIDED
2026-08-17)**, which names `usage_totals()` and the `restored_*_provider_cost_baseline`
family, records that **seven ledger rows already rest on it**, and closes: *"Do not port
them, and do not re-derive a BYOP equivalent."* Coordinator-verified.

And the premise was wrong about the data: `ChargedUsageTotals`
(`4111d08f9:crates/persistence/src/model.rs:1622`) is documented as a client-side mirror
of **warp-server's Go `SumChargedUsage`**, carrying `platform_cost_in_cents` (Warp's
platform fee) and `web_search_cost_in_cents` (Warp's hosted search). It is billing, not
provider cost. The fork already built the honest version of that goal —
`app/src/ai/usage_cost.rs` with a user-stated `TokenPrice` — and hard-sets
`cost_in_cents: 0.0` at `chat_stream.rs:7255` because "a BYOP provider reports tokens,
never money."

**That is five wrong coordinator briefs this round, every one caught.** The pattern
holds: each was confident, specific, and cited a real artifact — this one cited the
wrong DECLINED row while the governing one sat 40 lines away.

*(Minor: the adjudicator cited `chat_stream.rs:7239` for the zero-cost line; it is at
`:7255`. Line drift, not a substantive error.)*

### FINAL BUILD VERIFICATION — everything merged, 2026-08-29

Merged tree: 5 original ports + 3 test-port branches + the partial file move.
**No agent compiled anything at any point.** All results are the coordinator's.

| package | result |
|---|---|
| `warp` | **6218 passed**, 29 skipped |
| `warp_tui` | **947 passed** |
| `warpui_core` | **608 passed**, 7 skipped |
| `warp_completer` | **183 passed**, 4 skipped |
| `warp_terminal` | **125 passed**, 2 skipped |
| `warp_util` | **83 passed** |
| **total** | **8164 passed, 0 failed** |

`cargo check`/`build` clean, **zero new warnings** (3 pre-existing, in untouched files).
**All 10 fork-boundary guards ok.** `Cargo.lock` regenerated by the coordinator — the move
agent flagged it could not, and a `--locked` job would have failed.

**New tests confirmed to actually RUN, not merely to exist** (this round found dead and
vacuous tests repeatedly, so presence was checked separately from passing):
- `ai::agent_events::family_tests` — **9/9**: the 7 ported classifier tests plus 2
  fork-authored table-coverage tests added after refutation showed 1 of 9 wire rows covered.
- `output::tests::is_orphaned_by_finished_output_tests` — **6/6**, including
  `block_status_is_not_computed_when_the_action_has_a_status`, the guard added after
  refutation showed the "load-bearing" laziness was asserted in prose and enforced by
  nothing — while the eager alternative is the candidate's own signature.

### BUILD VERIFICATION — 2026-08-29, coordinator, the ONLY compilation this round saw

Merge commit `83b6f8fab` on `repin-2026-08-29-4111d08f9`, all five ports merged.
No agent compiled anything at any point; every result below is the coordinator's.

| gate | result |
|---|---|
| `cargo check -p warp --features gui --lib --tests` | **ok**, 0 errors |
| `cargo check` feature-unified, multi-package | **ok** |
| new warnings introduced by the round | **0** (3 pre-existing, in untouched files) |
| `cargo nextest -p warp` | **6207 passed**, 29 skipped |
| `cargo nextest -p warp_tui` | **947 passed**, 0 skipped |
| `cargo nextest -p warpui_core --features tui` | **608 passed**, 7 skipped |
| `cargo nextest -p warp_completer` | **183 passed**, 4 skipped |
| 8 fork-boundary guards | **all ok** |
| `known_test_failures.txt` baseline | empty; 0 failures observed, so no drift |

**Total 7945 passed, 40 skipped, 0 failed.**

#### The build caught one thing five reviewers could not

`warpui_core report_error::tests::report_error_throttles_per_callsite_not_globally`
FAILED on first run — the replacement written after the coordinator rejected its
tautological predecessor. Its own sanity guard fired, which is the only reason it
failed loudly instead of passing while testing nothing.

**Root cause, traced by the coordinator:** `crates/warpui_core/src/test.rs` uses
`#[ctor]` to install `simplelog::SimpleLogger` **before `main`**, unconditionally, in
every test process of that crate. `log::set_logger` can therefore never succeed in
`warpui_core` — `[ERROR] sanity` reached stderr from simplelog while the test's own
capture saw nothing. Not a race: nextest already gives each test its own process. The
porter had modelled it on `warp_core/src/errors_tests.rs`, which uses the identical
pattern and works there **only because `warp_core` has no such `#[ctor]`**.

Resolved by deleting the test and documenting the property at the macro — including
*why* no test is possible in this crate, naming the `#[ctor]`, so it is not attempted a
third time. `take_once_fires_exactly_once` is kept and passes. **No testability seam was
added and the sanity check was not weakened**; an honest documented gap beats either.

#### What the build did NOT verify — unchanged, and not to be read as passing

- **The wasm keystroke-leak fix.** `crates/warpui/src/platform/mod.rs` is
  `#[cfg(target_family = "wasm")]` and neither `pr-check.yml` nor `script/precheck`
  builds a wasm target, so `log_shape()` and both changed `event_loop/mod.rs` lines are
  compiled by **nothing** on this fork. The highest-value leak fix has no compile and no
  test coverage anywhere.
- **The fish/DCS bootstrap fixes.** No shell harness exists in this repo
  (`bootstrap_test.rs` stubs `fish.sh` entirely). Verified behaviourally under real
  `bash`/`fish` instead, including old-vs-new tables.
- **The font-fallback exclusion.** Its only guard was deleted as dead code, by design.

#### Operational note for the next round

`script/precheck` was killed three times at the `warp` **test-binary** compile — a
background-task duration limit, not memory (15G free each time, no OOM record). The
compile takes ~5m48s and completes fine in the foreground. Split it: build first, then
run nextest against the cached binary.

### ROUND STATUS — live, updated as the round runs

Round branch `repin-2026-08-29-4111d08f9`, pushed. `main` untouched.

**Refutation of scope: COMPLETE (6/6).** Results folded into the sections below.
Net effect: 9 commits moved into the port queue, 2 live data leaks found, 2 broken
tripwires found, 5 non-resolving shas corrected, 1 ledger entry (`cff5f778c`) found
to describe a quarter of its commit, and the partial-hunt premise refuted outright.

**Port wave 1: 5 agents, isolated worktrees off the round branch, none building.**

| worktree / branch | scope | state |
|---|---|---|
**All 5 written and coordinator-verified. All 5 under adversarial refutation by a
different agent than wrote each.** No agent compiled anything.

| worktree / branch | commit(s) | scope |
|---|---|---|
| `port/grep-parse` | `f79030afc` | `fbbfc41f3` grep NUL-delimiting |
| `port/leaks-logs` | `3c5f7c620` | `27f8ee6c` 2 data leaks + 3 log throttles |
| `port/shell-bugs` | `d08e3e951`, `b255dae5f`, `4ca52cd38` | `e722ebed` panic + fish kill + DCS + pwsh chord |
| `port/cursor` | `e8c05ac66`, `0f2877a52` | `ee95ac0fd` double cursor (+ refutation fixes) |
| `port/completer-cache` | `4ca8de96d`, `a9c000c19` | `213c9b32` cache bound + `98b1f5af8` font fallback |

**THE COORDINATOR'S BRIEFS WERE WRONG THREE TIMES, AND AN AGENT CAUGHT IT EVERY
TIME.**
Recorded because it is the strongest argument in this round for refuting the
coordinator as well as the ports:

1. **grep brief** told the porter the 25 new tests in `grep_tests.rs` were "a
   separate work item -- do not port them here." They are in `fbbfc41f3` itself;
   the Phase 2 queue mis-attributed them. Following the brief would have shipped
   `build_grep_content_scan_command`'s two-layer shell quoting with **zero test
   coverage**.
2. **leaks-logs brief** stated "the only adaptation needed is the import path:
   `warp_errors::` becomes `warp_core::errors::`". **That is a hard build failure**
   for `warpui_core` and `warpui`: `warp_core/Cargo.toml:50` depends on `warpui`,
   and `warpui/Cargo.toml:57` depends on `warpui_core`, so adding `warp_core` to
   either closes a Cargo cycle. Coordinator-verified. The porter instead wrote a
   local shim following the in-tree precedent `crates/warp_tui/src/report_error.rs`.
3. **cursor doc-fix instruction** told the porter to justify the `BeforeExecution`
   exception with "the output grid is empty there -- `grid_renderer.rs:1150-1157`
   short-circuits an all-empty row." **Coordinator-verified wrong:** that
   short-circuit lives only in `render_grid_with_ligatures` (fn starts `:973`);
   `render_grid_without_ligatures` (`:464-972`) has no equivalent, so the reason
   holds on one render path only and the corrected comment would have been
   half-wrong again. The porter found the path-independent reason one level up --
   `BlockGrid::draw` derives `end_row` from `len_displayed()`
   (`blockgrid_renderer.rs:81`), which is 0 before `preexec`, so the row range is
   empty on both paths -- and used that instead.

**Pattern worth acting on: all three coordinator errors were confident, specific,
and cited a real file:line.** None would have been caught by an agent that trusted
the brief. The round's refute-everything rule has to include the coordinator's own
instructions, not just the ports.

**Second defects found by porters while porting** (neither was in the brief):
- `register_signature` was a **silent no-op** for any name that had previously
  missed, because the `Option`-valued `MemoMap` cached the miss in an append-only
  structure. Fixed as part of the cache split.
- Upstream's fish preexec fix, ported literally, **would have inverted the bug**:
  `in_band_command_executor.rs:373` prepends a leading space to fish generator
  commands and `string match` is start-anchored, so without a `string trim` the
  "fixed" branch kills generator jobs *while a generator is running*. The porter
  added the trim and documented the Rust-side coupling in the script.

**Deliberate restraint worth keeping** — the cursor porter found real collateral in
`grid_renderer.rs` (`hide_cursor_cell && visible_cursor_shape.is_none()` now also
true for finished blocks, so a CLI agent's open rich input skips the cursor cell on
every scrollback block), was two lines from "fixing" it, checked
`ee95ac0fd:app/src/terminal/grid_renderer.rs` first, found **upstream ships the
identical condition and comment**, and left it alone rather than manufacture a
parity divergence. Flagged for separate decision, not folded into a port.

**Known gaps the porters declared rather than hid:**
- Fixes 2 and 3 of `e722ebed` (fish preexec, DCS encoders) have **no in-tree
  regression test** -- this repo has no shell harness, and `bootstrap_test.rs:9`
  stubs `fish.sh` entirely. Verified behaviourally under real `bash`/`fish` instead.
- The 4 soft-keyboard redaction tests are `#[cfg(target_family = "wasm")]`-gated and
  **will not run in the coordinator's native suite**.
- The font-fallback mapping generator was not run end-to-end (`fontTools` absent,
  fonts live in a Warp GCS bucket); its two changed paths were exercised with a
  stubbed harness that was deliberately not committed.

**Coordinator TODO at merge:** tick `TODO.md`'s `ee95ac0fd` row (unblocks
`f42c4ab6c`, which depends on it); take the round branch's `TODO.md` over the
grep worktree's copy and re-apply its row closure here.

**`port/grep-parse` (commit `f79030afc`) — coordinator verification record.**
Checked without compiling: the three pinned command-string tests preserve every
adversarial payload character-for-character (`$(touch /tmp/warp-poc); ` + backtick +
`id` + backtick, `owner'"'"'s code`, `$(New-Item C:\pwn); ''literal'''`) with only
the option list changed — legitimately updated, not weakened; `GrepFileMatch` /
`GrepLineMatch` both derive `Debug + PartialEq + Eq`, so the new `assert_eq!`s
compile; all 8 new functions defined exactly once, no dangling test references; old
`parse_grep_output` fully removed (0 occurrences, no dead code); the parser now
errors only when EVERY record is unparseable, which is the actual defect fix.

**The porter corrected the coordinator, with evidence.** The brief told it the 25 new
tests in `grep_tests.rs` were "a separate work item — do not port them here." That was
wrong: `git show fbbfc41f3` adds exactly those 25 test fns, including the six named in
the brief. The Phase 2 queue's ACTIONABLE row mis-attributed them to a separate item.
Porting `build_grep_content_scan_command` without its own quoting tests would have
shipped a shell-injection surface with zero coverage. **Check whether the same
generator defect split other commits in this round.**

**Known merge conflict:** the grep porter also edited `TODO.md` in its worktree to
close its ledger row. The coordinator owns `TODO.md` centrally; take the round
branch's version at merge and re-apply the row closure here.

### `port/completer-cache` REFUTATION — the sharpest finding of the round

**D1 (blocking, being fixed): both new font tests are dead code, and the commit
message's justification for adding them is factually false.** The porter wrote that
upstream's reviewer objections do not apply "because `mod font_fallback` is already
unconditional at `app/src/lib.rs:48`". It cited line 48 correctly and **did not read
line 47**, which is `#[cfg(target_family = "wasm")]`. The module is wasm-gated —
identically upstream, and since the fork's initial commit — which is exactly the
"per-target compilation" concern upstream's reviewer raised.

Three consequences: neither test appears in the coordinator's run at all, as pass or
failure; there is **no wasm build in `pr-check.yml` or `script/precheck` either**, so
`app/src/font_fallback.rs` is compiled by nothing in this repo and that module has
never been type-checked; and the claimed guard — "a regeneration that loses the
exclusion fails here rather than silently" — **does not exist**.

**This is the same error class the coordinator committed three times this round:
citing a real file:line while missing the attribute on the line above it.** Worth a
standing rule: when a claim rests on a module or item being reachable, read the two
lines above the citation.

- **D2 (being fixed): `test_neighbouring_arrows_stay_on_hack_nerd_font` passes
  pre-fix.** All eight code points it asserts were already on the Hack Nerd Font arm.
  Cannot distinguish fixed from unfixed code — fake coverage twice over, since per D1
  it also never runs.
- **D3 (being fixed): a residual leak the port did not close.** `SignatureCache::insert`
  (`registry.rs:92`) applies **no** length cap, and `register_signature` is reachable at
  runtime over plugin IPC (`app/src/plugin/app/service_impl/completions.rs:39-41`). A
  plugin sending many long names still grows the append-only `MemoMap` without bound, so
  the message's "bounded by the embedded corpus plus registered names" has a
  plugin-controlled second term.
- **D4/D5 (minor):** `MissCache::new(0)` has an effective capacity of 1; the cap is
  measured pre-lowercase, so a 255-byte token can be stored as a ~765-byte key.

**Concurrency: clean, and the port is an improvement.** No deadlock, no guard held
across `lookup_fn`, no escaping reference, and the `contains`/`insert` TOCTOU is benign
because `insert` re-checks under the write lock. Notably the **pre-fix** code was the
risky shape — it ran `lookup_fn` inside `MemoMap`'s creator closure — so the port
removes a re-entrancy hazard rather than adding one.

**Corrections to that porter's claims:** `test_evicts_in_fifo_order_regardless_of_lookups`
is verbatim upstream, not a fork addition "beyond upstream". Its corpus test *was*
verified to recurse properly — the refuter independently walked the 492 pinned signature
JSONs and found the longest name is 59 bytes (`dig.json`), 4x headroom to the cap.

### FIX ROUNDS — 4 of 5 complete. Every fix went beyond what was asked.

**`port/leaks-logs` — SHA CHANGED, `2f643f703` -> `cdd57dd28`.** The porter amended
rather than stacking, because the original message claimed "three per-frame error logs"
and described a test that did not test what it said. Correct call on an unpushed branch;
recorded here because anything referencing the old sha is now stale.

Its tautological test is **replaced, not deleted**: `report_error_throttles_per_callsite_not_globally`
installs a capture logger (following the precedent in `warp_core/src/errors_tests.rs`),
drives two distinct `report_error!` invocations in separate functions so each expands its
own `static`, and asserts `logged("second callsite") == 1` — which reads 0 under a shared
global flag. It also runs a sanity call-site first, so if another test in the binary wins
`set_logger` the test fails loudly on that specific message instead of every later
assertion reading a vacuous zero. Deliberately one test, not several, because
`OncePerRun` latches per process and parallel `#[test]`s would race the buffer.

It also **fixed rather than documented** the fourth log site: `flex/mod.rs:220`, the
`MainAxisSize::Max` sibling, is now throttled too. Verified: both sites use `OncePerRun`
and zero bare `log::error!` remain in that file. Marked in-code and in the message as a
deliberate step beyond upstream `8936686f2`, which scoped itself to one site.

**`port/shell-bugs`** (`47799b7af`, `2970bbdb2`): D1 fixed with `string match -v --`.
The porter then found a **second half of the same root cause on its own**: the seed
and reset at `:83`/`:208` were `set -g _warp_generator_pids ''`, a one-element list
holding the empty string, which `string match -v` does not remove — so the correct
removal alone would still have run `kill -9 ''` once per user command forever
(verified non-inert: `kill: failed to parse argument: ''`). Both seeds are now bare,
and the list provably drains to zero. D2 fixed with a `test -n` guard; the fish-only
divergence from `bash_body.sh:307` is now stated in both the code and the message.

**`port/grep-parse`** (`019776d50`): D3 fixed with `trim_start()` applied **once
before the first record** — not a blanket `.trim()`, and not per-record — on the
reasoning that real `grep` never emits whitespace ahead of a path, so bytes there are
transport, while every later boundary is one the parser identified itself. The
interior-newline test (`weird\nname.rs`) still passes, confirmed.

D4 was fixed **better than the brief asked**. Instead of special-casing the missing
newline, the porter derived a format invariant: content is one line of a text file, so
it can contain neither `\n` (single line) nor `\0` (`-I` excludes binary, and
`git grep -z` emits NUL only as a separator). A NUL before the record's newline is
therefore impossible inside content and proves the record carried none. Three shapes,
one rule — which **removes the dependency on undocumented PowerShell behaviour in both
directions**, and additionally fixes a `git grep -z` stream whose trailing newline was
stripped in transit and had been silently losing its last match. Tests 29 -> 37.

**Coordinator correction to that porter's report:** it suggested field reports of thin
PowerShell results might trace to D4. They cannot — D4 was introduced by `f79030afc`
in this round and has never shipped.

**`port/cursor`** (`0f2877a52`): doc comment corrected, collateral recorded on the
predicates rather than at the renderer (the porter's reasoning: `grid_renderer.rs` is
byte-identical to upstream there, and two comment-only hunks would become two new
divergences to reconcile at a re-pin that already has `8ba01aa1a` queued for that
file). Verified comments-only: zero non-comment lines changed.

**A fourth coordinator error, and an unresolved disagreement.** The brief offered
`--flag<u-umlaut>=x` as the `parsers/v2.rs` panic reproducer. The porter reported that
one does not fire (offset lands on the `=`) and substituted a 3-byte character,
`--<CJK>=x`, which it verified does. The coordinator's own model makes **both** land
mid-char, so the two disagree on how the offset counts. **Unresolved, and deliberately
left that way** — it does not change the outcome, because the recorded comment uses the
3-byte case, which panics under either model. Whoever files that issue should settle it
first.

### PORT REFUTATION RESULTS — 3 of 5 in. Every port had findings.

Each port was attacked by an agent that did not write it. **No port survived
unchanged.** Two refuters went beyond reading and executed reconstructed
artifacts against real tools (BusyBox 1.37, GNU grep 3.12, git 2.53, fish 4.2.1
under a pty), which is where the sharpest findings came from.

#### `port/shell-bugs` — the port ARMS a latent trap it does not fix. Blocking.

- **D1 (blocking, being fixed).** `fish.sh:135` removes a finished generator PID
  with `string replace $command_pid '' $_warp_generator_pids` — a **substring**
  replacement over every element, not element removal. Observed in real fish:
  `['', '40213', '213']` with `213` exiting becomes `['', '40', '']`. The next
  preexec then runs `kill -9 40` — **SIGKILL to a process that was never a
  generator**, stderr suppressed. **This was inert before the port**, because
  `kill -9 $pids` referenced an undefined variable and always failed. Fixing the
  loop variable made the corrupted list live. Correct removal is
  `string match -v -- $command_pid $_warp_generator_pids`.
- **D2 (being fixed).** Verified under a real pty: fish fires `fish_preexec` for a
  whitespace-only line, so **a bare Enter now SIGKILLs in-flight generators**. Old
  behaviour never killed. bash is unaffected (`$BASH_COMMAND` is unset for an empty
  line), so this is a fish-only divergence.
- **D3 (recorded, NOT fixed, and it questions the port's premise).** The clamp may
  not close the bug TODO.md:384 reports. Two adjacent raw-index sites survive:
  `completer/suggest/alias.rs:270` indexes `&input[..span.start()]` two lines after
  the now-clamped call, and `parsers/v2.rs:105-124` increments `offset` once per
  **char** inside `.chars().skip_while(..)` then uses it as a **byte** offset at both
  `Span::new(..)` and `item[offset..]`. A non-ASCII flag name (`--flag<non-ascii>=x`)
  builds a mid-char Span *and* panics at `item[offset..]` before `slice` is reached.
  **HYPOTHESIS: that, not `Span::slice`, may be the real reproducer.** File separately.

#### `port/grep-parse` — two regressions the port introduced, two inherited.

- **D3 (being fixed).** The new parser dropped the old `.trim()`. On `"\nsrc/a.rs\0..."`
  the path becomes `"\nsrc/a.rs"` — non-empty, so the emptiness guard passes, the
  digits parse, and **a corrupted path is reported to the model as a real file**.
  Unreachable by the skip-and-warn path *because it parses*.
- **D4 (being fixed).** `build_select_string_command` emits ``{path}`0{line}`0`` — no
  content, no newline. Byte-traced: on two such records the second is **silently
  dropped** (`find('\n')` returns `None` -> `rest = ""` -> loop ends), with no warning
  and no `Err`, because `matched_files` is non-empty. Only PowerShell's implicit
  per-object newline prevents "PowerShell always returns exactly one match" — stated
  nowhere, covered by zero tests.
- **D1 (upstream-inherited, NOT fixed).** **The BusyBox fallback cannot rescue
  BusyBox.** Proven by execution: BusyBox grep rejects `--devices=skip`, which sits
  *before* `--null` in argv, and both commands inside the fallback carry it. Status 2
  -> the fallback returns the original error. ~130 lines, 5 tests and a doubled
  traversal, inert on the only platform they exist for. Upstream has the same hole.
- **D2 (upstream-inherited, NOT fixed).** The fallback fires on *any* error (bad
  regex, timeout, exec failure), and `GREP_TIMEOUT` wraps the whole of `run_grep`
  once — so the strictly more expensive second scan eats the remaining budget and
  replaces the real diagnostic with "timed out".
- **§5.6 verdict: legitimately updated, not weakened.** Function-name diff shows
  additions only, 25 new, zero removals; all three adversarial payloads identical
  character-for-character; every full-string `assert_eq!` retained (none softened to
  `contains`). **Faithfulness: no unexplained drift** — the only diffs vs upstream are
  the two declared fork divergences.

#### `port/cursor` — correct and faithful; one prose defect, one inherited consequence.

- **Both tests confirmed NON-VACUOUS** by mutation analysis: deleting the
  `is_active_and_long_running()` / `is_command_grid_active()` conjuncts makes each
  fail, and the `set_was_long_running` latch does not bypass the transition under
  test because the `match self.state` early-returns first.
- **D1 (upstream-inherited, accepted, now documented).** The porter's restraint was
  right but its analysis stopped one step short: **this commit changes which side of
  the `hide_cursor_cell` condition the code lands on.** `visible_cursor_shape` used to
  be `Some` for finished blocks, so the cell-skip at `grid_renderer.rs:638` fired only
  when an agent had turned `SHOW_CURSOR` off. It is now `None` for every
  `DoneWith*`/`Static`/finished-`Background` block, and `hide_cursor_cell` is
  **element-wide, not per-block** (`view.rs:22940`), so while a CLI agent's rich input
  is open every finished block in the viewport skips rendering one cell. Upstream ships
  the identical condition — recorded, deliberately not fixed here.
- **D2 (being fixed).** The doc comment on `is_output_cursor_visible` argues the
  opposite of what the code does — it names `BeforeExecution` as the exception and then
  treats it as none. Fork-added prose; upstream shipped no doc comment.
- **Coverage gap worth knowing.** Both tests exercise the *predicates*, not the *call
  sites*. Reverting `block_list_element.rs` while keeping the methods leaves both green
  — the same shape as the `01778efe` lesson.

#### One refuter's best attack failed, and it said so

The shell refuter expected `(string trim -- $argv[1])` to yield **zero** arguments on
whitespace-only input, leaving `string match -q PAT --` reading stdin and eating the
user's keystrokes — and confirmed that failure mode is real in isolation. It is
unreachable: `string trim` emits one line per input, so the substitution yields exactly
one empty-string argument (`count (string trim -- '  ')` = 1, versus
`count (echo -n '')` = 0). Reported as an attack that failed rather than padded into a
finding.

### ROUND PROTOCOL — standing orders, 2026-08-29 (maintainer)

**In force for the whole round. Do not infer exceptions.**

1. **NO AGENT MAY BUILD.** No `cargo` in any form — check, build, test, nextest,
   clippy, fmt — and no `script/precheck` or `script/agent-cargo`. This is not a
   style rule: a parallel cargo exhausts this laptop (12c/22GB) and crashes the
   machine. **The coordinator is the only one who may build**, and only once the
   port agents are completely finished.
2. **Every unit of work is refuted before it counts as done.** Scope was refuted
   before porting began; each port is refuted after it is written. "Done" means
   done AND refuted, never done alone.
3. **Bugs are traced and fixed at the cause. Never by changing a test.**
   AGENTS.md §5.6. If a port makes an existing fork test look wrong, that is the
   signal to STOP and investigate, never to edit the test. Extending a test to a
   widened signature is not weakening it; changing what it asserts is.
4. **The coordinator verifies all code that is written.** No agent's diff lands
   unread.
5. **TODO.md is kept current as the round runs**, not written up at the end.

Round branch: `repin-2026-08-29-4111d08f9`, based on `main` at `52e94ae9b` with
the three in-flight fix branches merged (#621 autosuggestion remote-path
validation, #623 issue-template links, #625 runbook + this ledger). Work does not
touch `main`.

### Phase 2 was skipped and has now been run — the queue finds work the commit walk cannot

**The scoping round did Phase 2.5 (the commit walk) but never generated the Phase
2 queue.** `docs/pin-migration.md` is explicit that these are *two* inputs, and the
queue sees a class the walk structurally cannot: stale ledger verdicts, and tests
that exist at the new pin in files the ledger already covers.

Generated 2026-08-29 with `script/generate_repin_queue 4111d08f9 42effe840`
(pure shell, no cargo):

| queue bucket | count | meaning |
|---|---|---|
| **LEDGER COVERAGE GAP** | **228 tests / 42 files** | pin test in a ledger-covered file with NO row of its own — each is a FIRST adjudication |
| **LEDGER RE-EXAMINE** (rule 1) | 68 | pin file changed; recorded verdicts stale |
| LEDGER RE-EXAMINE (rules 2, 3) | 0 / 0 | clean — no struck DECLINED rows, no revived MISSING-SUBSYSTEM symbols |
| DECLINED COLLISIONS | 11 | read the DECLINED.md row FIRST; a collision means read the decision, not port the test |
| UNCLASSIFIED | 15 | never judged at all |
| ACTIONABLE (SCOPE A/D/MIXED) | 15 | verdict A is known overstated — trace, do not trust the letter |
| LOW-PRIORITY (SCOPE B/C) | 18 | |
| REMOVED AT NEW PIN | 3 | retire the ledger rows |
| CLOUD-DROPPED | 13 | counted, not listed; no action |

**Consequence for the numbers above: the 52-commit port queue was only the CODE
debt.** This is TEST debt on top of it, and the 228 in particular were invisible
to every check the round ran until the queue was generated. The previous re-pin's
equivalent figure was 284 tests across 63 files.

### Tally — counted from shard tables, NOT from their totals lines

| bucket | count |
|---|---|
| CLOUD | 64 |
| N/A | 29 |
| **NOT-PORTED** | **48** |
| SCOPE-DECISION | 20 |
| ALREADY-PRESENT | 6 |
| PARTIAL | 4 |
| PORTED | 0 |

**93 of 171 (54%) are out of scope.** Port queue = 48 NOT-PORTED + 4 PARTIAL = **52**.

**Do not trust an agent's own totals line.** Two of ten were wrong, and the
error is invisible to a sum check because in both cases the mistakes cancelled
within the shard: shard I reported `SCOPE-DECISION 10 / N/A 21` where its rows
say `8 / 23` (both sum to 36); shard G reported `NOT-PORTED 8 / CLOUD 6` where
its rows say `7 / 7` (both sum to 18). The counts above were obtained by
enumerating bucket labels row by row. Any future round should do the same and
should not re-derive these from the reports' summaries.

### REFUTATION PASS 2026-08-29 — corrections to the scope above

**2 of 6 refuters reported so far. The scope was wrong in both directions.**
Everything in this subsection is verified by the coordinator, not taken on the
refuter's word. Nothing was compiled.

#### P0 — two live data-leak defects, found by refuting an ALREADY-PRESENT verdict

`27f8ee6c` was bucketed ALREADY-PRESENT because its GEAP file is absent. Two of
its other files ship here **at the pre-fix state**:

- [ ] **`crates/ai/src/index/file_outline/native.rs:88` — dumps the entire
      repository's parsed outline into the log.** The code is
      `if let Err(e) = sender.send(result) { log::error!("... {e:?}") }`. The
      channel is a `futures::oneshot` (`:67`), and `oneshot::Sender::send`
      returns the **value** on failure — so `e` IS the
      `HashMap<FileId, Outline>`, i.e. every symbol name from every indexed file
      in the repo, Debug-formatted into the log file. Coordinator-verified by
      reading both the channel construction and the call site.
      Fix: `if sender.send(result).is_err() { log::error!("<static message>") }`.
- [ ] **`crates/warpui/src/windowing/winit/event_loop/mod.rs:2007` — logs the
      user's typed keystrokes.** `{:?}` on `EventLoopClosed<CustomEvent>` includes
      the wrapped `SoftKeyboardInput` payload. Fix: format with `Display`
      (`anyhow::Error::new(e)` or `{e}`), not `{:?}`.
      **ADDITIONAL, NOT IN UPSTREAM'S FIX — found by the coordinator while
      verifying:** line **`:2005`** is
      `log::debug!("Soft keyboard callback received input: {:?}", input)`, which
      logs the same payload unconditionally on the success path at debug level.
      Upstream's commit does not touch it. Porting `27f8ee6c` alone leaves the
      larger hole open. Fix both lines or neither.

#### Newly added to the port queue by refutation (9)

- [ ] `6696954c` — **fully refuted as CLOUD.** `CtrlCCancelsThirdPartyHarness` is
      "purely client-side status synthesis; the harness process/sandbox are never
      signaled" (its own doc), and its consumer `CLIAgentSessionsModel` is live
      here. **Sequence with `9921300b7`** (already queued) — it is that commit's
      stable-promotion, not a standalone change.
- [ ] `b1731dde0` + `8936686f2` — **refuted as N/A.** Both touch
      `crates/warpui_core/`, which the fork ships, at the pre-fix state:
      unthrottled per-frame `log::error!` at `runtime/mod.rs:667`, `:895` and
      `elements/flex/mod.rs:277`. `8936686f2` read as absent only because the fork
      flattened `elements/gui/*` -> `elements/*` — a rename. **Same shape as the
      already-queued P0 `8ba01aa1a`; land all three together**, same
      `warp_errors::` -> `warp_core::errors::` adaptation.
- [x] `98b1f5af8` — **refuted as N/A.** Fork ships both touched files at the exact
      pre-fix state; the U+21E7 fallback mismatch is live. 3-line port plus the
      generator hunk so it is not regenerated away.
- [ ] `d2cb17abb` — **refuted as ALREADY-PRESENT.** The throttle is absent; the
      fork's call site is a fork-local split (`oauth.rs:638`, not `native.rs`) on
      the spawn-failure/reconnect path — the hot path that produced upstream's
      4.4M events.
- [ ] `d13a30f4` (part) — the `TuiLink::render` signature refactor only. Honest
      caveat from the refuter: no behaviour change, no new coverage; value is
      purely reduced re-pin conflict surface. Droppable on triage — but not CLOUD.
- [ ] `b870d25d7` (part) — `script/windows/prepare_bundled_resources.ps1:51`
      `Split-Path` argument-binding fix, byte-identical to the pin's pre-image. The
      commit message never mentions it, which is why the bucketer missed it.
- [ ] `da434eb6e` (part) — the `archive_for_platform()` / `archive_sha256()` hunks
      of `script/install_cargo_binstall` only. On native Windows arm64 `uname -m`
      returns `unknown` -> empty SHA -> hard exit; under WOW64 it silently
      installs the x86_64 build. Only bites an arm64 Windows dev box.

#### Not ports — ledger entries the refutation produced

- [ ] `e054075b8` — **refuted as N/A, but do NOT port the commit.**
      `code_editor_line_number_mode` is registered (`app/src/settings/editor.rs:231`)
      and honoured by the editor (`app/src/code/editor/view.rs:1279`) but has **no
      settings UI anywhere** in the fork — and the old pin HAD one
      (`42effe840:app/src/settings_view/features_page.rs:1386`). Unfiled in both
      TODO.md and DECLINED.md. Destination if wanted is the fork's `code_page.rs`.
- [ ] `8cbb01d45` (partial) — the split itself is pure, but the pin-side path
      `app/src/workspaces/user_workspaces.rs` ceases to exist at `4111d08f9` and
      fork tooling keys on it (`docs/SWEEP-INVENTORY.md:944`). Confirm
      `generate_repin_queue` / `generate_pin_identity_manifest` follow the rename
      rather than reading delete+add.

#### Verdicts that survived but whose STATED REASON was wrong

Recorded because a wrong reason invites the next round to re-derive it wrongly:
`7feb88b5e` and `a9c0a1eb` (ALREADY-PRESENT -> correct label is DECLINED);
`a1cc3a3d`, `b277c0eb0`, `532667498`, `b0a638117`, `0209de56e`, `d511e17e5`
(N/A "file absent" -> the fork ships the file; right conclusion, wrong reason).
**The N/A rationale was factually wrong for 11 of 29** — the fork ships the
touched file in every one of those cases.

#### Data-integrity finding: shard I's report is unreliable in three ways

1. Its totals line was wrong (SCOPE-DECISION 10 / N/A 21 vs rows 8 / 23).
2. Its `ime_marked_text` verdict ("NO ACTION, nothing is dark") contradicted its
   own cited evidence; adjudicated against it.
3. **Three shas in its commit table do not resolve** — `3535362d7`,
   `e83d07d8b`, `1704db4cf`. The correct values (`3535362d7`, `e83d07d8b`,
   `1704db4cf`) are present in the git-generated commit map, so the corruption was
   introduced in the report, not upstream of it. `git show` on the printed strings
   would have errored, so those three were classified without inspection or
   mistyped after it. All three do check out as N/A once corrected.

**Consequence: shard I carried the two highest-value sweeps (Phase 3.5 dependency
drift, Phase 6.7 feature drift). Its dependency table was independently verified
correct by the coordinator; its feature table has NOT been.** Re-verify Phase 6.7
before acting on it.

### TWO BROKEN TRIPWIRES — coordinator-verified 2026-08-29. Fix before porting `workspaces/` or `permissions.rs`.

#### T1 — DECLINED.md:173's guard cannot fire at the new pin, and its citation is dead

`DECLINED.md:173` keeps `is_ai_allowed_in_remote_sessions` hard-`true` and cites
`42effe840:app/src/workspaces/user_workspaces.rs:1753`. **Verified: that file
exists at `42effe840` and is GONE at `4111d08f9`** — upstream split it into
`user_workspaces/{mod,team_workspace_settings,billing_workspace_settings}.rs`.
The function is now
`4111d08f9:app/src/workspaces/user_workspaces/team_workspace_settings.rs:474`,
generic over `TeamScope`, reading the team setting first and **defaulting an
unresolvable team to `false` (deny)**.

Three consequences, in order of danger:

1. **The fork's stub lives in a file upstream deleted.** It is
   `app/src/workspaces/user_workspaces.rs:751`. A structural port that adopts the
   new directory layout carries the new body in wholesale and removes the stub
   *with the file it lived in* — **no merge conflict, no test failure at the port
   site.**
2. **The named tripwire would not fire.** The stub's own doc comment says
   "`is_ai_allowed_in_remote_sessions_ignores_workspace_settings` is the test that
   fails if this is ever undone." That test builds a **workspace**-side setting;
   at the new pin the primary read is **team**-side. A team-scoped restoration
   passes it. **The guard needs a team-side arm added before `workspaces/` is
   touched.**
3. The pin's default moved toward denying, so the decision is more necessary than
   when it was written, not less.

- [ ] Re-anchor `DECLINED.md:173`'s evidence to the new path.
- [ ] Add a team-side arm to `is_ai_allowed_in_remote_sessions_ignores_workspace_settings`.
- [ ] Record the file split as a re-pin hazard (same family as `8cbb01d45`).

#### T2 — `keep:` markers on FORK-ORIGINAL symbols can never fire

Collision detection matches `DECLINED.md` markers against the **upstream** diff. A
`keep:` marker naming a symbol that exists only in this fork therefore can never
match, silently exempting the entire "we are ahead of the pin" class of
divergence — the class most dangerous to revert.

**Coordinator-verified at `4111d08f9`:** `denylist_match_candidates` and
`unquoted_command_parts` have **zero** hits upstream, so their markers are inert.
**The refuter's third example is wrong**: `hide_env_values` DOES exist upstream
(`4111d08f9:crates/warp_cli/src/lib.rs`), so that marker can fire. The hole is
real but narrower than reported — it affects fork-original symbols only.

Live exposure: `app/src/ai/blocklist/permissions.rs:1524`'s denylist unquoting fix
is guarded only by `keep:denylist_match_candidates` / `keep:unquoted_command_parts`,
both inert, while upstream restructured `get_execute_commands_denylist`'s
signature in this range. The intended backstop is a red test at build time — and
this round has no build.

- [ ] Give every `keep:` row on a fork-original symbol a SECOND marker keyed on the
      pin-side symbol or path it diverges from.
- [ ] Audit `DECLINED.md` for other inert `keep:` markers.

### DATA-INTEGRITY: five shas in the shard reports did not resolve

Found by two independent refuters and then swept exhaustively by the coordinator:
every sha cited in this section was validated with `git cat-file -t`. **7 of 109
did not resolve; 5 were real typos, 2 were false alarms.**

Corrected (each verified against the target commit's subject line before replacing):

| as reported | correct | commit |
|---|---|---|
| `1c925e330` | **`1c925e333`** | Bound rayon fan-out in `EditDelta::layout_delta` (APP-5392) — **was in the PORT QUEUE** |
| `cff5f778e` | **`cff5f778c`** | `lint_powershell`: report findings from every source — **was an ACCEPTED scope decision** |
| `1704db4c3` | **`1704db4cf`** | Update common skills lock |
| `3535362d2` | **`3535362d7`** | Bump command-signatures to 32a7fd56 |
| `e83d07d8d` | **`e83d07d8b`** | Bump command-signatures to d3725aa |

Not typos, correctly non-resolving: `b0886a952` and `fe3526693` are revs in
*other* repositories (`warp-proto-apis`, `warp-command-signatures`) and will never
resolve against this one.

**Why this mattered:** two of the five sat in actionable entries. A port agent
handed `1c925e330` gets "unknown revision", and the plausible failure is a report
of "nothing to port" rather than an error — silently dropping the work.

**The typos came from at least two different shards** (I and C), so this is a
systemic reporting-quality problem, not one bad agent. **Any future round must
validate shas against `git cat-file -t` before briefing anyone**, and a
coordinator must never pass an agent a sha it has not resolved itself.

**Coordinator error worth recording too:** the first sweep reported 184 of 282
non-resolving, which was nonsense — it sliced TODO.md to end-of-file (catching
every unrelated sha in the older sections) and validated against the 171-commit
range map instead of the repository, so it flagged the pins themselves. The
lesson is the same one this round keeps relearning: check the checker before
believing an alarming number.

### PARTIAL-HUNT RESULT — the premise was wrong, and that is the finding

A dedicated refuter swept **167 of 167** commits for missed partial ports, on the
premise that 4-of-171 was implausibly low against Phase 6.5's history. **It found
none, and explained why convincingly: in this range the fork has ported essentially
nothing** (PORTED = 0 across all 171), so the `01778efe` failure mode — which
requires the fork to have taken *part* of a commit — has almost no surface to occur
on. The premise is refuted, not merely unconfirmed.

**Phase 6.5 step 4's precision in this range was 0 of 67.** Its mechanical
"some present = PARTIAL" rule flagged 67 commits and every one was a false positive,
in three systematic classes: commits that MOVE code within a file (moved lines grep
as added and are already present); commits that add a second instance of existing
boilerplate; and generic identifiers (`render`, `Event`, `Action`, `should_render`,
`Cache`, `insert`, `floor_char_boundary`). Two cheap filters would have cut 67 -> 45
-> ~8: restrict to identifiers introduced by the commit and absent from its own
pre-image, then require absence from the fork's pre-existing vocabulary. **Worth
writing into `docs/pin-migration.md` Phase 6.5**, because the current phrasing
invites an agent to hand-investigate 67 dead ends and quietly stop early.

**The real risk this round is the adjacent shape:** ledger entries that describe
less than their commit does. `cff5f778c` above is the worked example, found by this
sweep.

### ADJUDICATED: do NOT port `391dd76ad` standalone

Two refuters conflicted. One called `with_bounded_retry_using` "the only substantive
salvage" in its territory; the other said porting it would trip
`script/check_stub_coverage`. **Coordinator ruling: do not port it standalone.**

Both agree on the fact — the first raised the same caveat itself. Verified here:
`create_managed_mcp_client_config` and `ManagedMcpResolutionFailed` have **zero hits**
in `app/src` or `crates`, so the generalised helper would land with **no caller**.
`script/check_stub_coverage`'s own header states the principle: *"A test ported
against a stub COMPILES AND PASSES while asserting nothing. That is worse than a
missing test: it is fake coverage that looks like progress."* A callerless helper
plus its tests is that shape.

Port it only if and when a fork consumer exists. The commit stays out of scope — but
note its *stated* reason was wrong: the fork DOES ship the touched file, at
`app/src/util/retry_strategies.rs` (the `server/` -> `util/` relocation), so "file
absent" was never the reason.

### Two more corrections from the partial hunt

- **`b870d25d7` — the recorded port is NARROWER than written.** Upstream fixes two
  `Split-Path` sites; only `script/windows/prepare_bundled_resources.ps1:51` exists
  here. The fork's `script/windows/bootstrap.ps1` is a diverged, shorter script with
  no `gitUsrBinDir` block. Separately, `script/windows/rebuild_icon.ps1:8` carries
  three more positional `Split-Path -Parent` calls of the identical shape that
  upstream never touched — cover them or say why not.
- **`04a7f8342` was unrecorded entirely.** Correct verdict ALREADY-PRESENT:
  `crates/warpui_core/src/core/app.rs:1937,1984` are already at the state upstream
  reverts *to*, because the fork never took the earlier
  `log::error!` -> `report_error!` migration. One residue worth a one-character port:
  upstream's post-fix form is `{error:#}` (full anyhow cause chain), the fork's is
  `{error}` (top frame only).
- **`19dc50535`** has a hunk in a fork-present file (`script/presubmit`) adding
  `./script/test_factory_files_skill.py`, which does not exist here. Recorded so the
  next round does not rediscover it as an unexplained gap.

### QUEUE GENERATOR DEFECTS — fix before the next round

- [ ] **Renames are reported as removals.** All three "REMOVED AT NEW PIN" entries
      are moves: `user_workspaces_tests.rs` -> `user_workspaces/user_workspaces_tests.rs`;
      `app/src/bin/generate_settings_schema_tests.rs` -> `app/src/settings/schema_generation_tests.rs`;
      `app/src/util/path_tests.rs` -> `crates/warp_util/src/path_tests.rs`. The last two
      are then **re-reported at their destinations**, so the same tests are both
      retired and double-counted.
- [ ] **`sym:` markers match substrings.** `sym:SettingsMode` fired on
      `OpenWarpNewSettingsModes` in **3 of 11** DECLINED collisions (27% false
      positives), every one on a line upstream deleted. Anchor to identifier
      boundaries.

### THE UNADJUDICATED TOTAL IS 1,100 TESTS, NOT 228 — and a third blind spot is open

The queue's own reconciliation block: 3,123 pin tests have no same-named fork
test; 2,023 carry a ledger row; **1,100 do not**, splitting into the 228 reported
here (42 files the ledger covers) and **872 across 101 files the ledger does not
cover**. The 872 reach the queue "only if the file changed between the two pins"
— so any of those 101 files upstream did NOT touch this round appears in **no
section of the queue at all**. That is a third blind spot, roughly **4x the size
of the one #592 closed**, and it is live.

Sizing this round off 228 understates standing test debt by ~5x.

**#603 is getting worse, not stable:** STATE.md's smaller figure now cancels **500
rows** (387 naming tests the fork has, 113 naming tests not at this pin), up from
277 at `42effe840`. Do not reconcile against STATE.md.

Of the 228: **72 portable, 136 not portable** (declined/cloud/crate absent), **20
need a decision**, **3 are false positives** — already ported under deliberately
different names with "Ported from the pin's ..." doc comments, which also means
the bucket cries wolf as well as under-reporting.

### PORT QUEUE — PARTIAL first (4). Highest value; invisible to every gate.

Phase 6.5's whole point: a partially-ported commit passes review, passes CI, and
passes its own upstream test, because the test came across too and cannot detect
what was dropped.

- [ ] **`146684ee` — IME marked text is dark on Linux, the fork's own platform.**
      The Windows half landed independently (with its own rationale comment); the
      Linux half never did. Every path is closed on Linux: `RELEASE_FLAGS`
      (`crates/warp_features/src/lib.rs:907`) is `cfg(any(macos, windows))`, so is
      `app/src/bin/phosphor_oss.rs:51`, `ime_marked_text` is not in `default`, and
      `DOGFOOD_FLAGS` is inert here. Upstream added it to `default` in this range.
      **Two shards disagreed on this; adjudicated by hand 2026-08-29 — the fact is
      confirmed, the remedy is not.** The fork's own comment asserts winit supports
      marked text "on macOS and Windows", so deleting the two cfgs needs runtime
      verification that winit delivers preedit on X11/Wayland at this fork's rev.
      Do NOT ship a cfg deletion on upstream's say-so.
- [ ] **`1a29f680d` — shared recovery budget across MAA retries and resumes.**
      Backoff already landed here independently by a *different* mechanism
      (`response_stream.rs:544`), so a naive port installs a second backoff on top.
      The `RecoveryBudget` unification did not land. `controller.rs:3961` carries a
      comment claiming `can_attempt_resume_on_error=false` "prevents an infinite
      loop" — that is the conflation upstream identifies; it does not bound
      recovery, it closes it. Provider-agnostic, not Warp-specific.
- [ ] **`e1bcf5d07` — AI-page split. One hunk MUST NOT be ported.**
      Two halves already present independently, and the fork's version is stronger
      (`persistence_key()`/`from_stable_key()` vs upstream's `slug()`). **Do not**
      port the `app/src/local_control/handlers/app_state.rs` switch to `from_slug`:
      the fork's `from_str` there is deliberate and documented, backing the
      `surface.settings.open` scripting contract, and must stay locale-independent.
- [ ] **`0a7d5380e` — wasm/web guards. Port 2 of 6 hunk groups.**
      Portable: the wasm early-return in `insert_notifications_discovery_banner`
      and the `ConversationView` arm of the `WasmNUXDialog::should_display` guard.
      Not applicable: `maybe_add_buy_credits_banner`,
      `check_and_trigger_free_ai_removal_modal`,
      `open_prompt_suggestions_unavailable_modal` — none exists here (credits/billing
      declined) — plus the five `&model` threading hunks that exist only to feed them.

### PORT QUEUE — NOT-PORTED (48)

Ordered by area. `P0` = live user-visible defect confirmed present in the fork.

**Shell integration / completion (7)**

- [x] `e722ebed` **P0 — four independent bugs, one is a PANIC.**
      `crates/warp_completer/src/meta.rs:97` slices `&source[start..end]` with no
      char-boundary clamp -> panics on a multi-byte command line (the fork already
      has a `floor_char_boundary` helper in two places, so the fix is a call).
      Plus: `fish.sh:181,185` generator jobs are **never** killed in either
      direction (empty command substitution makes `test` always false, AND
      `kill -9 $pids` where the loop binds `$pid`); `bash_body.sh:768` /
      `fish.sh:73` use `echo|od` instead of `printf '%s'`, corrupting DCS JSON
      (note `zsh_body.sh:577` already does this correctly — the fork is
      inconsistent across its own three shells, not divergent); PowerShell
      kill-buffer chord written in the same read as command text.
- [x] `fbbfc41f3` **P0 — grep tool `ParseIntError` discards ALL matches.** PORTED
      (branch `port/grep-parse`): `--null`/`-z` + `parse_null_delimited_grep_output`
      + BusyBox `run_grep_per_file_fallback`, and all 25 of the commit's new tests.
      The Phase 2 queue attributed the six
      `build_grep_content_scan_command_*_quoting_layers` tests to a separate work
      item; they are in `fbbfc41f3` itself, so they landed here.
      `parse_grep_output` (`grep.rs:650`) splits on `:` and parses field 2 as a line
      number, so a Windows drive path over the remote-server extension or a Go
      vendor path (`vendor/example.com/foo:v1/x.go`) makes it `return Err` on the
      FIRST bad line and drop every other match. All three backends route through
      it. **Three fork tests pin the exact emitted command strings and will need
      updating — that is a real behaviour change to the command, NOT a §5.6 test
      weakening, and the PR must say so.** Keep upstream's `--null` long option:
      on BSD/macOS grep `-Z` silently means `--decompress`.
- [ ] `294033bb` **P0 (pairs with `748b635c`)** — zsh `^P` bound to bare
      `kill-buffer` on `main` only; a user rc switching to vi mode makes the
      buffer-clear a no-op, leaking bootstrap residue into the next command.
- [ ] `748b635c` **P0 (pairs with `294033bb`)** — same defect in `pwsh.ps1`.
      **Trap:** the fix ADDS a second `Warp-Configure-PSReadLine` call inside
      `Warp-Finish-Bootstrap`; the fork has exactly one call site (`:452`, precmd).
      Do not "fix" this by relocating the existing call.
- [x] `213c9b32` — unbounded `SignatureCache` growth: append-only `MemoMap` keyed on
      the lowercased first token, retaining every **miss** forever with no length
      cap. Fork test file is `registry_test.rs` (singular) — a rename, not a gap.
- [ ] `79a9cb72` — completer resolves an option's argument by value position; needs
      a `name_span` field on `NamedArgument`.
- [ ] `4e49d04f` — **two separable ports, both valid.** (a) `parse_ls_script_output`
      refactor + truncation/malformed-output guard: cross-platform, applies to every
      legacy-SSH listing, 8 new unit tests. (b) WSL guest enumeration: Windows-only.
      Land (a) alone if you want the low-risk half. The `-L` already present here
      came from `1b65a8b9`, not this commit — not a partial land.

**Terminal / rendering (6)**

- [x] `ee95ac0fd` **P0 — double cursor in finished background blocks.** Fork
      computes one `cursor_visible` from `SHOW_CURSOR` alone
      (`block_list_element.rs:2586`) and feeds both grids, while gating `draw_cursor`
      differently. ~30 lines, 2 files; every helper already exists. Clean lift.
- [x] `8ba01aa1a` **P0 — `grid_renderer` OOB logs EVERY FRAME** in the ligature path
      (`:1137`), ungated and unthrottled; the non-ligature path (`:627`) is
      debug-only. Fork already has the macro surface (`ReportErrorLogMode::OncePerRun`);
      only adaptation is `warp_errors::` -> `warp_core::errors::`.
- [ ] `63a17a50a` — denormalize `is_passive` onto `AIBlock`; 7 call sites still
      re-derive from history.
- [ ] `f42c4ab6c` — `Lazy` field deferral (22 files, ~987 insertions). Adds
      `crates/warp_util/src/lazy.rs`. **Depends on `ee95ac0fd` landing first.**
      One hunk targets `local_tty/terminal_view_adaptor.rs`, dropped by the fork.
- [ ] `0a0fd3ae1` **(ordered pair, land before `c25ac4070`)** — Paste entry in the
      block-list context menu; introduces the `paste_menu_item` helper the other needs.
- [ ] `c25ac4070` — right-click behavior setting. **Costs 21 call sites across 14
      files** (`on_right_mouse_down` gains `&ModifiersState`), two of which upstream
      does not touch. Carries an unadvertised fix: right-click over an app that owns
      the mouse forwards a raw event instead of opening Warp's menu.

**Editor / UI framework (5)**

- [ ] `ee351a0e7` **P0 — TUI never terminates on host-terminal disconnect.** Reader
      thread logs (`runtime/mod.rs:842`) and breaks; nothing calls `terminate_app`.
      Also adds an `ErrorKind::Interrupted` retry — today an `EINTR` kills the reader
      loop permanently. `thiserror` and `libc` are already deps. **Port this instead
      of `b1731dde0`, which it rewrites.**
- [ ] `d89e78385` **(land before `1c925e333`)** — `Arc` the layout delta; upstream
      measured multi-GB transient allocation when two editors share one `Buffer`.
- [ ] `1c925e333` — layout chunking + line-length cap. **`730a4acc0`-shaped risk:
      `truncate_text_for_layout` silently drops text before shaping, and upstream's
      safety argument is an assertion about UPSTREAM's offset invariants.** Trace this
      fork's frame-offset clamping and `BlockMarker` 1-indexing first. The chunking
      half is coordinate-free and can be ported alone.
- [ ] `12e455c56` — macOS Core Text style-run coalescing (~36 lines + 3 tests).
      Upstream attributes an ~11.98 GB spike to it. **Upstream never built or ran
      this** (no macOS CI) — needs a real macOS build here, not a rubber stamp.
- [ ] `dc1077845` — monomorphization bloat in `warpui_core` spawn/update. Compile-time
      only. Preserve the `pending_flushes` reorder if ported.

**Search / workspace / system (10)**

- [ ] `36dd2cc2` **P0 — unbounded search channel.** `async_channel::unbounded()`
      (`app/src/search/searcher.rs:1249`) with all three consumers doing
      clear-then-rebuild per event. ~300 lines + ~500 test lines; relocate from
      `crates/warp_search_core/`. **Upstream's revision 1 design was wrong** — a naive
      side-slot coalescer reorders an insert issued between two rebuilds; ship the
      sequence-number + per-commit-chunking design.
- [ ] `90c2484d` **P0 — non-remappable shadowed keybinding, present here with a
      DIFFERENT keystroke.** Fork's `CustomAction::ToggleProjectExplorer` is
      `ctrl-2`/`ctrl-shift-2` (`util/bindings.rs:419`) where upstream is `ctrl-1`/`alt-1`.
      The 11-line deletion is keystroke-independent. **Upstream never built or tested
      this revision, and its "editable beats fixed" claim was code-inspection only** —
      re-derive against `warpui_core/src/keymap/matcher.rs` before landing.
- [ ] `b4a2a8fa` **P0 — stdio MCP servers cannot start in TUI/SDK on a fresh profile.**
      Fork is behind even upstream's pre-fix state: `native.rs:741` hard-requires
      `mcp_execution_path`, whose only writer is the GUI bootstrap.
- [ ] `092c1dce` — preserve scroll fraction across the markdown Rendered/Raw toggle.
      **Port requires EXTENDING a fork test, not weakening it**: add
      `scroll_fraction: None` to the exhaustive literal at `notebooks/file/mod_tests.rs:530`.
      Fork uses `BufferLocation` where upstream uses `LocalOrRemotePath`.
- [ ] `46c0b513` — Windows DPC-watchdog: avoid a full process-table walk per session bootstrap.
- [ ] `eaf70a6a` — oversized-diff early return; fork has `MAX_DIFF_SIZE` and the exact
      insertion point but parses the diff first.
- [ ] `e0d01fff` — **port the `system/info.rs` half only.** It gates a real local
      `MemoryUsageHigh` emit + jemalloc dump that latch for the process lifetime. The
      `telemetry/events.rs` half is dead weight: `send_telemetry_sync_from_ctx!` is a
      compile-only no-op here.
- [ ] `8b88df98` **(land before `40e39717`)** — tab shortcut hints.
- [ ] `40e39717` — follow-up to the above; **impossible to land alone**, every symbol
      it edits is introduced by `8b88df98`.
- [ ] `56921910` — 6-line wasm cfg split of `WORKSPACE_PADDING`.

**Agent / AI (8)**

- [ ] `dff0d13fe` **P0 — skills are invisible to Claude Code and Codex.** The fork
      ships both harnesses and supports `WARP_SKILL_DIRS`, but carries the pre-fix
      gate at `driver.rs:1083` ("Skill loading is Oz-only"). Upstream publishes them
      as symlinks into `.claude/skills` and `.agents/skills`. All deps present.
- [ ] `9921300b7` **P0 — Ctrl-C on a third-party harness reports nothing.**
      `CLIAgentSessionStatus` has no `Cancelled` variant. ~1000 lines incl. tests;
      fully local (PTY byte observation), the harness is never signaled.
- [ ] `bc0f17ce` — structured per-block diff-match failures for agent retry.
      **Preserve** the `RemoteFileOperationsUnsupported` arm's deliberate-divergence
      comment; the commit does not touch it. Its message over-describes the diff.
- [ ] `4cd1c77c4` — file-explorer chip in the native agent-view toolbelt; entirely local.
- [ ] `ff16a0b2a` — `hashbrown` raw-entry + `FxHashMap` in hot paths. `rustc-hash`
      is already a workspace dep; `app/Cargo.toml` needs both added.
- [ ] `216d0efe7` — **port the tooltip half only.** `dismiss_ai_tooltips` currently
      fires an unconditional `ctx.notify()` on every focus change. The recording-span
      cache is dead on arrival (session recording declined, #350) and
      `output.rs:2964` documents why. The `search_codebase.rs` sub-hunk has no target.
- [ ] `d68a638ef` — **5 of 26 sites apply**; the rest already differ because the fork
      never took upstream's earlier `log::error!`->`report_error!` migration.
      `hex_color.rs` (`HexColorError` -> `thiserror::Error`) is the cleanest and is
      entirely local.
- [ ] `9d3f3e1ec` — clone-reduction micro-refactor. **Not mechanical**: relies on
      `Revision` being `Copy`, and it is not here (`cloud_object/server_types.rs:177`).
      No behavioural delta. Lowest value in this group.

**Repo metadata / settings / misc (12)**

- [ ] `6e192572` **(land before `c6609ef2`)** — outer `Arc` on `FileTreeState.gitignores`,
      deep-cloned per event today.
- [ ] `c6609ef2` — inner `Arc` + new `gitignore_cache` module + `parking_lot` dep.
      **Strictly ordered after `6e192572`**; taken out of order the type changes fight
      each other. Final shape at the pin is `Arc<Vec<Arc<Gitignore>>>`.
- [ ] `be11be65d` — **Profiles half only.** Fixes a bogus "(1)" settings-search count.
      Higher value here than upstream: the fork's gate is
      `!is_byo_api_key_enabled()`, so the single-widget branch is the DEFAULT path in
      a BYOP fork, not an edge case. Reshape onto `ai_page.rs` and **keep the fork's
      gate** — a byte-faithful port reintroduces `UsageBasedPricing`. The Code
      Indexing half does not apply (fork's `code_page.rs` builds 11 discrete widgets).
- [ ] `3a7a4a5b3` — suppress empty category headers. Cheap hardening; the fork already
      filters empty index lists, so **do not sell this as a live bug** — no
      configuration was found where it is user-visible today.
- [ ] `25f07935` — MCP logo prefix-match (`"Sentry (OAuth)"`). Scope to the
      `starts_with` change; the fork also lacks 4 icon variants from out-of-range commits.
- [ ] `996babee` — two doc-comment URLs. Zero risk.
- [ ] `69254d73` — TUI focus-ownership hardening (13 files).
- [ ] `94daf47f3` — **two adaptations required, one hunk must be DROPPED.** Re-home the
      settings half from `warp_agent_page.rs` to `ai_page.rs`. The
      `set_zero_state_hint_text` hunk has **no fork counterpart** — the fork rewrote
      that function and deleted `AI_COMMAND_SEARCH_HINT_TEXT`; upstream's change is
      "don't advertise `#` when disabled" and the fork advertises nothing. Drop it.
- [ ] `fa2d43ce2` — widens `assert_eventually!` 20->100 ticks on two tests that exist
      here verbatim. **Not a §5.6 weakening** (condition byte-identical, only the wait
      grows) but it is upstream compensating for UPSTREAM's CI. **Do not port
      speculatively** — only if these go flaky here.
- [ ] `6a96a72d` — settings registration refactor. Compile-time only, no behaviour;
      fork's `macros.rs` already ~121 lines diverged, so a manual rewrite for an
      unmeasured build-speed win. Lowest value in the queue.
- [ ] `5fb3144db` — vim keybindings in the rule content editor
      (`ai/facts/view/rule_editor.rs:112`, `supports_vim_mode: false`). One line.
- [ ] `7795e6728` — vim-mode sweep across 6 multi-line editors; all 6 sites are at the
      pre-fix state here. Six lines.

### Port tasks NOT counted in the 48 (2)

These are real work but do not belong to the NOT-PORTED bucket, so they are listed
separately rather than inflating the queue count.

- [ ] `d019ddfe9` **(portable half of a DECLINED commit)** — `report_error!` ->
      `log::warn!` at `ambient_agents/task.rs:260`. The commit as a whole is declined
      below; this half is unconditionally correct, because the fork's `AgentSource` enum
      is MORE trimmed than upstream's pre-fix one (8 variants vs 13) and so hits the
      unknown-source branch strictly more often. Defensive only — no live wire producer
      of `AmbientAgentTask` JSON exists in the fork today.
- [x] `98b1f5af8` **(bucketed N/A; optional)** — U+21E7 font fallback. `fallback_font_fn`
      is registered only under `cfg(target_family = "wasm")` and the fork ships no web
      build, so this is cheap consistency, **not** a user-visible fix.

### Scope decisions — ACCEPTED 2026-08-29 (maintainer)

- [ ] **`21f413b7` — adopt the terminal crate boundary** (147 files,
      `app/src/terminal/**` -> `crates/warp_terminal/**`, `app/src/util/path.rs` ->
      `crates/warp_util/src/path.rs`). Pure code motion; the decision is about every
      FUTURE re-pin, not this commit. `fb594d2c` (sentry dep) is inert until this lands.
      **`8cbb01d45` poses the identical question for `app/src/workspaces/user_workspaces.rs`
      -> a directory split; four commits in this range already touch the new paths.**
      Until both are settled, Phase 6.5's "drop files absent from this fork" step will
      silently discard real work.
- [ ] **`c5e4a02e3` — both halves.** (a) Delete the unreachable project onboarding step
      (-2587 lines; will need `script/check_large_deletions`). Reachability
      independently verified here: `OnboardingStep::Project` is only constructed in the
      `!ZapNewSettingsModes` branch (`crates/onboarding/src/model.rs:660`) and that
      feature IS in the fork's `default`. (b) Retire `ZapNewSettingsModes` — 10 fork
      files branch on it. **Verify first:** two fork tests
      (`code_page_tests.rs:237,261`) pin the legacy branch with `override_enabled(false)`;
      they assert action dispatch, not the widget list, so flipping them to `true`
      preserves what they assert — confirm that before deleting anything.
- [ ] **`c9e5622943` — the settings-nav/search integration harness half only.** The
      Code-page IA split is declined below with the rest of that program. The harness is
      independently valuable: the fork has 5 helpers in
      `integration_testing/settings/step.rs` against 20 at the pin, and no
      `crates/integration/src/test/settings_navigation.rs` at all. Its position-id scheme
      is keyed on `SettingsSection` variants rather than display labels, which suits this
      fork better than upstream since the fork's `Display` is localized.
- [ ] **`18179177a` + its prerequisite `c25ac4070`** — right-click behavior setting and
      its follow-up copy. `c25ac4070` is in the port queue above with its 21-call-site cost.
- [ ] **`def3fd0e3` — bump `warp_multi_agent_api`.** Real target is the pin's
      `f0028fa6d05db1ba63726eaf6f8d33ab17abe37b` (this commit is an intermediate).
      Compile-surface change; **sequence it BEFORE any port using new API types**, and
      sweep the queue for proto-dependent commits when ordering.
- [ ] **`60d602df6` — MAA teardown race guard (QUALITY-1801). BLOCKED, not schedulable
      yet. Re-verified 2026-08-29; the blocker is real and WIDER than this row said.**
      Its host function `conversation_ready_for_pending_events` does not exist here, and
      `OrchestrationEventServiceEvent::EventsReady` is emitted 3x and subscribed nowhere.
      Porting now adds an unreachable flag plus tests that can only pass — the shape
      `script/check_stub_coverage` exists to catch. **Attach as a prerequisite to the
      deferred orchestration-consumer increment** (`blocklist/mod.rs:21-24`, TODO #310),
      so the race closes the day the consumer lands.

      **This row is also the home of test-adjudication "Cluster A" (6 driver tests) and
      shard E's `drop_pending_events_for_exiting_conversation` (1 controller test).** They
      are not separate work items and must not be scheduled as such; see the corrected
      Cluster A note above. A 2026-08-29 port agent was dispatched to land all 7 and
      correctly declined to write any of them.

      **Root cause, measured — `orchestration_events.rs` is an island.**
      `git grep -l OrchestrationEventService -- app crates` returns exactly two files: the
      module itself and `orchestration_events_tests.rs`. It is **never registered as a
      singleton** (`app/src/lib.rs` has zero hits; the pin registers it at
      `4111d08f9:app/src/lib.rs:2247`), so `OrchestrationEventService::handle(ctx)` in a
      test would lazily hit the `Default` impl and come up with **no history subscription
      at all**. `enqueue_event_batch`, `drain_events_for_request`, `requeue_awaiting_events`
      and `PendingEvent`/`PendingEventDetail` have **no production caller anywhere**. So
      nothing in this fork can drain a buffered child event into a new MAA request, which
      is the only way the defect manifests. The guard would have nothing to guard, and the
      6 driver tests would be asserting against a pipeline that is absent, not broken.
      This is invisible to a symbol-presence check: every symbol the porting brief listed
      as "already present in the fork" **is** present — and unconsumed. Check
      *consumers*, not symbols, before calling this unblocked.

      **Missing-piece inventory (8, not the 2 this row originally named).** Production:
      (1) `OrchestrationEventService` singleton registration in `app/src/lib.rs`;
      (2) `conversation_ready_for_pending_events` + its 3 call sites
      (`4111d08f9:controller.rs:1680/1754/1912/1968`); (3) the async dormant-Claude-wake
      step `maybe_prepare_local_claude_wake` (pin `controller.rs`); (4) an `EventsReady`
      subscriber; (5) `AgentDriver::run_conversation_id: Option<AIConversationId>` — the
      fork's `execute_run` instead keeps `conversation_id_cell: Arc<Mutex<Option<String>>>`
      (`driver.rs:1511`), the server conversation *token string*, populated from
      `UpdatedStreamingExchange`, which is a different value and cannot seed `on_commit`;
      (6) `AgentDriver::skip_initial_turn`. Test-only: (7)
      `BlocklistAIHistoryModel::update_conversation_for_new_request_input_for_test`
      (`4111d08f9:history_model.rs:1154`; the fork has only the non-test
      `update_conversation_for_new_request_input` at `history_model.rs:1090`);
      (8) `ResponseStream::emit_response_event_for_test` /
      `emit_after_stream_finished_for_test` (`4111d08f9:controller/response_stream.rs:309`
      and `:324`) — the cheapest item here, ~10 lines, and both
      `ResponseStreamEvent` variants they emit already exist in the fork
      (`controller/response_stream.rs:810-814`).

      **Note for whoever unblocks this: port `with_on_commit` but NOT its neighbours.**
      The pin's `IdleTimeoutSender` at `4111d08f9:driver.rs:289` grew four things at once
      — `on_commit` (wanted), the `IdleWait` trait (wanted, it is how the pin's tests stay
      off wall-clock time), and `pending`/`arm_refreshable`/`refresh` plus
      `idle_window_for_terminal_status` (**declined**, failure-side only, see
      `TODO.md:6267-6296`). Taking the struct wholesale re-imports the declined half.
      Also note the pin's bodies use let-chains (`if let … && let …`); the fork's copy at
      `driver.rs:120-185` uses nested `if let` and should stay that way.
- [ ] **`c6266ee19`** — verify the installed binary, not binstall's metadata (CI caches
      restore metadata without the binary).
- [ ] **`6e0feaf9c` — SUPPLY CHAIN.** Fork CI still uses the third-party
      `cargo-bins/cargo-binstall` action (`prepare_environment/action.yml:87`,
      `pr-check.yml:249`) while the fork's own SHA-verified script sits unused. Also adds
      a missing binstall bootstrap to the wasm deps script.
- [ ] **`352a7fc10`** — retry + exponential backoff on the binstall download; the fork's
      `curl` has no `--retry` at all, so a CDN brownout fails bootstrap outright.
- [ ] **`cff5f778c`** — **FOUR files, not one. This ledger entry previously described
      only the first and would itself have caused a partial port.**
      (a) `script/lint_powershell` throws on the first source with findings, hiding
      every later source — a gate that under-reports. (b) **`app/assets/bundled/bootstrap/pwsh.ps1`
      lines 433, 510, 515, 579** — `(Get-Location).Path` -> `$PWD.Path` in SHIPPED
      shell integration. (c) `script/windows/bundle.ps1:62`. (d)
      `script/windows/install_build_deps.ps1:6,9`. All four fork sites
      coordinator-verified at the pre-fix state. Cosmetic/lint-driven, but a porter
      following the old entry lands one file of four and the commit reads as done.
- [ ] **`0140af045`** — zsh `compadd` override drops descriptions whenever `-d` arrives
      **clustered** (`-ld`), which is exactly what `_describe` emits — i.e. most zsh
      completions that have descriptions. Fork still has the pre-fix `(I)-d` code at
      `zsh_body.sh:1330-1332`.
- [ ] **`83b4c101e`** — move settings-schema generation out of a separate `[[bin]]` into
      the main binary, removing a whole extra compile from the release path. The fork's
      own release workflow already documents a `SKIP_SETTINGS_SCHEMA=1` escape hatch,
      i.e. it is already paying and working around this. Touches the three diverged
      `script/{linux,macos,windows}` bundle scripts — a real port, not a cherry-pick.
- [ ] **`b1bcc3564`** — add `rust-analyzer` to `rust-toolchain.toml` components. One word.
- [ ] **`1e4b86a81`** — `release-cli` `codegen-units` 1 -> 4; roughly halves that
      profile's build time for ~4% larger stripped binaries. The fork does use
      `release-cli` for the macOS and musl TUI builds.

### Scope decisions — DECLINED 2026-08-29 (maintainer). Do not re-derive.

Each wants a `DECLINED.md` row so the next re-pin does not re-propose it.

- **`3f79221d1`** — make the AI page categorized + per-setting searchable. Declined with
  the rest of the settings-IA program below.
- **`a18026275`** — `PageTitle` / trailing-element infra. Upstream shipped it WITH
  `#[allow(dead_code)]`; porting it alone adds dead code with no consumer here, which is
  precisely what `script/check_stub_coverage` targets. Only meaningful as part of the
  APP-5559 program, which is declined.
- **`d019ddfe9`** — adding 5 server-only `AmbientAgentSource` variants (`Jira`,
  `GitLabWebhook`, `RunScorer`, `Autofix`, `BenchmarkTrial`). No local producer exists;
  every `serde_json::from_str::<AmbientAgentTask>` in the fork is in `task_tests.rs`.
  **Its portable half is still queued above** — declining the variants does not decline
  the `log::warn!` fix.
- **`0e075a072`** — mirroring `OZ_*` -> `WARP_*` env aliases. Consumers are the
  Warp-published harness plugins already removed under #595; this injects Warp branding
  into a fork whose point is not being Warp, for zero local consumer. **TRAP: do not sweep
  up `WARP_CLI_AGENT_PROTOCOL_VERSION` while declining — it IS load-bearing wire
  (`DECLINED.md:182`).**
- **`efbf553ed`** — the `crates/ai_types` split. Upstream's whole design is old-path
  re-exports, so never adopting it carries zero parity risk.
- **`4b894db80`** — hand-written `Deserialize` replacing derives, identical wire format.
  The fork's `dcs_hooks.rs` has already diverged (`HookSessionId`,
  `trim_null_byte_deserializer`, its own `RawPrecmdField`/`PrecmdHookValue`), so this is
  an adaptation rather than an apply — wire-format regression risk in the shell-bootstrap
  path for a compile-time payoff.

### Phase 3.5 — dependency drift introduced by this move

Both were byte-identical to the OLD pin, i.e. the fork was correctly aligned and this
move re-opens them. Invisible to the queue, the identity manifest, and every CI gate.

- [ ] `warp-command-signatures` `fe3526693` -> **`d3725aa42375cc229699c87be2b38f9d9f07080f`**.
      This is the completion-spec data compiled into the binary — the same dep that was
      found two pins stale last round. **One bump; do NOT replay the 7 intermediate bump
      commits** (`2861a6e43`, `3535362d7`, `59bda8db0`, `5bd9b8e15`, `33bb01256`,
      `e326a774a`, `e83d07d8b`) — data-repo revs do not apply as diffs.
- [ ] `warp_multi_agent_api` `b0886a952` -> **`f0028fa6d05db1ba63726eaf6f8d33ab17abe37b`**.
      See `def3fd0e3` above. Compile-surface change; sequence with the code shards.
- [ ] **`tink-core` / `tink-proto` / `tink-hybrid` — pre-existing, NOT introduced by this
      move, and worse than drift.** The fork pins them to a floating
      `branch = "warpdotdev/main"` (`Cargo.toml:593-595`) where both pins use
      `rev = "54b9ac9af93b0c08b446a7bc0582836c9403a71b"`. A branch pin is a
      non-reproducible build input — `cargo update` moves it silently and no gate sees
      it — and no comment says why. They are consumed by `crates/managed_secrets/`.
      Either pin back to the rev or write the reason at the pin line.
      **`docs/pin-migration.md` recorded these as "3 correctly absent (cloud-coupled)",
      which is false; corrected under #625.**
- `winit` — **deliberate, leave alone.** Reason IS written at `Cargo.toml:432-437`
  (carries `rust-windowing/winit#4453`, Windows dark-mode registry detection). Upstream's
  own winit rev did not move between the two pins. *Unverified:* whether
  `jwp2987/winit@9a0788c3a` is still a descendant of `warpdotdev/winit@a4e0ecb5f` — needs
  a fetch nobody has run.
- `session-sharing-protocol` — correctly absent; declined, and the rev did not move.
- 15 deps byte-identical: no action.

### Phase 6.7 — feature-default drift

New pin `default` = 202 entries, old pin = 197, fork = 160. **Only 7 differences are drift
this move introduces**; the other 49 "on at pin, off here" are pre-existing cloud/account-gated
flags already covered by `DECLINED.md`. **Do not touch the 49.**

- `ime_marked_text` — see the PARTIAL entry above; the one that matters.
- `open_warp_new_settings_modes` — **upstream DELETED both the `default` entry and the
  `[features]` declaration in this range**, i.e. graduated the gate away. The fork still
  ships it. Same decision as `c5e4a02e3` (b), arriving from the feature-list side.
- `factory_mcp`, `well_known_mcp_ids`, `orchestration_unified_stack`,
  `ctrl_c_cancels_third_party_harness`, `wait_for_events_parent_registration` — these are
  **unported features, not feature-default drift**; the cargo features do not exist here
  at all. Their commits are bucketed CLOUD in shards G/H.

### Round hygiene — findings about the process, not the code

- **`docs/pin-migration.md` Phase 2.5's bucket table had no row for NOT-PORTED**, the
  largest actionable class (48 of 171). All ten shards independently invented the bucket
  and flagged the omission. Fixed under #625.
- **Two of ten shards reported wrong totals lines**, and a sum check cannot catch it
  because the errors cancelled within the shard in both cases. Count rows, not summaries.
- **`SCOPE-*.md` verdicts are now path-stale as well as count-stale** for terminal and
  workspaces, pending decision on `21f413b7` / `8cbb01d45`.
- **`docs/STATE.md`'s "N are not adjudicated" is still #603** — a subtraction of totals
  rather than a set difference, so it under-reports.

## 🛑 BUILD FREEZE — in force from 2026-08-11 until the maintainer lifts it

**No builds. Nothing that compiles.** Maintainer instruction, 2026-08-11:
builds resume only on the explicit phrase **"Fucking go"** — not on inference,
not on "this seems safe", not on a green-looking tree.

Forbidden: `cargo` (`check`/`build`/`test`/`nextest`/`clippy`), `script/precheck`
in any form including `--fast`, `rustc`, the previously-granted
`lib/rust-genai --manifest-path` exception, and launching any agent that could
run one.

Still permitted: git, grep, reading, doc edits, `script/state`, and the
`script/check_*` guards — all pure shell.

**Why:** the freeze followed an hour-long suite run that turned out to be
compiling three agents' mixed uncommitted work in a shared checkout that had
been switched off `main`. See the hazard entry directly below. A build whose
input nobody can name is worse than no build, because its result gets believed.

## OPERATIONAL HAZARD — agents share ONE checkout (found 2026-08-11)

- [x] **Parallel agents work in the shared checkout `/cache/git/zap`, switch its
      branch, and mix uncommitted work — and the build agent compiles whatever
      that mixture happens to be.**
      **Observed 2026-08-11, ~06:00.** Six sweep agents were launched with
      "branch from local `main`". At least three edited the **shared working
      tree** instead of a worktree. The checkout was left on
      `sweep-tail-2026-08-11` (one agent's branch) holding, simultaneously:
      | file | owner |
      |---|---|
      | `app/src/ai/agent_events/{driver,driver_tests,mod}.rs`, `app/src/server/retry_strategies.rs` | agent-events package |
      | `crates/ai/src/project_context/{model,model_tests}.rs` | project-context package |
      | `TODO.md`, `docs/STATE.md`, `script/state` | coordinator |
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Resolved as an action item. Its own recommended fixes exist: per-agent worktree isolation is standard (`docs/FLEET-ROUND.md`, `script/agent-worktree`), and `script/check_workspace_clean` implements the workspace-clean gate. **Corrected 2026-08-20: "and are in daily use" was false of the gate.** `check_workspace_clean` had zero invocations anywhere in `script/` or `.github/` until 2026-08-21 — it was a script nothing ran. **Still unwired as of 2026-08-21.** Two placements were tried and both were wrong, so it was reverted rather than left broken: `script/precheck` fails every time (precheck exists to run OVER uncommitted work), and `script/agent-cargo` looked right but `precheck:79` and `check_test_failures:43` route cargo through agent-cargo — so it lands back in precheck's path anyway — and CI checks out a DETACHED HEAD, which trips the branch check and would kill every cargo job in `pr-check.yml`. `ALLOW_DIRTY=1` does not bypass the branch check either. Both failed placements are recorded at `script/precheck:259-273`. Wiring it needs a design that distinguishes a human's shared checkout from CI and from an agent worktree. Kept as history. NB the `/cache/git/zap` path is dead; the checkout is `/home/winters/git/phosphor`.

      **Consequence, and the reason this is a hazard and not an annoyance: the
      build agent runs `cargo`/`nextest` in this same tree.** Its results
      describe a mixture no one authored, so a green run proves nothing and a
      red run blames the wrong change. The hour-long suite run in progress when
      this was found was invalid for that reason.

      One agent (agent-sdk-harness) detected the collision itself, extracted a
      patch, reverted the shared tree, and replayed onto a clean worktree. That
      it *could* recover is luck, not design — it happened to notice.

      **Fixes, in order of value:**
      1. Every code agent gets `isolation: "worktree"`, not a shared checkout.
         Doc-only agents may share.
      2. The build agent must build a **named commit in its own worktree**,
         never the shared tree, so its result is attributable.
      3. Nothing should leave the shared checkout on a non-`main` branch.

      **Do not trust any build or suite result that cannot name the commit it
      ran against.**

## WINDOWS SMOKE SUITE — running, 5/19, GUI bootstrap is the blocker

**Status 2026-08-12 (run `31549580850`): 19 total, 5 passed, 8 failed, 6 skipped.**
Linux 12/0 and macOS 13/0 are green.

The earlier "0 passed / 13 failed" reading was a REPORTING artefact, since
fixed: the TUI surface runs as one batched nextest invocation and its single
outcome was stamped onto every scenario. Per-test attribution now parses
nextest's own PASS/FAIL lines.

| surface | Windows result |
|---|---|
| **TUI** | **5 of 6 PASS** — only `usage_tui_transcript_render` fails |
| **GUI** | **0 of 7** — every scenario fails at bootstrap |

- [x] **ROOT-CAUSED AND FIXED 2026-08-17. UNVERIFIED — needs a Windows run.**
      `local_tty/windows/mod.rs` dropped `STARTF_USESTDHANDLES` in `6da414cbc`
      (#215, 2026-06-16). **The pinned oracle sets it** (`42effe840:.../windows/mod.rs:193`),
      as does Warp's own base and Windows Terminal's `ConptyConnection`, which this
      file cites as its model.
      **Why exit 0 with empty output:** without the flag `CreateProcessW` copies the
      PARENT's std handles into the ConPTY child. In CI the parent chain is all pipes
      and `bInheritHandles` is false, so those handles are meaningless in the child.
      pwsh runs the `-EncodedCommand` init, enters the REPL (`-NoExit` works), prints
      its prompt via `CONOUT$` — which is why **InitShell IS emitted** — then does its
      first `Console.ReadLine()` on the broken stdin, gets EOF, and exits 0. The app
      never writes the bootstrap line into a live shell, so the block settles
      `DoneWithNoExecution`. Only pipes break: Explorer launch gives null handles
      (console fills them in), console launch gives ConDrv handles. CI always pipes.
      **#215's rationale was wrong** — MSDN's "handles must be inheritable" governs
      *supplied* handles; null handles + the flag is the documented way to say the
      child inherits no stdio, which is what makes the console subsystem hand it the
      pseudoconsole's ConDrv handles. **#214's 0x80070057 was misdiagnosed** and needs
      re-checking on Windows.
      **Fix:** flag restored via a new `new_pty_startup_info()` (`:260`) with ~30 lines
      of doc comment recording why it must not be "simplified" away again, plus
      `windows/mod_tests.rs::pty_startup_info_requests_null_std_handles`.
      **Also ruled out, so nobody re-checks:** `-EncodedCommand` is correct
      (`shell.rs:711-741`); the TUI/GUI contrast is a dead end (TUI scenarios have no
      PTY at all); `pwsh.ps1` is complete; the CLIXML clue is a red herring; and the
      workflow's own diagnostic (`usage-test.yml:110-128`) **proves nothing** — it runs
      inside an already-redirected `shell: pwsh` step, which is what led lines 119-122
      of this entry to assert the opposite of the truth.
      **Needs a Windows machine:** re-run the job; confirm #214 does not return (if
      `CreateProcessW` really does fail, `PtySpawnError::CreateShellProcessFailed` names
      it immediately). Discriminator if it still fails: `[Console]::IsInputRedirected`
      / `IsOutputRedirected` inside the spawned pwsh — all False means handles are now
      right and the fault is elsewhere. Original entry:
      **GUI bootstrap fails on Windows: the pwsh child exits immediately.**
      Log evidence from `usage_launch_bootstrap`:
      ```
      [ChildExitWatcher] shell pty child exited: exit_code=0 (0x00000000)
      Block finished with new state DoneWithNoExecution
      Assertion 'no block executing' timed out ... Block output is: <empty>
      Test step 'Wait for bootstrapping' failed after 1 attempts
      ```
      pwsh starts, **emits its InitShell hook, then exits cleanly** — the
      behaviour of a non-interactive shell that ran its init and had no REPL to
      enter. The terminal then waits forever for a bootstrap that cannot come.

      **RULED OUT — not tonight's #532 session-ID work.** The `@@WARP_SESSION_ID@@`
      substitution is applied to all four shells including PowerShell
      (`bootstrap.rs:228` covers the whole match result), and the captured hook
      carries a real value:
      `{"hook":"InitShell",...,"shell":"pwsh","session_id":7519507896035157563}`.
      So session-ID registration WORKS on Windows; this is a separate, older
      problem that had simply never been exercised.

      **MECHANISM ESTABLISHED 2026-08-12** by a CI diagnostic (run `31555886629`),
      not by inference:
      ```
      PSVersion: 7.6.4      Host name: ConsoleHost
      stdout redirected: True      stdin  redirected: True
      child-alive
      PS D:\a\phosphor\phosphor>          <- the REPL DID start
      RESULT: child EXITED early with code 0 -- -NoExit did NOT keep it alive
      ```
      A child launched with the same `-NoExit -EncodedCommand` shape **prints
      its prompt** — so `-NoExit` works and the REPL genuinely starts — and then
      exits 0 at once because **stdin is redirected and reads EOF on the first
      read**. The `#< CLIXML` comes from the same redirection.

      So the sequence is: run encoded init → emit InitShell hook → enter REPL →
      stdin EOF → exit 0. The terminal waits forever for a bootstrap that has
      already been and gone.

      **The fix is therefore NOT a PowerShell flag.** `-NoExit` is doing its
      job. The child needs a stdin that is the ConPTY, not a redirected pipe.
      Next step is to establish whether the app's ConPTY child actually
      inherits console stdin — the spawn looks textbook
      (`set_pty_connection`, `EXTENDED_STARTUPINFO_PRESENT`, no
      `STARTF_USESTDHANDLES`), so the discrepancy is between that code and what
      the child observes. Do not start by changing `shell.rs` args.

      **Original second signal:** PowerShell **CLIXML** envelopes (`#< CLIXML`, `<Objs …>`)
      are interleaved with the DCS payloads in the captured stream. pwsh emits
      CLIXML when its non-stdout streams are redirected. Worth establishing
      whether that redirection is also what denies it a REPL.

      **Not a CI defect.** The workflow, the runner and the reporting are all
      working — this is a real Windows product bug that three platforms of CI
      finally made visible.

- [x] **DONE 2026-08-18 — rmcp migrated to crates.io `1.6`, the fork dependency is GONE.**
      `Cargo.toml:464` is now `rmcp = { version = "1.6" }`, byte-identical to
      `42effe840:Cargo.toml:420`. The `warpdotdev/rmcp` git pin is removed. SSE transport
      vendored from the oracle's own solution into `app/src/ai/mcp/sse_transport/`,
      **byte-identical to `42effe840:crates/mcp/src/sse_transport/`** so future re-pins diff
      cleanly. Both warpdotdev patches were already upstream, so nothing was lost.
      **Two things were larger than scoped, both found by verifying rather than assuming:**
      5 extra `StoredCredentials` literals in `oauth_tests.rs`, and 5 SSE call sites not 4.
      Several predicted breaks were NOT breaks (`ErrorData` still intentionally exhaustive;
      two inner `ServiceError` matches already had catch-alls) and were left alone.
      **`Cargo.lock` untouched — must be regenerated by cargo.**
      ⚠️ **Five silent behaviour changes, no compile error and no test coverage.** The one
      to watch: at *exactly* 1.6.0 a single junk line on a child MCP server's stdout
      **closes the session** — a regression against the pre-migration build. 1.7/1.8 reply
      `-32700` and survive; `^1.6` admits 1.8.0. **Check what the lock resolves to.**
      Others: negotiated protocol version moves 2025-03-26 -> 2025-11-25;
      `start_authorization(&[], ..)` now auto-derives scopes from RFC 9728;
      `AuthorizationSession::new` returns `RegistrationFailed` where 0.10 fell back to a
      hardcoded client_id; `discover_metadata` returns `NoAuthorizationSupport` rather than
      guessing endpoints. Original entry:
      **`usage_tui_transcript_render` fails on Windows** (the other 5 TUI
      scenarios pass). Almost certainly a line-ending or width difference in a
      rendered-transcript assertion; needs the per-test detail pulled from a
      run, which the batched failure_detail currently truncates.

## VERIFIED WORK QUEUE (2026-08-11, post-sweep)

**The 50-test MISSING-SUBSYSTEM sweep is COMPLETE** — all 50 resolved on branch
`working`. What follows is the remaining code work, **after every premise was
re-checked by grepping for a DEFINITION rather than a name.**

**That check mattered: 6 of the ~18 items were false.** `rollup.rs` (claimed
absent with 8 tests — it exists, with exactly 8 tests), `/index` slash command
(exists), and 3 entries left unchecked under headers already reading
"IMPLEMENTED". One entry I *wrongly* called stale is genuinely open:
`RemoteServerClient::resolve_conflict` really has zero callers — the two hits
are `GlobalBufferModel::resolve_conflict`, same method name, different type.

**Confirmed genuinely absent — the real queue:**

| # | item | evidence |
|---|---|---|
| 1 | **`git pull`** (Stage 1, `--ff-only`) | zero hits for `git_pull`/`GitPull` tree-wide |
| 3 | **`language_by_filename` signature** | fork takes `&Path`; pin takes `&StandardizedPath` + `language_by_filename_parts` |
| 4 | **MCP tool results render as a JSON blob** | `inline_action/requested_command.rs:1494` — `to_string_pretty(result)` |
| 5 | **`with_semantic_selection_by_style`** | no definition tree-wide |
| ~~6~~ | ~~`use_computer_decoration`~~ **— NOT A SYMBOL.** It is a pin *test name*: `output_tests.rs:170 fn use_computer_decoration_skips_screenshot_only_rows()`. Screenshot handling exists (`view_impl/output.rs`, 25 refs). Re-scope against the actual decoration predicate or drop. | corrected 2026-08-11 |
| ~~7~~ | ~~TUI renderer for `MessagesReceivedFromAgents`/`EventsFromAgents`~~ **— FALSE.** `crates/warp_tui/src/agent_block.rs:1311` renders `MessagesReceivedFromAgents { messages }`; `:1318` deliberately no-ops `EventsFromAgents`. Types exist in `convert_{conversation,from,to}.rs`. `agent_block_tests.rs` exists in fork and pin. If the real complaint is the `EventsFromAgents` no-op, file that narrowly with `:1318` as evidence. | corrected 2026-08-11 |
| 8 | **Zap #324 — pane min size** | `MIN_PANEL_WIDTH: f32 = 300.` hardcoded, `ai_assistant/panel.rs:61` |
| ~~9~~ | ~~**Zap #329 remainder** — hunk staging, branch create/switch~~ **— BOTH HALVES FALSE, corrected 2026-08-20.** *Hunk staging ships end to end:* `toggle_hunk_staged` (`app/src/code_review/code_review_view.rs:5856`, called at `:5968`) → `StageTarget::Hunk` (`app/src/code_review/diff_state.rs:163`, matched at `:1259`) → `run_apply_patch_cached` (`app/src/util/git.rs:946`, `git apply --cached`), with a `StageHunkButton` in the gutter (`app/src/code/editor/element/gutter_button.rs`, `code/editor/{element,view}.rs`) and an SSH leg (`app/src/remote_server/server_model.rs:3568`). TODO.md records it landing further down this file. *Branch create/switch was a deliberate removal, not an absence:* `run_create_branch`/`run_switch_branch` were deleted 2026-08-18 with the rationale in place at `app/src/util/git.rs:922-932` — they were local-only and callerless, while the shipping path emits a shell command via `PromptChipShellCommand::{GitCheckout, GitCreateAndCheckoutBranch}`, which also works over SSH. **Why the evidence was wrong:** `no stage_hunk/checkout_branch` was a bare-name grep for symbols this fork never named that way. The rule stated below this table (grep `fn <name>`, and grep the *behaviour* not a guessed identifier) exists for exactly this; the row violated it. | corrected 2026-08-20 |
| 10 | 🛑 **Feature-reduced daemon target — DO NOT START.** Maintainer hold, 2026-08-11. Architectural, gates the distribution decision. No agent is to be assigned this without an explicit instruction. | on hold |
| 11 | **Remote-host global search has no binder** (opened 2026-08-15 by the server-file-browser removal) | `GlobalSearchView::set_server_search_root` (`workspace/view/global_search/view.rs`) now has **zero callers**: its only one was `LeftPanelView::set_server_file_browser_root`, deleted with that feature. It was already unreachable in shipped builds — the caller sat behind the never-enabled `ServerFileBrowser` flag — so this is a **pre-existing** dead path now made visible, **not a regression from the removal**. `server_search_root` is therefore permanently `None`, and since local `DirectoriesChanged` only reports local sessions, global search never sees an SSH root. Decide: re-bind it (natural guard is `SshRemoteServer`, which *is* live in release — so this would newly ENABLE the path, and it has never run in a shipped build) or delete the mechanism. Deliberately not decided during a removal, and not touched under the build freeze. |

**Rule that produced this list, and the reason it is short:** grep for
`fn <name>` / `struct <name>`, never the bare name. Six false positives and one
false negative were caught this way in a single session — including three where
the "missing" symbol appeared only inside a doc comment describing its absence.

## IN FLIGHT RIGHT NOW (2026-08-11)

**`main` is held by the build agent. Do not commit to it.** Standing maintainer
instruction: no commits to `main` until the genai bump builds and the suite is
green. Baseline to beat: **6,846 passing**.

- [>] **Vendored genai 0.6.0-beta.18 → 0.7.0-beta.18 — COMPILES, SUITE
      RUNNING.** `13603ac0f` re-ported the vendor tree, `02024ceaf` merged it.
      **The merge landed before the app-side call sites were updated, which
      broke `main` for ~40 minutes** — a sequencing error; it should have been
      one commit, or a branch verified first. Repaired by `c05e2bd07`:
      `cargo check --workspace --all-targets --features warp/gui` returns
      **0 errors**. The suite has not yet reported; the bar is the 6,846
      baseline. The four call-site fixes were: 

      | file | change | why |
      |---|---|---|
      | `Cargo.toml:252` | pin `=0.7.0-beta.18` | version bump |
      | `app/src/settings/ai.rs:1084` | `GE::None` → `GE::Zero` | 0.7 renamed the variant; `ReasoningEffort::None` no longer exists |
      | `app/src/ai/agent_providers/chat_stream.rs:7631` | same rename, in a test | |
      | `app/src/ai/agent_providers/oneshot.rs:214` | new `ChatStreamEvent::Heartbeat` match arm | 0.7 adds it (Anthropic keepalive); the match is exhaustive |
      **Carried forward, must not be lost on the next bump:** the Vertex
      `:streamRawPredict` fix and the Anthropic cache-breakpoint fix, both
      recorded in `lib/rust-genai/CHANGES-PHOSPHOR.md`; upstream pin recorded in
      `lib/rust-genai/UPSTREAM.md`.
- [x] **DECLINED — MAINTAINER DECISION 2026-08-17: we are not pushing to
      `jeremychone/rust-genai`.** The fix stays vendored in `lib/rust-genai`.
      **Accepted cost, stated so nobody re-files this as an oversight:** the
      patch must be re-applied at every genai bump. It has already been carried
      across two (`13603ac0f` re-ported 0.6.0-beta.18 -> 0.7.0-beta.18), and
      that re-port cost is now permanent rather than one more bump.
      A complete upstream package was prepared before this decision and is NOT
      lost — patch, PR description, reconstructed upstream baseline and the
      post-patch file, in the session scratchpad under `vertex-upstream/`.
      Nothing was ever pushed, no PR opened, no external service contacted.
      **If this is ever reversed**, the package needs one check first: its
      baseline is a *reconstruction*, not a fetched pristine file, so fetch the
      real file at the target ref, diff it against
      `upstream-baseline.reconstructed.rs`, and re-apply the two hunks by hand
      if they differ. Licence position was clean — MIT OR Apache-2.0, no CLA,
      no DCO, standard inbound=outbound.
      Original entry:
      **Upstream the Vertex `:streamRawPredict` fix** to
      `jeremychone/rust-genai`. Every vendored fix we do not upstream is a
      merge conflict we pay for at each bump — this one has now been carried
      across two.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: the app has been run, most of a day of real use.**
      This was the release blocker a green suite cannot discharge (the
      2026-08-10 startup crash passed 6,000 tests). Interactive launch is now
      recorded as done. Original entry:
      **Launch the app.** Nobody has since the freeze lifted. A green suite does
      not discharge this: the startup crash of 2026-08-10 passed 6,000 tests.
      See `docs/build/TRIAGE.md` § "Beyond the compiler" — singleton
      registration order and prompt-cache breakpoint placement are both
      invisible to `cargo check` and to `nextest`.
      **RESCOPE 2026-08-17 (v0.1.0 agent audit)** — the "since the freeze lifted" premise is stale: `script/precheck` went green 2026-08-17 and cargo work has landed continuously since the 2026-08-11 freeze. Whether a human has done an interactive launch is still unrecorded either way, which is the part worth keeping.

## WHAT DONE LOOKS LIKE (maintainer, 2026-08-11)

> "Part of it is having **100% non-cloud parity with Warp**, through either
> port, code, or declined." — and, on review, **divergent counts too, seeing as
> we are a BYOP product.**

That is the first stated definition of done for this project, and it is
measurable. Recorded here because it changes what the sweep buckets *mean*.

**A pin test is RESOLVED when it is one of:**

| resolution | meaning |
|---|---|
| **PORTED** | the test runs here |
| **COVERED-ELSEWHERE** | the fork tests the same behaviour under another name — **must cite the fork test** |
| **DECLINED** | a deliberate decision, with a `DECLINED.md` row |
| **DIVERGENT** | the fork's API or behaviour differs **because it is BYOP** — legitimate, not debt |
| **CLOUD** | outside the definition entirely; the backend is gone and is not coming back |

**Everything else is open work.** In practice that means the
MISSING-SUBSYSTEM bucket, and nothing else.

### Position as of 2026-08-11

Of 1,841 absent pin tests, **1,204 are cloud** — outside the definition by
construction. That leaves a **non-cloud universe of 637**:

**Superseded 2026-08-11 by `docs/sweep-verdict-ledger.tsv`.** The figures first
published here came from the six sweep docs' own summary tables. Four of those
six do not sum to their own per-file sections, so the ledger re-derives every
count from per-file evidence and is the authority. Counts below are the
ledger's.

| | tests | resolved? |
|---|---:|---|
| DECLINED | 405 | yes |
| DIVERGENT (BYOP) | 61 | yes |
| PORTED / PORTABLE / OUT-OF-AREA / DEFECT-FIXED | 59 | yes |
| COVERED-ELSEWHERE | 26 | yes, where cited |
| **MISSING-SUBSYSTEM** | **195** | see below |
| UNPARSED | 5 | no — the source docs never committed to a verdict |

Non-cloud universe: **752** of 1,843 (1,091 are CLOUD).
**551 of 752 resolved — roughly 73%.**

> ## ⚠ THE TABLE ABOVE IS STALE. DO NOT QUOTE IT.
>
> **`MISSING-SUBSYSTEM` is 50, not 195**, and the non-cloud universe is 713,
> not 752. The re-adjudication that produced those numbers is in
> **"Position as of 2026-08-10 — the 195 re-adjudicated"**, further down this
> file, and `docs/STATE.md` (generated) is the authority over both.
>
> **Two traps here, both of which have now caught a reader:**
> 1. **The stale table comes FIRST.** Anyone reading top-down hits 195 before
>    50 and stops, because the table looks complete.
> 2. **The dates run backwards.** This section says *08-11* and the section
>    that supersedes it says *08-10* — because this one was written later from
>    the six sweep docs' summary tables, which the ledger had already
>    superseded. **Do not use the date to decide which is current; the ledger
>    and `docs/STATE.md` are current, always.**
>
> Current, from `docs/sweep-verdict-ledger.tsv`: CLOUD 1,130 · DECLINED 417 ·
> DIVERGENT 65 · PORTABLE 59 · COVERED-ELSEWHERE 58 · PORTED 41 ·
> PORTABLE-OUT-OF-AREA 15 · **MISSING-SUBSYSTEM 50** · UNPARSED 5 ·
> DEFECT-FIXED 2 · MIXED 1. **658 of 713 non-cloud resolved — 92%.**

> **This 73% is NOT the parity figure, and reading it as one is a mistake this
> file has already caused once.** It is *gap-adjudication progress*: the
> denominator is only the non-cloud tests the fork is **missing**, so it
> measures how far through the backlog we are — it excludes every test that
> already passes here.
>
> **Parity is in `docs/STATE.md`: ~89.7% of the pin's non-cloud tests exist
> here, ~96.0% present or deliberately resolved.** That denominator is the
> whole non-cloud universe (~8,626), which is the one that answers "how close
> to the pin are we". Quote STATE.md for parity; quote this number only for
> "how much of the remaining backlog is triaged".

### What resolving the 23 symbols did, and did not, do

All 23 MISSING-SUBSYSTEM *symbols* are resolved (8 renames, 3 cloud, 1 declined,
11 implemented). **That is not the same as those 195 tests being resolved.** The
subsystems those tests needed now exist; the tests themselves are still unported.

So the honest statement is: **the MISSING-SUBSYSTEM bucket is no longer blocked.**
Its 195 tests moved from "cannot be ported" to "portable, not yet ported". Under
the definition above they are not yet resolved — but nothing structural stands in
the way of resolving them, which was not true this morning.

### The two honest caveats on that 61%

1. **DIVERGENT must say *why*.** Divergence earned by being BYOP is a
   resolution; divergence that is accidental drift is debt wearing the same
   label. The bucket currently records *that* the APIs differ, not always
   *that it is BYOP that made them differ*. Entries which cannot state the BYOP
   reason should be re-examined rather than assumed resolved.
2. **COVERED-ELSEWHERE mostly does not cite its covering test.** Until it does,
   it is an assertion rather than a verification. ~26 tests.

Neither is a reason to move the number; both are reasons not to trust it to the
last percent.

### What 100% requires

- The **22 symbols** in the MISSING-SUBSYSTEM section — implemented, or declined
  with a row. Eight agents were dispatched against them on 2026-08-11.
- The two caveats above closed, so the resolved buckets are verified rather than
  asserted.
- The cloud boundary held — already enforced by `script/check_cloud_boundary`,
  and a declined decision can no longer be silently reversed thanks to
  `script/check_declined_collisions`.

### Position as of 2026-08-10 — the 195 re-adjudicated

**Superseded the "23 symbols resolved but 195 tests still open" position
above.** Every one of the 195 `MISSING-SUBSYSTEM` rows was individually
re-checked against the pin and the current fork tree (not just the 23 blocking
symbols) — several of the "established corrections" going into this pass
turned out to be wrong themselves on verification (`ActiveAgentViewsModel` is
**not** relocated, it is permanently deleted — DECLINED.md #418; the
orchestration config-picker layer is **not** open local work, it is `CLOUD` —
DECLINED.md's picker-layer row), which is exactly why "verify before trusting
a correction" stayed the rule for this pass too.

| | tests | resolved? |
|---|---:|---|
| CLOUD | 1,130 | outside the definition |
| DECLINED | 417 | yes |
| DIVERGENT (BYOP) | 65 | yes |
| PORTABLE | 59 | not yet — fixtures/APIs exist, no fork test |
| COVERED-ELSEWHERE | 58 | yes, cited by fork test name |
| PORTED | 41 | yes |
| PORTABLE-OUT-OF-AREA | 15 | yes |
| **MISSING-SUBSYSTEM** | **50** | no — genuinely open |
| UNPARSED | 5 | no |
| DEFECT-FIXED | 2 | yes |
| MIXED | 1 | — |

Non-cloud universe: **713** of 1,843 (1,130 are CLOUD). **658 of 713 resolved
— 92%.** Of the 195 re-adjudicated: 145 changed verdict (39 → CLOUD, 12 →
DECLINED, 4 → DIVERGENT, 59 → PORTABLE, 58 → COVERED-ELSEWHERE, 8 → PORTED —
2 already existed under an identical test name and were simply mislabeled
(`abort_config_parse_cancels_and_removes_inflight_task`,
`manual_attach_and_detach_switch_running_command_input_ownership`), 3 were
genuinely ported this pass: `claude_runtime_error_patterns_returns_slice` /
`codex_runtime_error_patterns_returns_slice` /
`gemini_runtime_error_patterns_is_empty_by_default`,
`app/src/ai/agent_sdk/driver/harness/mod_test.rs`), 50 stayed
`MISSING-SUBSYSTEM` with refreshed, re-verified evidence. Full per-test detail
is in `docs/sweep-verdict-ledger.tsv`.

**The 50 that are still genuinely open**, by file (see the ledger for exact
test names and cited symbols):

> **These were a TABLE until 2026-08-11, and that is exactly how they went
> unassigned for a day.** `script/state` counts `- [ ]` checkboxes, so a table
> row is **invisible to the open-work count** — `docs/STATE.md` reported "37
> open" while these 50 sat here untracked and unassignable (nothing to mark
> `[>]`). Converted to checkboxes below. **Never record open work as a table
> row in this file.**

**STATUS 2026-08-11 06:xx — all work recovered onto branch `working`, 37 of 50
resolved, 13 open.** The six parallel agents were killed after the
shared-checkout collision (see the hazard entry above); their work was recovered
commit-by-commit onto `working` and reviewed individually. **Nothing is built** —
the freeze is in force; correctness below is review-verified, not compiler-verified.

| package | tests | status | branch commit |
|---|---:|---|---|
| `outcome-verdicts` | 8 | ✅ **done** — 1 ported (real codex/shell ordering bug), 7 re-adjudicated | `64d8eef60` |
| `outcome-agent-sdk-harness` | 10 | ✅ **done** — 5 ported, 5 re-adjudicated | `8ba03f47a` |
| `outcome-tail` | 10 | ✅ **done** — 4 ported, 4 re-adjudicated, **2 REJECTED** | `6f7e9461f` |
| `outcome-project-context` | 6 | ✅ **done** — all 6 ported (HostId dimension) | `721e4869d` |
| `outcome-warp-tui` | 3 of 5 | ⚠️ **partial** — 3 ported + 1 bonus; the 2 `orchestration_model` tests were **never reached** | `edd9b31b6` |
| `outcome-agent-events` | 0 of 11 | ⚠️ **WIP, incomplete** — agent killed mid-flight, work rescued but unfinished | `9b8307ec4` |

**Two review decisions worth knowing about:**

1. **Rejected the `shared_session/network/heartbeat.rs` port (2 tests).** Nothing
   in the fork consumes `Heartbeat`, and **both pin tests carry
   `#[ignore = "Flakes in CI"]`** so they can never run. `SCOPE-TERMINAL.md:154`
   had already adjudicated it: *"its only consumer is the dropped session-sharing
   websocket layer."* 202 lines of unreachable code guarded by tests that never
   execute is the shape `check_stub_coverage` exists to prevent. **Needs a
   `DECLINED.md` row.**
2. **`check_cloud_boundary` failed on the rescued agent_events work** and was
   fixed at the root rather than allowlisted — `app/src/server/retry_strategies.rs`
   has zero cloud dependencies and zero importers, so it was moved to
   `app/src/util/`. Allowlisting would have recorded a dependency that does not
   exist. See `5f7c96883`.

**Still open (13 tests):** `agent_events/driver_tests.rs` (11) and
`orchestration_model_tests.rs` (2).

**`orchestration_model_tests.rs` (2) — verdict RE-VERIFIED 2026-08-11, unchanged
at MISSING-SUBSYSTEM, and it is REAL WANTED DEBT, not cloud.** Adjudicated by
the coordinator rather than an agent. Chain of evidence:
- `failed_launch_cleanup_preserves_other_sessions` and
  `local_harness_children_fail_cleanly` need `StartAgentExecutor` /
  `StartAgentRequest`, which live in the pin's
  `app/src/ai/blocklist/action_model/execute/start_agent.rs`. **That file does
  not exist in this fork** (verified `git cat-file` against both trees), nor
  does its sibling `run_agents.rs`.
- `orchestration_model.rs`'s own doc comment (`:1-23`) says exactly this and is
  **correct** — checked, not assumed. *(Both `StartAgentExecutor` matches in the
  fork are inside that doc comment. Grepping the symbol makes it look present.
  This is the third comment-mention false positive of the night, after
  `ActiveAgentViewsModel` and `create_branch_tooltip` — grep the definition,
  never the name.)*
- **Not cloud.** The pin test dispatches
  `StartAgentExecutionMode::Local { harness_type: Some("codex") }`, and
  `DECLINED.md:179` reversed the blanket orchestration decline: *"LOCAL
  orchestration is back in scope … still declined: the cloud-runner half."*
  Only #290's remote-runner path is declined; this is the local path.
- **So the correct disposition is: keep open.** Closing it as CLOUD or DECLINED
  would be wrong. Resolving it means building a TUI child-materialization path
  — the module's own words, *"future work, not a mechanical trim"* — which is a
  feature, not a test port. Size it as such before assigning it.

Three packages are **verdict-first, not port-first** — `execution_profiles`
(likely DIVERGENT), `mod_tests` `auth_check_command` (#289 deferral), and
`orchestration_model` (cut as future work per its own doc comment). Porting
those without re-adjudicating would manufacture debt from decisions.

- [>] **`app/src/ai/agent_events/driver_tests.rs` — 11 tests.** Needs
      `AgentEventDriverConfig::{auth_error_give_up_failures, max_retry_duration,
      permanent_error_backoff_steps}`, `HttpStatusError::is_actionable()`,
      `agent_event_failure_should_log_error()`. **Largest single cluster and the
      best value-per-effort in the set** — two config fields plus an
      error-classification fn, on the BYOP error path.
- [>] **`crates/ai/src/project_context/model_tests.rs` — 6 tests.**
      `path_to_rules`/`ProjectRule::path` have no `HostId` dimension.
      **A working in-tree pattern already exists** — `global_rules.rs` (#575)
      solved the identical host-keying problem and `HostId` is already imported
      at `model.rs:5`. Cleanest port in the set; start here.
- [>] **`app/src/ai/agent_sdk/driver/harness/claude_code_tests.rs` — 5 tests.**
      `--resume` flag, `MessageBridgeCleanupDisposition`, parent-bridge event
      cursor persistence.
- [>] **`crates/warp_tui/src/terminal_session_view_tests.rs` — 3 tests.**
      `InputTypeAutoDetectionSource::AgentTerminalControl` + the "attach" hint
      string (mirrors `RUNNING_COMMAND_DETACH_HINT`).
- [>] **`app/src/ai/execution_profiles/config_tests.rs` — 3 tests.**
      **RE-ADJUDICATE BEFORE PORTING.** The fork persists profiles via
      `GenericStoredObject`/`StringModel`, not a settings.toml-embeddable file
      collection. That is a deliberate architectural difference, which under
      this file's definition of done is **`DIVERGENT`, not missing**. Porting
      these would mean adopting the pin's persistence model — a product
      decision, not a test port.
- [>] **`app/src/ai/agent_sdk/driver/harness/mod_test.rs` — 4 tests.**
      `auth_check_command`/`auth_check_command_for`, **deliberately deferred
      under #289** — the fork's own test header says so. Do not port without
      reopening #289.
- [x] **`app/src/terminal/model/terminal_model_test.rs` — 2 tests. DONE, and the
      old text here was wrong in three separate ways (corrected 2026-08-20).**
      (a) The file is `terminal_model_test.rs`, singular — `terminal_model_tests.rs`
      has never existed in this tree. (b) `should_validate_dcs_hook_session_id` is
      **not** hardcoded `false`: `app/src/terminal/model/terminal_model.rs:2728-2730`
      is the pin's `!self.shared_session_status().is_viewer()`. (c) Both gate tests
      are live and un-`#[ignore]`d — `viewer_processes_dcs_hook_with_unregistered_session_id`
      (`terminal_model_test.rs:2257`) and `sharer_rejects_dcs_hook_with_unregistered_session_id`
      (`:2311`). #419 and #532 are both CLOSED; neither blocks anything.
- [>] **`crates/warp_tui/src/orchestration_model_tests.rs` — 2 tests.**
      `cleanup_failed_child`/`begin_local_oz_child_launch` — explicitly deferred
      per the module's own doc comment (`:1-23`).
- [>] **`app/src/terminal/view/ambient_agent/block/setup_command_text_tests.rs` — 2 tests.**
      `setup_command_text.rs` does not exist; nor does its sole consumer
      `AmbientAgentViewModel`.
- [>] **`app/src/terminal/shared_session/network/heartbeat_tests.rs` — 2 tests.**
      No `network/` directory under `shared_session/`; `heartbeat.rs` absent.
- [>] **`app/src/ai/conversation_details_panel_tests.rs` — 2 tests.**
      `conversation_details_panel` does not exist — **this is the same absence
      as the wasm latent break** recorded further down this file. Resolve them
      together: whichever way that decision goes settles these two.
- [>] **`app/src/pane_group/pane/local_harness_launch_tests.rs` — 1 test.**
      Shell-validation/codex-precondition ordering is reversed vs. the pin.
      Product-scope question, not a mechanical port.
- [>] **Remaining 7 — one test each**, see `docs/sweep-verdict-ledger.tsv` for
      exact names and cited symbols.

**Sizing, honestly: this is ~36 tests of subsystem-building, not test-porting.**
Only the `project_context` six and one `remote_search` test are ports in the
ordinary sense. Seven more (`#289` four, `execution_profiles` three) should be
re-labelled rather than ported. Calling the whole set "porting tests" makes it
read about five times smaller than it is.

None of these need a new blocking symbol resolved first (unlike the 22-symbol
list above) — each is either a small, well-scoped addition or a genuine
product-scope question (the ordering reversal; the DCS-hook role gate is no
longer one of them — it landed, see the corrected entry above).

## LSP TRACK — **[x] document lifecycle BUILT AND TEST-GREEN, NEVER RUN** (verdict 2026-08-10: RESTORE)

**Corrected 2026-08-11.** This header read "CODE-COMPLETE, UNBUILT" until
`lsp/document-lifecycle` was confirmed merged into `main` (it is an ancestor of
`HEAD`, and all four marker symbols — `open_or_sync_document_with_lsp`,
`handle_lsp_manager_events`, `notify_lsp_of_content_change`,
`latest_buffer_version` — are present on `HEAD`). The 2026-08-10
`cargo check --workspace --all-targets --features warp/gui` therefore compiled
it, and it is inside the 6,846-test green run. **Twelfth in-tree document found
contradicting the code.**

**What remains true, and is the whole point of this track:** it has never been
*exercised*. LSP is the canonical "looks finished, silently dead" subsystem —
see the description below — and a green suite is exactly the evidence that
failed to catch the last one. Discharged only by launching the app, opening a
file in a language with a server installed, and seeing a diagnostic.

**Status: the functional gap is closed in code and has never been run.**
Everything above the document lifecycle was already merged and wired — the crate,
the driver (install/spawn/detect), the shutdown scan, the real `footer.rs`,
`try_connect_lsp_server` on buffer load, `format_and_save` on save, hover,
goto-definition, find-references, the context menu, vim `gd`/`gr`/`K`,
diagnostics *rendering*, and log routing to a terminal.

Nothing ever sent `didOpen` or `didChange`: the server started, the editor
connected, `refresh_diagnostics` ran, and the server held no document so it
published nothing. Hover / goto-def / find-refs returned empty for the same
reason — position queries against a document the server was never given. That is
the "looks finished, silently dead" failure mode, and it is what step 5 closes.
**Until someone builds and runs it, "functional" is a code claim, not a
measurement.**

- [x] **Step 5 — `global_buffer_model.rs`, hand-integrated** (branch
      `lsp/document-lifecycle`). `initial_content_version` on
      `BufferSource::{Local,ServerLocal}` and `latest_buffer_version` on
      `InternalBufferState`; the local `ContentChanged` subscription
      reconstructed in both `create_new_buffer` and `register_buffer_for_path`;
      `lsp_server_for_path`, `log_lsp_sync_debug`,
      `open_or_sync_document_with_lsp`, `close_document_with_lsp`,
      `handle_lsp_manager_events`, `notify_lsp_of_content_change`; didClose on
      `cleanup_file_id` / `remove_deallocated_buffers` / `rename`. The
      remote-buffer layer was untouched — the two paths are genuinely additive,
      as predicted.
- [x] **Step 6a — the wasm build break.** `code_pane.rs` was ported verbatim
      from the pin including its `#[cfg(target_family = "wasm")]`
      `CodeViewEvent::OpenLspLogs` no-op arm, but `code/wasm.rs` never declared
      the variant. Declared it, matching the pin.
- [x] **Step 6b — the `code_page.rs` LSP settings subpage.** **[DONE 2026-08-10 — hand-integrated (+931), 207 lines of tests, 16 i18n keys. Subset property confirmed FALSE as predicted. Used the pin's per-workspace shape, NOT efcaa42b8's own pre-removal version, whose global `enabled_lsp_servers` model no longer has a state layer. Also restored FormatOnSaveToggleWidget — the setting came back with LSP during the build repair but its only UI control did not, leaving code.editor.format_on_save unreachable. Deliberate divergences: no 'View logs' button (footer covers it), and rows are SORTED where the pin walks a HashMap and shuffles between frames.]**
- **`BufferState` was never divergent.** Both fork and pin carry exactly
  `file_id` + `buffer`. The two extra fields live on `InternalBufferState`
  (`latest_buffer_version`) and on `BufferSource::{Local,ServerLocal}`
  (`initial_content_version`). The handover conflated the two structs.
- **Model-event closures take 3 arguments here, not the pin's 4.**
  `ModelContext::subscribe_to_model` is
  `FnMut(&mut T, &S::Event, &mut ModelContext<T>)`; only `ViewContext` takes the
  4-arg form. So `handle_lsp_manager_events` drops the pin's `ModelHandle`
  parameter. Pasting the pin's signature compiles nowhere.
- **The pin's `new()` shape would crash the remote-server daemon.** The daemon
  registers `GlobalBufferModel` (`app/src/remote_server/mod.rs`) and never calls
  `lsp::init`; `LspManagerModel::handle` panics on an unregistered singleton.
  Resolved by gating on `has_singleton_model::<LspManagerModel>()` in `new()`
  and returning `None` from `lsp_server_for_path` — one guard on the single
  resolver every LSP entry point funnels through. This also leaves
  `buffer_location_tests` and `global_buffer_model_tests` working unchanged,
  with no test relaxed. Same hazard class as the one already documented on
  `subscribe_to_remote_server_manager`.
- **`local_code_editor_wasm.rs` needs nothing.** The handover expected
  `language_server_enabled` / `add_footer` / `with_find_references_provider` to
  be missing there; the **pin's own wasm stub has none of the three**, because
  every call site is excluded on wasm (`view.rs` → `wasm.rs`,
  `language_server_shutdown_manager` → `#[cfg(feature = "local_fs")]`, which
  wasm does not enable). The fork's stub also diverges from the pin in
  fork-favouring ways; do not sync it as part of this track.

### Adjudication verdicts (evidence-based, do not re-derive)
- 8 `LocalCodeEditorView` fields LSP-caused → restored; 3 explicitly NOT
  (`has_remote_conflict`, `auto_save_debounce_tx`, `auto_save_in_flight` — real
  parity gaps, but a general pin-sync, not this track); 0 unclear.
- The `Hoverable` + `on_right_click` render wrapper: **category (a), LSP
  fallout** — restore was correct. **Method worth reusing:**
  `git log -S base_with_handler` was *useless*, because `efcaa42b8` kept the
  binding and only changed its value, so the string count never moved. Probing
  the wrapper's *contents* (`-S on_right_click`) gave the answer. Generalisable:
  `-S` on a binding NAME proves nothing when a commit rebinds rather than
  removes.
- **Source rule:** `persisted_workspace.rs` follows the **pin**;
  `local_code_editor.rs` call sites adapt to the pin's shape. The fork base has
  `LspTask::Spawn { file_path, server_type }`, the pin has `{ file_path }` —
  pasting from the `efcaa42b8` removal diff compiles nowhere.
- **Subset rule:** wholesale replacement is only safe when the fork's file is a
  strict subset of the pin's. True for `footer.rs` (verified before copying),
  **false** for `global_buffer_model.rs`.

### NEW — an untracked product decision this surfaced, needs a maintainer entry
- [x] **`lsp_server_selector.rs` is NOT an LSP-track item.** **[RECORDED IN `DECLINED.md` 2026-08-10 — the removal is documented with its inherited rationale, the correction that the selector is an InitProject leaf rather than LSP debt, and an explicit note that the "cloud onboarding" framing is recorded but NOT endorsed and deserves a §5.10 re-look. No code change: reversing it means restoring the 1,901-line wizard first, which is a separate maintainer call.]** It was not removed
      by `efcaa42b8`. `app/src/terminal/view/init_project/` was deleted five days
      earlier by **`b0b1faef9`** — a separate decision, rationale *"the
      InitProject wizard is Warp cloud agent mode's first-run onboarding; openWarp
      BYOP has no cloud onboarding need"*. The selector is a leaf of a 1,901-line
      wizard (`mod.rs` 1,303 + `model.rs` 598) that would have to be reversed
      first. **That decision is in neither `DECLINED.md` nor `TODO.md`** — a third
      deliberate removal recorded nowhere, after LSP itself and the
      PersistedWorkspace/indexing retirement. Per §5.10 the rationale also
      deserves a second look: `/init` is a **local** flow, so "cloud onboarding"
      may be the wrong frame. Needs a maintainer verdict either way.

### Done
- [x] `crates/lsp` restored verbatim, 22 tests unweakened (`f4e99118a`)
- [x] initial app wiring — deps, `lsp::init`, `FeatureFlag::LSPAsATool`,
      `lsp_logs.rs`, `lsp_telemetry.rs` catalog, 3 editor helpers
      (**"terminate hook" removed from this line 2026-08-20 — it was never done;
      see the open item immediately below**)

### Open — the shutdown hook this section claimed was Done
- [ ] **No LSP terminate on app shutdown.** `app/src/lib.rs:2360-2392` runs
      `on_will_terminate` straight from `PersistenceWriter::terminate()` to
      `PtySpawner::prepare_for_app_termination()`, with no LSP step anywhere in
      the closure. The pin has one at `42effe840:app/src/lib.rs:2692-2694`:
      `lsp::LspManagerModel::handle(ctx).update(ctx, |manager, ctx| manager.terminate(ctx))`.
      `LspManagerModel::terminate` exists (`crates/lsp/src/manager.rs:306`) and
      has **zero callers repo-wide** — the only same-named call is
      `LspServerModel::terminate` from its own `Drop` (`crates/lsp/src/model.rs:745-747`),
      a different type, which merely detaches an async shutdown and is not a
      substitute for the graceful all-workspaces teardown. Consequence: language
      servers are left to be reaped by the OS on quit rather than shut down.
- [x] `workspace_language_server` migration, re-applied onto current main (`5f2f5d103`)
- [x] PersistedWorkspace LSP **state** layer — `EnablementState`,
      `language_servers`, the seven enable/disable/query methods, `ModelEvent`
      dispatch, both sqlite functions
- [x] **the `ON DELETE CASCADE` guard arm** — both halves of the hazard now
      covered, with the reason they are not interchangeable recorded in code

### Remaining — ~2,500 lines of surgery into a diverged host, NOT started
`language_server_extension.rs`, `find_references_view.rs` and
`language_server_shutdown_manager.rs` all gate on the same blocker:
`LocalCodeEditorView` state the fork does not have. The agent stopped here
deliberately rather than shipping a large blind edit, and it was right to.

**The host is the problem, not the three files.** Pin `LocalCodeEditorView` has
~25 fields, the fork has 15 — and **the absences are NOT all LSP-caused**. The
file diverged in both directions, so every field needs individual adjudication
rather than a bulk restore. On top of that: ~20 methods / ~800 lines in
`local_code_editor.rs`, new `LocalCodeEditorAction` variants and dispatch, the
`CodeEditorEvent::MouseHovered` arm (an explicit no-op today at
`local_code_editor.rs:227`), render changes for the hover tooltip and
find-references card, `editor/element.rs` (+88 LSP lines), and `code/mod.rs`'s
`ShowFindReferencesCardProvider` trait. Then the two 600-700 line files land on
top of that.

Also absent and needed by the shutdown manager:
`TerminalView::canonical_session_pwd_if_local`. Restorable — its inputs
(`active_session_path_if_local`, `repo_metadata::CanonicalizedPath`) both exist
— but it needs a new `canonical_session_pwd_cache` field on `TerminalView`.

**Do not assign this as one unit.** Adjudicating the diverged host is its own
piece of work and should land before any of the three files are attempted.

**Status 2026-08-10:** step 1 + part of step 2 MERGED (`f4e99118a`). The original
agent was **resumed** and is continuing with `language_server_extension.rs`,
`find_references_view.rs`, the shutdown manager, the server selector, the
`code_page.rs` section, the persistence half (unblocked now D1 landed), and the
`ON DELETE CASCADE` guard arm. **This is not working LSP yet.**

Was the largest item with no home — removed deliberately by `efcaa42b8` and
recorded in neither `DECLINED.md` nor this file. Maintainer decided 2026-08-10
to restore it, so it is tracked work now, not an open question.

**What it is.** Language Server Protocol support: the standard that lets the
editor talk to per-language backends and get code intelligence back. Without it
this fork ships a code editor and file tree with **no** diagnostics,
go-to-definition, hover docs, find-references or formatting —
`git grep -l 'language_server\|lsp_types\|LSPServerType'` matches nothing but
yarn cache zips and the migration that dropped it.

**SCOPE CORRECTION 2026-08-10.** The figure below (~6,600) counts what the pin
*has*. What `efcaa42b8` *deleted* is **14,611 lines** — it took `code/footer.rs`
(1,910), `local_code_editor.rs` (1,365) and `settings_view/code_page.rs` (1,055)
with it. Restoring LSP means restoring those too, or establishing that the fork's
current editor works without them. Scope this before committing to an estimate.
**Also honour the `ON DELETE CASCADE` trap recorded in the D1 section** — the
`workspace_language_server` FK has no cascade and the startup join silently drops
orphans, so enabled servers read as disabled.

**Scope, measured at the pin (~6,600 lines).**
- `crates/lsp/` — 20 files / 4,891 lines. `service.rs` exposes `definition`,
  `hover`, `references`, `format`, `did_open`, `did_change`.
  `supported_servers.rs:40` lists 5 servers: rust-analyzer, gopls, pyright,
  tsserver, clangd. Tests: `config_tests.rs`, 22 test names, all absent here.
- `app/src/code/language_server_extension.rs` (625)
- `app/src/code/find_references_view.rs` (696)
- `app/src/code/language_server_shutdown_manager.rs` (152)
- `app/src/code/lsp_telemetry.rs` (203), `lsp_logs.rs` (33)
- `app/src/terminal/view/init_project/lsp_server_selector.rs`
- the `code_page.rs` settings section
- two persistence tables, dropped by
  `crates/persistence/migrations/2026-05-11-000000_drop_lsp_workspace_tables/up.sql`

**Known couplings — do not discover these late.**
- **`node_runtime` dependency.** pyright and tsserver are node-based; the pin
  installs and runs them through it. Confirm whether this fork still has
  `node_runtime` before assuming the install path works.
- **Persistence.** The tables were dropped by migration. Restoring needs a NEW
  forward migration — do not edit or revert the existing one.
- **Delta D1.** `PersistedWorkspace` owns per-workspace LSP enable/disable
  state. With this verdict, D1's LSP half is now real work rather than a stub.
- **Telemetry.** `lsp_telemetry.rs` targets a telemetry channel this fork
  disabled (`ChannelState::is_telemetry_available()` is hard-`false`). Port the
  call sites, not the transport.

**Sequencing.** `crates/lsp/` first (self-contained, has the only tests), then
the app-side wiring, then settings + persistence, then the D1 join. Not a
single-agent single-pass job.

## INHERITED SUBSYSTEM REMOVALS (from the Zap/OpenWarp lineage, NOT this fork)

**Provenance correction 2026-08-10.** All four removals below are authored by
`zero <1603852@qq.com>` — the upstream Zap/OpenWarp author — between 2026-04-30
and 2026-05-10. **This fork's own history starts 2026-07-18.** They are
*inherited* decisions, not undocumented decisions of this project.

That corrects how they were first written up here. `DECLINED.md` and `TODO.md`
document *this* project's calls, so they were never going to contain zero's, and
describing these as "recorded nowhere" implied a bookkeeping failure that did
not happen. The real situation is narrower and more useful: **we inherited four
large local-subsystem removals whose rationales we have not audited**, and three
of the four turned out to be worth reversing.

| commit | date | author | scale | subsystem | status |
|---|---|---|---|---|---|
| `efcaa42b8` | 05-10 | zero | −14,891 / 92 files | **LSP** | restored 2026-08-10 through the document lifecycle |
| `d84dd8e4d` | 05-10 | zero | −2,858 / 39 files | PersistedWorkspace + indexing | restored (D1 + D2) |
| `b0b1faef9` | 05-05 | zero | −2,794 / 41 files | InitProject wizard | **under review** — rationale never verified |
| `9765692e1` | 04-30 | zero | −936 / 17 files | computer-use dispatch | **being restored** |

      **[DONE 2026-08-10 — both flow through `prepare_environment_config`. Found two REAL bugs beyond scope: `--harness codex --model X` was REJECTED as an unknown Zap model, and `--harness claude --model X` silently ignored the model because `harness_model_env_vars` was never called. Both fixed. `context` not ported (would be permanently None here). Claude MCP staging NOT wired — needs `--mcp-config` + `serialize_claude_mcp_config`, a capability port; see below.]**
- [~] **Computer-use dispatch — RESTORED 2026-08-10, still not reachable.** 1,332 lines across 22 files merged; `check_cloud_boundary` green. The `DECLINED.md` contradiction is resolved (recording stays declined; targeting and dispatch are not). **But an agent still cannot drive it — two blockers OUTSIDE `9765692e1`, both found during the restore:** (see the two rows below). Originally: `crates/computer_use`
      is fully restored and green, but `create_actor()` has exactly one caller
      (the dev CLI) because the dispatch path is gone, so no agent can drive it.
      **Also resolving a live contradiction**: `execute.rs:377` says *"Computer
      Use is out of scope for this fork (see `DECLINED.md`)"* while
      `DECLINED.md:137` lists `crates/computer_use` as **not** declined and
      `:125` says **"#349 is NOT covered"**. The `DECLINED.md` rows are right;
      the code comment is wrong. Recording *is* declined (#350/#367) and stays so.
- [x] **BLOCKER 1 — `FeatureFlag::AgentModeComputerUse` was hard-coded `false`.
      DONE 2026-08-10.** It was short-circuited in `is_enabled` alongside
      `ForceLogin`, `AvatarInTabBar`, `HOARemoteControl` by `5013248be` (zero,
      2026-04-29) **one day before** the dispatch removal — same inherited
      family. The grouping was **not** principled: `5013248be` added it in the
      same `matches!` arm as the six `CloudMode*` flags, under the commit
      message "hide Cloud Oz / **Cloud Agent** Computer Use / Privacy pages", so
      it was swept in on the belief that computer use was a Warp cloud-agent
      capability. It is not (`DECLINED.md`, "common false positives"). The list
      is now the named `FORCE_DISABLED_FLAGS` const with the other three
      unchanged, and `warp_features` pins its membership so nothing can be swept
      back in silently.
      **The "it also controls settings-page visibility" note above was wrong** —
      `AgentModeComputerUse` has exactly three consumers (flag registration in
      `app/src/lib.rs`, `app/src/ai/agent/api.rs:424`, and
      `ambient_agent/model.rs:340`) and gates no UI. The computer-use settings
      surface (permission dropdown, computer-use model picker, computer-use
      prompt-override slot) keys off **`LocalComputerUse`**, which rode on
      `DOGFOOD_FLAGS` — never applied to any shipping binary here — so it was
      also enabled, via `"local_computer_use"` in `app/Cargo.toml`'s `default`
      features (symmetric with `"agent_mode_computer_use"`, which was already
      there and matches the pin). No settings page needed restoring.
      **Default stays conservative**: the flags only make the capability
      *offerable*; the per-profile `ComputerUsePermission` still defaults to
      `Never`, so the user must opt in under Settings > Agents > Profiles.
      Nothing is user-visible until BLOCKER 2 lands.
- [x] **BLOCKER 2 — RESOLVED 2026-08-10.** `tools/computer.rs` adds `request_computer_use` + `use_computer` descriptors with a schema derived from the Rust types and deliberately narrower than `convert.rs` in seven places, plus a dispatch-site refusal (a leaked call maps to a real executor here, unlike the web tools). **The tool is BLIND, and that is now the open item** — see the row below. Originally:
      `app/src/ai/agent_providers/tools/REGISTRY` lists ~20 `OpenAiTool`
      descriptors with no `use_computer` / `request_computer_use` entry, so no
      model is ever offered the tool. Relatedly `AIRequestInput::computer_use_enabled`
      is **set and never read** — at the pin it travels to Warp's server, which
      owns tool selection; BYOP builds the tool list locally instead. Closing this
      means writing JSON schemas for the full action set plus `result_to_json`.
      **Genuinely new work with no pin reference** (the pin's schema is
      server-side), not a restore. This is the larger of the two.
- [x] **InitProject — REVIEWED and DECLINED 2026-08-10.** Recorded in `DECLINED.md` with the true reason: the removal commit's 'cloud agent mode's first-run onboarding' rationale is false on both clauses (it is per-repo setup, and a full grep finds no auth/sign-in/subscription/Warp API call — ~17% cloud-coupled). The removal was nonetheless CORRECT: three commits earlier the same day had already moved its one durable local capability into the `/init` prompt template. Residue tracked separately below.
      The "cloud agent mode's first-run onboarding" rationale came from zero's
      commit message and **has been repeated through several handovers without
      anyone reading the code**. `/init` is a local flow, so the framing is
      suspect. The review will answer what it does, whether it is cloud or local,
      its relationship to `/init`, and whether to restore, partly restore, or
      formally decline it. `lsp_server_selector.rs` went with it.

- [x] **Computer use is SIGHTED — DONE 2026-08-10.** The screenshot now travels
      as a `ContentPart::Binary` on a user message appended after the tool
      results, gated on `AttachmentCaps::images`. `ToolResponse` is untouched,
      so tool_call/tool_response pairing is unchanged by construction — the
      proposed route below was taken, with one correction: on **Anthropic** a
      standalone user message straight after a tool-result turn is two user
      turns in a row, which it rejects, so the parts are folded into the
      trailing `ChatRole::Tool` message and the vendored Anthropic adapter emits
      them after that turn's `tool_result` blocks (a match-arm change; no type
      changed; recorded in `lib/rust-genai/CHANGES-PHOSPHOR.md`). Widening
      `ToolResponse` was rejected — ~20 adapters would each silently drop the
      image until taught the new field, and OpenAI's `role: "tool"` message is
      text-only regardless, so a second route would still have been needed.
      Bounds: the two most recent captures only, re-encoded to ≤1568 px on the
      long edge and ≤3.5 MB of PNG; nothing goes through
      `cap_tool_response_content`. Degradation is explicit — the result carries
      `screenshot.delivery` ∈ {`attached_to_following_user_message`,
      `model_cannot_see_images`, `superseded_by_newer_screenshot`,
      `undeliverable`} with matching prose, and the tool descriptions swap to
      image-capable wording only when caps allow. Replay is a pure function of
      the message list, so a restored conversation cannot double-inject.
      **Two adjacent defects fixed in passing:**
      `serialize_outgoing_tool_call` had no computer-use arm, so every replayed
      call was renamed `warp_internal_UseComputer` with `{}` args — the model
      could not tell which action produced the screen it was shown; and the
      per-turn `[byop-diag] full_request_json` log plus the wire inspector
      dumped binary parts verbatim, a megabyte of base64 per turn once
      screenshots existed. Original text:
      A BYOP
      tool result is delivered as `genai::chat::ToolResponse { content: String }`
      — a plain string with no parts — and `cap_tool_response_content` truncates
      at 40,000 chars, two orders of magnitude under a base64 PNG (truncation
      would yield a corrupt data URI, not a degraded image). `AttachmentCaps`
      governs only *user-message* attachments and is unreachable from
      `result_to_json`. So results carry metadata only, with
      `screenshot.captured: true, attached: false` and a note saying why —
      deliberately not `{}`, which would let the model infer a blank screen. The
      user still sees the screenshot in the block render.
      **Fix proposed, not built:** after a `use_computer` result, inject a
      follow-up `ChatMessage::user` carrying `ContentPart::Binary` gated on
      `AttachmentCaps::images`, since genai's `ToolResponse` has nowhere to put
      one. That is a change to `chat_stream`'s message assembly and to
      tool_call/tool_response pairing validation.
- [x] **DONE 2026-08-18 — `script/check_large_deletions` landed. UNVERIFIED (not compiled;
      the script itself WAS run).** Two triggers, because a single removed-lines test is
      the cry-wolf shape: **T1** >=750 removed AND >2x more removed than added (rename
      detection on); **T2** >=1500 lines removed by files deleted outright, regardless of
      what the change adds elsewhere. Thresholds measured over all 2,295 non-merge commits
      — removed-lines p50=5, p90=153, p95=420, p98=1797. 750 is the largest round value
      still catching the smallest 2026-08-10 surprise removal (`9765692e1`, +25/-936).
      1500 is set against `44f71a6cc`, the one commit that deletes 2,988 lines while
      ADDING 12,776 — the bundling shape T1 structurally cannot see (net says it *grew*).
      **Firing rate 3/1,256 (0.24%) in August; all four 2026-08-10 removals fire; no false
      positives found.** Escape hatch is one line: cite `DECLINED.md` or add a
      `Large-deletion: <reason>` trailer — a bare `#123` is deliberately NOT accepted,
      because every squashed PR subject here already ends in `(#NN)`, which would have made
      the guard decorative from day one. CI: two steps in the `guards` job, with a
      base-commit fetch and `--require-base` so an unfetched base is an error, not the
      silent skip the oracle fetch spent weeks in. Also wired into `script/precheck`.
      Verified not firing on the current tree (+7463/-617, exit 0). Original entry:
      **KEEP-SIZED 2026-08-17 (audit-debt triage) — premise true, no such guard
      exists, and the size is one script + one CI step.** Verified: `script/` holds
      **12** `check_*` guards (`check_brand_strings`, `check_channel_command_names`,
      `check_cloud_boundary`, `check_dangling_modules`, `check_declined_collisions`,
      `check_generator_wrapper_names`, `check_integration_test_registry`,
      `check_license_config_sync`, `check_settings_registry`, `check_stub_coverage`,
      `check_sweep_ledger`, `check_test_failures`) and **none** of them looks at
      deletion size. Four of them already reference `DECLINED.md`, so the
      "does this PR cite a `DECLINED.md` row or an issue" half has prior art to
      copy from (`script/check_declined_collisions` is the closest model).

      **Size:** one new `script/check_large_deletions` in the house style, one step
      in `.github/workflows/pr-check.yml`, and a threshold + allowlist file
      (`script/cloud_boundary_allowlist.txt` is the precedent). Comparable guards
      in this tree run in seconds and are 80-150 lines. **By the entry's own
      admission it catches zero existing debt** — it is purely preventive, so it
      has no bearing on a v0.1.0 tag. Post-release; do it when the next
      large-deletion surprise makes the case, or as cheap insurance in a quiet
      round.
      Original finding:
      **Still worth a guard, but scoped honestly.** A CI check flagging large
      non-cloud deletions without a `DECLINED.md` row or issue would not have
      caught any of the four — they predate this fork. It would prevent *future*
      ones, and it is cheap. Lower priority than first framed.



Four deliberate removals of **local** subsystems surfaced on 2026-08-10, every
one found by an agent doing unrelated work, and **every one recorded in neither
`DECLINED.md` nor `TODO.md`**. The audit did not catch them because it keys on
pin tests, and these carry few or none.

- [x] **SUPERSEDED — see the in-flight entry above.** `9765692e1` (2026-04-30) — client-side computer-use dispatch, 17 files,
      −936 lines. VERIFIED, and it carries an active documentation
      contradiction.** Removed both executors
      (`execute/{use_computer,request_computer_use}.rs`), the `crates/ai` action
      and action_result variants (`UseComputer`, `RequestComputerUse`,
      `UseComputerResult`, `RequestComputerUseResult`, `ScreenDimensions`), their
      protobuf conversions, the `block.rs` ViewScreenshot lightbox, the render
      and persistence paths, and gutted `conversation.use_computer_action_ids()`
      to `std::iter::empty()`. Inbound `Tool::UseComputer` now returns
      `UnexpectedTool`.
      **Not cloud** — the executors call `computer_use::create_actor()` locally
      and the pin's versions run entirely client-side.
      **Consequence:** #349's port is complete and the feature still cannot work.
      `create_actor()` has exactly one caller, the `use_computer` dev CLI.
      **The contradiction:** `app/src/ai/blocklist/action_model/execute.rs:377`
      says *"Computer Use is out of scope for this fork (see `DECLINED.md`)"* —
      but `DECLINED.md:137` lists `crates/computer_use` under **"Not declined —
      common false positives"**, and `DECLINED.md:125` states outright
      **"#349 is NOT covered"**. The code cites a decision the decision file
      explicitly contradicts. **Maintainer ruling needed:** either record the
      dispatch removal as declined and fix `DECLINED.md`, or file it as debt and
      fix the comment. It cannot stay as-is.
- [x] **SUPERSEDED — under review, see the in-flight entry above.** `b0b1faef9` — InitProject wizard, 1,901 lines. Rationale given was
      "cloud agent mode's first-run onboarding", but `/init` is a **local** flow,
      so per §5.10 the framing deserves a second look. Takes
      `lsp_server_selector.rs` with it.
- [x] **`efcaa42b8` — LSP, 14,611 lines. RESTORED 2026-08-10** through the document lifecycle; builds and passes. (maintainer verdict
      2026-08-10), but the removal itself was never recorded.
- [x] **`d84dd8e4d` — PersistedWorkspace + codebase indexing. RESTORED 2026-08-10** (D1 + D2, both merged and green). D1 restored the
      workspace half; D2 is restoring indexing.

- [x] **SUPERSEDED by the scoped guard entry above** (it would not have caught these — they predate the fork). Original: Four in one day is not four oversights. Nothing in
      this project forces a removal to be recorded, and the parity audit cannot
      see them (no pin tests). Proposal: a CI guard in the spirit of
      `check_cloud_boundary` that flags a commit deleting more than N lines of
      non-cloud source unless it cites a `DECLINED.md` row or a `TODO.md` issue.
      Cheaper than any of the four restorations it would have prevented.

## LICENCE COMPLIANCE 2026-08-10 — CLOSED 2026-08-10

Read-only review against pin `02b53fcd8`. Reviewer is not a lawyer; these are
located concerns with evidence, not a legal opinion.

**Headline: the MIT question, asked about Warp, is a PASS. The failure is
against Alacritty under Apache-2.0.** `LICENSE-MIT` is byte-identical to the
pin, and upstream Warp uses no per-file copyright or SPDX headers at all
(`git grep -l 'SPDX-License-Identifier' 02b53fcd8` → 0), so nothing could have
been stripped from Warp's own code. AGPL is substantially compliant: correctly
declared `AGPL-3.0-only`, public repo, all 65 workspace members inherit it, and
the single AGPL dependency (`warp_multi_agent_api`) is compatible. No GPL-3.0 /
LGPL / BUSL / SSPL / Elastic / Commons Clause / CC-BY-NC anywhere in the graph.

- [x] **BLOCKING — restore Alacritty's Apache-2.0 attribution.** **[DONE b5fea7a86 — 18 files, not 16.]** The licence
      file `crates/warp_terminal/src/model/LICENSE-ALACRITTY` exists upstream and
      is absent from this repo *and its entire history* (stripped in the
      Zap/OpenWarp ancestor, before our history begins). The 2-line attribution
      header is gone from **16 shipping source files**; for
      `crates/warp_terminal/src/model/mode.rs` the header removal is the ONLY
      difference from the pin. Both bundling scripts had the entry deleted
      (`script/prepare_bundled_resources:107-114`, `script/windows/prepare_bundled_resources.ps1:147-154`),
      so the `THIRD_PARTY_LICENSES.txt` in every shipped release never mentions
      Alacritty. Apache-2.0 §4(a) (licence copy), §4(b) (change notices) and
      §4(c) (retain attribution) are all live and all unmet, in distributed
      artifacts. Our own `docs/DESIGN-PHOSPHOR-FORK.md:127` states the rule the
      code breaks. Mechanical fix: restore the licence file, restore 16 headers,
      re-add 2 manifest entries.
- [x] **AGPL §13 — no source offer in the shipped product.** **[DONE b5fea7a86 — README + About page. Third-party-licences VIEWER still outstanding.]** We ship a daemon
      users interact with over a network (`app/src/remote_server/`,
      `crates/remote_server/`, reached over SSH) and neither it nor the About
      page offers Corresponding Source. `README.md` has **zero** hits for
      "licen"/"AGPL"/"MIT" across 148 lines — upstream's `## Licensing` section
      (`02b53fcd8:README.md:54-58`) was dropped. About page shows only
      `Copyright 2026 Phosphor`. One fix discharges both this and the
      MIT-notice-communication problem: restore the README licensing section and
      add a source URL + third-party-licence link to the About page.
- [x] **Licence CI was dropped; the allowlists enforce nothing.** **[DONE b5fea7a86 — licenses job added; has never run, expect first-run surprises.]** `deny.toml:18`
      and `about.toml:3` both claim "CI enforces this via
      `script/check_license_config_sync`" — that script is referenced nowhere in
      `.github/` or `script/precheck`. Upstream ran `cargo deny -L error check
      licenses` AND the sync check (`02b53fcd8:.github/workflows/ci.yml:665-671`).
      Nothing now stops a GPL/BUSL/SSPL/unknown crate entering on a dep bump.
      This is why the next two items exist. Needs a cargo invocation → belongs in
      CI, not `precheck`.
- [x] **`libgit2` vendored statically, GPL-2.0 notice not emitted.** **[DONE b5fea7a86 — LICENSE-LIBGIT2 committed. The deny.toml exception was correctly REFUSED; see the merge note.]**
      `app/Cargo.toml:273-275` uses `vendored-libgit2`. Not a conflict — the
      linking exception resolves compatibility with AGPL — but `cargo about`
      reads `libgit2-sys`'s declared MIT and never emits the GPL-2.0 text that
      governs the bundled C source.
- [x] **`winit` — RESOLVED 2026-08-10, option 1 taken.** `jwp2987/winit` is now a
      fork of `warpdotdev/winit` (the same repo upstream Warp pins) carrying
      exactly one extra commit: `9a0788c3`, cherry-picked from
      `rust-windowing/winit#4453` — *"fix(windows): use registry value to detect
      dark mode"*, open and unreviewed upstream since 2025-12-27.
      `Cargo.toml:412` repointed; `deny.toml`'s `allow-git` now names a repo this
      project controls, and the stale personal-fork entry and its comment are
      gone. `check_license_config_sync` passes.
      **Why not the alternatives:** dropping the fix ships a known Windows
      dark-mode bug with no upstream fix to inherit (we do ship Windows builds);
      waiting on #4453 is not a strategy after 7+ months of silence. The
      availability risk is gone — the source is now an account we own.
      - [x] **Follow-up, cheap and permanent:** **[CLOSED 2026-08-10 — tracked
            externally; nothing actionable in this repo. Not a compliance item:
            the fork is correctly attributed. Landing it would only let us delete
            the fork and drop the maintenance.]** help review or land
            `rust-windowing/winit#4453`. If it merges, this fork can be deleted
            and `Cargo.toml` can point at `warpdotdev/winit` or crates.io again.

- [x] **[CLOSED 2026-08-10. The genai half was STALE — the eighth entry in this ledger
      found stating the opposite of the tree.]** The claim that genai's attribution
      "never reaches the generated notice" is **false**: `script/prepare_bundled_resources`
      appends it by hand at line 122 —
      `"genai (rust-genai)|MIT OR Apache-2.0|lib/rust-genai/LICENSE-MIT"` — under a comment
      giving the reason ("Vendored path dependency: skipped by about.toml/deny.toml as a
      path dep, so `cargo about` never reaches it"). The same `ADDITIONAL_LICENSES`
      mechanism carries libgit2, winit, Chromium and five others. Nothing to fix.

      Recorded rather than acted on: genai is dual `MIT OR Apache-2.0` and we ship the MIT
      text, which satisfies MIT's notice requirement — while
      `lib/rust-genai/src/adapter/adapters/anthropic/adapter_impl.rs` carries a header
      citing *Apache-2.0 §4(b)* for our modifications. Harmless under either licence, and
      `CHANGES-PHOSPHOR.md` documents the changes, but the two should agree if anyone tidies
      it.

      The other half stands as **accepted state, not debt**: `warpui`/`warpui_core` declare
      MIT while depending on AGPL `markdown_parser`/`sum_tree` — inherited from upstream and
      verified identical at the pin, so it is Warp's characterisation, not one this fork
      introduced.

**Reviewer could not determine (8 items) — do not read the above as exhaustive:**
provenance of `password.ttf`; identity/licence of the ~359-icon set (naming
suggests Untitled UI, no marker in any file); per-icon SVG Repo licences;
whether the ~29 vendor logos were redistributed with permission; whether the
generated `THIRD_PARTY_LICENSES.txt` is correct in practice (could not run
cargo, so all claims about its output are derived from config, not observed);
`warpdotdev/jemallocator` + `warpdotdev/rmcp` (GitHub reports NOASSERTION);
xdotool's licence for the ported logic; and whether 2 further upstream files
carrying the Alacritty header were deleted or renamed — if renamed, the
stripped-header count rises from 16 to 18.

## UNWIRED-CODE AUDIT 2026-08-10 — code that exists but nothing reaches

Agent audit for the recurring failure mode in this fork: **code that is present,
compiles, is often unit-tested, but is never reached at runtime.** A user cannot
tell "feature missing" from "feature present but not connected" — both look like
nothing happens.

**No code changes were taken from this audit.** The maintainer asked for an audit;
findings only. Fixes the agent had staged were reverted off `main` and sit on
`worktree-agent-a14fb158cda58e369` if any are wanted later.

**FOLLOW-UP 2026-08-10, branch `fix/unwired-audit-2026-08-10`:** #2, #4, #6, #7,
#8, #9, #11, #13 and both documentation corrections are now fixed and ticked
below. #3 is owned by the Drive-removal agent. Every fixed finding was
re-verified against the tree first; none of them had gone stale.
**Stale as of this note (2026-08-10, later same day): #1, #10 and #12 were also
fixed and ticked below by other agents/commits without this summary line being
updated** — #5 is now DONE too (see below), so every finding in this section is
resolved except #3, which stays with the Drive-removal agent.

Calibration for anyone continuing: two live examples found the same day were the
Claude harness accepting `resolved_mcp_servers` and dropping it (fixed,
`28d21e520`), and `global_skills::filter_skills_by_spec` being exported and
tested with zero production callers. **The `filter_skills_by_spec` example
was resolved 2026-08-10 (#487) as delete, not wire** — its only pinned caller
is cloud Team-policy delivery this fork never grew, and its "global skills"
job is already covered by `SkillManager`'s own directory-scan path. See the
"AI global skills" correction below for the full trace. Kept here as
calibration: it is proof the "tested-but-uncalled ⇒ forgot to wire" prior
is not universal — check the caller chain each time.

### Confirmed unwired — ranked by user impact

- [x] **#1 The codebase embedding index is built, maintained, and never queried.**
      **DONE** — wired up; the index now answers the agent's `get_relevant_files`
      tool. `app/src/ai/codebase_retrieval.rs` is the pin's
      `GetRelevantFilesController` lifecycle re-homed (one model, one
      `CodebaseIndexManagerEvent` subscription, in-flight `RetrievalID`s, abort on
      supersede), keyed by repository instead of `AIAgentActionId` because this fork
      has no `SearchCodebase` action, and answering over a `oneshot` instead of an
      event because the `chat_stream` interceptor has no `AppContext`. A
      `ModelSpawner` created once per controller is what lets the async interceptor
      drive a `&mut ModelContext` call.
      **The two settings are NOT the same and now do different, documented things:**
      `code.indexing.agent_mode_codebase_context` (settings) gates the embedding
      index — it pays for it, and now it is read; `AIExecutionProfile::
      codebase_context_enabled` (per profile) gates the outline mechanism and
      `search_codebase`. `get_relevant_files` is answered by either, so it is
      advertised when either is on — see the single predicate
      `get_relevant_files_runtime::relevant_files_tool_available`.
      Two defects the wiring exposed, both fixed here: reranking was **entirely
      discarded** (`process_fragments` collapsed the reranked `Vec<Fragment>` into a
      `HashSet`, and `rerank_fragments` reorders without truncating, so all of
      `c7b8d779d`'s RRF/cross-encoder work changed nothing observable) — a
      `ranked_paths` field now carries the order; and `retrieval_requests` only ever
      dropped entries in `abort_retrieval_request`, so every *completed* retrieval
      leaked an `AbortHandle` forever. Both are inherited from the pin.
      Original finding:
      `CodebaseIndexManager::retrieve_relevant_files`
      (`crates/ai/src/index/full_source_code_embedding/manager.rs:1224`) is the only
      retrieval API, and a tree-wide grep returns exactly three lines: the
      definition, its one-line delegation, and the inner `codebase_index.rs:1435`
      definition. **Zero callers, including tests.** Same for
      `abort_retrieval_request`. The pin's caller is
      `02b53fcd8:app/src/ai/get_relevant_files/controller.rs:248` — a directory
      that does not exist here, retired with the inherited outline removal.
      **User impact: real money.** Enabling Settings > Code > codebase context
      (`code.indexing.agent_mode_codebase_context`, described as *"Whether codebase
      context is provided to the AI agent"*) embeds the whole repo against the
      user's own `/embeddings` endpoint — paid calls plus DB growth — and **no
      agent answer is ever influenced by it.** The agent's `search_codebase` /
      `get_relevant_files` tools are a *different* mechanism over `RepoOutlines`,
      gated on the same-named but separate per-profile
      `AIExecutionProfile::codebase_context_enabled`.
      **This also means all D2d retrieval work is unreachable** — the ball-tree
      descent, RRF reranking and MRR 1.0 from `c7b8d779d` optimize a path nothing
      calls. Know that before spending more there.
      Plan: build the missing consumer. Smallest useful shape is
      `RequestParams::new` (`app/src/ai/agent/api.rs:408`, which already collects
      the outline snapshot and holds an `AppContext`) also calling
      `retrieve_relevant_files` for the active repo root, carrying results on
      `RelevantFilesSnapshot` so `get_relevant_files_runtime` ranks over both.
      Risk: retrieval is retrieval-id/abort-lifecycle shaped at the pin, not a
      synchronous call. **Needs a maintainer call: wire it, or gate the two
      settings off until it is wired.** Shipping a paid no-op is the worst option.

- [x] **#2 Remote (SSH) buffer conflicts are invisible in both directions.**
      **[FIXED 2026-08-10, both halves. Daemon: the `BufferUpdatedFromFileEvent`
      arm now sends `BufferConflictDetected`. Client: `has_remote_conflict` on
      `LocalCodeEditorView` + `reopen_remote_buffer`, which needed a bigger chain
      than the audit's "~40 lines" — a new `force_reload` field on the
      `OpenBuffer` proto, `RemoteServerClient::open_buffer(path, force_reload)`,
      a daemon force-reload branch, `GlobalBufferModel::force_reload_server_local`,
      and `ServerBufferTracker::pending_connections_for_open_buffer` so the
      re-opening connection is excluded from its own `BufferUpdatedPush`.]**
      Daemon half: the `GlobalBufferModelEvent::BufferUpdatedFromFileEvent` arm in
      `app/src/remote_server/server_model.rs` is a no-op stub
      (`/* Not relevant for server-local buffers. */`), so the daemon never sends
      `BufferConflictDetected` — despite a fully wired five-layer client receive
      path (`crates/remote_server/src/client/mod.rs:934` → `manager.rs:1633` →
      `global_buffer_model.rs:407` → `:1867`). The pin pushes from exactly this arm
      (`02b53fcd8:server_model.rs:667`).
      Client half: `RemoteBufferConflict` has three live emitters
      (`global_buffer_model.rs:1777` edit-flush failure on a dead SSH connection,
      `:1899` daemon push, `:1992` version divergence) and its only subscriber,
      `app/src/code/local_code_editor.rs:1679`, **discards it** — commented *"the
      local editor view doesn't care about them."* The doc at
      `global_buffer_model.rs:1864` claims *"so the UI shows the conflict
      resolution banner"*; no UI does.
      User impact: a remote file changed on the host keeps showing stale content
      and **the next save silently overwrites it.**
      Plan (~40 lines): the banner already exists (`local_code_editor.rs:2246`).
      Add `has_remote_conflict: bool`, set on the event, clear on
      `BufferLoaded`/`FileSaved`, return it early from `has_version_conflicts` for
      `BufferFileLocation::Remote` (pin `:1724`). **Must ship together with**
      `reopen_remote_buffer` (`02b53fcd8:global_buffer_model.rs:2201`, absent
      here) — remote files have no `file_path()`, so without it the banner's
      Discard button is dead.

- [x] **#3 Settings > "Phosphor Drive" page — RESOLVED 2026-08-10: KEEP DRIVE,
      restore the missing sidebar row.** The decision reversed twice; this is the
      final one, with the reasoning, so it is not re-litigated.

      First call was "remove it" — Drive was believed to be the dropped cloud
      product. It is not. The toggle's own text says *"a local workspace in your
      terminal ... on this device"*, and `is_anonymous_or_logged_out()` is
      hard-coded `false` here, so the feature is purely local.

      **Why it was remembered as cloud, which is the useful part:** in real Warp
      it *was*. Drive objects synced to Warp's servers through
      `app/src/server/sync_queue.rs` + `cloud_objects`. That sync layer was
      **physically deleted before this fork existed** — commit `834909cb9`
      (2026-05-12, `zero`, "Wave4-2 physically delete sync_queue.rs"), part of the
      inherited Zap/OpenWarp lineage. What survived is the local browser over the
      local object store, wearing the name of a feature that used to sync. Drive
      *Spaces* and `warp.dev/drive/...` links remain genuinely cloud
      (`DECLINED.md` #267) — that distinction is the whole story.

      The removal (`6041ffe7c`) was reverted whole. What remains open is the
      original finding: the page is constructed and pushed into `settings_pages`
      but absent from `nav_items`, so it is unreachable — **add the row.**
      Original finding: The audit's recommendation was to restore the missing sidebar
      row; the maintainer's call is the opposite — **Drive should have been removed
      with the rest of the dropped surface, so the settings page and the feature go,
      rather than becoming reachable.** Do not add the `nav_items` row. Remove the
      page, the `warp_drive.enabled` setting, and the local surfaces it gates
      (Drive toolbelt tab, command-search zero state, block toolbelt save actions,
      `local_control` surface routing), and record the decision in `DECLINED.md`.
      The original finding is kept below as the evidence of what has to come out.
      Original finding:
      `WarpDriveSettingsPageView` is constructed
      (`app/src/settings_view/mod.rs:1237`) and pushed into `settings_pages`
      (`:1302`), but `SettingsSection::ZapDrive` is absent from `nav_items`
      (`:1322-1358`) — the sole source of the sidebar (`:2376`) and arrow-key nav
      stops (`:2168`). `local_control/handlers/app_state.rs:726` refuses to open it
      deliberately. So: no row, no keyboard stop, no settings-search hit. Pin lists
      it at `02b53fcd8:mod.rs:1362`.
      **Not the dropped cloud product** — the page's one toggle is
      `warp_drive.enabled`, and `WarpDriveSettings::is_warp_drive_enabled` gates
      purely local surfaces (Drive toolbelt tab, command-search zero state, block
      toolbelt save actions, `local_control` surface routing), with
      `is_anonymous_or_logged_out()` hard-coded `false`. Renders under
      `FeatureFlag::ZapNewSettingsModes`, which **is** in `app/Cargo.toml`'s default
      features.
      User impact: a user who turns the Drive panel off cannot turn it back on
      outside `settings.toml`.

- [x] **#4 The vertical-tab unread-activity dot can never light up.**
      **[FIXED 2026-08-10. Also had to implement `WorkspaceView::notify_terminal_focus_change`,
      a second no-op stub the audit did not list: `mark_items_from_terminal_view_read`
      had zero callers too, so lighting the dot without it would have produced a
      dot that never clears.]**
      `has_unread_activity(_typed, _app) -> false`
      (`app/src/workspace/view/vertical_tabs.rs:2506`), consumed at `:2552`,
      `:3470`, `:5987`. This also leaves
      `NotificationItems::has_unread_for_terminal_view`
      (`app/src/notifications/item.rs:197`) with zero callers. Pin implements it at
      `02b53fcd8:vertical_tabs.rs:3368`.
      User impact: a background agent session that finished and filed a
      notification shows no tab indicator.

- [x] **#5 The remote codebase-index *search* leg has no caller.**
      **DONE 2026-08-10, partially — search itself is wired end to end; two
      narrower manual/opportunistic pieces are explicitly NOT.**

      The blocker named in the original finding (`active_repo_path` had zero
      callers) was real but was not the *whole* story: the daemon-side search
      RPC did not exist at all. The pin has no equivalent to port — at the pin
      the client and daemon share one vector store, so the client resolves a
      search by reading that store directly and only asks the daemon to map
      hashes back to files (`GetFragmentMetadataFromHash`). This fork's daemon
      has a private per-daemon SQLite store (see
      `app/src/remote_server/codebase_index_store.rs`'s module doc), so the
      search itself has to run on the daemon. Built, in order: proto
      `SearchRemoteCodebase`/`SearchRemoteCodebaseResponse`
      (`crates/remote_server/proto/remote_server.proto`, fields 22 and 37 —
      fork-original, not a pin renumbering); the daemon handler
      `handle_search_remote_codebase` beside `handle_get_fragment_metadata_from_hash`
      (`app/src/remote_server/server_model.rs`), which bridges
      `CodebaseIndexManager::retrieve_relevant_files`'s retrieval-id/event
      lifecycle the same way `app/src/ai/codebase_retrieval.rs` already does for
      the local leg (`pending_codebase_retrievals`, resolved from
      `CodebaseIndexManagerEvent::RetrievalRequestCompleted`/`RetrievalRequestFailed`,
      which the daemon previously discarded unconditionally);
      `HostRequestHandle::search_remote_codebase`
      (`crates/remote_server/src/manager.rs`, now `Clone` so it can be held
      alongside a resolved repo path as a client-side ticket); and
      `crate::ai::codebase_retrieval::CodebaseRetrievalHandle`, changed from a
      local-only struct to a `Local`/`Remote` enum, with a new
      `handle_for_session` entry point that branches on
      `SessionContext::is_remote()` and replaces the old `handle_for_directory`
      call in `RequestParams::new` (`app/src/ai/agent/api.rs`) — this is the
      call site that finally uses `active_repo_path`
      (`app/src/remote_server/codebase_index_model.rs:331`, previously the
      finding's named zero-caller method). `get_relevant_files_runtime.rs` did
      not need to change at all: it already took
      `Option<&CodebaseRetrievalHandle>` and treats it opaquely.

      **Distinguishing outcomes:** the client resolves only "is there a repo
      to search at all" up front (`active_repo_path` returning `None` costs
      nothing, mirroring the local leg's `handle_for_directory`); the finer
      states — no index / still syncing / index unavailable / retrieval failed
      after starting — are read back from the daemon's live
      `SearchRemoteCodebase` response every query, not pre-classified from a
      pushed status snapshot that could go stale between resolution and use.
      A new `RetrievalFailure::HostUnreachable` variant (distinct status token
      `"host_unreachable"`) covers the one state the local leg cannot have: no
      live connection / transport failure. The daemon's
      `RemoteCodebaseSearchErrorCode` and the client's `RetrievalFailure`
      mirror each other one-to-one (`NotEnabled`/`IndexFailed` →
      `IndexUnavailable`, `IndexNotFound` → `NoIndex`, `IndexSyncing` →
      `Syncing`, `RetrievalFailed`/`InvalidRepoPath`/`Unspecified` → `Failed`),
      so a "genuinely no matches" result (empty `ranked_paths`, not an error at
      all) reaches `get_relevant_files_runtime`'s existing `"no_matches"`
      status the same way the local leg's does.

      **Explicitly NOT done, and why:** `request_active_repo_index`
      (`codebase_index_model.rs:341`) and
      `RemoteServerManager::trigger_codebase_incremental_sync`
      (`crates/remote_server/src/manager.rs`) still have zero production
      callers, and `CodebaseResyncMode::Incremental` is still daemon-handled
      for a message no client sends. These are a manual "index this now"
      action and an opportunistic "resync because the index looked stale"
      trigger, respectively — genuinely separate from answering
      `get_relevant_files`, and `RemoteCodebaseSearchContext::is_stale`
      (`codebase_index_model.rs:1068`) is still unread for the same reason:
      wiring "stale → trigger a background resync" needs a
      `ModelContext`-bearing call site to mutate `RemoteServerManager`, and
      `handle_for_remote_session` only has a read-only `&AppContext`. A
      reasonable follow-up, not required for search to work correctly today.

- [x] **#6 Right-clicking the vertical-tabs panel does nothing.**
      **[FIXED 2026-08-10 with `.with_defer_events_to_children()` and a dedicated
      `panel_right_click_mouse_state` shared with nothing — the documented prior
      bug was two `Hoverable` trees sharing ONE handle, not the wrapping itself.
      `.on_click` -> `CancelActiveRename` was NOT ported: that action does not
      exist in this fork. `.on_double_click` -> `AddDefaultTab` was.]**
      `WorkspaceAction::OpenNewSessionMenu` exists
      (`app/src/workspace/action.rs:234`) and is handled
      (`app/src/workspace/view.rs:20130`) with **zero dispatchers** anywhere in
      `app/src` or `crates/`, tests included. The pin dispatches it from a
      `Hoverable` wrapping the whole panel
      (`02b53fcd8:vertical_tabs.rs:1725`); the fork replaced that with a bare
      `Container` (`vertical_tabs.rs:1564`), also losing `.on_click` →
      `CancelActiveRename` and `.on_double_click` → `AddDefaultTab`.
      Plan (~20 lines, two spots): add a `MouseStateHandle` field to the panel state
      struct (`vertical_tabs.rs:572`, init at `:607` — the pin's
      `panel_right_click_mouse_state` does not exist here), wrap `inner` in
      `Hoverable` with the three handlers plus `.with_defer_events_to_children()`,
      dispatching the fork's `OpenNewSessionMenu { position }` shape (the pin's
      `NewSessionMenuAnchor` does not exist here). **Deliberately not shipped:** it
      puts a new `Hoverable` around the panel's entire event tree, and the comment
      three lines above documents a prior bug from exactly that pattern.

- [x] **#7 Password-prompt polling arms on warpify-compatible subshells.**
      **[FIXED 2026-08-10. Ported both pin helpers; the "missing `&AppContext`
      shell-family accessor" was solved by adding `shell_family_from_app` and
      making the existing `shell_family` delegate to it. NB the audit's examples
      were partly wrong: `python` and a bare `docker run` do NOT match
      `is_compatible_subshell_command`; `bash`, `/bin/zsh`,
      `docker run … bash` and `aws-vault exec` do.]**
      `should_start_password_prompt_polling(&self, _command: &str, ctx)`
      (`app/src/terminal/view.rs:24256`) drops the command. The pin
      (`02b53fcd8:view.rs:25854`) opens with a
      `would_emit_block_started_for_password_prompt_polling` gate (`:25658`)
      returning `false` for anything matching `is_compatible_subshell_command`.
      User impact: `ssh`, `python`, `docker run` etc. arm the poller the pin
      suppresses, so navigating away can produce a spurious "needs attention"
      notification. Plan (~35 lines): port both pin helpers; the fork already has
      `is_compatible_subshell_command`, `session.alias_value`,
      `command_first_word_and_suffix`, and the identical alias-expansion block at
      `view.rs:10643`. Missing piece: an `&AppContext` shell-family accessor (the
      fork's `shell_family` takes `&mut ViewContext`).

- [x] **#8 Agent tips can render the literal string `<keybinding>`.**
      **[FIXED 2026-08-10. Only the keybinding guard was portable — the pin's
      other two arms key off `AgentTipKind::CodebaseContext` / `::Handoff`, and
      neither variant exists in this fork.]**
      `is_tip_applicable(&self, _cwd, _app) -> true`
      (`app/src/ai/agent_tips.rs:384`). `to_formatted_text` substitutes only when
      `keystroke()` resolves; seven `agent-tip-*` strings in
      `app/i18n/en/warp.ftl` contain the token, and one keys off
      `voice_input_toggle_key`, which can be unset. The pin has a non-cloud guard.

- [x] **#9 Vim goto-line is a no-op in the terminal command input.**
      **[FIXED 2026-08-10 with the pin's body verbatim. The audit's warning that
      `reset_selections_to_point` "is on `EditorView`, not `EditorModel`" is
      WRONG — it exists on both (`editor/view/model/mod.rs:2045` and
      `editor/view/mod.rs:5910`), so no adaptation was needed.]**
      `app/src/editor/view/mod.rs:2543` stubs `jump_to_line`, while dispatch is
      live (`crates/vim/src/vim.rs:2024`, six parser sites). So `42G`, `:42<CR>`,
      `5gg` and `d5G` silently do nothing there, while the code editor
      (`app/src/code/editor/view/vim_handler.rs:728`) and the TUI both implement it.
      Pin impl at `02b53fcd8:editor/view/mod.rs:2480` needs adapting — the fork's
      `reset_selections_to_point` is on `EditorView`, not `EditorModel`.

- [x] **#10 The model picker lost its ordering. NEEDS A JUDGEMENT CALL.**
      **[DONE `9ce314db4` — `ModelSelectorDataSource::order_model_choices`
      ported with only the "auto first" bucket, plus tests in the new
      `data_source_tests.rs`.]** `query_model_picker_choices(_llm_preferences, …)`
      (`app/src/terminal/input/models/data_source.rs:179`); the pin's first
      statement is `order_model_choices` (`02b53fcd8:data_source.rs:159`, helper
      `:251`), absent here. Two of its three bucket predicates
      (`is_custom_router_id`, `custom_llm_info_for_id`) belong to the surface
      `DECLINED.md` declines under #142/#347, so **only "auto first" is portable.**
      Confirmed against `DECLINED.md`: `is_custom_router_id` guards a
      Warp-hosted custom-model-router feature this fork never got at all (see
      the `FEATURE_INTROS` row, #404 — `FeatureIntroId::CustomModelRouter`
      ships unpopulated for exactly that reason; no `custom_model_routers`
      module or `custom-router:` id prefix exists here). `custom_llm_info_for_id`
      resolves the pin's `ApiKeyManager::custom_endpoints` store, superseded
      per #142/#347 by `AgentProviderSecrets`; `LLMPreferences` has no such
      method here. Dropped the now-fully-unused `llm_preferences` parameter
      from `query_model_picker_choices` per AGENTS.md 5.2 (delete unused
      params, don't `_`-prefix) and updated its one call site
      (`crates/warp_tui/src/model_menu.rs`). **Caveat found while porting: the
      pin's GUI `run_query` calls `query_model_picker_choices` too
      (`02b53fcd8:data_source.rs:325`), so this fix would cover the GUI there.
      This fork's `ModelSelectorDataSource::run_query` (same file, already
      diverged pre-existing from the pin) duplicates the filtering inline
      instead and never calls `query_model_picker_choices` at all — so this
      change fixes ordering for the TUI model menu (the function's one live
      caller in this fork) but NOT for the GUI dropdown, which is a separate,
      still-open gap.**

- [x] **#11 [FIXED 2026-08-10 — a real port, not a wire-up: added the
      `ShouldShowForAskUserQuestion` variant, `AskUserQuestionPermission::label()`,
      `render_autonomy_dropdown_setting_speedbump_footer`, the dropdown +
      footer + `SetPermission` action on `AskUserQuestionView`, and the seeding /
      `sync_ask_user_question_speedbump_footer` /
      `mark_ask_user_question_speedbump_as_shown` chain on `AIBlock`. The pin's
      `TelemetryEvent::ChangedAgentModeAskUserQuestionPermission` was NOT ported —
      that variant does not exist here and adding one is a cloud-telemetry change.]
      `should_show_agent_mode_ask_user_question_speedbump` has zero
      consumers.** `app/src/settings/ai.rs:2134`, covered by two tests, referenced
      nowhere else; the pin reads it at three sites in `block.rs`. Root cause is a
      feature gap: the fork's `AutonomySettingSpeedbump`
      (`app/src/ai/blocklist/block.rs:439`) has no `ShouldShowForAskUserQuestion`
      variant. `private: true`, so nothing lies in the UI — the nudge simply never
      appears.

- [x] **#12 CLI surface that parses and discards.** **[DONE `81c2ae7a1` — maintainer
      chose `hide = true`. `--share` no longer appears in `--help`; a script still passing
      it keeps parsing rather than failing at the argument parser. Recorded in
      `DECLINED.md`. The REST of this finding did not survive verification: `--name/-n`
      and the config keys `host` / `computer_use_enabled` were claimed to land in an
      `AgentConfigSnapshot` the fork never builds a task from, but `merged_config` IS
      consumed (`agent_sdk/mod.rs:502`) and `args.name` feeds it at `:237`. Those are
      deliberately left alone.]** Original finding: `oz agent run --share`
      (`crates/warp_cli/src/share.rs:13`) is **not** hidden: it appears in `--help`
      with a full recipient grammar and validates typos helpfully, then does
      nothing — `agent_sdk/mod.rs:500` hardcodes `let should_share = false;` and
      `driver/terminal.rs:85` does `let _ = options.should_share;`. It is cloud, so
      not parity debt, but a documented flag that silently no-ops is the same class
      of lie. Recommend `hide = true` or a hard error — a `--help`-visible interface
      change, so it needs a maintainer call. Same family, lower impact: `--name/-n`
      and config-file keys `host` / `computer_use_enabled` (advertised in
      `config_file.rs:94`'s help text) all land in an `AgentConfigSnapshot` this
      fork never builds a task from.

- [x] **#13 [FIXED 2026-08-10 — comment corrected, export kept.]
      `OZ_HARNESS` documents a consumer that does not exist.** The claim at
      `app/src/ai/agent_sdk/driver/harness/mod.rs:238` that child-agent telemetry
      reads it on `agent message *` is false; only `OZ_AGENT_MAILBOX_ROOT` is read.
      Comment-only correction; the export itself is still useful to the child
      process and to user hooks, so do not remove it.

### Suspicious — needs a judgement call, do NOT act as-is

- [x] **Two code-review keybindings dropped** (`app/src/code_review/mod.rs:66-93`) —
      **PORTED.** Both bindings are now registered: `code_review:toggle_file_navigation`
      (key `f`) → `CodeReviewAction::ToggleFileSidebar`, and the new
      `CODE_REVIEW_SUBMIT_KEYSTROKE` (`cmdorctrl-enter`) → the new
      `CodeReviewAction::SubmitReviewComments` unit variant, whose handler calls the
      existing `handle_submit_review_with_comments` (the same method the mouse path —
      `CommentListEvent::Submitted` — already calls; it reads the pending comment batch
      and repo path from view state, so no fields need to travel on the action). This
      resolved the field-carrying-action obstacle: `CodeReviewViewEvent::SubmitReviewComments`
      (with `comments`/`repo_path` fields) is unrelated — it is emitted *from inside*
      the handler after it recomputes those fields from state, not received by it.
      `CodeReviewView` gained a `keymap_context` override (ported near-verbatim from
      `02b53fcd8:code_review_view.rs:7153`) that inserts `CodeReviewView_NotEditing`
      into the context only when the focused view is not one of the pane's three
      text-input surfaces (`EditorView`/find bar, `RichTextEditorView`/comment
      composer, `CodeEditorView`/diff editor) — both new bindings are gated on that
      context, so neither can fire mid-edit. Two new fluent keys added to
      `app/i18n/en/warp.ftl` only (not zh-CN/ja, matching the ~125-350 keys already
      English-only there). Tests in `code_review_view_tests.rs`.
- [x] **`--bedrock-role-region`** **[DECLINED 2026-08-10 (maintainer) — "not right now". Recorded in `DECLINED.md`; left in place rather than removed or hidden, because dropping a `requires`-mandatory flag is a breaking CLI change and the path is unreachable either way.]** (`crates/warp_cli/src/agent.rs:378`) is
      `requires`-mandatory and never read; the pin threads it into
      `OidcManaged { region }` and the fork's variant has no `region` field. **But
      the path is unreachable anyway** — `refresh_aws_credentials_oidc` requires an
      ambient `task_id` (always `None` here) and mints its token through Warp's
      cloud `ManagedSecretManager`. The honest fix may be to drop both flags.
- [x] **`SshWarpifyBlockEvent::{WarpifySession, Cancel}`**
      (`app/src/terminal/ssh/warpify.rs:22`) have no emitters; the handler arms at
      `view.rs:8956/8965` are unreachable. Fork-original, so no pin counterpart to
      diff against. Not a hang: `add_ssh_warpifying_block` has a live second caller
      at `view.rs:25092`. **RESOLVED 2026-08-10: removed as dead code, not wired.**
      `git log -S` on `WarpifySession` across every fetched branch finds only the
      initial import and the pin's own later removal commit (`57062bd92`, never
      merged into this fork) — no commit ever added an emitter, upstream or here.
      The pin's pre-removal version of this exact block has the same no-button
      render. See `DECLINED.md`'s `SshWarpifyBlockEvent` row for the full trail.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: not debt — parked correctly.** The daemon fully handles `ResolveConflict` and
      **the pin has no client sender either**, so this is parity, not a gap. It would
      only become live as part of a client half that neither fork nor pin has built.
      Kept in the tree rather than deleted because the daemon side is real and
      complete; removing the client stub would make the protocol asymmetric for no
      gain. Not a checkbox any more — if a future audit re-raises it, this is the
      answer.** Original entry:
      **`crates/remote_server/src/client/mod.rs::resolve_conflict`** — zero callers
      while the daemon fully handles `ResolveConflict`, but **the pin has no client
      sender either.** Becomes live only as part of #2's client half.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: not debt — self-documented as intentional at `host_response.rs:4-8`.** The
      entry existed only so a future audit would not re-raise it, which is a note,
      not a task. Recorded here as settled.** Original entry:
      `crates/remote_server/src/host_response.rs` — four `pub fn`s with only test
      callers, self-documented as intentional at `:4-8`. Listed so the next audit
      does not re-raise it.

### Audit coverage — checked and found correctly wired

Recorded so this ground is not re-swept:

- **Every setting.** All 221 settings across 57 non-test `define_settings_group!`
  sites, counted by type name and field name, then re-run filtering for settings
  referenced only from `settings_view/`. Exactly one dead non-cloud setting (#11).
  Every other first-pass hit was a false positive from helper methods living in the
  defining file, each verified individually. Remaining zero-reference settings are
  all cloud.
- **Events, both directions.** 2,284 variants across every `*Event`/`*Events` enum
  in `app/src` and `crates`, classified emit-like vs match-arm-like and diffed
  against the pin. All resolved: `MCPGalleryManagerEvent::ItemsRefreshed` is
  orphaned by the documented cloud-gallery removal;
  `PaneConfigurationEvent::RenderElementFnUpdated`, `Event::ClearSelectedBlock`,
  `EnvVarCollectionEvent::UpdatedEnvVarCollection` and
  `CodeManagerEvent::EditCompleted` are equally dead **at the pin**; the rest are
  cloud.
- **Typed actions and keybindings.** All 1,663 variants of the 257 action enums
  diffed per-variant against the pin. 13 candidates, all 13 false positives or
  dead-upstream-too — including `TuiTerminalSessionAction::NavigateOrchestrationTabs`
  (registered `terminal_session_view.rs:789-794`, handled `:5193`; the pin's extra
  references belong to `TuiCloudRunAction` on the excluded cloud-runner surface) and
  `AttachAgentToRunningCommand` (registered `:838-846`, handled `:5123`; the pin's
  extra references are test-only, so the gap is test coverage, not dispatch).
- **Slash commands.** All 49 `StaticCommand` statics reach `all_commands()`; the
  unproduced `SlashCommandKind` variants (`Logout`, `RenameConversation`) are
  documented deliberate.
- **`remote_server` protocol.** All 21 `HostScopedRequest`, 6
  `SessionScopedRequest`, 7 `Notification` and every `ServerMessage` variant are
  two-sided except those listed above. `GetBranches` (#137) is wired end-to-end.
- **AI tool registry.** All 23 `OpenAiTool` descriptors are in `REGISTRY` — no
  advertised-but-unexecutable tool, and no executor the model cannot see.
  Web/codebase/computer-use gating is present in **both** `chat_stream.rs` filter
  paths and again at dispatch.
- **Settings sections, left-panel tool rail, command-palette modes.** Every
  remaining `SettingsSection` is reachable; all six `ToolPanelView` variants have a
  toolbelt entry; all six `PaletteMode` variants have a producer and a consumer.

### Two documentation corrections this audit produced

- [x] **[DONE 2026-08-10 — struck in place in `HANDOFF.md`.] Strike the HANDOFF.md slash-command false alarm.** Its open note that the
      fork "lacks the pin's `PrivacySettings` and `UserWorkspaces` recompute
      subscriptions" is wrong: both pin subscriptions fire only on cloud-only
      events, and `session_context()`
      (`crates/warp_tui/src/.../data_source/mod.rs:215-277`) reads none of them.
      Same for the missing `AIAutoDetectionEnabled` arm.
- [x] **[DONE 2026-08-10 — comment rewritten to name the two senders.] Stale
      comment in `remote_server.proto:164`** claims no daemon sends
      `RemoteAgentContextSnapshot`. False — `server_model.rs:978` and `:996` both do.

## MISSING SUBSYSTEMS — ALL 68 entries validated by hand 2026-08-11

**Method, because the first attempt at this was wrong twice.** The sweep's
per-area files carry 68 `MISSING-SUBSYSTEM` verdicts (36 app-ai, 16 warp_tui,
7 app-terminal, 5 crates-ai, 4 settings-workspace). An earlier pass reported
only the ~10 the agents had flagged as "highlights" and read as the whole set.

All 211 symbols named across those 68 entries were then extracted and checked
against the fork tree and against the pin's test-name index. Result:

| | count |
|---|---:|
| named symbol is a **pin test name**, not a subsystem | 104 |
| named symbol **is PRESENT** in the fork — claim wrong | 75 |
| **confirmed absent** | **32** |
| — of those, cloud (dropped on purpose, not debt) | 9 |
| — **of those, real non-cloud debt** | **23** |

Two arithmetic/judgement errors were made and corrected while producing this,
both worth recording because they are the same class of mistake the sweep made:

* The absent count was first published as 31. An exclusion regex had dropped
  `session_sharing_protocol::sharer::InitPayload`, which was never actually
  verified present. **32.**
* `AgentMessage` was briefly called cloud on a marker count taken from
  `app/src/ai/agent_events/driver.rs` — the wrong file, which merely contains a
  *different* symbol (`AgentMessageEventMetadata`). The file that matters,
  `crates/warp_tui/src/agent_block.rs` at the pin, has **zero** cloud markers and
  the section carries a plain `ReceivedMessageDisplay`. It is real debt.

**Roughly a third of missing-subsystem claims name something that exists.**
The agents reasoned from pin-side reading and inferred fork absence instead of
checking. Two rounds of validation each found more of this, so treat any
un-rechecked sweep verdict as unverified.

Confirmed-absent non-cloud symbols, grouped below. Each needs either
implementation or a `DECLINED.md` row — none should be actioned without one
more look at *why* it is absent.

### TUI agent-control — RESOLVED 2026-08-11 (#456 cluster 1)
- [x] **Mixed, and it corrected my premise.** I had recorded that
      `ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME` exists while its mechanism
      does not — "a binding with nothing behind it". **Wrong.** Commit
      `a67c9ad32` (2026-08-09, `fix(#390)`) already wired the full manual
      attach/detach mechanism. What was missing was narrower and worse: the
      *release* half.
      * `lock_for_agent_control` — **renamed.** It is
        `TuiInputView::exit_shell_mode` (`input/view.rs:1060`), same
        `AI_LOCKED_CONFIG` lock, already documented by `a67c9ad32`.
      * `reset_after_agent_control` — **absent, and a real user-facing bug.**
        Attaching the agent to a running command locked the composer to AI mode.
        If the command then finished *on its own* rather than being manually
        detached, the composer stayed **permanently AI-locked** — the user could
        not type ordinary shell commands again without a manual mode toggle.
        Neither `ModelEvent::BlockCompleted` nor
        `CLISubagentEvent::FinishedSubagent` restored it. Fixed.
      * `RUNNING_COMMAND_DETACH_HINT` — absent; footer now shows
        "ctrl-c to return to command" while attached, and ctrl-c detaches with
        priority instead of falling through to exit-confirmation arming.
      * `InputTypeAutoDetectionSource::AgentTerminalControl` — absent, and its
        **full** port stays out of scope: the fork's shared
        `BlocklistAIInputModel` deliberately carries one autodetection-source
        variant, and threading a second end-to-end is ~76 call sites across 12
        files, separately tracked as #399/#254 item d. The observable behaviour
        it gates is implemented with a per-view bool, recorded as a divergence.
      **9 real-debt symbols → 4.**

<details><summary>Original entry</summary>
- [~] **PARTIAL 2026-08-11** — `lock_for_agent_control` /
      `reset_after_agent_control` now exist and 4 tests landed (commit
      `edd9b31b6` on `working`, unbuilt). **`InputTypeAutoDetectionSource::AgentTerminalControl`
      was deliberately NOT added**: threading a second autodetection-source
      variant through the shared `BlocklistAIInputModel` is tracked separately
      as #399/#254 item d. A view-local `agent_terminal_control_lock` bool
      achieves the same behaviour and both call sites that hand the agent
      control of a running command go through it. Remaining tests in this
      cluster are unported.
      **DUPLICATE:** this entry and the "`InputTypeAutoDetectionSource::AgentTerminalControl`
      does not exist (15 tests)" entry further down are the same work, and they
      **stated opposite things** — this one said the binding landed but not the
      mechanism; the other said the mechanism landed but not the supporting
      pieces. Both are now corrected to the measured state.
      Original text: `AgentTerminalControl` /
      `InputTypeAutoDetectionSource::AgentTerminalControl`,
      `RUNNING_COMMAND_DETACH_HINT`, `lock_for_agent_control`,
      `reset_after_agent_control`. `ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME`
      IS present. 15 tests.
</details>

### TUI agent-message rendering — IMPLEMENTED 2026-08-11 (#456 cluster 2)
- [x] **Real, and now built.** New `crates/warp_tui/src/agent_message.rs` plus the
      `TuiAIBlockSection::AgentMessage(ReceivedMessageDisplay)` variant wired
      through `sections()` / `measurable_section_element()` / `render_element()` /
      `section_logical_text()`. Ported near-verbatim from the pin.
      **This was a wire-up, not a build:** the identity/colour infrastructure
      (`orchestrated_agent_identity_styling.rs`, 9 tests) was already in the fork,
      byte-identical to the pin, and unused.
      **The blocker was a wrong comment.** `agent_block.rs:1307` matched
      `MessagesReceivedFromAgents`/`EventsFromAgents` to `{}` saying "Inter-agent
      messages/events are orchestration (cloud) surfaces Zap does not render."
      They are not cloud — the dependencies are this fork's own LOCAL
      `orchestration_topology.rs` ("Local only... no remote-worker execution
      path"), already used by the shipped GUI renderer. **Twelfth in-tree document
      found contradicting the code.** `SCOPE-TERMINAL.md` had inherited the same
      false positive; both its rows are now annotated.
      **13 real-debt symbols → 10.**

<details><summary>Original entry</summary>
- [x] `TuiAIBlockSection::AgentMessage`, `agent_message_section_id`, `AgentMessage`.
      **The GUI half is built** (`blocklist/orchestration_events.rs` emits at
      `:331`, renders at `:407`). TUI only. 9 tests. **[x] 2026-08-11: this is the ORIGINAL claim, preserved under an IMPLEMENTED header. Verified present in code; it was stale checkbox state inflating the open count, not open work.]**
</details>

### Blocked-action acceptance — DISSOLVED 2026-08-11, it was a rename
- [x] `AcceptBlockedTerminalUseAction` **is present as `AllowBlockedLrcAction`**
      (and `RejectBlockedTerminalUseAction` as `RejectBlockedLrcAction`). Not a
      gap at all. The pin's `accept_active_cli_subagent_action` is mirrored by
      `terminal_session_view.rs:1450` `allow_blocked_lrc_action`, both routing to
      the same shared `execute_blocked_action` / `cancel_blocked_action` free
      functions in `tui_cli_subagent_view.rs:206,217` that the mouse
      `[Allow]`/`[Reject]` affordance already used — so the keybinding is a real
      keyboard path onto existing logic, not a stub. Test already exists:
      `allow_and_reject_blocked_lrc_actions_are_wired_to_distinct_ctrl_bindings`
      (`terminal_session_view_tests.rs:2327`).
      The binding differs deliberately (`ctrl-o`/`ctrl-r`, not the pin's
      `ctrl-enter`) and the fork documents why at `tui_cli_subagent_view.rs:31-42`.
      All three blocking-input variants relevant here are accounted for:
      LongRunningCommand/terminal-use via the above, `AskQuestion` and
      `Permission` via `agent_block::TuiBlockingChild` (`state.rs:318-325`).
      **23 real-debt symbols → 22.**

### Orchestration config-picker — DISSOLVED 2026-08-11 as CLOUD, see `DECLINED.md`
- [x] **All three are cloud, and this corrects a standing mis-tracking.** The layer
      was recorded as parity debt awaiting #310/#304; it cannot be built without
      Warp's backend. `AuthSecretSelection` resolves through
      `HarnessAvailabilityModel` → `ManagedSecretManager` + `ServerApiProvider`,
      and this fork already wires that to `DisabledManagedSecretsClient`
      (`lib.rs:1543`). `OrchestrationConfigState` / `apply_execution_mode_change`
      are typed against `RunAgentsExecutionMode`, from the #325-declined
      agent-invoked `run_agents` family — `crates/ai/src/agent/action/mod.rs:150`
      says `AIAgentActionType::RunAgents` is deliberately absent. `remote_child.rs`
      spawns children on Warp's servers and handles Warp credits.
      **#310/#304 were not "not yet built" — they were correctly not built.**
      **16 real-debt symbols → 13.**

<details><summary>Original entry, kept for the evidence trail</summary>
- [x] `OrchestrationConfigState`, `AuthSecretSelection`, `apply_execution_mode_change`.
      The picker layer only. Orchestration itself is built. #310/#304.
      **`AuthSecretSelection` needs a cloud/non-cloud ruling before implementing.**
      Its pin variants are `Named(name)` / `Inherit` / `CreatingNew` — a picker
      for which auth secret an orchestration child uses. In this fork "auth
      secret" would mean a BYOP provider key, which is local; at the pin it may
      mean a Warp *managed* secret, which is declined. It sits in the same file
      as `ORCHESTRATION_WARP_WORKER_HOST`. Decide before building, not after.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — The ruling was made 2026-08-11 and is recorded in `DECLINED.md:203` (all three CLOUD, with `sym:`/`path:` markers). This entry sits inside a `<details>` block marked historical; the unchecked box was a formatting artifact.
</details>

### Remote project skills — IMPLEMENTED 2026-08-11 (audit cluster 5) — LAST OF THE 23
- [x] **Real, and the gap was already named in-tree.**
      `app/src/ai/remote_context_files.rs:22` explicitly listed
      `skill_watcher.rs`'s `read_project_skill_contents` as one of two unwired pin
      call sites — so the fork knew, and nobody had connected it.
      **User-visible effect:** `SKILL.md` files in a project on an SSH-remote host
      were **invisible to the agent**, even though remote *bundled* and *home*
      skills already worked. Project skills were the one origin still going
      through a local-filesystem-only watcher.
      This was finishing the last file, not building machinery:
      `find_project_skill_files_in_tree`, `read_remote_text_file_contents`,
      `parse_skill_content_at_location`, `SkillPathOrigin` and a
      `LocalOrRemotePath`-accepting `SkillManager` all already existed. The fork's
      `skill_watcher.rs` was simply a pre-remote version still doing directory
      scanning, never updated to the pin's `RepoMetadataModel` standing-query
      design that works identically for local and remote.
      Ported with a generation guard, a local-only fallback when repo-metadata
      indexing fails, and two new remote-origin tests the pin did not have.
      **2 real-debt symbols → 0. All 23 resolved.**

<details><summary>Original entry</summary>
- [x] `parse_project_skill_contents`, `refresh_project_skills_for_repo`.
      `SkillWatcher` itself **exists** (`remote_server/mod.rs`) — the remote
      refresh/fallback layer on top of it does not. 13 tests. **[x] 2026-08-11: this is the ORIGINAL claim, preserved under an IMPLEMENTED header. Verified present in code; it was stale checkbox state inflating the open count, not open work.]**
</details>

### MCP config-parse cancellation — IMPLEMENTED 2026-08-11 (audit cluster 6)
- [x] **Real, genuinely absent, and it was a data-corruption bug — not just a
      missing helper.** `FileMCPWatcher` existed but every call to
      `update_servers_from_config_file` spawned a detached `ctx.spawn` parse with
      no tracking. Verified that `ctx.spawn`'s handle has **no `Drop` impl** — the
      future is owned by `AppContext.spawned_futures` — so `let _ = ctx.spawn(..)`
      genuinely detaches rather than cancelling.
      Two concrete failures it allowed, both confirmed against
      `file_based_manager.rs::apply_parsed_servers`, which applies whichever
      `ConfigParsed` arrives last, unconditionally, with no versioning:
      * **Two rapid edits to one MCP config race two parses.** A slow parse of the
        *old* content can resolve after a fast parse of the *new* one and silently
        clobber it with stale servers.
      * **A parse still in flight when its file is deleted** can land after
        `ConfigRemoved` and **resurrect MCP servers that no longer exist on disk.**
      Ported the pin's `parse_abort_handles` + `abort_config_parse`, with abort
      also called at the two other points a config is confirmed gone. Initial-scan
      parses are now tracked too — the old `spawn_config_parse` duplicate was
      collapsed into the same path. Deliberately did not port the pin's unrelated
      `is_tui`/`cloud_env_pending` bookkeeping.
      **4 real-debt symbols → 2.**

<details><summary>Original entry</summary>
- [x] `FileMCPWatcher::parse_abort_handles`, `abort_config_parse`. `FileMCPWatcher`
      **exists** (`lib.rs`); it cannot cancel an in-flight config parse. **[x] 2026-08-11: this is the ORIGINAL claim, preserved under an IMPLEMENTED header. Verified present in code; it was stale checkbox state inflating the open count, not open work.]**
</details>

### TUI CLI surface — DISSOLVED ENTIRELY 2026-08-11, all six
- [x] All six accounted for; **remove from the debt list.**
      * `phosphor_tui` — **renamed.** It is `CLIAgent::PhosphorTui`
        (`app/src/terminal/cli_agent.rs`), the pin's `CLIAgent::WarpTui`, per the
        naming call in #394: "`display_name()` must not surface 'Warp' branding to
        users." Fully tested. The lowercase string never existed.
      * `tui_commands` — **present, restructured.** `StaticCommand::kind()` plus
        `supports_tui()` / `is_tui_only()` / `supports_gui()` cover the same typed
        identity and explicit TUI-surface behaviour, spread across many tests in
        `static_commands/commands.rs`. The single combined pin test cannot port
        verbatim only because it calls `supports_surface(SettingsMode::Tui)`, and
        `SettingsMode`/`SettingSurfaces` are already declined.
      * `tui_resume_shell_command` + `tui_cli_shell_command` — **cloud, already
        neutered.** Declined; see `DECLINED.md`.
      * `provider_api_key_shell_command` + `ProviderApiKeyOperation` —
        **superseded** by this fork's two better in-process mechanisms. Declined.
      **22 real-debt symbols → 16.**

### Confirmed absent — computer use
- [x] `use_computer_decoration`. Block decoration for computer-use actions.
      Computer use itself is built and sighted as of tonight.
      **RESCOPE 2026-08-17 (v0.1.0 agent audit)** — not absent: `output.rs:2913/2961/2988` implement and wire a use-computer block decoration, tested at `output_tests.rs:36-71`. But it decorates *screenshot delivery* where the pin decorates *recording status*, so this is a different subject, not a port. Equivalence to the pin was NOT verified (the audit had no oracle access). Narrow this entry to the actual delta before acting.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Settled against the pin: `42effe840:output_tests.rs:170` tests `should_decorate_recorded_use_computer` (recording status); the fork's `output.rs:2913` does `should_decorate_blind_use_computer_screenshot` (screenshot delivery). Different subjects, and recording is formally declined under #350. Nothing actionable remains.

### Confirmed absent — CLOUD, no action (recorded so they are not re-raised)
`CLOUD_MODE_V2_COMPOSER`, `CloudEnvironmentCatalog`, `CloudModeSetupV2`,
`cloud_agent`, `cloud_mode_v2`, `not_cloud_agent`, `HandoffEntryPoint`,
`session_sharing_protocol` (bare path; its `::common`, `::sharer`, `::viewer`
sub-paths all exist), and `session_sharing_protocol::sharer::InitPayload`.

---

### Earlier hand-validated set (10 items, still accurate)

The pin-test sweep (`docs/SWEEP-SUMMARY.md`) bucketed 209 tests
MISSING-SUBSYSTEM. **Each claim below was re-verified against the tree by the
operator**, because the largest one turned out to be wrong. Three of ten were
wrong or half wrong. Do not act on a sweep verdict without this kind of check.

### Confirmed missing — real, non-cloud debt

- [x] **~~`app/src/ai/blocklist/usage/rollup.rs` is absent (8 tests)~~ — FALSE,
      corrected 2026-08-11.** The file **exists**, and so does
      `rollup_tests.rs`, which contains **exactly 8 `#[test]` functions —
      matching the pin's 8**. Nothing is missing. This entry was about to be
      handed to an agent as "the largest single remaining chunk of code work";
      it would have re-implemented an existing module.
      **Sixth-plus instance of the #148 class** (a TODO entry stating the
      opposite of the code). Note the entry even carried the word "Verified" —
      what had actually been verified was its *dependency*
      (`descendant_conversation_ids_in_spawn_order`), not its own premise.
      Original text: "`rollup.rs` is absent (8 tests). Most tractable item
      here: its sole real dependency, `descendant_conversation_ids_in_spawn_order`,
      already exists in `orchestration_topology.rs`. Verified."
- [~] **DUPLICATE of the `AgentTerminalControl` entry earlier in this file — see
      there for the live status.** Kept as a stub rather than deleted, because
      the two entries **contradicted each other** and that is worth recording:
      this one said "landed its mechanism but not its supporting pieces", the
      earlier one said "landed its binding and not its mechanism". Both were
      written from partial reads. Measured state 2026-08-11: the hint strings
      and `lock_for_agent_control`/`reset_after_agent_control` now exist
      (`edd9b31b6`), the enum variant deliberately does not (#399/#254 item d).
      Do not track this cluster in two places again.
- [x] **No TUI renderer for `MessagesReceivedFromAgents` / `EventsFromAgents`**
      (9 tests). **The GUI half is built** — `blocklist/orchestration_events.rs`
      both emits (`:331`) and renders (`:407`) them. Only the TUI side is
      missing. Second of the two clusters behind #456.
      **PARTIALLY STALE 2026-08-17 (v0.1.0).** "No TUI renderer" is no longer true for the first half: `crates/warp_tui/src/agent_block.rs:1349` renders `MessagesReceivedFromAgents { messages }`. `EventsFromAgents` is still an empty match arm at :1356, so only that half remains. Rescope before working it.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — **Not a gap at all — my earlier "partially stale" note was itself wrong.** The pin's own arm is `EventsFromAgents { .. } => {}` (`42effe840:crates/warp_tui/src/agent_block.rs:1616`) with the identical comment: "Event IDs contain no display detail." The fork's `:1356` is byte-identical. The empty arm is the pin's deliberate no-op, faithfully ported; `MessagesReceivedFromAgents` matches too. I read intentional parity as debt because I checked the fork and not the oracle.
- [x] **~~No `/index` slash command~~ — FALSE, corrected 2026-08-11.** It
      exists: `app/src/search/slash_command_menu/static_commands/mod.rs:257`
      maps `"/index" => SlashCommandKind::Index`. This entry also said
      "Verified", which it was not.
      *(If the real complaint is that the command exists but does nothing
      useful, that is a different, narrower item — file it with the dispatch
      site as evidence, not as "no command".)*
- [x] **MCP tool results render as a `serde_json::to_string_pretty` blob, not a
      collapsible tree.** `McpRenderable` / `mcp_result_to_renderable` exist
      **nowhere in production** — the only tree-wide hits are a comment in
      `ui_components/json_tree_tests.rs` noting the pin has 5 tests for them.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Claim is false: `McpRenderable` (`requested_command.rs:180`) and `mcp_result_to_renderable` (`:192`) exist in production and the render path builds an interactive collapsible tree (`:1586-1638`). `to_string_pretty` survives only in the copy-to-clipboard callback at `:1607`.
- [x] **`TuiSelectable::with_semantic_selection_by_style` does not exist** —
      double-click cannot select a whole styled span. Verified: no definition
      anywhere. **Note the correction below: its sibling DOES exist.**
      **RECONCILED 2026-08-17 (v0.1.0) -- claim is false.** It exists: defined at `crates/warpui_core/src/elements/tui/selectable.rs:134` and used at `crates/warp_tui/src/read_only_menu.rs:230`.
- [x] **`skill_watcher.rs` lacks the remote-project-skill refresh/fallback
      layer** (13 tests).
      **RECONCILED 2026-08-17 (v0.1.0) -- claim is false.** The layer is present in `app/src/ai/skills/file_watchers/skill_watcher.rs`: it imports `read_remote_text_file_contents` and `LocalOrRemotePath`, and :185 documents the fallback behaviour for remote repos explicitly.
- [x] **`languages::language_by_filename` has no `StandardizedPath` overload** —
      remote files resolve their language through a host-local `Path`. This is a
      deliberate fork simplification (documented on
      `try_chunk_code_semantically`), so it may belong in `DECLINED.md` rather
      than here — needs a maintainer call.
      **RECONCILED 2026-08-17 (v0.1.0) -- claim is false.** The overload exists: `pub fn language_by_filename(path: &StandardizedPath)` at `crates/languages/src/lib.rs:145`.

### Partly confirmed — the fork is host-blind, but the scaffolding is there

- [x] **Remote project rules have no `HostId` dimension** (6 tests,
      `crates/ai/src/project_context/model.rs`) — **DONE 2026-08-11**, commit
      `721e4869d` on branch `working` (unbuilt; freeze in force). Adds
      `remote_path_to_rules: HashMap<HostId, HashMap<PathBuf, ProjectRules>>`
      alongside the local map rather than re-keying it, so local lookups are
      untouched. Followed `global_rules.rs` (#575), which had solved the
      identical problem. The `warp_util::host_id::HostId` vs `warp_core::HostId`
      bridge is explicit and documented, mirroring
      `buffer_location.rs::core_host_id_to_util`. Stale `:272`/`:617`/`:1081`
      comments updated.
      **Note: this was double-counted** — the same work is one of the 50
      MISSING-SUBSYSTEM tests (`project_context/model_tests.rs`). The
      symbol-axis entries in this section overlap the test-axis list above; see
      the reconciliation note at the head of this section.

### CLAIMS THAT DID NOT SURVIVE VALIDATION — corrected, no work implied

- [x] **`app/src/ai/orchestration/` "doesn't exist at all" (39 tests) — WRONG.**
      Orchestration IS built here: `orchestration_topology.rs` (26 tests),
      `orchestration_events.rs` (10), four `agent_view/orchestration_*` modules,
      `block/view_impl/orchestration.rs`, `warp_tui/orchestration_{model,tab_bar}.rs`.
      Only the pin's **config-picker layer** is absent
      (`config_state`/`edit_state`/`providers`/`remote_child`/`snapshots`/`validation`)
      — the UI for *choosing* harness/model/environment/host. Tracked #310/#304.
      Corrected in `docs/SWEEP-SUMMARY.md` and `docs/sweep/app-ai.md`.
- [x] **`with_trimmed_selection_line_ends` "does not exist" — WRONG.** It exists
      at `crates/warpui_core/src/elements/tui/viewported_list.rs:471` **and is
      called at `crates/warp_tui/src/read_only_menu.rs:221`** — about 40 lines
      below the doc comment that claimed neither it nor its sibling existed.
      Comment corrected 2026-08-11. Only `with_semantic_selection_by_style` is
      genuinely absent. **Eleventh in-tree document found contradicting the code.**

## FLAG-DARK FEATURES — corrected 2026-08-15, 15 restored

**A category of its own, and much cheaper than parity debt: the code is already
here, already compiling, already tested. Some of these needed only a line in a
list.** Do not size any of them as porting work until the three-path check below
says otherwise.

### The corrected accounting — most dark flags are NOT this fork's doing

An earlier draft of this entry implied de-clouding had turned everything off.
That overstated it. Of 231 flags, 90 are unreachable in a normal GUI build, and
they split three ways:

| | count | verdict |
|---|---:|---|
| dark at the pin **too** | **69** | upstream gates these per-account from its backend. Not fork regressions. **Do not "fix" them.** |
| fork-original flags | 5 | this fork's own, off by its own choice |
| **on at the pin, off here** | **16** | **the fork lost the switch** — no `FORCE_DISABLED_FLAGS` entry, no recorded reason |

Only that third group is drift. See `docs/pin-migration.md` Phase 6.7 for the
method, the four traps, and why a naive script reports the pin as having 7
reachable flags (that is the size of `RELEASE_FLAGS`, and it means your parse
failed — the flag-init file MOVED between pin and fork).

### Root cause

The pin's `app/Cargo.toml` `default` list carries 193 entries; this fork's had
141. The list was trimmed once, wholesale, and never reconciled. Eight of the
sixteen additionally had no `#[cfg(feature = "x")] FeatureFlag::Y` entry at all,
so they were declared-but-never-wired — adding the feature to `default` alone
would have done nothing.

- [x] **Restore the 15. DONE 2026-08-15.** Declared the 8 missing cargo features,
      added the 8 missing `extra_flags` entries, and put all 15 in `default`.
      `SoloUserByok` deliberately excluded — BYOK is bring-your-own-**key**
      against Warp's account system, irrelevant to a BYOP fork, and it had one
      reference in `lib.rs` with no implementation behind it.

      Verified all 15 are default-on at the pin before enabling: 14 via the pin's
      `default`, `DragTabsToWindows` via its `RELEASE_FLAGS`. Compiles clean.

      What was restored, with the implementation each unlocks:
      | flag | implementation |
      |---|---|
      | `GroupedTabs` | `app/src/workspace/tab_group.rs` (54 call sites) |
      | `QueueSlashCommand` | `app/src/ai/blocklist/queued_query.rs` |
      | `PinnedTabs` | `app/src/workspace/view/tab_grouping.rs` |
      | `TerminalLifecycleRecovery` | `app/src/terminal/model/lifecycle/` (+ own tests) |
      | `AgentHarness` | gates every **non-Oz** harness — see below |
      | `CodexPlugin` | `app/src/terminal/cli_agent_sessions/plugin_manager/codex.rs` |
      | `GitOperationsInCodeReview` | commit / push / create-PR from the review panel |
      | `PendingUserQueryIndicator` | `app/src/terminal/view/pending_user_query.rs` |
      | `RemoteCodebaseIndexing` | `app/src/ai/codebase_auto_indexing.rs` |
      | `BackgroundComputerUse` | `execute/{use_computer,request_computer_use}.rs` |
      | `DirectoryTabColors`, `DragTabsToWindows`, `VerticalTabsSummaryMode`, `QueuedPromptsV2`, `AsyncFind` | tab/window/find surfaces |

**`AgentHarness` is the one that mattered most.** `agent_sdk/mod.rs:151` rejects
`--harness` for anything that is not `Harness::Oz` when the flag is off, and
`agent_view.rs:165` *silently drops CLI agent conversations on restore* with only
a `log::warn!`. Oz is Warp's cloud agent, which this fork does not have at all —
so the flag being off blocked the Claude Code and Codex harnesses that a BYOP
fork exists to use.

- [x] **CLOSE 2026-08-17 (audit-debt triage) — not a work item, a diagnostic
      pointer; the checkable half is checked.** Both flags are default-on and
      match the pin: `background_computer_use` (`app/Cargo.toml` default `:158`)
      and `remote_codebase_indexing` (`:168`). `BackgroundComputerUse` has **2**
      runtime call sites (`execute/use_computer.rs:51`,
      `execute/request_computer_use.rs:93`) and **0** tests.
      `RemoteCodebaseIndexing` has **7** call sites and *is* covered both ways
      (`app/src/ai/codebase_auto_indexing_tests.rs:41,50` override it false and
      true). Nothing further is settleable by reading, and this host cannot run
      the app. Reopens on an observed regression, at which point these are the
      first two places to look — which is all the entry ever asked for.
      Original finding:
      **Watch these two at runtime.** Both touch subsystems this fork has already
      mis-classified once and had to reverse: `BackgroundComputerUse` (computer
      use is the documented false-positive in `FORCE_DISABLED_FLAGS`' own comment)
      and `RemoteCodebaseIndexing` (`DECLINED.md` carries a codebase-indexing
      reversal). If behaviour regresses after this change, start here.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: leave it as-is.** Measured zero drift from the
      pin — `42effe840:app/src/terminal/settings.rs:230` is the identical
      composite and `async_find` is in the pin's `default` too, so the lost
      user toggle is lost UPSTREAM, not here. No divergence, no action.
      Original entry:
**NEEDS-MAINTAINER-DECISION 2026-08-17 (audit-debt triage) — premise true,
      but it is not fork drift: the fork is byte-identical to the pin here.**
      `42effe840:app/src/terminal/settings.rs:230` is the same composite
      (`FeatureFlag::AsyncFind.is_enabled() || *self.async_find_enabled`), the
      same explanatory comment sits at `:197-198` on both sides, and `async_find`
      is in the pin's `default` list too (`42effe840:app/Cargo.toml`). So the
      "lost toggle" is lost *upstream*, by upstream's design — turning the flag on
      did not introduce a divergence, it removed one. The only thing left is a
      product call. **Question: do we want to deliberately diverge from the pin so
      `experimental.async_find_enabled` stays a live user toggle — yes or no?** If
      no, this closes with a `DECLINED.md` row. Size if yes: one line in
      `app/src/terminal/settings.rs` plus a settings-UI surface; the setting is
      currently read at exactly one place (`app/src/terminal/find/model.rs:246`).
      Original finding:
      **`AsyncFind` is an override, not a gate**
      (`FeatureFlag::AsyncFind.is_enabled() || *self.async_find_enabled`). Turning
      it on matches upstream but *force-enables* async find and hides the user
      toggle. Revisit if the lost setting is wanted back.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: leave the three as they are; `DECLINED.md` row written.** The two genuinely dark flags (`WelcomeTab`, `NldImprovements`) are now default-on. Original entry:
      **PARTIALLY RESOLVED 2026-08-17 — the two GENUINELY dark flags are now
      default-on; only the 'leave the other three?' question remains.**
      `WelcomeTab` and `NldImprovements` were the only two in *no* list, i.e. the
      only two whose code could not run in any build. Maintainer said yes to both;
      `welcome_tab` and `nld_improvements` are now in `app/Cargo.toml`'s `default`
      with the reasoning in comments. Note `nld_improvements` pulls
      `nld_onnx_model` -> `input_classifier/onnx_candle`, which is **local** candle
      inference, not a provider call — no BYOP cost implication — and the models
      ship in-repo (`crates/input_classifier/models/`), with
      `InputClassifierModel::new` degrading to the fasttext/heuristic path on load
      failure, so it cannot hard-fail startup.
      **STILL OPEN — one yes/no:** leave `WindowsHighPerformanceGpuDefault`
      (`UNSTABLE_FEATURES`), `NLDClassifierModelEnabled` (`DOGFOOD_FLAGS`) and
      `ConfigurableContextWindow` (`DOGFOOD_FLAGS`, and a rename of the pin's
      `GPTConfigurableContextWindow` which is **not default-on upstream either**,
      so it is at parity) as they are? **None of these three is actually dark** —
      each is reachable through its toggle. A 'yes, leave them' still wants a
      `DECLINED.md` row so they stop being re-filed as dark code.
      Original finding below. **measured: 16
      fork-original flag names, of which 3 are renames and 5 are dark.** The
      `FeatureFlag` enum has **232** variants here vs **290** at the pin; 16 names
      exist here and not there. Three of the 16 are renames of pin flags, not
      fork inventions — `ZapLaunchModal` = `OpenWarpLaunchModal`,
      `ZapNewSettingsModes` = `OpenWarpNewSettingsModes`, `ConfigurableContextWindow`
      = `GPTConfigurableContextWindow` — and the first two are already in
      `default`. Eight more are fork-original **and already default-on**
      (`SettingsImport`, `SSHTmuxWrapper`, `LessHorizontalTerminalPadding`,
      `BlockToolbeltSaveAsWorkflow`, `RemoveAltScreenPadding`,
      `ChangedLinesOnlyApplyDiffResult`, `PRCommentsSlashCommand`,
      `APIKeyAuthentication`), and `HttpProxySettings` is inserted unconditionally
      at `app/src/lib.rs:2835` with its reason already in-source (issue #72).

      **The 5 that are actually dark, with what each gates:**
      | flag | cargo feature | where it lives now | gates |
      |---|---|---|---|
      | `WindowsHighPerformanceGpuDefault` | `windows_high_performance_gpu_default` | not in `default`; **is** in `UNSTABLE_FEATURES` (`lib.rs:3271`), so runtime-togglable | `app/src/settings/gpu.rs:6` — Windows high-perf GPU / Vulkan preference |
      | `NLDClassifierModelEnabled` | `nld_fasttext_model` | not in `default`; in `DOGFOOD_FLAGS` (`warp_features/src/lib.rs:809`) | `app/src/input_classifier.rs:66,68` |
      | `ConfigurableContextWindow` | `configurable_context_window` | not in `default`; in `DOGFOOD_FLAGS` (`:838`) | rename of the pin's `GPTConfigurableContextWindow`, which is **also not in the pin's `default`** — so this one is *at parity* and arguably needs no decision at all |
      | `WelcomeTab` | `welcome_tab` | in **no** list — fully dark | `app/src/pane_group/mod.rs:1860`, `app/src/workspace/view.rs:3755,20067` |
      | `NldImprovements` | `nld_improvements` | in **no** list — fully dark | `app/src/terminal/universal_developer_input.rs:914,940`, `app/src/terminal/input.rs:11970,12077`, tested at `input_test.rs:5939` |

      **Questions, each answerable yes/no:** (1) Should `WelcomeTab` be default-on?
      It gates a first-run tab in the workspace and is reachable from three call
      sites. (2) Should `NldImprovements` be default-on? It has live code on the
      NLD input path *and a regression test*, so it is dark code that is
      nonetheless tested. (3) Leave `WindowsHighPerformanceGpuDefault`,
      `NLDClassifierModelEnabled` and `ConfigurableContextWindow` as they are
      (unstable-toggle / dogfood / pin-parity respectively) — yes or no? A "no
      change" answer to all three still wants a `DECLINED.md` row, which is what
      the original entry asked for.
      Original finding:
      **Decide the 5 fork-original dark flags**, and record the reason in
      `DECLINED.md` rather than leaving them merely absent from every list.
- [x] **CLOSE 2026-08-17 (audit-debt triage) — the speculation is refuted and the
      count was the wrong quantity. Measured: 50 pin-only `default` entries, and
      all 50 gate feature flags. None gate build behaviour.** Re-measured today
      against `42effe840`: the pin's `default` list is **197** entries, this
      fork's is **157**; the *set* difference is **50 pin-only** and **10
      fork-only** (197−157 = 40 is a count difference, not a workload — the same
      net-vs-workload error `ORACLE.md` warns about, and the source of the "~37").
      Every one of the 50 resolves to a `#[cfg(feature = "x")] FeatureFlag::Y`
      pair in `42effe840:app/src/features.rs` and appears in **no other file at
      the pin** — verified by `git grep -l 'feature = "<name>"' 42effe840` for all
      50, which returns `app/src/features.rs` alone (one extra hit is a spec
      markdown). So the entry's worry ("may gate build behaviour rather than
      flags") is false for the whole remainder.

      **And they are out of scope, not debt.** All 50 map to flags on the
      `PIN-ONLY` side of the `FeatureFlag` enum diff, and they are Warp's account
      and cloud surface: `cloud_mode*` (5), `oz_*` (4), `handoff_*` (3),
      `ambient_agents_*` (4), `*shared_session*` / `*shared_sessions` (4),
      `warp_managed_secrets`, `team_api_keys`, `usage_based_pricing`,
      `billing_and_usage_page_v2`, `account_first_onboarding`,
      `skip_firebase_anonymous_user`, `global_ai_analytics_collection`,
      `cloud_runners`, `cloud_agent_runners`, `cloud_environments`,
      `cloud_conversations`, `conversation_api`, `supergrok`, `gemini_enterprise`,
      `solo_user_byok` (already excluded on purpose — see the entry above),
      `custom_model_routers`, `named_agents`, `agent_management_view` /
      `_details_view`, `remote_code_review`, `git_credential_refresh`,
      `search_codebase_ui`, `context_window_usage_breakdown`, `integration_command`,
      `artifact_command`, `create_environment_slash_command`,
      `file_backed_execution_profiles`, `shared_block_title_generation`,
      `loginless_conversion`, `session_sharing_acls`. Adding any of these to
      `default` would light up a `FeatureFlag` whose implementation this fork
      dropped. **No work follows from this entry.** If any single one is later
      wanted, it is a per-flag port, not a `default`-list edit.
      Original finding:
      **Audit the remaining ~37 `default` entries the pin has and this fork does
      not.** 15 of the 52-entry gap gated feature flags; the rest were not
      examined and may gate build behaviour rather than flags.

## UNPORTED UPSTREAM FIXES — 2026-08-15 AUDIT (fleet walk of `0dbd3d56..02b53fcd8`)

Findings from the partial-port sweep now written up as Phase 6.5 of
`docs/pin-migration.md`. **These are FULLY unported, not partial ports** — the
sweep's remit was to fix partials and merely record these, because a never-taken
commit is a scope decision rather than a silent defect. Nothing below is fixed.

Verified by the coordinator where marked. Agent worktrees were based at
`1990bdef8`; findings were reconciled against `main` before recording.

### workspace / settings / TUI (612 commits, 116 fix-flavoured, 34 partial candidates; 5 FIXED)

Five fixed and merged, two of them security (`c12836866` MCP `+Add` redaction
bypass, `175776e22` unescaped exec path). Remaining:

- [x] **CLOSED 2026-08-17 — the coverage hole is now SWEPT.** Full report:
      `docs/sweep/warpui-coverage-2026-08-17.md` (572 lines).
      **Census:** 472 files here / 479 at pin; 137 byte-identical, 333 differ, of
      which 7 are whitespace-only and 75 imports-only — **251 differ in code**, and
      only 20 exceed 100 changed code lines. The prior *"356 of 479 diverge"* was not
      a divergence count at all: it was `git diff --name-only | wc -l` inflated by **82
      rename pairs** (the fork flattened `elements/gui/*` -> `elements/*` and renamed 26
      `*_tests.rs` -> `*_test.rs`).
      **Test comparison: only 7 pin tests absent** of 696 — 6 DECLINED, 1
      COVERED-ELSEWHERE, **0 PORTABLE**. The fork has **734 tests here vs the pin's
      696**, including 45 fork-original ones in the risk areas (damage tracking, CJK
      shaping, input fallback). **The area was never undertested — only unaudited.**
      **The "~1,240 unswept commits" figure is corrected:** within the only verifiable
      window (231 commits, the clone is shallow-grafted at `02b53fcd8`), 22 touch these
      crates — 19 ported, 1 declined (voice push-to-talk), 2 a land-and-revert pair.
      The re-pin round DID sweep them commit-wise; only the audit was missing.
      **Limits, stated:** ~70 of the 251 code-diverging files were read;
      `core/app.rs` (1,374 changed lines) was sampled, not exhausted — and finding #2
      below proves one missed site there is a live defect. Original entry:
      **#13591 — wide-grapheme stray-character rendering bug.** Real, unfixed.
      Root cause sits in `crates/warpui_core` / `crates/editor`, which **no agent
      in the 2026-08-15 fleet owned** — the partition covered `app/src`, the
      terminal/AI/completer crates and `warp_tui`, but not `warpui_core`,
      `warpui`, or `warpui_extras`. That is a genuine coverage hole in the audit,
      not just an unfixed bug: **~1,240 upstream commits touch `warpui_core` and
      `warpui` and have never been swept at all.**
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — **The #13591 example is wrong** — `diff` of `crates/warpui_core/src/runtime/renderer.rs` against the pin is EMPTY (byte-identical), and `renderer_tests.rs:201` has `wide_grapheme_does_not_shift_following_cells`. That bug is already fixed here. The coverage-hole half stands: `docs/FLEET-ROUND.md:64-81`'s partition table has no `warpui_core`/`warpui`/`warpui_extras` row. Replace the example or drop it; do not cite #13591 as unfixed.
      **KEEP-SIZED 2026-08-17 (audit-debt triage) — bug half CLOSED, coverage hole
      REAL but the "~1,240 commits" figure is not measurable here and is very
      likely wrong.** Both refutation facts re-confirmed independently:
      `git diff 42effe840 -- crates/warpui_core/src/runtime/renderer.rs` emits
      **0 lines**, and `crates/warpui_core/src/runtime/renderer_tests.rs:201`
      is `fn wide_grapheme_does_not_shift_following_cells()`. **Stop citing
      #13591.**

      On the coverage hole — it is real, and here is a number that can actually be
      checked. **Correct the citation first:** `docs/FLEET-ROUND.md:61-68` is the
      *re-pin* round's shard table (`02b53fcd8 → 42effe840`, 6 agents), not the
      2026-08-15 audit's partition — that audit's path set was never written to
      any file, only to the agent briefs. Either way neither names
      `warpui_core` / `warpui` / `warpui_extras`, so nothing has ever swept
      them and the conclusion stands. But **the commit count cannot be reproduced
      in this clone**: the local
      history is *shallow*, grafted at `02b53fcd8` (`.git/shallow` contains
      exactly that sha), so `0dbd3d56..02b53fcd8` — the range the fleet walked —
      resolves to **1** commit here, and only **232** commits are reachable from
      the pin at all. Anyone re-deriving 1,240 must first
      `git fetch warp --shallow-since=2026-04-27`; until then treat 1,240 as
      unverified. In the window that *is* present (`02b53fcd8..42effe840`, the
      current re-pin) only **23** commits touch the three crates, and they are
      TUI-rendering, font/glyph and winit-windowing fixes (`#14322` GPOS offsets,
      `#14651` TUI probe-reply leak, `#14594` inline-block clipping, `#14585`
      retained text measurements, `#13144` reserved glyf flag bit, `#14109`
      fullscreen corners, `#14726` Quake-mode focus).

      **A better size than a commit count, measured today:** the fork's three
      warpui crates diverge from the pin in **356 of 479** files —
      `warpui_core` 195/262 (+5,711/−4,643), `warpui` 148/198 (+2,491/−2,156),
      `warpui_extras` 13/19 (+166/−104). Much of that is the fork's known
      renames (`elements/gui/*` flattened to `elements/*`, `*_tests.rs` →
      `*_test.rs`), so it is an upper bound, not a defect count — but it is an
      *unswept* upper bound, which is the point. Largest single divergences:
      `warpui_core/src/core/app.rs` (+1,038/−708),
      `warpui/src/windowing/winit/fonts/windows.rs` (+257/−30),
      `warpui_core/src/runtime/terminal_probe.rs` (+223/−3),
      `warpui/src/windowing/winit/event_loop/mod.rs` (+215/−163).
      **Action: add a `warpui*` row to `docs/FLEET-ROUND.md`'s partition table so
      the next round owns it.** Post-release — this is parity hygiene, and the
      one concrete bug ever cited against these crates is already fixed here.
- [x] **COMPLETE 2026-08-18 — all 54 adjudicated.** 16 already carried a verdict (the
      prior estimate said 10); **38 were genuinely unverified**, and of those **28 are
      PORTED**. Six live defects fell out and are filed separately below.
      **⛔ The `17 PORTABLE / 31 PORTED / 3 MISSING-SUBSYSTEM / 2 CLOUD / 1 DIVERGENT`
      breakdown quoted earlier appears NOWHERE in the repo** — unversioned prose, and
      it does not survive measurement. Same failure mode `artifacts-2026-08-15/README.md`
      documents: a number from a summary rather than from a command.
      **`partials_clean.txt` is wrong in BOTH directions on roughly a third of the
      candidates** — false MISSes from import reordering, let-chain rewrites, and three
      file relocations (`prompt_history_menu.rs` -> `prompt_and_command_history_menu.rs`;
      `terminal_session_view_tests.rs` -> `terminal_use_tests.rs`; `app/src/index/` ->
      `crates/ai/src/index/`), and false PARTIALs where the fork has evolved **past** the
      candidate to the pin's later shape. Adjudicating from its MISS lines alone would
      have produced at least six spurious PORTABLE verdicts.
      73 proposed ledger rows exist in the shard outputs but are **not** pasted: they use
      invented `area` values, `high`/`medium` confidence, and `-` for empty
      `declined_ref`. Normalising and appending them is a deliberate decision, not a
      mechanical step — the ledger currently has zero `PORTABLE`/`MISSING-SUBSYSTEM`
      rows by design. Original entry:
      **~25 of the 34 PARTIAL candidates were never hand-verified** (tab-group
      rename bugs, cross-window drag "fuzzy shake", MCP install-modal styling,
      and others). The triage script flagged them; nobody read them. These are
      the cheapest remaining wins in this area — the expensive part (finding
      them) is already done.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — **Drop the "cross-window drag 'fuzzy shake'" example** — fixed by `c3f4b667a` (2026-08-04), confirmed an ancestor of `main`, eleven days before the fleet round. The "~25 of 34" count and the other two examples are unfalsifiable without the source list of 34.
      **KEEP-SIZED 2026-08-17 (audit-debt triage) — the source list was FOUND, and
      both numbers in this entry are wrong. Real size: 54 PARTIAL candidates, of
      which ~44 are unverified.**

      **The list is not lost.** The 2026-08-15 workspace/settings/TUI agent's
      working files survive in the session scratchpad
      `/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad/`:
      `all_commits.txt` (**612** lines — the universe, confirming that figure),
      `fix_candidates.txt` (**116** lines, reproduced byte-identically from
      `all_commits.txt` with the recovered filter), and **`partials_clean.txt`
      (917 lines, 54 `===` blocks)** — the PARTIAL candidates with per-file
      `OK:` / `MISS:` evidence for each. Also `count_commits.sh` /
      `list_subjects.sh`, which record the owned path set verbatim:
      `app/src/workspace/  app/src/pane_group/  app/src/settings/  app/src/settings_view/  crates/warp_tui/`.
      No triage script was ever committed; the filter was an inline `grep -Ei`
      over commit subjects.

      **"34" was a measurement bug.** It came from
      `grep -B1 "PARTIAL" triage_out.txt | grep "^===" | sort -u | wc -l`, which
      only catches a commit whose **first** listed file was PARTIAL. The same
      agent's correct extractor printed `TOTAL PARTIAL BLOCKS: 54` three commands
      later. **54 is the real count**, all 54 are a subset of the 116, and all 54
      are in `partials_clean.txt`. Minus the 5 fixed and the 5 already rejected as
      false positives, **~44 remain unverified, not ~25.**

      The list visibly contains all three examples this entry cites: tab-group
      renames (`8089a74d3c` #13289, `3015d875b7` #13261, `8794f73251` #12706,
      `ebaef155b3` #12383), the "fuzzy shake" (`79fd190898` #13007 — already
      refuted above as fixed by `c3f4b667a`, so drop it), and MCP install-modal
      styling (`836b73c88d` #10586, `3ac1efb032` #10166).

      **Cost to finish: reading only, but gated on one fetch.** This clone is
      shallow, grafted at `02b53fcd8` (`.git/shallow`), so
      `git rev-list --count 0dbd3d56..02b53fcd8` returns **1** and only 232
      upstream commits exist locally. Reading any of the 54 diffs needs
      `git fetch --unshallow warp` first. **Release relevance: no** — these are
      upstream bug fixes in already-shipping subsystems, and the sweep's own remit
      recorded rather than fixed them. Post-release.

      ⚠️ **The artifacts are in `/tmp` and unversioned.** `triage_out.txt`, the
      1400-line master report, was already clobbered once by another agent writing
      the same filename in the shared scratchpad. **Copy `all_commits.txt`,
      `fix_candidates.txt`, `partials_clean.txt`, `count_commits.sh` and
      `list_subjects.sh` into `docs/sweep/` before the next reboot** — this triage
      pass was scoped to `TODO.md` only and deliberately did not create them.
- [x] **CLOSED 2026-08-18 — NOT WORTH DOING, and the premise dissolves.** Full
      report: `docs/sweep/excluded-commits-2026-08-17.md` (914 lines, includes the
      complete 496-commit list by stratum). Count verified at **exactly 496**.
      **The decisive finding: all 496 predate the graft at `02b53fcd8`, which IS the
      fork point — their content arrived by inheritance and is already in this tree.**
      Demonstrated end-to-end on `0167b43a8`: the line it adds is present at the old
      pin, the current pin, and in the working tree. So a sweep here cannot find
      unported upstream work; it could only find de-clouding collateral damage.
      (`merge-base --is-ancestor` answering NO for 495/496 is the graft truncating
      the walk, not real ancestry.)
      **Sample: 40 commits, stratified and seeded, 0 findings — 0% hit rate in every
      one of the 7 strata.** Representative: 24 of the `warpui_core` files touched by
      `fa0d6fc85` are byte-identical to the pin; `4b77c4de2` gives 71 `TabBar` sites
      on both sides; `164e60e42` (OSC 52) is present and AHEAD of the pin. The two
      absences found are both already in `DECLINED.md` (#404; #142/#347).
      **Readability correction:** the clone is shallow AND a `blob:none` partial
      clone. Commit objects 496/496 present, file lists 496/496 readable, but **full
      diffs only 18/496** — `--unshallow` is the wrong fix, a blob backfill is.
      **Scope correction:** this was never a global backlog — all 496 touch
      `workspace/`, `pane_group/`, `settings/`, `settings_view/` or `warp_tui/`, and
      184 of them are cloud/billing/orchestration whose files are deleted here.
      **Limits stated:** 478 diffs unread (end-state parity substituted, weakest for
      `workspace/view.rs`'s 13,696 diverging lines); 8% coverage; 0/40 gives a 95%
      upper bound of ~7.2% overall. If residual coverage is ever wanted, read the 18
      already-diffable commits plus any S2/S3 commit diverging >200 lines — a couple
      dozen, not 496. Better spend named by the report: the 44 unverified PARTIAL
      candidates, and re-deriving `SCOPE-*.md` at `42effe840`.
      Original entry:
      **KEEP-SIZED 2026-08-17 (audit-debt triage) — "~82" IS A FABRICATED NUMBER.
      The real remainder is 496.** No command in the 2026-08-15 agent's transcript
      produces 82; the only `82` in the whole log is `82 insertions(+)` from an
      unrelated `git commit`. The agent's prose summary said "~82 non-fix-flavored
      commits of the 612" and `TODO.md` copied it. The arithmetic is
      **612 − 116 = 496**, and the list is one line away from the preserved
      artifacts:
      `comm -13 <(sort fix_candidates.txt) <(sort all_commits.txt)`.

      So: **cheap to list, expensive to examine.** 496 commits, and by
      construction they are exactly the ones the fix-keyword filter judged
      *least* likely to contain defects — features and refactors. Reading them is
      a full second sweep of the same area, not a mop-up. It also needs
      `git fetch --unshallow warp` (see the entry above). **Recommend: do not
      queue all 496.** Either scope it to a named subsystem, or drop it in favour
      of the 44 PARTIAL candidates above, which are pre-filtered evidence-bearing
      leads. **Release relevance: no.** Post-release.
      Original finding:
      **~82 non-fix-flavoured commits** (features/refactors) were excluded by the
      keyword filter and never examined.

**#12492 (WSL UI freeze from redundant `canonicalize()`) — REFUTED 2026-08-15,
already fully ported. Do not re-raise.** The workspace agent flagged it and
handed it to the terminal agent, which triaged `aa873b543d` and found it present
on both sides. **COORDINATOR-VERIFIED:** `LocalSessionCanonicalPwdCache`
(`terminal/view.rs:2299`), the `canonical_session_pwd_cache` field keyed on the
non-canonical path (`:2576`), and the cache check inside
`canonical_session_pwd_if_local` (`:7054`, `:7061`) are all present — and the
actual freeze source, the every-10s scan across every terminal in every window,
already calls the memoized accessor (`code/language_server_shutdown_manager.rs:110`)
with no raw `.canonicalize()` left in the terminal hot path.

This one is worth keeping visible as a *method* result, not just a corrected
row: an agent reported a real-looking bug in code it could not see, and the
agent that owned that code refuted it in one pass. Cross-area findings are
hypotheses until the owning area checks them — which is the entire argument for
the refutation fleet.

Correctly report-only: **#12710** (hidden-files default) — the fork's default is
already `true`; only the Settings UI toggle is absent, which is a feature.

**False positives this agent rejected (do not re-raise):** #10839 (MCP redaction
toggle — `UserWorkspaces`→`PrivacySettings` rename), #14270 (NLD-after-agent-
terminal-use — deliberately adapted per #399/#254), #13594 (duplicate shell
commands — `agent_view_state`→`transcript_scope`, `LayoutInvalidated`→
`LayoutChanged` renames), #14425 (skill-status zero-state — `InventoryChanged`
covers it).

### ai/agent stack (251 commits triaged, ~35 hand-verified, 1 partial — FIXED)

The partial (`6f24ea230`, bounded parent-bridge retry) is fixed and merged. Every
other unported commit in this area was cloud and is now recorded by name in
`DECLINED.md` so it stops being re-derived. One non-cloud item is left:

- [x] **DONE 2026-08-18 (3 edits as scoped). UNVERIFIED — not compiled.** The refutation was
      right: the symbol was absent but the behaviour was present. The fork suppressed
      **unconditionally** (`app/src/ai/blocklist/view_util.rs:113`); the pin gates on
      `!ChannelState::channel().is_dogfood()`. Added `should_suppress_during_recovery` to
      `app/src/ai/agent/mod.rs` with the pin's verbatim condition, swapped the call site,
      and updated `crates/warp_tui/src/agent_block_tests.rs` to **assert** rather than set
      the channel — `ChannelState` is a process-global behind a mutex shared by every test
      in the binary and `init()` defaults to `Oss` (non-dogfood), so the test already
      exercised the suppressing branch; asserting makes the premise explicit without
      reaching into unrelated tests. Original entry:
      **`89ec9a397` — `should_suppress_during_recovery` has zero references
      anywhere in this fork.** Not ported, and not stubbed either, so it is
      neither a partial nor a working divergence — the concept simply does not
      exist here. Needs a look at what upstream suppresses during recovery and
      whether this fork has an equivalent path that should be doing it. Low
      priority, but genuinely unexamined rather than decided.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — **"The concept simply does not exist here" is false.** The symbol is absent, but the behaviour is present and tested: `view_util.rs:113` returns `None` when `error.will_attempt_resume()`, covered by `agent_block_tests.rs:211`. The real delta is narrow — the pin gates suppression on `!ChannelState::channel().is_dogfood()` (`42effe840:app/src/ai/agent/mod.rs:791`) so Local/Dev builds still see the raw error; the fork suppresses unconditionally. Rescope to that carve-out.
      **KEEP-SIZED 2026-08-17 (audit-debt triage) — refutation re-verified end to
      end; size is 3 edits and the impact is developer ergonomics only.** Pin
      condition quoted verbatim from `42effe840:app/src/ai/agent/mod.rs:791`:
      `self.will_attempt_resume() && !ChannelState::channel().is_dogfood()`, with
      the pin's own doc comment at `:786-789` — "Release builds stay quiet …
      dogfood builds (Local/Dev) keep the old, more aggressive behavior so
      developers still see every transport failure." Fork side confirmed:
      unconditional `if error.will_attempt_resume() { return None; }` at
      `app/src/ai/blocklist/view_util.rs:113`; `will_attempt_resume()` exists at
      `app/src/ai/agent/mod.rs:729`; the test at
      `crates/warp_tui/src/agent_block_tests.rs:211` is
      `agent_block_suppresses_recovery_pending_failure()`. **Nothing blocks the
      port** — `is_dogfood` already exists here at
      `crates/warp_core/src/channel/mod.rs:30` and is already used at
      `app/src/input_classifier.rs:58` and `app/src/lib.rs:1327`.

      **Size: 3 edits.** (1) add `should_suppress_during_recovery` next to
      `will_attempt_resume` in `app/src/ai/agent/mod.rs`; (2) swap the call at
      `view_util.rs:113`; (3) pin a non-dogfood channel in the existing test. The
      pin's other call sites (`block/cli.rs:1199`, `block/view_impl/output.rs:1216`,
      `view_util.rs:182`) have **no fork counterpart** — the fork's
      `should_show_failed_output_usage_notice` (`view_util.rs:166-173`) is a
      deliberate BYOP `-> false` stub — so they need no change.
      **Release relevance: none.** On Release/Preview the fork and pin behave
      identically; they differ only on Local/Dev builds, where the pin shows the
      transient error and the fork hides it. Post-release.

### remote_server + core crates (278 commits triaged, ~35 hand-verified, 0 partials)

- [x] **The daemon's writer loop is stuck pre-fix — two upstream commits, both
      100% absent, on the same function.** `d9dee18e1` ("Fix daemon message too
      big error") adds an `is_write_recoverable()` gate to the daemon's own write
      loop plus `RunCommand` output-size truncation in `server_model.rs`.
      **COORDINATOR-VERIFIED:** `is_write_recoverable` exists at
      `crates/remote_server/src/protocol.rs:174` and is referenced by exactly one
      call site, `client/mod.rs:1139` — the client. `app/src/remote_server/unix/mod.rs`'s
      writer loop never consults it, so *any* write error, including a recoverable
      "message too big", tears down the entire connection. `363d1d6e9`
      ("Downgrade remote server SSH disconnect errors") is the sibling: routine
      broken-pipe/connection-reset on client disconnect still logs at
      `log::error!` rather than `log::warn!`. Highest-severity item in this batch.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Both halves are implemented: `app/src/remote_server/unix/mod.rs:202` gates writer-loop errors on `is_write_recoverable()`, and `server_model.rs:3667-3683` truncates `RunCommand` output. **Caveat:** that code is annotated in-source *"NOT COMPILED: verified by reading only"* — ported, not proven. A compile/test pass is still owed.

- [x] **`97a9ff5f` — wasm debug panic in `StandardizedPath::from_local_absolute_unchecked`.**
      `debug_assert!(path.is_absolute())` uses std's `Path::is_absolute()`, which
      returns `false` for Unix-rooted paths on `wasm32-unknown-unknown`. This fork
      still targets wasm32 (lsp, ai, editor, code_review, terminal/view). Upstream's
      fix is a same-file swap to the encoding-aware `typed_path` check. Low risk,
      genuinely broken today.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Fixed in place: `crates/warp_util/src/standardized_path.rs:97` asserts on `normalized.is_absolute()` (typed_path, encoding-aware), and `:87-96` cites upstream `97a9ff5f…(#13552)` as the applied fix.

- [x] **BUILT 2026-08-18 — remote git status AND PR context now push to the client.
      UNVERIFIED (not compiled), and this is the largest single change in the tree.**
      **Stage 1, the prerequisite:** `GitRepoStatusModel` is now an enum `{Local,
      Remote}` (`git_status_update.rs:243`), the old struct renamed
      `LocalGitRepoStatusModel`, and both factory maps re-keyed to
      `HashMap<LocalOrRemotePath, _>`. New `git_status_update_remote.rs` (132 lines)
      ported from the pin, filed as a sibling `*_remote.rs` matching the fork's own
      `diff_state.rs`/`diff_state_remote.rs` shape rather than the pin's directory.
      **Stage 2, proto — nothing existing renumbered.** `Notification` took the pin's
      own free tags **8/9/10**; `ServerMessage` took **39/40/41** (pin uses 34/35/36,
      taken here). The `discard_files_response`-must-not-move reasoning — `Channel::Oss`
      skips the version check, so a stale daemon would decode a moved tag as the wrong
      message — is written into the proto at `:206-217` so it survives this session.
      **`rebased` was preserved, not dropped:** added as fork-original
      `bool tracking_rebased = 8`; proto3 defaults it false from an older daemon,
      exactly the pre-port behaviour. Round-trip test pins it.
      **Stages 3-5:** three `ClientEvent`/`RemoteServerManagerEvent` variants, new
      `app/src/remote_server/git_status_proto.rs` (+6 tests), daemon producers in
      `server_model.rs` (+333), and the PR chip wired via a new
      `current_remote_repo_path` + `current_repo_location()` in `terminal/view.rs`.
      `needs_pr_info_for_agent_context` now uses the location — **without that the
      agent still got nothing on SSH**, which was the point.
      **Two daemon-bootstrap defects found and fixed en route, not in the scoping:**
      (1) `GitStatusUpdateModel` was never registered as a daemon singleton —
      `GitRepoModels::handle(ctx)` would have **panicked**. (2) `LocalGitHubRepoModel`
      reaches for `LocalShellState` and `SessionSettings`, neither of which the
      headless daemon registers; both are now `has_singleton_model`-guarded rather
      than dragging the whole settings stack into the daemon.
      **Deviations from the pin, deliberate:** a second `current_remote_repo_path`
      field instead of widening `current_repo_path` to `LocalOrRemotePath` (that type
      change ripples into ~14 sites plus panel/pane-group/telemetry payloads, all
      local-only, unverifiable without compiling); `RemoteServerManagerEvent` carries
      `host_id` + the raw proto push rather than the pin's `RemotePath`, matching what
      the codebase-index events already do; and the fork's single
      `#[cfg(feature = "local_fs")]` gate is kept over the pin's per-variant split,
      which avoids the pin's enum-with-no-live-variants problem on wasm.
      **`current_repo_location()` prefers Remote over Local deliberately** — the local
      detection spawn assigns `current_repo_path` before checking
      `active_session_path_if_local`, so on SSH it can land on a coincidentally
      same-named local repo.
      **Not done:** client-side round-trip tests for the three new pushes
      (`server_model_tests::test_model()` has no `ModelContext`, so the daemon
      producers need a harness that does not exist here), and
      `current_remote_repo_path` is never cleared on session teardown — stale but
      harmless, and it matches the pin's "disconnect preserves stale data".
      Original entry:
      **RESCOPED 2026-08-17 (investigation only, nothing implemented). Four of
      this entry's claims were wrong; the port is BLOCKED on a prerequisite it
      never mentioned.**

      **1. The 'fork already reused tag 34' framing is NOT a defect.** Fork
      `remote_server.proto:178` has `DiscardFilesResponse = 34`; the pin has
      `GitStatusPush = 34`. But **35, 36, 37 and 38 are all taken here too**, and
      the fork's `ServerMessage` numbering diverged from the pin's across tags
      18-38 **by design**, documented in-file (`// The pin numbers this 3; taken
      here by session_bootstrapped`, the `reserved 2, 3, 4;` block in `Initialize`).
      The two protos were never the same wire format.
      **Do NOT move `discard_files_response`.** `should_enforce_remote_version_check`
      (`manager.rs:234`) returns **false for `Channel::Oss`** — this fork — so a
      client can talk to a stale daemon with no version gate. Moving a tag that
      deployed daemons already speak is the ONLY direction that corrupts: the
      daemon would emit `DiscardFilesResponse` on 34 and a reassigned client would
      decode it as `GitStatusPush`, silently, both being message-typed. Adding new
      tags is inert under the same mismatch. **Take 39/40/41** for the three
      pushes (39 is the next free slot); the client->server `Notification` triggers
      can take the pin's own **8/9/10**, which are free here.

      **2. '10 files, ~59 sites' -> 12 files, 114 sites.** A naive grep inflates by
      14 on `update_git_status_subscription`, an unrelated pre-existing local method.
      The '2 new files, 234 lines' figure is exactly right (148 + 86).

      **3. 'Chips go stale until manual refresh' is wrong for BOTH halves, in
      opposite directions.** *Git status on remote WORKS and refreshes every
      prompt* via `builtins::shell_git_branch_status()` (`context_chips/builtins.rs:176`)
      running on the remote host — the push port is an upgrade (structured data +
      watcher ticks instead of per-prompt shell re-execution), not a repair.
      *PR context on remote is ENTIRELY ABSENT*: `ContextChipKind::GithubPullRequest`
      is declared with generator `|_| None` (`context_chips/mod.rs:332-336`) and its
      feeder `GitHubRepoModel` is `Local(..)` only, keyed on a local `PathBuf`. On
      SSH `sync_pr_info_subscription` logs a failure and the chip stays empty
      forever. This also starves the agent of PR context on SSH.

      **4. The missing prerequisite that dwarfs the site count.** The pin's
      `GitRepoStatusModel` is an **enum** `{Local, Remote}` keyed by
      `LocalOrRemotePath`; the fork's is a plain **struct** and its factory is a
      `HashMap<PathBuf, _>` hard-wired to `DetectedRepositories`. A `Remote` variant
      needs struct->enum, `PathBuf`->`LocalOrRemotePath` on both factory maps
      (`git_status_update.rs`, 507 lines), a remote-aware `current_repo_path` in
      `terminal/view.rs:2705`, and consequent type changes at ~10 consumers across
      five subsystems. **Without it the push receivers are unreachable dead code.**
      `LocalOrRemotePath` and `RemotePath` already exist here, so the types are
      available.

      **5. `diff_state.proto` not existing here is a NON-ISSUE** — the fork keeps
      one proto file; only `RepositoryInfo` and `GitStatusMetadata` are new messages
      (`PrInfo` and `DiffStats` already exist).

      **6. NO `DECLINED.md` row is warranted — BOTH halves pass the de-clouding
      test.** The forge integration is **`gh`, a local CLI**, not a backend: the pin
      documents `RepositoryInfo` as 'Returned by `gh repo view`' and its
      `github_repo_model/local.rs` imports only `crate::util::git::{..}` — no
      `warp_graphql`, no `ServerApiProvider`, no HTTP. The fork already ships the
      entire local half (`PrInfo`/`RepositoryInfo` in `util/git.rs:1095-1110`,
      `LocalGitHubRepoModel`, 392 lines), and `github_repo_model/mod.rs:6-14`
      explicitly invites the `Remote` variant.

      Two shape mismatches for whoever does it: the fork's `GitStatusMetadata`
      carries a fork-original `rebased: bool` the pin's flat proto has no field for
      (remote sessions would silently lose the rebased indicator), and it is
      `#[cfg(feature = "local_fs")]`-gated, which the remote receiver needs
      reconciled. Original entry:
      **Remote git-chip / PR-context proto plumbing appears entirely absent.**
      Zero hits under `app/src/remote_server/` or `crates/remote_server/proto/` for
      `RepositoryInfo`, `GitBranchStatus` or PR-url fields, while the local-session
      equivalent (`app/src/util/git.rs`) is present and current. Traced across
      `3ed3ae1d`, `8c63aaf9b`, `dbaf6d50`, `90f7a4c8`. **Consequence, and why it
      belongs next to the legacy-SSH section below: on a remote/SSH session the
      agent gets materially less git and PR context than it does locally.** Not in
      `DECLINED.md`. Sampled, not exhaustively verified (`bbdc5a2ea`, `08487819f`,
      `856c74b0` unchecked) — needs a scope call, not an assertion.
      **RESCOPE 2026-08-17 (v0.1.0 agent audit)** — the blanket "entirely absent" claim is false: `remote_server.proto` carries `GetBranches`/`BranchInfo` (`:828-856`), `GitCreatePr*` (`:977-993`), `PrInfo.url` (`:1153-1159`) and a DiffState subscription. The literal names `RepositoryInfo`/`GitBranchStatus` are local-session types, not remote ones. Which specific fields remain missing was NOT established, and equivalence to the pin was not checked. Re-derive the real delta rather than treating this as a blanket gap.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Narrower than written. The request-response half is fully wired and tested: `GetBranches`/`BranchInfo`, `GitCreatePr*`, `PrInfo.url`, `GitCommitChain`, `GitPush` in `remote_server.proto:828-1170`, with round-trip tests at `client_tests.rs:611-791`. The real gap is admitted in-source at `codebase_index_model.rs:571-578`: the fork's `RemoteServerManagerEvent` lacks the pin's live-PUSH variants (`GitStatusPushReceived`, `GitHubPrInfoPushReceived`, `GitHubRepositoryInfoPushReceived`), so remote git/PR chips go stale until a manual refresh instead of updating live. Rescope to that.
      **KEEP-SIZED 2026-08-17 (audit-debt triage) — the rescoped claim is TRUE and
      the gap is deeper than "event-level". Size: 10 files, ~59 symbol sites, 2
      new files totalling 234 lines, plus a proto field-number collision.**
      Verified on both sides. Fork enum at
      `crates/remote_server/src/manager.rs:329`; the exhaustive `session_id()`
      match at `:557-600` confirms all three variants are absent, and a tree-wide
      grep returns **zero code hits** for the three names — only prose
      (`TODO.md`, `app/src/remote_server/codebase_index_model.rs:573-575`,
      `app/src/code_review/github_repo_model/mod.rs:7-8`). Pin has them at
      `42effe840:crates/remote_server/src/manager.rs:599,606,613` and
      `client/mod.rs:137,142,147`.

      **The proto half is missing too**, which the earlier rescope did not catch.
      The fork's `ServerMessage` oneof (`remote_server.proto:143-196`, tags 2-38)
      has no `git_status_push` / `github_pr_info_push` /
      `github_repository_info_push`; the pin has them at
      `42effe840:crates/remote_server/proto/remote_server.proto:123,124,125` on
      tags 34/35/36 — and **the fork has already reused tag 34** for
      `discard_files_response`, so a port needs renumbering. The client→server
      triggers `update_git_status = 8` / `update_github_pr_info = 9`
      (`42effe840:…:82-83`) are also absent, and the message bodies live in
      `42effe840:crates/remote_server/proto/diff_state.proto:476,490,496` — **a
      file this fork does not have at all** (`crates/remote_server/proto/` holds
      only `remote_server.proto`).

      Port surface: 3 proto messages + 3 oneof fields + 2 request messages; 3
      `ClientEvent` and 3 manager variants; ~6 emit/decode sites; 4 trivial
      exhaustive-match arms (`terminal/model/session.rs`, `terminal/view.rs`,
      `writeable_pty/remote_server_controller.rs`, `codebase_index_model.rs`); a
      daemon-side producer in `app/src/remote_server/server_model.rs`; and two
      receiver files absent here — `app/src/code_review/git_repo_model/remote.rs`
      (86 lines; the `git_repo_model/` directory does not exist) and
      `github_repo_model/remote.rs` (148 lines). **Medium port, not a one-liner.**
      **Release relevance: no.** The degradation is "remote git/PR chips go stale
      until manual refresh on an SSH session", not a broken feature. Post-release.

- [x] **`f0ca7861` — `capture_exit_status` races.** `crates/remote_server/src/manager.rs`
      still uses synchronous `child.try_status()`, which returns `None` on a
      just-killed child, instead of upstream's async `.status()` + 200ms timeout.
      Diagnostic accuracy only.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Fixed: `crates/remote_server/src/manager.rs:1841` defines `async fn await_exit_status` using `child.status().with_timeout(...)`, citing upstream `f0ca7861f (#10728)`; `AwaitingExitStatus` is wired at `:294-319`. **Same caveat:** annotated *"NOT COMPILED: verified by reading only"*.

- [x] **DECLINED 2026-08-17 (maintainer): `ae69bd4c` — no tarball cache for the SCP
      fallback.** The fork re-downloads on every install; no
      `remote_server_artifact_version()`. Declined outright — a caching layer for
      remote-server artifacts is infrastructure this fork does not want to own.
      See `DECLINED.md`. Reopen only if repeat installs become a real complaint.

- [x] **DECLINE 2026-08-17 (audit-debt triage) — out of scope: the entire path
      terminates in a compiled-out no-op, one symbol name in the entry is wrong,
      and the "per-callsite footgun" never accumulated here.** Three measured
      corrections:

      1. **`send_tracked_request` does not exist at the pin either.**
         `git grep -n 'fn send_tracked_request' 42effe840` → zero. The real
         symbols are `send_request` / `send_request_internal`
         (`42effe840:crates/remote_server/src/client/mod.rs:833-849`); the only
         occurrence of the name is a stale comment at
         `42effe840:crates/remote_server/src/manager.rs:3198`. `RequestFailedEvent`
         (`:168`) and `failure_tx`/`failure_rx` (`:210`, `:290`, `:842`;
         `manager.rs:79`, `:3666-3680`) are real and are genuinely absent here
         (tree-wide grep → zero hits).
      2. **The footgun is not present.** The fork has **2** ad-hoc emit sites, not
         many — `crates/remote_server/src/manager.rs:1539` and `:1634`. The pin
         has **2** as well (`42effe840:…manager.rs:2662`, `:3673`, the latter being
         the drain itself). Upstream's refactor consolidated N call sites this
         fork never grew. Everything else here is passive: the variant def
         (`manager.rs:539`), one `session_id()` arm (`:563`) and four consumer
         arms.
      3. **The sink is dead, not "mostly" dead.** The only UI consumer,
         `app/src/terminal/view.rs:4282`, forwards to
         `send_telemetry_from_ctx!(TelemetryEvent::RemoteServerClientRequestError…)`
         at `:4297-4298`, and that macro is `if false { … }` in
         `crates/warp_core/src/telemetry.rs:9-16`. `app/src/lib.rs:305` says so in
         as many words. Nothing observable depends on this.

      Upstream `44d87708385a22024c20030c0587d4136c09a083` (2026-05-13, #10828) is
      **9 files, +341/−83**, and three of the nine are telemetry-event files this
      fork has gutted. Declined as telemetry plumbing with no live sink. **Reopen
      only if telemetry is ever revived**, at which point the port is: split
      `send_request`/`send_request_internal`, add an `operation` argument to the
      ~25 `send_request(` call sites in `crates/remote_server/src/client/mod.rs`,
      thread a fourth channel through `transport.rs` + `ssh_transport.rs`, and
      delete the 2 hand-rolled emits.
      Original finding:
      **`44d87708` — reliability tracking.** `send_tracked_request` / `failure_rx` /
      `RequestFailedEvent` absent; the fork still uses the ad-hoc
      `ClientRequestFailed`-per-callsite pattern that upstream's commit message
      calls a footgun. Mostly moot while telemetry is dead, but the pattern is
      live debt.

- [x] **RECONCILED 2026-08-18 — all three commits accounted for.** `ebedb9fd` is ported
      (`crates/warp_terminal/src/shell/mod.rs:644-648`, regression test
      `mod_tests.rs:371`, which also asserts the pre-fix pipeline shape is gone and the
      Unix shells stay clean). The entry's own 2026-08-17 refutation — *"`shell/mod.rs:635`
      still has the pre-fix line"* — is **no longer true as of today's port**.
      `ef4b562191`/`eab3b3fa9` remain a **deliberate partial port**, reasoning re-verified
      link by link today, and now recorded in `DECLINED.md` so it stops being
      re-discovered — it had been, twice, because the decision lived only in a code comment. Original entry:
      **Windows PowerShell, low priority.** `ebedb9fd` (localized / non-UTF8
      executable-name mojibake) and `ef4b562191` / `eab3b3fa9` (deferred cmdlet and
      function-name loading, perf) — none ported in
      `crates/warp_terminal/src/shell/mod.rs`.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Two of the three commits ARE ported. `ef4b562191`'s `shell_command_to_get_all_builtins` and `eab3b3fa9`'s `shell_command_to_get_all_functions` are present verbatim at `crates/warp_terminal/src/shell/mod.rs:757-793` and wired at `session.rs:1165,1190`. Only `ebedb9fd` is genuinely missing: `shell/mod.rs:635` still has the pre-fix `Get-Command -CommandType Application` line with no UTF8Encoding wrapper, so localized non-UTF-8 executable names still corrupt the list. Rescope to `ebedb9fd` alone.

- [x] **`a792340801` and `5b047fc2` — record in `DECLINED.md`, do not port.**
      Sentry init and daemon client-ID forwarding, both cloud telemetry. Correctly
      unported, but not named in `DECLINED.md`, so they keep reading as gaps. A
      one-line row each stops the re-discovery.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Already done: `DECLINED.md:241` names both `a792340801` and `5b047fc2` under the 2026-08-15 partial-port audit.

**False positives this agent rejected (do not re-raise):** `5ea50d236` (documented
divergence — `SkillManagerEvent::InventoryChanged` is a safe superset),
`d775c92` (shipped under renamed symbols), `14c8c8de` (28-file host-scoped-request
refactor, verified present throughout), `f49457b2` (ported via `RemoteServerLog`),
plus `18baecd45`, `844dc2cec`, `e75b31553`, `99f80df4d`, `388f5dc1`, `1df6ff13`,
`fb3cb0e9b`, `b8e86f34`, `af5b45b6d`, `a9cb364a5`, `e6098a8a`.

## LEGACY-SSH REGRESSIONS AGAINST THE PIN (opened 2026-08-15)

Three things the fork dropped from the pin on the legacy-SSH path, found while
chasing a maintainer report of OpenSSH printing `channel N: open failed: connect
failed: open failed` into a live SSH session after choosing **Skip** on the
remote-server install prompt.

**ROOT CAUSE FOUND 2026-08-15 — it is none of the three below.** The storm is
`SessionContext::cached_directory_entries` (`app/src/completer/mod.rs`) being a bare
`DashMap` rather than `Arc<DashMap>`. `SessionContext` is `Clone` and the context-chip
layer clones it per chip (`context_chips/display.rs:312`) and per `DirectoryFetcher`
(`context_chips/display_chip.rs:935`); `DashMap::clone` deep-copies, so every clone gets a
private cache that starts empty and whose inserts nobody else sees. On a legacy-SSH session
each miss shells out to `find` over the ControlMaster, so each clone opens its **own SSH
channel for the same directory** — measured at three identical
`find . -maxdepth 1` in flight for one directory, plus `compgen -c` and two `git branch`
callers, peak 5 and climbing. Past the host's `MaxSessions` sshd refuses and OpenSSH prints
the error into the live shell. Upstream carries the `Arc` (pin `02b53fcd8`); this fork
descends from `0dbd3d56` which predates that fix, so it is an **unported upstream change,
not a fork regression**. Fixed by restoring the `Arc`. Skip, the remote server and the tmux
wrapper were all red herrings — Skip only routes you onto the path where the missing `Arc`
becomes visible.

**The three items below remain open and are still worth doing, but note:** *none of them
is the cause of that error.* They were found by diffing the fork's SSH path
against the pin (`02b53fcd8`) and are real regressions on their own merits, but
the causal chain to the channel storm is **still open**. The reported string is
sshd's `MaxSessions` refusal (it answers `CONNECT_FAILED` with the message text
`open failed`, which is why the phrase doubles), so something opened more than
the host's session budget concurrently — and every caller audited so far
(`execute.rs::action_phase`, the completer's two argument engines) correctly
serializes via `supports_parallel_command_execution()`. The unmeasured
alternative is that the remote is configured below the default `MaxSessions 10`,
in which case the pin would fail there identically and none of this is a fork
regression at all. Measure before attributing:

```
# local (phosphor's side) — do the ssh children pile up?
watch -n0.5 'pgrep -fa "ssh -q -o PasswordAuthentication=no" | wc -l'
# on the remote — what is the actual budget?
grep -ri maxsessions /etc/ssh/sshd_config /etc/ssh/sshd_config.d/ 2>/dev/null
```

Checked and found at PARITY with the pin, so do not re-derive these: the
`SshRemoteServer` / `InBandGeneratorsForSSH` flag-list membership, the
`are_in_band_generators_for_all_sessions_enabled` default (`false` both sides),
`handle_ssh_remote_server_skip`, executor selection in `command_executor.rs`,
the `SshInitState` machine (linear, no retry loop), and the fact that a wrapper
session never transitions back out of `IsLegacySSHSession::Yes`. The fork's
`SSHTmuxWrapper` is on by default, but `use_ssh_tmux_wrapper` defaults `false`,
which reduces the fork's gate to the pin's expression exactly.

**Correction 2026-08-15: `SSHTmuxWrapper` is NOT fork-original**, as this entry
first claimed. It exists at `0dbd3d56`; upstream *deleted* it in `57062bd9`
(#12478, 2026-06-12) — "Fully removes the legacy tmux-based SSH warpification
flow in favor of the remote-server SSH extension", ~6.3k lines plus a one-time
deprecation banner for opted-in users. The fork retained code upstream
abandoned. Do not treat `use_ssh_tmux_wrapper = true` as a supported escape
hatch: it is a path with no upstream future, and turning it on also drops the
session out of `IsLegacySSHSession::Yes`, which un-withdraws `read_files` /
`apply_file_diffs` / `read_skill` onto a host that cannot serve them.

- [x] **Port `1b65a8b9` (#14746), "Follow symlinks in Tab path completion
      (remote/SSH sessions)".** DONE 2026-08-15. Surfaced by the same
      `git log warp/master -- app/src/completer/mod.rs` that found the root cause
      above. `-L` added to both `find` invocations in `ls_script_for_dir`, plus
      upstream's test. **UNRUN** — ported under the build suspension, verified
      only by reading and `rustfmt --check`.

      That log walk found **three** unported upstream fixes in this one file, of
      which two are now taken (`01778efe`, `1b65a8b9`). Worth walking
      `git log warp/master -- <path>` for the other hot files on the SSH path
      before assuming any of them is at parity.

- [x] **IMPLEMENTED 2026-08-17 (A-D). UNVERIFIED — nothing compiled; CI or a
      local build is the first real check.** The entry below was also
      **OVERSTATED**: ~119 of upstream's 458 lines were already here
      (`a13b9d491`/`aaa436aad`), and the consumer half was complete but DEAD —
      `owns_control_master` could never be `false` because no wrapper emitted
      `external_control_master`, so `#[serde(default)]` fired unconditionally.
      **What landed:** `ssh -G` discovery + `ssh -O check` liveness probe in all
      three wrappers (`zsh_body.sh:904-937`, `bash_body.sh:1003-1036`,
      `fish.sh:626-654`), hook JSON now carries `external_control_master`, and
      the hardcoded `ControlMaster=yes` / `$SSH_SOCKET_DIR/$WARP_SESSION_ID` is
      replaced by the computed `$control_master_mode` / `$control_path`.
      Setting at `app/src/settings/ssh.rs:20` (`warpify.ssh.reuse_existing_control_master`,
      **default `false`**, `surface:` dropped — this fork removed `SettingSurfaces`).
      Env plumbing: `PtyOptions.reuse_ssh_control_master`, computed in
      `terminal_manager.rs:665`, exported as `WARP_SSH_REUSE_CONTROL_MASTER` at
      both `unix.rs` sites (host shell + docker sandbox), Windows, and the WSL
      allowlist. Two new `unix_tests.rs` tests assert the var is ALWAYS exported
      as `"0"`/`"1"`, so an unset value can never read as enabled.
      The pin's two DCS parse tests already existed here (`ansi/mod_test.rs:530,557`).
      **`~/.ssh/config` is never opened, parsed or written** — the wrapper shells
      out to `ssh -G` (ssh's own resolver) and reads one line, preserving the
      `DECLINED.md` "the system owns SSH" property. The `WARP_SESSION_ID`
      mint sites (#532) were deliberately not touched; verified unmodified.
      **STILL OPEN FOR THE MAINTAINER: default `false` vs `true`.** Kept at
      upstream's `false`. The argument for `true` is that creating a *second*
      master is the imposition, which is the same reasoning as the standing SSH
      decision. One-word change at `app/src/settings/ssh.rs:22`.
      **Known shell divergence (upstream's, not introduced here):** bash/zsh use
      `[:alnum:]`, which accepts non-ASCII letters under a UTF-8 locale, where
      fish's `[A-Za-z0-9._/~@:+,-]` rejects them. Not a safety hole (the path is
      double-quoted at every use) but the shells differ.
      Original entry:
      **Port `reuse_ssh_control_master` (upstream `0d24d2cf`, "Add setting to
      enable reusing user's existing control master", #12465).**
      **Correction 2026-08-15: the fork did not "drop" this**, as this entry
      first said — it was ADDED upstream after the `0dbd3d56` fork point and was
      never ported. Same end state, different story, and it means the whole
      feature has to be brought across rather than resurrected from history.
      The pin discovers an existing
      ControlMaster for the destination host (`ssh -G`, verified with
      `ssh -O check`) and attaches to it instead of always creating its own;
      it threads the decision through `PtyOptions` (pin
      `app/src/terminal/local_tty/mod.rs:110`) from
      `terminal_manager.rs:794`. The fork has **no trace of the setting or the
      field** — `app/src/terminal/local_tty/mod.rs:85-99` has neither.
      Consequence beyond the missing feature: this is why the bootstrap scripts
      never emit `external_control_master`, so the ownership signal that
      `a13b9d49` built the `owns_control_master` guard for is dead by
      construction and **#37 cannot be finished until this is ported**. That
      commit's TODO reads as unplumbed wiring; it is actually a feature this
      fork never took.

- [x] **FIXED 2026-08-17. **UNVERIFIED — rustfmt/shell-parse only, not compiled.** Premise was correct and the cost was WORSE than stated: per prompt the fork
      spawned `node --version` uncached **plus** up to `depth($PWD)` `dirname`
      processes for the package.json walk and more for the `.git` walk — and those
      ran **even in non-Node directories**, because the walk preceded the test.
      Ported verbatim from the pin: cache keyed on `"$PWD:$PATH"` (PATH included
      because `nvm use` changes the resolved binary without changing directory),
      globals `_WARP_NODE_VERSION_CACHE_KEY`/`_VALUE`, and `${current_dir%/*}`
      parameter expansion replacing `dirname` entirely. Gate is
      `WARP_PROMPT_NODE_VERSION_ENABLED` from a new
      `PtyOptions::node_version_chip_enabled`, computed in `create_pty` from the
      chip's presence in the prompt OR agent footer OR CLI-agent footer.
      **A FOURTH shell the three-row table missed: `pwsh.ps1` gates and caches too.**
      SSH matches the pin — the gate is not forwarded, so remote sessions default to
      enabled and are saved by the cache alone.
      **Flagged for review:** the new `PtyOptions` field has no `#[serde(default)]`
      (matching the pin, though its sibling `reuse_ssh_control_master` has one), and
      `PtyOptions` crosses the local_tty server IPC — a mixed-version client/server
      would fail to deserialize. The `windows/environment.rs` edits are `cfg(windows)`
      and get no compiler check until a Windows build.** Original entry:
      **Restore the `node --version` per-prompt cache, and the chip gate.** The
      pin caches the resolved version keyed on `"$PWD:$PATH"` in globals that
      persist across `precmd`, so `node --version` is only spawned when the
      directory or PATH changes (`nvm use`) — present in all three bootstrap
      shells at the pin, absent from all three in the fork:
      | file | pin | fork |
      |---|---|---|
      | `app/assets/bundled/bootstrap/bash_body.sh` | cached (`_WARP_NODE_VERSION_CACHE_KEY`) | spawns every prompt (`:611`) |
      | `app/assets/bundled/bootstrap/zsh_body.sh` | cached | spawns every prompt |
      | `app/assets/bundled/bootstrap/fish.sh` | cached | spawns every prompt |
      The pin *also* gates the whole detection per session via
      `PtyOptions::node_version_chip_enabled` → `WARP_PROMPT_NODE_VERSION_ENABLED`
      (pin `local_tty/mod.rs:116`); the fork dropped that field too, so there is
      no way to turn it off. In a git repo over SSH this is one remote
      subprocess per prompt that the pin does not pay. It burns remote CPU
      in-band and does **not** open an SSH channel, so it is a latency
      regression, not the channel bug.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Confirmed; line reference imprecise. The pin gates on `WARP_PROMPT_NODE_VERSION_ENABLED` (`42effe840:bash_body.sh:586`) and caches on `$PWD:$PATH` (`:617-630`); the fork has neither in any of the three shells. Cite the block, not `:611` — the actual call is ~`:623`.

- [x] **Escape single quotes in the ControlMaster executor's `cd`. DONE
      2026-08-15 — and this was a SECURITY fix, not the cosmetic quoting bug
      this entry first called it.** `remote_command_executor.rs` built
      `cd '{current_directory_path}' &&` by interpolation, so a remote directory
      whose name contains `'` closes the quoting and the remainder is
      interpreted as shell syntax — injected into a command that runs on the
      REMOTE host.

      Upstream fixed this in `88c344e2`, "[Security] Fix command injection in
      remote ssh sessions" (#25354, 2026-05-05), at **two** sites. This fork
      ported one and missed the other: `session.rs:1409`'s
      `cat '{escaped_history_file}'` is present, the executor's `cwd` was not.
      The helper (`shared.rs:27`) and its seven tests were already here — only
      this call site was missing. Fixed by applying it, matching upstream's
      change exactly.

      Upstream added no executor-level test (its tests went to `shared_tests.rs`
      and `session_tests.rs`), because the command string is built inline in an
      async fn that spawns a process. Not added here either; the helper's own
      coverage carries it.

      **Its sibling `43f4f483`, "[Security] Fix command injection in code search
      tools" (#25351), IS fully ported** — `shell_quote_arg` is present and used
      throughout `grep.rs` and `file_glob.rs`. Checked, no action.

## REMOTE-SERVER DISTRIBUTION AND BINARY SIZE (opened 2026-08-11)

Two items with one root cause, raised by the maintainer after the SSH-install
supply-chain fix (`a4ebf6876`).

### The root cause: the daemon is the whole application

`app/Cargo.toml` declares one real bin, `zap-oss`.
`LaunchMode::RemoteServerDaemon` is a **mode of that same binary**, so the
"CLI"/remote-server artifact is the entire app. The release workflow copies it
verbatim (`.github/workflows/phosphor_release.yml:671`):

```
cp "${bundle_cli.binary_path}" zap-oss
tar czf phosphor-cli-linux-x86_64.tar.gz zap-oss resources
```

A remote host that only needs file reads, git ops and ripgrep therefore
receives the wgpu renderer, font rasterisation, the terminal emulator,
tree-sitter grammars, the AI stack, LSP and vendored libgit2.

**It is not debug bloat — that was checked and ruled out.** The CLI artifact
builds under `[profile.release-cli]` (`Cargo.toml:522`: inherits `release-lto`
thin LTO, `opt-level = "s"`, `codegen-units = 1`), and
`script/linux/bundle:285` runs `strip --strip-all` on production builds. The
`debug = 1` in `[profile.release]` is stripped back out. Published tarballs:
linux-x86_64 ~45 MB, macos-aarch64 ~52 MB, macos-x86_64 ~52 MB (~149 MB for all
three, which is what kills the bundling option below).

- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: no longer required.** This was explicitly "the enabling work for the
      distribution decision below" — push-based install and in-package bundling both
      needed a small daemon to be tolerable. With the fetch retained and
      SHA-verified, neither is being built, so the size floor stops mattering.
      Reopen only if the distribution decision is ever revisited.** Original entry:
      **Build a feature-reduced daemon target.** Feature-gate the app crate so
      the `RemoteServerDaemon` path compiles without the renderer and the GUI
      stack, and ship *that* as `phosphor-cli`. Plausibly single-digit MB.
      This is the enabling work for the distribution decision below — a small
      daemon makes push-based install and in-package bundling both cheap.
      Not started; the crate is not currently structured for it.

### The distribution decision — NEEDS A MAINTAINER CALL

- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: keep the fetch; the supply-chain objection is ANSWERED by the SHA checks that
      have since landed, and pulling from an official release tag is acceptable.**
      Verified in code before closing: `expected_sha256()`
      (`crates/remote_server/src/setup.rs:529`) reads digests from
      `option_env!("PHOSPHOR_CLI_SHA256_*")` — **compiled into the client at build
      time**, not published next to the tarball. The doc comment reasons about
      exactly the attack: *"anyone able to replace the tarball can replace a
      checksum file next to it. A digest compiled into the client instead reaches
      the remote host down the user's already-authenticated SSH channel... That
      takes GitHub out of the integrity path entirely."* An empty digest (env var
      unset at compile time) is **fail-closed** — the script refuses to install
      rather than warning and continuing. `download_url()` (`:566`) pins to the
      release tag via `ChannelState::app_version()`.
      `download_url()`, `RELEASE_ASSET_PREFIX` and the `curl` therefore STAY.** Original entry:
      **Decide how the remote daemon reaches the host, and delete the fetch
      path.** Maintainer's stated position: *"can't we just push it from the
      terminal? This sounds like a supply chain attack waiting to happen"* —
      preference is **push, or a manual install provided as part of the
      package.** Options:
      1. **Push over the existing SSH channel** — no network egress from the
         remote host, no GitHub dependency, no trust in a release asset. Costs
         one upload per host per version; needs the small daemon to be tolerable.
      2. **Bundle all platforms in the package** — ~149 MB at current sizes.
         Only viable after the feature-reduced daemon lands.
      3. **Keep the fetch** — status quo, and the thing being objected to.
      Current code (`crates/remote_server/src/setup.rs:504-541`,
      `install_remote_server.sh:57`) does (3): the remote host curls
      `https://github.com/jwp2987/phosphor/releases/...`.
      **`a4ebf6876` only fixed *which* repo it fetches** (it was pointed at a
      different project's releases and 404'd) — it did **not** address the
      objection. The fetch path, `download_url()`, `RELEASE_ASSET_PREFIX` and
      the `curl` in `install_remote_server.sh` all come out once (1) or (2) is
      chosen.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Two corrections. (1) `90cae8c9f` (2026-08-13) bakes a build-time SHA-256 per platform into the install script, delivered over the authenticated SSH channel and fail-closing when absent — GitHub is no longer in the integrity path, which the entry never mentions. (2) SCP push is not hypothetical: `scp_install_fallback` (`ssh_transport.rs:570`) is already the PRIMARY path for dev-source builds and the fallback for release builds. The work is promoting it to primary, not building it. Line refs have drifted: `setup.rs` ~485-502/557/566, and the curl is `crates/remote_server/src/install_remote_server.sh:106`.

## LATENT BREAK — the wasm target does not compile (found 2026-08-11)

- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: phantom wasm-only import; both sites are `#[cfg(target_family = "wasm")]` and nothing in precheck or CI targets wasm, so it cannot break a build this fork ships.** Original entry:
      **`app/src/workspace/view.rs:179` and `wasm_view.rs` import a module that
      does not exist.** `crate::ai::conversation_details_panel::ConversationDetailsPanel`
      has no file and no `mod` declaration anywhere in the tree — it was deleted
      and these references were left behind.
      **Verified**: both sites are `#[cfg(target_family = "wasm")]`, which is the
      only reason `main` compiles. Nothing in `script/precheck` or CI targets
      wasm, so the break is invisible.
      So the honest statement is: **the wasm target is already broken and has
      been for some time.** It will fail the instant anyone builds it.
      Decide which: (a) fix the references and keep wasm building, or (b) declare
      wasm unsupported in `DECLINED.md` and strip the `target_family = "wasm"`
      code paths — carrying dead cfg-gated code that cannot compile is the worst
      of the three options, because it looks supported.
      Found by the tail-block sweep while adjudicating unrelated tests.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Confirmed still broken, line reference corrected: the import is `app/src/workspace/view.rs:178`, not `:179`. No `conversation_details_panel` file or `mod` declaration exists anywhere; both sites and the enclosing `mod wasm_view;` are wasm-gated, and nothing in CI or `script/precheck` builds wasm32, so it fails invisibly.

## UPSTREAM ZAP ISSUES — triaged against this fork 2026-08-10

Read-only triage of open issues on `zerx-lab/zap` (this fork's lineage: Phosphor
forked from Zap, which forked from Warp). The question asked was narrow — **is
the reported defect present *here*?** — not whether Zap should fix it.

Useful asymmetry this surfaced: several Zap issues are already fixed here as a
side effect of unrelated decisions. That is the inherited-lineage relationship
running in our favour for once, after four inherited subsystem removals ran the
other way.

### Already fixed here — no action, recorded so it is not re-investigated

- [x] **Zap #297 — English/multilingual tool descriptions and agent prompts.**
      Non-Chinese LLMs fail function-calling when tool descriptions arrive in
      Chinese. **Solved here by policy:** every prompt partial in
      `app/src/ai/agent_providers/prompts/partials/*.j2` greps to **zero** Chinese
      characters, and `CLAUDE.local.md` mandates English for comments, doc
      comments, log/error messages and test assertions, with conversion on touch.
      Arguably this fork's most valuable divergence from Zap.
- [x] **Zap #333 — PowerShell 7.6 will not open, "Shell process exited
      prematurely".** The PS 7.6 `-Command` quoting-parser crash. Fixed here by
      `encode_pwsh_command` at four sites including `local_tty/shell.rs`, the
      interactive session launch that is exactly the path the issue describes.
      **Caveat: fixed in code, unproven on hardware** — see the open
      "NEEDS WINDOWS VERIFICATION" item in this file. Do not close that on the
      strength of this row.

### Feature requests — triaged for what is ALREADY BUILT here

- [x] **Zap #319 (model filter) and #320 (disable a provider without deleting
      it) — BOTH ALREADY BUILT HERE.** Maintainer-confirmed 2026-08-11, then
      verified in code.
      **#320:** `AgentProvider.disabled` (`app/src/settings/ai.rs:1222`),
      `effectively_disabled()` (`:1400`), and the collapsed *"Disabled
      providers"* section in the settings UI. A provider stays enabled until
      explicitly disabled; it never auto-flips.
      **#319:** `model_search_query` / `set_model_search_query` /
      `model_matches_search` (`app/src/settings_view/agent_providers_widget.rs:171-200`),
      with render-time filtering at `:1585` and stale-query handling at `:1574`.
      > **Method note — this pair was first reported ABSENT, wrongly.** The
      > check grepped invented names (`provider_enabled`, `disable_provider`,
      > `model_filter`, `hidden_models`), all of which return zero because the
      > implementation uses different words. **A zero-hit grep on a guessed
      > identifier is not evidence of absence** — it is the same failure as the
      > `create_branch_tooltip` false *positive* recorded below, in the opposite
      > direction. Grep the subsystem's real files before concluding anything.
- [x] **Zap #293 — "executable size >300 MB on Linux" — EXPLAINED, no work for
      this fork.** Almost certainly an unstripped `--release` build: `cargo
      build --release` does not strip, and `[profile.release]` here sets
      `debug = 1`. Phosphor's shipped artifact does not have this problem —
      `[profile.release-cli]` is thin-LTO + `opt-level = "s"`, and
      `script/linux/bundle:285` runs `strip --strip-all` for production. Our
      tarball is ~45 MB, not 300 MB. The *remaining* size question here is a
      different one and is tracked separately — see the remote-server
      distribution section (the daemon is the whole application).


- [x] **DONE 2026-08-18 (unverified, not compiled) — both remaining gaps closed, local AND over SSH.**
      **My "this is UI wiring, not implementation" framing was WRONG** — recorded so it is not
      repeated. Per-file staging needed a staged column that existed nowhere in the model, and the
      SSH leg needed a new RPC end to end. Real size: **~1,000 lines across 21 files and 2 crates,
      including a wire-format change** (`GitStageRequest`/`GitStageResponse`, `FileDiff.staged`,
      `enum FileStagedState` in `remote_server.proto`). Landed whole rather than half.
      **Confirmed the pin has no counterpart**: every `git grep` hit for `stage_file`/`unstage`/
      `is_staged` at `42effe840` is the substring `include_unstaged`. Fork-original.
      Per-file staging deliberately omits `--worktree` — that single flag is the whole difference
      from the Discard path. Staging direction is read at **click time**, so a click racing a
      background reload cannot act on stale state; a rename stages both paths, or git stops seeing
      a rename and the old path returns as a phantom deletion.
      **Stated limitation, documented in code rather than hidden:** the code-review diff is
      `git diff HEAD`, which merges both sides of the index, so nothing in a rendered hunk reveals
      whether that hunk is already staged. Hunk direction therefore follows the *file* column. On a
      partially-staged file this guesses "stage", and re-staging an already-staged hunk is
      **rejected by `git apply`'s context check** (`--unidiff-zero` deliberately not passed) rather
      than corrupting the index. Honest per-hunk state needs a second `git diff --cached` parse per
      file — see the follow-up below. Original entry:
      **Zap #329 — "Improve local Git workflow inside Zap"** (open, `enhancement`,
      `lyfe2025`, 2026-07-28, marked *"1 (Nice to have)"*). Asks for a
      lightweight Git panel — changed files, diffs, stage/unstage files **or
      hunks**, commit with message, create/switch branches, pull/push, and the
      assistant able to reference git status/diff as workspace context.
      Explicitly scoped local-only: no account, no cloud sync, no hosted repo
      management — so it is **in scope for this fork by construction**.
      **RESCOPE 2026-08-17 (v0.1.0 agent audit)** — the "pull ABSENT" row is false: pull is fully built (proto `GitPullRequest` `:963`, `handle_git_pull` `server_model.rs:2695`, client `:582`, and a wired `GitDialogMode::Pull` with `git_dialog/pull.rs`). Hunk staging and branch create/switch now have primitives (`run_apply_patch_cached`, `hunk_to_patch`, `run_create_branch`, `run_switch_branch`) with **zero callers** — so those rows are still true in effect, but the work left is UI wiring, not implementation. The same stale state is repeated in this file's summary table near L162.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — **Four gaps, not three, and one cited evidence line is wrong.** `diff_state.rs:916` is a `log::warn!`, not the `restore --staged --worktree` call — the real ones (`:838-841`, `:980`) are the Discard Files path, not stage/unstage. Grep for `stage_file`/`unstage_file`/`is_staged` across `code_review/` returns zero, and `run_commit` (`util/git.rs:792`) takes only a binary `include_unstaged` and runs `git add -A`. So per-file staging is absent too. Pull is built; hunk staging, per-file staging, and branch create/switch are not.

      **Triage verdict: mostly already built here, inherited from Warp.**
      Phosphor has the whole `app/src/code_review/` subsystem — `diff_state.rs`,
      `git_status_update.rs`, `diff_selector.rs`, `code_review_view.rs`,
      `git_dialog/{commit,push,pr}.rs`, and `commit_message_gen.rs` (which is
      *better* than the ask: it drafts the commit message with the model). The
      remote leg exists too — `GitCommitChain`, `GitPush`, `GitCreatePr`,
      `GetBranches`, `DiscardFiles`, `GetCommittedBranchFiles` and the
      `GitFileStatus*` enum are all in `remote_server.proto`, so this works over
      SSH as well as locally.

      | #329 asks for | status here |
      |---|---|
      | View changed files | **built** — `git_status_update.rs`, `code_review_view.rs` |
      | View diffs | **built** — `diff_state.rs`, `diff_selector.rs` |
      | Commit with a message | **built** — `git_dialog/commit.rs` (+ AI-drafted message) |
      | Push | **built** — `git_dialog/push.rs`, `GitPush` |
      | Agent sees git status/diff as context | **built** — `code_review/context.rs` |
      | Stage / unstage **files** | **built** — `git restore --staged --worktree` in `diff_state.rs:916` |
      | Stage / unstage **hunks** | **ABSENT** — no `stage_hunk`; staging is whole-file only |
      | Create / switch branches | **ABSENT** — see the false positive below |
      | **Pull** | **ABSENT** — zero hits for `git_pull`/`GitPull` in the tree |

      **So the real work is three items, not a subsystem:** hunk-level staging,
      branch create/switch, and pull. Each lands on an existing surface rather
      than needing a new panel.

      **False positive, recorded so it is not re-derived:**
      `create_branch_tooltip` / `create_branch_name_element`
      (`code_review_header/mod.rs:265-297`) do **not** create branches — they
      render the current branch *name*. Grepping `create_branch` makes branch
      management look present. It is not.

      **Not filed upstream and not promised to anyone** — this is recorded as
      Phosphor's own backlog item. Also note the `pull` gap interacts with
      nothing else in this file, so it is a clean standalone starter task.

### Assigned 2026-08-10 — agent investigating whether these reproduce here

- [x] **CLOSED 2026-08-17 (maintainer) — closed as NOT-A-PROBLEM, not as fixed.**
      Maintainer runtime experience: *"314 have had no problems."* **The code
      smell remains**: `app/assets/bundled/bootstrap/fish.sh:618` still reads
      `$SHELL` and string-manipulates after the last slash, which is precisely
      what the issue objects to. It is simply not producing a user-visible
      failure. That distinction is recorded deliberately — if SSH Warpify shell
      detection ever misbehaves, this is the line to look at, and this row is not
      evidence that it was fixed.
      Original finding:
      **Zap #314 — SSH remote Warpify should not use `$SHELL` to pick the shell.**
      Likely present: `app/assets/bundled/bootstrap/fish.sh:618` does exactly what
      the issue objects to — *"We check the SHELL env var and use shell string
      manipulation to get the contents after the last slash"*. The bootstrap is
      inherited, so if the reasoning holds upstream it holds here. The fork does at
      least name the failure (`WarpificationUnavailableReason::UnsupportedShell`)
      rather than failing silently.
- [x] **Zap #328 — BYOP agent: approving an async tool action immediately triggers
      `OrphanToolResult`.** The most interesting of the three, because BYOP is this
      fork's core case and Zap's BYOP is the same lineage. The fork has a whole
      `app/src/ai/byop_readiness/` subsystem that *detects* the condition and has a
      repair pass — but whether it still *produces* it on async approval needs a
      real trace of the approval path. Unresolved by reading.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Already fixed: commit `5b2d600fa` (2026-07-29) "fix(byop): stop misclassifying fast tool results as OrphanToolResult", which cites the issue, was reproduced live against a local Ollama endpoint, and added `controller_readiness_treats_fast_current_input_result_as_pending_not_orphan` (`chat_stream.rs:9341`) plus two more regression tests. This predates the entry's own "Assigned 2026-08-10" line.
- [x] **DECLINED AS TRACKED WORK 2026-08-17 (maintainer).** The entry states its
      own blocker — "Runtime behaviour; not settleable by reading" — and that
      blocker is permanent in this environment: building is forbidden on this
      host and no agent can run the app. It is an unverified, possibly
      non-reproducing report with no source-level defect anyone can act on, so
      carrying it in the ledger costs re-reading and buys nothing.
      **Reopens on a concrete user reproduction: exact steps, shell, and OS.**
      The distinction below still holds and is why this was ever separate:
      `30dce9d5a` (2026-08-10, "fix(tui): ctrl-c must dismiss an open shortcuts or
      status sheet") was about an open shortcuts sheet, **not** signal delivery to
      a child process — do not treat that commit as having fixed this.
      Original finding:
      **Zap #275 — ctrl-c does not stop `python`.** Runtime behaviour; not
      settleable by reading. Distinct from the TUI ctrl-c sheet-dismiss defect
      fixed tonight (`30dce9d5a`), which was about an open shortcuts sheet, not
      signal delivery to a child process.

### Checked, not settleable by reading — no owner

- [x] **FULLY CLOSED — MAINTAINER DECISION 2026-08-17: #316 (Arabic rendering) closed too.**
      #310 and #294 were closed earlier today on maintainer runtime
      verification; #316 is now closed with them. None of the three is tracked
      further. Original entry follows.
      runtime verification; #316 stays open.**
      - **Zap #310 (tmux Warpify breaks `cd` + Tab completion) — CLOSED.**
        Maintainer tested it directly: *"310 works fine, tested."* Closed on
        runtime verification, not by reading — which is the only way an entry in
        this block can ever close.
      - **Zap #294 (`cat <<EOF` hangs) — CLOSED.** Maintainer: *"294 that works
        with tool calls."*
      - **Zap #316 (Arabic renders incorrectly) — STILL OPEN.** Not ruled on. Do
        not close it on the strength of the two above.
      Original finding:
      **Zap #310** (tmux Warpify breaks `cd` + Tab completion), **#294**
      (`cat <<EOF` hangs), **#316** (Arabic renders incorrectly). All runtime or
      visual claims needing the app running. Text-layout code exists across three
      platform backends, so #316 is not a missing-feature question.

## OPEN ISSUES FROM THE FIRST RE-PIN (2026-08-15) — `02b53fcd8` -> `42effe840`

Filed during the re-pin round and its refutation pass. **Open only** — defects
found and fixed inside the round are recorded in their commit messages, not
here, so this list stays a work ledger rather than a history.

### Pin moved 2026-08-15 — what the next round inherits

`ORACLE.md` now records `42effe840` (Warp `2026.08.12` stable). Carried forward
deliberately, each because finishing it is a reading pass rather than part of
moving the pin:

- [x] **435 unadjudicated absent tests.** `docs/STATE.md` counts 2,795 absent at
      the new pin, 2,360 of them adjudicated in `docs/sweep-verdict-ledger.tsv`.
      The remainder are tests in files that appeared or changed between the two
      pins. This is the next round's queue; generate it with
      `script/generate_repin_queue` rather than by hand.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Superseded by the `script/state` set-difference fix (#603). STATE.md now reports 2793 absent / 2078 adjudicated / **715** unadjudicated, not 2360/435. The 435 queue figure is dead; the procedural instruction (`script/generate_repin_queue`) still stands.
- [x] **`SCOPE-{AI,TERMINAL,REST}.md` are not re-derived at the new pin.** Their
      verdicts classify the 854 test-bearing files as they were at `02b53fcd8`.
      Banners now say so explicitly. Files added between the pins have no row at
      all — do not read "absent from SCOPE" as "no debt".
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Confirmed: `SCOPE-AI.md:5-13`, `SCOPE-TERMINAL.md:12-20`, `SCOPE-REST.md:5-13` all carry the "pin has moved; not re-derived" banner verbatim. The banner is the deliverable; nothing further is owed here.
- [x] **`ORACLE.md`'s "Gap at the pin" figures are old-pin figures** (net 2,239 /
      workload 1,605 / 854 files). Kept because they are the only written
      statement of net-vs-workload, flagged as historical. `docs/STATE.md` is the
      live number.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Confirmed: `ORACLE.md:82` is headed "measured at the OLD pin `02b53fcd8`" and every figure matches. The historical framing is already correct.
- [x] **Brand cleanup is string-literals-only.** `script/check_brand_strings`
      guards user-visible Rust string literals and is green. Identifiers and
      comments were never in scope: **~1,820 `Zap` and ~271 `Oz`** occurrences
      remain in `.rs`. Needs a maintainer decision on wire-value carve-outs
      first — `Harness::Oz` may be serialized the way `"warp-tui"` is
      (deliberately not renamed, interop surface), while `MCPProvider::Zap` is
      internal-only (no serde derives) and safe. Not a find-and-replace.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Confirmed and re-measured: guard exits 0; independent count is 1,856 `Zap` / 285 `Oz` in `.rs` (entry says ~1,820/~271, within its own tolerance). `Harness::Oz` carries `#[value(name = "oz")]` — a real clap wire surface; `MCPProvider::Zap` derives no serde, so it is internal-only. The carve-out reasoning holds.

### Defects — user-visible

- [ ] 🔴 **DIAGNOSED: the tab bar renders ONE tab for the whole test, so the six tab-group
      integration tests never drag anything.** Measured 2026-08-19 by logging inside
      `render_tab_bar_contents`: **`tabs=1 slots=1 active=0`, on all four paints**, during
      `test_drag_tab_out_of_group` — a test whose fixture creates FOUR tabs and whose model
      assertions pass. `TabComponent::build` likewise runs only for `idx=0`.
      **So every paint happens before the extra tabs exist, and nothing repaints afterwards.**
      That explains the whole cluster at once: `on_tab_drag` is never called (zero calls,
      symbol verified in the binary), `WorkspaceAction::DragTab` is never dispatched, and the
      assertions still pass because they read MODEL state while the bar on screen is stale.
      The drag helpers can only ever aim at tab 0, the one position the early paints saved.
      **Mechanism found:** `maybe_render_frame`
      (`crates/warpui_core/src/integration/step.rs:994`) only waits for a frame when
      `app.has_window_invalidations(window_id)` is true; otherwise it logs *"not checking for a
      frame to pass"* and moves on. So if the tab-adding steps leave no pending invalidation at
      that moment, no frame is ever rendered and the bar stays stale for the rest of the test.
      Note the real app is unaffected — the maintainer confirmed tab groups render and drag
      correctly — so this is a harness-only defect, not a product one.
      **Fix the harness, not the tests:** either force a render after steps that mutate the tab
      list, or have `open_extra_tabs` await a frame. Then re-verify ALL SIX by
      breaking the code each claims to cover — none of them has ever been shown to fail.
      Superseded framing:
      **ALL SIX tab-group integration tests pass WITHOUT ever calling `on_tab_drag`.**
      Found 2026-08-19 by instrumenting: a `log::error!` at the top of
      `Workspace::on_tab_drag` produced **zero** output across
      `test_drag_tab_out_of_group`, `test_drag_tab_into_group`,
      `test_drag_through_group_keeps_it_contiguous` and
      `test_drag_over_collapsed_group_keeps_it_contiguous`, while four other ERROR-level lines
      from the same run appeared normally and the symbol was confirmed present in the built
      binary. The drag code path is never entered.
      **So these tests do not verify the drag fixes.** They were reported as pinning the
      dragged-out-of-group / contiguity behaviour; they do not. They assert real end-state
      invariants (membership, header count, contiguity) but reach that state by some other
      route — most likely the mouse events never reach the tab `Draggable`, so nothing about
      the reorder logic is exercised.
      **The only verification the drag fixes have is the maintainer exercising them by hand in
      a real macOS build.** That is genuine, but there is no automated guard, and a future
      change to `on_tab_drag` would not be caught.
      **Narrowed by instrumentation, 2026-08-19.** `WorkspaceAction::DragTab` is never
      dispatched either, so the break is upstream of the workspace entirely: the tab's
      `Draggable::on_drag` (`app/src/tab.rs:2149`) never fires. Chain is
      `Draggable.on_drag` → `DragTab` → `handle_action` → `on_tab_drag`, and it dies at step 1.
      Ruled out along the way: events DO reach the window (`dispatch_mouse_event` goes through
      the platform window's `event_callback`), and tabs ARE laid out (`tab_bounds` panics if
      `tab_position_{i}` is missing, and it does not).
      Remaining suspects, in order: (a) no frame is painted between the mouse-down and the
      dragged event, so `Draggable` has no `origin`/`size`/`child_max_z_index` yet — its
      `LeftMouseDown` arm calls `self.origin().expect("origin should exist")`; (b) the armed
      drag never crosses whatever threshold `Draggable` requires; (c) the tab took the
      `sole_grouped_member` branch, which deliberately renders NO per-tab `Draggable`.
      **(a) TESTED AND DISPROVED 2026-08-19.** Split the mouse-down and the arming drag into
      two separate `TestStep`s (the driver advances a frame per step, and `begin_tab_drag` does
      both in one step via two `.with_action` calls). `on_tab_drag` was STILL never called.
      So a missing repaint between the two events is not the cause.
      **(c) MEASURED 2026-08-19, and it is the lead.** Logging every `TabComponent::build`
      during `test_drag_tab_out_of_group` yields **only `idx=0`** (4 calls, all
      `ghost=false sole=false`). Tabs 1-3 are never built, so the tab bar renders a single
      tab even though the fixture creates four. Tab 0 does get a `Draggable`, so the missing
      piece is not the sole-member branch.
      **So the likely story is that the tab bar under test is not rendering the tabs the test
      thinks it is** — which would explain every symptom at once: no drag, and assertions that
      pass because they read model state rather than what is on screen.
      Next: log `self.tabs.len()` and the slot count inside `render_tab_bar_contents` to see
      what the bar believes it is drawing.
      ⚠️ Two methodological traps hit while getting here, both worth avoiding: an instrumented
      build that **failed to compile** left a stale binary and produced a confident false
      negative (always check the symbol is in the binary), and nextest **captures output on
      passing tests** so `--no-capture` is required to see any of it.
      Superseded: (b) the drag never crosses `Draggable`'s threshold, or (c) the tab is not
      wrapped in a `Draggable` at all in this configuration — note
      `TabComponent::build` deliberately renders NO per-tab `Draggable` for a
      `sole_grouped_member`, and also skips it `for_drag_ghost`. **Check (c) first**: log
      which branch of `build()` each tab takes during the test. That is a cheap, direct
      question and the previous three suspects were all resolved by looking rather than
      reasoning.

- [ ] **The Windows usage suite is FLAKY, and that is the real problem.** The two specific
      failures are fixed (`9c6eb1621`) and both now pass — but the failure COUNT swings wildly
      run to run with a different set each time:
      **13 failures (08-18) → 2 (08-19 nightly) → 6 (on `9c6eb1621`)**, drawn from a shared
      pool including tests tagged `reliable-here` (`usage_tabs_add_switch_close`,
      `usage_agent_block_render`) which by definition should not be flaky.
      So the suite has never been a trustworthy signal on Windows, and chasing individual
      scenarios will not fix it. Needs its own investigation: all failures report a bare
      `exit code 1` alongside `warp::completer` path-conversion warnings, which is the thread
      to pull. **Do not treat a green Windows usage run as meaningful until this is understood.**
      Original entry:
      **The nightly `Usage Test Suite` has failed EIGHT NIGHTS RUNNING (2026-08-12 → 08-19),
      Windows only.** Linux and macOS pass every night. Nobody is watching it.
      **This corrects a framing error in this file:** `usage_tui_transcript_render` was recorded
      as needing "a Windows machine" to verify. It does not — **CI is a Windows machine and has
      been reporting this failure nightly for over a week.** Anything gated on "we have no
      Windows box" should be re-checked against this workflow before being deferred again.
      Two failures, from run `32228845452`:
      1. **`usage_tui_transcript_render`** — panics at
         `app/src/terminal/model/grid/ansi_handler.rs:189`: *"Grid received input but did not
         receive Reset Grid OSC"*. Tagged **`reliable-here`**, i.e. explicitly not expected to
         be flaky, and it still fails all 3 retries. This is the real one.
      2. **`usage_secret_redaction`** — expected `"Phone: … abcdef."`, got
         `"Phone: … abcdef\n."`. A newline where the assertion wants none: line-wrap or CRLF
         handling on Windows. Tagged `needs-real-shell`, so lower confidence, but the diff is
         one character and looks genuine rather than environmental.
      Reproduce without hardware: `gh run view <id> --log-failed` on the nightly, or dispatch
      `usage-test.yml` manually.

- [ ] **Collapsed-group drag: fixed and confirmed by hand; the hop STILL has no regression
      test, and an attempt to write one proved vacuous.** 2026-08-19: added
      `test_drag_over_collapsed_group_keeps_it_contiguous`, then disabled the collapsed-group
      hop in `on_tab_drag` and re-ran it — **it still passed**, so it does not pin the hop.
      **THREE attempts, three vacuous tests — the question is now about the CODE, not the test.**
      A `drag_over_group` helper targeting the container rect was written (it is in
      `crates/integration/src/test/tab_groups.rs` and works), and the test still passed with the
      hop disabled — as did dragging leftward and dragging at a member tab.
      **The code says the branch IS reachable, so the gap is the test geometry, not dead code.**
      Traced 2026-08-19: drag tab 1 (ungrouped) whose right neighbour is index 2 (a collapsed
      member) → `neighbor_drag_rect` returns the GROUP CONTAINER rect (`view.rs:25674`, it
      deliberately substitutes the container for a collapsed member) → if the drag midpoint
      passes `container.min_x()`, `calculate_updated_tab_index` returns `current_index + 1` = 2
      → index 2 is a collapsed member of a group the dragged tab is not in → the hop fires.
      Note the threshold is the container's **left edge**, not its centre.
      Empirically the drag still does not produce that state — so the next step is to instrument
      `calculate_updated_tab_index` / `neighbor_collapsed_group` during the test and see what
      the values actually are, rather than inferring. One diagnostic build answered the
      Welcome-pane mystery today after hours of theorising; the same applies here.
      **Do not write a fourth test before instrumenting.** Superseded detail:
      **Root cause identified, and the fix is a missing helper.** `drag_over_tab` aims the cursor
      at a TAB's saved rect (`tab_position_id`); a collapsed group renders no member tabs, so
      there is nothing to aim at and the drag does not move — tried both leftward and rightward,
      both still passed with the hop disabled.
      The hop IS reachable in principle: `calculate_updated_tab_index` moves one slot per event
      via `neighbor_drag_rect`, which for a collapsed member deliberately falls back to the
      **group container rect**. So the test needs a drag helper that targets
      `htab_group_position_id(group)` rather than a member tab. **Writing that helper is the
      actual task** — then re-verify by disabling the hop and confirming the test fails.
      The test is kept: it asserts real invariants over the collapsed path that had no coverage
      at all, and its doc comment says plainly that it is not the guard. ✅ **2026-08-19: the maintainer exercised tab groups in a real macOS
      build and reports them working.** That covers the three reported symptoms (drag out,
      drag in, duplicate headers) and is stronger evidence than any test here — the unit suite
      passed 151 tab-group tests while the feature was badly broken, because `on_tab_drag`
      resolves groups from laid-out element rects the unit harness cannot supply.
      What remains is only the missing **automated** guard for the collapsed-group path, so a
      future change can silently reintroduce it. Original entry:
      **Collapsed-group drag: FIXED but UNTESTED.** `on_tab_drag`'s collapsed-group hop was
      ported 2026-08-19 (`view.rs`, matching `42effe840:view.rs:28734`), and it is the third and
      last piece of the duplicate-header bug. But the six new tab-group integration tests cover
      only **expanded** groups — `test_drag_through_group_keeps_it_contiguous` does not collapse
      first. So the exact path that produced the reported duplicate for a collapsed group has a
      fix and no regression test. Add one: collapse a group, drag a non-member over it, assert
      one header and one contiguous run.

- [x] **DONE 2026-08-19 (`fb5f325fe`, CI-verified in run 32247666723).** Ported together with
      its caller `index_for_restored_tab`, plus the group-rejoin / pinned-clamp / expand steps
      `restore_closed_tab` was missing. Two regression tests added. Original entry:
      **`index_avoiding_group_split` is still unported** — restoring a CLOSED tab can land inside
      a group and split it, which renders as a duplicate header exactly like the drag bug did.
      Not ported because its only pin caller, `index_for_restored_tab`, does not exist here
      (`42effe840:view.rs:12486`), so it would be dead code. Porting requires bringing the caller
      too. Reproduce with: close a tab inside a group, then undo-close it.

- [x] **DECIDED 2026-08-19 (maintainer): keep pin parity, do not change it.** Recorded in
      `DECLINED.md`. The behaviour stays and is now documented by characterization tests
      rather than being silent. Original entry:
      **Credits round to 1dp PER AGENT before the rollup sums them.** `AIConversation::credits_spent()`
      rounds before `compute_orchestration_rollup` sees the value, so three agents at 0.04 each
      (0.12 real) report as nothing spent and the usage footer never appears. **Same at the pin**
      (`42effe840:app/src/ai/agent/conversation.rs:769-773`), so it was pinned as a
      characterization test rather than changed unilaterally — but this fork is BYOP, the user is
      paying the provider directly, and silently erasing sub-5c spend is a real accounting
      question. **Needs a maintainer decision:** keep pin parity, or sum before rounding.

- [x] **The release build has been run.** Closed 2026-08-19 by the `v0.1.0` release: the
      tagged build completed `--release` (ThinLTO and codegen paths exercised) and `script/bundle`
      produced 13 artifacts, including both macOS DMGs (`Phosphor-arm64.dmg`, `Phosphor-intel.dmg`)
      and the Windows installer (`PhosphorSetup.exe`) — the two paths this entry called
      unexercised — plus AppImage, deb and rpm. Verified against the published release, not
      against a local run; the Linux build host still cannot produce the DMG/installer, which is
      why this ran in CI.

> **HOW TO RUN THE TESTS (2026-08-18) — two mistakes that cost real time today.**
>
> **1. Use the CI configuration, or you will measure phantom failures.**
> `cargo nextest run --workspace --exclude warp` with **default features** reported
> **247 failures**, 194 of them the identical `terminal_view should exist ...` panic.
> None were real. CI runs these as
> `./script/check_test_failures -p warp -p warp_tui --lib --features warp/gui` — without
> `warp/gui` most of those tests have no terminal view at all. Under the CI command the
> same tree was **6885/6885**. The other jobs are `-p warpui_core --features tui` (the
> `tui` feature is **not** optional — CI says so at `pr-check.yml`), a 40-package sweep,
> and `-p integration` under `xvfb-run`.
>
> **2. `script/check_test_failures` is the right entry point, not bare `cargo nextest`.**
> It diffs against `script/known_test_failures.txt` and fails on **change**: a new failure
> is a regression, and a listed test that starts passing is a stale entry that must be
> deleted in the same change. That list is currently **empty**, so `main` is expected
> fully green. It also routes through `script/agent-cargo`, which pins `CARGO_TARGET_DIR`
> and `TMPDIR` onto real storage.
>
> **3. `/tmp` here is a 12G tmpfs — i.e. RAM.** A worktree or `TMPDIR` under it fills the
> tmpfs mid-build and every crate dies with a disk error while the real disk sits ~60%
> free. `check_test_failures`'s own header documents this from 2026-08-08; I hit it again
> anyway on 2026-08-18 by putting a comparison worktree in `/tmp`. Put worktrees under
> `/home` and export `TMPDIR=/home/winters/.cache/phosphor-tmp`.

> **BUILD LOG 2026-08-18 — first compilation of the day's work.** `cargo check --workspace
> --all-targets` went **21 errors → 0**. Only **three** were real; the rest cascaded.
> 1. **`pin_project_lite` was never declared** — 18 of the 21. `app/src/ai/mcp/sse_transport/`
>    is **entirely untracked** (new in the rmcp 1.6 migration) and uses it. The workspace
>    declares `pin-project`, a **different crate**, so it looked present at a glance, and the
>    lockfile carried `pin-project-lite` transitively. Added `pin-project-lite = "0.2.17"` to
>    the workspace table + `app/Cargo.toml`.
> 2. **`Option::map` on an owned `peer_info()`** (`ai/mcp/templatable_manager/native.rs`) —
>    rmcp 1.6 returns `Option<ServerInfo>` by value, so the borrow of `info.capabilities`
>    could not escape the closure. `.as_ref()`.
> 3. **`TabSettingsChangedEvent::EnableTabGroups` arm missing** from
>    `Workspace::handle_tab_settings_change` — the only error from the whole tab-groups port,
>    and it was in the toggle, not the rendering. Without it the toggle would not repaint the
>    tab bar until some unrelated event happened to, i.e. it would look intermittently broken
>    rather than cleanly failing.
>
> **The `collect` ambiguity in `notebooks/editor/model_tests.rs` was a cascade, not a latent
> bug** — it vanished once the `pin_project_lite` types resolved. I had wrongly suspected a
> pre-existing error because the file is unmodified in git.
> **~3,000 lines of tab-groups work across `view.rs`, `vertical_tabs.rs`, `tab.rs` and
> `warpui_core` produced exactly one compile error.** The agents diffing against the pin
> rather than writing from scratch is the plausible reason.
> ⚠️ Type-checking is not testing — the suite is the real gate, and the run-duration port's
> ~12 changed `terminal/view_test.rs` call sites are the prime suspect.

- [x] **DEFERRED PAST v0.1.0 2026-08-18 — known, documented limitation, not a blocker.**
      ⚠️ Maintainer may reverse. The behaviour is safe: git rejects a re-apply on its context
      check rather than corrupting the index. It is *silent* (logged, not surfaced), which is the
      part worth fixing, but the fix is a second `git diff --cached` parse per file — its own
      change with its own perf question. Original entry:
      **Per-hunk staged state is inferred from the file, not read.** The code-review diff is
      `git diff HEAD`, so a rendered hunk carries no indication of whether it is already staged;
      direction follows the file's column and guesses "stage" on a partially-staged file.
      Fix is a second `git diff --cached` parse per file. Failure today is safe (git rejects the
      re-apply) but silent — it is logged, not surfaced. Not v0.1.0-blocking.

- [x] **DONE 2026-08-18 (unverified, not compiled) — folded into step L as planned.** Both
      surfaces render: per-pane at `vertical_tabs.rs:439` and group header at `:2881`, with
      `is_pinned` threaded through 6 call sites. Doing it inside L rather than standalone avoided
      touching those call sites twice. Original entry:
      **Vertical tabs render no pin indicator at all.** Found 2026-08-18 while porting the
      horizontal one. `app/src/workspace/view/vertical_tabs.rs` has **zero** pin rendering; the
      oracle draws `PinFilledDiagonal` on two further surfaces — a per-pane pin at
      `42effe840:app/src/workspace/view/vertical_tabs.rs:449`, and the group-header pin at `:2908`.
      The icon and asset now exist here, so this is threading, not a port from scratch: `is_pinned`
      has to reach `VerticalTabPane` through ~6 call sites (`:1236`, `:1843`, `:1874`, `:2233`,
      `:2292` at the pin).
      **Fold into step L of the tab-groups port** (vertical-tabs group rendering) rather than doing
      it standalone — L rewrites the same call sites, so doing both separately means touching them
      twice. The group-header half is blocked on step H regardless.

- [x] **FIXED 2026-08-18 (unverified, not compiled).** `Icon::PinFilledDiagonal` + its path arm
      added to `crates/warp_core/src/ui/icons.rs`; `pin-filled-diagonal.svg` taken **byte-for-byte
      from the oracle** (`git show 42effe840:...`, `cmp`-verified, 1394 bytes — not hand-drawn and
      not aliased onto `pin-filled.svg`, which would have shipped the wrong glyph); consts and the
      render branch added to `app/src/tab.rs`, with `render_close_tab_button` renamed to the
      oracle's `render_close_button_or_pin_icon` so future re-pin diffs stay aligned.
      Exactly one exhaustive match over `Icon` exists tree-wide (`impl From<Icon> for &'static str`),
      so the enum change was atomic with its single consumer.
      New `icons_tests.rs` asserts the path mapping, that `PinFilledDiagonal != PinFilled`, and
      that all three pin SVGs exist on disk — a bad asset path renders as **nothing** at runtime
      rather than failing to compile, so it is the failure mode nothing else catches.
      **This doubles as step D of the tab-groups port** (prerequisite for E and G). Original entry:
      **Pinned tabs render no pin indicator — cosmetic, ~60 lines.**
      Found 2026-08-18. `Icon::PinFilledDiagonal` has **0** references tree-wide, the asset
      `app/assets/bundled/svg/pin-filled-diagonal.svg` is **absent**, and
      `TAB_PIN_INDICATOR_ICON_SIZE` has 0 references; the fork's `tab.rs` mentions `pinned`
      4 times to the pin's 12, all for the context-menu label. The pin draws an indicator.
      **`pinned_tabs` deliberately STAYS default-on, unlike `grouped_tabs`.** An initial
      reading grouped the two as the same anomaly; they are not. `pin_tab`
      (`view/tab_grouping.rs:573`) moves the tab to `pinned_boundary_index` and
      `clamp_to_unpinned_region` (`:557`) holds the boundary, so **pinning has its real,
      visible effect — the tab anchors to the front of the bar.** Only the glyph is missing.
      That is a cosmetic gap in a working feature, not invisible persisted state, so the
      argument for disabling `grouped_tabs` does not carry over.

- [x] **FIXED 2026-08-18 (unverified, not compiled) — one line.** Added
      `let index = self.clamp_to_unpinned_region(&self.tabs, index);` to
      `insert_transferred_tab_at_index` (`workspace/view.rs`), before the existing
      `clamp_past_group`, matching the pin's order exactly.
      **Note for the next reader:** `clamp_to_unpinned_region` is defined in the submodule
      `app/src/workspace/view/tab_grouping.rs:557` as `pub(super)`, **not** in `view.rs` — a
      `grep -n 'fn clamp_to_unpinned_region' view.rs` returns nothing and reads as "the helper
      does not exist". It does, and `pub(super)` makes it callable from `view.rs`.
      Original entry:
      **Cross-window tab drops can land inside the pinned region — the fork lacks a clamp the pin has.**
      Found 2026-08-18 while porting `79fdd7ceb`. **Verified directly:**
      `42effe840`'s `insert_transferred_tab_at_index` calls `clamp_to_unpinned_region` once;
      the fork's calls it **zero** times, and `tab_insertion_index_for_cursor` likewise returns
      an unclamped index. So dragging a tab from another window can insert it among the pinned
      tabs. Pre-existing and independent of the group work.
      **Partly masked as of today:** the `8794f7325` port routes the `AfterCurrentTab` path
      through `new_tab_index_and_group`, which *does* clamp — but that covers new-tab placement,
      **not** the cross-window transfer path, which is still unclamped.

- [x] **DEFERRED PAST v0.1.0 2026-08-18 — pre-existing fork-wide convention, not a regression.**
      ⚠️ Maintainer may reverse. Logged rather than fixed because the fix is "move `t!` into the
      render path fork-wide", which is its own task with its own blast radius, and the symptom
      is confined to a mid-session locale switch. Original entry:
      **Localized labels resolve once at view construction, not per render — minor i18n staleness.**
      A mid-session locale switch does not relabel affected widgets until the view is rebuilt.
      **Pre-existing class, not new** — it is the established fork convention
      (`settings_view/mcp_servers/edit_page.rs:133`, `list_page.rs:234`), and upstream has no
      equivalent because it uses bare literals. The `836b73c88` conversion added two more
      instances (`InstallationModalBody::new`, `UpdateModalBody::new`). Logged so the count is
      known; fixing it means moving `t!` into the render path fork-wide, which is its own task
      and **not v0.1.0 work**.

> **PARTIAL sweep CLOSED 2026-08-18.** All 54 candidates reconciled — 38 were genuinely
> unverified at the start (not the ~44 the brief assumed; 16 already carried verdicts).
> It sized four defects, all now done or assigned: `2356ddab2` settings search (1 line),
> `04e0c2297` child agents lost on restore (1 line), `a530563eb` chip back-sync (done
> above), `8794f7325` new tabs escape their group.
> **Two warnings from it, recorded so they are not re-derived:**
> the `17/31/3/2/1` PORTABLE breakdown quoted in earlier prose **appears nowhere in the
> tree** — an uncommitted chat artifact, exactly the failure `docs/sweep/artifacts-2026-08-15/README.md`
> documents; and of the nine PORTABLE defects listed further down, **four are stale, one is
> mis-sized, four are live**.
> It judged `8794f7325` "not worth doing standalone" behind the tab-group rendering hole.
> **That call is being revisited:** the group-splitting bug corrupts *persisted* membership
> via `move_group_block`, and bad data on disk outlives the rendering gap.

- [x] **DONE 2026-08-18 (unverified, not compiled) — net −92 lines across 4 files.**
      `installation_modal.rs` (−59), `update_modal.rs` (−32), `list_page.rs` (0),
      `settings_view/mcp_servers_page.rs` (−1). No hand-rolled button colour logic remains
      in either modal.
      **The `3ac1efb03` subsumption question is settled, and I verified it directly rather
      than accepting the report:** `PrimaryTheme::background` returns `Some(theme.accent())`
      when not hovered (`action_button.rs`), so `PrimaryTheme::text_color` evaluates to
      `theme.font_color(theme.accent())` — literally the `accent_text_color` that patch
      introduced — and `keyboard_shortcut_border` reproduces its 60-opacity border. `render`
      then applies `on_background(.., MinimumAllowedContrast::Text)`, which the hand-rolled
      path never had. So the conversion **replaces** the fix and improves on it.
      **Three premises in my brief were wrong**, recorded so they are not re-derived:
      the call site is `app/src/settings_view/mcp_servers_page.rs:86`, **not** under
      `mcp_servers/`; it is **four** files, not six; and `836b73c88` is **not an ancestor of
      the pin** (upstream rebased it in), so the port followed the pin's content and import
      grouping rather than the commit's post-image.
      Labels use `crate::t!` per fork convention (upstream uses bare literals).
      **No tests added** — the pin adds none and this subtree has no view-harness precedent;
      writing one blind under the no-build rule would be a guess. Original entry:
      **`836b73c88` — MCP modals still hand-roll their buttons (~200 lines, net −85).**
      `installation_modal.rs:74-75,444` and `update_modal.rs:50-51,313` build buttons by
      hand where the pin uses `ActionButton` (which exists here at
      `app/src/view_components/action_button.rs:207`). Call sites: `mcp_servers_page.rs:86`,
      `list_page.rs:189,715,937,955`.
      **Cosmetic only — no defect** (an earlier pass confirmed that). Worth noting because
      it **subsumes** the `3ac1efb03` accent-contrast fix already applied today: doing this
      would replace that hand-rolled colour logic with the shared component rather than
      layering on it. So it is cleanup, and mostly deletion, but it should be done as one
      piece if at all.

- [x] **FIXED 2026-08-18 (unverified, not compiled).** Inserted the pin's third reapply site
      after `set_and_refresh_current_page_internal` in the auto-select jump
      (`settings_view/mod.rs:1624`); the file now has **3** `reapply_search_filter_to_active_subpage`
      call sites, matching the pin, where it had 2 (`:2244`, `:2628`).
      A test was added at the level the omission actually lives at — a source-level invariant
      (`query_preserving_navigation_reapplies_the_subpage_filter`) asserting every
      `set_and_refresh_current_page_internal` call with `should_clear_query = false` is followed
      by a reapply, and that there are exactly 3 such sites. The existing `PageType`-level tests
      structurally **cannot** see a missing call site, which is why they stayed green; their own
      comment said so, and that comment (which claimed the fix was unported) is corrected.
      Original entry:
      🔴 **Settings search shows every widget after the auto-select jump — ONE LINE.**
      (`2356ddab2`, verified directly.) When a query filters out the current page, the view
      jumps to the first matching subpage — whose `PageType` is rebuilt with the default
      all-widgets filter and **never reapplied**. The fork has 2 of the pin's 3 reapply
      sites (`settings_view/mod.rs:2239`, `:2623`); the pin's third is
      `42effe840:app/src/settings_view/mod.rs:1690`.
      **Fix:** insert `self.reapply_search_filter_to_active_subpage(&search_query, ctx);`
      after `app/src/settings_view/mod.rs:1619`. `search_query` is already in scope from
      `:1457`. The fork's own test comment at `mod_tests.rs:1216-1219` explains why the
      tests stayed green.

- [x] **FIXED 2026-08-18 (unverified, not compiled).** Added `LLMPreferences::copy_agent_mode_selection`
      (`app/src/ai/llms.rs:1051`) and switched `copy_model_and_profile_to_terminal_view` to it
      (`workspace/view.rs:12596`). It copies the raw per-view override **including its absence**
      and emits/saves only on change, so it never touches settings.
      Confirmed before fixing that `update_preferred_agent_mode_llm` (`llms.rs:1023-1033`) wrote
      `byop_last_used_model_id` unconditionally and that this is priority #2 in
      `get_preferred_base_model` (`:717`) — so forking really did change the default for every
      new tab and across restarts.
      **The `byop_last_used_model_id` tier itself is untouched** — it is a deliberate fork
      feature with a `DECLINED.md` row; this stops `/fork` *writing* it, nothing more.
      Two tests added in `ai/llms_tests.rs`, including the absent-override case. Original entry:
      🔴 **`/fork` rewrites the global default model for all future tabs — ~35 lines.**
      (`56d2022b2`'s local half — this was CONTESTED earlier and is now confirmed by direct
      verification.) `copy_model_and_profile_to_terminal_view`
      (`app/src/workspace/view.rs:12584-12604`) copies the **resolved** model through
      `update_preferred_agent_mode_llm`, which **unconditionally** writes
      `AISettings::byop_last_used_model_id` (`app/src/ai/llms.rs:1023-1033`) — priority #2 in
      `get_preferred_base_model`. So forking a pane with an explicit override **changes the
      default for every new tab and across restarts**. Ordering also makes the fork silently
      resolve to the *source profile's* default when the models coincide.
      **This side effect does not exist at the pin.** Fix: add
      `LLMPreferences::copy_agent_mode_selection` (raw-override copy) and call it instead.

- [x] **FIXED 2026-08-18 (unverified, not compiled) — 18 files, far larger than the ~150-200
      estimate.** Both halves ported. **My framing was incomplete in two ways**, recorded so it
      is not re-derived: `crates/ai/src/agent/action_result/mod.rs` had **no timestamp fields at
      all** (four variants had to gain `start_ts`/`completed_ts`, and there were **four**
      hardcoded `None` sites in `convert.rs`, not one); and the actual user-visible zero duration
      came from `conversation.rs::to_serialized_blocklist_items` writing **the same** exchange
      timestamp to both fields — so the duration would still have rendered as 0s even after the
      wire fix.
      Restore half done as the full upstream refactor, because **both "dead" functions had live
      callers**: `Startup` now carries `Vec1<AIConversation>`, `take_conversations` added, and
      the forward scan replaced by `find_block_indices_for_exchange_timestamps` (14 unit tests).
      Deleted after grepping clean: `get_conversations_to_restore`,
      `find_run_shell_command_result_for_message`, `AIConversation::find_run_shell_command_result`,
      `BlockList::create_restored_command_block`.
      Not ported: upstream's `is_local: None`, the `seen_command_ids` dedup, and the cloud
      `set_is_executing_oz_environment_startup_commands` hunk.
      ⚠️ **A REGRESSION SLIPPED IN HERE AND WAS CAUGHT BY A GUARD — fixed 2026-08-18.**
      This port added a **second** `find_block_indices_for_exchange_timestamps`, in
      `app/src/terminal/view/load_ai_conversation.rs`, using the pin's `>=` tie-break. The fork
      **already had** that function in `app/src/terminal/conversation_restoration.rs` using strict
      `>`, which is recorded decision **#174**: a command block whose timestamp exactly ties an
      exchange belongs to the exchange *before* it, so the TUI and GUI restore a conversation in
      the same order. Verified at `HEAD` that the GUI's own pre-change scan skipped blocks with
      `ts <= exchange_timestamp` — i.e. the fork's rule — so the duplicate genuinely reverted it.
      `script/check_declined_collisions` caught it because two of the 14 new tests carry
      `test:` markers in `DECLINED.md` as pinned tests that must never exist here.
      **Fixed by deduplicating, not by deleting the tests:** the duplicate impl and its whole test
      module are gone, `conversation_restoration`'s version is now `pub(crate)`, and the GUI calls
      it. One implementation means the two surfaces cannot drift apart again, which is exactly what
      that function's doc comment asks for. No coverage lost — `conversation_restoration` already
      tests every same shape, plus two fork-specific tie tests the duplicate did not have.
      **Lesson: porting a pin function wholesale can silently revert a recorded divergence.**
      Check `DECLINED.md` markers before adding upstream tests, not after.
      ⚠️ **CARRY THIS INTO THE BUILD:** `app/src/terminal/view_test.rs` has ~12 calls to
      `restore_conversation_after_view_creation`, whose block-creation path moved — upstream
      adjusted its own tests in the same commit but the fork's are fork-specific. **This is the
      most likely source of test failures when the suite runs.** Original entry:
      **Shell-command run duration is never shown — ~150-200 lines.** (`a2f586584`.)
      `crates/ai/src/agent/action_result/convert.rs:35-37` hardcodes
      `start_ts: None, finish_ts: None` with a self-flagging comment ("See #11"). The
      accessors already exist (`terminal/model/block.rs:2714,2718`). The GUI restore path is
      separately unported: `terminal/view/load_ai_conversation.rs:1408-1431` still runs the
      pre-fix forward scan, and two functions the upstream commit **deleted**
      (`get_conversations_to_restore:402`, `find_run_shell_command_result_for_message:1015`)
      are still present here.

- [x] **FIXED 2026-08-18 (unverified, not compiled). Removing the Notifications chip no longer flips a settings value.**
      Deleted the back-sync at `app/src/workspace/header_toolbar_editor.rs:183-190` and the
      now-unused `AISettings` import; left a note in its place. **Verified against the pin
      directly:** `42effe840:app/src/workspace/header_toolbar_editor.rs` never mentions
      `NotificationsMailbox` at all. The legitimate direction survives untouched — the
      setting still gates chip visibility at `workspace/header_toolbar_item.rs:81`, and chip
      *placement* is stored separately in `TabSettings::HeaderToolbarChipSelection`, so the
      two controls are independent again as they are upstream. `report_if_error!` has two
      other uses in the file, so that import stays. Original entry:
      Residual half of `a530563eb`. `app/src/workspace/header_toolbar_editor.rs:183-190`
      (plus the `AISettings` import at `:11`) syncs `show_agent_notifications` when the chip
      is removed from the header toolbar; **the pin has no such sync** (0 hits at
      `42effe840`). The severe consequence is gone now that `add_notification` records
      unconditionally, but the coupling still silently suppresses the mailbox chip **and**
      the agent-input-footer affordance (`agent_input_footer/mod.rs:831`).

- [x] **DONE 2026-08-18 (unverified, not compiled).** `SettingsFileError::UnknownKeys(Vec<String>)`
      added, constructed at both `app/src/settings/init.rs:181` (startup) and `:400` (hot reload),
      with precedence `FileParseFailed` → `InvalidSettings` → `UnknownKeys`.
      **Two premises in my brief were FALSE**, recorded so they are not re-derived: neither
      `app/src/settings_view/settings_file_footer.rs` **nor** `app/src/workspace/view.rs` matches
      on this enum — both go through `heading_and_description()`. The `InvalidSettings` hits in
      `view.rs` are `WorkspaceBanner::InvalidSettings`, a **different enum**. So the contended file
      needed no edit and got none. Conversely I **missed two** match sites that did need extending,
      both in `app/src/settings/mod.rs` (the `Display` impl and `heading_and_description`).
      Final arm count verified: 5 sites, all three-armed.
      **`last_unknown_settings_file_keys()` was deliberately NOT used** — it is stale across the
      diagnostic's three early returns (flag off, file unreadable), so a banner driven by it could
      report keys from a file the call never read. `report_unknown_settings_file_keys` now returns
      the set instead; the old accessor stays `pub` and callerless with its doc rewritten to say why.
      Copy avoids the `InvalidSettings` trap entirely — it says the key is unrecognized and ignored,
      never that a default is being used. A regression test asserts `InvalidSettings`' own copy is
      byte-identical, so the two cannot drift together.
      **No `.ftl` entries added, and that is deliberate** — the sibling error strings and both TUI
      hint consts are bare literals, not `crate::t!`; only the surrounding button labels are
      localized. Localizing these six strings is its own consistent pass, not a side effect here.
      Minor follow-ups, not worth their own items: `WorkspaceBanner::InvalidSettings`'s doc comment
      still says "parse failure or invalid values" though it now also carries unknown keys.
      Original entry:
      **Surface unknown `settings.toml` keys in the app, not just the log.** The diagnostic
      landed 2026-08-18 but stops at a `log::warn`. The in-app affordance is
      `SettingsFileError` (settings-page footer + workspace banner + TUI hint), and adding an
      `UnknownKeys` variant breaks exhaustive matches in `app/src/lib.rs:1374`,
      `settings_view/settings_file_footer.rs`, `workspace/view.rs`, and
      `crates/warp_tui/src/terminal_session_view.rs:191` — so all five files must land
      together. `last_unknown_settings_file_keys()` already exists as the accessor.
      Do **not** reuse `InvalidSettings`: its message is *"Invalid value for 'x'. The default
      value is being used."*, which is wrong here — there is no setting.

- [x] **PORTED 2026-08-18 (`view.rs:23600-23633`). UNVERIFIED (not compiled).** Mirrors
      `42effe840:view.rs:28418-28443` including the pin's `if current_index >= self.tabs.len()`
      bail. **Bounded by reading:** the remap is the identity whenever `tabs[current_index]`
      is the dragging tab, and `unwrap_or(current_index)` makes it the identity whenever no
      tab reports a drag — it diverges only when the captured index names a non-dragging
      tab, which is exactly the bug. Verified at most one tab per window can report
      `is_dragging()`. Five consumers were exposed, including the newly-landed
      `ReorderInSource` arm, exactly as predicted. **No in-tree test exercises this path** —
      `test_reorder_tabs_with_drag` uses 2 tabs with each mouse event in its own step, so a
      pre-repaint second drag event never occurs. Original entry:
      **`on_tab_drag` lacks the pin's stale-`current_index` remap.**
      `42effe840:app/src/workspace/view.rs:28418-28443` opens `on_tab_drag` by re-finding
      the dragged tab via `draggable_state.is_dragging()`, because a mouse event arriving
      **before repaint carries a pre-swap index**. The fork has no such remap.
      Found while porting `ReorderInSource` (2026-08-18) and **deliberately not ported
      then**: the new arm inherits exactly the same exposure the fork's *existing*
      in-window reorder path already has, so adding it would have changed pre-existing
      reorder behaviour well beyond that defect's blast radius. Separate ticket, and it
      affects in-window reordering generally, not just cross-window drag.

- [x] **DONE 2026-08-18 — comment-only.** Rewritten to describe `ReorderInSource` +
      `ActiveDrag::reordering_in_source`. `DragPhase::InsertedInTarget` still exists but is
      now only the transient state between `perform_handoff` and `finalize`. Original entry:
      **`test_multi_tab_drag_back_to_source_and_out_again`'s doc comment describes a dead
      encoding.** `crates/integration/src/test/workspace.rs:822-826` explains the fork's
      `DragPhase::InsertedInTarget { target == source }` shim — which no longer exists as of
      the 2026-08-18 `ReorderInSource` port. Comment-only; the assertions are correct and
      were deliberately left untouched. Same class as the five unverified `This currently
      FAILS` blocks: prose that outlives the code it describes.

- [x] **ALL FOUR FIXED 2026-08-18 (unverified, not compiled).** Three were genuine
      production gaps in `app/src/ai/blocklist/history_model*`, now closed:
      (1) `restore_conversations` indexes `children_by_parent` through
      `resolved_parent_conversation_id_for_conversation`, restoring the `parent_agent_id`
      fallback; (2)/(3) `initialize_historical_conversations` rewritten as the pin's two
      passes, seeding `agent_id_to_conversation_id` and `server_token_to_conversation_id`
      for **every** restorable row before resolving parents (this also removed a trailing
      loop that iterated a `HashMap`, so duplicate-token resolution was nondeterministic);
      (4) `initialize_output_for_response_stream` now persists rather than only updating
      in-memory indices, so the `StreamInit` server token and run id survive a restart.
      **The fourth test needed a harness fix too, and I verified that is not a weakening.**
      It used `sync_channel(1)` with a single `recv_timeout`, so it read the persist emitted
      during *priming* rather than the `StreamInit` one it asserts about — it was never
      observing its own subject. Widened to `sync_channel(16)` +
      `drain_conversation_persist_events`, the pattern already used verbatim at
      `history_model_test.rs:4392`/`:4448`. The priming persist comes from
      `AIConversation::update_for_new_request_input`, which is **fork-only** (the pin's ends
      at `Ok(())`, checked directly) and deliberate — now recorded in `DECLINED.md` so a
      future sweep does not revert it to pin shape.
      All three stale "This currently FAILS" doc comments were rewritten; 0 remain.
      Original entry:
      🔴 **FOUR OF THE FIVE ARE REAL: the suite is RED on four counts.** Checked
      2026-08-18. These are **live `#[test]`s with no `#[ignore]`**, added in uncommitted
      work — so they will fail the moment anything runs them.
      • **`:3898` `test_restore_conversations_indexes_child_by_parent_agent_id` — ONE-LINE
        FIX.** `history_model.rs:1135` indexes with a bare
        `conversation.parent_conversation_id()` field read; the pin
        (`42effe840:history_model.rs:1145-1149`) calls
        `resolved_parent_conversation_id_for_conversation`. **The fork already HAS that
        resolver** at `history_model.rs:563` — it is simply not called here.
      • **`:4639`** `initialize_output_for_response_stream` updates both indices and emits
        `ConversationAgentIdAssigned` but never persists. Needs the pin's
        `should_persist`/`should_emit_server_token_assigned` hoist out of the borrow
        (`42effe840:history_model.rs:1388-1418`).
      • **`:3735` and `:3800`** need the pin's **two-pass** `initialize_historical_conversations`:
        pass 1 builds `HistoricalConversationRow`s seeding **every** row via
        `agent_id_key_from_persisted_data`, pass 2 resolves via
        `resolved_parent_conversation_id_from_persisted_data`. Neither helper exists here;
        `conversation_loader.rs:231-386` is still one pass and seeds run-ids only inside the
        `parent_conversation_id.is_some()` branch, so a child with only a `parent_agent_id`
        never seeds its parent.
      • **`:4731` was stale and is now corrected** — `assign_run_id_for_conversation` gained
        its `persist_conversation_state` call earlier today.
      All four surviving blocks also cite **drifted line numbers**; left alone deliberately
      under a comment-only brief. Original entry:
      **Five more `/// This currently FAILS` doc blocks in `history_model_test.rs` are
      unverified.** At `:3735`, `:3800`, `:3898`, `:4639`, `:4731`. Three siblings in the
      same file were corrected on 2026-08-18 after the gaps they described were fixed —
      these five were out of that item's scope and nobody has checked whether they are
      still accurate.
      **Why this matters more than tidiness:** a doc block claiming a test fails, on a test
      that now passes, is the same #148 failure mode that has produced seven wrong entries
      in this file — it sends the next reader looking for a defect that is not there. And
      the reverse is worse: if one of the five *is* still failing, it is an unfixed defect
      currently reading as documented-and-handled. Check each against the tree.

- [x] **FIXED 2026-08-18 (log surface). UNVERIFIED (not compiled).** New
      `app/src/settings/settings_file_diagnostics.rs` (+9 tests), a
      `SettingsManager::public_settings_file_paths()` that rejoins `hierarchy` + `toml_key`
      into the full dotted path, and calls in `init()` and after each successful hot-reload.
      **Root cause worth recording:** the loader is *pull-based* — it iterates registered
      settings and asks the backend for each — so nothing ever walked the file. **The pin
      has no such diagnostic either**, so this is a fork addition, not a port.
      Warns, never errors, as intended: dead keys are the expected state for anyone
      migrating a Warp `settings.toml`. The walk descends only into tables that are a proper
      prefix of a known path and stops at a known path, so a structured setting's inner keys
      are not false-positived. **Anti-spam matters more than it looks** — the app rewrites
      `settings.toml` on every setting change, so the reload path fires constantly; a
      process-global compares and records the unknown-key set under the lock before logging,
      so only a *change* re-emits.
      **Known false positive, documented in-module and in the warning text:** `#[cfg]`-gated
      groups (e.g. `LinuxAppConfiguration`) are not compiled on other platforms, so their
      keys read as unknown there. Feature flags do NOT cause this — `feature_flag:` only
      feeds the JSON schema; registration is unconditional.
      **In-app surface deliberately NOT added:** it would need an `UnknownKeys` variant on
      `SettingsFileError`, which breaks exhaustive matches in four files, two of them held by
      live agents. Reusing `InvalidSettings` would have produced a wrong message. Stopped at
      the log and left `last_unknown_settings_file_keys()` as the ready accessor. Original entry:
      🔴 **Settings.toml silently ignores EVERY unrecognised key, not just execution
      profiles.** Confirmed 2026-08-18: there is **no unknown-key diagnostic anywhere** in
      `crates/settings/src` or `app/src/settings`. A user who typos a setting, or writes one
      this fork dropped, gets **no error and no effect** — the file loads clean and the
      setting does nothing.
      This surfaced while investigating `[agents.execution_profiles.*]` being ignored, but
      that is a symptom, not the disease. **Fix: report unrecognised keys from the
      settings-file loader.** One change fixes the silence for all keys at once and is
      independent of any individual feature decision.
      Worth weighing: this fork has *dropped* many upstream settings, so a user migrating a
      Warp `settings.toml` will have dead keys — which is exactly when a diagnostic is most
      valuable and most likely to be noisy. Warn rather than error.

- [x] **ALL THREE RESOLVED 2026-08-18.** (a) **Tab/pane metadata-copy menu BUILT** —
      `copy_metadata_menu_items` in `app/src/tab.rs`, all five labels, per-surface
      asymmetry matching the pin, 3 unit tests, and the 3 integration tests ported.
      (b) **File-backed execution profiles DECLINED** — and the sibling rows' rationale
      was corrected: the pin has no `AIExecutionProfileObject` in `execution_profiles`
      at all, so the fork is sitting on upstream's *rollback* path, not diverging
      deliberately. Declined on better grounds (redesign not port; must be authoritative
      to pass the hot-reload test; ~700 lines of cloud-migration machinery).
      (c) **Inline model selector park/restore BUILT** — flag, `prompt_parked_for_search`,
      `open_model_selector_and_snapshot_prompt`, all three dismissal routes, 3 tests ported. Original entry:
      **Three parity gaps found by the integration-test sweep (2026-08-18). None are
      defects — all are absent features, ledgered MISSING-SUBSYSTEM.**
      **(a) Tab/pane context menu has no metadata-copy items.** Right-clicking a tab or a
      vertical-tab pane offers no *Copy tab title / Copy pane title / Copy branch / Copy
      working directory / Copy pull request link*. Present at the pin
      (`app/src/tab.rs:418` `copy_metadata_menu_items` / `push_copy_metadata_menu_item`),
      ~110 lines, **pure-local** — reads `current_git_branch`, `pwd`,
      `current_pull_request_url`. Three pin tests cover it. The most user-visible of the
      three and the cheapest to build.
      **(b) File-backed execution profiles absent.** `[agents.execution_profiles.*]` in
      `settings.toml` is **silently ignored** — no `ai/execution_profiles/config.rs`, no
      `FeatureFlag::FileBackedExecutionProfiles`. Silent is the problem: a user editing
      settings.toml gets no error and no effect.
      **(c) Inline model selector does not park/restore the prompt.** Typing a prompt then
      opening the model chip filters the model list *by the prompt text* instead of
      clearing and restoring it. Old behaviour, not destructive.

- [x] **DONE 2026-08-18 — comment-only diff; no assertion, body or attribute touched.**
      All three premises re-verified against the tree first. Also dropped a dead function
      name and two line-number citations that had already drifted. Original entry:
      **Three test doc comments now describe fixed gaps.** `history_model_test.rs:3939`,
      `:4020`, `:4265` still say "This currently FAILS" and name constructors/persistence
      calls that landed 2026-08-18. **Documentation only — the assertions are correct and
      unweakened.** Left alone deliberately rather than silently rewriting another agent's
      uncommitted test prose. Update once the tree settles.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** `app_state.rs` now passes
      `OpenVerticalTabsPanel` and the stale comment is gone. Already covered by
      `workspace/view_test.rs:2491-2494` (called twice, stays open). Original entry:
      **Surfacing the vertical-tabs panel currently CLOSES it when already open.**
      `app/src/local_control/handlers/app_state.rs:202` dispatches
      `WorkspaceAction::ToggleVerticalTabsPanel` for the `SurfaceVerticalTabsOpen` request,
      because until 2026-08-18 the open-only variant did not exist here. The pin passes
      `OpenVerticalTabsPanel` (`42effe840:app_state.rs:192`). **The variant now exists** —
      switch line 202 and delete the stale comment at `:199-202` saying it does not.
      This is the surfacing bug the variant was added to fix.

- [x] **DONE 2026-08-18.** `app/src/menu.rs:39` is now `pub(crate) const`, matching
      `42effe840:app/src/menu.rs:40`. The duplicate `NEW_SESSION_MENU_VERTICAL_PADDING`
      mirror in `workspace/view.rs` can now be deleted in favour of the shared const.
      Original entry:
      **`MENU_VERTICAL_PADDING` is private where the pin has it `pub(crate)`.**
      `app/src/menu.rs:39` vs `42effe840:app/src/menu.rs:40`. The new-session-menu work had
      to mirror the value as `NEW_SESSION_MENU_VERTICAL_PADDING` in `view.rs` with a
      keep-in-sync comment rather than reach into another module. One-word visibility change
      brings the fork to parity and deletes the duplicate.

- [x] **FIXED 2026-08-18 — and the correction is bigger than expected.** `script/state`
      now runs one `extract_test_map` against **both** the pin and HEAD, which greps
      `#[test]`, **subtracts** placeholder names read out of `macro_rules!` bodies (matched
      per `(name, file)` pair so a real test sharing the name survives), and **adds**
      `register_test!(..)` plus the expansions of the generating macros.
      **Measured delta, isolated at the same HEAD and ledger:**
      pin **10860 -> 11228** (+368), fork **10210 -> 10568** (+358), shared 8076 -> 8406,
      absent 2784 -> 2822. Contributions: `register_test!` 319 pin / 305 fork;
      macro-generated 53 pin / 57 fork. **The percentages barely move** (+0.2pp / -0.3pp)
      because the newly-visible tests split roughly proportionally between shared and
      absent — but the absolute counts everyone has been quoting were short by ~370 a side.
      **It found a FOURTH generating macro** nobody had spotted: `integration_tests` in
      `crates/integration/tests/common/mod.rs`. A brace-tracking scan now reports **every**
      `macro_rules!` whose body contains `#[test]` and warns — to stderr and into
      `docs/STATE.md` — about any not in `MACRO_TEST_ALLOWLIST`, so a fifth gets noticed
      instead of silently missed. The warning path was tested by removing allowlist entries.
      Also fixed a leak: the trap now cleans `$TMP.*`, which had been dropping `.pinmap` /
      `.absent` files into `/tmp` on every run.
      **Note on 305 vs 323:** `script/state` measures **HEAD**, as every number in it always
      has; 18 `register_test!` lines are still uncommitted. The fork total will rise again
      when they land. The HEAD-vs-worktree convention was deliberately not changed. Original entry:
      🔴 **`script/state` under-counts tests TWO ways. Fix the extractor.**
      **(1) Fully invisible: `crates/integration`.** It registers via
      `register_test!(name)`, so every `#[test]`-shaped extractor skipped the whole
      crate. Pin registers **319**, fork **306**; 33 were absent and none were
      ledgered. **All 33 are now adjudicated** — 6 ported, 5 portable-but-blocked on
      app-side helpers, 11 missing-subsystem, 6 declined (Warp's private GCP fixture),
      4 cloud, 2 declined, 2 covered-elsewhere. `register_test!` appears **nowhere
      else** in the repo, and no `rstest`/`test-case`/`datatest`/`googletest`
      dependency exists in any manifest — so this crate was the only total blind spot.
      **(2) Partially invisible: three `macro_rules!` that emit `#[test] fn $name()`.**
      The literal `#[test]` sits in the macro body, so only the metavariable is visible
      where names are supplied. **~57 real tests collapse to ~4 placeholder names, on
      BOTH sides of the comparison:**
      `app/src/terminal/ref_tests/mod.rs:23` `ref_tests!` (**39**),
      `app/src/terminal/input_test.rs:5992` `input_mode_prefix_tests!` (**8**),
      `crates/warpui_core/src/elements/scrollable_test.rs:30`
      `define_axis_agnostic_tests!` (5 invocations x 2 = **10**).
      **Fix:** extract names from `register_test!(..)` and from those three invocation
      lists, not just `#[test] fn`. Until then every parity percentage derived from
      `script/state`'s 10,860 pin total is slightly wrong.
      Original entry:
      🔴 **`script/state` and every ledger sweep are BLIND to `crates/integration`.**
      That crate registers with `register_test!(name)`, not `#[test]`, so every extractor
      built so far has silently skipped it. Measured 2026-08-18: the pin registers **319**,
      this fork **306**, **33 are absent here and NONE were in the ledger.** They have now
      been added as `UNPARSED` rows and are being adjudicated.
      **The blind spot is the real finding, not the 33.** `script/state`'s pin test total
      (10,860) is understated by the same construction, so every parity percentage derived
      from it is slightly wrong in the fork's favour. **Fix `script/state` to count
      `register_test!` as well as `#[test]`,** and check whether any other crate uses a
      non-`#[test]` registration macro — an agent is checking that now.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** `DragResult::HandoffNeeded`
      replaced wholesale by the pin's `ReorderInSource`, with `reordering_in_source` on
      `ActiveDrag` and the drop-time `DropInto` branch. **Safe to replace rather than
      add alongside:** it had exactly one construction site and one match arm, both
      guarded to the source-window case, so nothing else could regress.
      **One correction to my framing:** the fork had ALREADY ported the deferred-ghost
      model (`DragPhase::GhostInTarget`, `DropResult::DropInto`) — the live transfer
      survived only on the back-to-caller branch. Narrower gap than I described; the
      consequence was exactly as described. `cancel_drag()` was at `:1659`, not `:1695`.
      The new entry condition is **stricter than the fork's**: the pin's
      `target.window_id == source_window_id && has_dedicated_preview_window() &&
      !source_placeholder_consumed`, versus the fork's bare caller-window check.
      `perform_handoff` verified **byte-identical to the pin** and untouched; the other
      four `transfer_view_tree_to_window` sites are unchanged, and `reverse_handoff` is
      now reachable only at drop time — exactly the pin's reachability.
      `collapsed_source_placeholder_index` and `set_reordering_in_source_for_test` now
      key off the flag instead of the `InsertedInTarget { target == source }` shim, so
      both match the pin verbatim; existing unit tests unmodified and unaffected.
      The module header's state diagram was stale **before** this change (it still
      described the pre-ghost all-live-transfer model) and is now corrected.
      Traced against `test_multi_tab_drag_back_to_source_and_out_again`: it should now
      pass with `--features drag_tabs_to_windows`, which CI does not pass
      (`pr-check.yml:603`). Original entry:
      🔴 **Cross-window tab put-back orphans the drag gesture.** The fork does not merely
      *encode* source-reorder differently from the pin — **it removed the deferred put-back
      entirely.** Pin: `DragResult::ReorderInSource` keeps the tab `Floating` with
      `reordering_in_source = true` and no view-tree transfer
      (`cross_window_tab_drag.rs:998-1070` @ pin). Fork: `DragResult::HandoffNeeded` does a
      **live transfer** into `InsertedInTarget` (`:1001-1006`), and dragging back out runs
      `on_drag_while_inserted` → `reverse_handoff` →
      **`transferred_tab.draggable_state.cancel_drag()`** (`:1695`) — which is verbatim the
      failure the pin's test was written to catch: *"cancelled the drag's shared
      `DraggableState` — orphaning the gesture… leaving the preview window stranded."*
      **The ported test `test_multi_tab_drag_back_to_source_and_out_again` will go RED the
      moment `drag_tabs_to_windows` is enabled in CI.** It skips today because that feature
      is not in `crates/integration`'s defaults, so `pr-check.yml:603` never runs it — the
      same reason its four registered siblings skip. Fix lives in
      `app/src/workspace/cross_window_tab_drag.rs` + `workspace/view.rs`.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** The gate was **moved, not deleted**:
      the item is recorded unconditionally and `show_agent_notifications` now wraps only the
      `send_telemetry_from_ctx!(AgentNotificationShown)` emission. That is the right place
      because **the pin has no telemetry call here at all** — the emission is fork-added, so
      it could not simply vanish. Display suppression already existed and is untouched at
      four sites. Row re-homed to `app/src/notifications/model_tests.rs` and the test ported. Original entry:
      **`add_notification` drops items entirely — mechanism now confirmed.** Fork
      `app/src/notifications/model.rs:410-412` early-returns when
      `show_agent_notifications` is false; the pin
      (`agent_management_model.rs:454-482`) has **no such check** and always records the
      item, suppressing only at the display layer. So unread state is destroyed rather than
      hidden — this is the `a530563eb` defect, and it is why the ledger test
      `add_notification_tracks_unread_activity_when_in_app_notifications_are_hidden` cannot
      pass. That row also needs **re-homing**: `app/src/ai/agent_management/` no longer
      exists (deleted by `002ce4671`), so its `pin_file` points at a dead path. Two further
      re-expressions when it is ported: fork `NotificationSourceAgent::Oz` is a **unit**
      variant (pin: `Oz { is_ambient: bool }`), and fork `add_notification` takes 8 params
      with no `branch` (it derives it via `resolve_git_branch_for_terminal_view`).

- [x] **DECIDED 2026-08-18: declined, `DECLINED.md` row written.** The fork ships the
      capability as an explicit "Clear key · <provider>" row; the pin uses a contextual
      ctrl-x. Ledger row DECLINED with a `test:` marker. Original entry:
      **DECIDE: ctrl-x inline-menu clear is a UI divergence, not a missing capability.**
      The ledger row `input_cut_binding_yields_ctrl_x_to_contextual_menu_clear` looks like
      absent debt but **the fork already ships the capability by a different affordance** —
      an explicit *"Clear key · &lt;provider&gt;"* row accepted with Enter, deliberately modelled
      on `mcp_menu`'s "Log out" row and documented at `api_keys_menu.rs:60-62`. The pin
      instead puts a contextual **ctrl-x** on the provider row.
      Porting the pin's affordance means porting the pin's **redesigned api-keys menu** —
      different states (`Browsing`/`EditingProvider`/`ConnectingGrok` vs the fork's
      `List`/`EnteringKey`), different row kinds, `ApiKeyManager` persistence, Grok token
      clearing — plus `inline_menu.rs` `can_clear_selected`/`clear_selected` which the
      original scoping missed entirely. Landing only the in-boundary half would give a flag
      **no menu can ever set** — a mechanism with no producer, guarded by a test asserting a
      carve-out that can never engage, which is exactly what `check_stub_coverage` exists to
      catch. **Recommend a `DECLINED.md` row recording the divergence** rather than porting.

- [x] **SWEPT AND CLOSED 2026-08-18 — 1 genuine orphan in 257 pin files, already
      fixed, no open ledger row affected.** I filed this as a concern and then
      measured it rather than leaving it open.
      **Method:** for all 257 distinct `pin_file` values in the ledger, checked (a)
      the file exists at `42effe840` and (b) something at the pin `mod`-declares it
      or `#[path]`s to it. **0 are absent from the pin.**
      **Only real orphan: `app/src/terminal/writeable_pty/pty_controller_tests.rs`** —
      `42effe840:pty_controller.rs:820,823` declares only `command_bytes_tests` and
      `lifecycle_tests`, and nothing else in the tree declares it. Those tests have
      not compiled upstream since the `PtyController` rewrite. **This fork is AHEAD**:
      it declares `mod tests` (`pty_controller.rs:893`) and maintains the file against
      the live API. Its 6 rows are now all resolved (2 corrected today, 4 already
      DIVERGENT/COVERED-ELSEWHERE).
      **Three initial hits were false positives of my own crude check**, worth
      recording so the next person does not re-raise them: `queued_prompts_tests.rs`
      is declared at `app/src/terminal/view.rs` and `convert_tests.rs` at
      `app/src/workspaces/gql_convert.rs` — my grep was scoped to each file's own
      directory and missed declarations in a parent; and `crates/http_client/src/lib.rs`
      is a crate root, which needs no `mod` declaration at all. Likewise
      `crates/integration/src/test/workspace.rs` is declared at
      `crates/integration/src/test.rs:41` — so the PORTABLE row against it is sound.
      Original entry:
      **Ledger rows sourced from a pin file that is DEAD UPSTREAM.** Verified
      2026-08-18: `42effe840:app/src/terminal/writeable_pty/pty_controller.rs:820,823`
      declares only `command_bytes_tests` and `lifecycle_tests`, and
      `writeable_pty/mod.rs` declares nothing either — so the pin's
      `pty_controller_tests.rs` **has not compiled or run upstream** since the
      `PtyController` rewrite. **This fork is AHEAD**: it declares `mod tests`
      (`pty_controller.rs:893`) and maintains the file against the live API.
      Six ledger rows cite that pin file. Two were mis-filed `PORTABLE` and are now
      corrected; the other four (1 DIVERGENT, 3 COVERED-ELSEWHERE) happen to be
      harmless. **The general risk is the point:** any ledger row whose `pin_file` is
      dead upstream is asserting parity against something that does not build there.
      Worth a sweep — for each distinct `pin_file` in the ledger, check the pin actually
      `mod`-declares it. Cheap to script, and it would have caught this in one pass.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** Every `SSHValue` test construction
      audited tree-wide. Two were inert and are fixed with a real non-zero
      `remote_session_id`; the two DCS parser tests were never affected (they assert on
      parsing and already pass a real id).
      **One was worse than silently passing:** `zero_state_block_tests.rs:108`
      (`cwd_for_recent_conversations_does_not_use_startup_path_for_pending_ssh_bootstrap`)
      — with the hook dropped, `pending_legacy_ssh_session` stayed `None`, the startup-path
      fallback fired, and its `assert_eq!(cwd, None)` **should have been failing**. The fix
      restores the intended state.
      **The pin has the same latent bug:** `42effe840:terminal_model_tests.rs:412` also
      passes a bare `SSHValue::default()` against a byte-equivalent guard, so the SSH half
      of that test is vestigial upstream too. Our fix is a deliberate divergence —
      strengthening, not weakening. No production guard was touched. Original entry:
      **`SSHValue::default()` is now inert in tests — a side effect of today's #532 work.**
      `TerminalModel::ssh` (`app/src/terminal/model/terminal_model.rs:3236`) now rejects a
      hook whose `remote_session_id` is `None` or `0`. Correct and deliberate — but it
      means `ssh_bootstraps_if_blocklist_empty_and_reconciles_parent_return` **no longer
      exercises the SSH hook at all**, in either its old or new form:
      `pending_legacy_ssh_session` stays `None` and `init_shell` drives the nested-shell
      epoch alone. Its assertions are unaffected, so it still passes — which is exactly
      why this needs recording rather than discovering later. Any other test that builds
      an `SSHValue` by `default()` is in the same position; worth a grep.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** `CancellationOutcome` +
      `CancellationReason::conversation_outcome()` ported from the pin; `mark_request_cancelled`
      now consults it, so `Succeeded` stamps `ConversationStatus::Success` and
      `cli_controller.rs:469`'s comment is finally true. Two pin variants deliberately not
      added (`AutomaticCloudHandoff` dropped-cloud, `CLISubagentUserTakeover` absent).
      **Behaviour delta, flagged rather than buried:** `Reverted` now also counts as success
      at `controller.rs:569` — the pin's behaviour, and consistent with the fork's own
      `mark_response_stream_cancelled`, which already diverts `Reverted` to `Success`.
      **Three pin divergences deliberately left in place**, each a separate behaviour change:
      `action_model.rs:1403` still gates on follow-up rather than the `Cancelled` outcome;
      `output.rs:1201` still renders the stopped banner for `Succeeded` reasons (the pin
      suppresses for `KeepInProgress | Succeeded`); and `controller.rs` lacks the pin's early
      returns for `FinalizedExternally` and `KeepInProgress`. Original entry:
      🔴 **An LRC that completes while the agent is still streaming reports
      "Cancelled" instead of "Success".** Found 2026-08-18. `cancel_conversation_progress`
      with `OptimisticCLISubagentCompletion` routes through
      `try_cancel_streams_for_conversation` -> `ResponseStream::cancel` ->
      `mark_response_stream_cancelled` -> `AIConversation::mark_request_cancelled`
      (`app/src/ai/agent/conversation.rs:2207`), which stamps `Cancelled` unless the
      reason is follow-up-same-conversation or `AgentExitedShell`.
      `OptimisticCLISubagentCompletion` is neither.
      **The fork's own call site documents the behaviour it does not get** —
      `blocklist/block/cli_controller.rs:469`: *"Mark conversation as successfully
      completed BEFORE exiting agent view. The command finished naturally, so this is a
      successful completion."* — immediately before the call that stamps Cancelled.
      The existing `is_lrc_command_completed` -> `Success` mapping
      (`controller.rs:535-573`) covers only the **finished-actions** path, not the
      in-flight-stream path. **Fix:** port the pin's
      `CancellationReason::conversation_outcome() -> CancellationOutcome`
      (`42effe840:app/src/ai/agent/mod.rs:191`) — neither symbol exists here — and have
      `mark_request_cancelled` consult it, replacing the ad-hoc
      `is_follow_up_for_same_conversation` / `is_agent_exited_shell` /
      `is_lrc_command_completed` predicates. Unblocks 1 ledger row.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** Added the `ctx`-taking
      `set_server_conversation_token_for_conversation_and_persist`, made the plain setter
      return `bool` and sync cached metadata (all 17 call sites use it as a statement, so
      none change), and added the private `update_cached_metadata_for_conversation`.
      **The actual fix for the already-failing in-tree test at `history_model_test.rs:4726`
      was a fourth change, not the wrapper:** `assign_run_id_for_conversation` now calls
      `persist_conversation_state` before emitting. Its `recv_timeout` now has a write to
      receive.
      **No production caller was rewired**, deliberately: the pin's two callers are both in
      `app/src/workspace/view.rs` and **neither site exists here** — this fork has no
      production fork-then-bind path at all. So the wrapper is currently test-only; that is
      a real observation, not an oversight, and worth deciding on separately. Original entry:
      🔴 **Conversation token / run-id binding never persists — lost on restart.**
      `set_server_conversation_token_for_conversation_and_persist` **does not exist**
      (zero hits over `app/` + `crates/`); the fork has only the non-persisting
      `set_server_conversation_token_for_conversation` (`history_model.rs:838`), which
      takes no `ctx` so it can neither persist nor emit. **Already causing a failing
      test in the tree** — `history_model_test.rs:4726` documents it: *"never calls
      `persist_conversation_state`, so the assigned run id is lost on restart."*
      **Not cloud:** the "handoff token" is a local index here, and the sibling
      `test_fork_then_bind_handoff_token_resolves_to_forked_conversation` is already
      ported and passing (`history_model_test.rs:4789`).
      **Fix:** add a `ctx`-taking wrapper around the existing private
      `persist_conversation_state` (`history_model.rs:605`), updating
      `all_conversations_metadata` and emitting `UpdatedConversationMetadata` +
      `ConversationAgentIdAssigned`. **One fix unblocks 2 ledger rows AND the
      already-failing in-tree test.** Note `AIConversationMetadata` has no
      `has_cloud_data` field (deliberately de-clouded), so that assertion drops.

- [x] **DISCHARGED 2026-08-18 — every `PORTABLE` row has now been re-verified and the
      bucket is empty.** The rule held up: across the audit, 60 of 95 rows were stale
      (already ported), and in individual shards 4 of 7 and 7 of 8 carried evidence read
      off the **pin's** file as if it described the fork. All are now adjudicated with
      fork-verified evidence and the ledger's open count is **zero**. The rule itself is
      worth keeping in mind for any future sweep: re-verify the production symbol in the
      FORK, by `fn <name>`-shaped grep, before treating a row as ready-to-port. Original entry:
      **The ledger's `PORTABLE` evidence is systematically unreliable — audit it, do
      not trust it.** Measured 2026-08-18 across two independent passes:
      • Of 95 `PORTABLE` rows, **60 were stale** — the test was already in the tree and
        the row had never been updated.
      • Of the 7 in one shard, **4 had FALSE evidence** — the cited symbol/line was read
        off the **pin's** file and recorded as if it were the fork's. Examples:
        *"`fetched_memories()` exists (`conversation.rs:1251`)"* — line 1251 of the
        FORK's file is inside `is_single_passive_exchange`, and the method has **zero**
        definitions fork-wide. *"`new_restored_synthesizing_on_empty` exists
        (`conversation.rs:443`)"* — that is the pin's `:475`; the fork's `:436` is the
        strict `new_restored`.
      • One row (`process_ai_queries_for_nld_history_match`) carried a note that
        **contradicted its own verdict** — "No nld_prompts SQLite read exists yet"
        beside `PORTABLE`.
      **Rule for anyone consuming this bucket:** re-verify the production symbol in the
      FORK, by `fn <name>`-shaped grep, before treating a row as ready-to-port. The
      pattern is the same #148 class that has now produced seven wrong `TODO.md`
      entries — an evidence string written while reading the oracle, filed as if it
      described here.

- [x] **CONSTRUCTOR PORTED 2026-08-18. UNVERIFIED (not compiled).** Both ledger
      tests ported. **But it is necessary, not sufficient — the two in-tree failing
      tests will NOT pass from this alone**, and the agent was precise about why:
      • **All three need** the call-site switch at
        `app/src/ai/blocklist/history_model/conversation_loader.rs:78` —
        `new_restored(..)` -> `new_restored_synthesizing_on_empty(..)`. That is the
        pin's **only** non-test call site of the lenient variant
        (`42effe840:conversation_loader.rs:92`). The other ~30 call sites correctly
        stay on strict `new_restored`.
      • **`:3939` and `:4265` additionally need** `start_new_child_conversation`
        (`history_model.rs:511-546`) to call `persist_conversation_state`; without it
        the first `recv_timeout` times out before the constructor is ever reached.
      • **`:4020` additionally needs a distinct one-line defect fix** — see below.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** Now writes `self.is_remote_child`,
      matching `42effe840:conversation.rs:3619`. Original entry:
      **`updated_conversation_state_event` persists `is_remote_child: false`
      unconditionally.** `app/src/ai/agent/conversation.rs:3370` hard-codes it while
      the restore path reads the flag back; the pin writes `self.is_remote_child`.
      One line, but a distinct defect — found while porting the lenient constructor
      and deliberately **not** landed silently, per AGENTS §5.10. It is what blocks
      `test_mark_conversation_as_remote_child_persists_updated_conversation_state`
      (`history_model_test.rs:4020`).
      Original entry:
      **Port `AIConversation::new_restored_synthesizing_on_empty` — blocks 4 tests, two
      of which are FAILING in the tree right now.** The fork has only the strict
      `new_restored` (`app/src/ai/agent/conversation.rs:436`), which returns
      `NoRootTask` on an empty task list. The pin (`42effe840:conversation.rs:475`) also
      has a lenient variant that synthesises an in-progress optimistic root instead.
      Blocks 2 ledger tests, and the absence is **already documented as the cause of two
      currently-failing tests** at `app/src/ai/blocklist/history_model_test.rs:3948`,
      `:4027`, `:4271` ("that constructor does not exist here").
      **Not a mechanical port:** the pin obtains it by refactoring the ~200-line
      `new_restored` into a `(task_store, todo_lists, status)` tuple with an empty-tasks
      branch, making strict `new_restored` a wrapper that returns `NoRootTask` first.
      Building blocks exist here — `Task::new_optimistic_root` (`task.rs:180`),
      `TaskStore::with_root_task` (`task_store.rs:30`). The strict-path sibling
      `new_restored_with_empty_task_list_returns_no_root_task_error`
      (`conversation_tests.rs:1086`) must keep passing. **Not** the declined #107 arity
      issue — that constructor is untouched by this.

- [x] **FIXED 2026-08-18 — `git_dialog` error handling and a credential-prompt hang.**
      **UNVERIFIED (not compiled).** Found while auditing the existing `git pull`.
      (1) `user_facing_git_error` (`app/src/code_review/git_dialog/mod.rs:167-255`)
      mapped **none** of pull's characteristic failures — all six fell through to
      "Git operation failed", including the most likely one. Note `fatal: Not possible
      to fast-forward` does **not** contain `non-fast-forward`, so the existing push
      arm never caught it. Six arms added, every key a literal captured from real git
      2.53 output rather than recalled: diverged, dirty-tracked, dirty-untracked,
      unmerged, mid-merge, missing remote ref. Tracked and untracked get separate copy
      on purpose — stashing does not move an untracked file out of the way.
      (2) `GIT_TERMINAL_PROMPT=0` added via a new pure `git_child_env(path_env)`
      (`app/src/util/git.rs:59-86`). Previously a credential-needing fetch blocked on
      an **invisible** prompt, and `GitDialogAction::Cancel` early-returns while
      `self.loading` — so the dialog was an **unrecoverable spinner**. The two are
      deliberately coupled: (2) alone would have traded a hang for "Git operation
      failed". Only *terminal* prompting is suppressed; `GIT_ASKPASS` and credential
      helpers still run, so the usual macOS/Windows credential managers are unaffected.
      **24 tests added where the function had zero.** Includes anti-shadowing guards in
      both directions, bare-vs-wrapped equivalence for SSH sessions, and
      case-insensitivity. The matcher chain was transcribed to Python and all 27
      assertions executed against the captured constants — not a substitute for
      `cargo test`, but it rules out the ordering bugs an if-chain of `contains` invites.

- [x] **DELETED — MAINTAINER DECISION 2026-08-18.** Both removed from
      `app/src/util/git.rs`; no callers existed and none were left behind.
      **The decisive argument was reach, not tidiness:** the primitives were
      **local-only** (`#[cfg(feature = "local_fs")]`, `repo_path: &Path`), while the
      shipping path emits a shell command through the context chip
      (`PromptChipShellCommand::{GitCheckout, GitCreateAndCheckoutBranch}` ->
      `terminal/input.rs:950-958`), so it runs in whatever shell you are in and
      therefore **works over SSH**. Routing the chip through the primitives would have
      broken branch create/switch on remote sessions.
      Also relevant: they were **fork-original** — `git grep 'fn run_create_branch|fn run_switch_branch' 42effe840` returns nothing — so there was no parity argument for
      keeping them. The one thing they offered (errors through the improved
      `user_facing_git_error`) is worth little here, since raw `git checkout` errors are
      already legible, unlike the `pull` failures fixed earlier today.
      A comment at the removal site records all of this so the next person does not
      re-add them; git history holds the bodies. Reinstate only alongside a
      dialog-based Git panel that has a remote story (Zap #329).
      Original entry:
      **DECIDE: delete `run_create_branch` / `run_switch_branch`, or route the chip
      through them?** `app/src/util/git.rs:904,928`, **zero callers**. Branch
      create/switch already ships for users as terminal commands via the context chip
      (`display_chip.rs:748`, `input.rs:950-958`, tested at `display_chip_test.rs:243`),
      so this is an unwired duplicate. Trade-off: the chip path is *visible* (the user
      sees the command and its output, which some prefer for git); the primitives are
      better-behaved (`run_switch_branch` deliberately omits `--force` so git refuses to
      clobber a dirty tree) and would share the improved error surfacing.
      **Agent recommends deleting them** — an unwired duplicate is how the
      "already exists / doesn't exist" confusion in this ledger keeps happening.

- [x] **FIXED 2026-08-18 — skill removed, matching upstream `6984bc390`.**
      **UNVERIFIED (not compiled).** All five premises confirmed literally before
      deleting, including `DEFAULT_REPO = "warpdotdev/warp"` and the `_ => Always`
      fallthrough. `resources/bundled/skills/feedback/` deleted (7 files);
      `is_feedback_skill_available` and its four now-unused imports removed from
      `workspace/mod.rs`; the dynamic-label override replaced with the plain
      description; `send_feedback` reduced to the single `open_url` against the
      fork's own tracker (`links.rs:14` -> `zerx-lab/warp`), byte-identical in shape
      to the pin's `view.rs:6698`. `bundled.rs` needed no change (there was no
      feedback arm to remove). Guards green: `check_stub_coverage` ok,
      `check_dangling_modules` ok (3250 modules).
      **No `DECLINED.md` row** — upstream deleted the skill three months before the
      pin, so removing it IS parity, not a divergence.
      **Small follow-ups left, all cosmetic and all blocked on other agents' files:**
      a stale comment at `workspace/view_test.rs:195`; the now-orphan i18n key
      `keybinding-desc-workspace-send-feedback-oz` in all three locales (no unused-key
      guard, Fluent does not error); and `CONTRIBUTING.md:57` still calls `/feedback`
      the fastest way to file. Deliberately NOT changed: the `/feedback` mentions in
      the launch modal and `zap-launch-contribute-description` — **the pin ships the
      same inconsistency**, so changing them would be a divergence, not a fix.
      Original entry:
      🔴 **"Send feedback" files Phosphor bugs into `warpdotdev/warp`, under the
      user's own GitHub credentials.** `resources/bundled/skills/feedback/` still
      ships; `SKILL.md:3` and `scripts/file_feedback_issue.py:16`
      (`DEFAULT_REPO = "warpdotdev/warp"`) direct the agent to file via the local `gh`
      CLI. The fork repointed its *fallback* URL to its own tracker
      (`app/src/util/links.rs:14`), but `Workspace::send_feedback`
      (`app/src/workspace/view.rs:5798`) only reaches that fallback when the skill is
      unavailable — and `has_any_ai_remaining()` is hardcoded `true`
      (`app/src/ai/request_usage_model.rs:198`), so **any user with AI enabled takes the
      skill branch**. Worse, the fork dropped the feedback arm from
      `activation_for_bundled_skill`, so it falls through to `_ => Always`
      (`app/src/ai/skills/bundled.rs:617`) — unconditionally active. Upstream DELETED
      this skill (`6984bc390`). Not in `DECLINED.md`. Fix: delete the skill dir,
      `workspace/mod.rs:75-87` and the dynamic-label override at `:1619-1623`, reduce
      `send_feedback` to the single `open_url` line. ~60 Rust + ~1,100 resources.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** Both overrides added.
      `PaneGroup::child_view_ids` (`app/src/pane_group/mod.rs`) reports
      `user_default_shell_changed_banner` **only** — `share_block_modal`,
      `share_session_modal` and `shared_session_role_change_modal` deliberately
      omitted as removed-cloud, verified independently rather than taken on trust: the
      struct carries the removal comments at `mod.rs:818`/`:822`, an exhaustive scan
      shows **exactly one** `ViewHandle` field, and the render path's matching branches
      are gone too (`:6819`). A comment in the override records which pin entries were
      dropped and why. `PaneView::child_view_ids` (`pane/view/mod.rs`) transplants the
      pin's body unchanged — header plus every `pane_stack` view.
      Regression test `test_child_view_ids_reports_owned_but_unrendered_views` added,
      driving the **real** `PaneGroup`/`PaneView` (not a mock) with a non-empty guard
      so the loop cannot pass vacuously. Against the default `Vec::new()` impl every
      assertion fails, so it would have caught this.
      **The lesson, which is worth more than the fix:** `6f2a5afcd` ("test(warpui_core):
      port test coverage from the pinned oracle", 2026-08-07) landed the
      `child_view_ids` infrastructure **and its tests** with no production overrides.
      **A ported test suite that defines its own mock implementors cannot detect a
      missing production override** — the tests passed against the mocks for eleven
      days while the real types returned nothing. Any future infra port should carry
      an explicit check that real implementors exist.
      Original entry:
      🔴 **CRASH CLASS: view-transfer is inert — `child_view_ids` has no overrides in
      `app/src`.** `crates/warpui_core` has the whole machinery (trait method
      `core/mod.rs:285`, transfer walk `core/app.rs:3378`, tests), but
      `grep -rn child_view_ids app/src` returns **nothing**, where the pin has two
      overrides (`pane_group/mod.rs:8090`, `pane_group/pane/view/mod.rs:459`). The
      default impl returns `Vec::new()`, so the walk finds nothing — while the fork
      calls `transfer_view_tree_to_window` in five places (`root_view.rs:565`,
      `cross_window_tab_drag.rs:1464/1536/1616/1694`). The pin's own comment names the
      consequence: orphaned backing views that **"later trip a 'circular view
      reference' panic when accessed from its new window."** Infrastructure arrived
      2026-08-07 via `6f2a5afcd` "port test coverage from the pinned oracle" — the
      tests pass against their own mock and the production overrides never followed.
      Untracked until now. ~25 lines (only `user_default_shell_changed_banner` of the
      pin's four entries applies; the other three are physically-removed cloud modals).

- [x] **FIXED 2026-08-18 (setting + both guards). UNVERIFIED (not compiled).**
      **Both guards confirmed short-circuiting, traced end to end rather than inferred:**
      `AgentView` is not in `FORCE_DISABLED_FLAGS`; `FLAG_STATES` is populated from
      `enabled_features()` which includes it under `#[cfg(feature = "agent_view")]`
      (`lib.rs:3113`); and `agent_view` sits in `app/Cargo.toml`'s `default` block at
      line 594. So `!FeatureFlag::AgentView.is_enabled()` was `false` in every default
      build and both sites were dead — `select_most_recent_blocks` never called
      `focus_terminal`, and `has_block_or_text_selection_in_shell_mode` was always false.
      (The line numbers in the entry had drifted; the real sites were `view.rs:18843`
      and `:20029`.) The **"(code-reading inference)"** qualifier can now be dropped.
      Added `preserve_input_focus_on_block_selection` to `block_list_settings.rs`
      (default `false`, `general.preserve_input_focus_on_block_selection`), rewrote the
      subscription to the pin's shape, and swapped both guards. Two regression tests
      added in `view_test.rs`.
      **Widget half deliberately skipped, and should stay skipped this round:** two
      other agents are mid-change in `features_page.rs`/`settings_view/mod.rs`, and more
      decisively this fork i18n's **every** label on that page — landing it properly
      needs new `warp.ftl` keys in `en`, `zh-CN` and `ja`. Copying the pin's raw string
      would be an i18n regression. The setting works from `settings.toml` today and the
      defect is fixed regardless, since the default is `false`. Original entry:
      🔴 **Block navigation (up/down arrows) is dead in shell mode.** The pin replaced
      two `!FeatureFlag::AgentView.is_enabled()` guards with a
      `preserve_input_focus_on_block_selection` setting (`a8df31722`). The fork still
      has the pre-commit shape (`app/src/terminal/view.rs:18734` and `:19920`), and
      `agent_view` is a **default cargo feature** (`app/Cargo.toml:594`), so both
      guards short-circuit and `focus_terminal` is never called on block selection.
      This is upstream bug #10095. ~120 lines. (Code-reading inference, not executed.)

- [x] **FIXED 2026-08-18 (freeze half). UNVERIFIED (not compiled).** `read_link` now
      runs **only** on the `/mnt/<drive>` arm of `convert_wsl_to_windows_host_path`
      (`crates/warp_util/src/path.rs`), matching the pin's structure. The fork had it
      *after* the match, on the merged result, so the `\\WSL$\{distro}` UNC arm forced a
      9p round-trip into the distribution on **every** path conversion — the freeze.
      **The two other halves are BLOCKED, both on territory not code:**
      `canonical_directory_key` lives in `app/src/workspace/tab_settings.rs` and its
      caller updates fan into four more files, three in live-agent zones.
      `get_root_for_canonical_path` would be dead code without its only upstream caller,
      and the fork has drifted the sibling API badly — `get_root_for_path` takes
      `&Path` vs `&LocalOrRemotePath` inconsistently across ~20 call sites, with two
      in-tree comments already flagging the seam. A verbatim port would not compile.
      **Test caveat, stated plainly:** the added assertion only bites on a Windows host
      that actually has a WSL distro installed; without one `read_link` fails and the
      test passes either way. A platform-independent regression test is not possible —
      the bug is a filesystem round-trip cost and the UNC prefix is hard-coded.
      **⛔ The `#12492 REFUTED ... COORDINATOR-VERIFIED` block above should be struck.**
      Its cited evidence is all genuinely present — but it verified the *memoization*
      half and concluded the whole commit was ported. The `read_link` restructure was
      absent until now. The moral it drew (cross-area findings are hypotheses) now cuts
      both ways: **the owning agent's refutation was the wrong one.** Original entry:
      🔴 **The WSL UI freeze is STILL PRESENT — and `TODO.md`'s "#12492 REFUTED, do not
      re-raise", marked COORDINATOR-VERIFIED, is WRONG.** The memoization half it cites
      (`LocalSessionCanonicalPwdCache`) is present, but **the actual fix is not**: at
      the pin `crates/warp_util/src/path.rs` calls `std::fs::read_link` **only** on the
      `/mnt/<drive>` arm; the fork still calls it on both arms including the
      `\\WSL$\{distro}` UNC branch (`crates/warp_util/src/path.rs:500-506`) — exactly
      the 9p round-trip the PR names. Two further halves (`canonical_directory_key`,
      `get_root_for_canonical_path`) are also absent. The freeze fix itself is ~15
      lines. **This is the seventh entry found stating the opposite of the code.**

- [x] **DONE 2026-08-18 (unverified, not compiled). ALL THREE REQUIREMENTS MET.**
      (1) tab groups render, drag, collapse, rename, recolour and hit-test on **both** the
      horizontal and vertical bars; (2) `grouped_tabs` is back in the default cargo features
      (`app/Cargo.toml:654`); (3) the toggle is `appearance.tabs.enable_tab_groups`, default true,
      live on change. Steps A-L and the toggle all landed; see the execution log below.
      **Nothing is compiled** — this is the largest single body of unverified work in the tree and
      the build is the real test of it. Original entry:
      🔴 **SHIP TAB GROUPS FOR v0.1.0 — MAINTAINER DECISION 2026-08-18, reversing the
      deferral.** Required: (1) tab groups **fully working**, i.e. actually rendered, not just
      persisted; (2) `grouped_tabs` **back in the default cargo features** — done, restored at
      `app/Cargo.toml:654`; (3) a **user-facing toggle to turn it off**.
      The `DECLINED.md` row was deleted rather than left standing, so nothing in the tree still
      says this is declined.
      **Scope is the full port, both bars** (~2,700 lines): horizontal-only would leave the exact
      same invisible-state anomaly for anyone enabling vertical tabs, which is what made the
      half-present state unacceptable in the first place.
      **Toggle design** (settled by reading the tree, not guessed): add a `TabSettings` bool
      `enable_tab_groups` at `toml_path = "appearance.tabs.enable_tab_groups"`, `default: true`,
      alongside its siblings in `app/src/workspace/tab_settings.rs`; bridge it into the existing
      flag with `FeatureFlag::GroupedTabs.set_user_preference(..)`. That bridge is the reason to
      prefer it over gating each call site: `FeatureFlag::is_enabled` consults
      `USER_PREFERENCE_MAP` **before** `FLAG_STATES`, so **every existing guard picks the toggle
      up unchanged** — and all entry points are already guarded (`view.rs` "+" dropdown and
      `SelectNewSessionMenuItem`, `tab.rs:333`, three keybindings in `workspace/mod.rs`, the
      snapshot write and read). Precedent: `SSHTmuxWrapper` at `app/src/lib.rs:2128` — but that
      one reads a hidden `private_user_preferences` key, so it is the *mechanism* precedent, not
      the UI precedent. Must re-apply on `TabSettingsChangedEvent`, not only at startup.
      The ordered port plan (steps A-L, sized from the pin) is retained below as the work list.

      **EXECUTION STATE (updated as waves land).** `app/src/workspace/view.rs` is the bottleneck —
      ~1,540 of the ~1,945 horizontal-parity lines land there — so view.rs steps are serialized
      into waves, one owning agent at a time. Everything else runs in parallel.
      - **TOGGLE — DONE 2026-08-18 (unverified, not compiled). Requirement (3) is MET.**
        Setting at `app/src/workspace/tab_settings.rs:482` — `enable_tab_groups`, `default: true`,
        `toml_path: "appearance.tabs.enable_tab_groups"`, sibling idiom throughout.
        Bridge at `app/src/settings/init.rs:301` + subscription at `:303`, helper
        `apply_tab_groups_setting_to_feature_flag` at `:387`. **It landed entirely inside
        `settings/init.rs`** — no edit to `workspace/view.rs`, `workspace/mod.rs`, `tab.rs`, or even
        `lib.rs`, so the contended-file risk never materialised. Applies at startup **and** on
        `TabSettingsChangedEvent::EnableTabGroups`, so no restart is needed.
        **One flaw in my design, caught and fixed by the agent:** pushing `set_user_preference(true)`
        unconditionally would force-enable tab groups even in a build compiled **without** the
        `grouped_tabs` cargo feature, silently voiding the build gate — because
        `USER_PREFERENCE_MAP` outranks `FLAG_STATES`. The bridge now samples the flag's
        pre-preference state once (which reads `FLAG_STATES`, i.e. the cargo feature, since no
        preference is set yet) and pushes `available_in_build && enabled`, so **the setting can
        narrow what the build offers but never widen it**. Verified directly.
        The settings row is gated on `cfg!(feature = "grouped_tabs")` and **deliberately not** on
        `FeatureFlag::GroupedTabs.is_enabled()` — the toggle drives that flag, so gating the row on
        it would make the row vanish on first use with no way to turn it back on.
        i18n added for all three locales (`en`/`ja`/`zh-CN`); `t!` validates the key against the `en`
        bundle at compile time, so the en entry is load-bearing.
      - **Wave 1 — A, B, C ALL DONE 2026-08-18 (unverified, not compiled).**
        **C:** `tab_group_being_renamed` + 5 accessors in `app/src/workspace/util.rs`, wired into the
        same four call sites the pin touches.
        **A:** `TabBarHoverIndex::BeforeTab { index, group }` reshaped, plus `show_before_indicator`
        at `view/vertical_tabs.rs:1237`. **It was 11 call sites, not the 33 my brief claimed** —
        recorded so the number is not re-quoted. Two files I listed needed no change at all
        (`pane_group/pane/view/mod.rs`, `workspace/mod.rs` — they only name the type). ⚠️
        **`app/src/code/view.rs` has a DIFFERENT, unrelated `TabBarDragPosition::BeforeTab`** — do
        not sweep it into a grep for this enum.
        **PREMISE-FALSE in my brief:** no test anywhere constructs `TabBarHoverIndex`, so there were
        no test constructors to update. Three tests were *added* for `show_before_indicator` instead,
        including the boundary-vs-interior case at a shared index, which is the entire point of the
        new `group` field.
        **B:** all 11 variants in `workspace/action.rs` at the pin's positions. **All 11
        `handle_action` arms are STUBS** — each carries a `// STUB:` comment naming the helper step K
        must supply; none uses `todo!()`/`unimplemented!()`, so they log rather than panic. Two of my
        expectations were wrong: `blocked_for_anonymous_user` is a de-clouded `{ false }` with **no
        match to extend**, and `From<&WorkspaceAction> for LoginGatedFeature` is likewise gutted.
        `should_save_app_state_on_action` was the only exhaustive list, and each variant went into
        the same bucket as the pin. `DropGroup` may already be **complete** rather than stubbed — the
        pin's only other statement there is a telemetry send, and telemetry is dropped in this fork.
        One deliberate extra: the `DroppedOnTabBar` arm's `TODO(johnturcoo) inherit the tab group on
        pane drop` is now the pin's real body (inert until `refine_hovered_tab_index` lands in H).
      - **Wave 2 — F + G DONE 2026-08-18 (unverified, not compiled).**
        **F:** `TabBarSlot` (`view.rs:913`), `HorizontalTabGroupMouseStates` (`:906`) + field,
        `tab_bar_slots()` (`:23441`, gated on the flag **and** `tab_groups.contains_key` per the pin),
        `group_container_rect` (`:24387`), `rect_is_within_tab_bar` (`:24378`),
        `group_has_single_member` (`:24556`), and the three `*_group_position_id` helpers
        (`vertical_tabs.rs:156/161/168`). `tab_bar_rects_for_window` already existed at `:24371` and
        was confirmed byte-identical to the pin rather than re-added.
        **G:** the three group consts (`:24208-24214`), `render_horizontal_group_pin_indicator`
        (`:24219`), `render_group_member_icon_collage` (`:24243`), `select_unique_pane_kinds`
        (`:24327`), and widened `pub(super)` summary-icon renderers.
        **Existing vertical-tabs appearance is provably unchanged.** The pin *rewrites* the summary
        circle renderers onto `render_icon_with_status`, which looks visibly different; porting that
        would have restyled every vertical-tabs Summary row as a side effect of a machinery step.
        Instead the two functions were widened by wrapping, splitting `diameter` by the pin's own
        `SUMMARY_INLINE_ICON_RATIO = 2/3` — at `diameter == 24` that is exactly `16` glyph + `4`
        padding, i.e. `VERTICAL_TABS_SIZING` verbatim — and the **sole** existing call site runs at
        `scale == 1.0`, so every derived dimension is arithmetically identical. It was **3** widened
        call sites, all in `vertical_tabs.rs`.
        **Two fork divergences to PRESERVE, documented at the source:** the collage omits the pin's
        ambient-icon offset (it exists only to correct `render_icon_with_status` parking the circle
        top-left, and Phosphor draws every kind centered — and the fork's `SummaryPaneKind::CLIAgent`
        has no `is_ambient` field at all, being de-clouded); and the circle renderer keeps
        Phosphor's `(icon_size, padding)` construction.
      - **Wave 3 — H + I + J ALL DONE 2026-08-18 (unverified, not compiled). GROUPS NOW RENDER.**
        `render_horizontal_tab_group` (`view.rs:17514`), `render_horizontal_tab_group_header`
        (`:17742`), `compute_group_member_kinds` (`:17965`); `render_tab_bar_contents` now iterates
        `tab_bar_slots()` (`:18196`) and the flat `for i in 0..self.tabs.len()` loop is **gone (0
        remaining)**; `tab_insertion_index_for_cursor` (`:24032`) is a thin wrapper over
        `raw_tab_insertion_index_for_cursor` (`:24056`) → `clamp_to_unpinned_region` →
        `clamp_past_group`. Prerequisite done: `pane_summary_kind` extracted to
        `vertical_tabs.rs:1060` as `pub(super)` and imported at `view.rs:26`.
        **`SavePosition` key symmetry verified directly** (this was the atomicity risk in J): H writes
        `htab_group_position_id(group_id)` at `:17725`, `group_container_rect` reads the same helper
        at `:24985`, both resolving to `horizontal_tabs:group:{id:?}`. All five guards green.
        **Two boundary crossings, both justified:** `TAB_INDICATOR_HEIGHT` and
        `COMPACT_TAB_WIDTH_THRESHOLD` widened to `pub(crate)` in `app/src/tab.rs` (the pin has them
        `pub(crate)` and its only non-`tab.rs` consumers are exactly these step-H lines — a widening
        D/E missed); and **two genuinely absent `crates/warpui_core` APIs ported verbatim** —
        `DraggableState::dragging_mouse_position` and
        `Draggable::with_defer_to_handled_child_mouse_down` (+ the `EventContext` descendant-drag
        flag). Without the second, dragging a member tab drags the **whole group** instead of firing
        `DragTab`. Its other pin call site (`vertical_tabs.rs:3209`) is still unported — **step L**.
        One further divergence recorded: `any_member_active` drops the pin's
        `!is_agent_management_view_open` term, because `set_is_agent_management_view_open` is a
        no-op stub here so the flag is permanently false and the term is dead.
      - **Wave 4 — K DONE 2026-08-18 (unverified, not compiled). All 11 arms are REAL; zero
        `// STUB:` markers remain.** `tab_group_menu_items` (`view.rs:8817`),
        `toggle_tab_group_right_click_menu` (`:6771`), `can_move_tab_group`/`move_tab_group`
        (`:11742`/`:11785`), the three `close_tabs_*_group` (`:11837`/`:11856`/`:11866`),
        `set_/toggle_tab_group_color` (`:4991`/`:5012`), `neighbor_drag_rect` (`:25162`),
        `group_swap_threshold_rect` (`:25194`), `on_group_drag` (`:25226`).
        **The rename editor exists** — `Workspace::tab_group_rename_editor` (`view.rs:942`, init
        `:3033`) with the full builder/event/commit/cancel path, and **H's static-text branch is
        replaced** by the real editor at `:17902`. `render_inline_tab_rename_editor` was widened to
        `pub(crate)` (`vertical_tabs.rs:3766`) to match the pin, since a parent cannot see a child
        module's private items.
        Both "check before rewriting" calls proved right: **`DropGroup` was already complete** (its
        only other pin statement is a telemetry send, dropped fork-wide) and **`StartGroupDrag` was
        half-complete** (needed only `finish_tab_group_rename`). The 12th `// STUB:` grep hit was
        prose in a comment, not an arm.
        Also: `color_picker_menu_items` + `ColorPickerTarget` were missing from `app/src/tab.rs` (the
        fork had only the tab-only `dot_color_option_menu_items`); ported the pin's generalised free
        function **while preserving the fork's i18n** — the clear-dot tooltip stays
        `crate::t!("menu-tab-default-no-color")` rather than the pin's bare literal.
        Seven fork-original tests added; the pin has **no** tests for any of these helpers, so
        nothing upstream was copied and no `DECLINED.md` marker was in play. All guards green.
      - **Wave 5 — L DONE 2026-08-18 (unverified, not compiled). THE PORT IS COMPLETE.**
        `TabGroupMouseStates` (`vertical_tabs.rs:338`), group-aware `render_groups` (`:1765`),
        `render_grouped_tabs_header` (`:2714`), `render_grouped_tab_container` (`:2929`),
        `render_tab_group_header_icon_button` (`:2674` — not in my brief; the header cannot compile
        without it), group-inset insertion targets (`:1324`/`:1376`/`:1403`).
        **`render_group_action_buttons` was PREMISE-FALSE** — `vertical_tabs.rs:2566` already diffs
        byte-identical to the pin. Nothing to port.
        **The most valuable find: three vertical-branch consumers were reading a `SavePosition` key
        that NOTHING WROTE** — `group_container_rect` (`view.rs:25625`), `neighbor_drag_rect`
        (`:25280`) and `group_swap_threshold_rect` (`:25304`) all read `vtab_group_position_id`, and
        the writer only now exists at `vertical_tabs.rs:3210`. Verified directly. Same for
        `vtab_group_kebab_position_id`: it already had a **reader** (`view.rs:23476`, the group
        menu's anchor) and no writer, so the vertical kebab could never anchor; written now at
        `:2804`. A test (`tab_group_save_position_ids_are_distinct_per_axis_and_role`) pins the three
        ids apart, guarding exactly this silent-failure mode — writer and reader live in different
        files, so nothing else catches a mismatch.
        Both carry-overs done: the drag primitive's second call site (`:3194`, so dragging a member
        tab no longer drags the whole group) and **both vertical pin-indicator surfaces** (`:439`
        per-pane, `:2881` group header), with `is_pinned` threaded through **6** call sites — count
        confirmed, and `container_is_hovered` added too since the pin gates the glyph on it.
        **Four deliberate divergences, all commented at the source:** insertion strips stay
        `DropTarget`s (the pin resolves the drop group from cursor geometry via
        `refine_hovered_tab_index`/`insertion_group`, **neither of which exists here**, and the
        fork's `VerticalTabsPaneDropTargetData` still carries `tab_hover_index` through a file
        outside scope) — net effect matches the pin, mechanism differs; the trailing ungrouped strip
        is conditional (the fork's last ungrouped tab already renders its own after-strip, so
        unconditional would draw it twice); `PaneRowStackPosition`/flush-stacked rows **not** ported
        (the pin fires that for *every* tab, grouped or not, restyling existing ungrouped rows —
        out of scope for "render groups"); multi-selection and `summary_pr_badge_mouse_states` left
        as separate features. Summary rows render identically — `VERTICAL_TABS_SUMMARY_ICON_TOTAL_SIZE`
        reused so the arithmetic guarantee from F/G survives.
        Step **D** (`Icon::PinFilledDiagonal` + SVG + pin consts) was already in flight as the
        pinned-tab indicator and is a prerequisite for **E** and **G** — it is being done for its
        own sake, so it is not blocked on this decision.
      - **Wave 2:** **E DONE 2026-08-18** (unverified, not compiled) — `grouped_member` /
        `sole_grouped_member` fields, `for_grouped_member`, `with_effective_color`, and all **6**
        consuming branches in `app/src/tab.rs`, diffed **byte-identical to the pin across all ten
        edited regions**, so F/H/I graft on without translation. Both conjuncts the step-D port had
        dropped are restored (`show_pin_indicator`'s `&& !self.grouped_member`, and
        `build_full_content`'s grouped vertical padding). It also replaced a placeholder tab opacity
        with the pin's `if grouped_member { 55 } else { 30 }`, which **closes the "#108 follow-up"**
        the fork comment there was tracking. `for_grouped_member` / `with_effective_color` are
        callerless until F/H/I — expected, and no `#[allow(dead_code)]` was needed because
        `app/src/lib.rs:4` carries a crate-level `#![allow(dead_code)]`.
        **F** (`TabBarSlot`, `tab_bar_slots()`, position-id helpers, `group_container_rect`) and
        **G** (icon-collage stack) are **queued on the wave-1 foundation releasing `view.rs`**.
      - **Wave 3 (queued):** **H** (`render_horizontal_tab_group` + header, ~485) with **I**
        (rewrite `render_tab_bar_contents` onto slots) and **J** (split
        `tab_insertion_index_for_cursor`). **J must land with I** — `group_container_rect` reads a
        `SavePosition` key only I writes, so porting J early returns `None` for every group and
        silently drops them, which is worse than not porting it.
      - **Wave 4 (queued):** **K** (interaction handlers, ~730 — this is where wave 1's stubbed
        `handle_action` arms get their real bodies).
      - **Wave 5 (queued):** **L** (vertical-tabs group rendering, ~700-800). **Required, not
        optional** — horizontal-only would leave the identical invisible-state anomaly for anyone
        enabling vertical tabs, which is the whole reason the half-present state was rejected.
      Original deferral entry:
      **RESOLVED 2026-08-18 by DEFERRING the feature — `grouped_tabs` removed from the default
      cargo features (`app/Cargo.toml`).**
      See the `DECLINED.md` row for the full reasoning. Short version: everything except paint
      was complete, and the half-present state was worse than absence — creating a group
      silently reordered tabs, wrote membership to SQLite, read it back every launch, and
      exposed no affordance revealing the group existed. Porting instead was **measured at
      ~1,945 lines for horizontal parity (~1,540 in `workspace/view.rs`), ~2,700 with vertical**,
      including two all-or-nothing multi-file commits. Not v0.1.0 work.
      **Safe to disable, verified:** `FeatureFlag::is_enabled` consults the thread-local
      override *before* `FLAG_STATES`, so the 27 tests using `override_enabled(true)` pass
      unchanged; the `[features]` entry stays at `app/Cargo.toml:929` so all code stays
      compiled; and the restore path already drops orphaned `group_id`s, so no migration.

      **RE-ENABLE CHECKLIST (ordered, sized from the pin — keep this).** Re-grep every symbol
      by name at execution time; line numbers in `workspace/view.rs` drift constantly.
      A. `TabBarHoverIndex::BeforeTab(usize)` → `{ index, group }` + `show_before_indicator`
         — ~60 lines, 33 call sites across 6 files. **One commit; enum reshape.**
      B. 11 `WorkspaceAction` variants (`RenameTabGroup`, `MoveTabGroupUp/Down`,
         `CloseTabs{Outside,Above,Below}Group`, `ToggleTabGroupColor`, `StartGroupDrag`,
         `DragGroup`, `DropGroup`, `ToggleTabGroupRightClickMenu`) + `handle_action` arms
         — ~110 lines. **One commit; the match is exhaustive with no wildcard.**
      C. `tab_group_being_renamed` + 6 accessors (`workspace/util.rs`) — ~35
      D. `Icon::PinFilledDiagonal` + `pin-filled-diagonal.svg` + pin consts — ~60 (see below)
      E. `TabComponent` grouped-member support (`app/src/tab.rs`) — ~60, needs D
      F. `TabBarSlot`, `tab_bar_slots()`, position-id helpers, `group_container_rect` — ~120
      G. Icon-collage stack + `select_unique_pane_kinds` — ~190, needs D
      H. `render_horizontal_tab_group` + header + `compute_group_member_kinds` — ~485
      I. Rewrite `render_tab_bar_contents` onto `tab_bar_slots()` — ~60 net, land with H
      J. Split `tab_insertion_index_for_cursor` into `raw_` + wrapper — ~35. **Must land with I**
         — `group_container_rect` reads a `SavePosition` key only I writes, so porting it early
         returns `None` for every group and silently drops them, which is worse than not porting.
      K. Interaction handlers (menu, rename, move, close-around, colour, group drag) — ~730
      L. *Separate change:* vertical-tabs group rendering — ~700-800
      Minimum viable "groups are visible and collapsible" subset: A,D,E,F,G,H(partial),I ≈ 900
      lines — but it ships a header whose right-click does nothing, which is its own parity lie.
      Original entry:
      **Tab groups can be created and persisted but NEVER RENDERED.** `grouped_tabs` is
      a **default cargo feature** (`app/Cargo.toml:653`, wired `app/src/lib.rs:3191`),
      and the model/action/persistence/context-menu layers are all live — but
      `app/src/workspace/view/vertical_tabs.rs` contains **zero** occurrences of
      `TabGroupId` / `tab_groups` / `GroupedTabs` (the pin has 23), and
      `htab_group_position_id` / `vtab_group_position_id` / `group_container_rect` /
      `render_grouped_tab_container` exist nowhere. The fork's `render_tab_group*`
      functions render a *tab's pane group* — the pre-GroupedTabs concept.
      `view.rs:11496-11501` already carries a dangling `assign_tab_to_group` whose
      comment says it "has no caller" for exactly this reason.
      **A user can create a tab group, persist it across restarts, and never see it.**
      This single hole swallows three separate upstream commits (`665f0f657`,
      `9e23bd22f`, and hunks of `3015d875b`/`79fdd7ceb`) and will keep swallowing
      tab-group work until it is filed as one item — which this is.

- [x] **ALL RESOLVED 2026-08-18 (unverified, not compiled).** The 3 that were queued on
      `app/src/workspace/view.rs` are now done: `8794f7325` (tab-group splitting),
      `6ea1a52af` ("+" dropdown anchor), and the `RenameActiveTab` keybinding guard.
      `79fdd7ceb` is **partially** ported — the local `clamp_past_group` landed
      (`view.rs:23328`), its `tab_bar_slots`/`group_container_rect` half stays blocked on
      tab-group rendering (tracked separately above). Original entry:
      **5 of 9 FIXED 2026-08-18 (unverified, not compiled); 1 already-fixed; 3 queued;
      1 BLOCKED on a structural constraint.**
      **FIXED:** `a2d6833b8` review comments routing into hidden panes (one line —
      `right_panel.rs:1522`, and `visible_terminal_views` is at `pane_group/mod.rs:6543`,
      not the 6450 in the old note); `3ac1efb03` MCP install/update buttons drawing the
      label over accent fill; `0979019808` toolbelt chip colour baked at build time (all
      three cached surfaces now subscribe to `ThemeChanged`); `a530563eb` vertical-tab
      unread dots (`has_unread_activity` folded over terminal panes — the fork had
      hardcoded `false` with a comment claiming summary rows carry no dot);
      `8089a74d3` rename-menu guard (menu half; `TabData::tab_name_hidden_in_grouped_pane_view`).
      **`6c9604f15` verified ALREADY FIXED** earlier today, with its regression test.
      **One deliberate deviation from the pin, and it is the right call:** the fork's
      `AgentToolbarEditorModal` is save-on-close, unlike the pin's, so reloading from
      settings on a theme change would have **silently discarded an unsaved
      arrangement**. It rebuilds from the current on-screen arrangement instead — same
      colour effect, no data loss. The inline editor persists per-edit and uses the
      pin's `reset_from_settings` unchanged.
      **STILL OPEN — 3 queued on `workspace/view.rs`** (held by another agent; each
      touches that file and nothing else): `79fdd7ceb` unclamped cross-window drop
      index, `8794f7325` special-tab group placement, `6ea1a52af` "+" dropdown squashed
      against the window edge. Plus the `RenameActiveTab` keybinding guard
      (`view.rs:20008`) — **the keybinding can still rename a grouped tab in Panes view**
      even though the menu item is now hidden.
      **`3015d875b` cancel-rename leg is BLOCKED, not merely off-limits:**
      `Workspace::handle_action` matches `WorkspaceAction` **exhaustively with no
      wildcard**, so adding `CancelActiveRename` without its `view.rs` arm would not
      compile — the enum variant and the handler must land together. Its double-click
      half IS fixed (Panes view now dispatches `RenamePane`, Tabs/Summary
      `RenameTab`; previously both handlers were registered on the same row). The
      group-rename leg is dead here — `RenameTabGroup` does not exist.
      Original entry:
      **Nine more PORTABLE defects from the 54-candidate sweep** (2026-08-18), each with
      a sized fix in the agent report: `a2d6833b8` review comments route into hidden
      panes (**1 line**: `right_panel.rs:1522` `terminal_views` -> `visible_terminal_views`);
      `6ea1a52af` "+" dropdown squashed against the window edge (~12);
      `79fdd7ceb` cross-window drop lands at an unclamped index (~28);
      `6c9604f15` opening the same markdown file twice duplicates the pane (~28);
      `3ac1efb03` MCP modal buttons draw label over accent fill, unreadable on some
      themes (~30); `8089a74d3` "Rename tab" offered where no tab name is rendered (~35);
      `3015d875b` rename editor stuck open / double-click renames wrong thing (~45);
      `0979019808` toolbelt chip text colour baked at build time — light->dark leaves
      near-black on dark, measured contrast 1.07 (~60); `a530563eb` removing the
      Notifications chip silently sets `show_agent_notifications=false`, which makes
      `add_notification` DROP items entirely — unread dots stop appearing everywhere
      with no way to connect cause to effect (~60).

- [x] **DONE 2026-08-18 — all provenance comments cleaned, 0 remaining.** Real count
      was **27 occurrences across 20 files**, not 44/32. **The upstream provenance was
      KEPT** — only the transient `NOT COMPILED -- builds are suspended` /
      `verified by reading only` prefix was stripped, because that claim becomes false
      the moment a build runs, while `Ported from upstream <sha>` stays valuable.
      Handled both the single-line and the comment-wrapped forms, tidied the empty
      comment lines the strip left behind, and fixed one **dangling cross-reference**
      at `workspace/view.rs:23824` that pointed at a note being removed.
      All changed files parse clean. Original entry:
      **Cleanup before any push: 44 `NOT COMPILED -- builds are suspended` comments
      across 32 source files** are now shipping in the tree. An established convention
      for this effort, but it is provenance metadata in production source and reads as
      alarming to anyone outside it. Strip or reword once builds resume.

- [x] **FIXED 2026-08-18. UNVERIFIED (not compiled).** Shaped like `dispatch_action`
      (`gui_view` / `is_none()`-guarded `tui_view`, each arm reinserting into its own map).
      **A prerequisite the brief missed: the TUI view trait had no hook to dispatch to.**
      `TuiView`/`AnyTuiView` lacked `on_window_closed`, `active_cursor_position`,
      `self_or_child_interacted_with` and `accessibility_data` entirely — four of the eight
      sites had nothing to call. All four added with default impls (so no implementor
      breaks) and signatures copied verbatim from `AnyView`, which is what that file's own
      doc comment already promised. Original entry:
      **`self_or_child_interacted_with` is NEVER delivered to any TUI view.**
      `crates/warpui_core/src/core/app.rs:2204`,
      `dispatch_self_or_child_interacted_with`. `get_responder_chain` (`:2045-2053`)
      **explicitly** routes TUI windows through `view_ancestors` — its own comment
      says *"Without this the responder chain is empty and no keystroke/typed-action
      reaches the focused TUI view"* — so the chain it returns is entirely `tui_views`
      ids. The loop then only does `w.views.remove(view_id)` and can never find one.
      Fires on **every handled custom/typed action** (`:2076`). Same class as the
      `notify_model_observers` defect fixed 2026-08-17, but on the input path.
      **Verified 2026-08-17** by reading both sites. Fork-original — the pin has a
      single `views` map, so there is nothing to port; copy the sibling fallback.

- [x] **ALL 7 FIXED 2026-08-18. UNVERIFIED (not compiled).** Six regression tests added in
      `tui_view_tests.rs`, each written to fail against the unfixed code — the query one
      does not even typecheck pre-fix.
      **Extra defect found beyond the brief, same line of code:** in `handle_window_closed`
      the `view_ids` enumeration also drives the subscription / observation /
      `view_to_window` teardown, so **a TUI-only window was leaking every one of those
      entries**, not just missing `on_window_closed`. Folded the TUI ids in.
      **The desktop-notification pair (sites 5/6) is currently UNREACHABLE, and a naive fix
      would have been worse than the bug:** the only entry points are `ViewContext` wrappers
      in an `impl<T: View>` block, and **no type in the repo implements both `View` and
      `TuiView`** (verified by intersecting the implementor sets — empty). A blind
      `tui_views` fallback under the old bound would have turned a silent drop into a
      `downcast_mut().expect()` **panic**. The `AppContext` methods were relaxed to
      `T: Entity` instead; the `ViewContext` methods were deliberately left `View`-bound
      because `context.rs:901` records "the headless TUI has no desktop notifications" as a
      fork decision, and reversing it is a product call under §5.10. That stale comment was
      corrected — it claimed the `AppContext` APIs are View-bound, which is no longer true.
      Both are untestable regardless: the test platform delegates are no-ops.
      **`transfer_view_to_window` left alone, and I agree** — `on_window_transferred` is an
      `AnyView`-only hook with no TUI sibling, cross-window transfer is a GUI tab-tearing
      feature, and it already returns a clean `false` rather than misbehaving.
      **Compile risk to watch:** `views_of_type` / `view_with_id` relaxed `T: View` ->
      `T: Entity` (a `TuiView` is otherwise unnameable), rippling into ~20 call sites that
      all pass concrete GUI types and should be inert. Original entry:
      **The `views` / `tui_views` split is unfinished across `core/app.rs`** — 7 more
      sibling omissions found by auditing all 43 `views` accesses (2026-08-17). All
      fork-original. `:2694-2714` `handle_window_closed` — TUI views never receive
      `on_window_closed`. `:1016` `invalidate_all_views_for_window` — TUI views never
      force-invalidated. `:1406` `focused_view_accessibility_data` — uses
      `views.remove(&id)?`, so the `?` aborts the whole responder-chain walk at the
      first TUI view and a GUI ancestor's data is lost too (macOS-only).
      `:2287`/`:2346` desktop-notification callbacks silently dropped. `:2377`
      `active_cursor_position` returns `None` for a focused TUI view (IME).
      `:3199-3240` `transfer_view_to_window` — a TUI view cannot be transferred.
      `:5081`/`:5100`/`:5139` `views_of_type` / `view_with_id` / `view_ids_for_window`
      cannot see TUI views — and `view_name` (`:5029`) DOES check both, so the
      inconsistency is visible inside one impl block.

- [x] **PORTED 2026-08-18. UNVERIFIED — this file is `cfg(windows)` and cannot be built on
      this host at all.** Both behaviours added to `reg_value_to_string`: the
      `REG_SZ`/`REG_EXPAND_SZ` type guard (with the pin's `microsoft/terminal` link and
      message verbatim) and first-NUL truncation replacing `trim_end_matches('\0')`.
      **Verified byte-identical to the pin** by extracting the function from both trees and
      diffing. `RegType` was already imported. The pin has no test coverage for this
      function, so there was none to port. Original entry:
      **`windows/environment.rs` has two unported oracle behaviours** (found
      2026-08-17 while root-causing the pwsh bootstrap; unrelated to it). The
      `REG_SZ`/`REG_EXPAND_SZ` registry-type guard is missing, and the fork uses
      `trim_end_matches('\0')` where the oracle truncates at the first NUL.
      Separate parity debt, Windows-only.

- [x] **FIXED 2026-08-17. **UNVERIFIED — nothing compiled.** Restored on both; all three `.wgsl` files are now **byte-identical to the pin**
      (`git diff 42effe840` over the shader dir is empty). `rect_shader.wgsl` was
      unaffected — it has no integer varyings.
      **The important finding: this is neither a validation failure nor wrong shading
      TODAY. It is a latent hard failure that detonates on the wgpu 30 bump.**
      naga **29.0.3** (what the fork is on) silently repairs the omission —
      `front/wgsl/lower/mod.rs:4819` calls `apply_default_interpolation`, and
      `front/interpolator.rs` sets `Flat` for `Sint`/`Uint`, so the validator passes
      and the backends emit a real flat decoration. **naga 30.0.0 removed that repair
      deliberately** — its interpolator now handles `Float` only, documented as *"if
      the interpolation was `None`, it will remain so, and be rejected by the
      validator."* The fork is on wgpu `29.0.1` (`Cargo.toml:407`), the pin on
      `30.0.0`. The shaders are created in `Pipeline::new` with **no error scope**, and
      `rendering/wgpu/renderer.rs:202` documents the default handler as fatal — so the
      moment wgpu is bumped to parity, both pipelines die at startup.
      No test: unreachable without a GPU/adapter; a text assertion over the `.wgsl`
      source would only restate the diff.** Original entry:
      **Both WGSL shaders lost `@interpolate(flat)` on their integer varying.**
      `crates/warpui/src/rendering/wgpu/shaders/glyph_shader.wgsl:54` (`is_emoji: i32`)
      and `image_shader.wgsl:32` (`is_icon: u32`). The pin has
      `@location(5) @interpolate(flat)` on both; the fork dropped the attribute. The
      image shader's **entire** diff against the pin is that one deleted attribute.
      **Verified by direct fork-vs-pin comparison 2026-08-17.**
      WGSL requires integer varyings to be flat-interpolated, so this is either a
      naga validation failure or emoji/icons shading wrong across the triangle —
      **which one cannot be determined without building.** Note the fork is on
      wgpu/naga **29.0.3** and the pin on **30.0.0**, so some GPU diffs in this area
      are version skew rather than drift; a more permissive naga 29 may be why this
      has not visibly broken. Restore the attribute on both.

- [x] **FIXED 2026-08-17. **UNVERIFIED — nothing compiled.** Fixed by mirroring the `Subscription::FromView` fallback at `:4081` — same
      `#[cfg(feature = "tui")]` shape, same remove-call-reinsert bookkeeping, same
      post-callback window re-check. Liveness follows the *local* convention (the
      surrounding `views` arm: found => `true`), which differs subtly from
      `emit_event`'s arm. Regression test added:
      `crates/warpui_core/src/core/tui_view_tests.rs::test_model_observation_from_tui_view`,
      which notifies **twice** on purpose — pre-fix, both halves failed (the callback
      never fired AND the miss was reported as not-alive, dropping the observation).** Original entry:
      **`notify_model_observers` cannot reach TUI views — a TUI view observing a
      model stops re-rendering permanently.** `crates/warpui_core/src/core/app.rs`,
      the `Observation::FromView` arm at ~`:4164-4177` looks only in
      `windows[..].views` and has no `tui_views` fallback. **Five analogous sites do
      have it** (`:1486`, `:1609`, `:1954`, `:4081`, `:4273`), and the one at `:4081`
      carries the consequence in a comment: *"Without this branch the callback never
      fires and the subscription is dropped, so e.g. an editor's `ContentChanged`
      never re-renders the view."* **Fork-original defect** — `tui_views` does not
      exist at the pin, so there is nothing to port; copy the sibling branch.
      **Verified 2026-08-17.**

- [x] **DONE 2026-08-17 (UNVERIFIED — rustfmt-parsed only, not compiled).** The GUI
      half now exists: `SSH_REUSE_CONTROL_MASTER_CONTEXT_FLAG` (`settings_view/mod.rs:552`),
      the `SSHWidget` switch (`warpify_page.rs:769-826`, disabled and non-dispatching when
      SSH warpification is off), `WarpifyPageAction::ToggleReuseSshControlMaster`, the
      command-palette pair + telemetry arm in `features_page.rs`, and a 6-line context-flag
      insert in `workspace/view.rs`. Three i18n keys added to `app/i18n/en/warp.ftl`
      (851, 852, 2439) — brand words swapped to Phosphor so `check_brand_strings` passes;
      no ja/zh-CN entries, `fallback_language = "en"` covers them.
      **Telemetry ported verbatim**: `send_telemetry_from_ctx!` is a compiled-out no-op here
      (`app/src/lib.rs:305`), so following the fork IS following the pin, and omitting the
      arm would break `FeaturesPageAction::telemetry_event`'s exhaustive match.
      **One judgement call, reversible:** the switch sits inside the
      `if !should_prompt_ssh_tmux_wrapper` guard, taken from commit `0d24d2cf` rather than
      the pin — by `42effe840` upstream had deleted the tmux toggle from `SSHWidget`
      entirely, and this fork still has it. To always show the switch, delete that wrapper.
      Original entry:
      **SSH ControlMaster reuse: settings UI (item E, deliberately skipped).**
      The setting, the wrapper discovery and the env plumbing landed with the
      2026-08-17 port of `0d24d2cf`; the GUI half did not, and the setting works
      from `settings.toml` without it. Port: `warpify_page.rs` `SSHWidget` switch
      + `WarpifyPageAction::ToggleReuseSshControlMaster`, `features_page.rs`
      `FeaturesPageAction::ToggleSshReuseControlMaster` + command-palette toggle,
      `settings_view/mod.rs` `SSH_REUSE_CONTROL_MASTER_CONTEXT_FLAG`, and the
      `workspace/view.rs` context-flag insert. **Wider here than upstream's 114
      lines:** this fork routes page labels through `crate::t!`, so the two
      upstream string literals also need i18n catalog entries.

- [x] **FIXED 2026-08-17 (coordinator-verified). UNVERIFIED — nothing was compiled.**
      The block is gone from `app/src/ai/agent_sdk/driver.rs`, along with the
      private `SETUP_FAILED_IDLE_TIMEOUT` constant (120s) and the now-unread
      `let idle_on_complete = self.idle_on_complete;` local. The surrounding
      `let result = match rx.await {…}; … result` was restructured to a
      tail-position `match` to avoid `clippy::let_and_return`. Net **18
      insertions / 22 deletions, one file**. A 17-line comment was left in place
      recording why the deferral is gone and quoting the pin's own rationale
      (`42effe840:driver.rs`, doc comment on `idle_window_for_terminal_status`:
      the failure window keeps "a failed run's shared session attachable" and
      "the agent process is the session sharer"), with an explicit **"do not
      re-add this from the pin"**.

      All four inertness premises were independently re-verified before deletion:
      `should_share` hardcoded `false` (`agent_sdk/mod.rs:500`), discarded at
      `driver/terminal.rs:107`, documented at `crates/warp_cli/src/share.rs:15`,
      and `AgentCommand` (`crates/warp_cli/src/agent.rs:229`) has only
      `Run`/`Profile`/`List`/`Message` — no `Attach`. Zero `Attachable` hits
      fork-wide. No test covered the sleep; none was weakened.

      **Caveat: this passed the `rustfmt` parse gate only — it has not been
      compiled.** Residual risk is the borrow shape of the restructured `match`.
      Original finding:
      **Dead idle block: `driver.rs:517-531` waits for a viewer that cannot connect.**
      The surviving `idle_on_complete` branch sleeps up to `SETUP_FAILED_IDLE_TIMEOUT`
      after an `EnvironmentSetupFailed` / `SetupCommandExitedShell` error, with the
      comment *"Keep the session alive after environment setup failures so the viewer
      can connect, receive scrollback, and see the error."* This fork has no viewer and
      no attach path: `should_share` is hardcoded `false`
      (`app/src/ai/agent_sdk/mod.rs:500`), discarded at `driver/terminal.rs:107`, and
      documented as hardcoded in `crates/warp_cli/src/share.rs:15`; `fn attach` /
      `Attachable` / `AgentCommand::Attach` have no agent-related definition anywhere.
      Net effect: a setup failure is reported *later* than it needs to be, for no
      benefit. Delete the block, or keep it only if a local attach path is built.
      Found while adjudicating the idle-window lifecycle, which was **DECLINED**
      2026-08-17 for this same reason (14 ledger rows).

- [x] **FIXED 2026-08-17. **UNVERIFIED — rustfmt/shell-parse only, not compiled.** Removed, not wired. Confirmed genuinely unconstructed — repo-wide grep
      returned only the declaration and one prose mention. The pin's SOLE producer is
      `impl From<PrepareEnvironmentError> for AgentDriverError`
      (`42effe840:driver.rs:766-773`), and `PrepareEnvironmentError` lives in
      `driver/environment.rs`, whose imports are
      `crate::ai::cloud_environments::{CodeForge, SourceRepo}` — the Warp Environments
      subsystem declined under **#211** (`DECLINED.md:81`). So wiring is unavailable
      and removal follows the fork's own precedent (#487: cloud-orphaned code deleted
      rather than left as untested dead code). No match arms touched — this fork has
      no exhaustive match on `AgentDriverError`. `SetupCommandExitedShell` untouched
      and still live.
      **TWIN LEFT ALONE, decide separately:** `AgentDriverError::EnvironmentNotFound`
      (`driver.rs:311`) is in the identical state — one repo-wide hit, its own
      declaration — with pin producers in `resolve_environment` fed by
      `CloudAmbientAgentEnvironment::get_by_id`, the same declined subsystem. It is in
      no ledger. Obvious matching removal.** Original entry:
      **`AgentDriverError::EnvironmentSetupFailed(String)`
      (`app/src/ai/agent_sdk/driver.rs:313`) is never constructed.** Found
      2026-08-17 during the idle-block removal above, and **verified independently
      here**: `grep -rn 'EnvironmentSetupFailed' app crates` returns exactly two
      lines — the variant declaration at `driver.rs:313` and one mention inside
      the explanatory comment at `driver.rs:506`. Nothing constructs it.
      **Pre-existing, not fallout** from the idle-block removal, which only
      pattern-matched it. `AgentDriverError` is `pub`, so no `dead_code` warning
      fires and the compiler will never surface this on its own.
      Removing a public error variant is an API decision, hence filed rather than
      done. **Contrast `SetupCommandExitedShell`, which is genuinely live** —
      constructed at `app/src/ai/agent_sdk/driver/terminal.rs:298`, documented at
      `terminal.rs:68,81,91`, and covered by
      `app/src/ai/agent_sdk/driver/terminal_tests.rs:65,80`. The two variants sit
      next to each other in the enum; only one of them does anything.

**FOUND 2026-08-17 by the oracle-backed refutation fleet.** Each was
confirmed by an agent tasked to REFUTE it, not to confirm it, and
spot-checked against the pin. None was visible in any ledger count.

- [x] **FIXED 2026-08-17. **UNVERIFIED — nothing was compiled; CI or a local build is the first real check.**** The pin's lazy restore mechanism
      (`child_agent/restoration.rs`, a file absent here) ported into
      `pane_group/mod.rs`: `restore_missing_child_agent_panes_for_parent` /
      `_for_terminal_pane_if_needed` / `ensure_hidden_child_agent_pane_for_conversation`,
      hooked into `reattach_panes`, `replace_pane`, `restore_closed_pane` and
      `add_pane_with_options`; plus `focus_pane_preserving_maximized_state`.
      `TerminalPane::attach` now subscribes to `EnteredAgentView` and `detach`
      unsubscribes (**that unsubscribe was missing — a detached tab kept
      materializing panes**). Side effect worth knowing: `RevealChildAgent`,
      `OpenChildAgentInNewPane`, `OpenChildAgentInNewTab` and
      `SwapPaneToConversation` were **dead clicks after every restart** and now
      work. 5 ledger rows PORTED, 1 CLOUD (remote child), 2 still open.
      Original finding:
      **Child/orchestration agents do not survive a restart.** Four
      independent audits converged on one subsystem; three separate
      failures compound:
      (a) `start_new_child_conversation` (`app/src/ai/blocklist/history_model.rs:511-546`)
      never calls `persist_conversation_state`; the pin does, at its `:578`.
      A child spawned via `/orchestrate` that has not yet taken a turn is
      absent from SQLite, so a crash or restart drops it and its parent link.
      **One-line fix, matches the pin.**
      (b) `restore_conversations` (`:1021-1055`) indexes `children_by_parent`
      only by `parent_conversation_id()`, dropping the `parent_agent_id`
      fallback that `resolved_parent_conversation_id_for_conversation`
      (`:555-573`) already uses. The fork's own commit `1a351db1b` (#383)
      states this data shape occurs: "a child spawned by an agent run carries
      `parent_agent_id` and no `parent_conversation_id`". Such a child is
      invisible to the parent's pill bar, status card and pane group after
      restore. **One-line fix, matches the pin.**
      (c) No lazy hidden-child-pane materialisation.
      `create_missing_child_agent_panes` runs once from `new_internal`
      (`app/src/pane_group/mod.rs:2540`) and is never re-invoked by
      `reattach_panes` (`:6082`), `replace_pane` (`:4163`) or
      `restore_closed_pane` (`:4557`). The pin's
      `ensure_hidden_child_agent_pane_for_conversation` exists here only in
      comments describing its absence. **Larger; 9 ledger rows depend on it.**
- [x] **FIXED 2026-08-17. **UNVERIFIED — nothing was compiled; CI or a local build is the first real check.**** Ported the pin's
      `active_tab_bar_position_id`; `tab_bar_rects_for_window` now returns the
      single active rect instead of unioning `TAB_BAR_POSITION_ID` with
      `VERTICAL_TABS_PANEL_POSITION_ID`. Regression test
      `test_active_tab_bar_position_id_tracks_layout` ported. Note the stale
      comment at `cross_window_tab_drag.rs:1774` was left alone **on purpose** —
      the pin carries the identical stale comment, so fixing it would create a
      divergence. Original finding:
      **Cross-window tab drag accepts a drop on an inactive tab bar.**
      `tab_bar_rects_for_window` (`app/src/workspace/view.rs:23886-23897`)
      unions `TAB_BAR_POSITION_ID` and `VERTICAL_TABS_PANEL_POSITION_ID`
      unconditionally, and all three consumers hit-test the union. With
      vertical tabs on, the horizontal strip still renders (toolbar only, no
      tabs); dragging a tab from another window over it is accepted, an
      insertion ghost appears at the top or bottom of the *vertical* list, and
      releasing drops the tab there. Both `vertical_tabs` and
      `drag_tabs_to_windows` are in `app/Cargo.toml`'s `default`. The pin
      guards this with `active_tab_bar_position_id` (`42effe840:view.rs:29443`),
      whose doc comment says the bug shipped upstream once already. Fork-original
      code (`f2e0a1d6f`, #9275), not unported.
- [x] **FIXED 2026-08-17. **UNVERIFIED — nothing was compiled; CI or a local build is the first real check.**** Added
      `should_reserve_traffic_light_space_in_tab_bar(side) -> side == TrafficLightSide::Right`,
      matching the pin exactly, replacing the fork's
      `vertical_tabs_active || !right_panel_open`. Regression test
      `test_tab_bar_traffic_light_space_regression_for_resource_center_overlap`
      ported. The doc comment records that left-side (macOS) reservation is
      `compute_tab_bar_left_padding`'s job, so nobody later "fixes" this on macOS
      where the branch is dead. Original finding:
      **Traffic-light space regression (#10139) is live.**
      `app/src/workspace/view.rs:17936-17940` ties the reservation to
      `vertical_tabs_active || !right_panel_open`; the pin's fixed predicate is
      `side == TrafficLightSide::Right` alone (`42effe840:view.rs:29252`).
      Divergent in exactly one state — vertical tabs off (**the default**) with
      the right panel open — where window controls overlap tab-bar content.
      **Not macOS**: `traffic_lights.rs:70` returns `Left` there, so the branch
      is dead; it bites Windows and non-tiling-WM Linux. Inherited from
      `0dbd3d567` and never revisited.
- [x] **DECLINED as work 2026-08-17 — retained below as a LATENT note, not a task.**
      `DECLINED.md` (TUI autoupdate row) declines this explicitly: the fetch is
      severed in the only shipped binary, so the weaker guard is unreachable.
      Re-open the moment autoupdate is ever shipped. Original finding follows.
      **Autoupdate version-component guard is weaker than the pin's.**
      `crates/warp_tui/src/autoupdate.rs:490-497` gates on
      `ParsedVersion::try_from` plus `contains(['/', '\\'])`; the pin uses
      `is_safe_version_component` — an ASCII allowlist with `..`, reserved-name
      and trailing-dot rejection plus `Path::components()`. The raw server
      string is joined at `:525`, so junk around a valid version survives into a
      directory name. **Not exploitable today**: `oss.rs:42` sets
      `autoupdate_config: None` and `server_root_url()` is the reserved
      `192.0.2.0:9`, so the fetch is severed in the only shipped binary. Note
      `VERSION_RE` is unanchored in the pin too — the regex is not the defect,
      the replaced guard is.
- [x] **FIXED 2026-08-17. **UNVERIFIED — nothing was compiled; CI or a local build is the first real check.**** Tab now routes through a pure
      `shell_completion_tab_action(completions_menu_is_open, other_inline_menu_is_open)`
      at `terminal_session_view/completions.rs:326`. Previously Tab over any other
      open inline menu aborted the in-flight request, bumped the generation and
      spawned a completer shell round-trip whose results `completion_request_is_current`
      then discarded — which is exactly what hid the bug. Test lives in
      `completions_tests.rs`, **not** the pin's `input/view_tests.rs`: the pin
      dispatches `TuiInputAction::Complete` into `TuiInputView`, and both that and
      `TuiInputViewEvent::RequestShellCompletion` are 0 hits here (Tab is the
      session-level `TRIGGER_COMPLETIONS_BINDING_NAME`). Original finding:
      **`request_shell_completion` has no guard against other inline menus.**
      `crates/warp_tui/src/terminal_session_view/completions.rs:91-97` checks
      only `completions_menu.is_open`, so Tab is not consumed by an existing
      non-completion menu. The module doc lists two deliberate deviations from
      the pin and this is not one of them. Needs production code.


- [x] **#587 — `(key connected)` renders only on DISABLED models.** Inverted.
      **FIXED, pending merge** — on `repin-gap-keymarker`. `menu_result_row` now
      renders the second column when a `Default` row has *either* a description
      or a `state_suffix` (upstream `bf56c3c18`'s hunk), with a suffix standing
      alone taking the two-space gutter a description would have had. Two
      render-level tests added, one per layer:
      `inline_menu_tests::default_row_state_suffix_renders_with_and_without_a_description`
      and `model_menu_tests::selectable_key_connected_model_renders_the_suffix`.
      **The profile menu's `active` marker was invisible for the same reason**
      (`profile_menu.rs` passes `description: None` on every row) and is fixed by
      the same change. Original finding below for the record.
      `inline_menu.rs` emits `state_suffix` nested inside the `description`
      block, and `model_menu.rs` sets `description` only when `!is_selectable`.
      **Both existing tests pass** because they assert on
      `snapshot.rows[].state_suffix`, never on rendered lines — so any fix needs
      a render-level test or it reverts silently. ~15 lines, three files.
- [x] **#582 — `CLIAgentEventType::StopFailure` missing**, so agent failure
      never reaches the GUI status chip — for *every* integration, not just the
      TUI. Present at the OLD pin, so unported debt. Now asymmetric: the OSC 777
      publisher ported this round **emits** `stop_failure` with no consumer.
      ~8 files' exhaustive matches + a call on failure-chip semantics.
      **FIXED, pending merge** — `repin-gap-stopfail`. The gap was wider than
      filed: `CLIAgentSessionStatus::Failed` was missing too, so the variant
      alone had nowhere to land. Four exhaustive matches over the status enum
      were extended (`to_conversation_status`, `notifications/model.rs`,
      `terminal/view.rs`, `agent_sdk/driver.rs`) plus the one over the event
      type. Chip semantics follow the pin exactly: `Failed { .. }` →
      `ConversationStatus::Error` regardless of `error_type`, latching until the
      next `PromptSubmit` (no timer). Two stale comments claiming `Failed` was
      cloud-only were corrected in place.
- [x] **#596 — cancelling a TUI conversation lights an *error* in the GUI.**
      **FIXED, pending merge** (`repin-iss596-cancelled`). An interaction defect
      between two branches of this re-pin, neither wrong alone: the OSC 777
      publisher (`repin-osc777`) emits `stop_failure` + `error_type:"cancelled"`
      for `ConversationStatus::Cancelled` — **which is exactly what the new pin
      `42effe840` emits**, upstream has no cancellation event tag — while the
      `stop_failure` consumer (`repin-gap-stopfail`) faithfully restores the
      pin's mapping of every `Failed` to `ConversationStatus::Error`, a red
      triangle that latches until the next `PromptSubmit`. So the user's own
      Ctrl-C reads as a persistent error, and the fork's distinct `Cancelled`
      (gray stop) is never reached from a TUI session.
      Fixed in the **protocol**, not with a literal in the GUI: new
      `warp_core::cli_agent_error_type` types the `error_type` classification
      (`Cancelled` / `Other(&str)`) in the crate both producer and consumer
      already depend on, and `to_conversation_status()` branches on the variant.
      **No wire change** — the emitted bytes stay upstream's, so no protocol
      version bump and no plugin is affected. Follow-ups left open, both in
      files that arrive from `repin-gap-stopfail`:
      (a) `notifications/model.rs` still raises a `NotificationCategory::Error`
      toast for a cancelled turn;
      (b) the publisher's literal `Some("cancelled")` should become
      `Some(cli_agent_error_type::CANCELLED)` — byte-identical, hygiene only.
- [x] **#586 — shell completions never learn functions or builtins.**
      `Session::load_all_function_names` / `load_all_builtins` and the whole
      deferred name-set machinery exist at the old pin, absent here (verified 0
      hits vs 1-2). Same subsystem as the stale `warp-command-signatures` data
      this round fixed — that one made completions *wrong*, this makes a class
      of names invisible.
      **RECONCILED 2026-08-17 (v0.1.0).** Wrong on both counts: the machinery *is* present (`Session::load_all_function_names`, `session.rs:1160`), and bash/zsh/fish report functions and builtins in the bootstrap payload (`compgen -A function`, `functions -an`, consumed at `session.rs:763`). They return `None` from `shell_command_to_get_all_functions` because they need no deferred pass -- **identical to the pin**, which is also `_ => None` for everything but PowerShell. GitHub #586 closed `not_planned`. The one live question is PowerShell-only and is documented in place at `session.rs:1145`.
- [x] **#585 — the TUI OSS binary still uses the pre-rename app id.**
      **FIXED, pending merge** — flipped to `("dev","phosphor","Phosphor")` on
      `repin-gap-appid`. `crates/warp_tui/src/bin/oss.rs:24` was
      `AppId::new("dev","zap","Zap")` while `app/src/bin/phosphor_oss.rs:30` is
      `("dev","phosphor","Phosphor")`. GUI read `~/.config/phosphor`, TUI read
      `~/.config/zap`, different keyring service names — **API keys saved in
      the GUI were invisible to the TUI.** Shipped that way in
      `v2026.08.14.1-beta`.
      *Root cause of the miss:* `specs/phosphor-rebrand/LAYER3-PLAN.md` §1
      states "`AppId` is set in two places" and tabulates only
      `channel/state.rs` and `app/src/bin/zap_oss.rs`. There are three; the
      rename commit `874c2f43d` faithfully executed a plan that was one site
      short. Plan and inventory corrected in the same branch.
      *Migration:* none, matching `874c2f43d`'s recorded decision — the
      maintainer accepted losing local state rather than carrying a keychain
      migration. This orphans TUI-written state under `dev.zap.Zap` as well;
      recorded in `README.md`'s storage-identity note.

### Defects found by the fix-refutation pass (2026-08-15)

- [x] **#601 — every OpenCode/DeepSeek harness launch logs a false install failure.**
      `driver.rs:1125` calls `manager.install()` with no `can_auto_install()`
      check. DeepSeek returns `false` unconditionally, OpenCode likewise, Codex
      behind a flag — so `install()` hits the trait default `Err("Auto-install
      not supported")` and logs a warning for a *correct* decline. Also masks a
      genuine Claude failure in the same stream. Distinct from #600: that path
      needs `has_local_marketplace_override()`, this one needs
      `can_auto_install()`, and the pin calls each in only one of the two.
      **RECONCILED 2026-08-17 (v0.1.0).** GitHub #601 closed `completed`.
- [x] **#602 — MCP template variables render unmasked.** `mcp_install_flow.rs`
      collects them into the shared input with **no** `input_ownership`, while
      its own `Debug` impl writes `[REDACTED]`. `TuiMcpTemplateVariable` carries
      no secret flag, so nothing distinguishes an API token from a hostname.
      Same class as #599; masking primitive arrives with `repin-input`.
      **RECONCILED 2026-08-17 (v0.1.0).** GitHub #602 closed `completed`.

### Known-incomplete fixes — verified by refutation, not yet closed

- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: #596 is closed; the correction is not worth tracking.**
      Original finding retained below. Original entry:
      **#596's central claim does not hold.** The commit says the cancellation
      spelling is "written down once and both halves refer to the variant". The
      **publisher still hardcodes `error_type: Some("cancelled")`**, and the test
      that appears to guard this only asserts the constant against its own
      literal. Also the fix moves the *status chip* only — `notifications/model.rs`
      still files every `Failed` as `NotificationCategory::Error`, so a Ctrl-C
      still lands in the notification centre as an error.
      **STILL OPEN 2026-08-17 (v0.1.0) -- do not tick because the issue is closed.** GitHub #596 is closed `completed`, but the residue described above is still in the tree: `crates/warp_tui/src/cli_agent_osc_event_publisher.rs:360` still hardcodes `error_type: Some("cancelled")`, and `app/src/notifications/model.rs` still files failures as `NotificationCategory::Error`. Verified by reading the code, not by the issue state.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — **Half of this is false.** The publisher half is real: `cli_agent_osc_event_publisher.rs:360` still hardcodes `Some("cancelled")` and never imports the `CANCELLED` constant that exists for it. But `notifications/model.rs:331-336` HAS a dedicated `ConversationStatus::Cancelled` arm producing `NotificationCategory::Complete` ("Task was cancelled."), and `cli_agent_sessions/mod.rs:38-66` maps a cancelled `Failed` to `Cancelled` before it ever reaches there. I asserted this claim held earlier today after grepping for `NotificationCategory::Error` and finding hits — without checking whether the cancelled path reaches them. Rescope to the publisher literal alone.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: #597 is closed; its justification being wrong is not
      worth tracking separately.** Original entry:
      **#597's fix is sound; its justification is wrong.** The
      permanent-bootstrap-file argument does not hold — that file is returned
      only for `BootstrapSessionType::Local` PowerShell (the case #597 never
      affected), and `WarpifiedRemote` streams the script from the local binary
      each session rather than using an RC file. The rename direction is still
      right on its other grounds, but **the wrong evidence is now baked into two
      permanent doc comments** and should be corrected.
      **STILL OPEN 2026-08-17 (v0.1.0).** GitHub #597 is closed `completed` (the code fix), but this entry tracks the *justification* baked into two doc comments, which was not verified as corrected during the v0.1.0 reconciliation. Re-read the comments before ticking.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — One doc comment, not two: only `in_band_command_executor.rs:47-56` repeats the flawed permanent-bootstrap-file justification. The `pwsh.ps1` and `shell_command_tests.rs:23` comments reference #597 without repeating the wrong argument. Core claim confirmed — `bootstrap_file/mod.rs:45-50` returns `None` unless `BootstrapSessionType::Local`.
- [x] **`check_generator_wrapper_names` is defeated by commenting a line out —
      8 of 8 `require`/`require_count` checks.** Commenting the pwsh
      `Export-ModuleMember` line *is* #597's failure mode, and the guard passes.
      It checks presence, not activeness. It also scans 4 of the 19 files in
      `app/assets/bundled/bootstrap/`, so a third spelling in `bash.sh` passes.
      **COUNT CORRECTED 2026-08-17 (v0.1.0 agent audit)** — the defeat-by-commenting-out finding is reproduced (commenting the `Export-ModuleMember` line still exits 0), but the script now has **11** `require`/`require_count` checks, not 8, and scans 4 of 19 bootstrap files. All 11 use the same substring-presence technique and are equally defeated.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Reproduced empirically: commenting out `Export-ModuleMember` in `pwsh.ps1` still exits 0, because `require()` uses `grep -qF` (presence, not activeness). 11 `require`/`require_count` call sites, scanning 4 of 19 bootstrap files. Finding confirmed; count corrected from 8 to 11.
- [x] **`check_brand_strings` residual misses**, exact text:
      `concat!("Warp", " Agent…")`, `format!("… {} Agent…", "Warp")`,
      `"Zap-powered completions"`, `"Zap2 is available"`, `"Warp\nAgent"`,
      `"Warpを再起動してください"` (Rust side only), and **Fluent terms
      (`-term = Warp Agent`) are not scanned at all** — zero terms today.
      Worst false positive: a `brand-guard: allow` marker covers one line, so a
      multi-line Fluent value or a marked message's `.attribute` still fires —
      which hits its one sanctioned use, the AGPL §13 attribution.
      **`Zapfino` fires in production code**; green today only because all 16
      occurrences are in `*_test.rs`. Wider instance: `Ozone`.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Reproduced empirically: a synthetic Fluent message with a `.tooltip` attribute under a `brand-guard: allow` marker still fired on the attribute. Fluent `-term` definitions are structurally unscannable (`FTL_ID` requires `\.?[A-Za-z]`, never a leading `-`), and zero terms exist today. All listed blind spots confirmed.

### Defects — correctness of the guards and the record

- [x] **#591 — `user_controlled_alt_screen_...` asserts far less than the pin.**
      **FIXED, pending merge** — glyph set restored on `repin-statusline` (forced
      by the `4431b15ff` port itself), second signal restored on `repin-alttest`
      (`0c338ccfa`). Note the correction recorded on the issue: the missing
      `"auto (cost-efficient)"` clause was *inapplicable*, not merely dropped —
      BYOP has no built-in model list, so the fork reads the active model name
      dynamically. Original finding below for the record.
      Negated `any`, so the fork's narrower predicate forbids *less*: pin bans
      ten border glyphs + the model label, fork bans `┌` alone. This round's
      `4431b15ff` port replaces box-drawing borders with hairline glyphs, so the
      fork's assertion now has almost nothing left to catch. §5.6; recorded in
      no document and uncommented at the site.
- [x] **#593 — 28 ledger rows say CLOUD where `DECLINED.md` says the opposite.**
      24 are the `grok_*` cluster in `crates/ai/src/api_keys_tests.rs`, which
      `DECLINED.md:93` claims by name and count while `:250` says explicitly it
      is ***not*** a cloud drop. Kills the rule-2 tripwire and teaches the exact
      false history the "common false positives" section exists to prevent.
      **68 further `judgement`-confidence CLOUD rows are unsampled** — the
      sampled hit rate was 28/58.
      **RECONCILED 2026-08-17 (v0.1.0).** GitHub #593 closed `completed`.
- [x] **#592 — tests added to already-classified files land in no bucket.**
      `UNCLASSIFIED` is whole-file, so a file with existing ledger rows can
      never surface new upstream tests. Measured this move: **284** across the
      63 files first worked, plus **128** genuine candidates across the 9 files
      a tooling bug had hidden. `terminal_session_view_tests.rs` alone gained
      71 and rewrote 26 of the 84 it kept. Ceiling across the tree: 685.
      Understates new debt silently, every re-pin.
      **[DONE — `script/generate_repin_queue` grew a `LEDGER COVERAGE GAP`
      section: per-test, whole-ledger, independent of the file diff (5 of the 7
      files it reports are in `DECLINED COLLISION`, which never reaches the
      loop's ledger branch). Currently **17 tests across 7 files**; small
      because the manual pass this issue prescribes was since done by hand —
      re-run against only the old pin's 841 ledger rows it reports **170**, so
      the filters are not vacuous. It also prints a reconciliation block, which
      turned up a second defect NOT fixed here: `docs/STATE.md`'s "N are not
      adjudicated" subtracts the ledger's ROW COUNT from the absent-name count
      instead of differencing the sets, so 273 rows naming tests the fork now
      has (plus 4 naming tests not at the pin) cancel genuinely unadjudicated
      ones. It reports 435 where the set difference is 715 — filed as **#603**,
      not fixed here: it moves a headline number in a generated file.]**
- [x] **#603 — `script/state`'s unadjudicated count is a subtraction of totals,
      not a difference of sets.** `UNADJ=$(( ABSENT - LED_ROWS ))`, so each of
      the 273 ledger rows naming a test the fork now has (plus 4 naming a test
      not at the pin) cancels a genuinely unadjudicated one. Published 435;
      true set difference **715**. Wrong in the optimistic direction, and the
      error *grows* as rows become `PORTED`/`COVERED-ELSEWHERE` — i.e. as the
      fork makes progress. Found by #592's reconciliation block.
      **RECONCILED 2026-08-17 (v0.1.0).** GitHub #603 closed `completed`, and verified in the code: `script/state:112` now computes `UNADJ=$(( ABSENT - ADJ_ABSENT ))`, a set difference, with the old subtraction documented at line 87.
- [x] **#589 — branding guard blind spot** (fixed, kept for the latent edge):
      genuine proper nouns continuing lowercase would fire. `Zapfino` (Apple's
      font, 16 occurrences) survives only because every occurrence sits in a
      `*_test.rs` file or a comment. A font name in production code needs
      handling.
      **RETAINED DELIBERATELY 2026-08-17 (v0.1.0).** GitHub #589 is closed `completed`; this entry is kept on purpose for the latent edge (a proper noun continuing lowercase in production code, e.g. a font name). Not outstanding work.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Confirmed: `gh issue view 589` is CLOSED; 16 `Zapfino` hits, every one in a `*_test.rs` file. Retained deliberately for the latent edge, as written.

### Divergences that need a decision, not a fix

- [x] **#583 — `test_phosphor_tui_variant_properties` asserts `None`** for
      `brand_color()`/`icon()`; upstream now returns `Some(ColorU::black())` /
      `Some(Icon::Warp)`. Either give the variant Phosphor branding and update
      the assertion, or keep `None` with a `DECLINED.md` row and a `keep:`
      marker. Leaving it stale is the one option that is wrong.
      **RECONCILED 2026-08-17 (v0.1.0).** GitHub #583 closed `completed`.
- [x] **#584 — `todo_glyph` `InProgress` uses `•` (U+2022)** where both pins use
      `●` (U+25CF). Sibling arms (`✓`, `■`) match the pin exactly, so it is a
      single-glyph divergence recorded nowhere. A ported test now pins the
      character, which is the only thing holding it.
      **RECONCILED 2026-08-17 (v0.1.0).** GitHub #584 closed `completed`.
- [x] **#594 — `GeminiNotifications` stays out of `default`; promote only after
      someone runs it.** **The issue's premise is refuted: this is not fork
      drift.** There is no `gemini_notifications` cargo feature *upstream
      either* — verified absent from `app/Cargo.toml` at both `02b53fcd8` and
      `42effe840` — and upstream keeps the flag in `DOGFOOD_FLAGS` alone, with
      `specs/APP-4067/TECH.md` §8 still listing "**Promote
      `GeminiNotifications` from dogfood** — after validation" as an open
      follow-up. The three siblings the issue calls "wired normally"
      (`hoa_notifications`, `open_code_notifications`, `codex_notifications`)
      are in upstream's `default` too, at lines 671/672/682 of its
      `app/Cargo.toml`. So the asymmetry is upstream's, and the FLAG-DARK rule
      "dark at the pin too → do not fix" applies. Adding it to `default` would
      have shipped the chip *further than upstream has*.
      **RECONCILED 2026-08-17 (v0.1.0).** GitHub #594 closed `completed`.
- [x] **What #594 did surface, and what was changed.** Dogfood-only means
      **unreachable by anyone** in this fork, not "awaiting validation" as it
      does upstream. Upstream's dogfood channel is a GUI build; here
      `app/src/bin/phosphor_oss.rs` is the only GUI binary and adds
      `DEBUG_FLAGS` alone, while `DOGFOOD_FLAGS` reaches only `warp_tui`'s
      `dev`/`local` binaries — a TUI with no plugin chip — and the schema
      generators. So the validation upstream is waiting on could never happen
      here. Fix applied: `gemini_notifications` registered in
      `UNSTABLE_FEATURES` (`app/src/lib.rs`), i.e. opt-in via
      `ZAP_UNSTABLE_FEATURES=gemini_notifications` and off for everyone else.
      **Open work:** actually run it against a real `gemini` session, then
      decide whether to promote to `default` — and if the answer is no, that
      becomes a `DECLINED.md` row. No `DECLINED.md` row was added now, because
      agreeing with the pin is not a non-parity decision.
      **RECONCILED 2026-08-17 (v0.1.0).** Context for #594, retained as a record; the issue is closed `completed`.
- [x] **#594's trace, for whoever validates it.** Every link exists and was
      checked: `plugin_manager_for(CLIAgent::Gemini)`
      (`plugin_manager/mod.rs:281`, ANDed with `HOANotifications`) →
      `GeminiPluginManager` (`plugin_manager/gemini.rs`, 11 tests in
      `gemini_tests.rs`) → `gemini extensions install
      https://github.com/warpdotdev/gemini-cli-warp --consent` → detection at
      `~/.gemini/extensions/gemini-warp/gemini-extension.json` → the **local**
      OSC 777 listener. Externally verified rather than assumed: the extension
      repo is published and public, its manifest `name` is `gemini-warp` (so
      `EXTENSION_NAME` and the detection path are right) and its `version` is
      `1.0.0` (so `MINIMUM_PLUGIN_VERSION` matches and a fresh install does not
      immediately re-show the update chip). Upstream's *other* stated shipping
      blocker — "Publish `warpdotdev/gemini-warp` to GitHub — must happen
      before shipping to external users" — is therefore already satisfied. The
      protocol sentinel is still `warp://cli-agent`
      (`cli_agent_sessions/event/mod.rs:12`), which the extension emits, so the
      rebrand did not break the wire. `is_agent_supported`/`create_handler`
      already accept `CLIAgent::Gemini` under `HOANotifications` alone
      (`listener/mod.rs:46`, `:69`), so **notifications from a hand-installed
      extension already work today** — the flag gates only the install/update
      chip. Two residual risks for the validator: the extension needs Gemini
      CLI v0.26.0+ and nothing version-checks the CLI (an older CLI fails the
      install command and falls back to the manual-instructions modal via
      `record_plugin_auto_failure_and_notify`, degraded but handled); and
      `plugin_manager_for` is the flag's **only** consumer, so nothing else
      moves when it flips.
      **RECONCILED 2026-08-17 (v0.1.0).** Context for #594, retained as a record; the issue is closed `completed`.

### Consequences of this round's merges — check after integrating

- [x] **Benchmark debt becomes real.** `repin-misc` adds
      `benches/transcript_bench.rs` and `benchmark_support.rs`. Two other
      branches skipped bench hunks as "no bench harness here" — after merge,
      `a95e6e541`'s `ClippedTerminalBlockBenchmark` and `b462e0132`'s
      `zero_state_bench.rs` are genuine unported debt.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Both benchmarks are ported: `crates/warp_tui/src/benchmark_support.rs:10` (`ClippedTerminalBlockBenchmark`, commit `73753d713`) and `crates/warp_tui/benches/zero_state_bench.rs` (commit `5cd185c55`); both commits are ancestors of `main`.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: issue-accuracy note, not tracked further. **The failing assertion was then FIXED 2026-08-17 (UNVERIFIED — rustfmt-parsed only, not compiled).** `ORCHESTRATE` moved out of the prompt-submitting assertions into the run-now loop in `app/src/terminal/input/slash_commands/mod.rs`, with a doc comment recording the deliberate divergence: this fork's `/orchestrate` is user-invoked and executes directly, so it has no `kind()` arm and falls through to `SlashCommandKind::Other` (pinned from the other side by `orchestrate_command_is_registered_and_available_on_both_surfaces`). `SlashCommandKind::Orchestrate` was left in the predicate's `matches!` arm deliberately — unreachable here, but it keeps pin fidelity so the deferred agent-invoked path works unchanged if wired up.** Original entry:
      **`slash_command_is_submitted_as_prompt`** (`app/src/terminal/input/slash_commands/mod.rs:80`)
      matches `SlashCommandKind::Orchestrate`, which this fork's `kind()` never
      produces — `/orchestrate` maps to `SlashCommandKind::Other`. Latent
      unreachable branch; found while confirming `a86400ede` was already present.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: issue-accuracy note, not tracked further.** Original entry:
      **`join_a_workspace()`** (`app/src/integration_testing/assertions.rs:43`)
      is now unreferenced — the two deleted team command-palette tests were its
      only consumers. Left deliberately as pin-derived scaffolding.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: issue-accuracy note, not tracked further.** Original entry:
      **`agent-zero-state-title-cloud` (ja)** reads
      `新規 Phosphor Agent ローカルエージェント会話` — the doubled-noun artifact,
      but the redundancy sits inside `ローカルエージェント`. Removing it changes
      meaning, not branding. Needs a translator; the key has no `en` counterpart
      to follow, and the brand guard cannot see it.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: issue-accuracy note, not tracked further.** Original entry:
      **`input_cut_binding_yields_ctrl_x_to_contextual_menu_clear`** is not
      portable: neither the test nor the `INLINE_MENU_CAN_CLEAR_SELECTED_FLAG`
      carve-out for cut exists here. Porting needs a production change.

## FOLLOW-UP FROM #595 (2026-08-15) — the plugin manager's other uncalled method

#595 removed the four Oz-platform-plugin methods (`DECLINED.md`, "Oz platform
plugins"). Tracing their pin-side callers surfaced one residual that is **not**
declined and is a genuine, if small, divergence. Recorded here rather than as a
GitHub issue because the agent that found it had no remote-write authority.

- [x] **`CliAgentPluginManager::has_local_marketplace_override` has no non-test
      caller, and the call site it belongs at is missing its guard.** The pin
      calls it in `ensure_local_claude_child_plugins`
      (`app/src/pane_group/pane/local_harness_launch.rs:35-53` at `02b53fcd8`) to
      *skip* notification-plugin install/update when a developer has pointed the
      `claude-code-warp` marketplace at a local checkout — installing re-adds the
      public marketplace and clobbers the override. This fork's equivalent
      (`app/src/pane_group/pane/local_harness_launch.rs:261`) calls
      `manager.install()` unconditionally, so launching a local Claude child pane
      on a machine with a local marketplace override silently replaces it. The
      method, its two impls (`claude.rs`, `codex.rs`) and its two tests are all
      present and correct — only the caller's guard is missing. Unlike the
      platform-plugin methods this is **local and non-cloud**: it reads
      `settings.json` / `config.toml` on disk and gates a `claude plugin` /
      `codex plugin` invocation, with no Oz surface involved, so it was left in
      place. Fix is ~3 lines at the call site; it needs the same
      `needs_update()`-else-`is_installed()` shape the pin uses, not a bare
      `if !override`.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Fixed by #600: `app/src/pane_group/pane/local_harness_launch.rs:84` checks `has_local_marketplace_override()` and branches `needs_update()`/`is_installed()`. The entry predates that fix.

## GIT-PINNED DEPENDENCY DRIFT — found 2026-08-15 during the first re-pin

`ORACLE.md` pins the Warp *commit*, but upstream's `Cargo.toml` pins several
**external git repos** that move on their own schedule. Nothing in the re-pin
runbook covered them until now (`docs/pin-migration.md` Phase 3.5 closes that),
so they have drifted silently since the initial public release.

Audited all 22 git-pinned deps against `42effe840`. **17 match upstream exactly**
— this has mostly been tracked — and the exceptions split three ways.

### Real drift — the fork is simply behind

- [x] **`warp-command-signatures`: `00a032b8` → `fe352669`.** The completion
      spec data, pulled with `embed-signatures`, compiled into the binary and
      consumed by `crates/warp_completer`. Set in "Initial public release of
      Warp" and never touched since. Upstream was already at `29cd61c3` at the
      **old** pin, so the fork was behind before this pin move — this is
      pre-existing debt, not something the re-pin created. Recorded in no
      document: not `DECLINED.md`, not `ORACLE.md`, not `docs/STATE.md`.
      Consequence: every command whose flags or arguments changed upstream
      completes against stale data. The bump is a lockfile change plus a build,
      **not** a source port; the five intermediate revs do not apply as diffs
      because the fork's base does not match the start of the chain.
      Repo is reachable (HEAD `d79e09c4` as of 2026-08-15).
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Already bumped: `Cargo.toml` pins `warp-command-signatures` at `fe3526693fe…` — the target rev — via commit `993c7102e`.

- [x] **`notify-debouncer-full`: `f3afcda30` → `91b719849`.** Same repo
      (`warpdotdev/notify`), fork behind, undocumented. This is the filesystem
      watch debouncer — it sits under the directory-watch paths that the
      `RepositoryWatchMode` work also touches, so check the two together.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Already bumped: `Cargo.toml` pins `notify-debouncer-full` at `91b719849bc…` — the target rev — same commit `993c7102e`.

### A divergence needing a decision, not a bump

- [x] **STALE — CLOSED 2026-08-18. This is a DUPLICATE of the rmcp entry already
      closed at line 166, and the migration is done.** `Cargo.toml:464` now reads
      `rmcp = { version = "1.6" }` — the `warpdotdev/rmcp` git pin is gone, the SSE
      transport is vendored from the oracle's own solution, and both fork patches
      turned out to be already upstream. Nothing here needs a maintainer decision.
      Original entry:
      **NEEDS-MAINTAINER-DECISION 2026-08-17 (audit-debt triage) — premise true,
      but it is a MAJOR-VERSION migration, not a sourcing preference, and the
      stated benefit is false.** Re-verified: `Cargo.toml:443` pins
      `warpdotdev/rmcp` at `c0f65dc441af7d714b9c453ac5e7ef641451abe3`;
      `42effe840:Cargo.toml:420` is `rmcp = { version = "1.6" }`. What the entry
      does not say is the version: `Cargo.lock:11643-11645` resolves that git rev
      to **rmcp 0.10.0**. So this is **0.10 → 1.6**, across a 1.0 stabilisation —
      expect API breakage, not a rev bump.

      **Size: 22 `.rs` files, 176 `rmcp::` references**, concentrated in
      `app/src/ai/mcp/` (`templatable_manager.rs` + `native.rs` + `oauth*`,
      `reconnecting_peer.rs`, `http_client.rs`), `app/src/ai/agent/api/`, and
      `app/src/ai/blocklist/action_model/execute/{call_mcp_tool,read_mcp_resource}.rs`.
      Note the pin has *also* restructured this code into a `crates/mcp` crate
      (`42effe840:crates/mcp/src/{lib,runtime,oauth}.rs`) that this fork does not
      have, so "follow upstream" is two changes, not one.

      **The stated benefit is wrong: `deny.toml`'s `allow-git` would not get
      shorter.** `deny.toml:69-75` lists only `jwp2987/winit` and
      `servo/core-foundation-rs`; every `warpdotdev` repo is covered by
      `allow-org = { github = ["warpdotdev"] }` at `:77`. **16 other
      `warpdotdev/*` git deps remain** in `Cargo.toml` (`command-corrections`,
      `font-kit`, `mermaid-to-svg`, `notify`, `vte`, `workflows`,
      `warp-proto-apis`, `command-signatures`, `rust-objc`, `pathfinder`,
      `yaml-rust`, `tink-rust` ×3, `jemallocator` ×2), so dropping `rmcp` removes
      no line from either list.

      **Question: do we migrate `rmcp` 0.10 → crates.io 1.6, yes or no?** If no,
      this closes with a `DECLINED.md` row saying the fork stays on the
      `warpdotdev` fork rev deliberately. **Release relevance: no** — the current
      pin builds and MCP works; this is dependency hygiene. Post-release, and it
      is the largest single item in this batch.
      Original finding:
      **`rmcp`: fork pins `warpdotdev/rmcp` at `c0f65dc44`; upstream has moved
      to crates.io `version = "1.6"`.** Not a lag — a different sourcing
      strategy. Decide whether to follow upstream onto the published crate
      (fewer git deps, `deny.toml`'s `allow-git` gets shorter) or keep the fork
      pin, and record whichever it is.

### Correct as they stand — do not "fix" these

- **`winit`** differs deliberately and is documented in `Cargo.toml`: the fork
  carries one extra commit (rust-windowing/winit#4453, Windows dark-mode
  detection via registry), open and unreviewed upstream since 2025-12-27.
- **`tink-core` / `tink-proto` / `tink-hybrid`** are upstream-only and correctly
  absent: they back `crates/managed_secrets`' envelope encryption, and this fork
  wires `DisabledManagedSecretsClient`.

## FOLLOW-UP FROM #600 (2026-08-15) — the *other* unconditional plugin install

#600 fixed the local-child-pane call site
(`app/src/pane_group/pane/local_harness_launch.rs`, now
`ensure_local_claude_child_plugins`). Tracing the pin's shape for that fix
surfaced the same divergence at the second call site, which #600 deliberately
did **not** touch — it is a different pin function and a different failure
mode. Recorded here rather than as a GitHub issue because the agent that found
it had no remote-write authority.

- [x] **`AgentDriver::setup_harness` installs the notification plugin
      unconditionally** (`app/src/ai/agent_sdk/driver.rs:1125`,
      `if let Err(e) = manager.install().await`). The pin splits this into
      `setup_harness_plugins` → `setup_notification_plugin`
      (`driver.rs:2671-2723` at `02b53fcd8`), which does two things this fork's
      version does not: it **returns early when `!manager.can_auto_install()`**,
      and it branches `needs_update()` → `update()`, else `!is_installed()` →
      `install()`. Two consequences here. First, every third-party-harness
      launch shells out to a full `plugin marketplace add` + `plugin install`
      even when the plugin is already current — the same redundant subprocess
      work #600 removed from the child-pane path. Second, and unlike the
      child-pane path, `plugin_manager_for` can hand back a manager whose
      `can_auto_install()` is `false`: `OpenCodePluginManager` and
      `DeepSeekPluginManager` both return `false` unconditionally, and
      `CodexPluginManager` returns `FeatureFlag::CodexPlugin.is_enabled()`.
      For those, `install()` falls through to the trait's default impl, which
      returns `Err("Auto-install not supported for this agent")` — so this line
      logs a warning on **every** OpenCode/DeepSeek harness launch, describing
      a failure that is really "this agent was never auto-installable".
      Note the marketplace-override guard is *not* part of this one: the pin's
      `setup_notification_plugin` does not call
      `has_local_marketplace_override` either, only
      `ensure_local_claude_child_plugins` does. Port the `can_auto_install` +
      `needs_update`/`is_installed` shape only. The pin's version also wraps each
      call in `SetupClientEventReporter::record_result`; that reporter has no
      definition anywhere in this fork (`grep -r SetupClientEventReporter app/src`
      is empty), so port the branching and drop the reporting, rather than
      dragging in a telemetry surface to carry it.
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Fixed: `driver.rs:1141` calls `setup_notification_plugin` (`:1204-1215`), which gates on `can_auto_install()` then `needs_update()`/`is_installed()` — the pin shape this entry says is missing. Commit `a4b2ce3fb`.

## WINDOWS TUI INSTALLER — deferred 2026-08-15 (maintainer's call)

- [x] **DEFERRED PAST v0.1.0 2026-08-18 — not a port, and its main purpose is already declined.**
      ⚠️ **Maintainer may reverse this.** Reasoning: these two commits ship a signed Inno
      installer **and rework TUI autoupdate to download and run it** — and *"this fork does
      not ship autoupdate"* is already a recorded maintainer decision (see the entry directly
      below, and the TUI-autoupdate row in `DECLINED.md`). So the installer's primary consumer
      does not exist here. The standalone first-install case is also moot for now: the fork's
      `script/windows/bundle.ps1` has **no `tui` artifact at all** and the TUI install layout
      is Unix-only (`#[cfg(unix)]`, symlinks), so there is no packaged Windows TUI for an
      installer to install. The entry's own words were "Deferred, not declined" — this only
      makes that explicit so it stops counting as open release work. Revisit when Windows
      packaging is worth investing in. Original entry:
      Upstream `ddba1684e` / `d9ed47239` ship a signed Inno Setup installer for
      the TUI and rework TUI autoupdate to download and run it. **Deferred, not
      declined** — revisit when Windows packaging is worth investing in.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — **Two of the three named features exist here.** `nld_heuristic_v2` and `voice_input` are present (`app/Cargo.toml:227,667,673,823`, `root_view.rs:262-264`); only `nld_classifier_v3` is genuinely absent. The `bundle.ps1`-has-no-tui and unix-only-autoupdate claims stand (`autoupdate.rs:917-920`).

      What it needs, so the next person does not re-derive it: the fork's
      `script/windows/bundle.ps1` has no `tui` artifact at all; the TUI install
      layout here is Unix-only (`#[cfg(unix)]`, symlinks); and the commits carry
      `warp_tui` feature additions that do not exist in this fork
      (`nld_classifier_v3`, `nld_heuristic_v2`, `voice_input`). So this is a
      packaging project, not a port.

      Related and already tracked separately: the Windows smoke suite above is
      at 5/19 with GUI bootstrap as the blocker.

## HOMEBREW-MANAGED TUI UPDATES — SCOPE-DECISION 2026-08-15 (needs a cask identity)

- [x] **DECLINED 2026-08-17 (maintainer): this fork does not ship autoupdate.**
      `DECLINED.md` (TUI autoupdate row) names this port and says do not do it.
      Original entry: Upstream `f4be4f692` (#14899) makes TUI autoupdate package-manager-aware:
      a `Homebrew` arm on the install-method detection that only *checks* for a
      newer version and renders `brew upgrade --cask <token>` instead of staging
      an install, leaving Homebrew-owned files alone. The mechanism is sound and
      non-cloud, and the fork's `crates/warp_tui/src/autoupdate.rs` still has the
      pre-change shape it grafts onto (`InstallLayout::detect`,
      `AutoupdateEligibility::Enabled(layout)`, `check_now`), so the port is
      mechanically straightforward.

      **It is blocked on a product decision, not on code.** Upstream keys the
      detection on the binary `warp-tui-stable` sitting under
      `Caskroom/warp-agent-cli`, and renders that cask token in a user-visible
      status string. Porting those literals gives this fork permanently dead
      code whose only tests can never fire, and — worse than dead — a status
      line telling a Phosphor user to run a `brew` command that installs Warp.

      What is missing, so the next person does not re-derive it:

      1. **No Homebrew distribution exists for this fork at all.** Upstream's
         companion PRs are a tap (`warpdotdev/homebrew-warp`, cask
         `warp-agent-cli`) and an automated cask-bump job in
         `warpdotdev/channel-versions`. Neither has a Phosphor counterpart, and
         nothing in this tree references a tap, a cask, or `brew` as a
         distribution channel.
      2. **The binary name the detector keys on does not exist here.**
         `crates/warp_tui/Cargo.toml` sets `autobins = false` and declares
         exactly one bin, `zap-tui-oss`. `src/bin/stable.rs` is present as
         source but is deliberately undeclared — it needs `warp_channel_config`,
         which this fork does not have.
      3. **That name is itself in flux.** Unmerged branches rename
         `zap-tui-oss` to `phosphor-tui-oss`. Choosing a cask token before that
         lands means choosing twice.

      To unblock, the maintainer needs to settle three things: whether Phosphor
      distributes via Homebrew at all; if so, the tap repo and cask token; and
      the release binary name after the zap→phosphor rename. **Do not invent a
      cask name to make the port compile** — the token is a public identifier
      and a user-visible string, not an implementation detail.

## RE-PIN AUTOMATION -- build during catch-up, pays off at pin N+1

Decided 2026-08-08. The catch-up against `02b53fcd8` is the FIRST pass and is
expensive by nature. Moving the pin later repeats tonight's motions, and most of
it can be mechanised -- but only if the inputs are recorded WHILE the first pass
happens. Retrofitting them afterwards costs as much as the pass itself.

**Mechanisable, worth building:**
- [x] **Identical-to-pin manifest.** **DONE 2026-08-11** (`script/generate_pin_identity_manifest`
      + `docs/PIN-IDENTITY-MANIFEST.md`). Compares git blob hashes (not
      per-file `git show`) between the pin and fork HEAD for every `.rs` file
      under `app/src`/`crates`. Current measurement: **572 identical (17%),
      2334 differ, 460 fork-only** of 3366 fork files scanned. Regenerate with
      the script; it is a snapshot, not a live gate.
- [x] **Re-pin work queue generator.** **DONE 2026-08-11** (`script/generate_repin_queue
      [<new-pin>] [<old-pin>]`). `git diff <pin N> <pin N+1>` over test-bearing
      files, bucketed by inherited `SCOPE-*.md` verdicts (explicitly labelled a
      verdict, not a fact) and a pin-source cloud-import check in the spirit of
      `script/check_cloud_boundary`, plus a DECLINED.md-marker collision check.
      Runnable now with no `<new-pin>` (self-diffs the current pin, trivially
      empty) and demoed against `warp/master` (549 files changed, 177
      test-bearing -> 11 declined-collision, 20 unclassified, 52 actionable, 50
      low-priority, 44 cloud-dropped; math reconciles exactly).
      **UPDATED 2026-08-11:** now also consumes `docs/sweep-verdict-ledger.tsv`
      (see the next item) and splits a ledger-covered file's output into
      three kinds instead of one -- carried forward (untouched, not printed
      as work), RE-EXAMINE with a specific reason (three checkable
      invalidation rules), or genuinely new. Demoed again against
      `warp/master`: of 1,843 ledger rows, ~1,025-1,066 carried forward
      untouched and ~62 files' worth (~813-818 tests) flagged RE-EXAMINE
      because the pin file itself changed -- exact counts vary slightly
      run-to-run in this environment (see the caveat below, not a bug in the
      ledger logic: `total_test_bearing` itself varied by 1 file between
      otherwise-identical invocations, traced to the pre-existing
      `is_test_bearing`/`is_cloud_touching` `git show`-per-file calls against
      a shallow `--depth=1` fetch of `warp/master`, not to anything this
      change added -- worth a maintainer look, out of scope here).
- [x] **Sweep-verdict ledger.** **DONE 2026-08-11** (`docs/sweep-verdict-ledger.tsv`,
      built by `script/extract_sweep_ledger.py`, validated by
      `script/check_sweep_ledger`). Extracts all 1,843 per-test verdicts from
      the six `docs/sweep/*.md` prose files (1,841 per their own stated
      totals; the ledger trusts each doc's per-file section headers over its
      top-level totals table, which is what four of the six docs' own
      arithmetic disagrees with itself about) into one TSV, cross-validated
      against `docs/SWEEP-INVENTORY.md`'s per-file name registry so no
      hallucinated test name can enter the ledger. 90% of rows extracted
      cleanly (exact per-test citation), 10% by a documented, verified
      inference, 0.3% left genuinely unresolved (the sweep itself declined to
      bucket them). `script/check_sweep_ledger` is wired into
      `script/precheck`'s guards step and `pr-check.yml`'s `guards` job,
      continuously (not just at re-pin) -- a `DECLINED.md` row can be struck
      at any time, and a ledger row still citing it is wrong from that
      moment. See `docs/SWEEP-SUMMARY.md`'s new "The ledger" section for the
      re-pin procedure and the four invalidation rules (two machine-checked,
      one partially, one deliberately not).
- [x] **Divergence-collision guard.** **DONE 2026-08-11** (`script/check_declined_collisions`,
      wired into `script/precheck`'s guards step and the `pr-check.yml` `guards`
      job). `DECLINED.md` rows now carry `<!-- markers: kind:value ... -->`
      comments (`test:`/`sym:`/`path:`/`keep:` -- see DECLINED.md's new
      "Machine-checkable markers" section for the convention and the false-
      positive trap it documents, found live during the retrofit:
      `VoiceInputLifecycle` looked markable and was not, because that exact
      name is legitimately reused by `crates/voice_input`'s real audio-capture
      state machine). 19 of ~33 active `DECLINED.md` rows retrofitted (37
      individual markers); the rest name nothing exact enough to check safely
      and were left unmarked on purpose. Runs clean on the current tree (0
      findings, all 4 marker kinds verified to fire on injected true
      positives, cleaned up afterward).
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
- [x] #342 port `repository_gated_command_{drops_when_leaving,stays_within}_repository`.
      Blocker removed: `simulate_directory_for_completion` exists at `app/src/terminal/input_test.rs:515`.
      Pin source: `app/src/terminal/input/slash_command_model_tests.rs:556,627`.
      NB the issue title garbles the first test name.
- [x] #410 util/bindings: two editable-binding regressions vs the pin.
      Verified: fork declares AND registers `TOGGLE_MAXIMIZE_PANE_BINDING_NAME`
      (`pane_group/mod.rs:184,434`) but never uses it at the pin's second site,
      `terminal/view/pane_impl.rs:692`.
- [x] #436 warpui_core TuiViewportedList: no trimmed-selection-line-ends option.
      Verified absent; pin has `trim_selection_line_ends` + `trimmed_selection_row_end`
      in `crates/warpui_core/src/elements/tui/viewported_list.rs:21,168,438`.
- [x] #498 file tree: `show_hidden_files` has no Settings toggle / palette action.
      Verified: setting IS read (`code/file_tree/view.rs:357,418,726,1704`), no UI entry.
- [x] #549 duplicate dead test-fixture helpers. Verified: `app/src/test_util/virtual_fs.rs`
      and `crates/virtual_fs/src/lib.rs` both define `git_repository_fixture`/`executable`/
      `fixtures`; the ONLY callers are each file's own `git_repository_fixture` calling its
      own `fixtures()`. Trap: delete inner-first or you break the self-reference.
- [x] #547 view_components: ActionButton.callout / AlertConfig::success / Dropdown::Naked unwired.
      `AlertConfig::success` verified at zero uses; confirm the other two individually.
- [x] #552 search/ai_context_menu: `render_search_bar` never called. Verified: defined at
      `app/src/search/ai_context_menu/view.rs:1656`, no call site. (The same-named methods in
      command_palette/welcome_palette/theme_chooser ARE called — do not confuse them.)
- [x] #555 prompt/editor_modal: same-line-prompt toggle UI missing. Verified:
      `render_same_line_prompt_section` defined once at `app/src/prompt/editor_modal.rs:592`,
      never called.
- [x] **CLOSED 2026-08-18 — #532 done, DCS session-id validation is LIVE. UNVERIFIED (not compiled).**
      The gate at `terminal_model.rs:2730` is now the pin's `!self.shared_session_status().is_viewer()`,
      and `sharer_rejects_dcs_hook_with_unregistered_session_id` is un-`#[ignore]`d.
      **The stated blocker was resolved, and I re-verified it rather than taking the report:**
      the three fork-only hooks (`RemoteWarpificationIsUnavailable`, `SshTmuxInstaller`,
      `TmuxInstallFailed`) all carry an ID. `dcs_hooks.rs:158-160` returns it and `:192-194`
      requires it; the `_log`/`_l` helpers in `warpify_ssh_session*.sh` build the JSON with
      `@@WARP_SESSION_ID@@`; 11 of the 13 scripts under `app/assets/bundled/ssh/` carry the
      placeholder and **the 2 that do not (`install_tmux_and_warpify_brew.sh`, both shells)
      emit nothing at all**, so they need none.
      **Substitution is real on both paths** — `warpify.rs:214` for the warpify scripts and
      `install_tmux.rs:555`/`:603` for the eight tmux installers, the latter reached through
      function-local `use` of `SESSION_ID_PLACEHOLDER` (which is why a grep for the literal
      `@@WARP_SESSION_ID@@` in `app/src` appears to show no substituter — it is not there,
      the constant is). `test_warpify_scripts_substitute_session_id` asserts no placeholder
      survives across all 6 uname/shell combinations.
      **TOFU was superseded, not adopted.** Original entry:
      **UNVERIFIED (not compiled).**
      ⛔ **Strike the "PRODUCT DECISION 2026-08-17: trust-on-first-use" block and
      its "six verified mint sites" premise. Both rest on a false reading of the
      pin, which I supplied.** The pin does NOT have this problem: it mints
      `remote_session_id` **locally** from `od -An -N8 -tu8 /dev/urandom`
      (`42effe840:bash_body.sh:1006`), bails to plain `ssh` if it cannot, bakes it
      into the remote shell's `WARP_SESSION_ID` (`:1027`/`:1117`), reports it on the
      `SSH` hook's `remote_session_id` field from the **already-registered local
      session** (`:1082`), and `TerminalModel::ssh` rejects a missing or zero value
      before registering (`42effe840:terminal_model.rs:3081-3091`). Verified
      directly. The earlier reading took the FORK's `$(date +%s)$RANDOM` and assumed
      upstream shared it.
      **So the pin's design was implemented, not TOFU** — strictly stronger, because
      there is no trust-on-first-use window at all, and no id asserted by the pty is
      ever registered. The ⚠️ low-entropy caveat is **resolved**: 64 bits from the
      kernel CSPRNG replaces ~15 bits, and `$(date +%s)$RANDOM` is gone from all
      three shells. Registration is now 5 production sites.
      **Also done:** `session_id` threaded through **14 of 17** hook variants (Rust +
      all three shells), KV-pair parsing restored, `unknown_init_subshell.sh` now
      byte-identical to the pin, 48 struct-literal call sites updated across 11 files.
      **Extra defect found and fixed:** `Bootstrapped` omitted `session_id` in
      `zsh_body.sh` and `fish.sh` (bash had it) — a silent gap at flip time.
      **STILL OPEN, and this is now the SOLE gate blocker:** three fork-only hooks —
      `RemoteWarpificationIsUnavailable`, `SshTmuxInstaller`, `TmuxInstallFailed` —
      are emitted by the 19 remote-side scripts under `app/assets/bundled/ssh/` (a
      directory the pin does not have), run on the remote host **before any session
      exists there**, carry no id, and have `requires_registered_session() == true`.
      Flipping the gate now would silently drop them — a failed remote warpification
      would hang with **no error block**. Threading them means templating an id into
      19 assets and reshaping `SshTmuxInstaller` from a bare string to an object, and
      the two macOS variants sit under a hard <1020-byte pty limit. Real work.
      The gate stays `false` (`terminal_model.rs:2731`) and
      `sharer_rejects_dcs_hook_with_unregistered_session_id` stays `#[ignore]`d with
      an accurate reason; its body is now byte-identical to the pin's.
      **Behaviour change live NOW, not behind the gate:** the `SSH` hook hard-rejects
      a missing `remote_session_id`. Safe — the only three emitters are the bundled
      scripts, and `pwsh.ps1` has no ssh wrapper — but worth naming.
      **One deliberate deviation from the brief:** the fallback when `/dev/urandom`
      is unavailable is *plain unwarpified ssh* (the pin's behaviour), which is worse
      than today on such a host rather than "no worse". Pin parity was chosen over
      inventing an unverifiable path, since any other fallback reinstates a guessable
      credential. Reversible on request. Original entry:
      **#532 — REOPENED 2026-08-11. The 2026-08-08 closure was wrong.** It is the
      **fifth** entry in this file found stating the opposite of the code (#148 class).
      Original closure text, kept verbatim as the evidence of how it failed:
      > "#532 CLOSED 2026-08-08: #419 has now landed (recovered from PR #538) and
      > `requires_registered_session`, `is_registered_session`, and
      > `should_validate_dcs_hook_session_id` are present in
      > `app/src/terminal/model/ansi/{dcs_hooks,mod}.rs` and `terminal_model.rs`."
      **RESCOPE 2026-08-17 (v0.1.0 agent audit)** — "0 production call sites" is false: `terminal_manager.rs:816`, `remote_tty/event_loop.rs:172` and `view.rs:13613` all register (commit `1f793fb0d`).
      **SUPERSEDED 2026-08-20.** The rescope's own residue — "`should_validate_dcs_hook_session_id` remains hardcoded `false`" — is itself false and contradicted the CLOSED note at `:5739` in this same file. The gate at `terminal_model.rs:2728-2730` reads `!self.shared_session_status().is_viewer()`, i.e. the pin's role-conditional form, and both gate tests run un-`#[ignore]`d (`terminal_model_test.rs:2257`, `:2311`). Nothing is left open here.

      **PRODUCT DECISION MADE 2026-08-17 (maintainer): trust-on-first-use.**
      This settles the *second* blocker found today — the SSH warpify wrapper
      mints `WARP_SESSION_ID="$(command -p date +%s)$RANDOM"` **remotely**, at six
      sites verified today: `app/assets/bundled/bootstrap/bash_body.sh:1001,1057`,
      `zsh_body.sh:902,957`, `fish.sh:624,674`. No local `register_session_id` can
      ever have seen such an id, so strict validation would reject every remote
      warpified session.

      **Chosen design: register the remotely-minted id on receipt of its
      `InitShell` hook (TOFU), then validate every subsequent hook against it.**

      **#532 stays OPEN — the decision unblocks it, it does not complete it.** The
      *first* blocker is untouched: `DProtoHook::session_id()` returns `None` for
      10 of 13 variants, and this fork's bootstrap scripts emit no `session_id` on
      those hooks at all.

      ⚠️ **Caveat to carry into the implementation, recorded so it is not
      discovered later.** The remote id is a second-resolution timestamp plus 15
      bits of `$RANDOM`. TOFU closes the "unregistered id is accepted" hole for
      hooks arriving *after* the first `InitShell`, but the id itself stays
      low-entropy and guessable, and anything that can write to the PTY before the
      first `InitShell` can still seed it. **Strengthen the id generator alongside
      the TOFU work**, not after it.

      **It closed on SYMBOL PRESENCE, not on the wiring those symbols exist to serve.**
      All three symbols are present. `should_validate_dcs_hook_session_id` returns a
      hardcoded `false` (`terminal_model.rs:2700`); the pin returns
      `!self.shared_session_status().is_viewer()` (`02b53fcd8:…:2569`). Present and inert.

      **Measured 2026-08-11** — `register_session_id` call sites:
      | | pin | fork |
      |---|---:|---:|
      | total | 5 | 1 |
      | production | **4** | **0** |

      Pin's production sites: `terminal/local_tty/terminal_manager.rs:629` (PTY spawn),
      `terminal/model/terminal_model.rs:3090` (remote session),
      `terminal/remote_tty/event_loop.rs:175`, `terminal/view.rs:14988`.
      The fork's single call is `terminal_model.rs:1091`, inside a
      `#[cfg(any(test, feature = "test-util"))]` fixture with `session_id = 123`.

      **The fork's own code already documented this**, 150 lines from the function:
      `terminal_model.rs:2691` says *"nothing yet calls `register_session_id` at
      PTY-spawn time … See #419's follow-up for the PTY-spawn wiring."* The closure
      contradicted a comment in the same file.

      **Scope is smaller than "missing subsystem":** both pin files that perform the
      registration — `terminal/local_tty/terminal_manager.rs` and
      `terminal/remote_tty/event_loop.rs` — **already exist here**. The calls were
      never added. This is wiring into existing files.
      Unblocks the 2 DCS tests in the MISSING-SUBSYSTEM bucket
      (`sharer_rejects_dcs_hook_with_unregistered_session_id`,
      `viewer_processes_dcs_hook_with_unregistered_session_id`).

### Tier 2 — small (~half a day each)
- [x] **CLOSED — MAINTAINER DECISION 2026-08-18.** Both halves are done or answered;
      what remained needed the app running and a profiler, which this effort does not
      have.
      **Min-size half: DONE and nothing was missed.** `vertical_tabs.rs:89` is
      correctly still `MIN_PANEL_WIDTH: f32 = 200.` — a different const for a different
      panel, already below the 240 the AI panel was lowered to.
      **Drag-latency half: DIAGNOSED, and the two real divergences it found were both
      PORTED** — (a) macOS async Metal presentation restored (`host_view.m` and
      `entity.rs` now byte-identical to the pin; every frame was blocking on
      `waitUntilCompleted`), and (b) `EntityIdMap`/`EntityIdSet` un-gated so the
      presenter hot path uses FxHash rather than SipHash. So the actionable findings
      shipped even though the issue itself is closed.
      **What is deliberately NOT being pursued**, recorded so it is not re-raised as an
      oversight: the remaining costs are **shared with the pin** and are not fork bugs —
      ~3 O(blocks) SumTree rebuilds per drag frame, a full per-grapheme scrollback
      reflow on every cell-column crossing (`flat_storage/index.rs:101-147`, which
      carries its own upstream `TODO(vorporeal)` admitting it is unoptimized), and
      non-grid text re-shaped each frame because `max_width` is in the layout cache key.
      Terminal grid glyphs are NOT width-invalidated, so the grid itself is fine.
      **If this is ever reopened, the cheapest discriminator is one question: what OS is
      the reporter on?** If not macOS, divergence (a) is killed outright. Second
      cheapest, no profiler needed: time the same drag on a deep scrollback vs a fresh
      session — if latency scales with history, the reflow dominates and the fix is
      `update_old_blocks = false` while dragging (the seam exists at
      `terminal_model.rs:2160-2174`). Third: log `resize_rx.len()` mid-drag; >1 means a
      cumulative backlog, since that channel has neither `throttle` nor `debounce`
      where its siblings have both. Original entry:
      **DIAGNOSED 2026-08-18 by reading; min-size half CLOSED; two real divergences
      found, both now being ported.**
      **Sub-item 1 (min size) is DONE and nothing was missed.** `vertical_tabs.rs:89`
      (this file said `:88` — drift) is still `MIN_PANEL_WIDTH: f32 = 200.` and
      **correctly so**: it is the tab-sidebar floor, a different const for a different
      panel, already BELOW the 240 the AI panel was lowered to. Both consts accounted
      for.
      **⛔ The lead in this entry was misleading: `is_being_resized()` is NOT a resize
      fast path.** Its only caller suppresses hover-focus
      (`pane_group/mod.rs:5061`, def `:5137`), identically to the pin. Do not start there.
      **The whole app-layer resize path is fork ≡ pin** — `pane_group/{mod,tree}.rs`,
      `TerminalView::resize_internal`, `TerminalModel::resize`, `BlockList::resize`,
      `grid/resize.rs`, block-list layout/paint, `text_layout.rs`, the winit/mac event
      path, and the absence of layout caching. No dropped fast path in `app/`.
      **Two divergences, both in `crates/warpui*`, both upstream optimizations this
      fork's base predates and never received. Neither has a `DECLINED.md` entry.**
      **(a) PORTED 2026-08-18 — macOS async presentation restored. UNVERIFIED: no Rust
      AND no Objective-C compiler ran; this path cannot be built on this Linux host at
      all.** `finish_with_capture` now takes `presents_with_transaction: bool` and, when
      `!should_capture && !presents_with_transaction`, does `presentDrawable` + `commit`
      and returns early — **no `waitUntilCompleted`**. `host_view.m` and
      `crates/warpui_core/src/core/entity.rs` are now **byte-identical to the pin**;
      `renderer.rs` line numbers are the pin's minus one throughout (the pin has one
      extra import). The live-resize `YES`/`NO` toggle is preserved per-window exactly
      as the pin has it — **not** simplified to a constant — and `setAsyncCallback`
      ordering is untouched.
      **Unverifiable here, listed so a Mac build knows where to look:** the ObjC
      selector and the `CAMetalLayer *` cast are unchecked; the
      `ProtocolObject::from_ref` upcast from `dyn CAMetalDrawable` to `dyn MTLDrawable`
      is inferred (same line compiles at the pin with identical `objc2-metal` /
      `objc2-quartz-core` 0.3.2 and identical feature lists); and whether resize is
      still tear-free needs a run. The drop path was reasoned through — the early
      return sets `encoding_finished = true`, so `Drop for RenderPass` will not call
      `endEncoding()` twice.
      Original finding: **macOS presents SYNCHRONOUSLY every frame.** `host_view.m:278` is
      `presentsWithTransaction = YES` with **no setter**; the pin's `:282` is `NO` plus
      a setter (`host_view.h:16`, `host_view.m:156-158`) and a live-resize `YES`/`NO`
      toggle (`window.m:176`/`:185`). The fork's `renderer.rs:73-95` runs
      `commit(); waitUntilCompleted();` unconditionally where the pin's `:74-90` takes
      `presents_with_transaction: bool` and early-returns without waiting. So frame N's
      GPU work cannot overlap frame N+1's CPU work — **every frame, not just resize**.
      Verified directly. Agent porting it now.
      **(b) PORTED 2026-08-18 — `EntityIdMap`/`EntityIdSet` un-gated, 17 sites retyped.**
      `entity.rs` byte-identical to the pin; `presenter.rs` (13 sites) plus
      `debug/{root_view,view_tree_debug_view}.rs` (4 sites, required for coherence since
      `parents()` feeds them — and exactly what the pin does). `rustc-hash` was already
      an unconditional dependency, so no `Cargo.toml` change. `HashMap`/`HashSet` remain
      for the `TaskId`- and `AssetHandle`-keyed collections, same as the pin.
      **Deliberately NOT ported:** the pin also uses these aliases in `core/app.rs`,
      `core/mod.rs`, `core/window.rs`, `keymap/matcher.rs` and `core/autotracking/` —
      retyping `WindowInvalidation` in particular would ripple into `crates/warp_tui`
      test files that live agents own. **Note the removed `cfg(feature = "tui")` gate
      cited `specs/warp-oss-sync/SCOPE.md`, which contains no mention of `EntityIdMap`**
      — so it was a local minimal-diff choice by the TUI-runtime port, not a recorded
      decision. Original finding: **Presenter hot path uses SipHash.** `warpui_core/src/core/entity.rs:29-32`
      gates `EntityIdMap` behind `cfg(feature = "tui")` and lacks `EntityIdSet`; the
      pin declares both unconditionally (`:17,20`). ~4 SipHash-1-3 hashes per view per
      frame instead of multiply-xors. Real but ~an order of magnitude smaller than (a).
      **Inherent costs SHARED with the pin (not fork bugs, but they set the ceiling):**
      ~3 O(blocks) SumTree rebuilds per drag frame per resized pane
      (`blocks.rs:2551`, `block_list_element.rs:3234`, `:3336`); a full **per-grapheme**
      scrollback reflow on every cell-column crossing (~every 8px of horizontal drag)
      via `flat_storage/index.rs:101-147`, which carries its own `TODO(vorporeal)`
      admitting it is unoptimized; and non-grid text re-shaped every frame because
      `max_width` is in the layout cache key (`text_layout.rs:320-326`, 2 generations).
      Terminal grid glyphs are NOT width-invalidated (`CellGlyphCache` is keyed
      `(char, FontId)`), so the grid itself is fine.
      **CHEAPEST NEXT STEP once someone can run it: establish the reporter's OS.**
      If not macOS, (a) is killed outright. Second cheapest, no profiler needed: time
      the same drag on a deep scrollback vs a fresh session — **if latency scales with
      history, the reflow dominates** and the fix is to pass
      `update_old_blocks = false` while `dragged_border.is_some()` (the seam already
      exists at `terminal_model.rs:2160-2174`) and do one full reflow at
      `end_resizing`. That one would be an upstream-diverging change needing its own
      issue. Third: log `resize_rx.len()` mid-drag — the channel is unbounded with
      neither `throttle` nor `debounce` where its siblings have both
      (`view.rs:3538` vs `:3527-3536`); **>1 means a cumulative backlog**, which fits
      "feels slow" better than "feels choppy".
      Note `crates/warpui_core` already has a zero-cost `traces` cargo feature for
      exactly this — but `end_trace_after_next!("window:redraw:end")`
      (`view.rs:10747`) waits on an event name nothing records, so that harness is
      half-wired and needs the missing `record_trace_event!` first. Original entry:
      **Zap #324 — pane resize: drag feels slow, and the minimum panel size is
      too large.** Two independent halves; the second is nearly free.
      1. **Minimum size.** `MIN_PANEL_WIDTH: f32 = 300.`
         (`app/src/ai_assistant/panel.rs:61`) is exactly the ~300px floor the
         reporter hits; `workspace/view/vertical_tabs.rs:88` has its own `200.`.
         Both are hardcoded consts. On a 1600px-high screen the floor costs real
         estate for no reason. Lower them, or make the floor proportional to the
         window — **check both consts, they are not shared.**
      2. **Drag latency.** Not diagnosed. `pane_group/mod.rs:4819` already
         special-cases `is_being_resized()`, so start by measuring whether the
         drag re-lays-out the whole group per frame. **Do not guess at this
         half — it needs the app running.**
      Note the numbering trap: **Zap #324 is unrelated to Phosphor #334**, which
      was this fork's own divider work (reset + double-click) and is already
      done. Same subsystem, different issue, colliding numbers.
      **RESCOPE 2026-08-17 (v0.1.0 agent audit)** — the min-size half is fixed: `ai_assistant/panel.rs:68` is now `MIN_PANEL_WIDTH = 240.` (commit `390453a94`, citing #324). The drag-latency half is untouched and undiagnosed. Keep only that.
- [x] **CLOSED 2026-08-17 — THE PREMISE IS FALSE. `git pull` is fully built.**
      Verified in the tree: `app/src/code_review/git_dialog/pull.rs` builds
      `proto::GitPullRequest` (`:125`), calls `client.git_pull(..)` (`:130`), and
      handles both `git_pull_response::Result::Success(delta)` (`:134`) and
      `::Error(e)` (`:144`); `app/src/remote_server/server_model.rs` imports the
      types (`:23`, `:30-31`) and dispatches `handle_git_pull` (`:1214`). The proto
      slot `git_pull_response = 38` is **wired, not a reserved stub**.
      **This is the sixth entry in this file found stating the opposite of the code
      (#148 class)** — and worse, the correction already existed: the Zap #329 entry
      carries a RESCOPE 2026-08-17 note reading *"the 'pull ABSENT' row is false:
      pull is fully built"*. This entry was simply never updated to match, so the
      tracker contradicted itself in two places. An agent was dispatched to build it
      before the contradiction was caught, and was stopped and reverted.
      Original entry:
      **`git pull` — the one Git verb the fork has no path for at all.**
      Raised by Zap #329 (see the upstream-issues section for the full triage;
      the other two gaps there are hunk staging and branch create/switch).
      Zero hits for `git_pull`/`GitPull` in the tree — this is absence, not a
      stub.
      **RESCOPE 2026-08-17 (v0.1.0 agent audit)** — Stage 1 is done: `git pull --ff-only` exists end to end (`code_review/git_dialog/pull.rs`, `handle_git_pull`, proto, client, round-trip test). Stage 2 remains accurate — `global_buffer_model.rs:2390`'s `resolve_conflict` is buffer-sync, not git-merge, and no merge-conflict UX exists. Keep only Stage 2 open.
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Confirmed accurate under renewed attack, with one path correction: `global_buffer_model.rs:2390` is now `code/global_buffer_model.rs:2390` (module reorg). Stage 1 is fully wired (`util/git.rs:881` `run_pull`, `server_model.rs:2695`, proto `:963`, two round-trip tests at `client_tests.rs:694,728`); invalidation is handled via `refresh_after_git_operation`/`DiffStateWatch` rather than the `FileInvalidationTask` the entry expected. Stage 2 genuinely absent.

      **Do it in two stages, because pull is NOT symmetric with push.** Push
      never touches the working tree; pull does, and that is the entire
      difficulty:

      **Stage 1 — `git pull --ff-only` (this Tier 2 item).** A fast-forward
      cannot conflict, so the hard half is designed out. Mirror `run_push`
      exactly; every piece already exists:
      | layer | mirror |
      |---|---|
      | local git | `util::git::run_push` → add `run_pull`; `run_git_command` is already generic |
      | proto | `GitPushRequest/Response` → `GitPullRequest/Response`, reusing `GitOpDelta`/`GitOpError` (`remote_server.proto:940`) |
      | daemon | `handle_git_push` (`server_model.rs:2613`) — including its `guard_git_operation_in_progress` lock |
      | client | `client.git_push` → `git_pull` |
      | UI | `git_dialog/push.rs` (440 lines, already has the local/remote split at `start_confirm_remote`) |
      Working-tree changes must then be fed to `FileInvalidationTask`
      (`code_review/file_invalidation_queue.rs`) or open buffers will render
      stale content after a successful pull. **That is the step most likely to
      be forgotten, because push never needed it.**

      **Stage 2 — merging pull, i.e. conflicts. NOT Tier 2, do not bundle it.**
      Needs a conflict-resolution UX. Note `crates/remote_server/src/client/mod.rs:898::resolve_conflict`
      still has **zero non-test callers** (verified 2026-08-11 — the elsewhere
      entry in this file is accurate); the only `resolve_conflict` callers are
      `global_buffer_model.rs:2376` via `server_model.rs:4178`, which is the
      *buffer* sync conflict path, **a different thing from a git merge
      conflict**. Do not assume that path can be reused without reading it.

- [x] #523 cmd-k: `try_clear_buffer_in_agent_view` still checks only `is_agent_monitoring`
      (`clear_buffer` was fixed; this one guard remains)
- [x] #545 CLI-agent image paste: keystroke is still agent-agnostic. Pin sends `ESC v`
      ONLY for `CLIAgent::Claude` on Windows; fork sends it for every agent, in BOTH
      `cli_agent_paste_keystroke_bytes` and `TerminalView::paste`.
- [x] #205 skill path classification uses client home dir, misclassifies remote skills
- [x] #299 SkillReference lacks remote/SSH path support
- [x] #300 Mermaid code block does not defer to code-block rendering while loading/failed
- [x] #313 BlocklistAIInputModel does not take an injected InputModePolicy
- [x] #342 cannot port repository_gated_command_* without simulate_directory_for_completion
- [x] #396 forking a conversation starts the new pane in the wrong working directory
- [x] #403 notebooks/editor: mermaid asset-load relayout tracking missing
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
- [x] #552 search/ai_context_menu: render_search_bar never called
- [x] #554 code/editor_management: CodeManagerEvent::EditCompleted has no subscriber

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
- [x] #440 remote_server: bundled global skills/resources install mechanism.  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
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

**FILED 2026-08-09 — tiered at filing per the rule above:**

**REAL as filed:**
- [x] #284 no `received_rich_notification` latch on `CLIAgentSession`; fork derives  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      rich-status statically per agent type (`listener/mod.rs:36-38`) vs the pin's
      per-event latch (`cli_agent_sessions/mod.rs:153,412,441`). 3 pinned tests.
      **Touches the same struct as tier-2 #545** — adjacent, low risk.
- [x] #343 `BlocklistAIContextModel` has no `try_start_new_conversation` for TUI;  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      fork hard-codes the GUI path and always errors on TUI (`context_model.rs:1184`).
      **BLOCKED on #316** — needs a real `AgentViewConversationSelection` to inject.
- [x] #316 `AgentViewConversationSelection` never ported. Delegation half is real,  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      portable debt (the `AgentViewController` it needs already exists at
      `agent_view/controller.rs:778`). **The `classify_entry` half is entangled with
      the #418 DECLINED decision** — it calls `ActiveAgentViewsModel`, permanently
      deleted here; needs a `BlocklistAIHistoryModel`-based substitute, not a port.
- [x] #256 no persisted prompt-history snapshot / `prompt_history_candidates`  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      (pin `history_model.rs:331-333,2370`). Items 1/3/4 of the original issue are
      superseded by #336/#337/#331; only item 2 remains.
- [x] #431 no lazy metadata-only conversation read + summary backfill. Fork reads  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      eagerly on every startup path (`sqlite.rs:3347`). 4 pinned tests. Real perf
      AND correctness gap.
- [x] #217 CLOSED 2026-08-09 by maintainer decision. Verified REAL first (361 `"Zap"`
      literals on main; every cited example still present), so this is a deliberate
      leave-it, not a false premise. Renaming touches persisted keybinding names and
      settings keys, where a wrong move silently breaks existing users' configs, and
      the `zapctrl` vs `warpctrl` naming decision is still open. If revisited: 19 of
      the 361 are user-facing, the rest internal — that subset is the low-risk cut.
- [x] #254 NARROWED to two items: `Input::unfreeze_agent_input` (pin  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      `input.rs:7625`) and `CommandExecutionSource::SharedSession`'s `preserve_input`
      field. Items b/c are already ported (`input.rs:2037,2064`) via #399.
- [x] #323 NARROWED: `Harness::Codex` now exists (landed under #411), but local  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      Codex launch still returns "not yet implemented" (`local_harness_launch.rs:145-148`),
      and `ANTHROPIC_MODEL` merge, `normalize_orchestrator_agent_name`, and the
      OZ_CLI *prompt-text* augmentation (`local_claude_child_prompt`) are all absent.

**PARTLY REAL — scope narrowed, see each issue's re-scope comment:**
- [x] #147 ONLY `/theme` remains. `/clear`+`/set-tab-color` done; `/rename-conversation`
      is genuinely cloud-coupled; `/reset-statusline`+`/copy-debugging-id` never existed
      at the pin — **that issue cited `warp/master`, the exact ORACLE.md trap.**
- [x] #341 prompt-attachment plumbing DONE (`29049f4f8`); `register_mock_stream_for_test`  **LANDED 2026-08-09** (tier 3 batch, 8,434 tests green).
      exists. Remaining: `schedule_auto_resume_after_error`, `fail_conversation_due_to_shell_exit`,
      `emit_response_event_for_test`.
- [x] #389 voice half DECLINED. Menu half is **ported but NOT WIRED** — `TuiReadOnlyMenuKind`  **LANDED 2026-08-09.**
      has zero call sites. Also: `status_menu.rs` landed at the WRONG PATH (top-level
      instead of nested under `terminal_session_view/`); move it, do not re-port.
- [x] #390 `state.rs` done. Remaining: `completions.rs`, `shortcuts.rs`, the  **LANDED 2026-08-09.**
      attach/detach running-command API, and `terminal_use.rs`'s missing 6th param
      `agent_owns_alt_screen_input`. **`completions.rs` is BLOCKED on #395's
      completion-menu API.**
- [x] #395 footer wording FIXED. Remaining: ask-question multiselect, blocked-action  **LANDED 2026-08-09.**
      presentation, completion-menu API shape. File-edits expand/collapse: API landed
      but the DEFAULT still diverges (fork collapses, pin expands).
- [x] #397 error tone FIXED. Remaining: statusline datetime/footer grouping  **LANDED 2026-08-09.**
      (`format_statusline_*`, `render_statusline_datetime`,
      `TuiUiBuilder::shell_command_accent_style` — all absent).

**SEQUENCING — the warp_tui cluster is NOT parallelisable.** #389/#390/#395/#397
all touch `crates/warp_tui/src/terminal_session_view.rs`; #390 depends on #395's
completion-menu API; #390 and #397 both need `TuiUiBuilder::shell_command_accent_style`.
Work them as one ordered sequence. Likewise #343 is blocked on #316 — one pair.

### Tier 3.5 — LOCAL multi-agent orchestration (reopened 2026-08-08 late)

Reversed from `DECLINED.md` on the maintainer's product call. The original
decline was correct that the code is **non-cloud** (`SCOPE-AI.md` verdict D);
what changed is that we want the feature. Reason it changed: the fork already
ships the substrate — `app/src/pane_group/pane/local_harness_launch.rs` launches
local child agents, and `agent_sdk/driver/harness/mod.rs:191-204` already stamps
`OZ_RUN_ID`/`OZ_PARENT_RUN_ID`/`OZ_CLI`, so parent-child identity is tracked
today. A 2026-08-08 audit also found **unwired, already-tested local scaffolding
sitting dead in the tree**: `children_by_parent`, `ChildAgentStatusCard`, a
de-cloud'ified `local_harness_launch.rs`. We were building the foundations while
declining the feature.

Sizing: ~72 of ~305 orchestration-adjacent pin tests are import-clean of cloud.

**Build order — these have a real dependency chain, do not parallelise:**
- [x] #310 topology + events modules (the non-cloud core, 36 pinned tests) — FIRST
- [x] #376 `AgentConversationData` fields the view reads. **Verify each field
      individually**: the issue's claim that `is_remote_child` is missing is
      FALSE, it is already present.
- [x] #304 the orchestrator/child-agent view **[DONE — issue CLOSED; orchestration_pill_bar.rs + _model.rs + avatar + conversation_links all present with tests]** (pill bar, avatar, conversation
      links, block view-impl, inline controls). Folds in **#410's second half**
      (`cycle_next/previous_orchestration_child_agent` bindings) — #410 was
      closed citing the orchestration decline, and that citation is now stale.
- [x] #325 run-agents child prompt composition **[DONE — issue CLOSED]** — **LOCAL arm only.**
- [x] #329 collapsible defaults **[DONE — issue CLOSED]** — LAST, it configures presentation of the above.
- [x] #309 topology half only. **The credit-rollup half stays declined** — Warp
      credits are a billing concept with no BYOP equivalent.

**Still declined, and this boundary matters:** the cloud-runner half. #290
(RunAgents — children executing on Warp's servers) stays out. Children run as
**local processes on this machine**. `is_remote_child` will be permanently
`false`; the pin defines it as a placeholder for a child on a remote worker.

**Warp never built "spawn an agent on your own SSH host."** That would be
fork-original work, not parity — and Phosphor has better foundations for it than
Warp does, since `remote_server` is a real daemon on the host. Do not confuse it
with `is_remote_child`.

**Persistence needs a NEW forward migration.**
`crates/persistence/migrations/2026-03-23-180000_remove_orchestration_persistence`
deleted orchestration storage deliberately; this is not a revert.

### Tier 3.5 remaining — AGREED SEQUENCE 2026-08-09

**ONE agent at a time. ONE build at a time. Each step lands green and merges before
the next starts.** Coordinator builds and merges; agents never merge.

- [x] **Step 1a** **[DONE — orchestration avatar helpers extracted]** — extract the avatar helpers into a new shared module
      `agent_view/avatar_disc.rs`. Six items, ALL pure rendering with **zero**
      pill-bar state (verified: `render_avatar_disc` has 0 references to telemetry,
      `self`, or `PillBarModel`):
      `render_orchestrator_avatar_disc` (pin pill_bar:127, 11 lines),
      `render_agent_avatar_disc` (:143, 13 lines), `pill_avatar_color` (:109),
      `pill_initial` (:117), `AvatarGlyph` (:196), `render_avatar_disc` (:2125).
      ~60-90 lines total. The pin already exposes them `pub(crate)`, so Step 2's
      pill bar imports them from here instead of defining them.
- [x] **Step 1b** **[DONE — orchestration_avatar.rs present with tests]** — `orchestration_avatar.rs` (41 lines) + `block/view_impl/orchestration.rs`
      (656). The latter uses `OrchestrationAvatar` 7x, so these go together.
      `CollapsibleExpansionState` already exists generically in `block.rs` — not
      gated on #329.
- [x] **Step 1c** **[DONE — orchestration_conversation_links.rs present]** — `orchestration_conversation_links.rs` (299). **Independent of
      1a/1b** — uses `OrchestrationAvatar` 0 times. Needs
      `TerminalAction::OpenChildAgentInNewPane` (0 in fork; note
      `RevealChildAgent` already exists and is wired, so #410's second half is
      partly done) and `AgentConversationsModel::resolve_open_action` /
      `AgentConversationNavigationSubject` (0 in fork).

      **CORRECTION 2026-08-09:** an earlier version of this plan said "the avatar
      cannot land alone" and had Step 1 reach into Step 2's 2,539-line file. That
      was wrong — the six helpers are self-contained, so 1a makes the split clean
      and no structural deviation from the pin is needed.
- [x] **Step 2** **[DONE — orchestration_pill_bar.rs present with tests]** — `orchestration_pill_bar.rs` (2,539). Port the
      `blocklist::telemetry` module FIRST (`BlocklistOrchestrationTelemetryEvent`:
      6 pin files, **0 in fork**), then the pill bar, then the new variants on
      `PaneHeaderAction`/`MenuEvent`/`WorkspaceAction`/`TerminalAction`. Own session.
- [x] **Step 3** **[DONE — #325 CLOSED]** — #325. Add `AIAgentActionType::RunAgents` (16 pin sites) and let the
      compiler walk the **59 files** matching that enum. Also needs
      `StartAgentExecutionMode`/`RunAgentsExecutionMode`/`RunAgentsAgentRunConfig`
      (all 0 in fork). LOCAL arm only. One deliberate compiler-checked pass.
- [x] **Step 4** **[DONE — #329 CLOSED]** — #329, collapsible defaults in `block.rs`. Small, and genuinely last:
      it configures presentation of steps 1-2.
- [x] **NOT IN THIS TIER** — `inline_action/orchestration_controls.rs` (~1,336) is
      **cloud**: `orchestration_controls.rs:48` imports `crate::server::experiments`.
      `DECLINED.md` covers it under the RunAgents entry; its one non-cloud caveat is
      **#11's** scope. Do not port it here.

**Deviating from this order requires asking first.** Recorded because the coordinator
changed an agreed order twice on 2026-08-09 (#381 folded into the #440 batch against
"after 440"; #405 re-tiered unasked) and both were wrong.

### Landed 2026-08-09 — untracked features (no issue filed, maintainer directed)

These shipped to `main` on 2026-08-09 without GitHub issues, by explicit maintainer
decision. Recorded here so the next port sweep finds a decision rather than
apparent debt.

- [x] **Remote file-viewer routing.** Every file opened from the remote (SSH) file
      tree used to land in the code editor: the remote branch asked one question
      (`is_supported_image_file`) and its own comment said "everything else opens via
      the buffer-sync protocol". `FileTreeEvent::OpenRemoteFile` carried no
      `FileTarget`, so no viewer choice could be expressed. **Remote markdown never
      rendered.** Fixed by threading `target` through `OpenRemoteFile` ->
      `LeftPanelEvent` -> `Workspace`, adding `SourceFile::Remote`,
      `FileNotebookView::open_remote` (over the existing `ReadFileContextRequest` RPC)
      and `RemoteServerManager::host_request_handle`. Root cause of the class:
      the pin unified local/remote behind one `LocalOrRemotePath`; this fork split
      them into two events over two `RemotePath` families.
- [x] **Remote notebook Raw-mode toggle.** `open_as_code`/`ToggleMarkdownDisplayMode(Raw)`
      were gated on `local_path()`, always `None` for remote, so Raw was a silent
      no-op. `PaneEvent::ReplaceWithCodePane.path` widened `PathBuf` ->
      `BufferLocation`. Only 4 reference sites across 3 files. Deliberately used the
      fork-native `BufferLocation` rather than renaming to the pin's
      `LocalOrRemotePath`; `ReplaceWithFilePane` left as `PathBuf` (its callers are
      local-only by design — remote panes toggle rendered/raw inline in `CodeView`).
- [x] **TUI orchestration tab bar.** The pin's TUI imported `crate::orchestration_tab_bar`,
      absent fork-wide, blocking 9 tests. Ported the module plus a **local-only**
      `TuiOrchestrationModel` fed by `orchestration_topology.rs`, dropping the pin's
      `StartAgentExecutionMode::Remote` branch (cloud-runner, declined under #290).
      `crate::tab_bar` turned out to be already ported byte-identical — the generic
      tab machinery was never the gap. All 9 tests ported unweakened.
- [x] **Per-host skills and global rules reaching agent context.** This fork had built
      per-host storage TWICE (`BundledSkills::remote_by_host` under #487/#353,
      `remote_global_rules` under #575) and wired consumption NEITHER time.
      `SkillManager` was already remote-aware and tested; the bug was call sites
      hardcoding `LocalOrRemotePath::Local(...)` regardless of session type — four of
      them. Upstream cause: `ActiveSession::current_working_directory_location`
      carried a doc comment claiming "BYOP sessions are local, so this is always a
      Local path", false since this fork tracks `SessionType::WarpifiedRemote`.
      A wrong comment propagated a wrong assumption into every consumer.
- [x] **`format_todo_progress` + statusline wiring.** Previously declined as
      "not small" because the bare function would be dead without the
      `TuiStatuslineItem`/`FooterSegment` plumbing. Ported whole. No settings
      migration needed — new items append disabled via `TuiStatuslineConfig::normalized()`,
      same as #397's Date/Time variants. **Also fixed two tests #397 left stale on
      `main`**: `ai_tests.rs` and `statusline_config_view_tests.rs` hardcoded a 7-item
      `TuiStatuslineItem::ALL` order against what is now 12 entries.

### Tier 4 — large (a week+)
- [x] #576 (replaces **#210**, closed 2026-08-09) · #382 · #236 · #324 · #405  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
- [x] **#349 PARKED 2026-08-09 (maintainer).** `computer_use` per-window activation
      (`mac/activation.rs`, `mac/window.rs`, `mac/post.rs`, `linux/x11/seat.rs`,
      `linux/x11/windows.rs`). Parked, not declined: the macOS half cannot be built or
      verified on this host, so porting it would ship code no one here can test.
      Revisit only if a macOS build host becomes available.
- [x] #575 `RemoteAgentContextSnapshot.global_rules` is always empty. Split out of the  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      #440 batch after a scope correction: I had assumed `remote_context_files.rs`
      supplied it — **it does not.** `global_rules` arrives pre-serialized in the
      snapshot from daemon-side `ProjectContextModel::global_rules()`;
      `remote_context_files.rs`'s real consumers (`metadata_project_rules.rs`,
      `skill_watcher.rs::read_project_skill_contents`) are unrelated and absent here.
      **Real scope:** daemon `ProjectContextModel::global_rules()` + client
      `set_remote_global_rules`/`remove_remote_global_rules`/`remote_global_rules`
      storage. **Blocker:** this fork's `ProjectContextModel`
      (`crates/ai/src/project_context/model.rs`) is a flat local-only `path_to_rules`
      map with no per-host scaffolding — comparable in size to the per-host skills
      work that landed under #487/#353, not a wiring change. **MOVED TO TIER 4 2026-08-09 by maintainer** — sized like the #487/#353
      per-host skills work, not like the rest of tier 3, and it was the only item
      holding tier 3 open. Verified before the move: `project_context/` has **zero**
      `HostId` references (model.rs 0, mod.rs 0, model_tests.rs 0), no `global_rules()`
      accessor exists at all, and `app/src/remote_server/server_model.rs:262` hardcodes
      `global_rules: Vec::new()`. The protocol half is already correct —
      `protocol_tests.rs` round-trips the field.
- [x] #312 NLD prompt-history match — **moved here from the maintainer-decision bucket  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      2026-08-09; it was never a decision, it is ordinary local work.** Warp's
      natural-language detection consults TWO history sources (shell command history +
      agent prompt history) and breaks ties by recency, so retyping a previous agent
      prompt locks the input to AI mode and retyping a previous shell command locks it
      to Shell. The fork consults command history only, so a previously-sent prompt is
      re-classified from scratch every time. Entirely local (both sources on disk),
      9 pinned tests blocked.
      **The issue's claim that none of the symbols exist is WRONG** — 4 of 5 are partly
      present: `HistoryMatch` 2 fork/6 pin, `InputTypeAutoDetectionSource` 5/16,
      `NldPromptHistoryMatch` 2/5, `prompt_history_candidates` 2/3. The genuinely
      absent one is **`resolve_history_match` (0 fork / 2 pin)** — the tie-break itself.
      **SEQUENCING: blocked on #256** (tier 3, in flight) — `prompt_history_candidates`
      is its prompt-side source. Once #256 lands this is `resolve_history_match` plus
      porting 9 tests.
      (#252, #289, #142 CLOSED 2026-08-08/09)
- [x] #381 — **work DONE, issue still open.** Its two real modules  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      (`local_harness_setup.rs` 5 tests, `remote_context_files.rs` 4 tests) are
      committed on `working` awaiting the tier-3 merge; the other four modules it
      named are either done (`remote_agent_context.rs`), moved to tier 3.5
      (`orchestration/`), or declined (`agent_management/`,
      `active_agent_views_model.rs`). **Close it when the tier-3 batch merges.**

**#210 was re-filed as #576 after re-measuring all ten rows against `main`.** Its
figures were wrong in BOTH directions: pin counts undercounted 2-4x on 6 of 10 rows
(`input_tests.rs` 149 not 54, `view_tests.rs` 142 not 37); three rows listed as
absent actually exist under fork-renamed paths at 78-94% ported (`input_test.rs`,
`view_test.rs`, `local_model_test.rs`) — the exact filename-not-content error #210's
own rules warned against; two rows were already closed (#142, #252); and
`pane_group/mod_tests.rs` is majority-cloud (21 marker lines: `CodebaseIndexManager`,
`IapManager`, `CloudConversationData`), not clean debt.
**~521 claimed -> ~214 genuinely portable non-cloud tests.**
- [x] #405 Jupyter (`.ipynb`) rendering. **STAYS IN TIER 4** (maintainer, 2026-08-09).  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
      Scoped 2026-08-09: verdict REAL, zero cloud,
      but **~3-4x smaller than the tier-4 framing**: ~500-700 net-new lines across
      ~12 files, ~30 tests, 1-3 days. The only genuinely new code is
      `crates/ipynb_parser` (401 lines + 24 tests, self-contained nbformat-v4 JSON ->
      `FormattedText`); everything else is 2-13 line hooks into files that already
      exist, because ~90% of the scaffolding is already here (the whole
      `app/src/notebooks/` subsystem, the `FeatureFlag` mechanism, `ContentFormat`,
      `markdown_parser`, and `is_jupyter_notebook_file()` with its 6 tests already
      passing). No blocking dependencies. See the issue for the 5-step order.

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
- [x] #149 · #150 · #203 (design decision) · #206 · #207 · #279 · #312  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**

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

**Completed and superseded sections live in [`docs/TODO-ARCHIVE.md`](docs/TODO-ARCHIVE.md)**
(moved 2026-08-10). The selection rule was mechanical and asserted at move time: a section
went to the archive only if it contained **zero** open checkboxes. This file had grown to
~2,950 lines of which 41% was history, and a duplicated item can be ticked in one place
while still reading as open in another — which is exactly how several status reports came
out wrong. Look in the archive for *why* something was decided; look here for what is left.

**Before acting on any entry in this file, verify it against the code.** Seven entries have
now been found stating the opposite of the tree — see the `#12`, AI-global-skills and
Phosphor Drive entries for worked examples of the correction, and note that two of those
were themselves corrections of earlier corrections.

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
| CDPATH-aware `cd` completion | #483 (**closed as done, but the chain is UNWIRED — see "CDPATH completion is dead code" below**) |
| Launch-at-login | #484 (closed — done, see "Requires macOS/Windows" below) |
| NLD heuristic feature flags | #485 (closed — done) |
| AI bundled + global skills | #487 (**CLOSED 2026-08-10** — resolved as delete, not a gap; see correction below) |

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

- [x] **WSLENV passthrough vars** *(Windows)* — **STALE: this claimed absent, but it
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Ported: `app/src/terminal/local_tty/windows/environment.rs:201` defines `wsl_env_allowlist`, called at `:159`; commit `17ee390a2` is an ancestor of `main`. The remaining caveat (never run against real WSL) is genuine and is covered by the Windows-verification entries.
  is DONE.** `wsl_env_allowlist` exists at
  `app/src/terminal/local_tty/windows/environment.rs:202` (commit `17ee390a2`, PR #119,
  targeted issue #117). Compile-only port, per the commit's own note — still not
  runtime-verified on an actual WSL/Windows host, which is the real remaining item.
- [x] **Launch-at-login** *(macOS + Windows)* — **STALE: this claimed absent, but it
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Ported: `app/src/login_item/` contains `mod.rs`, `macos.rs`, `windows.rs` and both test modules; same commit `17ee390a2`. Same runtime-verification caveat as above.
  is DONE.** `app/src/login_item/` exists (`mod.rs`, `macos.rs`, `windows.rs`,
  `windows_tests.rs`; commit `17ee390a2`, PR #119, targeted issue #118). Same caveat:
  compile-only on this Linux host, not runtime-verified on macOS/Windows.
- **Edition-2024 release verification** *(macOS)* — index entry only; **tracked as the
  canonical item "Edition-2024 cross-platform build — macOS release verification only"
  further down this file.** Not a second checkbox: this pair was counted twice for days.
- **pwsh `-EncodedCommand` at 2 call sites** *(Windows)* — index entry only; **tracked as
  the canonical item "NEEDS WINDOWS VERIFICATION: pwsh `-EncodedCommand` at 2 more call
  sites" further down this file.** Not a second checkbox.

### STALE-WRONG — corrected 2026-08-07
- [x] **AI global skills** — **this entry previously said "WON'T DO (maintainer,
      **RECONCILED 2026-08-17 (v0.1.0 agent audit)** — Self-resolved 2026-08-10 and never ticked: `app/src/ai/skills/mod.rs:143-146` records the removal, no `global_skills*` file exists, and `DECLINED.md:87` carries the #487 decision.
  2026-08-06)" and stated the opposite of the actual decision.** #11's 2026-08-07
  closing comment quotes the ledger's own "Maintainer BYOP decisions — 2026-08-02"
  section, settled before the WON'T-DO note was ever written: *"AI skills: build
  `bundled` + `global` (local); DROP the `remote` daemon-sync / cloud-repo arm."*
  ~~Verified against `origin/main` today: `app/src/ai/skills/` is missing exactly
  `bundled.rs`, `bundled_tests.rs`, `global_skills.rs`, `global_skills_tests.rs`
  (plus `remote.rs`/`remote_tests.rs`, which stay dropped per the decision above).~~
  **STALE-WRONG A SECOND TIME — re-verified against the working tree 2026-08-10.**
  All four of those files are PRESENT, and so are `remote.rs`/`remote_tests.rs`.
  `bundled.rs` is fully wired (`skill_manager.rs:33-36`), and home/global skill
  directories are loaded through `SkillManager`'s own `directory_skills` /
  `home_directory_for_origin` path. The file-presence claim above was simply
  false; do not act on it without re-checking `ls app/src/ai/skills/`.

  What is *actually* left is narrow: `global_skills::filter_skills_by_spec` has
  **no production callers** — it is exported from `mod.rs:132` and covered by
  `global_skills_tests.rs`, but nothing outside the module calls it. So it is
  either dead code to delete or an unwired helper to connect. Resolve that one
  question rather than re-porting a subsystem that is already here.

  **RESOLVED 2026-08-10 — DELETE, and #487 is now closed.** Traced the pin's
  only call site: `AgentDriver::load_global_skills`
  (`app/src/ai/agent_sdk/driver.rs:2073` at the pin) is fed by
  `resolve_global_skills` (`:1885`), which is gated on
  `FeatureFlag::OzPlatformSkills` and reads
  `AuthStateProvider::as_ref(ctx).get().global_skills()` — a Warp
  Team/workspace-policy value delivered over the cloud auth channel, the same
  shape as the already-declined `UserWorkspaces::current_team()` class
  (`DECLINED.md`, #445). `resolve_skill_repos`, the other half of the pin's
  `global_skills.rs`, was already cut for exactly this reason during the #493
  port (`GithubRepo` comes from the deleted `cloud_object_models`). This
  fork's `driver.rs` never grew `load_global_skills`/`resolve_global_skills`
  at all — confirmed by grep, and by the fact that `crate::ai::cloud_environments`
  (the pin's `GithubRepo` source) has no `mod` declaration anywhere in this
  tree. `RunAgentArgs.skill` (`crates/warp_cli/src/agent.rs:323`) is a single
  `Option<SkillSpec>`, not a `Vec`, so there is no local, non-cloud concept of
  "multiple global skill specs to filter by" for a new caller to feed —
  wiring one would be new feature work, not a parity port. The per-user
  "global skills outside a workspace" reading of #487's own title is already
  covered by a different mechanism: `SkillManager`'s `directory_skills` /
  `home_directory_for_origin` and `resolve_skill_spec.rs`'s
  `resolve_unqualified` resolve home/global skill directories directly,
  without going through `SkillSpec`-list filtering at all. Two mechanisms
  covering the same ground would be worse than one, so `filter_skills_by_spec`
  is redundant even setting the cloud question aside. **Deleted**
  `global_skills.rs` and `global_skills_tests.rs`, dropped the `mod.rs`
  export; recorded as "AI skills — global-spec filtering" in `DECLINED.md`
  so the next audit does not re-file it as a gap.

  Separately, confirmed `remote.rs`/`remote_tests.rs` being present is
  correct and not a leftover: the 2026-08-08 late "SSH half of
  `ai/skills/remote.rs` is UN-DROPPED" decision earlier in this file already
  established that `bundled_skill_snapshot_protos` is Phosphor's own SSH
  daemon (`remote_server::proto`), not Warp's cloud sync, and is a hard
  dependency of #353. Nothing further to do there.

### Not started — true gaps
- [x] **Skill remote-path** — now **#205**. Promoted out of this ledger after finding a
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
- [x] **`local_control` / `warpctrl` app-side** — **#200 is now CLOSED**, as a subset
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
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17.** The remaining item was a local
      macOS `script/run --release`; the maintainer has been running the app, so this
      is discharged. Original entry:
      **Edition-2024 cross-platform build — macOS release verification only.** The code work
  is DONE and on `main`: the mac/wasm/windows `unsafe`-syntax fixes from branch
  `fix/edition-2024-native-targets` are merged (commit `48bc21cb9`, via PR #53) — verified
  2026-08-06 with `git merge-base --is-ancestor 48bc21cb9 main`, and the remote branch has
  been deleted. **All that remains is a local macOS `script/run --release` run**, which
  cannot be done on this Linux host — it needs a Mac (no CI-discovery builds). That run may
  still surface further latent mac-only errors.
- [x] **CLOSED — MAINTAINER DECISION 2026-08-17: the entry itself says STALE — the deadlock it describes no longer exists.** Original entry:
      **#4 warp_tui suite** — **STALE, corrected 2026-08-07.** The deadlock this
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Framing is stale. `gh` reports **#4, #456 and all six named siblings (#384/#387/#389/#390/#392/#395) CLOSED**, and `docs/SWEEP-SUMMARY.md:273-277` already supersedes the "trailing the pin by a generation" reading: "#456 is not 'the crate is behind'; it is two specific unfinished features", both marked resolved at TODO.md:1477,1527. CI gating is real (`pr-check.yml:279-351`). Narrow to the surviving #399/#254 item-d divergence (`InputTypeAutoDetectionSource::AgentTerminalControl`) and decide whether this box should now be ticked.
  entry describes (`tui_generic_tool_call_view::…_completes_the_executor`) is FIXED
  — see Part 2 below, PR #124, commit `87d06d179` — do not re-investigate it. #4
  itself stays open, but its scope moved: CI now gates the `warp_tui` crate at all
  (it previously didn't — issue #465 covered that gap; PR #469 addressed it), and the remaining gap
  is understood as `warp_tui` trailing the pin by a generation, tracked with a full
  root-cause map at **#456**, with #384/#387/#389/#390/#392/#395 as siblings tracing
  to the same cause. Treat the old 18-failure nextest breakdown above as historical
  context for how this was first noticed, not as the current state.
- [x] **#2 sweep** — the 2 missing GUI auto-resume oracle tests
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Fully done: both auto-resume tests are real (not stubs) at `terminal/view_test.rs:4118-4187`; `docs/SWEEP-INVENTORY.md:24` matches the 3,902→2,357 count with 269 file sections; `handle_interrupt` (`terminal_session_view.rs:3155-3177`) carries the fix with two non-ignored regression tests at `terminal_session_view_tests.rs:373-416`.
  (`completed_user_controlled_lrc_{resumes_when_not_suppressed,skips_resume_when_suppressed}`)
  are now PORTED to `terminal/view_test.rs` (2/0; the resumes case needed a
  `GlobalResourceHandlesProvider` mock for the subagent-sidecar persist path; the fork's
  teardown method is `set_user_control_with_stop_reason`, Warp's is `set_user_control_for_teardown`).
  Broader sweep: **inventoried 2026-08-10, see `docs/SWEEP-INVENTORY.md`** — the
  mechanical diff against the pin, per file, with the specific absent test names and a
  bucket for each. The real number is **2,357 absent** (down from `ORACLE.md`'s 3,902 on
  08-06; the fork went 7,884 → 9,716 tests in between), across 269 files, **not 379
  modules**. 20 ported in that pass (task-store subtask pruning ×4, `agent run` flag
  parsing ×4, project-skill tree discovery ×3, remote skill resolution ×5, PR-info proto
  round trip, TUI shortcuts-sheet toggle/insert/up ×3), plus **one real code defect
  fixed**: the TUI's `handle_interrupt` did not close an open `?` shortcuts sheet or
  `/status` menu, so ctrl-c left it painted over the session while the interrupt worked
  underneath. The pin closes it first; this fork's copy was missing that block. Found by
  tracing `terminal_use_interrupt_closes_shortcuts_before_taking_control`, which reads
  like test debt and is not — it would have compiled and gone red. Fixed in
  `crates/warp_tui/src/terminal_session_view.rs`, pinned by two new tests.
  **Three things the inventory establishes, worth not re-deriving:**
  (a) name-diffing over-reports by roughly a quarter — 566 of the 2,357 absent names
  already appear verbatim in fork prose that adjudicates them, and three separate
  mechanisms produce false "missing" verdicts (renamed-with-the-code, replaced-by-a-
  documented-analogue, same-basename-different-module); (b) `SCOPE-*.md` verdict A is
  overstated in a second way nobody had written down — it only asks whether the fork
  ships a file of that name, not whether it is the same module, whether the API under
  test still exists, or whether the fork deliberately inverted the behaviour;
  (c) four **feature** gaps fell out of it, all non-cloud and user-visible — MCP tool
  results render as a JSON text blob rather than a tree, there is no `/index` slash
  command so indexing is auto-only, TUI selection cannot trim trailing whitespace or
  select a styled word, and `languages::language_by_filename` has no `StandardizedPath`
  overload. All four are written up in the inventory. Cheapest next ports, already
  traced: the two `apply_shell_completion` UTF-8 span tests (that function has zero
  coverage today), `move_left_from_shortcuts_replaces_it_with_conversation_menu`, and
  re-adding the three attach-hint assertions dropped from
  `visible_startup_script_shows_no_interrupt_hint`.
  (Anchor Stop/auto-resume regression already code-fixed.)
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
- [x] **get_relevant_files live smoke** — now **#206**. Unit + lib green (4 tests in  **[STALE — issue(s) CLOSED on GitHub; reconciled 2026-08-10]**
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

# Part 2 — Code-review debt

Actionable items from the code reviews run on 2026-07-26 (and later). Grouped by review.
Each item notes `file:line`, the problem, and the suggested fix.

Consolidated here from the former lowercase `todo.md` on 2026-08-06. Every item was carried
over. Items re-verified against `main` during the consolidation and found already landed were
flipped to `- [x]` with the evidence inline — none were deleted. Note that several `file:line`
references below predate later refactors (e.g. `app/src/settings_view/about_page.rs` is now the
`about_page/` module); the original paths are kept as written so the findings stay traceable.

## Follow-up code-review fixes (2026-07-29, commit `fddc193a`)

Dev machine is Linux; nothing below has been run against a real Windows
`pwsh.exe`. Verified only via `cargo check`/`cargo test` (static + unit-level).

- [x] **CODE COMPLETE; VERIFICATION DEFERRED 2026-08-18 — needs Windows hardware, not work.**
      ⚠️ **This is verification debt, not a code gap — do not read the tick as "tested".**
      The port itself is done at both call sites and the encoding is unit-tested
      (`encode_pwsh_command_round_trips_without_trailing_nul`). What cannot be done on this
      machine is the only thing left: spawning a real `pwsh.exe` 7.6 to confirm it accepts the
      argv. Carried forward in the "verify on Windows" list below. Original entry:
      **NEEDS WINDOWS VERIFICATION: pwsh `-EncodedCommand` at 2 more call sites**
      **REFUTATION AUDIT 2026-08-17 (v0.1.0, oracle-backed)** — Line reference wrong: `local_command_executor.rs:55` is an unrelated struct field; the real call site is `:179-183`. Substance confirmed — `util/mod.rs:68-75` does UTF-16LE+base64 only, and `:115-129`'s test round-trips the encoding without ever spawning `pwsh.exe`, so the needs-Windows-verification framing is right.
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

## Refutation round 2026-08-21 — findings from a 24-agent adversarial fleet

Each agent was briefed to REFUTE a subsystem, default to "no defect", cite
file:line, and never build. **These are agent findings, independently verified
only where noted.** Verify before acting — that rule applies to this section
more than any other in this file.

> **VERIFICATION IN PROGRESS (started 2026-08-21).** All 116 findings were
> numbered and split across 16 independent verifier agents, each briefed to
> CONFIRM / REFUTE / PARTIAL every claim in its batch, defaulting to REFUTED
> where the evidence does not compel, and told explicitly that *confirming a
> cited `file:line` exists is not verification — the claimed consequence must be
> checked*. That rule exists because one finding here (the `dead_code` entry)
> cited a real attribute at a real line, compared it correctly against the pin,
> and drew a conclusion that was false; it is corrected in place below.
> Verdicts are being written back against each item as they land. **Until an
> item carries a verdict, treat it as an unverified lead.**

Ordered by severity, not by area.

### Security / permission

- [x] 🔴 **`use_computer` executes with NO approval check — found independently by
      TWO agents.** `app/src/ai/blocklist/action_model/execute/use_computer.rs:17-31`
      returns `true` unconditionally, justified by a pin-inherited comment claiming
      the action "is only executed by the computer use subagent, which cannot begin
      without the user approving it via a `RequestComputerUse` action". **That premise
      is false in this fork.** The pin's server chose the tool set; BYOP builds the
      array client-side and advertises `USE_COMPUTER` alongside `REQUEST_COMPUTER_USE`
      (`agent_providers/tools/mod.rs:144-145`), gated only on `computer_use_enabled`,
      which is true for `ComputerUsePermission::AlwaysAsk`. No approval state is
      recorded or consulted anywhere. A model calling `use_computer` first drives the
      real mouse and keyboard, irreversibly, with no prompt, on a profile whose
      description says "require explicit approval". Reachable by prompt injection via
      `webfetch`. Not in DECLINED.md.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Chain traced end to end: `execution_profiles/mod.rs:122` `is_enabled()` is true for `AlwaysAsk`, so `agent/api.rs:484` sets `computer_use_enabled`; the only dispatch re-check (`chat_stream.rs:6285`) tests that same flag; `execute.rs:531,541` turns `can_auto_execute=true` into `needs_confirmation=false`. No approval state exists anywhere — grep finds only `TelemetryEvent::ComputerUseApproved`. DECLINED.md:212 explicitly excludes this path, so it is not a recorded decision.
      **FIXED 2026-08-21:** `AIConversation::has_approved_computer_use()` (`conversation.rs:1667`) derives approval by scanning exchanges for an `AIAgentInput::ActionResult` carrying `RequestComputerUseResult::Approved` — a derived fact, not a stored flag, so it cannot be set without the approval actually having happened. `use_computer.rs:19-65` now returns that instead of `true`; no approval means the normal confirmation path. **Not done:** the profile's own always-allow is not re-checked, because this executor is built without a `terminal_view_id` and cannot resolve the right view's profile.

      **REFUTATION 2026-08-21 — FIX DEFEATED, REOPENED.** The gate is computed and then discarded: `action_model.rs:1062-1078` → `execute_action` → `start_pending_action_by_id(..., true, ...)` (`:739`) forges `is_user_initiated=true`, and `execute.rs:543-548` makes `needs_confirmation` false regardless of the approval check. Second, independent hole: approval is rebuilt from disk (`convert_conversation.rs:1306-1330` via `into_exchanges` `:224,373`), so an approval granted in a previous session unlocks the conversation permanently. Repair dispatched.

      **REPAIRED 2026-08-21 (both refutation grounds upheld; the earlier FIXED note overstated what landed and was wrong).** Ground 1: the `is_user_initiated: bool` is now an `ActionInitiator` enum (`execute.rs:225-286`) with `Agent`/`User`/`AutoAcceptedTagIn`. `is_out_of_band()` reproduces the old bool exactly for the serialisation guard, so no concurrency behaviour changed; `can_stand_in_for_confirmation(&action)` grants stand-in authority to `User` always, `Agent` never, and `AutoAcceptedTagIn` for everything **except `UseComputer` and `RequestComputerUse`**. The auto-accept loop calls a new `auto_accept_action` (`action_model.rs:775,1137`) instead of `execute_action`. **The pin has no such caller at all** — `git grep "auto_accept\|queue_actions_with_options" 42effe840 -- app/src/ai/blocklist` is empty and the pin's only two `true` producers (`42effe840:action_model.rs:767,782`) are genuine clicks — so the forged `true` was fork-original and illegitimate. **Hole the refuter did not name:** `RequestComputerUse` was itself auto-acceptable on that path, a one-hop laundering route that *manufactures* the `Approved` record the gate reads; excluded too. All 8 `execute_action` callers traced and all are genuine clicks, so nothing outside the LRC path changed. Ground 2: approval is now session-scoped — `restored_computer_use_approval_ids` (`conversation.rs:271`) is populated once in the restore constructor before the conversation is reachable, and `has_approved_computer_use` (`:1771`) requires an `Approved` whose action id is **not** in that set. Chosen over hooking an append site because approvals can arrive via three different paths, so any single hook would be a gate with a hole; keeping it derived also preserves the invalidation event (a rewind drops the exchange and revokes the approval, where a stored bool would survive). **Remaining debt, deliberately not shipped:** approval is scoped to the session, not to the sub-task as the pin's wording implies; narrowing to `task_id` needs a live trace of BYOP task-id assignment that cannot be run under the closed build gate. Path correction: the file is `app/src/ai/agent/api/convert_conversation.rs`, not `app/src/ai/agent/convert_conversation.rs`.

- [x] 🔴 **`/plan` mode is advisory, not enforced.** `PLAN_MODE_BLOCKED_TOOLS` filters
      only the advertised array (`chat_stream.rs:3831,3948`); `parse_incoming_tool_call`
      resolves names straight from the full REGISTRY with no `advertised` check
      (`chat_stream.rs:7171`). The comment at `:3711-3714` claims an unlisted tool
      "simply can't be called" — contradicted by this same file's dispatch-time
      re-checks for web (`:6929`) and computer use (`:6285`). A model emitting
      `run_shell_command` by name during `/plan` executes normally.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `chat_stream.rs:7171` `tools::lookup(&call.fn_name)` searches all of REGISTRY; `advertised` is consulted only inside `recover_tool_by_arg_shape` (`:7120-7124`). Verifier additionally checked for any downstream plan gate: `UserQueryMode::Plan` appears only at `chat_stream.rs:3704` and `blocklist/controller.rs:763` (prefix stripping), nothing in the executor path.
      **FIXED 2026-08-21:** enforced at DISPATCH (`chat_stream.rs:6335-6389`), immediately before `parse_incoming_tool_call`, matching the shape the web and computer-use re-checks already use in the same file. The two false comments claiming an unlisted tool "simply can't be called" are corrected at `:3713` and `:3949`.

      **REFUTATION 2026-08-21 — FIX DEFEATED, REOPENED.** `plan_mode` is derived from a `UserQuery{Plan}` in `params.input` (`chat_stream.rs:5028`, `:3699`), but the post-tool follow-up is built by `RequestInput::for_actions_results` (`controller.rs:306-328,1646`), which carries only `ActionResult`s and is copied verbatim at `api.rs:530`. **From round-trip 2 onward `plan_mode` is false**: the gate is off, `build_tools_array` (`:3949`) re-advertises `run_shell_command`, and `prompt_renderer.rs:718` drops the plan block. Trigger: `/plan` followed by any `read_files`. Repair dispatched.

      **REPAIRED 2026-08-21 (refutation upheld in full).** Plan mode is now **turn-scoped state resolved once per request** rather than sniffed from the payload: `RequestParams::plan_mode` (`api.rs:144-157`, set in `new`/`new_for_test`) via `resolve_plan_mode` (`:292`) — if the payload carries the user's own query that query decides (which is also the **exit** event, since a query without `/plan` resolves `false`), otherwise it inherits from the conversation. `plan_mode_from_request_inputs` (`:325`) deliberately returns `Option<bool>` so "payload says nothing" is **not** fused with "payload says off" — that fusion is exactly the `is_none_or` pattern that caused this bug. `inherited_plan_mode` (`:349`) walks `all_exchanges()` newest-first; multi-task payloads resolve toward read-only. `chat_stream.rs:3734` `request_plan_mode` replaces the derivation at all four sites (`:1856`, `:3868`, `:3955`, `:5121`), and the OR can only turn plan mode *on*, so it cannot defeat the exit path. All three consequences are fixed together — dispatch gate, `build_tools_array` advertising, and the `prompt_renderer` plan block — via a single `plan_mode_blocks_tool` predicate (`:3801`) replacing three copies, which also gives the dispatch gate a condition a unit test can reach. **The old doc comment on `is_plan_mode_turn` described the defect as intentional** ("plan state isn't automatically sticky across turns"). **Root cause of why this survived: the original fix shipped with zero tests** — no test in the tree referenced `plan_mode` outside two renderer tests. Now 13 tests across `api.rs:673` and `chat_stream.rs:11471`, the latter building the real follow-up shape and asserting all three consequences together, plus a non-vacuity control. **The pin offers no pattern:** `git grep plan_mode 42effe840 -- app/src` shows Warp only *serializes* `UserQueryMode` to the server and enforces server-side, so the fork's local gate has no oracle counterpart. **Deliberately not done:** stamping `mode` into `make_user_query_message` — it matters only for shared-session viewers, and stamping the root copy while leaving the LRC subtask copy unstamped would let the unstamped copy shadow the real one in the newest-first walk; stamping neither is a no-op, partial stamping would be a new bug.

- [x] 🔴 **`mcp_read_resource` fails open on an unknown server** (#615's shape).
      `mcp.rs:238-248` — an unmatched `server` falls to `unwrap_or_default()`, giving an
      empty `server_id`; `permissions.rs:848-858` then treats it as not-denylisted, so
      under AlwaysAllow it executes against a server the user may have denylisted.
      **VERDICT REFUTED (independent verifier, 2026-08-21):** The chain breaks before `permissions.rs:848`. At `:819-826`, when `uuid_of_mcp_server.is_none()` — which an empty or unparseable `server_id` always yields — it falls back to `server_from_resource(name, uri)`, resolving by URI (`templatable_manager.rs:330-343`) and denylisting THAT server. `read_mcp_resource.rs:92` ignores `server_id` and resolves the same URI, so the two agree. Not a fail-open.

- [x] 🔴 **Whole conversation JSON written to the log on every turn.**
      `chat_stream.rs:5085` `log::info!("[byop-diag] full_request_json={...}")`,
      unconditional, at the default level, containing system prompt, full history, file
      contents, shell output, cwd/git env. Repeated at `:5489`. `warp_logging` exposes
      `write_log_bundle_zip_to`, so this ships in bug reports. Only base64 binaries are
      redacted (`:5078`).
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `chat_stream.rs:5085` is unconditional and only binaries are redacted (`:2753-2768`). Verifier traced the sink past the original: default filter is `LevelFilter::Info` (`warp_logging/native.rs:732`), there is no `release_max_level` feature (`Cargo.toml:200`), file logging is the default off-tty, no scrubbing in the writer, and `write_log_bundle_zip_to:673` zips warp.log. (`:5489` is error-path only.)

      **FIXED 2026-08-21 (first pass, INSUFFICIENT — see below):** the two `full_request_json` dumps were put behind `ZAP_BYOP_LOG_FULL_REQUEST` (`chat_stream.rs:2748-2773`, gated at `:5164-5176` and `:5586-5596`), with the default path claimed to log a "non-content summary".

      **REFUTED THEN REPAIRED 2026-08-21.** The gate worked; the claim about the default path did not. `log_chat_request_details` was still called **unconditionally** at `Info`, and `warp_logging`'s native base filter *is* `Info` (`warp_logging/src/native.rs:732`), so **every turn still shipped roughly 10 KB of verbatim conversation content** into `warp.log` — 240-char slices of the system prompt, the first text of every message, every tool call's arguments, every tool response (i.e. the head of every file read and every shell result), the full `tool_names` list leaking configured MCP server names, and the persistence `query=`/`content=` snippets. `warp.log` goes into `write_log_bundle_zip_to`, which is the whole reason this entry exists. Now every free-text field routes through a single tier boundary, `snippet_or_shape_for_log` (`chat_stream.rs:2728-2841`); the default tier emits a **shape descriptor** — bytes, chars, backslash/raw-control/non-ASCII counts, plus an FNV-1a digest hand-rolled so its meaning cannot shift on a toolchain bump and pinned by test — which is exactly what the illegal-escape hunt the logging exists for actually asks about. **`tool_names` decision:** MCP *server* segments are redacted to `srv-<digest>` while built-in names and the `<tool>` segment stay — a built-in name is a fixed vocabulary that says nothing about the user, whereas the server segment is a string the user typed into their own MCP config, routinely a company, product, customer or internal-host name; the digest still answers the only two questions that segment is read for (how many MCP tools, and did X and Y come from the same server). **The ±200-char window was REMOVED, not gated:** it sliced `diag_body_json` — the outbound **request** — using a `line`/`column` that `genai::Error::StreamParse` derives from serde's position in the provider's **response** chunk, so the one thing deliberately kept ungated "because it localizes a bad escape" localized nothing and printed an arbitrary 400-char slice of the request. genai never surfaces the raw chunk text, so it **cannot** be repointed from that call site; replaced by a position-only line explicitly labelled for the document it refers to, with the reasoning and the capture point a real response-side window would need recorded in-source. **No oracle constraint:** `git grep byop-diag 42effe840` is empty, so `[byop-diag]` is fork-local and free to redesign. The leak test plants a secret in message text, tool arguments, a tool result, an attachment filename and a URL and asserts **every ≥8-char run** of it is absent — so a partial, truncated or escaped leak fails too — while also asserting the diagnostic shape that must survive, making it non-vacuous in both directions. Two residuals are logged as their own open items rather than left implicit (the provider `endpoint_url`, and the verbatim provider error body).

- [x] **`read_skill`'s disk fallback reads outside the skill index with no permission
      check.** `read_skill.rs:140-170`, `should_autoexecute` unconditionally true; the
      path shape test requires no home or workspace prefix.
      **VERDICT PARTIAL (independent verifier, 2026-08-21):** The unindexed disk read is real (`read_skill.rs:140-171`), but "outside the skill index" overstates the reach: `extract_local_skill_parent_directory` (`skills/file_watchers/utils.rs:232-257`) requires the path to end `<known-provider>/skills/<name>/SKILL.md` with the provider in `SKILL_PROVIDER_PATHS:187` — not an arbitrary-file read. `should_autoexecute` is pin-identical, so this is inherited, not a fork regression, and the no-prefix choice is documented at `:129-132`.

      **FIXED 2026-08-21 — the verdict UNDERSTATED it in one direction and overstated it in another.** Shape-checking does bound it to `<…>/<provider>/skills/<name>/SKILL.md` — but **the prefix group is completely unconstrained** (`SKILL_FILE_PATTERN` = `(.+)/…`), so any user's home, `/etc` or `/tmp` qualifies. And **the check is lexical on the *link* while `parse_skill` opens the *target***, so a planted symlink at a legitimate in-repo skill path made this an **arbitrary-file read** — with the model supplying the string directly (`tools/skill.rs:49-59`) and `should_autoexecute` unconditionally `true`, so nothing prompts. **The in-source claim that the missing home prefix is "at the same safety level as the cache hit" is false** — the cache-hit path is index-confined. Fixed by normalising lexically **before** judging (so validation and open see the same string), confining the read to exactly what `get_skills_for_working_directory_with_origin` could ever surface, failing closed when cwd or home are unknown, and **re-running the same test on the `canonicalize`d path at read time**, which closes the symlink escape. `should_autoexecute` left pin-identical — the fix is confinement, not a prompt. **The two existing fallback tests asserted the vulnerable behaviour** and now set a session scope.

### My own changes today — regressions and false claims

- [x] 🔴 **#616's fix is HALF A FIX and completions now fail closed silently.**
      `external_commands` is correctly left unset on a failed probe, but
      `load_external_commands_future` is still set, so
      `has_attempted_to_load_external_commands` (`session.rs:1145`) is permanently true
      and both retry gates (`view.rs:11146`, TUI `completions.rs:78`) skip. Chips now
      fail OPEN (intended), but `top_level_commands()` yields zero PATH executables for
      the session's life. Verify and fix — this is a regression introduced 2026-08-21.
      **VERDICT PARTIAL — NOT a regression (independent verifier, 2026-08-21):** The mechanism is real — `load_external_commands_future.try_insert` runs on every path (`session.rs:1386-1389`), so `has_attempted_to_load_external_commands` is permanently true and both retry gates skip. **But it is not a regression, and the 'completions now fail closed' claim is wrong.** Pre-fix (`eaf71e730^`) the cell was set to an EMPTY set, which `top_level_commands` (`:1397-1400`) flattens identically to an unset cell. Completions behaviour is unchanged by my fix. What remains is a pre-existing no-retry limitation, not damage I introduced.

- [x] 🔴 **`load_deferred_name_set` still caches failure — the sibling #616 never
      fixed.** `session.rs:1269` sets `storage` on every arm including the error arms
      that produce an empty set. Unlike `load_external_commands` the failure IS
      distinguishable (it logs a warn two lines above), so the "empty ≡ unknown"
      defence does not apply. `session_test.rs:421` blesses the caching and never tests
      retry.
      **VERDICT PARTIAL — no consequence follows (independent verifier, 2026-08-21):** `storage.set(new_names)` does run on all four arms including the two `HashSet::new()` error arms (`session.rs:1257,:1262`). But no consequence follows: retry is blocked by `future_cell.try_insert` (`:1274-1278`) regardless of what was stored, and every consumer reads `.get().into_iter().flatten()` (`:1078,:1086,:1403,:1407`), so `Some(empty)` and `None` are indistinguishable. No availability enum reads it, which is what made the #616 case harmful. Cosmetic.

- [x] 🔴 **The #615 regression test cannot fail, and DECLINED.md:179 states the
      opposite.** `shell_command_tests.rs:362-381` exercises only the extracted helper
      `write_skips_pty_permission_check`, never `should_autoexecute` (`:230`).
      Reverting the fix at the call site leaves it green. Both the test's doc comment
      and the DECLINED row claim "non-vacuous by construction — reverting turns it
      red". Both are wrong. Same error as the #620 test, made twice.
      **VERDICT PARTIAL — 'cannot fail' overstated (independent verifier, 2026-08-21):** Confirmed that `shell_command_tests.rs:363` calls only `write_skips_pty_permission_check` and never `should_autoexecute` (zero references in the file), and that reverting `shell_command.rs:230` to `is_none_or` leaves it GREEN — `lib.rs:4`'s `#![allow(dead_code)]` silences the then-orphaned helper. **But 'cannot fail' is wrong:** the fix's predicate IS the helper (`:64`), so reverting THAT turns the test red. Only the call-site wiring is unpinned. DECLINED.md:179's claim is therefore overstated rather than false, and the #620 analogy holds.

      **FIXED 2026-08-21 — both halves.** The verdict's "'cannot fail' is overstated" is right: the *predicate* is pinned, the **call-site wiring** was not — reverting `should_autoexecute` to an inline `is_none_or` merely leaves the helper unreferenced, and the crate-level `#![allow(dead_code)]` keeps the old test green. New `unresolved_block_write_falls_through_to_the_pty_permission_check` drives the real `should_autoexecute` with an unregistered `BlockId`, with a **precondition assert that the fixture profile resolves `write_to_pty == AlwaysAsk`** so it cannot go silently vacuous. **The `DECLINED.md` row is corrected** — it is at line **186**, not 179, and its claim that the single test was "non-vacuous by construction" was false as stated. Ledger path also stale: the file is under `ai/blocklist/action_model/execute/`, not `terminal/`.

- [x] **DECLINED.md:156 contains a false clause.** The credit-rounding claim checks out
      against the pin, but "the usage footer never appears at all" is wrong — the
      footer is opened by user toggle with no credit gate; a `None` rollup only
      suppresses the drill-down.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Rounding matches the pin (`conversation.rs:783-785` vs `42effe840:...:769-773`), and the verifier checked the consumer the original did not: `conversation_usage_view.rs:558-560` returns the bare value text when `rollup.is_none()` and `append_per_agent_rows` (`:590-592`) early-returns, while footer construction (`view.rs:6380-6446`) is driven by `ToggleIsUsageFooterExpanded` with no credit gate. The clause is false.
      **FIXED 2026-08-21:** False clause removed and replaced with what actually happens (the footer opens on an ungated user toggle; a `None` rollup suppresses only the drill-down). The rounding half of the row is untouched.

### Correctness — user-visible

- [x] 🔴 **ALL FOUR tab-group drag tests are silently skipped in CI and have never
      run.** `crates/integration/src/test/tab_groups.rs:228-230` gates on
      `cfg!(feature = "drag_tabs_to_windows")` evaluated against the **integration**
      crate, which never enables it (`crates/integration/Cargo.toml:69,73`); the app
      crate has it on by default, so the `cfg!` asks the wrong crate.
      `set_should_run_test` false → driver logs "Skipping test" → exit 0 → green. Five
      cross-window drag tests in `workspace.rs` go the same way.
      **This supersedes the three open TODO items below** — "tabs=1 slots=1 on all four
      paints" is exactly what a skipped test looks like, and the
      `maybe_render_frame` diagnosis at TODO:3147 is a false conclusion the whole
      investigation was blocked on. The collapsed hop at `view.rs:25460` was traced
      REACHABLE, contradicting TODO:3268.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `crates/integration/Cargo.toml:69` declares the feature and `:70` `default = ["run_on_linux"]` omits it; `pr-check.yml:603` passes no `--features`; `app/Cargo.toml:653` has it, so only `warp` does. **Verifier closed the chain the original left open:** `driver.rs:242-247` → `winit/event_loop/mod.rs:989` `std::process::exit(0)` → `tests/common/mod.rs:54-57` maps 0 to PASS. One sub-claim wrong: TODO.md:3259 already says the hop is reachable, so that part was not new.
      **FIXED 2026-08-21:** Both `drag_tabs_feature_enabled()` helpers now call `FeatureFlag::DragTabsToWindows.is_enabled()` instead of `cfg!(feature = "drag_tabs_to_windows")`, which asked the `integration` crate for a feature only `warp` enables. Safe because `run_test_and_cleanup` evaluates the predicate AFTER app init (`driver.rs:241`), so the flag registry is live. **These nine tests will now execute for the first time and may fail — that is the intended outcome.**

- [x] 🔴 **`current_repo_path` is filled by LOCAL filesystem detection on SSH sessions
      and never cleared.** `view.rs:11955-11958` assigns before the
      `active_session_path_if_local` bail at `:11982`. On a remote session the remote
      cwd is probed against the local FS, so a same-named local clone wins. The code
      review panel then auto-opens and diffs the WRONG repository. The pin cannot hit
      this — its detection is `LocalOrRemotePath`-typed.
      **VERDICT PARTIAL — 'never cleared' false (independent verifier, 2026-08-21):** The mechanism holds: `view.rs:11958` assigns before the local bail at `:11982`, and unlike the pin (which routes remote sessions to `RepoDetectionSessionType::Remote`) the fork always probes locally. But "never cleared" is false — each detection reassigns, normally to `None` (`repositories.rs:66` canonicalize fails). Wrong-repo needs an exact remote-path collision, and auto-open is only `view.rs:6809`. Already documented at TODO.md:2432.

      **FIXED 2026-08-21 — and the fix already existed in-tree, bypassed.** `apply_block_metadata_update` called `DetectedRepositories::detect_possible_git_repo` directly; that function is local-only (`from_local_canonicalized` + `find_git_repo` on the local FS), so on an SSH session it **walks the local filesystem with a remote path** — `None` usually, a coincidentally same-named local repo otherwise. It lands in `current_repo_path` and fires `PaneEvent::RepoChanged` **before** the `active_session_path_if_local` bail, reaching code-review auto-open, the repo banner, chips and telemetry, and registering a **local** `DirectoryWatcher` on the wrong directory as a side effect. **`app/src/util/repo_detection.rs` is the fork's own port of the pin's session-type gate** (`Remote` → `None`) — but only the TUI used it; the GUI called the raw model. Now routed through it. Ledger line numbers were stale (`:12015`/`:12039` now). `workspace/view.rs` was not needed.

- [x] 🔴 **Pane-group swap corrupts layout and loses a pane on restart.**
      `pane_group/mod.rs:3951` replaces a pane that this fork keeps IN the tree (the pin
      keeps child-agent panes off-tree), so the target becomes a leaf twice and the
      anchor's slot vanishes; `show_pane_for_child_agent` (`tree.rs:330`) has zero
      callers. `mod.rs:2090-2124` snapshots the replacement verbatim, so quitting during
      a swap loses the terminal pane. `visible_pane_count` (`tree.rs:219`) undercounts,
      so closing one of two panes during a swap kills the whole tab.
      **VERDICT CONFIRMED — one limb false (independent verifier, 2026-08-21):** `mod.rs:3951` calls `replace_pane` on a target that IS an in-tree hidden leaf (`add_pane_with_options` splits then hides, `:5921`), and `PaneNode::replace_pane` (`tree.rs:886-889`) overwrites only the anchor leaf, duplicating the target. The pin proves both other limbs: `42effe840:mod.rs:2129-2133` substitutes the original on snapshot (the fork's `:2090` does not), and `42effe840:tree.rs:216-218` warns against exactly the subtraction at `tree.rs:219-222`. **One limb false:** `show_pane_for_child_agent` has three production callers (`terminal_pane.rs:1002,1030,1046`), not zero.

      **FIXED 2026-08-21 — and TWO MORE CLAIMS IN THIS ENTRY WERE FALSE.** (i) `show_pane_for_child_agent` does **not** have zero callers: three live production callers at `pane/terminal_pane.rs:1002,1030,1046` (`RevealChildAgent`, `OpenChildAgentInNewPane`, `OpenChildAgentInNewTab`) plus three in `tree_tests.rs`. Neither dead nor an unwired repair path — not deleted, not rewired. (ii) **The `workspace/*` file attributions were wrong**: `workspace/mod.rs` is 1648 lines, so `:2090-2124`, `:3951`, `:5921` cannot be in it — they are `pane_group/mod.rs` lines, which match exactly. `workspace/view.rs:13234-13242` is `move_tab`, a tab-reordering `Vec::swap` whose match guard is adequate; unrelated to pane-group swap. **No `workspace/` file needed an edit at all.** The three real limbs are fixed in `pane_group/`: `visible_pane_count` (`tree.rs:251`) now uses the pin's `visible_pane_ids().len()` — the old subtraction removed off-tree hidden panes from a total that never included them, so a two-visible-pane group read as `1` mid-swap and `close_pane`'s `== 1` guard killed the tab; `replace_pane` (`:462`) now vacates and unhides the replacement's own slot first (recorded on `HiddenPane` as `DisplacedReplacement`, with rollback), because this fork keeps child-agent panes in-tree flagged hidden where the pin attaches them off-tree, so the bare leaf-rename left the pane at two positions, both hidden, and `PaneBranch::render` skipped the anchor's slot; `snapshot_for_node` (`mod.rs:2091`) ports the pin's substitution so a mid-swap quit persists the swapped-out terminal instead of dropping it. `revert_temporary_replacement`, `swap_active_pane_to_conversation` and `close_pane`'s child-agent branch were brought to pin shape too, with two adjacent pin lines **deliberately not ported** (`remove_hidden_pane` would unhide the child, wrong under this fork's re-hide-don't-destroy semantics; `clear_orchestration_split_off` has no fork equivalent). **Test gap recorded:** the snapshot test sits at `PaneData` level, not at `PaneGroup::snapshot_for_node`, because the real harness lives in `mod_tests.rs` outside the edit list. **Ledger note:** `move_tab` (`workspace/view.rs:13234`) would underflow on `tabs_len - 1` for an empty `tabs`; unreachable today, left alone rather than shipping a speculative guard.

- [x] **PR info refreshes on EVERY prompt, not only after `gh`/`gt`.**
      `view.rs:10300-10307` calls `refresh_pr_info` unconditionally in
      `refresh_warp_prompt`, which runs on every `BlockCompleted` and every OSC 7. The
      comment describes the pin's gate; the gate is absent (zero occurrences of the
      pin's `"gh" | "gt"` match). A `gh` subprocess plus a GitHub API call after every
      shell command, per pane.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `view.rs:10302-10307` calls `refresh_pr_info` inside `refresh_warp_prompt` with no command gate, and grep for `"gh" | "gt"` across `app/src/` returns nothing; the pin gates it at `42effe840:...:12393`. Verifier went past the citation: `github_repo_model/local.rs:177-208` has only an in-flight guard, no debounce, and spawns a `gh` subprocess. Callers include `BlockCompleted` (`:11500`) and OSC 7 (`:11512`).
      **FIXED 2026-08-21:** Unconditional call removed from `refresh_warp_prompt`; the pin's `refresh_pr_info_after_gh_or_gt_command` ported at `view.rs:4804-4824` with the `gh`/`gt` top-level-command gate (alias-resolved, flag-guarded) in the `AfterBlockCompleted` arm `:11289-11331`.

- [x] **TUI: a blocked agent command displays "Command finished".**
      `tui_cli_subagent_view.rs:354-360` reports an unresolved `block_id` as finished,
      and the status text returns before the `is_blocked` branch that carries the only
      ctrl-o/ctrl-r hint. #615's fix turned this from cosmetic into a silent deadlock.
      **VERDICT REFUTED (independent verifier, 2026-08-21):** `render` returns an EMPTY element when `target(app)` is `None` (`tui_cli_subagent_view.rs:569-573`) and `target_for_block_in_model` bails on an unresolved id (`block/cli_controller.rs:582`). An unresolved `block_id` renders nothing at all — never the wrong text. The finished-before-blocked ordering is also pin-identical.

- [x] **`TmuxCommandExecutor` never forgets a command and cannot be cancelled.**
      `tmux_executor.rs:53` inserts into `in_flight_commands` with no removal anywhere,
      no `cancel_active_commands` override, no `on_cancel`. Fork-original. Reachable via
      the user-toggleable SSHTmuxWrapper flag.
      **VERDICT PARTIAL — not fork-original (independent verifier, 2026-08-21):** Leak and no-cancel confirmed: insert at `tmux_executor.rs:53`, no removal, no `cancel_active_commands` override (trait default no-op), reachable via `command_executor.rs:200-213`. **But "fork-original" is false** — the file is byte-identical to upstream `57062bd92^` apart from an import; the pin simply deleted the whole tmux flow, which this fork keeps deliberately per DECLINED.md:210.

      **FIXED (leak) / REFUSED (cancel) 2026-08-21 — and the two limbs are ONE defect.** The map holds the channel's **only `Sender`**, so a forgotten entry both leaks *and* pins the awaiting `recv()` open forever. All three growth paths fixed: `.remove()` on completion, `.on_cancel` on the awaiting future (**the dominant path** — generator commands are aborted every keystroke), and **a guaranteed hang the ledger does not mention**: a failed `try_send` to the PTY controller logged a warning and **returned the receiver anyway**, so a command that never reached tmux awaited forever. **`cancel_active_commands` deliberately NOT overridden:** `TmuxCommand` has no kill-window variant, so an override could only clear the local map — which buys nothing, since each command owns a private channel unlike in-band's shared queue — and costs something real, because it fires session-globally on **Enter** and would turn every in-flight probe into a failure, importing the #616 mode into the tmux path. Residual recorded: a command that neither reports nor has its future dropped still hangs, and no sibling executor has a timeout either.

- [x] **`should_hide_command_grid` is computed then discarded by the layout math.**
      `block.rs:1548-1565` unconditionally adds command height and padding; the pin
      zeroes the term when the flag is set. Painter and layout disagree — a blank gap,
      and `is_visible()` true for a block with nothing rendered.
      **VERDICT PARTIAL — latent (independent verifier, 2026-08-21):** The divergence is real (fork's `height()` `:1548-1565` and `prompt_and_command_height()` `:1977-1978` ignore the flag; the pin zeroes both). But the pin's only PRODUCTION setter, `view/ambient_agent/view_impl.rs:383/399`, is absent from this fork — the remaining callers are tests. Latent, not a live blank gap.

      **FIXED at the model layer 2026-08-21 — path corrections, and the divergence is WIDER than stated.** The flag lives in `app/src/terminal/model/block.rs`, not `ai/blocklist/block.rs`, and the pin's setter is `terminal/view/ambient_agent/view_impl.rs`. **The fork dropped three of the pin's four honouring sites**, leaving only the TUI painter — so the answer is split: the TUI shows a **blank gap** the height of the hidden command (`is_visible()` true for a block with no content rows), while the **desktop** painter has the reverse, drawing a grid that should be hidden. Both model-layer sites restored verbatim from the pin. **Neither is user-visible today** — nothing in the fork sets the flag in production, since the pin's only setter is in an unported path — and both bodies were otherwise byte-identical to the pin, so this was a two-line restoration rather than a structural gap. `block_list_element.rs:2627` left alone and flagged in-source: that branch was rewired around `snackbar_header` and draws unconditionally, a separate port.

- [ ] **Orchestration rollup total is computed and thrown away.**
      `conversation_usage_view.rs:240` computes `rollup`, `:266-275` passes
      `usage_info.credits_spent` instead. "Credits spent (total)" omits every child
      agent's spend while the drill-down beneath lists children summing to more.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `rollup.total_credits` (`usage/rollup.rs:55`) has **zero non-test readers repo-wide**; `conversation_usage_view.rs:266,273` pass `usage_info.credits_spent`, and `render_total_credits_value_row:551-576` uses the rollup only to decide whether to add the "View details" toggle. `terminal/view.rs:6441-6446` feeds the footer that same single conversation's usage while `append_per_agent_rows:604` lists orchestrator plus descendants.

      **FIXED 2026-08-21:** new `headline_total_credits` (`conversation_usage_view.rs:171-191`) returns the rollup total when present, else `usage_info.credits_spent`; both call sites (`:289`, `:296`) use it. **The pin settles which end was wrong** — `42effe840:...:329-332` reads `rollup.total_credits` for the headline, citing "PRODUCT invariants 2a, 11" — so the headline was the defect and the drill-down was correct. Test at `:1300-1414` builds a real orchestrator plus two children and asserts the headline equals the **sum of the rendered drill-down rows** (derived from `rollup.per_agent`, not hardcoded); against the old code that was 1.0 vs 8.0. `rollup.rs` needed no change — the defect was entirely consumer-side. (No `platform_credits_spent` term: that field does not exist in this BYOP fork.)

      **SIBLING FOUND, fix dispatched:** the collapsed usage pill has the identical bug at `app/src/ai/blocklist/block/view_impl/output.rs:2769` (pill number is self-only) and — worse — `:2752`, where `has_any_usage = conversation.credits_spent() > 0.0` means **an orchestrator that spent nothing itself but whose children spent is denied a usage button entirely**, so the spend is invisible. The pin fixes both in `render_usage_button` (`42effe840:...output.rs:3686-3713`, "PRODUCT invariants 11, 11b").

      **FIXED 2026-08-21:** `output.rs:2741-2782` new `usage_pill_headline_credits` + `usage_pill_has_any_usage`, both rollup-derived, used for **both** the displayed number (`:2830`) and the suppression check — fixing only the number would have left the worse limb (button entirely absent) in place. Ported from `42effe840:output.rs:3686-3713`; no `platform_credits_spent` term exists in the pin's `render_usage_button`, so nothing BYOP-divergent had to be dropped, and the fork's BYO-API-key early return is preserved *ahead* of the rollup so BYOK users pay no cost. **Render cost considered and kept unmemoised, documented at the call site:** the non-orchestrator case is one empty-slice probe with no allocation; a cheaper totals-only sum was explicitly rejected as a second implementation free to drift from the footer, and headline-equals-footer is the invariant being fixed. Tests at `:3598` include the required case (orchestrator spends 0, child spends 30) plus a guard against the suppression check becoming a tautology. **Unrelated gap noticed, not acted on:** the pin's `output.rs` renders a "This response won't count towards your usage" notice via `should_show_failed_output_usage_notice`; the fork has both symbols (`view_util.rs:70,168`) but only `tui_export.rs:117` uses them, so the GUI output view is missing that notice.


- [ ] **`[byop] build_client: endpoint_url=` logs the user's provider base URL.**
      `chat_stream.rs:4584` prints the configured endpoint at `Info`, which for a
      self-hosted or corporate gateway is an internal hostname, and `warp.log` goes
      into `write_log_bundle_zip_to`. Same exposure class as the request-content leak
      fixed 2026-08-21 but **config rather than conversation content**, so it was left
      out of that change deliberately and needs its own decision: redact to scheme+host,
      digest it, or accept it. Flagged by the agent that fixed the content leak.

- [ ] **`[byop] stream chunk error:` prints the provider's error body verbatim.**
      `chat_stream.rs:5794`. Some providers echo a fragment of the request in a 400
      body, so this can carry request content. **Deliberately kept** — suppressing it
      leaves a failed turn with no diagnosis at all — and documented in-source as a
      known residual. Recorded here so the trade is visible rather than implicit.

- [ ] 🔴 **Terminal OSC-8 hyperlinks reach the OS handler with no scheme check.**
      `terminal/view/link_detection.rs:529,565` call `ctx.open_url` on hyperlinks
      emitted by whatever is running in the terminal — including a remote host over
      SSH. Same hole as the notebook markdown links fixed 2026-08-21, but the content
      is arguably *less* trusted. The scheme allow-list now lives in `notebooks::link`
      (`is_openable_url_scheme`); this call site does not consult it.

      **CORRECTION 2026-08-21 — the first fix was OVER-BROAD and broke a real feature.** `4f5e38690` refused `file:` outright for terminal content. **`crates/integration/src/test/osc8_hyperlinks.rs::test_osc8_file_scheme_opens_url` went red** — and it is right: `file:///tmp/osc8-test.txt` is a **local, hostname-less** URL, and a build tool or linter printing a clickable path to a local file is the feature OSC 8 exists for and why this fork ported it (`ccc1e3c84`, #11). **`precheck` does not run `-p integration`**, so this was invisible to every local run; only the separate 3-shard suite catches it. Now `file:` is allowed when the authority is local and refused when it is not — `file://host/share/x` is a UNC path on Windows, so the OS opens an SMB connection to a host the *link* chose, and terminal output can arrive from a remote machine over SSH. Same rule and same reasoning as `notebooks::link::file_url_is_local`. Two unit tests added on both sides of the line, one of them naming the integration test that caught this.

- [ ] **`AppContext::set_before_open_url` cannot refuse a URL, only rewrite it.**
      `warpui_core/src/core/app.rs:599` — `BeforeOpenUrlCallback` is
      `Fn(&str, &AppContext) -> String`, so the one global pre-open hook cannot enforce
      a scheme allow-list app-wide and every call site must guard itself. Fix: change it
      to `-> Option<String>` and treat `None` as "do not open". Would let the ~40
      `ctx.open_url` call sites be secured in one place instead of individually.

- [ ] **The scheme allow-list exists in three places and they disagree.**
      `notebooks::link::is_openable_url_scheme` (new, reads `ChannelState::url_scheme()`),
      `crates/warpui/src/browser.rs:29` (hardcodes `warposs`/`zap` — stale, the OSS channel
      scheme is now `phosphor`, so the browser build's own deep links fail its check), and
      the WSL guard in `crates/warpui/src/windowing/winit/delegate.rs:110`. The policy's
      natural home is `app/src/uri/`; move it there and have all three consume one definition.

- [ ] **Failed settings writes are invisible to the user.**
      `report_if_error!` is log-only since the Sentry sink was removed
      (`crates/warp_core/src/errors.rs:212-223` — `report_error` is a documented no-op),
      so a toggle clicked while `settings.toml` is unparseable flips **in-memory**, never
      persists, and silently reverts at next launch. The startup
      `SettingsFileError::FileParseFailed` banner (`app/src/lib.rs:1401-1424`) says the
      *file* is broken — captured once at startup, not shown at the click, which may be
      hours later — and never says *this toggle was lost*.
      **Fix:** error toast at the write site (`ToastStack` + `add_ephemeral_toast`, the
      pattern at `app/src/root_view.rs:889-895`) plus a new `t!` string. **Blocked on**
      the `AppContext`-only global-action handlers (`workspace/global_actions.rs:137-172`)
      having no `window_id`. Deliberately not built blind during a no-build round.

- [ ] **~40 production `let _ = …set_value(…)` sites discard the error entirely.**
      `settings_view/ai_page.rs` (~20), `appearance_page.rs`, `app_menus.rs:730`,
      `agent_input_footer/mod.rs:1369`. This is the *pre-`e0c3dfe2f`* behaviour — silent
      no-op with no log line at all — so they are not crashes and were left out of the
      `.expect` sweep. `settings_view/features/external_editor.rs:245-250` is already
      correct (`report_if_error!` + `unwrap_or`) and is the pattern to copy.

- [ ] **`ai/blocklist/block.rs:274` maps `is_supported_image_file` straight to**
      **`FileTarget::SystemGeneric`, including `.svg`.** `.svg` is a scripting document
      whose default handler is normally a browser, so this hands model-referenced SVG to a
      handler that executes content. Same defect as `notebooks/link.rs:945`, **fixed there
      on 2026-08-21 but not here.** Proper fix: add `is_supported_raster_image_file` to
      `util::openable_file_type` (current list minus `svg`) and have both call it —
      `is_supported_image_file` itself must NOT change, because its four other callers mean
      "can we display this as an image", which stays true of SVG.

- [ ] **CORRECTION to the 2026-08-21 notebook link-scheme entry — it was wrong twice.**
      (i) Its claim that `WebIntent::try_from_url`'s `ALLOWED_ACTIONS` acted as "a second
      gate" on the app's own scheme was **false**. `try_from_url` is reached only from
      `maybe_rewrite_web_url_to_intent` (web→intent); an own-scheme URL returns through
      `on_open_urls` → `handle_incoming_uri` → `validate_custom_uri`, which routes on
      `UriHost` and never calls it. So `phosphor://launch/<name>` — which loads a launch
      config and starts every tab and command it defines — was **one plain click away in
      `Selectable` (model-authored) views.** (ii) The same entry's `file:` claim was also
      wrong: `resolve` handles `file:` *before* the allow-list, and the test that "proved"
      the refusal ran against a **session-less fixture** whose own comment admitted it.
      Both closed 2026-08-21 by allow-listing `UriHost` exhaustively at the notebook call
      site. **The justification was doubly wrong:** `set_before_open_url` runs *after*
      `ctx.open_url`, so the rewriter never needed the own scheme in the notebook
      allow-list at all.

- [ ] **The usage footer is still frozen at open time for everything except credits.**
      `terminal/view.rs:6438-6448` builds a `ConversationUsageInfo` snapshot when the
      footer opens and passes it into `new_footer_with_rollup`. The credits headline was
      moved to a live read on 2026-08-21, but `credits_spent_for_last_block`, `tool_calls`,
      `models`, `context_window_usage`, the file/line/command stats and `TimingInfo` are
      all still the snapshot — so they stop updating while the user watches them.
      **Clean fix:** stop passing a snapshot at all and let the view derive everything from
      `parent_conversation_id` at render, exactly as the credits now do. Same defect class,
      wider surface.

- [ ] **Unverified assumption: is the usage-footer rich-content view re-rendered when its**
      **conversation updates?** If it is not, the live credits read is inert. Note this
      would be **pre-existing and would affect the rollup limb equally** — `b18a81603`
      already depends on it — so it is not something the 2026-08-21 change introduced.
      Needs a run to settle; the build gate was closed when it was found.

#### BYOP logging residuals — the exhaustive list (2026-08-21)

All 74 production `log::` sites in `chat_stream.rs` were judged individually: 20 route
through the tier boundary, 44 are structural (counts, ids, offsets, enum variants), and
the 10 below are deliberate residuals. This list supersedes the earlier "two residuals"
claim, which was wrong by four.

- [ ] **`scan_suspicious_backslash` prints up to 5 x 10 bytes of the request body**
      (`chat_stream.rs:5616,5619`) on a `\u`/`\x` hit. **Not fixed on purpose** — those ten
      bytes *are* the finding. Documented in-source; it was missing from the ledger.
- [ ] **`[byop][webfetch] error` (`:7553`) leaks the fetched URL.** `web_runtime` builds
      `HTTP GET {url}` into its context chain, so the URL reaches `warn` ungated. Websearch
      (`:7570`) carries the Exa endpoint, not the query.
- [ ] **`[byop] open stream failed` (`:5887`)** — same class as the recorded `stream chunk
      error`, not previously recorded.
- [ ] **Parser error text** (`:6587`, `:7804`, `:7826`) — serde_json is normally
      position-only, but `unknown field` / `invalid value` renderings can quote a field name
      or a short value.
- [ ] **`RUST_LOG=debug` widens the tier** — `:6512` prints raw tool arguments. Off by
      default, but a verbosity switch is **not** a privacy opt-in, so it is a residual
      rather than a gate.
- [ ] **Proxy URL host** (`:4858`) still logged after userinfo redaction — same class as the
      already-recorded `endpoint_url`.

- [ ] **`InlineDiffView::restore_diff_base` writes the diff base over the file with no**
      **conflict check.** Unlike accept (fixed 2026-08-21 via `FileModel::save_if_unchanged`),
      its correct pre-image is the content the *accept* wrote, which the view does not retain,
      so guarding it against the diff *base* would refuse every revert following a
      format-on-save. Fix: retain the accepted bytes at accept time and guard with those.

- [x] **`warp_tui/src/tui_diff_storage.rs:147` is the TUI counterpart of the lost-update**
      **defect.** Same AI-diff persistence, same `register_file_path(..., false, ...)`, same
      unguarded `save`. `FileModel::save_if_unchanged` is now available for it.

      **FIXED 2026-08-21 — and it was THREE write modes, not one.** `dispatch_write` routes `Write`, **`Delete`** and **`Rename`** through the guarded API, with both rename endpoints checked before any mutation. `rename_and_save` ended in `async_fs::rename`, which **silently destroys whatever is at the destination** — rename succeeds, so there was no error path at all — and `diff_application.rs` rewrites the rename-onto-existing case into deletion+update, so a `PersistAction::Rename` only ever targets a path absent at proposal time, i.e. the destination pre-image genuinely is `Absent`. `FileModel::delete` had no guarded variant and `ExpectedDiskState` could not express a delete's pre-image at all; both now exist.

- [ ] **The save-conflict toast drops the reason.** `code_diff_view.rs:655` hardcodes
      "Failed to save file {path}" and ignores the error's message, so *why* the write was
      refused reaches the log and the agent but not the user. One-line fix: use the error's
      `Display` for `FileSaveError::Other`. Un-localised today; key would be
      `code-diff-save-conflict` with a `$file` variable.

- [ ] **`tab.move` over local control acks moves it did not perform.**
      `local_control/handlers/app_state.rs:453-474` dispatches `MoveTabLeft/Right` and
      `ack(...)`s unconditionally, so a scripted caller cannot tell a performed move from a
      refused one — and the 2026-08-21 `can_move_tab` port substantially **enlarged** the
      refused set (pinned boundary, group edge). Fix: check `can_move_tab` before dispatch
      and return `TargetStateConflict` (already used three times in that file).
      **Blocker:** `Workspace::can_move_tab` is `pub(super)`; it needs widening to
      `pub(crate)` or a thin `pub(crate)` wrapper. `TabMovement` is already reachable.

- [ ] 🔴 **Redirection glued to or preceding the command name defeats the Agent Mode denylist.**
      `simple/parser.rs:146-149` consumes `<`/`>` *inside* `parse_part`, so `rm>/dev/null -rf ~`
      yields candidates `rm>/dev/null -rf ~` and `rm/dev/null -rf ~` and no `rm .*` rule matches;
      `parser.rs:91-94` consumes a leading redirect, so `>/dev/null rm -rf ~` decomposes with the
      redirect **target** as the command name. Both confirmed running `rm` in bash.
      **`rm 2>/dev/null -rf ~` IS caught, so this is accidental, not declined.** The
      `contains_redirection` guard does not compensate — it is consulted only in the
      `AgentDecides` arm (`permissions.rs:964`), *after* the denylist, and never under
      `AlwaysAllow` or auto-approve-with-org-denylist, i.e. the modes where the denylist is the
      only gate. Fix belongs in `parse_part`/`parse_command_list` (redirect operators should
      delimit a part, not be absorbed), which needs checking against command x-ray, error
      underlining and the allowlist. **Deliberately not half-fixed:** closing the glued form
      while leaving the leading form open is the false-confidence failure the residue list exists
      to prevent. Pin-parity.

- [ ] 🟠 **Brace expansion and shell control-flow keywords hide the command name from the denylist.**
      `{rm,-rf,~}` decomposes to `rm,-rf,~`; `{r,}m -rf ~` to `r,` + `m -rf ~`;
      `if true; then rm -rf ~; fi`, `while … do rm …; done` and `for … do rm …; done` all make
      `then`/`do` the command name. All confirmed running `rm` in bash. Brace expansion is purely
      **textual and statically decidable** — it is not covered by the "needs the shell evaluated"
      residue bullet — and the grouping form `{ rm -rf ~; }` *is* caught, so the parser is
      inconsistent rather than deliberate. The existing advice to "carry denylist entries for the
      prefixes" is sound for `sudo` and useless for `then`/`do`. Repair is a parser change.
      Pin-parity.

- [ ] **Zero-command input makes both the denylist and the allowlist vacuous.** `;`, `{}`, `()`
      and whitespace-only input decompose to zero commands, so the denylist `.any()` is false and
      the allowlist `.all()` is true, and `AlwaysAsk` returns `Allowed(ExplicitlyAllowlisted)`.
      No zero-command spelling was found that also executes anything, so this is a latent hazard
      rather than a bypass — recorded so it is not rediscovered as one.

- [ ] **The codebase-index embedding model switches on provider-list ORDER, spending the**
      **user's quota.** `resolve_configured_embedding_model` returns the first entry of
      `SUPPORTED_EMBEDDING_MODELS` that resolves, so merely *adding* a provider can re-key an
      index that was working: `storage_key()` changes (`full_source_code_embedding/mod.rs:208-216`),
      `known_hashes` returns nothing, and the next full sync re-embeds **every repo** against the
      user's own paid provider. The user never chose a model.
      **Fix:** prefer the model the vector store already holds rows for whenever it is still
      configured, falling back to preference order only otherwise — a `SqliteVectorStore` query
      plus a change to what `preferred_model` means. That changes indexing behaviour, not just
      reporting, which is why 2026-08-21 shipped only a `log::warn!` naming both models and the
      cost. **A log line is not consent** — the honest fix is to stop making the choice on the
      user's behalf, not to add a prompt. Related: the index-consent-banner row in `DECLINED.md`.
      i18n keys drafted but not added: `settings-code-embedding-model-switched{,-desc}`.

- [ ] **`DaemonStoreClient` has the same two-cache desync the app path just fixed.**
      `remote_server/codebase_index_store.rs:354-386` holds one model in a `Mutex` while its own
      `CodebaseIndex` caches another, and `remote_client_preferences` only ever ships the
      *preferred* model's endpoint. The per-model endpoint table (`set_endpoints`) is available
      to it; wiring it needs that file.

- [ ] **`crates/warp_features/src/lib.rs:888` still states the opposite of the code.**
      It says *"This fork does not ship autoupdate: … the release workflow publishes no
      update feed."* `DECLINED.md:179` records that exact sentence as corrected on
      2026-08-20 **and** says the rationale is duplicated at the removal site "because that
      is where someone restoring parity will be standing" — but the correction never landed
      there. The authoritative-looking in-source comment is still wrong.

- [ ] **`InlineDiffView::restore_diff_base` (GUI revert) is still unguarded, on a premise now**
      **refuted.** `0219e06c3` deferred both reverts saying the accepted bytes are not
      retained. For the TUI that was shown false on 2026-08-21 — the content is a pure
      function of the diff the caller already holds — and the TUI revert is now guarded.
      The GUI path carries the same shape and the same stale note.

- [ ] **TUI `/rewind` has zero revert tests.** `tui_diff_storage_tests.rs` covers only accept.
      The four revert pre-images and the `REVERT_CHAIN_TAIL` ordering added 2026-08-21 are
      untested.

- [ ] **A refused `/rewind` is invisible to the user.** `terminal_session_view.rs:4151` shows
      "Rewound conversation and reverted file edits" unconditionally; refusals arrive after
      that function returns and land only in the log. `TransientHint` is view-owned, so
      `revert_file_diffs(&[FileDiff], &mut AppContext)` cannot reach it — closing this needs a
      call-site change, either handing it a way to raise the hint or returning the completions
      for the view to await.

- [ ] **The pin's second consumer of `is_container_subshell` is still absent.**
      `42effe840:writeable_pty/pty_controller.rs:444` writes the bootstrap in 4KB chunks with
      50ms gaps under a container subshell, because the double-PTY proxy in
      `docker/podman exec -it` drops data on large writes. The guard function was ported
      2026-08-21 but only its first consumer; this one needs `pty_controller.rs`.

- [ ] **Three stale "real preprocess pipeline" comments in `crates/warp_tui/`.**
      `tui_permission_prompt_tests.rs:333` and `tui_generic_tool_call_view_tests.rs:21` say the
      action "blocks on confirmation through the real preprocess pipeline" — it does not, the
      fixture installs it already blocked. `test_fixtures.rs:43-45` says the helper enqueues
      "action preprocessing through `ctx.spawn`"; it emits synchronously, so `settle()` is
      still needed but for the effect flush, not preprocessing — the code is right and its
      justification is wrong.

- [ ] 🟠 **Daemon sockets are not version-partitioned in practice, despite the docs and**
      **three tests saying they are.** `daemon_socket_name()` / `daemon_pid_name()` — and so
      `version_hash` — are **production-dead**: their only non-test callers are
      `ssh_transport.rs::remote_daemon_{socket,pid}_path`, which have **zero callers**
      repo-wide. The live path is `remote_server/unix/proxy.rs:23-33`, which hardcodes
      `"server.sock"` / `"server.pid"`. Either wire the versioned names or delete them with
      their tests and correct the doc comments.

- [ ] **`setup_tests.rs:554-575` `version_hash_is_deterministic` is vacuous.** It never calls
      `version_hash`; it re-implements `DefaultHasher` inline and asserts the copy against
      itself, so it stayed green through the 2026-08-21 switch to a stable hash and now
      actively misinforms. Replace with a **pinned literal** through
      `remote_server_identity_dir_name` — that is the only assertion that can catch a silent
      algorithm change.

- [ ] **`AIAgentActionType::FileGlobV2` has no slot for a result limit, so a model's**
      **`limit` cannot be honoured.** The parameter is accepted and the schema says plainly
      that results are always capped at 200 and a smaller value is not applied — so it is
      documented rather than silently dropped — but honouring it needs a field on that
      pin-inherited enum, which carries the upstream `TODO: Maybe implement client side depth
      and result limits`. Filed rather than diverging a shared crate.

- [ ] **`script/precheck` does not run `-p integration`.** Its package list covers 40 crates
      and excludes the integration suite, which CI runs as a separate 3-shard job under
      `xvfb-run`. So a change to integration assertions — or, as on 2026-08-21, an over-broad
      security fix that breaks a feature only that suite covers — passes a fully green local
      `precheck` and fails in CI. That is precisely the round trip `precheck`'s own header
      says it exists to prevent. Either add it (it takes ~5.5 min locally) or say plainly in
      the header that integration is not covered.
### Reliability

- [x] **Compaction can hide messages that were never summarised.** `commit.rs:71` and
      `chat_stream.rs:1981` compute the head/tail split with DIFFERENT configs, and both
      hardcode `ModelLimit::FALLBACK` so a 1M-window model is budgeted at 172k.
      **VERDICT PARTIAL — FALLBACK half inert (independent verifier, 2026-08-21):** The config mismatch is real (`chat_stream.rs:1981` uses `CompactionConfig::default()`, `controller.rs:4096` passes `from_settings`), but it diverges only under NON-default settings. The `ModelLimit::FALLBACK` half is inert: `select` uses it only via `preserve_recent_budget`, clamped to `MAX_PRESERVE_RECENT_TOKENS`=8000 (`config.rs:60-63`) for any usable ≥32k, so 200k and 1M give an identical split.

      **FIXED 2026-08-21 — cause re-derived; this entry's stated cause is minor and the verifier's inertness finding is right.** The config mismatch barely matters (`config.rs:59-63` clamps `preserve_recent_budget` to 8000 for any usable ≥32k, so 200k and 1M give the same split). **The real cause is three orderings/sets nobody reconciled:** (1) `message_view::project` **re-sorts by timestamp** and its doc claims that matches `build_chat_request` — it does not; `build_chat_request` uses `collect_linearized_task_messages`, **DFS order, which exists precisely because timestamp sorting was the Issue #94 defect** — so `head_end` was computed in one order and indexed into another by all three consumers; (2) `commit_summarization` collected via `all_linearized_messages`, which has **no UserQuery dedup and no orphan-task fallback** — a different *set*, not just a different order; (3) by commit time the conversation has grown by the summary `AgentOutput`, and since `algorithm::turns` cuts only on user messages it folds into the **final turn**, inflating the size `select` measures against an 8000-token budget and reliably moving the cut later. **The messages that fall in the gap** are the most recent full turn before compaction: hidden from every later request while absent from the summary that replaced them. **Recoverable in principle** — the protobuf is untouched, hiding is a sidecar — but nothing pops `completed`, so there is no path to undo it. Fixed by building the projection in request order and recording the head actually sent; the fallback excludes the summary output. **Left alone:** `message_view::project`'s sort and its now-contradicted doc, outside the edit list.

- [x] **A truncated stream is indistinguishable from success.**
      `chat_stream.rs:5842-5924` — `end_count` is counted but never checked; a
      connection dropped mid-SSE ends the loop normally and the partial text is
      committed as a complete turn.
      **VERDICT PARTIAL — narrower cause (independent verifier, 2026-08-21):** `end_count` is genuinely write-only (`:5451,:5740`, logged `:5876`), and a stream ending without `End` does commit partial text. But the stated cause breaks: a DROPPED connection surfaces as `Some(Err)` (`web_stream.rs:180`, `openai/streamer.rs:400`) and `:5486-5515` yields `AIApiError::Other` and returns. Silent only for a clean close missing `[DONE]`.

      **FIXED 2026-08-21 — settled from genai's source, not inferred.** `openai/streamer.rs:402-404` and `anthropic/streamer.rs:275` return `Poll::Ready(None)` when the byte stream ends without the terminal event, and `done` is set only *after* emitting `End`; a dropped connection is a separate `Some(Err(WebStream))`. So `end_count == 0` with no error **provably is** truncation. `end_count` was write-only and every loop exit fell into one unconditional `yield Ok(make_finished_done(..))`. **Compounding beyond the ledger:** with no `End` there is also no `captured_usage`, so `token_usage` is empty and `usage_metadata` is `None` — `/cost` loses the turn, the context meter does not move, and `aggregate_token_count` is 0 so the **auto-overflow check is skipped entirely** for that turn. Now errors instead of `Done`; partial content is preserved (already yielded, and `final_messages` flushed). `Err` chosen over `Reason::Other` because the proto's `Other {}` carries no text. Gemini synthesises its `End`, so it cannot false-positive.

- [x] **Vertex token cache is never invalidated on 401.** `vertex_auth.rs:189-195`,
      30-minute TTL with no eviction path; after a revocation every turn 401s for up to
      30 minutes and a successful re-login still returns the stale cached token.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `vertex_auth.rs:188-195` is the only reader and `store_token:198-212` the only writer; no eviction API exists, and grep for `401` in `agent_providers/` finds only comments. Verifier added the decisive detail: the cache key is the settings `credential` string (`:123`), unchanged by re-login, so a re-mint is impossible before the 30-minute TTL.

      **FIXED 2026-08-21:** `vertex_auth.rs:198-220` adds `invalidate_token`; `chat_stream.rs:4790` `evict_vertex_token_on_auth_failure` evicts on 401/403 (403 included deliberately, documented) from both stream error sites (`:5492`, `:5583`). Credential captured at `:5085` before `api_key` moves into `build_client`. Cache test added.

- [x] **`agent_id_to_conversation_id` is keyed by a mutable value.**
      `history_model.rs:2682` keys on run_id-or-server-token; the pin keys on run_id
      only. A rebound token resolves to the previous owner, and
      `remove_conversation_from_memory:2219` leaves the old key pointing at a deleted
      conversation.
      **VERDICT PARTIAL — no production path (independent verifier, 2026-08-21):** The keying divergence is confirmed (`agent_link_id()` = run_id-or-token vs the pin's `orchestration_agent_id()` = run_id only), and `history_model.rs:2218-2220` lacks the equality guard its token sibling has. But the "rebound token" scenario has NO production path: `set_server_conversation_token_for_conversation[_and_persist]` has zero non-test callers.

      **FIXED 2026-08-21 — the verifier is right that token rebinding has no production path, but THAT IS NOT THE MUTATION.** `run_id` going `None → Some` is: the first `StreamInit` keys the entry under `agent_link_id()` (the **token**), a run id arriving later changes the key and **inserts without re-keying**, and `remove_conversation_from_memory` computes only the *current* key — so a token entry survives pointing at a deleted conversation. **And the orphan wins:** `conversation_id_for_agent_id` hits this map first and only falls back to `server_token_to_conversation_id` on a miss, so a stale hit **shadows** the index that is maintained correctly. A lookup that should return `None` returns a dead id. Startup also keys off `run_id` alone, so the same conversation is keyed differently before and after a restart. **The pin settles it:** `42effe840:history_model.rs:2892` is `orchestration_agent_id()` and `42effe840:conversation.rs:1093` is just `run_id()` — the fork's `agent_link_id()` here is the divergence; restored verbatim, removal equality-guarded. **No wrong-conversation hit is claimed** — no production path reissues a token to a different conversation — but it was one unguarded reissue away. `agent_link_id()` is left alone for *recording* a parent link, which is a snapshot rather than a map key. **Related but distinct:** `resolved_parent_conversation_id_from_refs:596` still has no self/cycle guard, so mutual `parent_agent_id` references resolve into each other — its own open defect.

- [x] **MCP servers whose sanitised name contains `__` are unroutable.**
      `mcp.rs:42-53` maps non-alnum to `_`; `parse_mcp_tool_call` splits at the FIRST
      `__` (`:168`). Every tool of such a server is advertised and permanently
      uncallable.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `mcp.rs:44-53` collapses each non-alnum char to `_`, so any two adjacent characters (e.g. `"GitHub (remote)"` → `GitHub__remote_`) produce `__`. `function_name:57-63` embeds it; `parse_mcp_tool_call:172-177` splits at the FIRST `__` and then requires an exact `sanitize_server_name` match the truncated prefix cannot satisfy. Verifier also checked recovery: `is_mcp_function` (`:7157`) returns before `recover_tool_by_arg_shape`, so there is no fallback.

      **FIXED 2026-08-21:** `mcp.rs:59-78` `sanitize_server_name` collapses `_` runs and trims ends, so a sanitised name can neither contain nor abut `SEP`; the server-id fallback in `serialize_outgoing_call` goes through it too. Round-trip test added inline (`function_name_roundtrip_tests`) because `mcp_tests.rs` was outside the edit set.

      **REFUTED THEN REPAIRED 2026-08-21.** The collapse fix traded "one server unusable" for "two servers silently ambiguous" — a worse failure, since `mcp.rs:196-200` resolved with `find(..)`, first match wins. Confirmed by re-running the old sanitizer: `"GitHub (remote)"` and `"GitHub remote"` both → `GitHub_remote`, both advertised, both resolvable. **Worse than the refuter stated:** `"!!!"`, `"文件服务器"` and `"Сервер"` all → `""`, and the resulting `mcp____read_file` *does* resolve — `split_once("__")` yields `("", "read_file")` — so the empty key was a live misroute covering the entire non-Latin world, not merely a broken name. **The pin offers nothing:** `42effe840` has no `app/src/ai/agent_providers/` tree, no `sanitize_server_name`, and no `mcp__` in `app/src` at all — this module is fork-local, so the design is ours. Repaired with a two-branch injective encoding (`:122`): a name already in `[A-Za-z0-9-]+` is used **verbatim** (so `server-a`/`my-server` keep their exact current keys — no prompt-cache or existing-test regression), anything else becomes `<stem>_<8 hex FNV-1a of the full original>`. `_` is excluded from the canonical alphabet so the branches are disjoint by construction. **The residual 32-bit hash collision is closed at the other end rather than asserted away:** `parse_mcp_tool_call` (`:338`) now **counts** matches and errors naming both candidates instead of taking the first; `build_mcp_tool_defs` (`:271`) dedupes after sorting so a collision cannot produce the duplicate-tool-name 400 that kills the whole turn; `parse_read_resource` falls through to the empty `server_id` (locate-by-uri) on two matches rather than picking. Worst case is visibly unusable tools, never a wrong-server execution. Same-name servers — the one case no name-derived function can solve — get a `_<fingerprint of server.id>` suffix applied **only to the servers that actually clash** (`server_keys:165`). **Stability reasoning:** the tiebreak derives from the persisted `installation_id()`, not from position in `ctx.servers` (this file documents that order as drifting between requests, so an order-derived suffix would rewrite every function name on reshuffle); and `fingerprint` (`:68`) is hand-rolled FNV-1a rather than `DefaultHasher`, whose algorithm is documented as unspecified and free to change between Rust releases — this digest is baked into prompt-cache keys and serialized history. **Point 5 reversed:** `serialize_outgoing_call` (`:503`) no longer sanitises the raw id — sanitising mapped it *into* the key space live servers occupy, making an accidental match **more** likely, i.e. the opposite of the old comment's claim. It now emits `unresolved_<fp>_id`, provably outside the range of `server_keys`. The false "globally unique" comment (`:128-130`) is corrected, and the vacuous tests are replaced by 9 real ones (`:593`) including direct injectivity over a 16-name corpus and key stability under reordering.

- [x] **Partial agent-message delivery is reported as total failure.**
      `send_message.rs:115-137` breaks on the first failing address and discards
      `delivered_ids`, so a retry duplicates to earlier recipients.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `send_message.rs:117-131` breaks on the first `Err`; `:133-161` returns `Error(err)` and drops `delivered_ids` entirely, so the model gets no partial-delivery signal. Verifier established it is fork-introduced: the pin (`42effe840:send_message.rs:186`) makes ONE call for all addresses and reads back `response.message_ids`, so it has no per-address loop that could abort mid-way.

      **FIXED 2026-08-21:** `send_message.rs:117-175` attempts every address; `Success` only if all land, otherwise `Error` naming delivered addresses+ids and failed addresses and instructing the model not to resend to the delivered ones. `SendMessageToAgentResult` in `crates/ai` has no partial variant and was not editable, hence the encoding in the error text.

### Ledger and test-integrity defects

- [x] **TODO.md:534-535 and :562 state the opposite of the code.** They claim
      `should_validate_dcs_hook_session_id` is hardcoded `false` and blocked on #419.
      The code is `!self.shared_session_status().is_viewer()`
      (`terminal_model.rs:2728`), both gate tests are live, and #532 is closed. They
      also name a nonexistent file. Anyone trusting this concludes the anti-spoofing
      gate is off.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `terminal_model.rs:2728-2730` is `!self.shared_session_status().is_viewer()`, both gate tests are live (`terminal_model_test.rs:2257,:2308`), and `gh issue view` shows #419 AND #532 both CLOSED. The named file `terminal_model_tests.rs` does not exist. **Verifier found a THIRD instance the original missed:** TODO.md:5805 repeats "remains hardcoded `false`", contradicting TODO.md:5739.
      **FIXED 2026-08-21:** All three sites corrected to `!self.shared_session_status().is_viewer()`; #419/#532 noted closed; filename corrected to the singular `terminal_model_test.rs`; the `:5805` residue marked SUPERSEDED.

- [x] **A ported denylist test carries the INVERTED, pre-fix assertion, hidden behind
      `#[ignore]`.** `permissions_test.rs:641,661-676` asserts NOT-denylisted where the
      pin asserts denylisted with the message "user denylist entries should be merged
      with org denylist, not replaced". The fork's own fix (`52382d125`) merges, so the
      test now encodes the defect that commit fixed. Its ignore reason never mentions
      the inversion, so lifting it resurrects replace semantics as expected behaviour.
      **VERDICT PARTIAL — consequence fails (independent verifier, 2026-08-21):** The inversion is real (`permissions_test.rs:661-676` vs pin `:661-668`), but the stated consequence does not follow. The hunk is inherited from `0dbd3d567`, not authored by `52382d125`, and lifting the `#[ignore]` cannot resurrect replace semantics: with `current_team()` = `None` the PRECEDING `git status` assertion (`:641-657`) fails first. The mechanism is already declined (`DECLINED.md:83`, #445).

      **FIXED 2026-08-21 — the assertion, not the code.** The inversion is real: the fork asserted `!matches!(…ExplicitlyDenylisted)` where the pin asserts `matches!(…)` with "user denylist entries should be merged with org denylist, not replaced". **The code is right** — `get_execute_commands_denylist_for_profile` merges org into user with dedup — so the test was corrected, not the merge. **It does belong to the ten `#[ignore]`d tests** (counted: exactly ten, matching the `DECLINED.md` row). **The verifier's consequence-refutation holds and is now recorded in-file:** lifting the ignore could not resurrect replace semantics, because `current_team()` is `None` so the preceding `git status` assertion fails first. Fixing it still matters for the day `current_team()` gains a producer, when the inverted text would go red for the wrong reason and invite someone to "fix" the merge into a replace.

- [x] **The regression test for that same fix cannot fail.**
      `permissions_test.rs:1563` — `current_team()` returns `None` unconditionally, so
      the org denylist is never read and the test passes against the pre-fix code. The
      merge arm is unreachable in production.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Verifier checked the counterfactual rather than the doc comment: `test_empty_org_denylist_allows_user_entries` (`permissions_test.rs:1563`) sets `Some(vec![])` into `workspace.teams[0]`, but `ai_autonomy_settings()` reads `current_team()` → `None`. Pre-fix (`unwrap_or_else`) and post-fix (`permissions.rs:419-430`) both take the `None` arm — identical result. The test cannot fail.

      **RESOLVED 2026-08-21 — `#[ignore]`d, matching its siblings; `DECLINED.md` amended to ten.** Confirmed vacuous: `update_ai_autonomy_settings` writes into the team's `organization_settings`, `current_team()` is unconditionally `None` (`user_workspaces.rs:229-231`), so `get_execute_commands_denylist_for_profile` always takes the `None => user_denylist` arm and the `Some(org_denylist)` merge arm it guards is unreachable. **Ignored rather than deleted, and the distinction matters:** the two siblings `f4a45bed8` deleted were deleted because they would be **red** (they assert a merged, non-empty org list); this one is **green but fake**, which is exactly what `#[ignore]` is for here. The merge arm is kept verbatim from the pin per the existing `DECLINED.md` row, so the test becomes a real regression test the day `current_team()` gains a producer.

- [x] **Six permission mutators write to a store nothing enforces.**
      `permissions.rs:1009-1202` write `AISettings.agent_mode_command_execution_*`;
      enforcement reads only the profile. Latent only because the four call sites hang
      off editors that are never rendered.
      **VERDICT PARTIAL — four, not six (independent verifier, 2026-08-21):** The divergence is real: the fork writes `AISettings` (`permissions.rs:1015-1076`) where the pin writes the profile (`42effe840:...:1002-1005`), and enforcement reads the profile (`:400-412`). Call sites are dead (`ai_page.rs:709,741` editors never rendered; `RemoveFromCommandExecution*` never dispatched). But it is **four** mutators, not six, and the lists ARE read once by `execution_profiles/mod.rs:473-479`.

      **FOUR, not six — and FIXED as a port catch-up, not declined.** The verifier's count is right. **Not covered by the workspace/team `DECLINED.md` row** — that is about `AiAutonomySettings` via `current_team()`, a different mechanism. **The ledger's framing was also incomplete:** this is not a fork divergence to be declined but code **inherited verbatim from the fork's base `0dbd3d567`**, which predates upstream's move of these lists into execution profiles — the pin already writes the default profile (`42effe840:permissions.rs:997-1050`). So it needed no `DECLINED.md` row at all. All four now write `default_profile_id()`; signatures unchanged, so the four dead call sites need no edit. Latency confirmed: both editors are built but only ever passed to `update_editor_interaction_state`, never rendered, and the two `RemoveFrom…` actions are declared and handled but never constructed. `AISettings.agent_mode_command_execution_*` is now read-only from this module, still read by `create_default_from_legacy_settings` as the one-shot TOML migration source. New test asserts on the **decision**, not the store.

- [x] **Tab-group header/contiguity assertions never read the rendered bar.**
      `integration_testing/tab_group/assertion.rs:47-65,159-197` reimplements
      `tab_bar_slots` from the model, so "exactly one group header is rendered" cannot
      catch a `tab_bar_slots` regression.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `integration_testing/tab_group/assertion.rs:12-27` reads `workspace.tabs`/`tab_groups` directly and `:47-67` re-implements run-collapsing; `:159-192` and `:194-200` consume only that. `Workspace::tab_bar_slots` (`workspace/view.rs:24740`) is never called by any assertion — verified by grep across the whole `integration_testing/` tree.

      **FIXED 2026-08-21 — claim confirmed.** `Workspace::tab_bar_slots` (`workspace/view.rs:24740-24771`) is private, so `integration_testing` could not reach it; the old `tab_group_ids` (`assertion.rs:12-29`) and `tab_bar_runs` (`:47-67`) were line-for-line copies of its filter and run-collapsing loop. Both assertions compared the model against a duplicate of the model→slots function and **would have passed with a tab bar that painted nothing**. Now `rendered_tab_bar_faults` (`:327`) reads the rects the app actually painted, via the same last-frame `SavePosition` cache the shipped drag hit-testing uses (`view.rs:24836`, `:26008`), checking that each group's container was painted, that every non-dragging member landed inside it, and that no foreign tab did. `assert_group_header_count` (`:486`) and `assert_groups_contiguous` (`:450`) now require the model check **and** an empty fault list. **Two honest limits, documented in-file rather than faked:** headers cannot be counted directly (both runs of a split group write the same key, so the cache holds one rect per group id) — the split is caught geometrically instead; and "not painted this frame" is unobservable because `cache_position_indefinitely` never evicts (`warpui_core/src/presenter.rs:172`), so every check is "the rect that exists is in the wrong place", never "no rect exists". Collapsed-group members and dragging tabs are therefore **skipped rather than trusted**. `assert_group_collapsed` (`:569`) remains model-only for the same reason and **was deliberately not given a fake check** — its limitation is documented on the function. Two ids had to be mirrored rather than called (`htab_group_position_id`/`vtab_group_position_id` are `pub(crate)` in a private module; `group_container_rect` is private), with the reason recorded at `:178-189` and `:210-217`; a drifted key fails loudly rather than silently. **Unverified:** no build or test run, and the `vertical_tabs:group:` branch is exercised by nothing since the group tests all run on the horizontal surface.

      **REFUTED THEN REPAIRED 2026-08-21 — and the repair corrected BOTH refuters.** The rendered-read fix was only partly real. **C-1:** the dragging-tab skip rested on a false premise — it claimed `Draggable` paints to an overlay so the cached rect is the floating chip's. It is not: `SavePosition` is the **outer** wrapper (`tab.rs:2145-2161`) and caches `RectF::new(origin, child.size())` *before* the child paints (`save_position.rs:50-69`); `Draggable` receives that same origin, stores it as `unmodified_origin`, and only *draws* elsewhere (`draggable.rs:547-586`); `start_overlay_layer` pushes no position-cache namespace (`scene.rs:483`). Corroborated three ways: production reads `tab_position_id(index)` during a live drag (`workspace/view.rs:24843-24845`), and the cross-window drag ghost deliberately **skips** `SavePosition` (`tab.rs:2127-2134`) precisely so it cannot clobber the real key. So the skip discarded a usable rect and blinded the check at the only instant the `on_tab_drag` defect is visible. Removed. **The repair found what neither refuter had, and it narrows the fix: groups nest the other way round.** A group's `Draggable` is wrapped by `SavePosition` (`vertical_tabs.rs:3170-3210`, `view.rs:18265`), so a dragging group's *container* rect is real — but each member tab's `SavePosition` sits **inside** that `Draggable`, so the overlay paint re-caches every member at the chip's origin. Member geometry is therefore the only thing a drag actually invalidates, and only during a *group* drag. So C-4's whole-group `continue` was replaced by `members_follow_the_cursor` (`assertion.rs:405-411`) — container lookup and foreign-tab check still run — plus a **converse guard the old code lacked** (`:469-473`): a tab belonging to a *different* mid-drag group is skipped, without which removing the tab-level skip would have produced false faults. **C-3** fixed differently than proposed: `tab_position_id` is *imported* from production (`:8`), not mirrored, so key drift is impossible; the real defect was that with every group collapsed nothing loud ran at all. New one-shot check (`:369-397`): if the model says N tabs are on screen and the bar resolved a rect for **none**, that is a fault — loud, no false positive from a single unpainted tab, and it runs in the all-collapsed case and when the model holds no groups. **C-2 downgraded to an honest documented gap, deliberately:** `PositionCache` exposes only keyed `get_position` (`presenter.rs:129-142`), `committed_positions` is private with no iterator, so enumeration needs a new `warpui_core` API **and** eviction — a closed group's rect never clears, so prefix-counting would report a phantom header. **The false claim was removed rather than faked**: `assert_group_header_count` now states plainly that at `expected == 0` the painted half sees nothing and cannot detect a header with no live group behind it. **C-5** likewise: `assert_group_collapsed`'s "still paints exactly one container" was untrue on both counts and now says what is actually checked. **Still open:** enumerating painted keys needs `crates/warpui_core` work.

- [x] **`ensure_grouped_tabs_enabled` is unfalsifiable.** `tab_group/step.rs:44-52` sets
      the user-preference tier, which `is_enabled` consults first, so the assertion
      holds even in a build without `grouped_tabs`.
      **VERDICT PARTIAL — impact wrong (independent verifier, 2026-08-21):** The tautology is confirmed (`warp_features/lib.rs:952-954` consults `USER_PREFERENCE_MAP` before `FLAG_STATES`, so the assert cannot fail). But the impact claim breaks: `grouped_tabs` is **not** a `cfg`-gate on any grouping code — its only uses are `lib.rs:3228` (the default-on flag list) and `settings_view/appearance_page.rs:1588`. The preference genuinely enables the paths, so the step works as setup.

      **FIXED 2026-08-21 — both the finding and its verdict were off.** The tautology is confirmed (`is_enabled` reads `overrides` then `USER_PREFERENCE_MAP` before `FLAG_STATES`, `warp_features/src/lib.rs:972-974`, and the action writes the user-preference tier). **The verdict's impact refutation conflates the *cargo* feature `grouped_tabs` with `FeatureFlag::GroupedTabs`** — the cargo feature does have only two uses, but the *flag* gates the grouping paths extensively (`workspace/view.rs:11753,:11807,:11841,:11872,:24799,:25010`), so the in-file doc was accurate and the action is load-bearing setup. **What neither found is the real check:** `settings/init.rs:301` samples the flag **before** any preference is written, specifically so the settings toggle "can turn tab groups off but never on in a build that did not have them" (`:427-429`) — and `set_user_preference(true)` walks straight past that guard, so a build compiled without the feature ran the whole suite against an unshipped configuration and reported PASS. Assertion is now `cfg!(feature = "grouped_tabs") && is_enabled()`; the `cfg!` half is genuinely falsifiable. The `is_enabled()` half is kept with its claim **narrowed in-file** to the two things it can still catch (`FORCE_DISABLED_FLAGS`, a thread-local override) and now says plainly it is **not** evidence grouping is live. Deliberately not `PreconditionFailed` — that path ends the test as a non-failure, i.e. the skip-reads-as-pass shape already on this ledger.

- [x] **TUI permission fixtures are vacuous.** `queue_tui_permission_action` →
      `queue_confirmation_action` force-installs the blocked state, so tests that claim
      to exercise "the real preprocess pipeline" wait on conditions already true. 38
      call sites across 9 files.
      **VERDICT PARTIAL — not vacuous (independent verifier, 2026-08-21):** `queue_confirmation_action` (`action_model.rs:997-1012`) does force the blocked state, and the "real preprocess pipeline" comments are stale. **But it is a faithful port** — `42effe840:tui_test_support.rs:299` calls the identical helper — and the tests are not vacuous: events reach the view only through the effect loop and the assertions check real rendering (`:60-79`).

      **REFUTED 2026-08-21 — not vacuous; the COMMENTS were.** Faithful port: `42effe840:tui_test_support.rs:299` calls the identical helper, and the pin has the method at `action_model.rs:964`. The tests assert real rendering and focus behaviour reachable only through the effect loop (`tui_permission_prompt_tests.rs:162-183` focus delegation, `:186-211` a background prompt must not steal foreground focus, `:213-250` `dispatch_focused_key` after `render_lines`); `settle_until(… is_active …)` waits for an emitted event to reach the view, which is real work. **They are NOT part of the `#[ignore]`d permission group** — that is `ai/blocklist/permissions_test.rs` (team AI-autonomy overrides, 11 sites); `crates/warp_tui/` contains **zero** `#[ignore]`. Nothing deleted. **What was actually wrong is documentation:** `tui_test_support.rs:79-82` claimed the fork has no `queue_confirmation_action` and that the helper falls back to `queue_action_for_test` — **both stale** since the port at `action_model.rs:1044-1053`. Rewritten to state that the helper *installs* the blocked state and is therefore **not** evidence the real preprocess pipeline would produce it.

- [x] **`completions.rs:51-59` is factually false** — it claims
      `load_all_function_names` / `additional_function_names` "do not exist in this
      fork's Session". All exist; DECLINED.md:158 says so. The pin calls both loaders
      and the fork does not — a real TUI gap hidden behind a false excuse.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** All five named items exist and are public (`session.rs:1181,1206,1232,922,924`). Verifier went further: the pin warms BOTH loaders (`42effe840:.../completions.rs:47-50`) and the fork's GUI does too (`terminal/view.rs:12767`) — so the TUI-only gap is real and not a declined decision.
      **FIXED 2026-08-21:** Comment corrected: all five symbols exist and are public, the GUI warms them at `view.rs:12766-12773`, and the TUI's omission is now stated as a genuine open gap instead of being justified falsely. The missing calls were deliberately NOT added — comment only.

### Refutation round — second wave

- [x] 🔴 **Codex is launched with `--dangerously-bypass-hook-trust` and the fork
      deleted the only gate the pin paired with it.**
      `ai/agent_sdk/driver/harness/codex.rs:92,206,211` always passes the flag. The
      pin's identical line is guarded by `requires_verified_platform_plugin()` →
      `setup_platform_plugin`, which hard-fails the run when the plugin is missing
      (`42effe840:.../codex.rs:96`, `driver.rs:3224`). The fork has neither (removed per
      DECLINED.md:191), and the pin's doc sentence "Driver setup verifies the Codex
      platform plugin before launching commands with this flag" was silently dropped
      from the fork's copy of the comment. **DECLINED.md:191 argues the removal is "not
      a regression" and never addresses the bypass flag, whose own text says it
      "bypasses hook trust globally".** Every driver-launched Codex run executes
      repo-declared hooks with trust review disabled.
      **VERDICT PARTIAL — impact misattributed (independent verifier, 2026-08-21):** The fork does pass the flag unconditionally (`codex.rs:92,206,211`) and lacks `requires_verified_platform_plugin`. **But the pin passes it just as unconditionally** (`42effe840:.../codex.rs:192,197`) — its gate required plugin INSTALLATION and never conditioned the flag. So the hook-trust bypass is identical at the pin and "deleted the only gate" misattributes the impact. The dropped doc sentence (pin `:184-185`) is confirmed.

      **RESOLVED 2026-08-21 — DECLINED, and the flag's semantics were established from Codex's docs rather than its name.** Hook trust is **per-hash** and the flag is **invocation-scoped, not plugin-scoped** — both fork comments claimed otherwise. **This entry's supporting quote was circular:** "whose own text says it bypasses hook trust globally" quoted the *fork's* own comment at `codex.rs:44`, not Codex's documentation. **The impact was misattributed AND understated:** the pin passes the flag identically (`42effe840:codex.rs:192,197`) and its declined gate checked plugin *installation*, never the flag — while the real enabler of repo-hook execution is `prepare_codex_config_toml` writing `trust_level = "trusted"` for the working dir *and every child git repo* (`codex.rs:648-655`), which the pin also does. **Not removable:** `setup_harness` installs/updates `warp@codex-warp` right before launch, so its hooks are unreviewed-by-hash on every driver run, and they emit the `SessionStart` that captures the session id; removing the flag skips them and `resume` loses its input. **Marginal exposure ≈ nil** beside `--dangerously-bypass-approvals-and-sandbox` on the same line. Rationale written into `codex.rs:91-121`, and the pin's dropped "driver setup verifies the platform plugin" sentence is now explicitly accounted for at `:228-234` rather than silently absent. New `DECLINED.md` row.

- [x] 🔴 **MCP "Log out" silently keeps OAuth tokens.**
      `ai/mcp/templatable_manager/oauth.rs:600-612` handles only `get_template_uuid`,
      which reads `locally_installed_servers` — file-based installs are never in it, so
      it logs an error and deletes nothing. The pin has the
      `FileBasedMCPManager::get_hash_by_uuid` branch, which the fork's own
      `save_credentials_to_secure_storage` still mirrors. `can_log_out` DOES check the
      hash map, so the TUI offers a Log Out row that is a no-op. Refresh tokens survive
      logout and revocation.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `oauth.rs:600-614` handles only `get_template_uuid`, which reads `locally_installed_servers`; the pin's file-based branch (`42effe840:.../native.rs:214-225`) is absent while `save_credentials_to_secure_storage` still mirrors it. Verifier closed the caller chain: `tui/mcp.rs:383-395` resolves `FileBased(hash)`→uuid then calls delete, and `can_log_out` returns true. The pin's `CredentialsChanged` emit is also gone.

      **FIXED 2026-08-21:** `oauth.rs:611-628` ports the pin's file-based branch. **`CredentialsChanged` deliberately not ported** — no such event exists anywhere in this fork; documented at `:600`.

- [x] 🔴 **`claude_code_tests` can write to the REAL user profile on Windows.**
      `claude_code.rs:509-528` duplicates `claude_config_dir` using `dirs::home_dir()`
      instead of the pin's `home_dir_for_claude_config`. `dirs::home_dir()` ignores the
      `HOME` the tests set on Windows, so the suite writes `.claude.json` /
      `settings.json` containing `hasTrustDialogAccepted` and
      `skipDangerousModePermissionPrompt: true` into the developer's actual profile.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `claude_code.rs:508-518` and `:521-528` both use `dirs::home_dir()` where the pin uses `home_dir_for_claude_config()`, whose `#[cfg(test)]` HOME check exists precisely for this (`claude_transcript.rs:77-85`). Verifier established the reachable path: `claude_code_tests.rs:719-733` removes `CLAUDE_CONFIG_DIR`, sets only `HOME`, then calls `prepare_claude_environment_config`, writing `has_trust_dialog_accepted` and `skip_dangerous_mode_permission_prompt`.

      **FIXED 2026-08-21:** `claude_code.rs:28` imports `claude_config_dir`/`home_dir_for_claude_config` from `claude_transcript`; the duplicate local `claude_config_dir` is deleted and `:525` uses the helper.

- [x] 🔴 **`move_tab` has no pinned/group boundary check — the pin's `can_move_tab` was
      never ported.** `workspace/view.rs:13234-13242` is a bare `swap` guarded only by
      list bounds; menu gating (`tab.rs:557,569`) is index-only. The fork's DRAG path
      guards exactly this (`view.rs:25433-25440`), so the same move is blocked by mouse
      and allowed by menu or keybinding. "Move tab left" on the first unpinned tab
      evicts a pinned tab and breaks the contiguous pinned prefix that
      `pinned_boundary_index` assumes; it also splits a group, producing the duplicate
      group header this fork already documents. The group-level analogue
      `can_move_tab_group` WAS ported and tested, so this is an omission, not a
      decision.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** The pin has `can_move_tab` (`42effe840:workspace/view.rs:14150`), called at `:14191` inside `move_tab` and at `:7892,:7979` for menu gating. The fork's `move_tab` (`workspace/view.rs:13234-13242`) is a bare bounds-only `swap`, and menu gating (`tab.rs:557,569`) is index-only while the drag guard exists at `view.rs:25436-25441`. Nothing in DECLINED.md. Verifier confirmed the consequence: `pinned_boundary_index` (`tab_grouping.rs:550-554`) is a `take_while`, so the prefix really does shrink.

      **FIXED 2026-08-21.** Confirmed an omission, not a decision: `can_move_tab` did not exist in the fork (`grep -c` on HEAD → 0) while the group-level `can_move_tab_group` **was** ported, wired at three sites and tested. **The pin's `move_tab` does more than gate, which `can_move_tab` alone would not have caught:** at `42effe840:view.rs:14191` it also **hops a neighbouring group as a whole unit** rather than swapping into its run — without that, an ungrouped tab could still land inside another group's contiguous run, producing the duplicate group header this fork already documents in the drag path. Both were ported. **Drag path reconciled with one shared predicate** (`tabs_share_pinned_region`), called by `can_move_tab` and by `update_tab_index_from_drag`, so the mouse and the menu can no longer disagree. The group policy legitimately still differs and was deliberately **not** merged: a drag *reassigns* group membership, whereas a keyboard/menu move never does, so for it crossing a group edge is always a split — that asymmetry is the pin's too. **One documented divergence:** the pin's `can_move_tab` is asymmetric (a pinned tab may step left past an unpinned one); the shared predicate uses strict equality like the drag path. They differ only when the pinned prefix is *already* non-contiguous — i.e. the invariant `pinned_boundary_index` assumes is already broken — and refusing there is what the drag path has always done. **The adjacent underflow disappeared rather than being guarded:** `tabs_len - 1` is gone from `move_tab` entirely, since `can_move_tab` returns false for any index not in `self.tabs`. Group clauses are gated on `FeatureFlag::GroupedTabs` so a stale persisted `group_id` cannot start refusing moves when grouping is off. **Note:** `TabData::menu_items` (the 5-arg wrapper) became `#[cfg(test)]` — its only production caller moved to the gated entry point and its four remaining callers are in `view_test.rs`, outside the edit list; collapsing the two entry points is the tidier end state once that file can be touched.

      **REFUTED THEN REPAIRED 2026-08-21 — the divergence recorded in `4a2c0d14c` is WITHDRAWN.** That commit diverged from the pin by using strict equality instead of the pin's asymmetric rule, on the argument that the two differ only when the pinned prefix is already broken. **The refutation turned that argument around:** the only way to reach that state is a workspace saved by a **pre-fix build of this fork** — restore preserves order and `pinned`, and every other mutator clamps — i.e. exactly the users the fix is for. They upgrade and the tab the old bug displaced becomes **permanently unmovable** by menu, keybinding, drag and local control. Worse, the fork **already contained** the asymmetric rule in `can_move_tab_group` (`view.rs:11963`, ported faithfully and wired at `:8899`), so a pinned **group** could dig itself out of a broken prefix while a pinned **tab** could not — two contradictory policies, in a commit claiming "one spelling". Repaired by adopting the pin's rule and **extracting it**: new `pinned_step_allowed(mover_pinned, neighbor_pinned, direction)` (`view.rs:13300-13345`) is now called by both `can_move_tab` and `can_move_tab_group`, so the claim is literally true. Option (b) — strict everywhere plus a one-shot restore normalisation — was rejected because it would reorder a user's saved tabs unasked and still freeze the mid-session case. `tabs_share_pinned_region` survives as the **drag path's** rule only, with its doc no longer claiming to be the single policy and now stating why drag is strict: a drag names an arbitrary destination index, so "toward the region I belong in" is undefined for it — **which is exactly where the pin draws the line too** (`42effe840:view.rs:28726` strict, `:14168`/`:7580` asymmetric). New `test_a_pinned_tab_stranded_behind_an_unpinned_one_can_walk_back` fails under strict equality. **The `GroupedTabs` gate was dropped at both sites** — it defended a state unreachable at rest while creating a real one (a mid-session flag flip turned `move_tab` into a plain swap that shreds a group's run, the very symptom the commit exists to prevent); gating only the predicate would have half-fixed it. **The rendered menu is now tested**: `test_tab_context_menu_omits_a_move_entry_the_action_would_refuse` drives the production `toggle_tab_right_click_menu` and reads the actions off the menu rather than re-asserting the predicate — bounds-only gating offers `MoveTabRight(0)` there, so it fails pre-fix.

- [x] **Warpified `ssh` fails outright when the user's ssh config sets
      `RemoteCommand`.** All three fork bootstrap copies dropped the pin's
      `ssh -G … remotecommand` probe and its plain-ssh fallback
      (`bash_body.sh:1035`, `zsh_body.sh`, `fish.sh:658`). OpenSSH aborts with "Cannot
      execute command-line and remote command" — the connection fails rather than
      degrades.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** The pin probes `remotecommand` and falls back to plain ssh (`42effe840:bash_body.sh:1013-1022`, `zsh_body.sh:967-976`, `fish.sh:641-650`); the fork has ZERO `remotecommand` matches repo-wide and still passes a command-line remote command. Verifier confirmed the consequence empirically on OpenSSH 10.2: it prints "Cannot execute command-line and remote command." Not in DECLINED.md.
      **FIXED 2026-08-21:** The pin's `ssh -G … remotecommand` probe and plain-ssh fallback ported into all three scripts (`bash_body.sh:1035-1045`, `zsh_body.sh:932-942`, `fish.sh:660-670`), at the pin's placement. `bash -n` / `zsh -n` / `fish --no-execute` all pass.

- [x] **Fish's DCS terminator is the wrong bytes.** `fish.sh:27` sets `\u9c`, which
      emits **c2 9c**; every other emitter and the Rust constant use the single byte
      `9c`. A stray `0xC2` makes `hex::decode` fail and the hook is dropped. The pin
      avoided this by using `ESC \` in all four shells.
      **VERDICT PARTIAL — consequence refuted (independent verifier, 2026-08-21):** Byte claim confirmed BY OBSERVATION (fish 4.2.1: `\u9c` → `c2 9c`; `\x9c` → `9c`), against `printf '\x9c'` elsewhere. **But the consequence is refuted:** in vte's `DcsPassthrough` table (`table.rs:146-153`) 0xc2 has no entry and neither does `Anywhere`, so it resolves to `Action::None` — the byte is DISCARDED, never `put`, and `hex::decode` at `ansi/mod.rs:907` sees a clean payload. Cosmetic inconsistency, not a dropped hook.

      **FIXED 2026-08-21 as latent fragility — but the CONSEQUENCE claim is refuted, by execution.** fish 4.2.1 is installed here: `\u9c` emits `c2 9c`, `\x9c` emits `9c` — byte claim confirmed. **"A stray `0xC2` makes `hex::decode` fail and the hook is dropped" is FALSE:** the pinned vte revision gives `DcsPassthrough` no entry for `0xc2` and `Anywhere` none either, so `advance()` unpacks `(Anywhere, Action::None)` and the byte is discarded **before** `put` — `hex::decode` sees a clean payload. Two more stale claims: there is **no non-test Rust constant** using `0x9c` (only three test files), and "the pin avoided this by using `ESC \` in all four shells" is true of the pin but is **not the fork's convention** — the fork deliberately uses bare `0x9c` in `bash_body.sh:16`, `zsh_body.sh:25`, all six `*_init_*.sh` and twice more inside `fish.sh`. Fixed for consistency and because it survives only by vte happening to drop an out-of-table byte. **`app/src/terminal/table.rs` does not exist** — the state table is in the vendored `vte` git dep.

- [x] **Bash sessions permanently keep the giant HISTSIZE sentinel.**
      `local_tty/unix.rs:431-436,922-926` exports `HISTSIZE=57265949261`;
      `bash_body.sh:1238-1241` unsets only the HISTFILESIZE pair, where the pin unsets
      both. Every bash session runs with unbounded in-memory history and leaks
      `HISTSIZE` + `WARP_INITIAL_HISTSIZE` into every child process. `unix_tests.rs:42-99`
      asserts the sentinels are set, never that they are removed.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `local_tty/unix.rs:432,436` and `:924,926` export both sentinels; `bash_body.sh:1238-1241` unsets only the HISTFILESIZE pair while the pin also unsets `HISTSIZE`/`WARP_INITIAL_HISTSIZE` (`42effe840:bash_body.sh:1230-1233`). Verifier checked the whole fork tree: `HISTSIZE` is never unset anywhere, and inherited-from-environment variables stay exported in bash, so children do inherit. Not in DECLINED.md.
      **FIXED 2026-08-21:** `bash_body.sh:1252-1266` now unsets `HISTSIZE`/`WARP_INITIAL_HISTSIZE` alongside the HISTFILESIZE pair, matching the pin. `local_tty/unix.rs` needed no change — its exports already match.

- [x] **CDPATH completion is dead code, and #483 plus TODO.md:6498 both say "done".**
      `completer/mod.rs:289` has no `cdpath()`, so the trait default returns `None`
      forever and `engine/path.rs:154` always early-returns. No bootstrap script emits
      it, `dcs_hooks.rs` never parses it, `session.rs` has no field — the pin has the
      whole chain. The 10 `with_cdpath` tests inject the value into a fake context and
      pass with the feature entirely unwired.
      **VERIFIED 2026-08-20, but by a SINGLE reader — NOT independently confirmed.**
      Everything above checks out against the tree; treat it as one verification, not two.
      What I read: `app/src/completer/mod.rs:289` is the only `impl PathCompletionContext
      for SessionContext` in `app/`, and it defines no `cdpath()`, so the trait default at
      `crates/warp_completer/src/completer/context/mod.rs:167-170` (`fn cdpath(&self) ->
      Option<&str> { None }`) is what production uses. `engine/path.rs:154`
      (`let Some(cdpath) = ctx.cdpath() else { return sorted_directories_relative_to(...) }`)
      therefore always takes the early return. `CDPATH`/`cdpath` appears in **no** `.sh`,
      `.ps1` or `.fish` asset in this repo and in **no** field of
      `app/src/terminal/model/session.rs`. The only other implementor is
      `MockPathCompletionContext` (`crates/warp_completer/src/completer/testing/mod.rs:220`),
      which is what the `with_cdpath` tests inject into — so they pass with the feature
      entirely unwired, exactly as the entry says.
      **Still open** and needs a second reader before #483 is reopened.
      **FIXED 2026-08-21:** TODO/#483 entry corrected. Marked explicitly as single-reader verification rather than independent, since this finding was one of the two the verifier fleet skipped.

- [x] **Repo detach leaves `GitBranchStatus` stale.** `current_prompt.rs:1511-1517`
      clears only `GitDiffStats`; the pin loops over both. `is_updated_externally` gates
      three chips on the watcher, so on detach the branch-status chip keeps the last
      structured value with no source to correct it.
      **VERDICT PARTIAL — stale window only (independent verifier, 2026-08-21):** The divergence is real (`current_prompt.rs:1511-1519` clears only `GitDiffStats`; the pin loops over both). But the consequence breaks: `self.git_repo_status.take()` at `:1505` runs FIRST, so `is_updated_externally` (`:1697-1703`) returns false and the shell fallback `builtins::shell_git_branch_status` resumes as the source. A stale window, not a permanently stuck chip.

      **FIXED 2026-08-21.** `apply_git_repo_metadata` writes **both** `GitDiffStats` and `GitBranchStatus`, but the detach branch cleared only the first; the pin loops over both (`42effe840:...:1477-1490`). The PARTIAL verdict is right that it is a stale *window* rather than permanent — `git_repo_status.take()` runs first, so the 30s timer eventually corrects it. **Different mechanism from `5dbf396ca`:** that wired invalidation *events* deciding whether to hold a subscription at all; this is cached chip state not invalidated when the subscription is dropped. Same family, different site, no overlap. `ShellGitBranch` deliberately excluded, matching the pin — its source is the shell fallback, which needs no repo model.

- [x] **Chip-change detection compares rendered TEXT, not values.**
      `context_chips/display.rs:174-180` uses `chip.text() != value.to_string()`; the
      pin compares values. `git push -u` on a 0/0 branch yields an identical string, so
      the chip is never rebuilt and its tooltip still reads "No upstream configured".
      Same file `:185` compares only the FIRST on-click value, so the branch-switcher
      dropdown goes stale.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `display.rs:174-186` compares `chip.text()` vs `value.to_string()` where the pin compares `chip.value()` and the full on-click slice. Verifier confirmed the value→text collapse: `display_chip.rs:526-530` yields the bare branch name for both `upstream=None` and `upstream=Some, ahead=0, behind=0`, while `tooltip_text` (`:586-606`) differs — and the tooltip is captured only at rebuild, so it stays stale.
      **FIXED 2026-08-21:** The fork's `DisplayChip` keeps only `text` + `first_on_click_value`, so rather than the pin's `chip.value()` the agent added `PromptDisplay::last_chip_results` (`display.rs:60-71`, populated in `reset_chips`); `check_if_chip_values_have_changed` (`:180-190`) now compares `value()`, `kind()` and the FULL `on_click_values()` slice — the pin's semantics via different state. All four `reset_chips` sites feed the cache so it cannot desync.

- [x] **Tab-completion no longer aborts the in-flight input-detection future.**
      TUI `completions.rs:110-125` dropped the pin's `abort_input_detection`. A
      classifier from the previous keystroke lands after Tab and flips input mode,
      closing the popup the user just opened. The same hunk also dropped the
      MCP-install guard, so natural-language detection now runs while the user types
      MCP install answers.
      **VERDICT PARTIAL — MCP half refuted (independent verifier, 2026-08-21):** Abort half confirmed with the chain the original omitted: the pin's `abort_input_detection` is gone, so a landing classifier hits `ai_input_model`, whose subscription (`terminal_session_view.rs:2155`) calls `handle_completion_editor_changed` → `abort_shell_completion`, killing the pending Tab request. **MCP half REFUTED:** that drop is in `input_detection.rs:112`, not this hunk, and is subsumed by the `inline_menu_owns_input` return at `:102-113`.

      **FIXED 2026-08-21 — and it is NOT a harmless leak.** The orphaned future completes **and applies a stale result**: `parsed_result_is_applicable` compares the pre-Tab buffer against the current one, but **Tab does not change the buffer** and the popup is not open yet, so the guard passes. It then classifies on the *previous* keystroke, which emits, which calls `handle_completion_editor_changed`, which finds no open menu and **aborts the newer Tab request**. So the stale classifier both flips shell/agent input mode on stale input *and* cancels the request the user just made, so the popup never appears. One line, at the pin's exact position. Ledger path stale: the file is `crates/warp_tui/src/terminal_session_view/completions.rs`.

- [x] **The TUI prints a command for a binary that does not exist.**
      `warp_tui/src/session.rs:261` prints `warp --resume {token}`; the bin is
      `zap-tui-oss`. `#[command(name = "warp")]` also puts "warp" in `--help`, which the
      branding rule forbids. README:230 and README:141 contradict each other about
      whether that binary is user-facing.
      **VERDICT PARTIAL — the println is dead (independent verifier, 2026-08-21):** `session.rs:261` and `#[command(name = "warp")]` (`:46`) are real, and the clap name IS live (`:135` `try_parse`), inherited verbatim from `42effe840:session.rs:51`. **But the println is unreachable:** it needs `exit_summary.token()`, set only from `server_conversation_token()` (`terminal_session_view.rs:2836-2841`), which BYOP never produces — DECLINED.md:216 documents this. README:141 and :230 both name `zap-tui-oss`; no contradiction.

      **FIXED 2026-08-21 — and BOTH the ledger and its verifier named the wrong binary.** Both say `zap-tui-oss`; that is the **cargo bin name only**. Every release job builds `--bin zap-tui-oss` and then **copies it to `phosphor-tui`** before packaging (`phosphor_release.yml:426,824,904`), and `README.md:202` calls it "the `phosphor-tui` binary". So the correct instruction is `phosphor-tui --resume <token>` — neither `warp` nor `zap-tui-oss`. The clap name is genuinely live (it feeds `--help` usage and every parse error), so this was user-visible beyond the unreachable `println!`. Now a single `CLI_NAME` const feeds both, so they cannot drift. The verifier's "println is unreachable" holds — the token's only production setters are cloud multi-agent paths — and the block is kept with a comment saying so, since `--resume` is a live flag.

- [x] **`LaunchMode::Tui` folds into the App arm, so the TUI inherits the GUI's default
      profile object.** `execution_profiles/profiles.rs:175`. The pin makes that arm
      `unreachable!("TUI profiles use settings")`.
      **VERDICT PARTIAL — no new defect (independent verifier, 2026-08-21):** `profiles.rs:175` does fold `Tui` into the App arm and `42effe840:profiles.rs:380` is `unreachable!`. But that arm is unreachable AT THE PIN only because `42effe840:profiles.rs:71-73,262-277` routes the TUI through file-backed settings — a subsystem this fork never ported and already tracks (`TODO.md:3781-3784`). Sharing the GUI object store is the fork's stated design (`bin/oss.rs:18-24`).

      **REFUTED 2026-08-21 — not a defect; porting the pin's arm would PANIC on every TUI launch.** The pin does have `LaunchMode::Tui` (`42effe840:profiles.rs:380`) but it is `unreachable!("TUI profiles use settings")`, reachable-as-unreachable only because the pin diverts the TUI into file-backed `[agents.execution_profiles.*]` settings **before** the match — and **that subsystem is DECLINED in this fork** (`TODO.md:3789-3791`, "redesign not port; ~700 lines of cloud-migration machinery"). So copying the arm would crash. The inherited App-arm default is *correct* for the TUI, which deliberately runs under the GUI's app identity and config (`DECLINED.md:173`), and it must **not** fall into the `CommandLine` arm, which is intentionally more permissive. The ledger presents "the pin makes that arm `unreachable!`" as the fix; it is not one. Comment-only change recording this so it stops being re-filed.

- [x] **Unreachable branch from a mis-desugared let-chain.**
      `current_prompt.rs:822-832`: `if suppress_on_failure { … } else if
      suppress_on_failure { … }`. The pin has one arm.
      **VERDICT CONFIRMED — dead code only (independent verifier, 2026-08-21):** `current_prompt.rs:822` and `:826` are both `if suppress_on_failure`, so the second arm is unreachable; the pin has one arm as a let-chain (`42effe840:...:791-795`). Verifier went further than the original: behaviour still MATCHES the pin, because `timed_out` implies failure. Dead code, no functional divergence.
      **FIXED 2026-08-21:** `current_prompt.rs:821-825` collapsed to the pin's single let-chain arm.

- [x] **Unquoted rcfile paths in the bootstrap.** `bash_body.sh:1220-1226` and four
      `${ZDOTDIR:-$HOME}` sites in `zsh_body.sh`; the pin quotes all of them. A `$HOME`
      containing a space sources the wrong file or nothing. Also: `bootstrap.rs` lacks
      the pin's `is_container_subshell` guard, so docker/podman subshells get a
      host-side temp RC file the container cannot read.
      **VERDICT PARTIAL — zsh half and container half both narrower (independent verifier, 2026-08-21):** Bash half confirmed and empirically checked (`bash_body.sh:1220-1226` unquoted vs pin `:1207-1213` quoted; bash splits, `argc=2`, so `source` fails). **Zsh half has no consequence** — zsh does not word-split unquoted expansions (verified, `argc=1`), so those four sites are cosmetic parity. The container guard IS absent (`bootstrap.rs:56-88`), but without it RC-file bootstrap triggers only for fish/pwsh/Windows-zsh subshells, not docker/podman generally.

      **FIXED 2026-08-21 — and the ledger's severity claim is WRONG in an instructive direction.** "or worse, executes" is **false**: bash does not re-run command substitution on a parameter-expansion result, so a `$(...)` inside `$HOME` is inert — verified by executing it, `PWNED` was never created. **The real risk is sharper than the one claimed:** with `HOME` containing a glob, the unquoted `source $HOME/.bash_profile` **silently sources a different directory's rcfile and exits 0**, so nothing logs. With a space it exits 1 and skips the user's rcfile. Executed in real `bash 5` and `zsh 5.9`. **The zsh half's "cosmetic" verdict is refined, not accepted:** the equivalence is option-dependent, not structural — under `SH_WORD_SPLIT`, settable from `/etc/zshenv` which is sourced before this and cannot be opted out of, the four zsh sites break exactly like bash's (exit 127, user rcfile skipped). All seven sites quoted (bash line numbers were `:1231-1237`, not `:1220-1226`), plus the container-subshell guard ported with its claim narrowed: it is **not** docker/podman generally — a plain `docker run -it <img> bash` never selected the RC-file method — but is reachable via the Fish/PowerShell disjuncts and the Windows-zsh-subshell disjunct.

- [x] **`active_window_index` indexes the wrong vec** (`app_state.rs:362-393` computed
      unfiltered, consumed as an index into the filtered list). **Pin-identical** —
      upstream bug, not fork drift. Record before "fixing".
      **VERDICT CONFIRMED — pin-identical (independent verifier, 2026-08-21):** `app_state.rs:396-400` enumerates ALL `window_ids()` while `:407,:419` skip entries from `windows`; consumers index the filtered vec (`persistence/sqlite.rs:1232`, `root_view.rs:665`). Verifier diffed `get_app_state` against the pin: byte-for-byte identical including the drag-preview skip. Upstream bug — do not "fix" as fork drift.

      **FIXED 2026-08-21 — deliberate divergence ahead of the oracle.** Confirmed pin-identical (`42effe840:app/src/app_state.rs:353-395`, comment included): the index is assigned inside `for (index, window_id) in app.window_ids().enumerate()`, **above** three filters (no workspace / `is_tab_drag_preview()` / empty `tabs`). Every consumer indexes the *filtered* vec — `root_view.rs:665`, `persistence/sqlite.rs:1528`, `launch_configs/launch_config.rs:26`. **The ambiguity resolves cleanly:** the unfiltered list is never serialised, so the filtered reading is the only one a persisted `AppState` can express; that argument is in the code. New private `collect_windows_with_active_index` (`app_state.rs:387-446`) counts at push time, and yields `None` rather than a neighbour's index when the active window is itself filtered out. The restore-side counterpart (`sqlite.rs:3589-3662`) was checked and is a 1:1 map, so it does not share the defect. Five platform-independent tests (`app_state_tests.rs:108-175`) including the distinguishing case (leading window filtered → index must be 0, not 1) and the nothing-filtered control that used to pass either way. **Wants a `DECLINED.md` divergence row** with markers `sym:collect_windows_with_active_index` and `test:test_active_window_index_counts_only_persisted_windows`.

      **REFUTED THEN EXTENDED 2026-08-21 — the fix was incomplete at one consumer, and the doc I committed said otherwise.** `LaunchConfig::from_snapshot` applies a **second, narrower** filter (drops `quake_mode`) and copied the index verbatim, so the same off-by-one survived one layer down; its consumer (`root_view.rs:452-470`) indexes the narrower vec. Pin-identical (`42effe840:launch_configs/launch_config.rs:22-33`), so this is a **second** deliberate divergence. Fixed by sharing `collect_windows_with_active_index` (now `pub(crate)`) so one tested helper is the single implementation for **both** producers, rather than two hand-rolled loops. **Two more facts in that same doc block were false:** there is no `restore_windows` anywhere in the tree (the consumer is `open_from_restored`, `root_view.rs:589`), and "only `get_app_state` ever produced it" is wrong — `read_sqlite_data` (`sqlite.rs:3556`, assigns `:3662`, returns `:4130`) is a second producer, consistent only by luck of enumerating the same vec. **Known gap, now recorded in-code rather than implied:** `get_app_state` itself is untested **and untestable in a unit test** — `App::test` installs a `WindowManager` whose `active_window_id()` is hardcoded `None` (`warpui_core/src/platform/test/delegate.rs:100-102`), so such a test would assert `None == None` and be vacuous **on the exact property under test**. Re-inlining the old unfiltered `enumerate()` at the call site would leave every test in the crate green. Closing it needs an active-window-tracking test `WindowManager` or a `crates/integration` test.

### Refutation round — third wave (security-weighted)

- [x] 🔴 **The remote-server SHA-256 integrity check is bypassed by the SCP fallback,
      and the tamper signal itself triggers the bypass.**
      `remote_server/ssh_transport.rs:195` — `should_skip_scp_fallback` returns true only
      for exit code 2, but `install_remote_server.sh` exits **4** (no pinned digest),
      **5** (no sha tool) and **6** (digest mismatch). All three fall through to
      `scp_install_fallback` (`:569`), which curls the same GitHub release tarball
      locally and installs it through the staging branch the script itself documents as
      needing "no verification … it is a locally cross-compiled dev binary". It is not —
      it is the published release, fetched off the network, unverified. The local curl
      also omits the `--proto '=https' --proto-redir '=https'` the remote path sets
      deliberately, so an HTTP downgrade on redirect is accepted.
      **Detected tampering escalates to an unverified install of a binary that then runs
      on the remote host.** `setup_tests.rs:778` asserts the fail-closed exit 4 — the
      Rust caller undoes it, and `should_skip_scp_fallback` has zero tests.
      **This contradicts the closed maintainer decision at TODO.md:2866-2881**, which
      closed the supply-chain objection on "an empty digest is fail-closed", "verified in
      code before closing".
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Exit codes enumerated: 2 (arch/OS), 3 (no fetcher), 4 (no digest, `:101`), 5 (no sha tool, `:117`), 6 (MISMATCH, `:124`). `should_skip_scp_fallback` filters only 2 (`ssh_transport.rs:195-197`), so 4/5/6 all reach `scp_install_fallback` (`:750-753`). Verifier also confirmed the local curl omits `--proto` (`:536-544`) and the staged branch skips verification by design. **TODO.md:2866-2881 rests on exit 4 being fail-closed — genuinely contradicted.**
      **FIXED 2026-08-21:** `should_skip_scp_fallback` now skips the fallback for exit 2 AND 4/5/6 (missing digest, missing sha tool, digest MISMATCH). Exit 3 — no curl/wget on the host — still falls through, since that is the case the fallback exists for. The local curl gained `--proto '=https' --proto-redir '=https'`. Four tests added to the existing module. **This makes TODO.md:2866-2881's closed decision true** — it was closed on "an empty digest is fail-closed", which was true in the script and undone by the caller. **Still open:** exit 3's fallback remains unverified; `expected_sha256()` is private in `setup.rs`.

      **REFUTATION 2026-08-21 — FIX DEFEATED AND PARTLY A REGRESSION, REOPENED.** `install_remote_server.sh:24` `set -e` plus curl as an `if` body (`:106`) means the script exits with *curl's* status, which is then read as a digest verdict. curl 6 (DNS failure) now reads as "digest mismatch" → **hard fail where the SCP fallback used to work (new regression)**; curl 7 (connection refused / attacker RST) and 22 (HTTP 5xx) are unfiltered → **unverified staging install proceeds**. The `exit 1` at `:140` falls through the same way, and `non_script_failures_still_use_scp_fallback` enshrines it. Tests pin only the predicate, never the call site (`:792`). (`--proto` syntax was fine.) Repair dispatched.

      **REPAIRED 2026-08-21 (refutation upheld; its mechanism corrected).** The refuter's conclusion was right but its reason was wrong, and the correction matters for the next reader: `set -e` exempts the *condition* of an `if`, **not** the then-branch — the then-branch aborts with the failing command's own status (verified empirically: `set -e; if true; then (exit 22); fi` exits 22). New explicit exit-code contract at `install_remote_server.sh:26-65`: 2 unsupported / 3 no-fetcher / 4 no-pinned-digest / 5 no-digest-tool / 6 **digest mismatch** / 7 **download failed (new)** / 8 bad tarball / 9 unclassified backstop. An `ERR` trap (`:71-76`) remaps any unguarded `set -e` abort to 9 so no foreign tool's status can masquerade as a verdict; fetcher status is captured with `|| fetch_status=$?` (`:161-179`). **Unknown codes now default to Fatal, not fallback** — "couldn't check" must never collapse into "check passed". Call site rewritten: `InstallFailureKind`/`classify_install_failure`/`route_install_failure` (`app/src/remote_server/ssh_transport.rs:211,259,279,287`) with the two-arm dispatch at `:866` holding no policy of its own. `non_script_failures_still_use_scp_fallback` was **split, not deleted**: its `InstallError::Other` half is a real requirement (SSH died, no verdict was produced) and survives as `ssh_level_failures_still_use_scp_fallback:1158`; its `script_failure(1)` half encoded the defect and is inverted into `unrecognised_script_exit_codes_fail_closed:1177`. All eight paths driven for real against the actual script with a curl shim. **Residual, still open:** the SCP fallback is itself an unverified install — `download_remote_server_tarball` (`ssh_transport.rs:570-610`) does no digest check, so an attacker who can reliably break the host's connection to GitHub can still force an unverified install via exit 7 or 3; they simply can no longer do it by serving bad bytes. Closing that needs `expected_sha256()` (`crates/remote_server/src/setup.rs:529`, currently private) exposed to the app crate.

      **REFUTED THEN REPAIRED AGAIN 2026-08-21 — this entry's earlier text is now STALE on two counts** (the `{2,4,5,6}` skip-list and the code-2 numbering both changed). The refutation found the contract itself could mint a false verdict: `compute_sha256` ended every branch in a pipeline and the script never set `pipefail`, so a digest tool that **exists but fails** (unreadable file, SELinux denial, OOM, FIPS-restricted openssl) returned the last pipeline element's status — 0 — with empty stdout, the `EXIT_NO_DIGEST_TOOL` guard never fired, and the empty comparison fell through to **`EXIT_DIGEST_MISMATCH`**. The "couldn't check" → "check failed" fusion, one layer below where it was fixed, raising a **false tampering alarm** on the one code the whole design turns on. **Reproduced by execution before fixing** (a `sha256sum` shim exiting 1, and one exiting 0 with empty output, both yielded exit 6), and re-verified after. Fixed at `install_remote_server.sh:152`: each branch captures the tool's own status via a **locally scoped** `set -o pipefail` and rejects a result that is empty or not bare hex. **`pipefail` is deliberately NOT global** — measured: `set -e; set -o pipefail; x=$(find … | head -n1)` exits **141** (SIGPIPE), which the ERR trap would convert to exit 9, breaking both staging-tarball paths. **Second finding — a regression the previous repair introduced:** OpenSSH reports *its own* failures (connection closed, dead ControlMaster, host-key change) as **exit 255**, and `run_ssh_script` returns `Ok(output)` whenever the ssh *process* ran, so 255 arrived as `ScriptFailed{255}` → `_ => Fatal` → **no fallback**, for a script that never executed — while the doc claimed `TransportFailed` covered exactly that. Verified with real ssh against a nonexistent host and a dead ControlPath. Now `255 => TransportFailed` (`ssh_transport.rs:294`); the fallback is still reachable only on 3, 7, 255 and `Other`, so it was not widened. **Third:** `EXIT_UNSUPPORTED_PLATFORM` moved 2 → **10**, because bash exits 2 on a *parse* error, which precedes trap installation and so cannot be remapped; 2 now belongs to no contract code and fails closed as unrecognised. New test `install_script_exit_codes_avoid_bash_reserved_statuses` bars 0/1/2/126/127/255 and 128-192. **Caveat:** no Rust test was executed (build gate); one 255 shape — a remote command killed by a signal — could not be executed without a real host.

- [ ] 🔴 **The BYOP API key is sent to every SSH host, ungated and undisclosed.**
      `ai/codebase_embeddings.rs:441-448` puts the keychain key into
      `EmbeddingProviderConfig`; `client/mod.rs:332` ships it in every `Initialize`, and
      `:356` re-ships to every connected daemon on a settings change. Neither side has a
      `FeatureFlag::RemoteCodebaseIndexing` gate (`server_model.rs:1683` lacks the one its
      siblings at `:1663/:1725` have). `auth_token` rides along. The consent dialog
      (`i18n/en/warp.ftl:260`) mentions only "file browsing, code review". A compromised
      host harvests both.
      **VERDICT PARTIAL — not every host (independent verifier, 2026-08-21):** The chain holds: keychain → `secrets.rs:22-37` → `embeddings.rs:112-117` → `codebase_embeddings.rs:441-448` → every `Initialize` (`client/mod.rs:330-333`) and every settings change (`manager.rs:818-827`), with no flag check at `lib.rs:2229-2245`. **But "every SSH host" is wrong** — transmission requires the per-host install choice (`ssh_remote_server_choice_view.rs:78-91`). The undisclosed limb stands (`warp.ftl:260`).
      **FIXED 2026-08-21:** the `embedding_provider` carrying the keychain key is populated only when `FeatureFlag::RemoteCodebaseIndexing.is_enabled()` — the same flag the daemon requires (`server_model.rs:1663`), so a daemon that could not use the key never receives one. Failure is loud rather than silent: the daemon reports `Unavailable` with a user-visible reason.

      **REFUTATION 2026-08-21 — FIX DEFEATED, REOPENED.** The gate is a constant. `remote_codebase_indexing` is in `app/Cargo.toml:660`'s **default** feature set (independently confirmed by the coordinator) and is force-enabled at `lib.rs:2821,3234`, so `is_enabled()` is always true and the key still ships. A compile-time feature that is on by default cannot express user consent; the real runtime predicate is `should_use_codebase_indexing` (`codebase_auto_indexing.rs:32,56-59`). The user-facing disclosure at `i18n/en/warp.ftl:260` was never updated despite this being ticked. Repair dispatched.

      **REPAIRED 2026-08-21 (refutation upheld).** Re-gated on the runtime predicate `should_use_codebase_indexing` (`codebase_embeddings.rs:461-462`), and the keychain resolver is now a **closure** (`:501-517`) so when the gate is false the key is never read at all rather than read-and-discarded. **Consent chain confirmed opt-in:** the predicate requires `CodeSettings::codebase_context_enabled`, whose default is `false` (`settings/code.rs:65-73`) and which is written only by the user via the settings UI; `FullSourceCodeEmbedding` is env-gated and also off by default. **The refuter's line number was wrong** — the disclosure is `app/i18n/en/warp.ftl:1279`, not `:260` — and the string claimed *"nothing here is sent to any external server"*, which was flatly false while `Initialize` carried `api_key`. Rewritten at `:1268` and `:1279`; the zh-CN and ja translations of the same false claim were corrected too (coordinator). **Accepted deviation:** the agent also added a `CodeSettings` subscription in `app/src/lib.rs:2230-2269` — without it, withdrawing consent would not retract the key from already-connected daemons (defect pattern 4), which would make the new disclosure text a lie. Precedent: `codebase_index_model.rs:246-249` subscribes for the same setting for the same reason. **The pin has no `ClientPreferences`/`EmbeddingProviderConfig` at all** — the BYOP key on the wire is a fork invention, so the gate had to be invented too; this is a deliberate divergence, not a parity port. Also corrected the doc comment on the credential field itself (`crates/remote_server/src/client/mod.rs:93-98`), which still documented the defeated gate.

      **NLD MIGRATION REGRESSION — FOUND BY REFUTATION AND FIXED 2026-08-21, see `DECLINED.md`.** `f0b71fe3e` also restored the pin's `nld_in_terminal_enabled` startup migration verbatim. Its predicate reads two settings whose defaults diverge in this fork, so it evaluated `false && true` and **explicitly wrote `nld_in_terminal_enabled = false` for every user who never touched it**, killing the fork's CJK terminal-input default — and because the write is explicit, the default could never apply again even if the migration were removed. Caught before any build. Now gated on `ai_autodetection_enabled_internal.is_value_explicitly_set()` (`settings/initializer.rs:132-212`), which is what the pin's `is_onboarded()`-based "existing user" test was standing in for and which this tree can actually answer (`is_onboarded()` is a constant `Some(true)` — `auth/mod.rs:213`). An untouched user returns early before any `set_value`. **Sibling migrations audited** (`initializer.rs:53-73`): the Adeberry-theme one is the only near-miss (`ThemeKind::default()` is `PhosphorAmber` here vs `Dark` at the pin) but it compares against the literal `ThemeKind::Phenomenon`, so divergence can only make it silently *not* fire, which is correct here. **Defect 2 also fixed:** the seeding guard recorded failure as success. `toml_backed.rs:189-227` `flush()` now returns `Err` instead of a silent `Ok(())` while `write_inhibited`; `crates/settings/src/lib.rs:543-560` propagates the result the old `let _ =` discarded; `privacy.rs:614-640` sets the guard **only** on confirmed success. **Scope call:** the *per-key* inhibition was deliberately left non-erroring, because it is set by the settings layer in-process and erroring would make "reset this setting to its default" fail for exactly the settings whose stored value is broken; residual gap documented. **Blast radius handled honestly:** propagating the result made `Err` reachable at eight production sites that `.expect(...)`ed on `set_value` assuming only serialization could fail — a broken `settings.toml` would have turned those into crashes. All converted to `report_if_error!` (`settings/editor.rs:279-288`, six sites in `terminal/warpify/settings.rs`, `workspace/view.rs:16894-16906`), and `toml_backed_tests.rs:311-333`, which asserted `Ok` on the inhibited write, is inverted.

- [x] 🔴 **The command denylist is bypassable with one quote character.**
      `warp_completer/src/parsers/simple/mod.rs:242-247` returns the literal typed text,
      quotes included, and `permissions.rs:929-934` matches it with an anchored regex. A
      denylist of `rm .*` matches `rm -rf ~` but not `"rm" -rf ~`, `r"m" -rf ~` or
      `'rm' -rf ~` — all of which the shell executes identically. Any model-authored
      command evades any user or org denylist. **Pin-parity**, so it is inherited rather
      than a port regression, but it is unrecorded and unfiled.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Chain verified end to end: `simple/mod.rs:242-247` slices raw `src` by span so quotes survive `decompose`, and `permissions.rs:928-934` matches with `^…$` anchors added at `settings/ai.rs:851`. `"rm" -rf ~` misses `rm .*` and falls through to AlwaysAllow. Pin-identical, so inherited. **One caveat the original missed:** "or org denylist" is moot — that list is always empty in this fork.

      **FIXED 2026-08-21 — divergence AHEAD of the pin, recorded in `DECLINED.md`.** The chain was confirmed end to end, and the parser turned out to be innocent: it **does** unquote (`"rm"` → `Part::Literal("rm")`, `"r"m` → `Part::Concatenated` whose `Display` is `rm`) — every text-returning API then threw that away, because `Command::decompose`/`source` slice the raw source by span. Fixed with a **split**: `unquoted_command_parts` (new, additive, `warp_completer/.../simple/mod.rs:107-143`) exposes the parser's existing unquoted view, and the *policy* lives in `denylist_match_candidates` (`permissions.rs`). **Fail-closed by construction:** the candidates are a **superset** with the as-typed text always #0, so the decision can only deny more, never less — including when the name is unresolvable (`$(echo rm) -rf /`). **Deliberately NOT normalising `decompose_command` itself:** its output also feeds command x-ray, error underlining and the **allowlist**, and widening an allowlist match is the unsafe direction; a denylist that normalises differently from the text shown to the user is its own hazard. Also repaired a smaller pre-existing fail-open: the old helper *replaced* raw text with its env-stripped form, so a rule written `X=1 rm .*` had stopped matching. **Handled** (each tested): `"rm"`, `'rm'`, adjacent concatenation, mid-word `r\m`, leading `\rm`, `$'rm'`/`$"rm"`, `X=1 rm` incl. quoted, quoting in arguments, all of the above inside `$(...)`/backticks and after `&&`, and PowerShell backtick escapes. **Explicit residue, documented in-source:** `$'...'` escape *decoding* (would teach the lexer a bash-only dialect and change tokenisation for every consumer); `command`/`env`/`sudo`/`timeout`-style prefixes (open-ended set, each with its own option grammar — a partial list creates false confidence); anything needing shell evaluation; path equivalence. Negative controls included so unquoting does not over-match.

- [x] 🔴 **The env-var strip that the fork's own comment says closes that hole strips
      only assignments containing exactly one `=`.** `mod.rs:255` breaks unless
      `split('=').count() == 2`, so `FOO=a=b rm file.txt` — a valid bash assignment —
      reaches the denylist unstripped, which is precisely what the comment at
      `permissions.rs:903-904` claims cannot happen.
      **VERDICT PARTIAL — attribution wrong (independent verifier, 2026-08-21):** The gap is real: `mod.rs:255` requires `split('=').count() == 2`, so `FOO=a=b rm file.txt` is never stripped and `source()` returns it whole, missing `^rm .*$`. **But the attribution breaks** — `42effe840:.../simple/mod.rs:255` is byte-identical and the pin has the same call site, so only the COMMENT is the fork's, and the comment's own stated example (`X=1 rm file.txt`) genuinely is stripped.

      **FIXED 2026-08-21 — divergence ahead of the pin, and the pin's rule is wrong in BOTH directions.** `split('=').count() == 2` fails to strip `FOO=a=b cmd` (a valid assignment — bash, dash and zsh all strip it) **and wrongly strips** `1FOO=b cmd`, `FOO-1=b cmd`, `=b cmd`, none of which is an assignment: all three shells report "command not found" for the prefix itself, so **no command runs at all**. The ledger only had the widening half. Verified with an `rm` shim under bash, dash and zsh across 17 spellings. `mod.rs:342` now applies the shell's own rule (`[A-Za-z_][A-Za-z0-9_]*`, optional bash `+=`, value may contain `=`); predicate cross-checked against the shell truth table — new 14/14, old 8/14. **A correction the ledger did not have:** `"X"=1 rm file` is **not an assignment at all** (all three shells run a program named `X=1`), so the bullet claiming it "handled" was misdescribing an accepted **over**-match — left alone, since it can only deny more. Residue: bash's array form `FOO=(a b) cmd`. **No parser-level unit test** — `parser_test.rs` was outside the edit list, though its existing cases were checked by hand against the new predicate.

- [x] 🔴 **Hunk staging can stage a hunk the user did not click, silently.**
      `code_review_view.rs:5902-5903` computes `hunk_end` from `hunk.lines.len()`, which
      includes `Delete` lines that occupy no new-file line, so the extent is overstated
      and `.find()` returns the first overlapping hunk. `hunk_to_patch` then rebuilds the
      PREVIOUS hunk's patch, so `git apply --cached` succeeds with no error: click
      "stage" on hunk 2 and hunk 1 lands in the index and the next commit. Fork-original,
      untested.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Verifier walked a concrete case as instructed: editor ranges cover only changed lines, no context (`diff.rs:294-303`); with hunk1 `@@ -1,10 +1,6 @@` (4 deletes) and hunk2 at new line 11, `hunk_end = 1 + lines.len() = 11 >= 11`, so `.find()` (`code_review_view.rs:5897-5904`) returns **hunk1**. Trigger threshold is deletes ≥ gap+3, gap min 1. The pin has the same arithmetic (`:6209`) but FILTERS the lines; the fork returns the whole earlier hunk and stages it (`:5874-5880`).
      **FIXED 2026-08-21:** the extent now counts only lines occupying a new-file line (`Add | Context`), not `lines.len()` which included `Delete`. Re-walked the worked example: hunk1 `@@ -1,10 +1,6 @@` now yields `hunk_end = 7`, so a click at new line 11 skips it and resolves to hunk2. A `.max(1)` guard beyond the pin keeps a zero-new-line hunk (whole-file deletion, `diff.context=0`) clickable — it cannot reintroduce the bug since it fires only when a hunk contributes no `+` lines at all.

      **REFUTATION 2026-08-21 — FIX DEFEATED AND STRICTLY WORSE IN ONE CASE, REOPENED.** (a) The re-derivation is redundant and divergent: `hunk.new_line_count` (`diff_state.rs:252`, set at `:3090` from the `@@` header) already is the authoritative count and `hunk_to_patch` uses it (`:279-282`) — coordinator confirmed. (b) With `diff.suppressBlankEmpty=true` the parser drops blank lines (`:3020-3023`), so the Add|Context recount undercounts and the button **silently does nothing — worse than the code it replaced**. (c) The twin predicate at `code_review_view.rs:6575` was left unfixed (coordinator confirmed), so paperclip and plus can now resolve to *different* hunks — a divergence created by fixing one of two identical predicates. (d) With `diff.context=0`, `.max(1)` plus the exclusive-end comparison `requested_start <= hunk_end` makes a click on hunk 2 stage hunk 1. (e) **The TODO claim that the pin filters these lines was wrong:** `42effe840` has no `hunk_covering_line_range` at all and its `extract_diff_hunk_data` predicate is character-identical to the original buggy one — so any fix here is a deliberate divergence ahead of the pin, not a parity port. Repair dispatched.

      **REPAIRED 2026-08-21 (refutation upheld on every limb).** The re-derivation is gone. New shared helpers `hunk_new_line_span` (`diff_state.rs:353`) and `hunk_covers_new_lines` (`:375`) are built on `hunk.new_line_count`, which comes from the `@@` header and is therefore immune to both the `suppressBlankEmpty` undercount and the `context=0` case. **Both predicates now call the same code path** (`code_review_view.rs:5925` and `:6573`), so they can only diverge again if one stops calling the shared function — corroborated as user-visible, since `element.rs:1279,1292` hand both gutter buttons the identical range. **End convention now half-open `[start, end)`, strict comparisons**, stated in a `# End convention` doc section; abutting hunks share no line. `.max(1)` removed: a pure deletion occupies no new-file line and is anchored on `new_start_line + 1`, matching `DiffStatus::deletion_mapping` (`code/editor/diff.rs:691`), which is keyed by the line *following* the removed block. **(e) confirmed and the earlier TODO claim was mine and was wrong:** `git grep hunk_covering_line_range 42effe840` has no hits and `42effe840:code_review_view.rs:6190` is character-identical to the buggy predicate, so this is a fix *ahead of* the pin — a warning against "restoring parity" at the next re-pin is in the doc comment. **(d) is far more common than the refuter's `context=0` example:** `@@ -10,50 +10,1 @@` has `lines.len()==50`, so the hunk reached to new line 60 and swallowed a hunk starting at 21 — no exotic git config required. **Adjacent defect found, deliberately NOT fixed, needs its own issue:** the parser's empty-content-line skip (`diff_state.rs:3020-3023`, inherited from the pin) drops the line from `hunk.lines` entirely, so `hunk_to_patch` emits a body one line shorter than the header it writes and `git apply --cached` will reject or misapply it. Under `diff.suppressBlankEmpty=true` hunk staging is broken at the patch level, independently of the predicate.

- [x] **`no_trailing_newline` is never set, so the marker `hunk_to_patch` promises is
      never emitted.** `diff_state.rs:3034` tests for `"\\No newline at end of file"`;
      git emits `\ No newline at end of file` (backslash SPACE), which is then skipped as
      not starting with `+`/`-`/space. The flag is always false, `:294-297` is dead, and
      `hunk_to_patch_preserves_missing_trailing_newline` hand-sets the flag the parser
      cannot produce. Staging the last hunk of a newline-less file silently appends a
      newline or fails.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `diff_state.rs:3034` tests `"\\No newline..."` without the space, and the marker never reaches it anyway — it starts with `\`, so `:3022-3025`'s else-branch skips it. The flag is unconditionally false and `:295-297` is dead. Verifier added the attribution: the parser line is pin-parity (`42effe840:.../diff_state/local.rs:2676`) where the flag was only SERIALIZED — the fork's `hunk_to_patch` is what made a dead flag load-bearing.
      **FIXED 2026-08-21:** `diff_state.rs:3021-3042` — git emits `\ No newline at end of file` as its OWN line after the line it describes, so the flag is now set retroactively on the previously-pushed line. The dead `ends_with` test is removed.

- [x] **Conflicted files get a hunk stage button the file-level path deliberately
      withholds.** `stage_button_appearance` returns `None` for `Conflicted` because "a
      click the backend cannot honour", and `toggle_file_staged` re-checks it — but
      `stage_hunk_direction` and `toggle_hunk_staged` omit the gate, and unmerged entries
      get `staged: Unstaged` (i.e. `Some`), so the button renders.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `code_review_view.rs:340-341` returns `None` for `Conflicted` and `:5809` re-checks it in `toggle_file_staged`, but `stage_hunk_direction` (`:3157-3160,:3260-3263`) and `toggle_hunk_staged` (`:5867-5871`) read `file_diff.staged` only, and `diff_state.rs:2658-2662` gives unmerged entries `Some(Unstaged)` — so the gutter button renders. Consequence is a failing `git apply --cached`, silent per finding 55; not corruption.
      **FIXED 2026-08-21:** New `hunk_stage_direction()` (`code_review_view.rs:356-368`) mirrors `stage_button_appearance`'s `Conflicted` gate; both gutter sites (`:3160`, `:3253`) and `toggle_hunk_staged` (`:5898`) now go through it.

- [x] **Stage failures are invisible** — `diff_state.rs:1228-1230` handles `Err` with
      `log::error!` and no toast, so the acknowledged "partially staged file guesses
      stage" heuristic fails as a silent no-op.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `diff_state.rs:1226-1229` is `Err(err) => log::error!(...)` with no toast and no `ToastStack` reference anywhere in the file; reload runs only on `Ok`, so a rejected patch leaves the UI unchanged — indistinguishable from nothing happening. `code_review_view.rs:5853-5854` acknowledges it. Verifier noted the sibling DISCARD path (`:1183`) behaves identically, so it is a design gap rather than staging-specific.
      **FIXED 2026-08-21:** New `DiffStateModelEvent::GitWriteFailed { operation, message }` emitted from BOTH the stage (`diff_state.rs:1236`) and discard (`:1183`) error arms — the verifier noted discard had the same gap — rendered as a `DismissibleToast::error` (`code_review_view.rs:2365`). Message is untranslated; adding an `.ftl` key was outside the agent's file list.

- [x] **TODO.md:215 states the opposite of the code.** It lists hunk staging under
      "Confirmed genuinely absent" with evidence "no `stage_hunk`/`checkout_branch`". It
      exists end-to-end (`toggle_hunk_staged`, `StageTarget::Hunk`,
      `run_apply_patch_cached`, `StageHunkButton`, plus the daemon path), and TODO.md:2982
      records it as landed. The evidence cell is a bare-name grep — the exact failure the
      surrounding text warns against.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `TODO.md:215` sits under "Confirmed genuinely absent" (`:204`) and is contradicted by `toggle_hunk_staged`, `StageTarget::Hunk` (`diff_state.rs:163`), `run_apply_patch_cached`, `StageHunkButton` and the daemon leg (`server_model.rs:3542`, proto `:1418`); TODO.md:2980 records it landed. Verifier also checked the row's SECOND half: `checkout_branch` was a deliberate removal shipped via prompt chip, so **both cells are stale**.
      **FIXED 2026-08-21:** Row struck with citations, and the second half corrected too — `checkout_branch` was a deliberate removal shipped via prompt chip, not an absence. The bare-name-grep failure is explained against the table's own stated evidence rule.

- [ ] **`@`-context attachments lock the input with no invalidation on edit.**
      `context_model.rs:748` adds them to `has_locking_attachment`, which kills
      autodetection and blocks unlock; `prune_stale_at_context_attachments` runs only on
      menu-accept and submit, never on edit. Delete the `@ref` text and the input stays
      stuck in AI mode with an empty buffer — **the next shell command goes to the
      agent.**
      **VERDICT PARTIAL — 'blocks unlock' wrong (independent verifier, 2026-08-21):** `context_model.rs:745-749` does include the at-context attachments, gating `should_run_input_autodetection`, and pruning runs only at `input.rs:9779` and `:12923`. But it is NOT a parity gap — `42effe840:context_model.rs:269-271` has no at-context machinery at all — and "blocks unlock" is wrong: Esc reaches `set_input_mode_terminal` (`input.rs:7881`, `:13102-13114`), an unconditional manual override.

      **PARTIAL CONFIRMED, NOT FIXED 2026-08-21 — doc corrected, behaviour needs a file outside this round.** "Blocks unlock" is **wrong**: Escape clears attached context, a second Escape reaches `set_input_mode_terminal` (`input.rs:13111`, an unconditional manual override), and a send resets via `reset_context_to_default`. So it is a **stale** lock, not a stuck one. **But the headline stands:** the submit-time prune (`input.rs:12933`) runs *inside* the AI submit path, so it removes the stale attachment **after** routing — the next shell command does go to the agent. No phantom attachment is ever *sent*. `retain_at_context_attachments_in_query` runs from exactly two places and **nothing runs it on a buffer edit**. **Deliberately not fixed by dropping the at-context clause** — while the `@ref` is in the buffer the lock is exactly right, and removing it would let the classifier flip a genuine `@`-reference query to shell. The predicate's doc did not even name the third source; rewritten to name all three, distinguish the two this model owns from the one that is a cache of a buffer fact, and cite the missing invalidation. **Proposed fix for the `input.rs` owner:** prune on the buffer-edited editor event, so the predicate reconciles per keystroke.

- [x] **The fork dropped the pin's agent-view guard from the boundary-backspace
      handler.** `terminal/input.rs:10262-10312` checks only `is_fullscreen()`. With an
      inline agent view active, backspace at buffer start runs the legacy classic-mode
      path, clearing follow-up targeting so the next message silently starts a new
      conversation. The comment at `:10294` claiming this "doesn't get called when
      AgentView is enabled" is now false.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** The pin returns early at `42effe840:terminal/input.rs:11762-11765` before the follow-up/icon logic; the fork's `maybe_backspace_ai_icon` (`input.rs:10262-10311`) has no such guard, and `is_fullscreen()` at `:10271` sits in the OTHER branch. Verifier checked the callers too: `:9697` and `:9703` dispatch both backspace events ungated, so `set_pending_query_state_for_new_conversation` fires with an inline agent view active.
      **FIXED 2026-08-21:** `input.rs:10286-10294` — the pin's `AgentView.is_enabled() && controller.is_active()` early return added before the follow-up/icon logic, and the `:10302` comment corrected (it is now true).

- [x] **Daemon sockets are partitioned by a 32-bit unkeyed `DefaultHasher`**
      (`remote_server/setup.rs:298-308`) while its sibling at `:317` states collisions
      are unacceptable and avoids hashing. Colliding identities share a daemon holding
      the other's token.
      **VERDICT PARTIAL — consequence fails (independent verifier, 2026-08-21):** `setup.rs:298-308` does truncate to 32 bits and the sibling comment does call collisions unacceptable for the data dir. But the consequence fails: the directory sits under ONE remote UNIX account's `$HOME` (`:270-282`) — already a shared trust boundary — and no protocol message discloses a stored token; `server_model.rs:1631-1632` merely overwrites it. The hashing is a documented `sun_path` length tradeoff with tests (`setup_tests.rs:440-478`).

      **FIXED 2026-08-21 — of the two concerns, only instability bites.** **Collision is not the problem** and the trust boundary is weaker than the verifier said: `remote_server_identity_key` resolves to `user_id()` or `anonymous_id()` and the fork no longer distinguishes them, so there is effectively **one identity key per install**. Widening is impossible anyway — `socket_path_fits_within_sun_path_worst_case` leaves ~6 bytes under macOS's 103 and 16 hex chars would add 8. **Instability is the real defect:** `DefaultHasher`'s algorithm is explicitly unspecified across Rust releases and its output is baked into a path **on a remote host that outlives the process**, so a toolchain bump silently re-points the daemon dir, the client finds no PID file, starts a second daemon, and the first keeps its socket, auth token and memory until reboot — nothing sweeps it, because sweeping means reading the PID file at the path we no longer compute. Replaced with FNV-1a 64 plus the MurmurHash3 `fmix64` finaliser, constants written out in-file. **The finaliser is not decoration:** raw FNV-1a on the real UUID-shaped inputs collapses `"key-a"`/`"key-b"` to `71132af2`/`711329f2` — one nibble apart, **in exactly the truncated half** — because identity keys differ in their last bytes; with `fmix64` they are `b2009287`/`a6ec4b66`. The changeover orphans one daemon per remote host, documented in-source: the same event a toolchain bump used to cause silently at an unpredictable time, happening once, on purpose.

### Refutation round — search and AI tool output

- [x] **`file_glob`'s result cap is computed and then thrown away.**
      `agent_providers/tools/search.rs:136-156` clamps `max_matches` (default 200, hard
      cap 2000), but `crates/ai/src/agent/action/convert.rs:162-172` builds the action
      from only `patterns` + `search_dir`, and the enum has no limit fields at all
      (`crates/ai/src/agent/action/mod.rs:94-98`, with a standing
      `// TODO: Maybe implement client side depth and result limits`). The executor
      applies no cap. Pattern #3 — verdict computed, discarded a layer below. The fork's
      own comment says the cap exists because thousands of paths "cut the stream off
      instantly" on a 32K-context local model; that failure is entirely unmitigated. The
      UI additionally renders `"limit": g.max_matches`, showing the user a limit that was
      never enforced.
      **VERDICT PARTIAL — not unmitigated (independent verifier, 2026-08-21):** The enum drop is confirmed (`crates/ai/src/agent/action/mod.rs:94-97`, `convert.rs:161-172`) and the executor applies no cap. **But "entirely unmitigated" is wrong:** `chat_stream.rs:805-822` truncates every tool response at 40,000 chars with an explicit notice. The UI claim is also wrong — `:3514` is `serialize_outgoing_tool_call`, model-facing history, not the UI.

      **FIXED 2026-08-21 — consequence sharpened.** The only backstop was `chat_stream`'s 40,000-char truncation, and that is a **blind `chars().take()`**, so it sliced the serialised JSON **mid-array and mid-path**: the model received **malformed JSON**, plus a notice pointing at a `limit` parameter nothing could enforce. (So "reported as complete" was not happening — the notice was there — but it named a knob that did not exist.) Capped in `glob_result_to_json`, the only layer that still sees both the true match count and the serialised shape, emitting `truncated`/`total_matches`/`note` so a shortened list is never presented as complete. **The `limit` parameter was REMOVED, not fixed:** advertised-and-silently-discarded is the identical defect to `grep.md`'s phantom `include`, already CONFIRMED in this same file. Deliberately kept inside fork-original `agent_providers` rather than adding a field to the pin-inherited `AIAgentActionType::FileGlobV2`, which carries the pin's own `TODO`.

- [x] **`grep.md` documents an `include` parameter that does not exist and is
      schema-forbidden.** `prompts/tool_descriptions/grep.md:4` tells the model to
      "restrict file types via the `include` glob"; `grep_parameters()` declares only
      `queries`/`path` with `"additionalProperties": false`. Strict providers reject the
      call; lax ones drop it silently and the model believes it searched only `*.ts` when
      it searched everything. Fork-original, not pin drift.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `prompts/tool_descriptions/grep.md:4` advertises `include`; `tools/search.rs:21-37` declares only `queries`/`path` with `"additionalProperties": false`, and `GrepArgs:14-19` has no `deny_unknown_fields`, so a passed `include` is silently discarded. The file is absent at the pin, so fork-original. Verifier added the decisive detail: **no `strict: true` is set anywhere** in `tools/mod.rs`, so the silent-drop branch is the one that actually runs.
      **FIXED 2026-08-21:** Removed, and replaced with true guidance rather than a bare deletion: the model is told `queries` and `path` are the only parameters and pointed at `file_glob` + `read_files` for type filtering.

- [x] **Both search tools promise mtime ordering that no code produces.** `grep.md:5` and
      `file_glob.md:3` claim results are "sorted by modification time (most recent
      first)". `run_ripgrep` collects into a `HashMap` and iterates it
      (`execute/grep.rs:438-455`), so order is randomised per call; no `sort`/`mtime`
      call exists in either executor. With the cap unenforced the model gets neither the
      promised ordering nor a truncation flag — `glob_result_to_json` emits
      `status: "ok"` unconditionally.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `execute/grep.rs:437-455` builds a `HashMap` and consumes it with `into_iter()`; neither `grep.rs` nor `file_glob.rs` contains any `sort` or `mtime` call, and the git-grep/git-ls-files fallbacks yield path order. Contradicts `grep.md:5` and `file_glob.md:3`. **The trailing "no truncation flag" rider is wrong** — `chat_stream.rs:816-821` appends an explicit notice.
      **FIXED 2026-08-21:** Both promises removed from `grep.md` and `file_glob.md` and replaced with an explicit "order is unspecified — do not read anything into it", which is what the HashMap iteration actually gives.

- [x] **Clearing the global-search query leaves the ripgrep subprocess running.**
      `workspace/view/global_search/view.rs:872-878,913-919` reset the id and the
      in-progress flag but never call `abort_search()`; the pin routes both sites through
      `cancel_search`, which does. The handle is never aborted so `kill_on_drop` never
      fires: a full-tree scan runs to completion after the user clears the box, spawning
      batch callbacks onto the UI thread that the stale-id guard then discards.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `global_search/view.rs:872-875` and `:913-916` null the id and flag without touching `find_model`; only `abort_search` (`:844-851`) drains handles, and the pin routes both sites through `cancel_search` (`42effe840:...:852-853,892-893`). Verifier traced the consequence: `model.rs:437` streams from `warp_ripgrep::search::search_streaming`, which spawns a child with `kill_on_drop(true)` — so an un-dropped handle keeps the scan alive.
      **FIXED 2026-08-21:** `global_search/view.rs:844-863` — `cancel_search` split out; `abort_search`, the empty-query editor event and `rerun_search_from_query` all drop the handles, so `kill_on_drop` fires.

### Refutation round — fourth wave (de-clouding fallout)

- [x] 🔴 **Default secret-redaction regexes are NEVER installed — out of the box no
      secret is detected or blurred in terminal output.**
      `settings/privacy.rs:592` `initialize_default_regexes_once` has exactly one caller
      chain, ending at `initialize_from_fetched_settings_or_update_settings` (`:377`),
      **which has zero callers** — `fetch_or_update_settings` is now an empty stub and
      `auth_manager.rs` / `cloud_preferences_syncer.rs` were deleted with the cloud
      layer. At the pin both entry points are live
      (`42effe840:app/src/auth/auth_manager.rs:510`,
      `42effe840:app/src/settings/cloud_preferences_syncer.rs:502`).
      `CustomSecretRegexList` defaults to `Vec::new()` and
      `terminal/secret_regex_updater.rs:39` builds the scanner from that list alone. The
      user must find Settings > Privacy and click "Add all recommended" to get any
      redaction at all. Unrecorded and unfiled — a de-clouding casualty nobody noticed.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Chain verified end to end: `privacy.rs:592` is reached only from `:629` ← `:619` ← `:403` inside `initialize_from_fetched_settings_or_update_settings` (`:377`), which repo-wide grep shows has **zero callers**; `fetch_or_update_settings` (`:368`) is an empty stub, also uncalled. The pin calls it at `42effe840:app/src/auth/auth_manager.rs:510`. An empty list makes `secrets.rs:348` compile a match-nothing regex. Not in DECLINED.md. **Verifier also explains why CI never noticed:** the only other caller is `integration_testing/terminal/step.rs:55`, gated on `feature = "integration_tests"` and invoked explicitly by `crates/integration/src/test/secrets.rs` — so the suite seeds the regexes itself and stays green.

      **FIXED 2026-08-21:** new `settings::run_startup_settings_initialization` (`settings/init.rs:326-370`), called once from `lib.rs:1373` right after `PrivacySettings::register_singleton` — the earliest point where both `AuthStateProvider` and `PrivacySettings` exist (`settings::init` runs too early). **No clobbering:** the guard is the persisted private setting `HasInitializedDefaultSecretRegexes` (`privacy.rs:129-135`), not list contents, so "never seeded" and "seeded then user deleted" stay distinguishable and deliberate removals stay removed.

- [x] 🔴 **`agents.warp_agent.is_any_ai_enabled` is a public, schema-emitted setting that
      nothing reads.** Defined at `settings/ai.rs:1850-1857` with `private: false` and the
      description "Controls whether all AI features are enabled", so it appears in
      `settings.toml` and the JSON schema. The getter at `:2816` returns a hardcoded
      `true` and never reads the field. Because the key IS known,
      `settings_file_diagnostics` will not flag it either. **A user who believes they
      disabled AI still ships terminal context to their BYOP provider.**
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `ai.rs:2816-2820` returns `true` unconditionally and the field is read nowhere — all ~40 hits are the getter. The pin's version (`42effe840:app/src/settings/ai.rs:2182-2191`) reads `*self.is_any_ai_enabled`. It is emitted publicly (`ai.rs:1850-1857`, `private: false`, with a `toml_path`). **Verifier added:** no UI writes it either — only `slash_command_model_tests.rs:57,194`, and `:192`'s own comment concedes the value "is now ignored".

      **FIXED 2026-08-21:** `settings/ai.rs:2829` returns `*self.is_any_ai_enabled` (pin semantics minus the auth/org-policy terms this fork does not have). **Consequence accepted:** `slash_command_model_tests.rs` `test_ai_commands_ignore_legacy_global_ai_disabled_setting` asserted the defect (that the user's off-switch is ignored); it was inverted to `test_ai_commands_honour_global_ai_disabled_setting` — this is correcting a defect-enshrining test, not weakening a test to go green.

      **COMPLETED 2026-08-21 (the writers).** The first fix restored the **reader only**, and the verifier's "no UI writes it either" was recorded and not acted on — leaving a user whose key is `false` with a fully greyed AI page, no switch, no keybinding and no explanation, recoverable only by hand-editing TOML. `git show 14ed5014f` confirms the pairing: that commit removed the getter's honesty **and** every writer together (the `flags::IS_ANY_AI_ENABLED` toggle binding, `AISettingsPageAction::ToggleGlobalAI`, `GlobalAIWidget`'s switch, the telemetry event, and five `.ftl` keys). Now restored in `ai_page.rs`: `MasterAISwitchState::for_setting` (`:180-205`), the single writer `toggle_global_ai` (`:207-215`) shared by switch, keybinding and test, the `ToggleGlobalAI` action pair (`:229-245`) whose `context_prefix` is deliberately the bare parent context rather than `& id!(IS_ANY_AI_ENABLED)` **so the "Enable AI" half is offered in the command palette precisely when AI is off**, and `build_page` pushes the widget before the `match` (`:1640-1646`) so no branch can drop it. Off state renders an explainer banner. `should_offer_cli_agent_in_tab_menu` was added as a separate getter and **REMOVED AGAIN on 2026-08-21** — adjudication found the master switch does not scope third-party CLI agents (two verbatim pin statements say so, in a tree with *more* reason to gate than this one), so the tab menu, the title bar and the footer all gate on their per-agent settings only, and the `e0c3dfe2f` gate was reverted. See the "master AI switch scope" row in `DECLINED.md`. Superseded original text: it was added as a **separate** getter rather than changing `is_cli_agent_tab_menu_enabled`, whose other two call sites are the per-agent settings toggles that must stay editable while AI is off; `workspace/view.rs:6342` switched to it (coordinator). Three i18n keys added to en/ja/zh-CN (coordinator). **Reasoned refusal accepted:** the `DockerSandbox` arm of `default_session_mode` is **not** a defect — it is pin-verbatim (`42effe840:settings/ai.rs:2209-2231`, only `CloudAgent`→`AmbientAgent` differs), and a Docker sandbox session is a containerized *shell*, not an AI surface, so turning AI off must not confiscate it; a comment now records that so it is not re-derived. **Remaining:** `TelemetryEvent::ToggleGlobalAI` was not re-added (`server/telemetry.rs` was outside the edit list); and the "widget is installed" assertion is against `subpage_shows_master_ai_switch` rather than a constructed `AISettingsPageView`, which needs ~8 singletons and could not be verified without a build.

- [x] **`SettingsInitializer::handle_user_fetched` is dead code and its migrations never
      run.** `settings/initializer.rs:35` has zero callers; the pin calls it from
      `auth_manager.rs:430`. The `KeepThinkingExpanded` → `ThinkingDisplayMode` migration
      never fires, so upgraders silently lose that preference and the stale key is never
      cleaned. Comments at `input_mode.rs:9` and `theme.rs:17` still promise an override
      that cannot occur.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `initializer.rs:35` has zero callers tree-wide; `init.rs:127` only registers the singleton, and the pin calls it at `42effe840:app/src/auth/auth_manager.rs:430-431`. So the `KeepThinkingExpanded` → `ThinkingDisplayMode` migration (`initializer.rs:121-170`) and its key cleanup never run, and `disable_default_regex_trigger` never fires for new users. The stale promises at `input_mode.rs:8` and `theme.rs:17` are confirmed.

      **FIXED 2026-08-21:** `handle_user_fetched` → `apply_startup_settings_migrations` (`initializer.rs:29-56`), invoked from `run_startup_settings_initialization` **before** the regex seeding, matching pin order (`42effe840:auth_manager.rs:430` precedes `:510`). **Caveat recorded in-source:** the `is_onboarded()==Some(false)` block stays dead because `auth/mod.rs:213` hardcodes `is_onboarded: true`, so the `input_mode.rs:8`/`theme.rs:17` promises still cannot fire; the `KeepThinkingExpanded`→`ThinkingDisplayMode` migration now does.

- [x] 🔴 **`DOGFOOD_FLAGS` / `PREVIEW_FLAGS` / `LOCAL_FLAGS` have no consumer in any
      buildable binary — six flags gating live code are dark.** Their only
      `with_additional_features` call sites are `crates/warp_tui/src/bin/{dev,local,
      preview,stable}.rs`, which are never compiled (`autobins = false`, one declared
      bin, and they reference a crate absent from the workspace). `phosphor-oss` adds
      `DEBUG_FLAGS` alone. So `FullSourceCodeEmbedding`, `CodebaseIndexPersistence`,
      `WarpControlCli`, `JupyterNotebookRendering`, `MultiLevelOrchestration` and
      `LocalDockerSandbox` have **no enable path at all** while gating shipped code —
      including the whole `warpctrl` surface (`lib.rs:2296`), its bundled skill, and two
      CI guards. **Three in-tree statements assert the opposite**, including
      `DECLINED.md:161` ("They are not dark") and `warp_features/src/lib.rs:830-834`
      ("this list is the only thing that turns them on").
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `crates/warp_tui/Cargo.toml:7` sets `autobins = false` with one `[[bin]]`, so `dev/local/preview/stable.rs` never compile; `app/Cargo.toml:6` likewise builds only `phosphor-oss` and `generate_settings_schema`, and `phosphor_oss.rs:44` adds `DEBUG_FLAGS` in debug only. All six flags appear solely in `DOGFOOD_FLAGS` — none in `RELEASE_FLAGS`, `UNSTABLE_FEATURES` (`lib.rs:3306`), `RUNTIME_FEATURE_FLAGS`, or any cargo feature. Both contradicting statements confirmed (`DECLINED.md:161`, `warp_features/src/lib.rs:830-834`).

      **FIXED 2026-08-21:** the six flags added to `UNSTABLE_FEATURES` (`lib.rs:3353-3386`) — opt-in via `ZAP_UNSTABLE_FEATURES`, default unchanged (off), no promotion. **The two false in-tree statements were corrected**, not just noted: `warp_features/src/lib.rs:848-853` and `app/src/lib.rs:3335-3340` (the claim that `DOGFOOD_FLAGS` reaches warp_tui's dev/local binaries — they never compile), plus a new doc block at `warp_features/src/lib.rs:795-818`.

- [x] **Nine drag tests report PASS without executing a step** — independent
      confirmation of the tab-group finding above, and it extends to
      `workspace.rs:187`. `driver.rs:242-248` logs "Skipping test" and exits 0;
      `tests/common/mod.rs:54-58` maps 0 to success, so **a skip is indistinguishable
      from a pass** across all 75 `set_should_run_test` sites — including four
      macOS-only tests on a Linux-only job. No SKIP is ever surfaced.
      **NOT ADJUDICATED (2026-08-21):** Its verifier returned verdicts for the other five findings in its batch and silently omitted this one. Note the closely-related finding above (all four tab-group drag tests skipped) IS CONFIRMED, and this one extends the same mechanism to `workspace.rs:187` and to the general claim that a skip is indistinguishable from a pass across all 75 `set_should_run_test` sites — that extension is unverified.
      **FIXED 2026-08-21:** Same fix; `workspace.rs:187` was the second site.

- [x] **Saved-position clicks aim at stale rects.**
      `warpui_core/src/integration/step.rs:700-716` fails only when the id is absent, but
      `PositionCache::committed_positions` persists "until explicitly cleared", so a
      moved or removed element still yields old bounds and the click silently lands
      nowhere.
      **VERDICT PARTIAL — pin-identical, narrower (independent verifier, 2026-08-21):** `presenter.rs:128-215` is byte-identical to the pin and the bounds check matches — upstream behaviour, not fork debt. The "moved element" half is FALSE: `end()` at `presenter.rs:167` does `committed_positions.extend(last.drain())`, overwriting on every re-render. Only a REMOVED, never-cleared element goes stale.

      **NARROWED then DOCUMENTED rather than faked, 2026-08-21.** **The "moved element" half is FALSE and is dropped:** `PositionCache::end` (`presenter.rs:167`) does `committed_positions.extend(last.drain())`, so a re-rendered element overwrites its own key every frame. **The surviving defect is a *removed* element** — `cache_position_indefinitely` (`:172`) never expires and the tree's only `clear_position` caller is `editor/view/element.rs:1281` — so a click lands at a dead rect's centre on whatever is painted there now, and `bounds.is_none()` (id never painted at all) is the only failure the step can report. **A click on the wrong element is silent.** **No check was added, deliberately:** "not painted this frame" is unobservable through the cache's public API (no frame stamp, `committed_positions` private with no iterator, no eviction), and the obvious clear-then-repaint workaround **does not work** because `maybe_render_frame` only waits for a frame when `App::has_window_invalidations` is *already* true — a live-but-idle element would fail spuriously, which is the mirror of a check that cannot fail. A 30-line hazard note now states this at the call site. **Still open:** a generation counter on `PositionCache`, which is pin-identical shared production code (menu anchoring, tab drag hit-testing).

- [x] **De-clouding turned `report_error()` from a no-op into a duplicate log.**
      `warp_core/src/errors.rs:218` — every actionable error now logs twice, and the
      extra line carries the module-path target rather than `LOG_TARGET`, so
      `RUST_LOG=errors::report_error=off` cannot suppress it and the `extra:` fields are
      missing. `errors_tests.rs:52` filters on `LOG_TARGET`, so it cannot see the
      duplicate.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `report_error!(@log)` calls `err.report_error()` then `log::log!` (`warp_core/src/errors.rs:81-85`); the fork's impls log (`:218`, `errors/anyhow.rs:27`) where the pin captures to Sentry. Verifier traced the sink: `env_logger` defaults to `LevelFilter::Info` with no `warp_core` filter (`warp_logging/src/native.rs:732-751`), so **both Error lines emit in release**. The duplicate's target is `warp_core::errors[::anyhow]`, invisible to `errors_tests.rs:53`.
      **FIXED 2026-08-21:** Both impls (`errors.rs:218`, `errors/anyhow.rs:30`) are no-ops again — the pin's shape minus Sentry — since `report_error!(@log)` already emits at `LOG_TARGET` with the `extra:` fields. Verified independently that the only two callers are inside the macro itself (`errors.rs:80`, `:112`), so no direct caller loses logging.

- [x] **`script/check_channel_command_names`'s header contradicts the code it guards** —
      it documents `Channel::Oss` as `zap-oss` for both `cli_command_name()` and
      `Display`; both are `phosphor-oss`. The guard passes (it derives from Rust), but
      anyone reading it to decide whether a rename is safe gets the pre-rename answer.
      Related: `crates/warp_tui/Cargo.toml:9,15` still ships the bin as `zap-tui-oss`,
      and `DECLINED.md:216` repeats it as current.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** The header says `zap-oss` at `:12`, `:23` (cli name) and `:49` (Display); both are `phosphor-oss` (`warp_core/src/channel/mod.rs:44,:70`). The fragments are derived from Rust (`:38-41`), so the guard still passes while misinforming its reader. `crates/warp_tui/Cargo.toml:9,16` do still ship `zap-tui-oss`, and DECLINED.md:216 repeats it.
      **FIXED 2026-08-21:** Header corrected: three `zap-oss` occurrences → `phosphor-oss`, matching `channel/mod.rs:44,:70`. Zero `zap-oss` strings remain in the guard.

- [x] **`paths_tests.rs:123` is vacuous** — it asserts `secure_state_dir() == None`, but
      on non-macOS that function returns `None` unconditionally, so on Linux CI it passes
      with the `Channel::Oss` guard deleted.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `paths.rs:191-208`: after the `Integration | Oss` early return, the only non-`None` path is inside `#[cfg(target_os = "macos")]`, so on Linux the function returns `None` for every channel. `paths_tests.rs:123-128` asserts only `secure_state_dir() == None` with no channel assertion, so deleting the `Channel::Oss` guard leaves it green on Linux CI.

      **FIXED 2026-08-21.** The channel decision is extracted into `channel_may_use_secure_state_dir(Channel) -> bool` (`crates/warp_core/src/paths.rs:186-212`) as an **exhaustive `match`, not a `matches!`**, so a new channel cannot silently default into the container. New `test_secure_state_dir_channel_gate` (`paths_tests.rs:122-148`) asserts `Oss`/`Integration` false and `Stable`/`Preview`/`Dev`/`Local` true — and **it goes red on Linux** if `Oss` moves to the `true` arm, which the old assertion could not. The original `test_oss_secure_state_dir_is_disabled` is kept verbatim but `#[cfg(target_os = "macos")]`-gated, since macOS is the only platform where it distinguishes anything; the name is preserved because this file refers to it in prose. The dependency is real and was confirmed: `persistence/sqlite.rs:152-175` documents that the legacy App-Group migration enumerates historical app ids *because* `secure_state_dir()` has returned `None` for `Channel::Oss` since the commit that added the migration.

      **CORRECTED 2026-08-21 — the justification for keeping the macOS test overstated its coverage.** The gate was defended as "macOS is the only platform where it distinguishes anything" — true, but it implied coverage that does not exist. The only macOS CI job (`pr-check.yml:675`) runs `-p warp -E 'test(/login_item/)'` (`:718`, `:741`) and checks `-p warp` (`:697`), so this `warp_core` test runs in **zero** jobs and **is not even compiled on macOS in CI**; on a dev Mac it is vacuous unless the App Group container already exists *and* `tempfile_in` succeeds. The comment now says exactly that and points at `test_secure_state_dir_channel_gate` as the real protection, and an `assert_eq!(ChannelState::channel(), Channel::Oss)` was added so a default-channel change fails loudly rather than turning it silently green. **Also corrected `paths.rs`:** the new doc claimed an OSS build must *never* touch the App Group container, but `sqlite.rs:688,706-707` probes it deliberately as the legacy-DB rescue — marker-guarded (`:805-814`) and bounded by `MAX_LEGACY_SCAN_ATTEMPTS`. The invariant is "no *routine* access", with one named exception, not "never".

- [x] **`step.on_failure_handler.take()`** (`integration/step.rs:846`) discards the
      handler, so a retried step loses its bail-out.
      **VERDICT PARTIAL — miscited and pin-identical (independent verifier, 2026-08-21):** The `.take()` is at `integration/step.rs:901`, not `:846` (`:846` is `idx += 1;`), and it is identical at the pin (`42effe840:...:895`) — not a fork defect. The mechanism was also misstated: `'outer` (`:815`) iterates `step.assertions`, so the loss hits LATER ASSERTIONS IN THE SAME STEP, not a retried step.

      **FIXED 2026-08-21 — and the refutation was itself half wrong.** The miscite is confirmed (`:901`, not `:846`) and the line is pin-identical (`42effe840:…:895`) — but the verdict's "not a retried step" is **incorrect**: `run_step` takes `&mut TestStep` and `driver.rs:562-592` re-runs the *same* step inside `'retries: loop`, so attempt 2 finds `on_failure_handler == None`. The ledger's original wording was right. The same-step loss the verifier found is *also* real. **What is lost is not a dump or a screenshot but a verdict downgrade:** the sole handler (`integration_testing/terminal/step.rs:68`) converts a bootstrap timeout into `PreconditionFailed("bash flaked on startup")` on bash 3.x, so losing it turns a known-flaky environment into a hard panic. `on_failure_handler` was the only single-use field `run_step` touched — everything else is iterated by reference. Now `.as_mut()`, a documented divergence from the pin. **No live test is affected:** the five `set_retries` sites set no handler, and the sole handler sets no retries.

### Refutation round — autoupdate (bears on any release decision)

- [x] 🔴 **Downgrade protection is dead code, and a beta user is silently
      auto-downgraded.** `crates/channel_versions/src/lib.rs:43` matches
      `^v?(\d{4})\.(\d{1,2})\.(\d{1,2})(?:\.(\d+))?$` — which matches NONE of this
      repo's actual tags (`v0.1.0`, `v0.1.1`, `v2026.08.14.1-beta`, nor the
      dispatch-generated `v0.$(date +%Y.%m.%d.%H%M)`). `try_from` errors, and
      `autoupdate/mod.rs:403` (`if let Ok(true) = …`) treats Err as "not ahead" and
      proceeds. Chain: the release workflow marks betas `prerelease/make_latest:false`;
      `github.rs:84` fetches `/releases/latest`, which skips prereleases (#614's family)
      and returns `v0.1.1`; the string compare says "new"; the guard that exists to stop
      exactly this is inert. `get_curr_parsed_version` is `None` for the same reason, so
      `is_incoming_version_past_current` is permanently false, killing all nine
      soft-cutoff / prominent-update gates in `workspace/view.rs`.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Tags are only `v0.1.0`, `v0.1.1`, `v2026.08.14.1-beta`; none match `channel_versions/src/lib.rs:43` (needs `\d{4}` first, `$`-anchored) nor `:34`. The dispatch tag also fails. `mod.rs:403` `if let Ok(true)` swallows the Err and `:477-491` auto-downloads. `make_latest:false` for beta at `:960`; `github.rs:84` fetches `/releases/latest`. `get_curr_parsed_version` → None → `mod.rs:1291` always false.

      **FIXED 2026-08-21:** `ParsedVersion` is now `{components: Vec<usize>, prerelease: Option<String>}` (`channel_versions/src/lib.rs:34-196`) instead of the `(major, date, patch)` triple no real tag fits. New anchored `RELEASE_TAG_RE` accepts every shape actually in use — `v0.1.0`, `v0.1.1`, `v2026.08.14.1-beta`, the workflow-generated `v0.$(date +%Y.%m.%d.%H%M)`. Bare date tags get a synthetic leading `0` so they share an axis with the dispatch tag; trailing zeros trimmed so `PartialEq` stays consistent with the zero-padding `Ord`. `mod.rs:425-451` `if let Ok(true)` became a full `match` that fails closed to `UpdateReady::No` on `Err`. **Known limit, flagged at `lib.rs:143-155`:** a beta user is correctly not downgraded but also will not be offered later `v0.1.x` — moving them forward needs channel awareness, not a numeric compare.

      **REFUTED THEN REPAIRED 2026-08-21.** The first fix was defeated on three counts, one of which would have **broken the build**. (1) `RELEASE_TAG_RE`'s group 1 backtracked, so `"v0.1.1-"` parsed as `[0,1]` + label `"1-"` — and `channel_versions_tests.rs:216` asserted `is_err()` on exactly that string, so the newly-added test would have failed the moment the gate opened. Same class: `"v1.2.3."`, `"v0.1.1."`, and `"v1.2.3.4.5.6.7.8.9"` (8 components + label `9`, silently reclassifying a numeric component as a label). Fixed by requiring the prerelease to start with a letter (`:56-71`); documented cost is that semver's bare-numeric prereleases (`v1.2.3-1`) no longer parse — nothing here publishes one, and it is precisely the shape indistinguishable from another numeric component. (2) The synthetic leading `0` fused a **build counter** with an **HHMM clock**: dispatch `v0.2026.08.21.0930` → `[0,2026,8,21,930]` outranked the real release `v2026.08.21.1` → `[0,2026,8,21,1]`, so everyone on a dispatch build silently never updated — the exact failure the fix claimed to cure, and both shapes come from the same workflow file (`.github/workflows/phosphor_release.yml:117-128`). Fixed by splitting `HHMM` onto its own hour/minute axis *below* a `DISPATCH_BUILD_COUNTER = 0` (`:191-265`), so a dispatch build is a pre-release of its date, superseded by that date's first numbered release and ordered among its own kind by time of day; `dispatch_clock_reading` (`:248-264`) distinguishes the shapes by four-digit fifth segment plus valid HH/MM, so a hand-cut `v0.2026.08.21.3` keeps counter semantics. `channel_versions_tests.rs:112,122` had **pinned the fusion as intended** and are corrected. (3) Prerelease ordering was string `cmp` while the doc claimed semver, so `-beta.10` sorted below `-beta.2`; now implements semver 11.4 (`:292-345`), and `to_ascii_lowercase` is removed so `-RC1` and `-rc1` stay distinct. Also closed the latent `Deserialize` hole: a hand-written impl routes through `new()` so a deserialized value cannot carry trailing zeros and violate the `Ord`/`PartialEq` invariant. **Method note:** the agent re-implemented both regexes and the compare path in Python and ran every real tag, every existing test and every repo version literal through them — the defects were confirmed by execution, not by reading.

- [x] 🔴 **No authenticity check on the downloaded artifact.** `mac.rs:473` deliberately
      skips `verify_code_signature` for Oss, and the only remaining check,
      `verify_oss_asset_sha256` (`mod.rs:53-70`), returns `Ok(())` on THREE separate
      absent-conditions (no cached release, asset name not found, no `digest`) — pattern
      #1, fails open. The digest also arrives in the same API response as
      `browser_download_url`, so it is corruption detection, not supply-chain integrity:
      no signature, no pinned key. Zero test references to `verify_oss_asset_sha256`.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `mac.rs:471-476` documents skipping `verify_code_signature`; `mod.rs:57,61,65` each `return Ok(())` on absent cached release / asset / digest. `github.rs:32-38` shows `digest` and `browser_download_url` in the SAME response — no independent key. Grep finds zero tests for `verify_oss_asset_sha256`.

      **FIXED 2026-08-21:** `mod.rs:41-115` — all three absent-conditions are now `Err`, the verified digest is returned, and `sha256_file` is new; `mac.rs:140-154,212-250,522-540` records the verified path+digest in `VERIFIED_OSS_DMG` and re-hashes before `open`, with `find_latest_dmg` demoted to a fallback that must itself pass verification. **The in-source comment states plainly that this is corruption detection, not supply-chain integrity** — there is still no signature check.

      **REFUTED THEN REPAIRED 2026-08-21.** The refutation upheld the fail-closed direction but found the fix incomplete in three ways, all now fixed. **(1) Two fail-OPEN siblings were left in the same file.** `get_curr_parsed_version()` ended in `.ok()`, fusing "parse failed" with "no version"; `is_incoming_version_past_current` returned `false` on either string failing to parse — and `false` there means "your version is not past the cutoff", so on any parse failure the user was told nothing was wrong. **The key insight, which one bool could not express:** the consumers split into two classes with **opposite** safe directions. The *deprecation banner* (`view.rs:19868,19893,19916`) needs Unknown ⇒ **true** (over-warning is recoverable — an error banner with "Update Phosphor manually" is already rendering there, so only the wording changes — while under-warning is not); the *prominent-update affordances* (`view.rs:8645,8657,8671,19413,19642`) need Unknown ⇒ **false**, because `true` there *removes* affordances. Hence `CutoffComparison` with five outcomes (`mod.rs:1437-1523`) and two deliberate bool projections; the five prominent-update sites were switched to `is_incoming_version_past_current_strict` (coordinator). The `warn_once` guard suppresses log repeats only — these run from render paths — and **never touches the verdict**, deliberately avoiding defect pattern 2. **(2) The fail-closed branch stranded users silently.** `UpdateReady::No` with only a `log::warn!` meant no banner and no manual-download affordance, and `current_version` is `option_env!("GIT_RELEASE_TAG")` while the release workflow triggers on any `v*` tag — so tagging `v1`, `v0`, `v0.1.1+build.5` or `v0.1.1_hotfix` **once** would have killed autoupdate for every installed client with no user-visible sign. New `UpdateReady::VersionComparisonFailed` maps to `AutoupdateStage::UnableToUpdateToNewVersion`, raising the existing error banner; the update is still refused, so no fail-open hole is reopened. The same mechanism now surfaces the **digest hard-fail availability trade** (a release published without a `digest` bricks updates) via an `UpdateBlocked` marker, escalating only blocked errors while transient network failures keep the quiet retry. **(3) The mac re-hash moved the TOCTOU rather than closing it** — `open` fires after this process exits, an app-shutdown plus up to 200 ms after the hash, on a path in the user-writable cache dir, and the doc claimed otherwise. The comparison is now **inside the deferred script**, between the pid wait-loop and `exec /usr/bin/open`; the other three options were considered and rejected with reasons (`open(1)` takes a path so an fd cannot be held; a private staging dir does not help against the realistic same-uid attacker; copy-after-hash is a several-hundred-MB copy that still sits user-writable across the same teardown) — only re-checking adjacent to the use removes the window. Residual is microseconds inside one shell. Also removed the `.exists()` filter (a second check-then-use), added a hex-shape guard, recovered the poisoned mutex instead of `.ok()`-ing it, and **rewrote the false doc claim** into an explicit statement of what is and is not guaranteed. **(4) The re-verify was mac-only.** Windows now re-checks the digest immediately before `cmd.spawn()` (`windows.rs:356-383`); `verified_sha256` is `None` on official channels, documented as "Authenticode stands here" and explicitly **not** a "verification skipped" value. Linux is documented as needing no second check, with the reason stated rather than assumed.

- [x] 🔴 **macOS verifies one file and executes another.** `oss_download_dmg` hashes
      `download_dir/<update_id>/<asset>` (`mac.rs:479`), but `relaunch` runs
      `find_latest_dmg` (`mac.rs:646,667`) — the newest `*.dmg` by mtime ANYWHERE under
      `cache_dir/autoupdate/`. The verified identity is never carried to `open`.
      **VERDICT PARTIAL — needs local write access (independent verifier, 2026-08-21):** Divergence real: hashed path is `dmg_path()` (`mac.rs:752-761`, used `:471,479`), executed path is `find_latest_dmg` over ALL subdirs by mtime (`:207,231-258`), no re-verification in `oss_open_installer`. But in the normal flow both resolve to the same file — failed downloads are deleted (`:481`) and `cleanup_all_except` runs (`:446`). Exploiting it requires local write access to the cache dir.

      **ALREADY CLOSED by `01112f35b` — verified 2026-08-21, no further edit.** The hashed file **is** the opened file and the identity survives the shutdown gap: `mac.rs:769-776` records `(path, digest)` in `VERIFIED_OSS_DMG` after verification (recovering a poisoned mutex rather than `.ok()`-ing it, which would have fallen back to the scan permanently); `resolve_oss_dmg` (`:250-285`) is the **single** resolver used by both the pre-flight re-hash (`:316`) and the script (`:426`), so they cannot disagree about which file they mean; the script binds `dmg=<quoted>` once and does `shasum "$dmg"` … `exec /usr/bin/open "$dmg"` (`:479-491`), leaving a residual window of microseconds inside one shell; and `find_latest_dmg` (`:496`) is demoted to a fallback that must itself pass `verify_oss_asset_sha256`. **Every line number in this entry is stale** (`:479`, `:646`, `:667`), as is "the verified identity is never carried to `open`" and the verifier's "needs local write access" note.

- [x] **`DECLINED.md:172` is factually wrong.** It states "This fork does not ship
      autoupdate… the release workflow publishes no update feed". `script/macos/bundle:351`
      and `script/windows/bundle.ps1:118` both set `autoupdate` for the OSS channel, and
      the workflow publishes the GitHub Releases that `github.rs` consumes as its feed.
      The decision recorded is not the code shipped.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `script/macos/bundle:351` and `bundle.ps1:118` add `autoupdate` to `FEATURES`, consumed by `cargo build --features` (`bundle.ps1:162`), and the workflow's `softprops/action-gh-release` step (`phosphor_release.yml:952-963`) publishes exactly the Release `github.rs:84` polls. Only the "not in Cargo.toml default" half is true.
      **FIXED 2026-08-21:** Row rewritten. The agent also caught a nuance I had missed: `autoupdate_ui_revamp` IS in `app/Cargo.toml` default but is a DIFFERENT feature from `autoupdate`, which is not — so the row's first clause was true and only the feed claim was false.

- [ ] **Linux OSS ships with autoupdate compiled out.** `script/linux/bundle:203` sets
      `FEATURES="release_bundle"` only, so all 523 lines of `autoupdate/linux.rs` are dead
      in shipped AppImages. Linux users receive no updates while mac and Windows do, and
      nothing tells them.
      **VERDICT PARTIAL — outcome right, mechanism wrong (independent verifier, 2026-08-21):** `autoupdate/linux.rs` is **not** feature-gated — `mod.rs:4-5` gates on `cfg(target_os = "linux")` only, so it compiles. What is off is `FeatureFlag::Autoupdate`: absent from `RELEASE_FLAGS` (`warp_features/src/lib.rs:864`) and added only under `#[cfg(feature = "autoupdate")]` (`lib.rs:2872`), so the runtime guards never fire. Linux gets no updates; the code is dead at runtime, not absent from the binary.

      **VERIFIED OPEN 2026-08-21 — accidental, NOT the recorded decision, and this entry's mechanism is wrong.** It is **not** covered by `DECLINED.md:215`, which is about `warp_tui/src/bin/oss.rs:42` hardcoding `autoupdate_config: None` plus the `192.0.2.0:9` sentinel — a different mechanism, and `DECLINED.md:179` explicitly warns not to conflate them. There is **no** row for the Linux **GUI** path. **Evidence it is an omission:** `160cfca59`, the commit that turned the feature on, touches `script/macos/bundle` and `script/windows/bundle.ps1` and **not** `script/linux/bundle`; the latter's `FEATURES="release_bundle"` (`:203`) was simply never revisited, while `macos/bundle:358` and `windows/bundle.ps1:125` both append `autoupdate`. **Mechanism correction:** `autoupdate/linux.rs` compiles fine (`mod.rs:4-5` gates on `cfg(target_os)` only) — what is off is `FeatureFlag::Autoupdate`, absent from `RELEASE_FLAGS` and added only under `#[cfg(feature = "autoupdate")]`. **Dead at runtime, not absent from the binary**, so "all 523 lines are dead in shipped AppImages" is wrong. **The Linux path is the strongest of the three:** it resolves the real asset URL from the cached release, verifies SHA-256 and `mv`s the verified bytes into place **with no await between**, so unlike mac and Windows it has no TOCTOU to re-check, and it detects AppImage vs package manager with a manual-install bail. Re-enabling is one change — `FEATURES="release_bundle,autoupdate"` — after confirming the published asset name against a real release, since `linux.rs:84` hardcodes it and a mismatch falls back to a constructed URL rather than failing loudly.

- [x] **Windows staged-installer reuse regressed against the pin.** `windows.rs:75` uses
      `already_exists = path.is_file()` where the pin requires `m.len() > 0`. With
      `rand_bytes(0)` the path is fully predictable, so a 0-byte or partial leftover is
      adopted as the installer and later spawned elevated (`windows.rs:346`).
      **VERDICT PARTIAL — not directly spawned (independent verifier, 2026-08-21):** The regression is real: `windows.rs:77` is `path.is_file()` where the pin uses `m.len() > 0` with the comment "Treat a 0-byte file as missing", and `rand_bytes(0)` makes the path predictable. But the reuse branch falls through to the SHA-256 check at `:153-158`, which sits OUTSIDE the `if !already_exists` block — so a partial leftover is rejected unless `verify_oss_asset_sha256` fails open (finding 75).

      **FIXED 2026-08-21.** Regression confirmed: the pin has `// Treat a 0-byte file as missing.` with `m.len() > 0` (`42effe840:windows.rs:48-50`); the fork had a bare `path.is_file()`, and `rand_bytes(0)` makes the path fully predictable. **The 2026-08-21 re-verify changed the picture for OSS only** — there the reuse branch falls through to `verify_oss_asset_sha256`, which now hard-fails and drops the `NamedTempFile` so the bad leftover is removed. **On the official channels it changed nothing:** `verified_sha256` is `None`, so `relaunch`'s re-check is explicitly skipped and the reused file is spawned by Inno with **nothing having examined it** — the macOS TOCTOU shape with a *wider* window, since the file sits in `%TEMP%` from download until the user clicks Install. Fixed in two parts: the pin's non-empty test is restored **with the fail-safe direction preserved** (a metadata error counts as *missing*, never as "safe to reuse"), and reuse is now permitted only on a channel that will check the bytes it reuses (`:126`). On official channels a pre-existing file is truncated and re-fetched over TLS — the weaker-but-real check those channels actually have — instead of adopting an unexamined, predictable-path file and running it elevated.

- [x] **Version tests are vacuous.** `channel_versions_tests.rs:90-120` asserts only
      synthetic tags (`v2026.05.26.2`) the pipeline never emits — and its own comment
      names the above failure mode as the thing it exists to prevent.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `channel_versions_tests.rs:90-129` exercises only `v2026.05.26.2`, `v2026.05.26`, `2026.05.26.2` — none of which the pipeline emits. Its own header (`:85-89`) says the tests exist so the function does not "keep returning Err, misleading users into upgrading to a rolled-back version" — the exact live failure.

      **ALREADY FIXED 2026-08-21 — no further work; this entry was simply never annotated.** The vacuous state no longer exists: `channel_versions_tests.rs` was reworked by `f0b71fe3e` and `7d025574d`, and the synthetic `v2026.05.26.*` shapes are gone from the crate (the only remaining occurrence anywhere is `crates/warp_tui/src/autoupdate_tests.rs:72`). Coverage was cross-checked against `git tag` and `.github/workflows/phosphor_release.yml:122-125` and now exercises the real shapes — semver tags, the beta tag, the dispatch-generated tag, dispatch-vs-numbered ordering, and the four-digit-clock discrimination. **Nothing was un-pinned by the deletion:** each of the three removed synthetic shapes has a real-tag equivalent (date+counter → `v2026.08.14.1-beta`; bare date → `v2026.08.21`; no-`v` prefix → `0.1.1`). The agent assigned this deliberately made no edit rather than duplicate another agent's committed fix.

- [x] **Stale docs in the bundle scripts.** `script/macos/bundle:350` and
      `bundle.ps1:117` name repo `zerx-lab/warp` (the code uses `jwp2987/phosphor`) and
      claim Inno Setup is not invoked — `windows.rs:348-357` invokes it.
      `settings/local_control.rs:8` cites the superseded pin `02b53fcd8`.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `script/macos/bundle:350` and `bundle.ps1:117` name `zerx-lab/warp` while `github.rs:13-14` uses `jwp2987`/`phosphor`. `bundle.ps1:117` claims "without invoking Inno Setup" but `windows.rs:347-357` spawns it with Inno switches. Both also claim downloads land in Downloads; mac uses `cache_dir/autoupdate/`, Windows a tempfile.
      **FIXED 2026-08-21:** Repo corrected to `jwp2987/phosphor`; download locations corrected (mac `cache_dir/autoupdate/<id>`, Windows a tempfile); the "without invoking Inno Setup" claim removed — `windows.rs:347-357` invokes it. `local_control.rs:8` re-pinned to `42effe840`.

### Refutation round — editor, notebooks, file edits

- [x] 🔴 **`/rewind` silently rewrites and DELETES files with no content check —
      fork-only.** The pin has no file revert at all
      (`git grep rewind 42effe840 -- crates/warp_tui` is empty).
      `warp_tui/src/tui_diff_storage.rs:215-259` writes `diff.base.content` — the
      snapshot taken at PREPROCESS time — back over Update/Delete targets, and for
      Create it **deletes the file**, never checking the file still matches what the
      agent wrote. Any user edit made after the AI edit is destroyed; an
      agent-created file the user has since built on is deleted.
      `revert_file_diffs:216-226` keeps only the sync `Result` and drops the returned
      `SaveFuture`, so async write failures are invisible and
      `terminal_session_view.rs:4151` reports "Rewound conversation and reverted file
      edits" regardless.
      **VERDICT PARTIAL — NOT fork-only (independent verifier, 2026-08-21):** Mechanism real: `tui_diff_storage.rs:233-257` writes `diff.base.content` back, Create → `PersistAction::Delete`, no staleness check; `:216-229` drops the `SaveFuture` and only `log::warn`s while `terminal_session_view.rs:4151` always reports success. **But "fork-only" is false** — the pin reverts identically: `42effe840:app/src/terminal/view.rs:25199` → `block.rs:4461` → `code_diff_view.rs:1033` → `local_code_editor.rs:2235-2277`, with base write-back, `std::fs::remove_file`, and no content check. Inherited.

      **CORRECTION + FIXED 2026-08-21 — and `0219e06c3`'s stated reason for deferring this was WRONG.** That commit recorded that the correct pre-image "needs it retained at accept time" and "the view does not retain it". **Nothing needed retaining:** `start_saving` wrote exactly `final_content_from_op(&diff.base.content, &diff.diff_type)`, a **pure function of the diff `revert_file_diffs` is already holding** — the accepted bytes were re-derivable all along. That mistaken premise is precisely why the delete limb went unguarded. `revert_plan` now returns a per-step `ExpectedDiskState` derived from the accept: `Create` → delete guarded on `Content(insertion)`; `Delete` → write base guarded on `Absent`; rename → write base at the source guarded on `Absent` **plus** delete the target guarded on `Content(accepted)`; in-place → write base guarded on `Content(accepted)`. **The delete limb is guarded** — an agent-created file the user has since built on now refuses instead of vanishing. `dispatch_write`'s `Option<ExpectedDiskState>` is **removed**, so there is no longer any way to ask this module for an unguarded write. **The dropped `SaveFuture` was load-bearing, not cosmetic:** guard refusals arrive *asynchronously*, so keeping only the sync `Result` would have made every refusal invisible and the whole guard inert. **`REVERT_CHAIN_TAIL` is necessary, not gold-plating:** the caller reverts newest-first and depends on that order, but `FileModel` writes run on spawned tasks so dispatch order ≠ execution order — unguarded that was a silent coin-flip between original and intermediate state, and **guarded it would have been a near-certain spurious refusal**, i.e. exactly the "refuse the common case" failure the old comment feared. **"Fork-only" is false** (the verifier was right: the pin reverts identically via `local_code_editor.rs:2235-2277`). **Not closed:** the GUI's `InlineDiffView::restore_diff_base` carries the same unguarded shape and the same now-refuted note.

- [ ] **Notebook load dead-ends on a `ServerId` that never exists (#609 sibling).**
      `notebooks/notebook.rs:1541-1560` — `fetch_needed` calls
      `notebook_id.into_server()`, but `set_server_id` (`cloud_object/mod.rs:172,673`)
      has zero callers, so ids stay `ClientId` and the arm is a `log::warn`.
      `fetch_needed` is also true when `focused_folder_id` does not resolve, so an
      in-memory notebook computed one line earlier is discarded with no toast.
      **VERDICT PARTIAL — impact overstated (independent verifier, 2026-08-21):** Confirmed: `cloud_object/mod.rs:172,673` has zero call sites (the fork's own `update_manager.rs:1229` says so) and `notebooks/notebook.rs:1542-1557` dead-ends in `log::warn` with no toast. But `focused_folder_id` is parsed ONLY from warp.dev Drive URLs (`uri/mod.rs:205`) and never set for local notebooks — territory DECLINED.md:200 (#267) deliberately keeps as dead code.

      **CORRECTION + FIXED 2026-08-21 — the reported mechanism was wrong.** There is no wait and no hang: `into_server()` returning `None` was already handled by an `else` arm, and `fetch_single_cloud_object` (`cloud_object/update_manager.rs:255-266`) is a gutted stub that fires its oneshot immediately, so even the `ServerId` branch cannot hang. **The real defect was that `else` arm**, in a fork where `SyncId::ClientId` is the only id kind objects ever get (`set_server_id` has zero call sites; `update_manager.rs:1229` already documents this and already collapsed two other `ServerId` guards for the same reason). Two failures behind it: (1) a notebook absent from the store with a `ClientId` — reachable with no link involved via `notebooks/manager.rs:189-199` on session restore — produced an empty pane with **no toast at all**, where every other terminal branch toasts; (2) a notebook already fetched from the store one line earlier was **discarded** because `fetch_needed` is also true when `focused_folder_id` fails to resolve, which it never can here. Fixed at `notebook.rs:1555-1581` via a new `NotebookLoadRoute::resolve` (`:2429-2465`); absent → `StoredObjectNotFound` toast, present → load. **`focused_folder_id` itself was left alone: `DECLINED.md:200` (#267) deliberately keeps that Drive-URL path dead** — no cloud behaviour was restored, the fix only stops it discarding a purely local object. The fork's port is byte-faithful to the pin (`42effe840:notebooks/notebook.rs:1502-1538`), and the pin's toast arm is unreachable even at the pin, so this is de-clouding breaking an already-vestigial shape, not a botched port. `cloud_object/mod.rs:171-183,686-688` now record that `set_server_id` has zero callers and that `#![allow(dead_code)]` is why nothing flagged it.

- [x] **Lost update on every AI edit (pin-parity).** `code/inline_diff.rs:137`
      registers with `subscribe_to_updates=false` and `:213-228` writes the whole stale
      snapshot buffer; `warp_files/src/lib.rs:797-830` writes unconditionally with no
      mtime or version compare. Accepting a diff minutes later clobbers concurrent
      external edits.
      **VERDICT CONFIRMED — inherited (independent verifier, 2026-08-21):** `code/inline_diff.rs:138` passes `subscribe_to_updates=false` and `:214-228` saves the whole editor buffer; `warp_files/src/lib.rs:797-830` does a bare `async_fs::write`. Verifier confirmed the pin is byte-equivalent (`42effe840:code/inline_diff.rs` same call, `42effe840:warp_files/src/lib.rs:725-758` also unconditional). A real lost-update, inherited.

      **FIXED 2026-08-21 — divergence AHEAD of the pin (all three limbs are pin-identical).** New `FileModel::save_if_unchanged(.., ExpectedDiskState, ..)`; read, compare and write happen inside one spawned task. **The `version: ContentVersion` parameter was never a concurrency check** — it is a process-global `AtomicUsize` that `report_save_outcome` records only *after* a successful write. **Content compare, not mtime, and the decisive reason is that mtime was never available:** the diff view uses `register_file_path`, which does not load, so there is no earlier `stat` to compare against; mtime is also preserved by `cp -p`/`rsync --times`. A digest was rejected because the full read is unavoidable either way. **Compared LF-normalised on both sides** — the editor stores the base normalised while the accept write emits the buffer's own inferred line endings, so a byte compare would have failed **every** accept on a CRLF file while protecting nothing. `ExpectedDiskState::Absent` covers `DiffType::Create`, and **local `NotFound` is the only failure that clears it**; every other read error refuses, so "could not read" never becomes "safe to overwrite". On conflict nothing is written, the version is **not** recorded, and the refusal flows through the existing `FailedToSave` → error-toast path. **`subscribe_to_updates=false` deliberately left as-is,** with reasons at the call site: flipping it buys nothing alone (the subscription ignores `FileUpdated`), would change live editor behaviour under the user, and is not a substitute since the watcher is 200 ms-debounced and advisory. **A guarded variant rather than changing `save`,** after auditing all 11 callers — most are legitimately unguarded, since a live buffer the user is typing into is not snapshot-derived.

- [ ] **The protected-path guard operates on unresolved paths and ignores rename
      targets (pin-parity).** `permissions.rs:1239` matches absolute MCP config paths,
      but `request_file_edits.rs:126-129` feeds it raw LLM strings, and
      `ParsedDiff::file()` returns the SOURCE, never `move_to`
      (`diff_application.rs:325-335`) — so a V4A rename auto-writes `~/.mcp.json`.
      **VERDICT PARTIAL — one limb refuted (independent verifier, 2026-08-21):** Rename limb confirmed: `ParsedDiff::file()` (`crates/ai/src/diff_validation/mod.rs:39-45`) never returns `move_to`, so `check_protected_write_paths` never sees the destination and `rename_and_save` writes it. Pin-parity. **The "unresolved paths" limb breaks:** `mcp/mod.rs:135-143` suffix-matches components, so a raw `~/.mcp.json` string IS caught. Only `~/.claude.json` escapes.

      **HALF FIXED 2026-08-21 — and the verifier's refutation of the paths limb was too NARROW.** It tested only `~/.mcp.json`. The mechanism is real and provable at the call site: `request_file_edits.rs:127-130` feeds the guard **raw model strings** while the writer resolves via `host_native_absolute_path` — the same module, six lines away, already importing it. The verifier was right about *scale*: it bites for exactly one provider, because `mcp_provider_from_file_path` matches **project** configs by suffix but **home** configs by absolute equality, and Claude is the only provider whose home name differs from its project name. Evasions confirmed by emulating `Path::components`/`ends_with` semantics: `~/.claude.json`, `.claude.json`, `../.claude.json`, `/home/u/tmp/../.claude.json` all escaped — while `/home/u/./.claude.json` did **not**, since Rust folds mid-path `.`. Fixed with tilde expansion plus lexical `.`/`..` folding (never `canonicalize` — it blocks, and fails on a not-yet-created file) and a home-config suffix match. **Renames limb still OPEN, same shape as the file-write rename hole found this morning:** `ParsedDiff::file()` returns the source for both variants and never `move_to`, and **the rename path consults no guard at all**. The fix belongs in `request_file_edits.rs` (add `move_to` to `paths`, resolved) or a `ParsedDiff::written_paths()`.

- [x] **Untrusted markdown link opens an OS handler on a plain click (pin-parity).**
      `notebooks/link.rs:147,275` accepts any scheme; `notebooks/editor/view.rs:1993`
      opens without a modifier in `Selectable` (LLM-authored) views; `lib.rs:1647`
      rewrites but never validates.
      **VERDICT CONFIRMED — inherited (independent verifier, 2026-08-21):** `notebooks/link.rs:154` returns `LinkTarget::Url` for any parsed scheme and `:276` calls `ctx.open_url`, reaching `Window::open_url` unvalidated (`warpui/src/platform/mac/delegate.rs:220`). `notebooks/editor/view.rs:1993` opens without a modifier in `Selectable`, which AI views set (`block/cli.rs:1265`, `ai_document_view.rs:747`). Pin identical.

      **FIXED 2026-08-21 — inherited from the pin, so this is a fix AHEAD of the oracle** (stated in-source so a re-pin does not revert it): `42effe840:notebooks/link.rs:147` returns `LinkTarget::Url` for any scheme `Url::parse` accepts and `:266` calls `ctx.open_url` unconditionally, and the pin's `view.rs` has the identical direct-open branch. Downstream is real — `AppContext::open_url` (`warpui_core/src/core/app.rs:5323`) → platform delegate → `open::that_detached`/`NSWorkspace.openURL`; **the wasm build already guarded this** (`crates/warpui/src/browser.rs:26`), the desktop build had nothing. **Allow-list:** `http`, `https`, `mailto`, plus `ChannelState::url_scheme()` — the first three reach a browser or mail composer and cannot run a local program with an attacker-chosen argument, and the app's own scheme returns to us where `WebIntent::try_from_url`'s `ALLOWED_ACTIONS` is a second gate; it must stay openable because the `lib.rs` rewriter deliberately produces it. Reading the scheme from `ChannelState` rather than hardcoding avoids the staleness already visible in `browser.rs`, whose list still says `warposs`/`zap`. **Refuse rather than confirm** — a dialog is a weak control against model-authored content and the file already set the precedent of silently downgrading to a safe action; refusal is made *visible* by reusing the existing `LinkState::Broken` affordance (`view.rs:2005-2027`) instead of a click that does nothing. Re-checked in `open()` too (`:410-421`), since `LinkTarget::Url` is publicly constructible without going through `resolve`. **Defect pattern 3 found and fixed en route:** `resolve_and_open` (`:487-500`) did `if let Ok(link) = resolved` and dropped the `Err` entirely, so a refusal — or any broken link — on the direct-open path was completely silent. **Plain click deliberately KEPT in `Selectable` views** (documented `view.rs:1993-2004`): with the allow-list in place a click can only reach a browser, mail composer or this app, the same guarantee every chat UI gives, and requiring a modifier would cost every generated document its one-click links with no affordance advertising it; the comment says to revisit if the allow-list widens. **Honest limitation that could not be fixed in scope:** `BeforeOpenUrlCallback` is `Fn(&str, &AppContext) -> String` and **cannot veto an open**, only rewrite — so `lib.rs:1661-1699` applies the rewrite only if its *output* passes the allow-list (the rewriter cannot launder a link), but a disallowed scheme arriving from elsewhere is logged and passed through. Returning `""`/`about:blank` was rejected as untestable platform-dependent behaviour on a hook that funnels ~40 other call sites.

### Refutation round — persistence

- [x] 🔴 **Rewound sub-agent tasks are resurrected on restart and re-sent to the
      model.** `persistence/agent.rs:117` — the pin's `upsert_agent_conversation`
      deletes every `agent_tasks` row for the conversation not in the snapshot
      (`42effe840:app/src/persistence/agent.rs:66,111-118`); the fork has no such step.
      Rewind prunes tasks in memory (`prune_unreachable_subtasks`) and persists only
      the survivors, but the stale rows remain and `read_agent_conversation_by_id`
      reloads ALL rows by `conversation_id`. **Content the user explicitly rewound away
      comes back after restart and is sent to the model again.** Upstream titles its fix
      "Fix conversation rewind re-sending rewound-away prompts and stale sub-agent tasks
      (#13072)". The fork's OWN generated ledger already records the miss —
      `docs/sweep/artifacts-2026-08-15/triage_out.txt:255` — and neither TODO.md nor
      docs/STATE.md tracks it.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** The pin genuinely deletes missing rows (`42effe840:persistence/agent.rs:63,107-116`, `kept_task_ids` + `ne_all`); the fork's `:85-116` has no such step, and the read path reloads all rows by conversation id unfiltered (`:376-379`). Rewind prunes only in memory (`conversation.rs:4295` → `task_store.rs:265`). The artifact line is verified: `triage_out.txt:254-255`.
      **FIXED 2026-08-21:** ported the pin's replace/delete-missing step — `kept_task_ids` from the snapshot plus a delete scoped by `conversation_id` and `task_id.ne_all(...)`, inside the existing transaction. `kept_task_ids` deliberately includes ids whose blob is skipped as oversized, since deleting those rows would discard the last stored copy.

      **REFUTATION 2026-08-21 — SOUND.** The only wave-3 fix to survive. diesel 2.3.10 `array_comparison.rs:98-101` emits `1=1` for an empty `NotIn` and SQLite selects that dialect (`sqlite/backend.rs:68`), so the comment holds; `ux_agent_tasks_task_id` makes `task_id` globally unique, so the `conversation_id` filter can only narrow. Byte-identical to the pin, retention 200 included, no dependants on 100. Only gap: no test coverage.

- [x] **Agent-history retention was silently halved.** `persistence/agent.rs:46` is
      `100`; both `42effe840` and the older pin are `200`, and the fork also deleted the
      sentence justifying the number. Introduced by `9840d7d52`, whose message claims a
      faithful port. Rows are DELETEd, so users lose about half their retained agent
      conversations permanently. All eight eviction tests pass an explicit `limit`, so
      none binds the constant.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `persistence/agent.rs:46` is `100`; BOTH pins are `200` (`42effe840:...:45`, `02b53fcd8:...:45`) with the dropped "10–40 orchestration sessions of headroom" sentence. Verifier ran `git log -S`: `100` entered at `9840d7d52` and `200` never existed here. Eviction is a real `diesel::delete`, and all eight tests pass literal limits — never the constant.
      **FIXED 2026-08-21:** restored to `200` with the justifying sentence. No deliberate reason for `100` was found — sole usages are in-file, no test pins it, and `git log -S` confirms it entered at `9840d7d52`.

- [ ] **`MAX_TASK_BLOB_BYTES`' doc asserts a guard that does not exist, and the
      write-side skip corrupts conversations.** `agent.rs:13-16` says tasks over 10 MB
      are "skipped on both write and read"; the constant appears only at `:91` (write).
      Both read paths decode unconditionally, so the stated startup-OOM protection is
      absent for exactly the pre-cap rows it targets. Worse, the `continue` at `:99`
      drops the task while the summary — derived from the full list including
      `is_restorable` — is still written, so restore sees a conversation with a missing
      (possibly root) task and silently promotes a child to root.
      **VERDICT PARTIAL — consequence wrong (independent verifier, 2026-08-21):** Doc mismatch confirmed (`agent.rs:13-16` claims read-side skipping; both read paths decode unconditionally at `:275`, `:383`), and the constant is fork-invented — absent from the pin. **But the claimed corruption is wrong:** a skipped root leaves no parentless task, so restore returns `RestoreConversationError::NoRootTask` (`conversation.rs:536-543`) and orphans are dropped with `log::error` (`:523-528`). No child is promoted to root.

      **DOC FIXED 2026-08-21; READ-SIDE GUARD DECLINED, with the migration proposed.** Confirmed, plus a third thing wrong with the old sentence: the read it describes — "when all task records are loaded at once" — **has not existed since #431**, when startup moved to `read_agent_conversation_metadata`, which reads the `summary` column and touches `agent_tasks` only for pre-column rows. **So the protection was claimed for precisely the rows it does not cover.** What actually happens over the limit is neither truncation nor a failed write but a **silent skip that leaves the stale row** — `kept_task_ids` deliberately retains the id so the row is not deleted, so the DB keeps the last version that fit and restore hydrates a stale copy; if no version ever fit the task is absent, and a missing root gives `NoRootTask`. **Meanwhile `summary` is still derived from the full in-memory snapshot including the skipped task, so `is_restorable: true` is persisted — the conversation is listed in history and fails when opened.** Read enforcement **withheld deliberately**: a size check before `decode` would skip exactly the blobs written before this constant existed, and since `is_restorable` is what startup filters on and eviction deletes rows, it would turn "restores slowly" into "silently vanished from history". Migration shape recorded in the doc. **Also considered and rejected:** deriving `summary` from only the tasks on disk — it would make `is_restorable` honest but trade a *visible* restore failure for silent disappearance plus eviction eligibility. The skip log is promoted `warn` → `error` and reworded, since it is the only notice anyone gets that a turn was dropped.

- [x] **The macOS legacy-DB migration looks in a directory that can never exist, then
      records success forever.** `persistence/sqlite.rs:610` builds the legacy App Group
      path from the CURRENT app id (`dev.phosphor.Phosphor`); the data it is meant to
      rescue was written under the OpenWarp/Zap ids. It then writes
      `.zap-app-group-sqlite-migrated` so the miss is permanent. The three tests inject
      `legacy_dir` by hand, so they pass with the broken path computation.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `persistence/sqlite.rs:604-612` joins `WARP_APP_GROUP_ID` with `ChannelState::app_id()`, now `dev.phosphor.Phosphor`. Verifier traced the origin: introduced at `03ce9dcbb` as `openwarp_legacy_app_group_sqlite_dir` when `app_id()` WAS `dev.openwarp.OpenWarp`, and the renames at `a00ae6cfd` stranded it. The marker is written on miss (`:626-628`), and the tests inject `legacy_dir` so they never exercise the path builder.

      **FIXED 2026-08-21.** Historical app ids **established from git history, not guessed**: `dev.openwarp.OpenWarp` (commit `03ce9dcbb`, PR #76, is the commit that *introduced* this migration, so the id current at that moment is the directory the data actually sits in — and the same commit stopped OSS builds writing there, making it terminal) and `dev.zap.Zap` (commit `a00ae6cfd`, whose diff is literally `-AppId::new("dev","openwarp","OpenWarp")` / `+AppId::new("dev","zap","Zap")`). Warp's own `dev.warp.*` ids are deliberately excluded — that container belongs to a different application. `LEGACY_APP_GROUP_APP_IDS` (`sqlite.rs:152-175`, newest first) carries the commit-level provenance; the `ChannelState::app_id()` join is gone. **SUPERSEDED 2026-08-21 — the marker rule below was REFUTED and is wrong; see the correction after it.** `init_db` creates the target database on the same launch, so a later launch judged the rescue against a file this code had just created, and the mtime comparison rejected it **every time** — a permanent wrong answer that merely *looked* like an open question. It is replaced by a versioned `v2` marker recording the miss **and** whether the target existed at the first empty scan; the discriminator is that recorded provenance, not mtime (mtime survives only as the no-marker fallback, the one case it can still answer honestly). Retries are bounded to 3 and **only fresh installs retry at all** — an install that already had a database when the search first came up empty can only ever lose a later comparison, so re-scanning has a foreordained answer; that keeps the `03ce9dcbb` / `test_oss_secure_state_dir_is_disabled` per-launch App Group read from coming back for the overwhelming majority of users. `v1` markers have no version prefix, so a fixed build re-evaluates once. Copying is now non-destructive (the live DB **and its sidecars** are renamed aside), which also fixes a latent corruption: the old code copied the main file while leaving the stale `-wal` beside it. **Third historical id `dev.warp.WarpOss` added, and the exclusion reasoning I approved was wrong about exactly it** — `0dbd3d567` has `AppId::new("dev","warp","WarpOss")` at `app/src/bin/oss.rs:16` and `identifier = "dev.warp.WarpOss"` at `app/Cargo.toml:934`, both under `Channel::Oss`, so the OSS lineage was WarpOss → OpenWarp (`f41f4bfac`) → Zap → Phosphor; the comment now excludes upstream's *commercial* channels by name rather than treating the `dev.warp.` prefix as the test. **`dev.zap.Zap` is provably a no-op** — the App Group gate landed nine days before that rename — kept defensively but labelled, not left asserting data sits there. New `test_..._rescues_a_source_that_arrives_after_the_first_launch` fails against `e0c3dfe2f` **and** against the blanket-marker version before it, for opposite reasons. **Superseded original reasoning follows:** no legacy database found in *any* candidate → **no marker**, because nothing was migrated and the source can still appear later (Migration Assistant, a Time Machine restore still running, a home copied to a new Mac), and the re-check costs two `exists()` calls per launch — absence is indistinguishable from "not restored yet", so claiming terminality would be a guess; legacy database found but **rejected** (live db is newer) → marker written, because a real decision was made and re-litigating it each launch risks clobbering active data. **The vacuous-test half is fixed too:** the path computation was extracted into the pure `legacy_app_group_sqlite_dirs_for_home` (`:629-644`) so it is testable without the filesystem, and the new `test_legacy_app_group_sqlite_dirs_target_historical_app_ids` asserts no candidate ends with `ChannelState::app_id()` — the assertion that fails against the old code. Also new: newest-id preference, older-id fallback, no-marker-on-miss-then-rescued-later, and marker-on-rejection. **The pin has nothing here** — `git grep -n "app_group\|legacy" 42effe840 -- app/src/persistence` returns only unrelated `legacy_ts` hits; this migration is fork-specific and the rename it depends on has no upstream counterpart. **Unrun:** the new tests are `#[cfg(target_os = "macos")]` and this host is Linux.

### Refutation round — codebase indexing / LSP

- [x] 🔴 **Source code can be sent in cleartext.** `agent_providers/embeddings.rs:243-254`
      and `:439-450` check `is_plaintext_bearer_risk` **inside** `if !api_key.is_empty()`.
      A keyless provider on an `http://` non-loopback endpoint therefore POSTs every
      code fragment unencrypted with no guard at all.
      **VERDICT PARTIAL — severity overstated (independent verifier, 2026-08-21):** The nesting is confirmed (`embeddings.rs:244/247` and `:439/442` gate `is_plaintext_bearer_risk` inside the key check), so a keyless `http://` non-loopback provider posts fragments unencrypted. But the guard is scoped to CREDENTIAL exfiltration by name and comment (`mod.rs:72`), the chat path behaves identically, and the endpoint is user-configured with no TLS promise anywhere. Not a bypass.

      **FIXED 2026-08-21 — and it was LIVE; today's rework did not close it.** The line citations were stale (`5ebf2d082`/`b09c024f6` moved the sites ~145 lines down to `embeddings.rs:388`/`:583`) **but left the nesting intact**. The verifier's "not a bypass, the guard is scoped to credentials by name" is right about intent and **wrong as a disposition**: `is_plaintext_bearer_risk` is defined as "would sending the BYOP API key put it on the wire in cleartext", and both call sites sat inside `if !endpoint.api_key.trim().is_empty()` — so **a keyless provider, which is the normal self-hosted configuration and the one most likely pointed at a non-loopback host, had no guard at all for the payload.** Split into two rules of deliberately different widths, neither nested in the other: the **credential** rule (`https`/loopback, unchanged wording because a test asserts on it) and a new **payload** rule (`https`/loopback/RFC1918/IPv6 ULA/link-local), because a LAN self-hosted embedding server is a legitimate deployment for *code* even though it is not acceptable for a *key*. **Credential reported first because it refuses a strict superset** — an invariant now pinned by its own test. Bare hostnames are deliberately **not** treated as private (fail-closed, no DNS lookup, matching `is_loopback_host`). **Daemon parity holds by construction:** `remote_server/codebase_index_store.rs:509` builds the same provider and shares the request path. Four doc comments in `codebase_embeddings.rs` that named the bearer guard as the sole reason no request escapes were corrected.

- [x] 🔴 **`script/check_cloud_boundary` under-enforces — 10+ live violations it cannot
      see.** Its regex misses `use crate::{server::…}` brace form (live at
      `ai/blocklist/controller.rs:75` and several `code/editor/find/view.rs` variants) and
      inline references (`root_view.rs:488`), contradicting its own header. This is one
      of the two guards CLAUDE.md names as enforcing what the compiler cannot.
      **VERDICT CONFIRMED — 161 sites (independent verifier, 2026-08-21):** `check_cloud_boundary:45` matches only `^\s*use crate::(server|cloud_object)::`. Verifier tested empirically against a scratch file: the brace form does not match. **161** multi-line `use crate::{` + `server::`/`cloud_object::` sites are invisible, plus `ai/blocklist/controller.rs:75` — and the header at `:22` claims to cover exactly that form. One cited example was wrong: `code/editor/find/view.rs:10` is plain-form and allowlisted.
      **FIXED 2026-08-21:** Matcher widened from `^\s*use crate::(server|cloud_object)::` to all three forms — plain, `pub use`, and the `use crate::{...}` brace form including multi-line, via awk since no line-based regex can span the latter. **169 previously-invisible sites** surfaced, so the allowlist understated the retained cloud surface by 62% (273 → 442). Rebaselined with a header recording that these are pre-existing, not new. Proved by probe: all four forms are now caught and an unrelated import is correctly ignored.

- [x] **LSP servers are never shut down, and `TODO.md:672` claims the hook is wired.**
      `lib.rs:2360-2392` (`on_will_terminate`) goes straight from `writer.terminate()` to
      `PtySpawner`; the pin calls `lsp::LspManagerModel::terminate(ctx)` there
      (`42effe840:app/src/lib.rs:2691`), and `crates/lsp/src/manager.rs:306` has zero
      callers. No shutdown/exit handshake, so on autoupdate the new app spawns while old
      rust-analyzer/gopls children are still resident — exactly what the surrounding
      comment claims to prevent. **Fifth TODO entry found stating the opposite of the
      code.**
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Fork `lib.rs:2360-2392` goes `PersistenceWriter` → `PtySpawner`; the pin has `lsp::LspManagerModel::handle(ctx).update(...manager.terminate(ctx))` at `42effe840:app/src/lib.rs:2692-2694`. `crates/lsp/src/manager.rs:306` has zero callers repo-wide, and TODO.md:673 lists "terminate hook" under Done. The only residual path is `Drop for LspServerModel` (`model.rs:745-747`), whose terminate merely detaches an async shutdown.
      **FIXED 2026-08-21:** TODO entry moved out of Done into an open item stating the real state. The missing `LspManagerModel::terminate` call itself is NOT yet fixed — that is a code change, queued for a later wave.

- [x] **The embedding endpoint is frozen at startup.** `build_store_client` resolves once
      at `lib.rs:2209`; `set_endpoint` is called only by the daemon. Its own doc claims
      refreshability. The sibling LLM path subscribes to both `AISettings` and
      `AgentProviderSecrets`; indexing subscribes to neither, so configuring a provider
      after launch leaves the settings page reporting the model live while indexing
      returns `NoEmbeddingProvider` until restart. Key rotation is ignored the same way.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `lib.rs:2209` resolves the client once inside `add_singleton_model`; `set_endpoint` has exactly one non-test caller (`remote_server/codebase_index_store.rs:383`) while `embeddings.rs:132-134` claims refreshability. **Correction to the original:** no `AgentProviderSecrets` subscription exists anywhere — the LLM path stays fresh by re-resolving per request (`chat_stream.rs:4292-4308`), not by subscribing.

      **FIXED 2026-08-21.** New `RefreshingStoreClient` (`codebase_embeddings.rs:356-598`) assembles a fresh `LocalStoreClient` per call from an `HttpEmbeddingProvider` + `SqliteVectorStore` + a `Mutex<StoreClientConfiguration>`; `refresh_from_settings` re-resolves endpoint, model and reranker, and **sets the endpoint unconditionally rather than only on a model change — that is the key-rotation case.** Rebuilding per call also fixes model switches, since `full_sync` asks `codebase_context_config()` at the start of every sync. Driven from `lib.rs:2282-2372` by **extending** the 2026-08-21 `CodeSettings` block rather than adding a second mechanism, now with three subscriptions: `AISettings`, `CodeSettings`, and **`AgentProviderSecrets` — a second, separate hole**, because the pre-existing remote block subscribed to the first two only, so a rotated BYOP key was never re-pushed to connected daemons either. **Ordering checked rather than assumed:** `add_singleton_model` invokes its closure immediately, so "registered before" and "constructed before" are the same instant; every singleton the refresh reads is already registered at that point, and moving the client build one statement outside the closure changes nothing about what it reads. **The false doc claim is now true** — it had said the client could be refreshed "when the user edits their providers", which was true of the type and false of the app. Tests go through the `StoreClient` trait, not `set_endpoint`, because the existing `set_endpoint_replaces_a_missing_one` passed for the entire time the defect was live.

- [x] **Index tables grow forever.** No DELETE or TTL for
      `codebase_index_{nodes,embeddings,node_summaries}` anywhere; snapshots get 30 days
      and `ai_queries` gets trimming, these get nothing.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `persistence/sqlite.rs:1910-2008` only upserts the three tables, and every `diesel::delete` in the file targets others; `delete_codebase_index_metadata:1891-1897` removes only the `workspace_metadata` row, leaving that repo's nodes/embeddings/summaries behind. Verifier checked the comparators too: snapshots DO expire at 30 days (`index/full_source_code_embedding/snapshot.rs:26-31`) and `ai_queries` IS capped (`block_list.rs:169-202`).
      **FIXED 2026-08-21:** **REFUSED, documented instead.** The three `codebase_index_*` tables are keyed `(embedding_space, hash)` where `embedding_space` is the embedding MODEL, not a workspace; there is no repo column and rows are content-addressed, so reachability is shared between repos. Per-workspace deletion is unimplementable without a schema change, and deleting by space would wipe every other repo's index. Documented at `sqlite.rs:1891-1910`, including the absent TTL. Note the only caller of `delete_codebase_index_metadata` is itself `#[allow(dead_code)]`.

- [x] **File outlines fail instead of degrading.** `file_outline/native.rs:57` uses
      `FailFast` where the pin uses `StopAndLazyLoad`, so over-budget repos now get NO
      outline. The enum's own doc says `FailFast` is for embedding only. Undocumented
      divergence.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `index/file_outline/native.rs:57` passes `BudgetExceededBehavior::FailFast` where `42effe840:...:58` passes `StopAndLazyLoad`, with no comment explaining the change. Chain traced: `entry.rs:279-281,386-388` returns `ExceededMaxFileLimit` under `FailFast`, `build_outline:51-59` propagates with `?`, and the sole caller `ai/outline/native.rs:220` passes `Some(5000)` — so repos over 5,000 files get NO outline instead of a partial one.
      **FIXED 2026-08-21:** `crates/ai/src/index/file_outline/native.rs:58-64` restored to the pin's `StopAndLazyLoad`, with a comment.

- [x] **The pin's index consent banner was dropped with no DECLINED.md row**, and
      `codebase_context_enabled`'s default was flipped `true`→`false`
      (`settings/code.rs:67`) under a comment claiming a faithful restore.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `42effe840:app/src/ai/blocklist/codebase_index_speedbump_banner.rs` is 357 lines implementing an "Index Codebase?" consent with allow / always-allow / don't-show-again, and has no counterpart here — grep for the module or `CodebaseIndexSpeedbump` returns nothing, with no DECLINED.md row. The default flip is confirmed too: pin `:17` is `true`, fork `settings/code.rs:67` is `false`, under a comment disclosing only the `AdminEnablementSetting` omission.

      **RESOLVED 2026-08-21 — declined, with evidence, and the row is now in `DECLINED.md`.** The banner's consent half is **unreachable at the pin**: one insertion site, one caller, and that caller passes `show_is_indexing = true`, so the "Index Codebase?" prompt never renders; `/index` calls `index_directory` **before** inserting the banner, i.e. the pin indexes first and reports after. Its feature flag gates a function that touches a different banner, and both persisted dismissals are written but never read as a display gate. So the "consent is already expressed by the opt-in setting" argument does not merely hold — porting would import a UI whose consent path was dead at the source. **The `true` → `false` default flip stands** (opt-in is what the BYOP key gate depends on) but the false comment above it is fixed: it claimed a faithful restore naming only the `AdminEnablementSetting` drop, hiding the load-bearing divergence. It now enumerates all three, says the default is deliberate and why, and ends with an instruction not to flip it back without moving the consent elsewhere first. A stale `02b53fcd8` pin citation in the same comment was corrected to `42effe840`.

- [x] **Indexing UI is untested and its test doc is false.** `code_page_tests.rs:68`
      says the flag "is on by default in this fork" — it is not — and both tests force it
      false, so no test ever renders the three widgets.
      **VERDICT PARTIAL — 'untested' overstated (independent verifier, 2026-08-21):** The doc IS false — `code_page_tests.rs:68` says `FullSourceCodeEmbedding` "is on by default in this fork" when it is `DOGFOOD_FLAGS`-only and unreachable. And no test renders the three widgets: the rendering tests force the flag false (`:104`, `:132`) and the rest never override it. **But "untested" is overstated** — `:221` and `:245` exercise both toggles' write-through via `handle_action`.

      **FIXED 2026-08-21.** The false half: `FullSourceCodeEmbedding` is `DOGFOOD_FLAGS`-only (`warp_features/src/lib.rs:855`) and that list **enables nothing at runtime in this fork** (`:800-819`), its one enable path being `ZAP_UNSTABLE_FEATURES` — so "on by default in this fork" was wrong, and the callers' `false` override is a pin against a dev shell rather than a correction of the shipped default. The *rest* of the old comment was true and is kept. **The vacuity trap was checked for and does not apply here:** `PageType::update_filter` (`settings_page.rs:1412-1430`) evaluates each widget's `should_render` **before** `search_terms_match`, so a filter query is a genuine read of the gate — unlike the hardcoded `active_window_id()` case that made an earlier test vacuous. New `the_indexing_rows_appear_only_behind_the_indexing_flag` asserts all three rows absent with the flag off, present with it on, and auto-indexing additionally gated on codebase context. **Still uncovered, stated in the test doc rather than implied:** the widgets' `render` bodies — `PageType` owns the boxed widgets privately, so the only way in is rendering the whole page, which would drag in singletons that cannot be verified without a build. The test proves installation and gating; a row that panics while drawing would still pass.

> **Coverage gap in this round:** three areas were briefed but never ran, rejected at
> the 20-agent concurrency cap — **warpui / warpui_core**, **guards + CI**, and
> **util / uri / themes / workspaces / lib.rs**. Nothing below is known about them.

### Refutation round — util/uri/workspaces, and the ROOT CAUSE of the de-clouding class

- [x] **`#![allow(dead_code)]` blanket-silences the `app` crate — real, but much
      narrower than first logged.** `app/src/lib.rs:4`, absent from the pin, with the
      comment "Orphaned code left over from upstream Zap trimming is temporarily kept".
      **CORRECTION (verified 2026-08-21):** the original entry here claimed this was the
      root cause of the de-clouding defect class and that rustc "would have named
      `initialize_default_regexes_once` and `handle_user_fetched` on every build".
      **That is false.** `app` is a lib crate and `pub mod settings;` (lib.rs:131) is
      PUBLIC, so both functions are publicly reachable and `dead_code` never fires on
      them — with or without the attribute. Neither of the two severe de-clouding finds
      would have been surfaced by removing it.
      What DOES hold: `mod ui_components` (:89), `mod uri` (:91) and `mod workspaces`
      (:103) are private, so orphaned `pub fn`s inside those three would be flagged —
      `uri/mod.rs:1345 open_window_with_action` is a confirmed instance. That is a
      tidiness win over three modules, not a mechanism that catches the class.
      **The real lesson is the opposite one:** the de-clouding orphans are mostly in
      PUBLIC modules, where no compiler lint will ever find them. Finding them needs
      call-graph analysis, not a lint. Sized accordingly, this is a low-priority cleanup.
      *(Logged first as 🔴🔴 "highest-leverage item in this file" on the strength of an
      agent's framing plus a two-line check that the attribute exists — without checking
      whether it does what was claimed. Recorded here rather than quietly edited, because
      it is the same error this round keeps finding in others.)*
      **VERDICT CONFIRMED (the correction is right) (independent verifier, 2026-08-21):** The correction holds and the verifier strengthened it: `settings/mod.rs:31` `mod privacy;` is private BUT `:73` `pub use privacy::*;` re-exports it, and `:17` `pub mod initializer;` exposes `SettingsInitializer` — both publicly reachable via `lib.rs:131`, so `dead_code` cannot fire on either. `lib.rs:4` is confirmed absent from the pin. **Two errors in the ORIGINAL finding also surfaced:** `open_window_with_action` is at `uri/mod.rs:1183`, not :1345, and is a PRIVATE `fn`, not a `pub fn` — so even the one example offered was miscited.

      **SIZING ATTEMPTED 2026-08-21 — CANNOT BE DONE WITHOUT A BUILD; attribute left in place, and the correction above was itself incomplete.** The premise that this covers three private modules is wrong: **84 of the 108 module declarations in `lib.rs` are private** (only 24 are `pub mod`), so the blanket covers the entire private half of the crate, and "replace it with narrower per-module allows" cannot be scoped to three modules. Two modules already carry their own narrower `#[allow(dead_code)]` (`context_chips:24`, `remote_server:69`), which is evidence someone previously found those noisy enough to need it. For scale only, not as a warning count: the three named modules alone are 33 files / ~9,100 lines with ~256 `pub` items, and `dead_code` is **reachability**-based, so no grep can tell you which are live — a `pub fn` referenced only from another dead function is still dead. The one confirmed orphan checks out but at a different line than recorded: `open_window_with_action` is `uri/mod.rs:1183`, not `:1345`. A 7-line comment at `lib.rs:3-10` now records that it is absent from the pin, that it is an 84-module problem, and that the swap must be sized with a build first.

- [x] 🔴 **A LIVE vacuous test masks the sandbox bypass.**
      `permissions_test.rs:1253-1309` injects `AlwaysAsk` via
      `UserWorkspaces::update_ai_autonomy_settings` into `workspace.teams[0]`, then
      asserts that sandboxed mode "bypasses the workspace restriction". Every read path
      goes through `current_team()`, which is hard-`None`, so the restriction never
      existed and the test passes with the bypass deleted. **Its eight siblings were
      `#[ignore]`d for exactly this reason — this one was missed.**
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** Verifier checked the deletion counterfactual: both arms of `workspace_autonomy_settings` (`permissions.rs:233-243`) yield `apply_code_diffs_setting`/`read_files_setting` = `None` — the sandboxed arm via `..Default::default()`, the other via `current_team()` → `None`. `determine_write_permissions_from_active_profile` (`:863-878`) then returns the profile's `AlwaysAllow` either way, so **removing the sandbox branch leaves the test green**. Eight `#[ignore]`s carry that reason; this one lacks it.

      **RESOLVED 2026-08-21 — `#[ignore]`d, matching its eight siblings.** File is `app/src/ai/blocklist/permissions_test.rs`; the test is `test_sandboxed_mode_allows_read_write_files` (`:1253-1310`). **Deletion counterfactual confirmed:** in `permissions.rs:229-243` the sandboxed arm yields `AiAutonomySettings { execute_commands_denylist: None, ..Default::default() }` and the unsandboxed arm yields `AiAutonomySettings::default()` — all eight fields are `Option`, all `None`, so **the two arms are the same value** and deleting the whole `if is_sandboxed` block is a fork-wide no-op the assertion cannot observe. `current_team()` is hard-`None` by `0387ae832` ("Remove cloud team and analytics entrypoints"), and the test's `update_ai_autonomy_settings` write lands in `workspace.teams[0].organization_settings`, which nothing in the fork reads; it was green only because `apply_cli_profile_defaults_for_test(.., true, ..)` sets the profile to autowrite/autoread. **Not a §5.6 violation** — the test asserted a property that does not exist, so this removes a false green rather than weakening a real signal. "Make it real" was unavailable (it needs un-stubbing `current_team()`, which is the de-cloud decision itself); deletion was rejected because the test is a verbatim port of the pin's `permissions_tests.rs:1352` and keeping it ignored preserves the re-port target. **The bypass itself is SOUND and must not be changed** — `permissions.rs:229-243` is byte-identical to `42effe840:permissions.rs:229-243`; upstream's intent is that in sandboxed mode the sandbox provides containment, so workspace overrides drop and only the org sandbox denylist survives. It is dead here but is the correct shape if `current_team()` is ever un-stubbed. Recorded as a `DECLINED.md` row. **Residual coverage loss, deliberately not papered over:** the test incidentally covered "sandboxed CLI profile defaults ⇒ Autowrite/Autoread enabled", and no other test uses `initialize_permissions_test_sandboxed`. A replacement would have to be a new test that never touches `update_ai_autonomy_settings`; not invented here.

- [x] **Tautological test, and the only one in `app/src/workspaces/`.**
      `user_workspaces.rs:922-928` asserts `is_ai_allowed_in_remote_sessions()`, whose
      body at `:734` is a bare `true` that never touches `self`. The pin ships four test
      files in that directory. `check_stub_coverage` did not catch it.
      **VERDICT PARTIAL — 'only one' false (independent verifier, 2026-08-21):** The tautology is confirmed: `user_workspaces.rs:734-737` is a bare `true` ignoring `self`, where the pin's version at `:1753-1762` reads `current_workspace()`. Verifier also explained why the guard misses it: `check_stub_coverage`'s `TRIVIAL` regex requires the body on the `fn` line and this one spans a comment. **But "the only test in that directory" is false** — `user_profiles.rs:116,132` holds two.

      **RESOLVED 2026-08-21 — and the important part is not in this entry.** "Only one in that directory" is false (`user_profiles.rs:116,132`). **The decisive fact:** the pin's body reads `current_workspace()`, which is **fully live in this fork** — so unlike its `current_team()`-bound neighbours, this function never needed stubbing on reachability grounds. **It must stay stubbed anyway:** `allow_ai_in_remote_sessions` derives `Default` → `false` and arrives from Warp's server, with nothing local writing it and no UI exposing it, so restoring the pin's body would return `false` for **every user holding a cached workspace** — silently disabling the Agent in remote sessions with no way back on. Now a `DECLINED.md` row, which did not exist. The vacuous test is replaced by one that builds a *selected* workspace saying remote AI is off and asserts `true` anyway, so it **goes red exactly if the pin's body is restored**, plus two real tests of the live selector. The module goes from one test that cannot fail to three that can.

- [x] **`zap://settings` (bare) is dead.** `uri/mod.rs:261,279,335` dropped the pin's
      `.filter(|s| !s.is_empty())` and the `OpenSettingsArgs::Default` / `?q=` routes,
      both non-cloud. The comment at `:1370-1379` claims parity "minus the two cloud
      branches" — false.
      **VERDICT PARTIAL — comment claim wrong (independent verifier, 2026-08-21):** Route breakage confirmed: `uri/mod.rs:261-266` drops the pin's `.filter(|s| !s.is_empty())` (`42effe840:app/src/uri/mod.rs:390`), and the fork has no `OpenSettingsArgs` at all — the pin's bare-URL `Default` (`:486`) and `?q=` search (`:448,468`) branches are gone, leaving a `log::warn!` at `:335`. **But the comment at `:1367-1376` is accurate**: it describes `settings_section_for_simple_subpage`, whose pin version really does have four arms, two cloud. The dropped routes are in the caller, not that function.

      **FIXED 2026-08-21 — reachability is honestly "only theoretically".** Verified dead: the bare form yields one *empty* path segment, so without the pin's `.filter(|s| !s.is_empty())` it arrives as `Some("")`, matches no arm and hits a `log::warn!`. **No documented link, menu item or support instruction produces it** — nothing outside code comments references it, and `web_intent_parser.rs:79-93` structurally cannot emit it (it requires two segments and a non-empty query). Fixed anyway because it is the most guessable form of a scheme the fork does register, the change is ~10 lines, and **it adds no capability**. **Why the landing is safe**, stated in-source given the `UriHost::Launch` precedent: it reaches exactly two `WorkspaceAction`s, both confined to the settings pane — `ShowSettings` takes **no parameters at all**, and `ShowSettingsPageWithSearch` takes one `String` used only as search-filter text. Neither starts a session, runs a command or touches a path, and both are already reachable without a URI from the Settings menu and from `local_control`'s `surface_settings_open`, which builds the identical pair from the identical inputs. The ledger's claim about the comment at `:1370-1379` is wrong (the verifier caught it) — that doc describes a different function and was left alone. **Deliberately not done:** `?q=` on a *sub-page* URL, since at the pin a query there silently discards the section and reproducing that would change a route that currently works.

- [x] **`external_editor/linux.rs:88` parses a Desktop-Entry `Exec` with
      `shell_words::split`** where the pin uses a purpose-built `tokenize_exec`.
      shell-words adds `SingleQuoted`/`Comment` states the Desktop-Entry spec lacks, so an
      `Exec` containing an unpaired `'` errors and silently disables that editor.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `external_editor/linux.rs:89-90` uses `shell_words::split` where the pin hand-rolls `tokenize_exec` (`42effe840:...:27-79`) treating `'` as an ordinary character. shell-words 1.1.1 has `SingleQuoted` and `Comment` states and errs "missing closing quote". Consequence traced: error → `MalformedFieldCode` → `linux.rs:554-559` logs and returns `None`, so the open silently does nothing while the editor still lists.

      **REPAIRED 2026-08-21 — with an attribution correction.** The `%` divergence is **NOT a regression introduced by `f0b71fe3e`**, as the refutation round-up implied. Diffing `f0b71fe3e~1` shows the per-character `%` scan, the `parts` accumulator and the empty-part filter were all already present; the commit only swapped `shell_words::split` for `tokenize_exec`. `--arg=100%` failed and `/opt/50%off/ed` was mangled before it too. **The commit's actual fault is narrower:** it claimed to fix the "editor appears in the picker and does nothing" failure while leaving the other cause of that same failure in the caller. (Detail correction: `%o` swallowed the `%` and re-emitted the `o`, so the path became `/opt/50off/ed`.) Now fixed to pin semantics: `build_command` (`linux.rs:153-212`) uses the pin's whole-token rule (`strip_prefix('%')` + `len()`), pushes tokens unconditionally including empty ones, and returns `NoExec` for empty argv via `split_first().ok_or(NoExec)`; `process_field_code` (`:218-274`) pushes whole arguments instead of appending into `last_mut`. **`UnterminatedQuote` added** (`:673-676`) rather than rewording `MalformedFieldCode` — `DesktopExecError` is private and referenced only in these two files, so no exhaustive match breaks, and keeping one variant for two unrelated faults was the thing being complained about; both messages are now the pin's verbatim. **Three stale comments corrected, all caused by `f0b71fe3e`:** `linux_tests.rs:70-72` and `:420-428` still cited `shell_words`, and `:597-603` claimed `process_field_code` had no arm for the deprecated codes and that the test would be red — it had one and the test passed. **One change beyond the brief, flagged:** the unknown-field-code arm now drops (`_ => {}`) as the pin does, instead of re-emitting the bare character as its own argv entry; the "per the spec" citation on the old arm was unsupported and no test asserted it.

      **FIXED 2026-08-21:** `linux.rs:17-105` ports the pin's `tokenize_exec`; `build_command` uses it. **Deliberate divergence:** an unterminated `"` keeps mapping to `MalformedFieldCode` (no `UnterminatedQuote` variant exists) because `linux_tests.rs:61-79` asserts that and was outside the edit set.

### Refutation round — guards and CI

- [x] 🔴 **VERIFIED: `check_stub_coverage` has been a no-op in CI since the 2026-08-15
      re-pin — it resolves the SUPERSEDED pin.**
      `script/check_stub_coverage:43-44` hardcodes
      `grep -oE '\b02b53fcd8[0-9a-f]*' ORACLE.md`, and ORACLE.md:64 still carries that
      string, so PIN resolves to the OLD `02b53fcd81ac…` rather than `42effe840…`.
      CI fetches only the 40-char NEW pin (`pr-check.yml:102-107`) and then asserts it is
      present with the comment *"stop the job rather than silently disarm a guard"* and
      the error text *"stub coverage would silently skip"* (`:114-117`).
      **That assertion guards the wrong SHA.** The old pin is absent from a depth-1
      checkout, so the guard's own `git cat-file -e` fails, it prints `skipped` and exits
      0 — exactly the outcome the workflow step was written to prevent.
      Verified directly: the hardcoded literal, the ORACLE.md line, and the CI fetch/assert
      block. Locally the guard still runs (a full clone has the old pin), which is why
      this survived — it is green everywhere a human would look.
      `script/state:42` and `generate_pin_identity_manifest:57` were both fixed for this
      exact literal; this script was missed. Any test written against a stub gutted after
      `02b53fcd8` slips through.
      **This is one of the two guards CLAUDE.md names as enforcing what the compiler
      cannot.** The other, `check_cloud_boundary`, is separately holed (below).
      **VERDICT CONFIRMED (second verification) (independent verifier, 2026-08-21):** Independently re-verified: `:43` resolves `02b53fcd81ac…` from ORACLE.md:64, CI fetches only the 40-char `**Commit (full)**` row after a depth-1 checkout, and the old pin is unreachable from phosphor history (`merge-base --is-ancestor` fails; only `refs/remotes/warp/master` has it), so `:46` fails and `:47-48` exits 0.
      **FIXED 2026-08-21:** `script/check_stub_coverage` now reads the `**Commit (full)**` row like `script/state`, `generate_pin_identity_manifest` and `pr-check.yml`, with NO hardcoded fallback — guessing a pin is what caused this. Verified it now resolves `42effe840…` and still passes.

- [x] 🔴 **`check_cloud_boundary` misses `pub use`, and the tree already launders imports
      through it.** The regex at `:44` requires `^\s*use`. Live and unallowlisted:
      `app/src/lib.rs:300` `pub use crate::server::telemetry::{…}` and
      `app/src/drive/folders/mod.rs:11`. Worse, `lib.rs:302-304` comments that the
      re-export exists so `remote_server::codebase_index_model` "must not import from
      `crate::server::` directly (`script/check_cloud_boundary`)" — a **documented
      bypass**, and a crate-root laundering point for any future cloud import. (Separate
      from the brace-form `use crate::{server::…}` hole logged earlier.)
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `:45` requires `^\s*use`, which `pub use` never matches. Live and unallowlisted: `lib.rs:300` `pub use crate::server::telemetry::{…}` and `drive/folders/mod.rs:11` — neither is in `script/cloud_boundary_allowlist.txt` (the `lib.rs` entries there are `:118-123` only). `lib.rs:302-303` names the guard as the REASON for the re-export.
      **FIXED 2026-08-21:** Covered by the same widening. Both live re-exports (`lib.rs:300`, `drive/folders/mod.rs:11`) are now visible and allowlisted with a note that they should be REMOVED rather than allowlisted permanently — `lib.rs:302-303` names the guard itself as the reason its re-export exists.

- [x] **Two guards run nowhere.** `script/check_dangling_modules` (3,265 declarations)
      and `script/check_workspace_clean` are referenced by neither `precheck` nor any
      workflow.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `check_dangling_modules` and `check_workspace_clean` both exist and are executable, but grep across `script/` and `.github/` finds zero invocations; `precheck:216-255` enumerates ten guards and neither is among them. **TODO.md:41 asserts `check_workspace_clean` "implements the workspace-clean gate… in daily use"** — nothing runs it.
      **PARTIALLY FIXED 2026-08-21:** `check_dangling_modules` is wired into `script/precheck` and passes. **`check_workspace_clean` is still unwired** — the agent-cargo placement described in the first version of this note was reverted after adversarial review showed it would have failed every CI cargo job (detached HEAD trips the branch check) and would have landed back inside precheck's path anyway, since precheck routes cargo through agent-cargo. Both failed placements and their reasons are recorded at `script/precheck:259-273`. This item stays OPEN for that half.

- [x] **Guards fail open on missing input, and `precheck` hides it.**
      `check_sweep_ledger:59-62` exits 0 if the ledger is renamed;
      `check_declined_collisions:168-170` only warns when zero markers parse;
      `check_brand_strings:159-161` exits 0 without python3. `precheck:216-247` runs every
      guard `>/dev/null 2>&1`, so all of them read `ok` regardless.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `check_sweep_ledger:60-62`, `check_declined_collisions:167-169` (warning only, `status` untouched) and `check_brand_strings:159-161` all exit 0 or merely warn; `precheck:216-255` runs each with `>/dev/null 2>&1` and reports on exit status, so "skipped" is indistinguishable from "ok". Nuance: CI closes only the brand-strings hole (`pr-check.yml:139-141`).
      **FIXED 2026-08-21:** All three now fail closed: `check_sweep_ledger` exits 1 on a missing ledger, `check_declined_collisions` exits 1 when zero markers parse (it was a warning `precheck` discarded), and `check_brand_strings` exits 1 without python3. Each carries a comment saying why a skip was indistinguishable from an ok.

- [x] **`check_large_deletions:302` is cleared by one incidental mention.** It does
      `grep -qF 'DECLINED.md'` across all commit messages plus the PR body, so any single
      reference anywhere in the range clears any bulk deletion.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `:302` greps `$description` — every commit body in `base..HEAD` plus `PR_TITLE` and `PR_BODY` (`:297-298`) — and `:312-317` exits 0 on `cites_declined`. Any single mention anywhere in the range clears any bulk deletion. It IS a designed escape hatch (`:341-345`); the defect is the range-wide, substring-only test.
      **FIXED 2026-08-21:** The DECLINED.md citation is now scoped to commits that actually delete something (`git log --diff-filter=D`) plus the PR title/body, instead of every commit message in the range. The `Large-deletion:` trailer keeps its range-wide reading on purpose — it is an explicit opt-in nobody writes by accident, whereas "DECLINED.md" is a filename that appears in ordinary prose.

### Refutation round — warpui / secure storage (final wave)

- [x] 🔴 **VERIFIED: every real credential takes the world-readable Linux fallback; the
      hardened write is used only for a non-secret.**
      `warpui_extras/src/secure_storage/linux.rs:246` writes with plain `std::fs::write`
      (default umask → 0644). The 0700/0600 variant
      `write_owner_only_fallback_value` exists at `:252-284`, and the **only** caller of
      its public wrapper is `app/src/settings/local_control.rs:132` — a mode enum.
      Everything that is actually secret uses the unhardened path: BYOP API keys
      (`crates/ai/src/api_keys.rs:235`), agent-provider secrets
      (`agent_providers/secrets.rs:106`, fork-original), MCP OAuth refresh tokens
      (`oauth.rs:644`), the proxy password (`network_secrets.rs:73`, fork-original).
      **And the AES-256-GCM key is a literal in this open-source repo** —
      `"zap-local-secure-storage-fallback-key"` zero-padded (`linux.rs:116`). So on a
      keyring-less Linux host any local user can read and decrypt them.
      `linux_test.rs:59` tests owner-only permissions only on the path these callers do
      not use. Verified directly: both write paths, the single caller, and the constant.
      **VERDICT PARTIAL — upstream design, NOT a fork defect (independent verifier, 2026-08-21):** **This corrects my own earlier verification, which checked the mechanism but never checked the pin.** Every limb of the mechanism holds (`linux.rs:246` → 0644 under umask 022; `:252-284` is the only 0600 path; its sole caller is `local_control.rs:132`, a mode enum; `api_keys.rs:235`, `secrets.rs:106`, `oauth.rs:644`, `network_secrets.rs:73` all use `write_value`; `linux_test.rs:59` covers only the unused path). **But it is upstream Warp's design:** `42effe840:linux.rs` is structurally identical, its owner-only path has the same single caller (`42effe840:local_control.rs:91`), its API-key writer uses `write_value` (`42effe840:api_keys.rs:383`), and **the pin hardcodes an AES key too** — disguised as `"https://releases.warp.dev/channel_versions.json"` (`42effe840:linux.rs:108`). Only `secrets.rs` and `network_secrets.rs` are fork-original, so only those two are this fork's own missed opportunity. Reachability is also narrower than "any keyring-less Linux host": GNOME and KDE ship a Secret Service provider, so the fallback needs no D-Bus session bus / no keyring daemon (minimal WMs, containers, WSL, SSH) or a locked keyring with no prompter — and world-readability additionally needs a traversable `~/.local/state` chain (Debian/Ubuntu 0755 homes; not Fedora/RHEL 0700).

      **FIXED 2026-08-21 — hardened, not refused.** Verified end to end: `linux.rs:246` was a plain `std::fs::write` → `File::create` → `0o666 & ~umask` = **0644**, and the existing 0600 variant's only caller stored a mode enum, not a secret. Four real credentials route through it — BYOP API keys (`crates/ai/src/api_keys.rs:235`), provider secrets (`agent_providers/secrets.rs:106`), **MCP OAuth refresh tokens (`oauth.rs:669` — the ledger said `:644`)** and the proxy password (`network_secrets.rs:73`). **Inherited, not fork-introduced:** `42effe840:linux.rs` is structurally identical, down to the same unused 0600 variant and the same `fallback_value_is_owner_only` test exercising only the unused path. **The easy miss, and the reason this needed care: `.mode()` applies only at CREATION, and `truncate` preserves an older build's 0644** — so `write_fallback_file` (`:247`) re-chmods **the descriptor** before `write_all`, which both tightens a pre-existing blob and keeps ciphertext from ever sitting momentarily group-readable. A blob that is never rewritten (a long-lived API key may never be) is migrated on **read** (`:332`, `:353`), fd-based to avoid symlink TOCTOU and best-effort so a read-only FS cannot turn a readable credential into a missing one. **Refusal was rejected deliberately:** the fallback exists for headless/container/WSL hosts, and refusing *writes* would leave already-exposed blobs in place while breaking rotation — strictly worse. **No credential moved** — same dir, filename, ciphertext and AES key, so nothing becomes unreadable on upgrade. The parent dir is deliberately **not** chmod'd, because the fallback lives in the shared `paths::state_dir()`; a test asserts the dir mode is untouched. `warn_fallback_backend` (`:379`) makes the backend visible — a keyring write and a locally-encrypted blob were previously indistinguishable from outside — and states plainly that the encryption key is a build-time constant, so it defends against other local users only. **Left alone with reasons:** the hardcoded AES key (changing a byte silently orphans every existing blob) and the `OnceCell` that caches a failed Secret Service connection permanently (upstream, carries its own `TODO`, and changing it alters connection behaviour).

- [x] **`check_stub_coverage` disarmed — CORROBORATED independently by a second agent**,
      which additionally found that `generate_repin_queue:178` was repaired for this exact
      literal and cites `check_stub_coverage` as the convention it copied. The script the
      others were modelled on was itself never fixed. (Logged in full above.)
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `generate_repin_queue:176-183` names the removed `02b53fcd8` fallback as "the exact shape of the bug those two were repaired for", and `generate_pin_identity_manifest:50` states "same convention as script/check_stub_coverage" while itself reading the `**Commit (full)**` row — as does `script/state:45`. `check_stub_coverage:43-44` is the only guard still carrying the literal.
      **FIXED 2026-08-21:** Same fix as above; the literal `02b53fcd8` is gone from the last guard carrying it.

- [x] **A rejected key event is resurrected by the layer below.**
      `warpui_core/src/event_loop/mod.rs:1336` dropped the pin's
      `matches!(logical_key, Key::Unidentified(_))` gate
      (`42effe840:…:1301-1311`); `key_events.rs:162` re-checks only
      synthetic/state/modifiers, so every key `convert_keyboard_input_event`
      deliberately rejects now types characters.
      **VERDICT PARTIAL — miscited and overstated (independent verifier, 2026-08-21):** **Path is wrong:** the file is `crates/warpui/src/windowing/winit/event_loop/mod.rs:1333`, not `warpui_core`. The gate gap is real, but "dropped the pin's" is false — fork commit `7e1e53a08` (2026-05-20) added this independently, BEFORE the pin's version. Consequence overstated: the fallback re-checks synthetic and state, adds a modifier gate the pin lacks, and requires non-empty text, so textless `NamedKey`s never leak.

      **REFUTED as stated, FIXED structurally, 2026-08-21.** Path is `crates/warpui/src/windowing/winit/event_loop/`, not `warpui_core`, and provenance is fork commit `7e1e53a08` — **nothing was dropped from the pin**, the fork wrote this independently. **"Every key now types characters" is wrong three times over.** `convert_keyboard_input_event` returns `None` for four reasons, only one of which is a *deliberate* refusal (`KEYS_TO_IGNORE`) — and that list is **empty on every non-wasm target**; on wasm its single entry carries a modifier the fallback's own gate rejects. For an identified key to reach the fallback it needs non-empty `event.text` **and** to be unmapped by `convert_key`, but every `NamedKey` winit gives text to is in that table, and a dead-key press carries `text: None` (winit returns `Ok(None)` while composing, the same branch that produces `Key::Dead`). So `Key::Unidentified` is in practice the only key that reaches it. The fork's variant is **stricter** than the pin on three axes and looser on one that is currently vacuous. **The real residue is defect pattern 3:** the refusal was collapsed into a bare `None` and the fallback re-derived "is this a shortcut?" by hand, the two agreeing only by the accident that the one ignored keystroke has a modifier. Now a three-variant `KeyConversion` the compiler forces you to handle — **zero behaviour change by construction**. **No test, deliberately:** any test of the `Ignored` path is vacuous on every non-wasm target; the enum enforces it at compile time instead.

- [x] **GUI ancestors get no keystrokes under a focused TUI leaf.**
      `warpui_core/src/core/app.rs:2081` routes on presenter presence while its sibling at
      `:1487` routes on `tui_views.contains_key`, and the comment at `:1481` states
      exactly why the presenter branch is wrong. `tui_view_tests.rs:479` cannot catch it —
      it builds the chain with `view_ancestors()` and passes it in, never calling
      `get_responder_chain`.
      **VERDICT PARTIAL — latent (independent verifier, 2026-08-21):** Asymmetry confirmed (`core/app.rs:2081-2085` routes on presenter presence, `:1488-1491` on `tui_views.contains_key`, and `:1477-1484` states why the presenter branch is wrong; `:1463` is a third presenter-only site), and the test blindness is real. **But latent:** no production site calls `add_tui_view` on a GUI window — all callers are in `crates/warp_tui`, whose windows come from `add_tui_window` and have no presenter.

      **FIXED 2026-08-21 — and a mechanical port of the pin would have been ACTIVELY WRONG.** At the pin the *GUI* presenter reports its layout embeddings into `view_parents`, so `view_ancestors` is the single answer for every view and `get_responder_chain` is a one-liner. **In this fork `view_parents` is written only by the TUI render path** (`presenter/tui.rs:202`), so porting that one-liner would collapse **every GUI responder chain to one element**. Fork-introduced split, fork-correct answer required. There were **three** copies of the correct rule and two of the wrong one; all now route through one `responder_chain_for_view` helper. **An in-family site named in no ledger entry:** `dispatch_action_for_view` routed on presenter presence and returned `false` outright, so the same action reached a TUI view *by type* and vanished *by name*. Also fixed a stranded doc comment — `get_responder_chain`'s docs were sitting on `view_ancestors`, leaving one function with none and the other with two.

- [x] **Panic on TUI-only windows.** `core/app.rs:1911` and `:1963` `.expect("Invalid
      window id")` on a presenter that `add_tui_window` never inserts.
      **VERDICT PARTIAL — latent (independent verifier, 2026-08-21):** Both `.expect("Invalid window id")` sites are real (`core/app.rs:1913`, `:1963`) and `add_tui_window` (`core/app/tui.rs:150`) inserts a `Window::default()` with no presenter; the pin cannot panic here because `42effe840:app.rs:1945,1992` use `view_ancestors`. **But no TUI caller reaches them** — the only callers are GUI-only (`app_menus.rs:1115`, `command_palette/new_session/data_source.rs:118`, `search/action/data_source.rs:87`).

      **FIXED 2026-08-21 — the assertion's stated invariant is simply false.** Line numbers were `1913`/`1963`, not `1911`. **The decisive evidence is `presenter`'s own doc** (`app.rs:1004`): it returns `None` "if there is a race condition where a window event comes in after the window is closed" — so `.expect("Invalid window id")` asserts something the function it calls documents as untrue, **before TUI is even considered**; `add_tui_window` merely made `None` a steady state. The route that matters resolves the **active** window and its focused view, so it is never called with a hand-picked id; it stays latent only because `app/` never creates TUI windows and `warp_tui` never runs `app_menus` — **a build-configuration accident, not an invariant**. Both sites now use the shared helper, matching what the pin does at the same two sites. Zero `.expect("Invalid window id")` remain. **Deliberately not converted to `Result`:** the degrade is already visible and typed, and both functions already return `Vec::new()` on error, so a second failure channel would duplicate one that exists.

- [x] **#585's "inventory corrected" is still one site short.**
      `secure_storage/registry_backed.rs:28` is `Software\Zap\`, joined to
      `application_name()` = `Phosphor`, so Windows prefs live under a pre-rename
      identity.
      **VERDICT PARTIAL — miscited, and it is deliberate (independent verifier, 2026-08-21):** The literal exists but **not where cited** — it is `warpui_extras/src/user_preferences/registry_backed.rs:28`, not under `secure_storage/` (no `Software\` string exists there at all), joined at `settings/init.rs:474`. And the "missed site" framing breaks: `bin/phosphor_oss.rs:32-35` names `Software\Zap` explicitly as one of the DELIBERATE load-bearing zap compatibility surfaces.

      **REFUTED 2026-08-21 — closed with no change, on three independent grounds.** (1) **The cited file does not exist.** There is no `secure_storage/registry_backed.rs`; the tree's only `Software\Zap\` is `user_preferences/registry_backed.rs:28`, joined at `settings/init.rs:518` — and the verifier's `:474` is also wrong. (2) **Category error:** #585's inventory is the `AppId::new` inventory; a Windows *user-preferences* registry base path was never in its scope. (3) **That inventory is in fact complete** — all three production `AppId::new` sites read `("dev","phosphor","Phosphor")` (`channel/state.rs:46`, `bin/phosphor_oss.rs:30`, `warp_tui/src/bin/oss.rs:39`), and the fourth (`integration.rs:30`) is the deliberately-isolated `Channel::Integration` harness, excluded on purpose per `specs/phosphor-rebrand/LAYER3-PLAN.md:63-67`. Moreover **the literal is deliberate**: `bin/phosphor_oss.rs:32-35` names `Software\Zap` as one of the load-bearing zap compatibility surfaces, alongside persistence keys, `X-Zap-*` headers and the DCS reply. Renaming it would orphan Windows user preferences with no migration — the same harm the credential-fallback item above was careful to avoid.

- [x] **`register_unavailable` + `unavailable.rs` + 3 tests are dead** — the pin's only
      caller (RemoteServerDaemon, `42effe840:app/src/lib.rs:1468`) was not ported, and
      `run_daemon_app` registers no secure storage at all, so `ctx.secure_storage()` there
      panics rather than degrades.
      **VERDICT PARTIAL — panic half refuted (independent verifier, 2026-08-21):** Dead-code half confirmed: `register_unavailable` (`secure_storage/mod.rs:67`) has no reference anywhere in `app/`, `crates/` or `lib/`, and the pin's caller is `42effe840:app/src/lib.rs:1468`. **The panic half is refuted:** reverse-tracing all five `ctx.secure_storage()` sites (`agent_providers/secrets.rs`, `mcp/.../oauth.rs`, `settings/local_control.rs`, `settings/network_secrets.rs`, `crates/ai/src/api_keys.rs`), none is reachable from the singletons `run_daemon_app` registers (`remote_server/mod.rs:118-295`).

      **WIRED, not deleted, 2026-08-21 — dead-code half confirmed, panic half refuted.** `register_unavailable` had no caller anywhere: the fork's `LaunchMode::RemoteServerDaemon` never reaches `initialize_app`, because the daemon bootstraps via `run_daemon_app`, which registered no secure storage. The panic half is correctly refuted — none of the five `ctx.secure_storage()` sites is reachable from the daemon's singletons — **but `ctx.secure_storage()` does panic when unregistered**, which is the exact failure mode `run_daemon_app`'s own comments already guard against for two other models. Wired at `remote_server/mod.rs:140` (**outside the stated edit list — the "wire it" branch is impossible otherwise**), restoring the pin's behaviour and rationale verbatim: a headless daemon must not reach a platform keychain that may block on an interactive unlock. The three tests are now backed by live code, so there is no test/code split and no `DECLINED.md` row is needed.

- [x] **Correction to an earlier brief premise:** `add_singleton_model` is a
      `debug_assert!` (`core/app.rs:2309`), so in RELEASE a duplicate silently *replaces*
      the singleton rather than panicking.
      **VERDICT CONFIRMED (independent verifier, 2026-08-21):** `crates/warpui_core/src/core/app.rs:2311` is `debug_assert!(prev_value.is_none(), ...)` and the `singleton_models.insert` at `:2306-2308` PRECEDES it, so in release the entry is already replaced and only a debug build panics. The line cited earlier (`:2309`) is the "Panic in debug mode" comment, not the assert.

      **CLOSED 2026-08-21 as latent-and-documented, no behaviour change — and that is the right answer.** Caller sweep over ~200 sites found **no reachable duplicate registration in production**: every non-test registration is one-shot inside `initialize_app` (`lib.rs:1268`, once per `App` — extra windows re-register nothing) or inside `run_daemon_app` (`remote_server/mod.rs:118-296`), which builds its **own** `App` in a separate process, so its overlap with `lib.rs` is across instances not within one; the four `secure_storage::register*` variants are an if/else chain (`lib.rs:1310-1320`); `init_and_register_user_preferences` is `#[cfg(any(test, feature = "test-util"))]`. The duplicates that do occur are test-harness-only, and tests always build with debug assertions on, so the panic still fires exactly where it matters (`tui_test_support.rs:98-99`, `test_util/settings.rs:14-19`, both of which already document the "was called twice" failure). **The pin is byte-identical** (`42effe840:crates/warpui_core/src/core/app.rs:2266-2278`, comment included), so promoting to `assert!` would be a deliberate divergence adding a release-mode panic to a path that has never fired, to catch a bug the debug path already catches in the only place it occurs; `Result` would ripple through ~200 call sites for the same non-event; first-wins-plus-log would silently change release semantics away from the pin. **Fixed the misleading comment instead** (`core/app.rs:2300-2318`): it now states that duplicates are *reported, not prevented*, that release keeps the replacement and strands earlier `ModelHandle`s, and why it stays a `debug_assert!`. The old inline comment ("Panic in debug mode if this is the second time…") let a reader believe the assert prevented the duplicate; the insert has already replaced the previous handle by the time it runs.
