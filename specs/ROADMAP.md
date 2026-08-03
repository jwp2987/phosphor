# Phosphor roadmap — post-parity-sweep

> **STATUS: PLANNING (2026-08-02).** The Warp test-parity sweep is complete: ~150
> tests ported back, ~41 regressions filed, the #11 feature-gap ledger triaged +
> BYOP-decided, and the SSH-manager removal scoped. **Nothing is fixed, built, or
> removed yet.** This is the top-level map; each track links to its detailed scope.

## The three tracks

| Track | What | Detail | Tracking | Size | Priority |
| --- | --- | --- | --- | --- | --- |
| **1 — Bug fixes** | Fix regressions the sweep caught (each has a committed red test) | [`warp-parity-sweep/SCOPE.md`](warp-parity-sweep/SCOPE.md) Workstream A | issues #3–#47 | ~41 | **Highest** |
| **2 — Feature builds** | Restore ticked Warp gaps, BYOP-adapted | [`warp-parity-sweep/SCOPE.md`](warp-parity-sweep/SCOPE.md) Workstream B | issue #11 | ~40 | Medium / long-tail |
| **3 — SSH-manager removal** | Delete the fork-original SSH/SFTP manager + `zap_sync` gist-sync | [`remove-ssh-manager/SCOPE.md`](remove-ssh-manager/SCOPE.md) | (fork-original, no issue) | 1 cluster / 7 steps | Independent |
| **4 — Exhaustive test-gap audit** | Per-*function* triage of all missing Warp tests (close the sweep's sampling gap) | this doc, [§Track 4](#track-4--exhaustive-test-gap-audit) | (to file) | ~3,122 fns | Lowest / ongoing |

## How the tracks interlock

- **Independent** — different areas; any order, can run in parallel.
- **~6 bugs overlap Track 2:** #17, #18, #22, #25, #46, #47 are also closed by feature
  builds. Sequence so they aren't done twice.
- **Track 3 is disjoint** — fork-original, no Warp oracle, touches nothing in Tracks 1–2.
  Good "clear the decks" work (shrinks codebase + attack surface).

## Branching strategy — keep `main` green

All ~41 **red** regression tests + these scopes currently live on branch
`warp-test-parity-sweep`. **Do not merge that branch wholesale** — it would make
`main` red.

- The sweep branch stays the **reference/audit** (red tests + trackers).
- Each **fix PR** branches off `main` and carries *its* red test **+** the fix
  together, so it lands green (`Fixes #N`, AGENTS.md §5.11).
- **Feature PRs** and the **removal** work the same way (branch off `main`, own PR).
- Net: `main` only ever gets green, self-contained PRs.

## Definition of done — the test lands with the change (all tracks)

Coverage grows as gaps close. A change is not complete until its test is green.

- **Bug fix PR** = the fix **+ its red regression test** (already on the sweep branch), now passing. `Fixes #N`.
- **Feature PR** = the feature **+ the Warp oracle tests that were blocked on it**, ported and passing. A feature is **not done** while its now-unblocked tests remain un-ported — that would silently re-open the coverage gap this effort exists to close.
- **Removal** deletes fork-original tests *with* the code (no oracle → not a coverage regression).
- **Track 4** "ported" disposition means the test is wired and passing, not just categorized.

Net: total test count only goes **up** as gaps close; coverage tracks the code. Never weaken an assertion to make it pass (§5.10).

## Recommended sequence

1. **Track 1 — security first:** #22 OSC 52 → #25 browser scheme → #7 file_glob injection → #12 log leak.
2. **Track 1 — crashes:** #33 markdown panic → #39 FlatStorage panic → #35 reversed range.
3. **Track 3 — removal** (early; quick + independent; shrinks surface before building more).
4. **Track 1 — remainder** (data/behavior → UX), sequencing the ~6 feature-overlap bugs.
5. **Track 2 — features**, highest-value clusters first.
6. **Track 4 — exhaustive test-gap audit** (lowest; ongoing — partly resolves as Track 2 lands).

## BYOP decisions already made (2026-08-02)

Recorded in #11 + the sweep scope. **Dropped** (need cloud/BYO backend the fork lacks):
OTEL trace-link, VoiceInputLifecycle, AI semantic-search, computer_use recording.
**Split — build non-cloud half only:** AI skills (drop remote arm), history_model (drop
cloud-metadata-merge/remote-child), context-window (drop pricing warning), persistence
(drop team_uid). **Adapt:** autoupdate channels → the fork's own release repo.

## Track 4 — exhaustive test-gap audit

**Why:** the parity sweep triaged at the *file/module* level with sampling, not
per-function. It swept every area, but the long tail (especially AI) was categorized
fast, not verified line-by-line. This track closes that gap so we can honestly say
"every Warp test is accounted for."

**Goal:** for each of the ~3,122 missing Warp test *functions* (fork ~7,428 vs Warp
~10,550), assign a **verified** disposition:
- **ported** — brought over (regression net), or
- **already-covered** — confirmed equivalent in the fork under a different name/inline, or
- **cloud-drop** — confirmed cloud, legitimately dropped, or
- **feature-blocked** — needs a #11 feature first (link the ledger item).

**Method:** systematic per-function diff of `warp/master` test fns vs the fork, area
by area, producing a definitive ledger (spreadsheet/issue checklist). Prioritize the
sub-scopes agents flagged as **unfinished**:
- `app/src/workspaces/*` (team/user_profiles/user_workspaces — not audited)
- rest of `app/src/search/*` (mixer, searcher, command_palette, command_search)
- large `remote_server` daemon subsystems (setup/version/lifecycle — "too big to port")
- **AI area** — biggest under-covered surface; `history_model` (45 tests triaged, ~0 ported)

**Priority: lowest.** Do it after Tracks 1 & 3; it partly resolves itself as Track 2
features land (feature-blocked tests become portable). Lower ROI than fixing known
bugs, but it's the only way to *guarantee* no high-signal regression is still hiding.

**Models:** Sonnet for the mechanical per-function diff/triage; Opus for the AI-area
judgment calls.

## Agent model guidance

Tier by judgment required, not by track. Hard rules apply regardless of model:
flock-serialized cargo (§5.8, one build at a time), tests-first + issue→PR (§5.11),
≤3 concurrent agents (contention), and **review feature-build output against the
oracle** (a wave-4 agent over-reached — implemented features unsupervised — and had
to be reverted).

- **Opus 4.8** — security fixes (#22, #25, #7, #12); subtle control-flow / data bugs
  (#3 Stop/auto-resume, #10 batched-edit, #13 regex); and the judgment-heavy feature
  builds (history_model reconciliation, `local_control`, `repo_metadata` lazy-tree,
  code_review-over-SSH, context-window). Also the orchestrator/reviewer role.
- **Sonnet 5** — the mechanical bug-fix bulk (missing match arm, guard, paint reorder,
  `u8`→`usize`, etc. — red test + oracle diff make these low-ambiguity); the
  self-contained feature builds (jupyter detection, `PATHEXT`, box-drawing,
  `tmux_passthrough`, CDPATH); and **Track 3 removal** (mechanical delete + build-fix).
- **Haiku 4.5** — only the truly trivial one-liners if cost-squeezing; otherwise Sonnet
  is more reliable through build breakage.
- **Fable 5** — separate usage bucket; use to spread cost on parallelizable *mechanical*
  batches. Keep security/judgment work on Opus.

Rule of thumb: **bug fixes lean Sonnet** (test-guided, oracle has the answer);
**feature builds lean Opus + close review** (faithful-port risk); **removal = Sonnet**.

## Current state (2026-08-03)

- **Track 1 (bug fixes): DONE.** Branch `parity-fixes` (off `warp-test-parity-sweep`) has **42 of 43 bugs fixed + closed** (issue→commit→close, each a verified oracle port). Only **#37** remains open (groundwork committed; needs `external_control_master` plumbing — a cross-layer follow-up). Full run: **4036 pass / 5 fail** = the FD-exhaustion flakies (#24, pass in isolation). #48/#49 were extra reds caught by the final verification.
- Track 2 (features): not started — awaiting the #11 sign-off decisions.
- Track 3 (SSH-manager removal): not started — see `remove-ssh-manager/SCOPE.md`.
- Track 4 (exhaustive audit): not started.
- **Nothing pushed.** `main` untouched. Next action: **push `parity-fixes` / open the fix PRs** (each fix is a clean commit with `Fixes #N`), then Tracks 2/3/4.
- GitHub `jwp2987/phosphor` open: #37 (groundwork), #24 (FD test-health), #11 (feature ledger), #5/#4/#2 (deferred/tracking).

## Verify command (all tracks, flock-serialized — MANDATORY)

```
ulimit -n 8192
flock /home/winters/.claude/jobs/d323e5af/tmp/zap-cargo.lock -c \
  'cargo test -p <crate> --lib [--features gui,tui,local_fs] 2>&1 | tail -60'
```
