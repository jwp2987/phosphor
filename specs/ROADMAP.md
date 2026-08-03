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

## Recommended sequence

1. **Track 1 — security first:** #22 OSC 52 → #25 browser scheme → #7 file_glob injection → #12 log leak.
2. **Track 1 — crashes:** #33 markdown panic → #39 FlatStorage panic → #35 reversed range.
3. **Track 3 — removal** (early; quick + independent; shrinks surface before building more).
4. **Track 1 — remainder** (data/behavior → UX), sequencing the ~6 feature-overlap bugs.
5. **Track 2 — features**, highest-value clusters first.

## BYOP decisions already made (2026-08-02)

Recorded in #11 + the sweep scope. **Dropped** (need cloud/BYO backend the fork lacks):
OTEL trace-link, VoiceInputLifecycle, AI semantic-search, computer_use recording.
**Split — build non-cloud half only:** AI skills (drop remote arm), history_model (drop
cloud-metadata-merge/remote-child), context-window (drop pricing warning), persistence
(drop team_uid). **Adapt:** autoupdate channels → the fork's own release repo.

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

## Current state

- Branch `warp-test-parity-sweep`: all ported tests + scopes committed; tree clean; compiles (3997 pass / 31 fail = ~26 tracked red + 5 fd-flaky (#24) / 33 ignored).
- GitHub `jwp2987/phosphor`: issues #3–#47 (bugs), #11 (feature ledger).
- **Nothing fixed / built / removed.** Next action = pick a track and open the first PR.

## Verify command (all tracks, flock-serialized — MANDATORY)

```
ulimit -n 8192
flock /home/winters/.claude/jobs/d323e5af/tmp/zap-cargo.lock -c \
  'cargo test -p <crate> --lib [--features gui,tui,local_fs] 2>&1 | tail -60'
```
