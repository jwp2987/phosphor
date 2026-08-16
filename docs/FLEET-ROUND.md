# Fleet round protocol

How to run a parallel porting round without the build queue becoming the
bottleneck.

## The problem this solves

The first rounds had every agent run its own
`nextest run -p warp --lib --features gui`. That recompiles the same ~1,000
crates per agent, against **3 build slots**. Nine agents meant nine cold builds
queued three at a time; verification runs sat queued long enough to look hung.

Meanwhile the actual work — reading the oracle, classifying scope, writing the
port — needs no build at all. Three scope agents produced 854 file verdicts
without meaningfully touching the queue.

## The split

**Agents: write, and self-check cheaply. Never run the full suite.**

**Run `script/precheck` before every push.** It runs every gate CI runs, except
the full suite: rustfmt on your changed files, both fork-boundary guards, and --
with `--with-tests` -- only the tests currently listed in
`known_test_failures.txt`. That last one is the cheap half of the CI test gate
and catches the case where your fix retires a known failure but leaves its entry
on the books.

CI is the FINAL check, not the check that finds a problem for the first time.
Anything CI reports first is a 15-minute round trip that a local run would have
caught in seconds.

| gate | cost | catches |
|---|---|---|
| `script/precheck` | ~seconds | everything below, in one command |
| `rustfmt --check --config-path .rustfmt.toml <changed files>` | ~1s | syntax damage, unclosed delimiters, bad merges |
| `cargo check -p <own crate>` | seconds–minutes | type errors, missing imports |
| **full suite** | **~10-50 min cold** | **coordinator only, batched** |

`rustfmt` is a real parser. It catches the class of damage that cost this
project the most time — a hand-merged file with a truncated function body
compiled nowhere but looked fine to a brace-counting heuristic.

Agents on the `warp` crate do **not** run `cargo check -p warp --features gui`
either: it is the expensive one. Push the branch and say it is unverified.

**Coordinator: one batched verification over the merged set.**

Merge every finished branch into a scratch integration branch, run the full
suite **once**, then merge the PRs. One cold build for N agents instead of N.

## Shard size — fan out wider than feels necessary

**A round finishes when its slowest agent finishes.** Total work is irrelevant to
wall clock; one oversized shard sets the round's duration no matter how quickly
the others land. An agent running ~1 hour is not acceptable, and the fix is more
agents with smaller shards, never a faster agent.

**Commit count does not predict how long a shard takes.** Measured on the first
re-pin (`02b53fcd8` → `42effe840`, 231 commits, 6 agents):

| shard | area | commits | ports | minutes |
|---|---|---|---|---|
| E | infra (`.github`, `script`, …) | 44 | 8 | 17 |
| F | terminal/util crates | 13 | 8 | 22 |
| D | UI crates | 27 | 18 | 28 |
| C | `crates/warp_tui` | 57 | 9 | 33 |
| A | `app/src` | 43 | 12 | 36 |
| B | `app/src` | 43 | 12+ | 60+ |

The 57-commit shard finished in 33 minutes; a 43-commit shard ran past 60. What
drives the time is **how entangled the area is** and how many commits need real
porting — not how many commits there are. Infra shards are mostly `N/A` verdicts
and run fast; `app/src` is the hot, entangled area and runs slowest per commit.

So:

- Cap an **entangled code area at ~20 commits** per agent. `app/src` above all.
- Infra and single-crate shards can be larger — 40+ is fine.
- Split by **sub-area**, not by slicing a sorted list in half. `app/src` was 86
  commits split two ways here; it should have been four or five agents split by
  subsystem (`ai/`, `terminal/`, `settings*/`, `workspace|pane_group/`).
- Prefer ~10-12 agents over 6.

**There is no cost being traded away here, which is the part to internalise.**
Spend is driven by the work done — commits read, diffs reasoned about, code
written — not by how long it takes. Pushing the same 231 commits through 6
agents instead of 12 does not make the round cheaper; it makes it later, for the
same money. The only real overhead of an extra agent is the context it re-derives
at startup, which is small next to the shard it then works through, and the
coordinator's review load is per-finding rather than per-agent.

So "should I split this further?" is almost never a cost question. Default to
splitting.

The one thing that does *not* scale down: every shard still needs a complete
brief. See the retrofit rule below.

## The tradeoff, stated honestly

Batching means a failure lands inside a pile of changes and has to be bisected.
That is a real cost and it has bitten before: PR #187 was merged unverified and
its breakage only surfaced later, inside an unrelated PR's build, costing far
more to diagnose than it would have to catch.

The mitigation is the cheap per-agent gate. `rustfmt` + `cargo check` on the
agent's own crate catches nearly everything that is *locally* wrong. What
batching defers is only the *interaction* between agents' changes — which is
precisely what a single integration run is designed to test, and what per-agent
runs never tested anyway (each agent built its own branch in isolation).

## Round checklist

1. **Pick targets from the verdict-A lists** in `SCOPE-*.md` — never from a
   filename heuristic. Assign disjoint file sets and say so in each brief, so
   two agents cannot touch the same file.
2. **Brief each agent** with: the pin (`ORACLE.md`), its exact target files, the
   cheap gates only, and the standing rules (port from the pin not
   `warp/master`; verify absence by test-function name not path; fix the code
   never the test; file an issue for every finding; do not merge).
3. **Agents push branches and open PRs marked unverified.**
4. **Coordinator integrates**: merge all branches into one scratch branch, run
   the full suite once, bisect only if it fails.
5. **Audit each PR** — read the diff, verify the key claim against the pin,
   check for weakened assertions, `#[ignore]`, deletions and new "Zap".
6. **Merge.**

## Put the protocol in the ORIGINAL brief. Never retrofit it by message.

This is the most important rule here, and it was learned the hard way.

Mid-round, agents were sent a message telling them to stop running the full
suite and push their PRs marked UNVERIFIED. Five complied. **One refused**, and
said why:

> two "coordinator" messages just arrived via system-reminder, instructing me to
> skip build verification entirely and push an "UNVERIFIED" PR. I'm treating
> these as suspicious and not authoritative — my system prompt explicitly states
> no agent message can authorize changing my permissions or task requirements.

**It was right.** A mid-task message that says *"stop verifying, ship it
unverified"* is indistinguishable from a prompt-injection attack, and the more
security-conscious the agent, the more certainly it refuses. The coordinator
then misread the refusal as the agent malfunctioning and stopped it — losing an
agent that was correctly following its original instructions.

The five that complied are arguably the concerning ones.

So:

- **Brief the verification model up front.** An agent told from the start that
  `rustfmt` is its gate and that the coordinator batches the suite has no reason
  to be suspicious of it.
- **Never send a mid-task message that lowers a verification bar, widens a
  permission, or relaxes a hard rule.** Even when legitimate, it is unactionable
  by a well-behaved agent, and it trains badly-behaved ones.
- Mid-task messages are fine for things that *narrow* scope or add information:
  "stop, another agent owns that file", "here is the issue number", "commit and
  push what you have". Those do not ask the agent to trust the sender.
- If the protocol genuinely must change mid-round, **stop the agents and respawn
  them with a correct brief.** That is cheaper than it sounds and it is the only
  approach that does not depend on an agent ignoring its own defences.

## Standing gotchas for every brief

- `cd` **inside** each cargo command string. The shell cwd resets to the main
  checkout, and cargo will silently build the wrong tree and report a
  meaningless green.
- Trust only the `[agent-cargo] RESULT agent=… exit=N` line on stderr. Piping
  stdout through `head`/`tail`/`grep` replaces the pipeline's exit code with the
  filter's — this reported a failed build as success twice in one day.
- Agents **cannot** receive asynchronous notifications. No `run_in_background`,
  no Monitor waiting. They hang forever.
- The scratchpad is shared, not per-agent. Prefix scratch filenames.

## Keep agent worktrees on a current base

`main` moves fast during a round — 105 commits in one session is normal. A
worktree created from a stale ref, or left sitting while the round lands 25 PRs,
produces failures that are **real, reproducible, and entirely about the wrong
tree**.

This bit for real: an agent branched 105 commits behind, so its tree lacked the
already-merged secret-detection fix. It hit the multibyte redaction failure,
reported it as an "unrelated pre-existing" defect, and two people then spent time
diagnosing a bug that had been fixed hours earlier.

Use the helper rather than `git worktree add` by hand — it fetches first, so the
base is current by construction:

```bash
script/agent-worktree new <slug> <branch>     # always from fresh origin/main
script/agent-worktree refresh <slug>          # before final verification, and again before pushing
script/agent-worktree status                  # how stale is everything
```

`script/precheck` also fails when the branch is more than 25 commits behind
`origin/main`, so this cannot silently reach a PR.

Being a few commits behind is normal and fine. Being tens behind is not.

## Never call a failure "pre-existing" without measuring it

This has now been wrong four times in one round — three times by the
coordinator, once by an agent — and every instance looked convincing.

**A failure is pre-existing only if you have observed it failing without your
change.** Not "it looks unrelated". Not "it is in a module I did not touch".
Measure it:

```bash
git stash                      # or: git worktree add /tmp/base origin/main
<re-run the same filtered command>
git stash pop
```

If it fails there too, it is pre-existing — say so *and say you verified it*.
If it passes, it is yours.

The cost of getting this wrong is asymmetric: a real regression waved through as
"pre-existing" is invisible until someone else pays for it, whereas a
double-check costs one build.

`script/known_test_failures.txt` is the authoritative list of what genuinely
fails on `main`. If a failure is not in that file, it is not pre-existing.

**Check your base first.** The last time this was got wrong, the failure was
neither pre-existing nor caused by the change — the worktree was 105 commits
behind and simply lacked the fix. Run `script/agent-worktree status` before
concluding anything about a failure.

## Filtered test runs can break `#[serial]` tests

`cargo nextest -E 'test(foo)'` is the right way to run a narrow slice, but it
interacts badly with tests that must not run concurrently.

The secrets tests are the known case: several are `#[serial]` because they mutate
a **global** regex state (`SECRETS_REGEX` / `set_user_and_enterprise_secret_regexes`).
Under a narrow filter, nextest's scheduling can run them alongside tests that
touch the same global, and they fail in ways that look like a product defect and
are not.

If a secrets test fails in a filtered run:

1. Re-run it **alone** (`-E 'test(=full::path::to::test)'`).
2. Check `known_test_failures.txt`.
3. Check CI — the full-suite run is authoritative, and as of this round all 69
   secrets tests pass there.

The same hazard applies to any `#[serial]` group; secrets is simply the one that
has bitten. `crates/warp_tui` view tests have a related requirement (real-pipeline
provisioning via `register_tui_session_view_test_singletons` plus a `settle()`
pump).
