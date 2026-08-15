# TODO — Phosphor: Warp parity ledger (#11) + code-review debt

**Checkbox key:** `- [ ]` open · `- [>]` **IN FLIGHT, agent assigned** · `- [~]` partial · `- [x]` done.
Added 2026-08-10 after a status report listed four in-flight items as unstarted:
the assignment lived in the operator's head and not in this file. **Record the
assignment here when you start work, not when you finish it.**

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

- [ ] **Parallel agents work in the shared checkout `/cache/git/zap`, switch its
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

- [ ] **GUI bootstrap fails on Windows: the pwsh child exits immediately.**
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

- [ ] **`usage_tui_transcript_render` fails on Windows** (the other 5 TUI
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
| 2 | **#532 PTY-spawn wiring** | `register_session_id` has 0 production call sites here vs 4 in the pin |
| 3 | **`language_by_filename` signature** | fork takes `&Path`; pin takes `&StandardizedPath` + `language_by_filename_parts` |
| 4 | **MCP tool results render as a JSON blob** | `inline_action/requested_command.rs:1494` — `to_string_pretty(result)` |
| 5 | **`with_semantic_selection_by_style`** | no definition tree-wide |
| ~~6~~ | ~~`use_computer_decoration`~~ **— NOT A SYMBOL.** It is a pin *test name*: `output_tests.rs:170 fn use_computer_decoration_skips_screenshot_only_rows()`. Screenshot handling exists (`view_impl/output.rs`, 25 refs). Re-scope against the actual decoration predicate or drop. | corrected 2026-08-11 |
| ~~7~~ | ~~TUI renderer for `MessagesReceivedFromAgents`/`EventsFromAgents`~~ **— FALSE.** `crates/warp_tui/src/agent_block.rs:1311` renders `MessagesReceivedFromAgents { messages }`; `:1318` deliberately no-ops `EventsFromAgents`. Types exist in `convert_{conversation,from,to}.rs`. `agent_block_tests.rs` exists in fork and pin. If the real complaint is the `EventsFromAgents` no-op, file that narrowly with `:1318` as evidence. | corrected 2026-08-11 |
| 8 | **Zap #324 — pane min size** | `MIN_PANEL_WIDTH: f32 = 300.` hardcoded, `ai_assistant/panel.rs:61` |
| 9 | **Zap #329 remainder** — hunk staging, branch create/switch | no `stage_hunk`/`checkout_branch` |
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
- [ ] **Upstream the Vertex `:streamRawPredict` fix** to
      `jeremychone/rust-genai`. Every vendored fix we do not upstream is a
      merge conflict we pay for at each bump — this one has now been carried
      across two.
- [ ] **Launch the app.** Nobody has since the freeze lifted. A green suite does
      not discharge this: the startup crash of 2026-08-10 passed 6,000 tests.
      See `docs/build/TRIAGE.md` § "Beyond the compiler" — singleton
      registration order and prompt-cache breakpoint placement are both
      invisible to `cargo check` and to `nextest`.

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
- [>] **`app/src/terminal/model/terminal_model_tests.rs` — 2 tests.**
      `should_validate_dcs_hook_session_id` is hardcoded `false`, not
      role-conditional. Blocked on **#419** PTY-spawn registration wiring.
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
product-scope question (the ordering reversal, the DCS-hook role gate).

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
      terminate hook, `lsp_logs.rs`, `lsp_telemetry.rs` catalog, 3 editor helpers
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
- [ ] **Still worth a guard, but scoped honestly.** A CI check flagging large
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
- [ ] **`crates/remote_server/src/client/mod.rs::resolve_conflict`** — zero callers
      while the daemon fully handles `ResolveConflict`, but **the pin has no client
      sender either.** Becomes live only as part of #2's client half.
- [ ] `crates/remote_server/src/host_response.rs` — four `pub fn`s with only test
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
- [ ] `OrchestrationConfigState`, `AuthSecretSelection`, `apply_execution_mode_change`.
      The picker layer only. Orchestration itself is built. #310/#304.
      **`AuthSecretSelection` needs a cloud/non-cloud ruling before implementing.**
      Its pin variants are `Named(name)` / `Inherit` / `CreatingNew` — a picker
      for which auth secret an orchestration child uses. In this fork "auth
      secret" would mean a BYOP provider key, which is local; at the pin it may
      mean a Warp *managed* secret, which is declined. It sits in the same file
      as `ORCHESTRATION_WARP_WORKER_HOST`. Decide before building, not after.
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
- [ ] `use_computer_decoration`. Block decoration for computer-use actions.
      Computer use itself is built and sighted as of tonight.

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
- [ ] **No TUI renderer for `MessagesReceivedFromAgents` / `EventsFromAgents`**
      (9 tests). **The GUI half is built** — `blocklist/orchestration_events.rs`
      both emits (`:331`) and renders (`:407`) them. Only the TUI side is
      missing. Second of the two clusters behind #456.
- [x] **~~No `/index` slash command~~ — FALSE, corrected 2026-08-11.** It
      exists: `app/src/search/slash_command_menu/static_commands/mod.rs:257`
      maps `"/index" => SlashCommandKind::Index`. This entry also said
      "Verified", which it was not.
      *(If the real complaint is that the command exists but does nothing
      useful, that is a different, narrower item — file it with the dispatch
      site as evidence, not as "no command".)*
- [ ] **MCP tool results render as a `serde_json::to_string_pretty` blob, not a
      collapsible tree.** `McpRenderable` / `mcp_result_to_renderable` exist
      **nowhere in production** — the only tree-wide hits are a comment in
      `ui_components/json_tree_tests.rs` noting the pin has 5 tests for them.
- [ ] **`TuiSelectable::with_semantic_selection_by_style` does not exist** —
      double-click cannot select a whole styled span. Verified: no definition
      anywhere. **Note the correction below: its sibling DOES exist.**
- [ ] **`skill_watcher.rs` lacks the remote-project-skill refresh/fallback
      layer** (13 tests).
- [ ] **`languages::language_by_filename` has no `StandardizedPath` overload** —
      remote files resolve their language through a host-local `Path`. This is a
      deliberate fork simplification (documented on
      `try_chunk_code_semantically`), so it may belong in `DECLINED.md` rather
      than here — needs a maintainer call.

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

- [ ] **Watch these two at runtime.** Both touch subsystems this fork has already
      mis-classified once and had to reverse: `BackgroundComputerUse` (computer
      use is the documented false-positive in `FORCE_DISABLED_FLAGS`' own comment)
      and `RemoteCodebaseIndexing` (`DECLINED.md` carries a codebase-indexing
      reversal). If behaviour regresses after this change, start here.
- [ ] **`AsyncFind` is an override, not a gate**
      (`FeatureFlag::AsyncFind.is_enabled() || *self.async_find_enabled`). Turning
      it on matches upstream but *force-enables* async find and hides the user
      toggle. Revisit if the lost setting is wanted back.
- [ ] **Decide the 5 fork-original dark flags**, and record the reason in
      `DECLINED.md` rather than leaving them merely absent from every list.
- [ ] **Audit the remaining ~37 `default` entries the pin has and this fork does
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

- [ ] **#13591 — wide-grapheme stray-character rendering bug.** Real, unfixed.
      Root cause sits in `crates/warpui_core` / `crates/editor`, which **no agent
      in the 2026-08-15 fleet owned** — the partition covered `app/src`, the
      terminal/AI/completer crates and `warp_tui`, but not `warpui_core`,
      `warpui`, or `warpui_extras`. That is a genuine coverage hole in the audit,
      not just an unfixed bug: **~1,240 upstream commits touch `warpui_core` and
      `warpui` and have never been swept at all.**
- [ ] **~25 of the 34 PARTIAL candidates were never hand-verified** (tab-group
      rename bugs, cross-window drag "fuzzy shake", MCP install-modal styling,
      and others). The triage script flagged them; nobody read them. These are
      the cheapest remaining wins in this area — the expensive part (finding
      them) is already done.
- [ ] **~82 non-fix-flavoured commits** (features/refactors) were excluded by the
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

- [ ] **`89ec9a397` — `should_suppress_during_recovery` has zero references
      anywhere in this fork.** Not ported, and not stubbed either, so it is
      neither a partial nor a working divergence — the concept simply does not
      exist here. Needs a look at what upstream suppresses during recovery and
      whether this fork has an equivalent path that should be doing it. Low
      priority, but genuinely unexamined rather than decided.

### remote_server + core crates (278 commits triaged, ~35 hand-verified, 0 partials)

- [ ] **The daemon's writer loop is stuck pre-fix — two upstream commits, both
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

- [ ] **`97a9ff5f` — wasm debug panic in `StandardizedPath::from_local_absolute_unchecked`.**
      `debug_assert!(path.is_absolute())` uses std's `Path::is_absolute()`, which
      returns `false` for Unix-rooted paths on `wasm32-unknown-unknown`. This fork
      still targets wasm32 (lsp, ai, editor, code_review, terminal/view). Upstream's
      fix is a same-file swap to the encoding-aware `typed_path` check. Low risk,
      genuinely broken today.

- [ ] **Remote git-chip / PR-context proto plumbing appears entirely absent.**
      Zero hits under `app/src/remote_server/` or `crates/remote_server/proto/` for
      `RepositoryInfo`, `GitBranchStatus` or PR-url fields, while the local-session
      equivalent (`app/src/util/git.rs`) is present and current. Traced across
      `3ed3ae1d`, `8c63aaf9b`, `dbaf6d50`, `90f7a4c8`. **Consequence, and why it
      belongs next to the legacy-SSH section below: on a remote/SSH session the
      agent gets materially less git and PR context than it does locally.** Not in
      `DECLINED.md`. Sampled, not exhaustively verified (`bbdc5a2ea`, `08487819f`,
      `856c74b0` unchecked) — needs a scope call, not an assertion.

- [ ] **`f0ca7861` — `capture_exit_status` races.** `crates/remote_server/src/manager.rs`
      still uses synchronous `child.try_status()`, which returns `None` on a
      just-killed child, instead of upstream's async `.status()` + 200ms timeout.
      Diagnostic accuracy only.

- [ ] **`ae69bd4c` — no tarball cache for the SCP fallback.** The fork re-downloads
      on every install; no `remote_server_artifact_version()`.

- [ ] **`44d87708` — reliability tracking.** `send_tracked_request` / `failure_rx` /
      `RequestFailedEvent` absent; the fork still uses the ad-hoc
      `ClientRequestFailed`-per-callsite pattern that upstream's commit message
      calls a footgun. Mostly moot while telemetry is dead, but the pattern is
      live debt.

- [ ] **Windows PowerShell, low priority.** `ebedb9fd` (localized / non-UTF8
      executable-name mojibake) and `ef4b562191` / `eab3b3fa9` (deferred cmdlet and
      function-name loading, perf) — none ported in
      `crates/warp_terminal/src/shell/mod.rs`.

- [ ] **`a792340801` and `5b047fc2` — record in `DECLINED.md`, do not port.**
      Sentry init and daemon client-ID forwarding, both cloud telemetry. Correctly
      unported, but not named in `DECLINED.md`, so they keep reading as gaps. A
      one-line row each stops the re-discovery.

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

- [ ] **Port `reuse_ssh_control_master` (upstream `0d24d2cf`, "Add setting to
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

- [ ] **Restore the `node --version` per-prompt cache, and the chip gate.** The
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

- [ ] **Build a feature-reduced daemon target.** Feature-gate the app crate so
      the `RemoteServerDaemon` path compiles without the renderer and the GUI
      stack, and ship *that* as `phosphor-cli`. Plausibly single-digit MB.
      This is the enabling work for the distribution decision below — a small
      daemon makes push-based install and in-package bundling both cheap.
      Not started; the crate is not currently structured for it.

### The distribution decision — NEEDS A MAINTAINER CALL

- [ ] **Decide how the remote daemon reaches the host, and delete the fetch
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

## LATENT BREAK — the wasm target does not compile (found 2026-08-11)

- [ ] **`app/src/workspace/view.rs:179` and `wasm_view.rs` import a module that
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


- [ ] **Zap #329 — "Improve local Git workflow inside Zap"** (open, `enhancement`,
      `lyfe2025`, 2026-07-28, marked *"1 (Nice to have)"*). Asks for a
      lightweight Git panel — changed files, diffs, stage/unstage files **or
      hunks**, commit with message, create/switch branches, pull/push, and the
      assistant able to reference git status/diff as workspace context.
      Explicitly scoped local-only: no account, no cloud sync, no hosted repo
      management — so it is **in scope for this fork by construction**.

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

- [ ] **Zap #314 — SSH remote Warpify should not use `$SHELL` to pick the shell.**
      Likely present: `app/assets/bundled/bootstrap/fish.sh:618` does exactly what
      the issue objects to — *"We check the SHELL env var and use shell string
      manipulation to get the contents after the last slash"*. The bootstrap is
      inherited, so if the reasoning holds upstream it holds here. The fork does at
      least name the failure (`WarpificationUnavailableReason::UnsupportedShell`)
      rather than failing silently.
- [ ] **Zap #328 — BYOP agent: approving an async tool action immediately triggers
      `OrphanToolResult`.** The most interesting of the three, because BYOP is this
      fork's core case and Zap's BYOP is the same lineage. The fork has a whole
      `app/src/ai/byop_readiness/` subsystem that *detects* the condition and has a
      repair pass — but whether it still *produces* it on async approval needs a
      real trace of the approval path. Unresolved by reading.
- [ ] **Zap #275 — ctrl-c does not stop `python`.** Runtime behaviour; not
      settleable by reading. Distinct from the TUI ctrl-c sheet-dismiss defect
      fixed tonight (`30dce9d5a`), which was about an open shortcuts sheet, not
      signal delivery to a child process.

### Checked, not settleable by reading — no owner

- [ ] **Zap #310** (tmux Warpify breaks `cd` + Tab completion), **#294**
      (`cat <<EOF` hangs), **#316** (Arabic renders incorrectly). All runtime or
      visual claims needing the app running. Text-layout code exists across three
      platform backends, so #316 is not a missing-feature question.

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
- [ ] **#532 — REOPENED 2026-08-11. The 2026-08-08 closure was wrong.** It is the
      **fifth** entry in this file found stating the opposite of the code (#148 class).
      Original closure text, kept verbatim as the evidence of how it failed:
      > "#532 CLOSED 2026-08-08: #419 has now landed (recovered from PR #538) and
      > `requires_registered_session`, `is_registered_session`, and
      > `should_validate_dcs_hook_session_id` are present in
      > `app/src/terminal/model/ansi/{dcs_hooks,mod}.rs` and `terminal_model.rs`."

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
- [ ] **Zap #324 — pane resize: drag feels slow, and the minimum panel size is
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
- [ ] **`git pull` — the one Git verb the fork has no path for at all.**
      Raised by Zap #329 (see the upstream-issues section for the full triage;
      the other two gaps there are hunk staging and branch create/switch).
      Zero hits for `git_pull`/`GitPull` in the tree — this is absence, not a
      stub.

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
| CDPATH-aware `cd` completion | #483 (closed — done) |
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

- [ ] **WSLENV passthrough vars** *(Windows)* — **STALE: this claimed absent, but it
  is DONE.** `wsl_env_allowlist` exists at
  `app/src/terminal/local_tty/windows/environment.rs:202` (commit `17ee390a2`, PR #119,
  targeted issue #117). Compile-only port, per the commit's own note — still not
  runtime-verified on an actual WSL/Windows host, which is the real remaining item.
- [ ] **Launch-at-login** *(macOS + Windows)* — **STALE: this claimed absent, but it
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
- [ ] **AI global skills** — **this entry previously said "WON'T DO (maintainer,
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
