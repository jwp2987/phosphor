# #2156: batching the no-helper side calls to at most one exec per refresh

Status (round 25): step 1 (a debug line per exec) and levers A and C are implemented in
`backend/shell/git_batch.rs`, behind `PHOSPHOR_SHELL_BATCH_GIT=0`, with unit tests
(`git_batch_tests.rs`); written but NOT compiled or run (no cargo on the authoring machine), and not
measured in a lab. Lever B (one shared `ls-files` for tree + index) and the tree refresh joining the
batch are NOT built, see "What was built" at the end. The r24a lab artifacts had expired, so the
per-producer split was not re-measured; the order of work followed the inventory estimates below
(git_status 5-12 and diff_state 6-14 execs, the largest). Related: #1800 (laggy no-helper typing), #1240 (the
unbounded mux clients; closed, its in-flight limit stays).

## The problem

Round 15 Linux lab, no helper, bash and zsh: one `cd` into a git repository opens 54 ssh exec
channels for one prompt (90 for two). The lab cases `lab_chips` and `lab_fam_*` allow at most one
exec per prompt (`script/realapp/appci/cases_lab.py` `c_chips`, `cases_lab_helper.py` `fam_body`:
`1 <= calls <= prompts`). Every exec is a fresh `ssh` mux client on the ControlMaster, so each
costs a fork, a channel open and a login-shell start on the host; they also compete with the
user's typing for the one connection (#1800).

## Step 1: attributing the count (done)

One `log::debug!` per exec, from the three shell-backend exec points:

| line | where | names |
|---|---|---|
| `shell exec: producer=git_runner family=git:<sub>\|gh:<sub> elapsed_ms ok` | `ShellGitRunner::exec` (`backend/shell_git_runner.rs`) | the git/gh subcommand only (no paths, no values) |
| `shell exec: producer=run_script sections=[a,b] elapsed_ms ok` | `ShellBackend::run_script_stdin` (`backend/shell/mod.rs`) | the section names of the script |
| `shell exec: producer=index_read files=N elapsed_ms ok` | `ShellBackend::read_batch` (`backend/shell/index.rs`) | batch size |

Not covered: the PowerShell git path (`shell_git_runner/pwsh_run.rs:96`) and the direct executes in
`shell/write.rs` / `shell/buffer.rs`; they are not on the cd path. The next lab run should
`grep "shell exec:"` in the app log between the cd and the next prompt; that gives the split the
issue marks UNPROVEN. The estimates below are from reading the code and must be replaced by it.

## Inventory: what one cd runs (no helper)

`n` = execs. Lower bound assumes the main branch is cached and the upstream exists.

| producer | entry | what it execs | n |
|---|---|---|---|
| repo_metadata / file_tree | `shell/tree.rs` `git_snapshot` (:139) via `refresh_repo_tree` (:374) / `load_repo_metadata_directory` (:230) / `navigate_to_directory` (:185) | one script, 2 sections (`files` = ls-files, `ignoredpaths`); falls back to `lazy_root` (:169, 1 more) when the list is too big or not git | 1-2 |
| codebase chunk scan | `shell/index.rs` `scan_codebase_shell` (:123), `host_store` (`shell/host_store.rs`) | `resolve` store dir (once per repo, cached), `base64_tool` probe (once per host, cached), one `scan_page_command` exec PER PAGE of `SCAN_PAGE_FILES` files (:161; page 0 also counts files, so it does its own ls-files), then `read_batch` per batch of files to chunk (:599) | 2-3 warm, +1 per page/batch |
| git_status | `shell/status.rs` `git_status_shell` -> `shellcmd/git/status.rs` `compute_git_status_metadata` | `detect_main_branch` (`ops.rs:43`: symbolic-ref, then up to 5 `rev-parse --verify`, then `branch --show-current`; skipped when cached), `detect_current_branch_display` (1-2), `status` (1), `diff --numstat HEAD` (1), `rev-parse @{u}` (1), `rev-list --count` (1) | 5-12 |
| github PR | `status.rs` `pr_info_shell` -> `gh.rs` | `detect_current_branch` (1-2), then if the branch moved: `rev-parse --is-inside-work-tree`, `symbolic-ref --short HEAD`, `gh pr view` | 1-5 |
| github repo | `status.rs` `repository_info_shell` -> `gh.rs:101` | `rev-parse --is-inside-work-tree`, `gh repo view` | 2 |
| diff_state | `shell/diff.rs` `get_diff_state_shell` -> `shellcmd/git/diff.rs` `compute_snapshot` (:169..) | `rev-parse --git-dir`, `detect_main_branch` again (1-7), `detect_current_branch` again, `rev-parse HEAD`, `status` again (:212/:391), per-mode `diff --name-status`, `merge-base`, numstat | 6-14 |

Sum for a repo with no `origin/HEAD`: 1 + 3 + 12 + 5 + 2 + 14 = the observed 40-54. The same
questions are asked up to four times (main branch, current branch, status, `is-inside-work-tree`)
because each producer is a separate async call over a `GitRunner` that execs per git call
(`shell_git_runner.rs` `exec`, one per `GitRunner::run`).

Triggers: the cd's Precmd fires `repo_metadata`, `codebase_chunks` (`hop=remote_index_sync
start reason=Navigated`), `git_status`, `github` x2 and `diff_state` back to back (see the
`backend_route` lines in the evidence log, 19:36:43). Each is independent; nothing coalesces them.

## Design

Two levers, both in the shell backend (the helper path is untouched, it pushes):

### A. A batching git runner (the big win, and the first slice)

Replace "one exec per git call" with "one exec per refresh" for the read-only chip/diff/PR
questions, WITHOUT rewriting the shared algorithms (`shellcmd/git/*` is shared with the helper and
has its own tests). Mechanism: a `PrefetchGitRunner` wrapping `ShellGitRunner`:

1. A refresh (one `git_status_shell` / `pr_info_shell` / `repository_info_shell` /
   `get_diff_state_shell` call set for a repo+prompt) first runs ONE exec, a framed `Script`
   (`shellcmd::Script`, nonce-delimited sections, exactly as `agent_context.rs` does) whose
   sections are the fixed set of read-only git calls the algorithms are known to make:
   `symref` (symbolic-ref origin/HEAD), `verify_<candidate>` for the five candidates, `curbranch`
   (rev-parse --abbrev-ref HEAD, branch --show-current, rev-parse --short HEAD), `status`
   (STATUS_CHIP_ARGS), `numstat`, `upstream`, `counts`, `inside_wt`, `symref_head`, `head`
   (rev-parse HEAD), `gitdir`. Each section stores `args`, stdout, stderr, exit code.
2. The `PrefetchGitRunner::run(args, stdin: None, cwd)` looks the call up by `(cwd, args)` in the
   prefetched map and returns the stored `GitOutput` with no exec. A call not in the map
   (different args, a write, any stdin) falls through to the real `ShellGitRunner::run` (one exec,
   exactly today's behaviour). The map lives for one refresh generation and is dropped after
   (never reused across prompts: stale answers are the failure mode to avoid).
3. `gh` calls (`gh repo view`, `gh pr view`) are NOT in the cheap script: they are the network
   round trip and run only when `pr_info_shell` decides to (branch moved, `force`). They are
   appended to the same script as sections only when the caller asked for PR/repo info in this
   refresh and `gh` exists; otherwise they stay separate execs (1-2).

Because the algorithms keep calling `runner.run(...)` with the same args, the sequencing
(`detect_main_branch` stops at the first success) still behaves identically: the prefetch simply
holds the answers. The cost is that the single script runs a few git calls the algorithm would
have skipped (the later verify candidates after the first success). Sections are sequenced with
short-circuit inside the script for the main-branch chain (shell `||`), which also keeps it to one
exec; the sections that depend on others (`counts` only if `upstream` succeeded) are guarded the
same way. Result: git status + branch + ahead/behind + PR branch check + diff_state's git calls
become 1 exec; `gh` adds 0-1 (appended section when wanted).

Alternative considered and rejected for now: a new `git_status_all` shell function and a rewrite
of `compute_git_status_metadata` to parse one `git status --porcelain=v2 --branch` output. It is
less code on the host but forks the algorithm from the helper's, so the helper and shell values
could diverge; the chip contract is "identical values". The prefetch keeps one algorithm.

### B. One shared ls-files for the tree and the index

`tree.rs` `git_snapshot` already runs `repo_files_command` (ls-files) and the ignored list in one
script; `scan_codebase_shell` page 0 lists the same repo again. Make the scan take its file list
from a short-lived per-repo cache filled by the tree refresh of the same generation (or have the
scan's page 0 and the tree share a `RepoListing` handle keyed by repo + generation). Saves 1-2
execs and the duplicated ls-files cost on a large repo. Index page/batch execs remain (they scale
with repo size, not with prompts); they are rate limited by the `remote_index_sync` debounce and
should be excluded from the per-prompt budget by being deferred: do not start `remote_index_sync`
until the prompt-triggered batch is done (see C).

### C. Coalesce one precmd into one refresh

A `RefreshCoalescer` per (host, repo): the first request for a repo opens a generation and waits a
short settle window (target 30-50 ms, a constant, not a user setting); every request for that repo
in the window joins it. The generation runs ONE batched script (A) and fans the answers out to
`git_status`, `github`, `diff_state`, then the tree (B). A second precmd for the same repo while a
generation is running marks it dirty and runs exactly one more after it; it never queues N.
This is what bounds execs to "one per prompt" when the user hits Enter repeatedly.

### Per-section failure (do not drop the others)

Same contract as `backend/shell/agent_context.rs`: the script is a `Script` of independent
sections, a section is parsed only if `section.success()`, and a failed or absent section yields
its algorithm's existing absent answer (no upstream, no PR, `Ok(None)` status) while the others
are used. Concretely: a missing `gh` or a failing `gh pr view` leaves the git sections intact; a
repository without `HEAD` fails `numstat`/`counts` and keeps `status`/`branch`. A whole-script
failure (exec error, truncated, no frame) falls back to today's per-call path for that refresh
(never an empty chip), and a prefetch miss for any single call also falls back to one exec. Output
cap: the combined output uses `MAX_EXEC_OUTPUT_BYTES`; a `truncated` result drops the prefetch
and falls back, as `run_script_stdin` already refuses truncated output.

### Kill switch

`PHOSPHOR_SHELL_BATCH_GIT=0` (read once when the `ShellBackend` is built, checked next to the
other remote kill switches) disables A, B and C and restores the exact per-call behaviour. Default
on once the lab case passes. A second flag is not needed: A is the only part that changes what is
exec'd; B and C only reorder.

### PowerShell hosts

`ShellGitRunner` already refuses git on PowerShell (`exec`), and `pwsh_run.rs` is a separate path.
The first slice is POSIX/fish/nu only; PowerShell keeps today's behaviour.

## How the lab proves it

`c_chips` and `fam_body` already assert `1 <= calls <= prompts` on sshd exec-channel starts with
no helper, and `calls == 0` with the helper. They stay UNCHANGED (never weaken). The new debug
lines make a failure attributable: on a red run the report lists the `shell exec:` lines between
the cd and the next prompt, grouped by `producer`/`family`. Add to the case, as a metric only, the
per-producer counts parsed from those lines (`side_calls_by_producer`), so a regression names its
producer. The two-prompt arm (90 -> expect <= 2) covers the coalescer; the 150 ms arms cover that
the settle window does not add a visible delay (chip still on screen within the existing OCR
loop).

Rust unit tests for slice 1 (to be run on the box by the coordinator):

* a `PrefetchGitRunner` over a scripted executor counting execs: `compute_git_status_metadata`
  returns the same `GitStatusMetadata` as over the plain `ShellGitRunner` (existing
  `shell_git_runner_tests.rs` scaffolding) with exactly 1 exec; same for `get_diff_state_shell`
  and the PR branch check;
* a failing section (no upstream, no `HEAD`, gh absent) leaves the other answers intact;
* a call not in the map falls through (1 extra exec); a truncated script falls back wholesale;
* a generation is not reused across a second request (stale-answer test);
* the kill switch restores N execs.

## Why only the design now

The first slice is not small: the prefetch script has to reproduce the main-branch chain and the
guards of `counts`/`gh` in POSIX, fish and nu syntaxes (`ShellKind::wrap` only wraps a POSIX
script; the framed `Script` renders per kind), and its output must be proven identical to the
helper's. That needs the box to compile and the lab to run. Order of work once approved: (1) get a
lab run with the Step 1 lines to replace the estimates above; (2) slice 1 = A for the git status
family only (`compute_git_status_metadata` is the smallest consumer); (3) extend A to PR/repo and
diff_state; (4) B; (5) C.

## What was built (round 25, #2156)

Levers A and C, in `backend/shell/git_batch.rs`; PowerShell hosts and the kill switch keep the
per-call path.

* **A. `BatchedGitRunner`** (the design's `PrefetchGitRunner`). `ShellBackend::batched_git_runner`
  gives `git_status_shell`, `pr_info_shell`, `repository_info_shell` and `get_diff_state_shell` a
  runner whose answers come from ONE framed `Script`. Sections: `gitdir`, `symref`,
  `verify0..4`, `showcur`, `abbrev`, `shorthead`, `head`, `statuschip`, `statusfull`, `numstatz`,
  `numstathead`, `upstream`, `counts`, `insidewt`, `symhead`, plus the dependent ones `unpushed`,
  `forkunique`, `forkparent`, `forklog` (the unpushed-commits log against the upstream, or the fork
  point chain when there is none) and `ghpath`. The five main-branch candidates are not
  short-circuited in the script: they are cheap local `rev-parse`s and each section keeps its own
  real answer. Every section reuses `ShellGitRunner::git_command_prefix` (the same `env -u ... git
  -c ...`), so the environment and base arguments are the per-call ones. Lookup is by exact
  `(repo, args)`; a call with other arguments, any stdin, or another directory is one ordinary exec
  (per-file diffs, `merge-base`, a branch-mode `diff`). The argument lists are constants shared with
  the algorithms (`MAIN_BRANCH_CANDIDATES`, `UNPUSHED_LOG_FORMAT` in `shellcmd/git/ops.rs`,
  `STATUS_*_ARGS`).
* **Dependent sections print the value they used.** `unpushed`, `forkunique`, `forkparent` and
  `forklog` print the upstream range, the current branch, the fork target or the log range as their
  first line; `assemble` files the rest under the key the Rust algorithm will ask for only if its
  own computation of that value is equal. A mismatch is a miss (one exec), never a wrong answer.
* **`gh`.** Not run in the batch. The script asks only `command -v gh`; when it is absent
  `run_gh` returns exit 127 "not found" with no exec (what the host's `sh` says), so a host with
  no `gh` costs no extra channel. With `gh` present the lookup stays its own exec, and runs only
  when `pr_info_shell` decides a lookup is due.
* **C. Generations.** `GitBatchStore` (one per host, shared by every backend the router builds)
  keeps one generation per repo path: a `futures::Shared` future. A request for a repo whose
  generation started less than `JOIN_WINDOW` (100 ms) earlier joins it, otherwise it starts its own;
  the answer is never reused beyond that, and any write (`git_write_parts`, `write_file_outcome`,
  `delete_file_expecting`) clears all generations. No timer: it compares instants. Joiners therefore
  get answers read up to the window before their request; a second prompt inside 100 ms of the
  first prompt's refresh is the only way to see an older answer, and the next refresh event
  corrects it.
* **Per-section failure.** Same contract as `agent_context.rs`: a section is a miss only for its own
  calls when it is absent from the frame; a section that ran and failed holds the real failing
  answer (no upstream, no HEAD). A script that fails as a whole (exec error, truncated output, no
  frame) means no prefetch for that refresh and every call takes the per-call path.

### Not built

* **B. Shared `ls-files` for the tree and the index scan.** The tree refresh (`tree.rs`
  `git_snapshot`, 1-2 execs) is still its own script, and the index scan (`index.rs`, one exec per
  page of files plus the file reads) is unchanged. Folding the tree's two sections into the batch
  needs the executor's truncation to be survivable per section (a big repository's file list would
  otherwise discard the git answers too); the index scan is content-dependent (hash, then read the
  new files) and cannot be one exec. After this change a `cd` should cost about the batch (1) + the
  tree refresh (1-2) + the index work, down from the observed 17-55; the lab's `1 <= calls <=
  prompts` limit still needs the tree and the index debounce (the design's "do not start
  `remote_index_sync` until the prompt batch is done") before it can pass.
* The settle window of C (waiting 30-50 ms before the first script) is not used: the first
  request execs at once and the others join it, which adds no delay to the chip.
