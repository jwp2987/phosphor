# Session Handoff — Phosphor parity work

Continuity doc. The durable state is git + GitHub issues/PRs + `TODO.md`; this
file ties it together and records the operational lessons a fresh session will
not otherwise have.

Last rewritten: 2026-08-06 evening, after the migration to the `/cache/git/zap`
host.

## App identity (read first)
- The app is **Phosphor** (`jwp2987/phosphor`). "Zap" is only the **upstream
  ancestor** (`zerx-lab/zap`) — NOT the app name. Do not introduce new "Zap"
  branding; legacy identifiers (`zap-tui-oss` binary, `zap_*` crates,
  `SkillProvider::Zap`) stay. See `docs/DESIGN-PHOSPHOR-FORK.md`.
- **English only** in code/comments/tests/docs (`CLAUDE.local.md`). Exception:
  `app/i18n/zh-CN|ja/*.ftl` are intentional translations — never edit them.
- `warp/master` is a fetched git remote and is the **behavioral oracle**. Never
  weaken a test to go green — fix the code (AGENTS §5.10). Every defect → issue
  → branch → PR (§5.11).
- **The repo is PUBLIC.** Issue comments and PR bodies are indexed. The
  maintainer has accepted this for engineering detail including security
  findings (decision made 2026-08-06); do not re-litigate it, but be aware.

---

## Where main is

`6aecd280f` — two merges landed this session, the first movement on main all day:

| commit | contents |
|---|---|
| `7df4ca4bb` | PRs #134 + #155 — `util::git` coverage 8→19 tests, plus two real `get_pr_for_branch` regressions fixed |
| `6aecd280f` | PR #169 — mermaid negative-height production race |

---

## Open PRs — 15, none merged

Stacked pairs merge as a unit, child first.

| PR | branch → base | what |
|---|---|---|
| #125 | `parity-remote-git-writeops` → main | Remote git write-ops over SSH. **Carries my 4 audit fixes + 15 tests**; agent was verifying |
| #127 | `chore/build-concurrency-new-host` → main | The build governor. 5 commits of evolution — read its history, it encodes the OOM lessons |
| #128 | `chore/consolidate-todo` → main | `todo.md` → `TODO.md`; 59 local + 58 remote merged branches already deleted |
| #130 | `feat/byop-commit-message-gen` → main | BYOP commit-message generation, **local path only** (remote deferred to #125 landing) |
| #132 | `parity-pinned-tabs-ui` → main | Pinned-tabs GUI layer. 4025/0/33 |
| #172 | `feat/pinned-tabs-deferred-ui` → **#132** | Move-to-group submenu + multi-tab menus. 4031/0/33 |
| #139 | `fix/warp-tui-suite-green` → main | **warp_tui 579/18-failing → 1043/0**, plus stops tests writing to the real `~/.config/zap/user_preferences.json` |
| #166 | `feat/editor-parity-followups` → **#139** | Char-cell reference oracle + TUI clipboard/selection. 1087/0 |
| #133 | `test/port-warp-ai-crates` → main | `crates/ai` test port |
| #140 | `test/port-warp-terminal-coverage` → main | **Lands a RED suite deliberately** — 9 genuine regressions (#171). §5.6 conflict; needs a sequencing decision |
| #153 | `chore/host-test-deps` → main | rustfmt on distro-cargo hosts + verify test tooling |
| #158 | `test/port-warp-cloud-triage` → main | Non-cloud coverage hiding behind cloud-named files |
| #159 | `fix/issue-136-read-files-partial` → main | `read_files` stops discarding successes |
| #160 | `feat/issue-privacy-page` → main | The missing Privacy settings page |
| #168 | `fix/issue-138-watch-filter` → main | Watcher now prunes gitignored trees |

**Merge #139 + #166 first** — highest value, and until #139 lands every full-suite
run mutates the real user preferences file.

---

## Decisions only the maintainer can make

- **#149 / proto re-pin** — the fork pins `zerx-lab/warp-proto-apis@14ab9a71`, **40
  commits behind** upstream `warpdotdev@b0886a95` (which Warp pins today). The
  fork's proto has **zero additions**, so nothing is load-bearing. A re-pin
  proposal is in progress on `chore/repin-proto-upstream`. Consequence of the
  staleness: `AnyFilesSuccess` has no `failed_reads`, so per-file read failures
  cannot be expressed on the wire at all (#136).
- **#140 sequencing** — merge #171's fixes first, or merge #140 red and fix after.
- **#167** — TUI keybinding validator strategy. Warp exempts TUI bindings from the
  cross-platform validator; the fork drops `cmd-` keys off macOS. Linux TUI users
  currently get no keystroke for Paste / SelectToLineStart-End.
- **#174** — `>` vs `>=` restore-order divergence, currently justified only in a
  source comment with three Warp tests marked "intentionally NOT ported".
- **#11** — the standing ledger: AI global skills, skill remote-path,
  `local_control` app-side.

---

## Highest-value open issues

Security / correctness first:

- **#171** — 9 ported Warp terminal tests fail. Includes an **OSC 1337 parser panic
  on untrusted PTY output** (`ansi/mod.rs:1073` indexes `params[1]` unguarded;
  Warp guards it) and an **unquoted `cat {history_file}`** (`session.rs:1384`).
- **#164** — `ai::agent::redaction::redact_secrets` has **no tests**. It is the
  function stripping secrets before data leaves the device to a BYOP provider,
  and it rewrites byte ranges in reverse order.
- **#151** — command denylist bypassed by an env-var assignment prefix.
- **#142** — `api_keys` has 7 of Warp's 82 tests. Custom API endpoints — core BYOP —
  untested.
- **#143** — Privacy page never ported: 13 settings live at runtime with no user
  control. PR #160 addresses it.
- **#165** — telemetry setting is user-togglable but nothing consumes it.

The recurring defect class, worth internalising:

- **"Ported but never wired"** — #137 (dead `GetBranches` RPC → empty branch
  dropdown over SSH), #138 (`repo_watch_filter` never called), #146 (4 callerless
  functions), #173 (`write_command` discards `StartCommandOutcome`), #162
  (`dispatch_event` discards a result). `#![allow(dead_code)]` on `app/src/lib.rs`
  and `crates/warpui_core` is why the compiler never flags these.

---

## The #2 sweep — methodology correction

The headline "468 missing test files" figure is **inflated and should not be
quoted**. Two independent agents found the cause: the fork renames Warp's
`*_tests.rs` → `*_test.rs`, and same-path matching counts those as absent.

- Cloud-triage slice: 5 of 23 were never missing.
- Terminal slice: **30 of 48** were renames.

The real loss is test **functions dropped inside renamed files** (~330 in the
terminal subtree alone), which path matching cannot see. **Verify absence by
content, never by filename.** And classify by what a test *targets*, not the
path it sits in — `server/server_api/ai_tests.rs` reads as pure cloud but 14 of
its 40 tests cover retained non-cloud code.

---

## Build governor — `script/agent-cargo` (PR #127)

```
script/agent-cargo <agent-name> <cargo-args...>
```

Current shape, and **why** each part exists:

- **3 slots × 5 jobs.** Started at 3, was raised to 4 and **OOM-killed the whole
  fleet**, dropped to 2 as a panic response, then returned to 3 on measured
  evidence.
- **`NEXTEST_TEST_THREADS` / `RUST_TEST_THREADS` capped.** This was the *actual*
  OOM cause: nextest defaults test-execution parallelism to the CPU count, so a
  **single slot** consumed ~25 GB. Slot count could never have prevented it.
- **Per-agent `CARGO_TARGET_DIR`.** Prevents cross-agent artifact contamination,
  which on the previous host produced phantom "missing symbol" errors and a bogus
  issue.
- **Memory precheck + per-run low-water logging** to `logs/mem-<agent>.log`. Size
  slots from these numbers, not intuition. Worst dip since the fix: 6.6 GB (a cold
  full `warp` build); pre-fix it was 4.4 GB.
- **Live slot re-read.** Editing the script does **not** reach already-queued
  invocations — bash read `SLOTS` at start-up. Change `/cache/git/.phosphor-build/slots`
  instead; it is re-read each poll pass.
- **`RESULT agent=… exit=N OK|FAILED` on stderr.** Piping stdout through
  `head`/`tail`/`grep` replaces the pipeline's exit code with the filter's. This
  reported a failed build as success **twice in one day** (`|| true` and `| head`).
  **Trust the RESULT line, never the pipeline status.**

---

## Host setup (this box)

24 cores / 45 GB (~28 GB usable after resident VMs) / 561 GB free on `/cache`.

Installed by hand this session because the bootstrap had never run here: **mold**
(`.cargo/config.toml` hard-requires `-fuse-ld=mold`; without it every link fails
with a misleading `cannot find 'ld'`), **protoc 25.1**, **gh 2.83**,
**cargo-nextest**, and **rustfmt** (via apt — see below).

**Known host divergence:** `script/install_rust` guards on `command -v cargo`, so
a distro cargo at `/usr/bin/cargo` short-circuits it and rustup is never
installed. Consequence: `rust-toolchain.toml`'s `channel = "1.92.0"` pin is
**silently ignored** (host runs 1.93.1) and its `components = ["rustfmt",
"clippy"]` never arrive. PR #153 adds rustfmt via apt and verifies both tools.
Installing rustup would fix the pin but invalidates every target dir — do not do
it mid-fleet.

---

## Operational lessons — do not relearn these

1. **Subagents cannot receive asynchronous notifications.** Not
   `run_in_background`, not Monitor, not "I'll wait until it reports back". Four
   agents were lost to this today, one twice. Every brief must say so explicitly,
   naming *both* mechanisms — an agent told only "no run_in_background" reached
   for Monitor instead.
2. **Exit-status masking.** See the RESULT line above. It bit me and an agent on
   the same day.
3. **Capture before you stop.** I killed an agent and discarded its worktree before
   reading its reasoning; its transcript was empty and the analysis was
   unrecoverable. Read the diff and the report *first*, stop second.
4. **File the issue before dispatching.** I dispatched agents at findings and
   skipped the issue repeatedly, so several pieces of live work existed only in
   chat. §5.11 exists because a finding must survive an agent failing.
5. **Do not trust a self-reported green.** Auditing caught overstated claims three
   times, including one of my own. Read the diff, verify the key claim against
   `warp/master`, grep for new "Zap", confirm zero weakened asserts and no
   `#[ignore]`.
6. **Check whether the work is already done.** Twice I nearly dispatched agents at
   things already fixed.
7. **The scratchpad is shared, not agent-isolated.** One agent overwrote another's
   file. Prefix scratch filenames with the agent name.
8. **Verify the union before merging** — but know when it adds nothing. If the PR
   and main touch disjoint crates with no dependency between them, disjointness is
   a stronger argument than a green build.
9. **`cd` inside every cargo command string; never rely on persisted shell cwd.**
   Three agents hit this in one session. The Bash cwd defaults to `/cache/git/zap`
   (the main checkout), and a backgrounded call resets it for subsequent commands
   — the tool says so explicitly: *"Session cwd remains /cache/git/zap; directory
   changes made by the backgrounded command do not apply to subsequent commands."*
   The failure is **silent and worse than an error**: cargo happily builds the
   unmodified main-branch tree and reports a clean green that means nothing. One
   agent only caught it by inspecting cargo's `.fingerprint` dep-info files and
   noticing they listed the *old* file set. Write
   `cd /cache/git/zap/.worktrees/<name> && agent-cargo …` as one string, every time.
10. **A baseline number is only valid for the tree it was measured on.** The
   "4025/0/33" figure circulated all session as if it were main's; it was PR #132's
   *branch* number. It made a clean re-pin look like it had lost 22 tests and
   nearly cost a full re-measurement. The real baseline at `44bf4daa6` is
   **4005/0/33** (measured directly, and independently corroborated by two agents'
   deltas). Always state which commit a count belongs to.
11. **Check call-site *counts* against the oracle, not just symbol presence.** PR
   #195 ported `readable_chip_label_color` verbatim but wired it to 1 of Warp's 2
   call sites, leaving `chip_configurator` sub-WCAG-AA (#196). A same-symbol grep
   reports that as covered. This is the "ported but never wired" class in its
   hardest-to-spot form.

---

## What a fresh session should do first

1. Merge **#139 + #166** (child first). Stops the suite mutating the real prefs file.
2. Decide **#140** sequencing (red suite).
3. Pick up the **proto re-pin** on `chore/repin-proto-upstream` — checkpointed but
   incomplete; its verification bar is the full 4025-test suite.
4. Work the security-relevant issues: **#171**, **#164**, **#151**.
5. Rewrite `TODO.md` per **#148** — it is stale in both directions and one entry
   states the opposite of the truth, actively misdirecting readers.

**Every branch is pushed.** Nothing lives only on disk. In-progress agent work is
committed as `WIP checkpoint (<agent>)` commits — those are unverified and may not
compile; re-verify before building on them.
