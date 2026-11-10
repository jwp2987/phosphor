# r10 unit-plugins: 13 failing plugin_manager unit tests on the Linux box

Pipeline 3048013125, job test-linux 17885063200 (commit 413cd4241): 13 tests under
`terminal::cli_agent_sessions::plugin_manager::{claude,codex,gemini}::tests` fail. No issue number
exists for this group; it comes from the round 9 unit run.

## Verdict: TEST fault (one cause for all 13). No app change.

### Cause

Each of the 13 tests ends with `assert_all_local(&runner)`, which scans every recorded command line
for the words `http`, `github`, `gitlab`, `git@` (plus `ls-remote`, `upgrade`, ` update ` per
agent). The scan covers the whole command, including the folder argument:

- `app/src/terminal/cli_agent_sessions/plugin_manager/claude_tests.rs:623-631` (panic at `:628`)
- `app/src/terminal/cli_agent_sessions/plugin_manager/codex_tests.rs:678-686` (panic at `:683`)
- `app/src/terminal/cli_agent_sessions/plugin_manager/gemini_tests.rs:433-441` (panic at `:438`)

The folder is a `tempfile::tempdir()` made by the test (`make_bundle`). On the box the temp
directory is under the runner's home: `.gitlab-ci.yml:156` sets
`PHOSPHOR_TARGET_ROOT: /home/gitlab-runner/.phosphor-build/targets`, and the temp root is
`$PHOSPHOR_TARGET_ROOT/tmp`. So the command is

    claude plugin marketplace add /home/gitlab-runner/.phosphor-build/targets/tmp/.tmpszJXX1/bundled/claude_plugin

and the word `gitlab` is found in `/home/gitlab-runner/`. The panic message in the job log is
exactly that command text (the assert prints `{call}`), in all 13 failures. Each fails in about
0.02 s: nothing ran, the scan failed.

### Evidence that the app is right

- The same 13 tests assert `runner.calls()` against the exact expected list just before the scan
  (for example `claude_tests.rs:644-647`), and that assertion passed: the panic is at the scan line,
  not at the `assert_eq!`. The command sequence is the one the tests expect.
- The tests of the same managers that do not call `assert_all_local` pass in the same job
  (`a_failed_add_restores_a_previous_local_bundle_folder`, `install_without_the_bundle_...`,
  `move_with_nothing_to_move_runs_nothing`, and the rest).
- The failing set is exactly the set of `assert_all_local` callers: 5 in claude, 5 in codex,
  3 in gemini (`grep -c 'assert_all_local(&runner)'`). OpenCode has no such helper and no failure.
- `git log 41340e2fd..HEAD -- app/src/terminal/cli_agent_sessions/plugin_manager/` is empty: round 10
  did not touch this code, so the result holds on this branch too.

### Why it was not seen before

The tests were added on 2026-11-09 (`037f695d0`, `fa7f05c5b`). This was their first run on the box.
On a developer machine the temp directory is `/tmp` or a job directory with no such word in it.

## Fix

Cut the folder the test itself created out of the command before the word scan. The folder is local
by construction; the rest of the command (the CLI, the subcommand, any other argument) is still
scanned with the same word list, so the tests still fail on a remote URL, a `git ls-remote`, an
`upgrade` or an ` update `.

- `mod_tests.rs`: new `pub(crate) fn without_local_path(command, local) -> String` and a unit test
  of it (the path is removed even when it contains `gitlab`; a remote word elsewhere stays).
- `claude_tests.rs`, `codex_tests.rs`, `gemini_tests.rs`: `assert_all_local(&runner, &bundle)`.

This is not a weaker test: the claim it checks is "no command touches the network", and the name of
the temp directory was never part of that claim.

## Rejected

- Changing the box's temp directory or the runner's user name: a box change, needs the owner, and
  hides a test that would also fail for any developer whose checkout or temp path contains
  `github` or `gitlab`.
- Dropping `gitlab` from the word list: weakens the test.
- Skipping every word that is an absolute path: looser than needed, and wrong for paths with spaces
  (the recorded command is a plain join).

## Left as is (same class, did not fail)

The four `instructions_*_no_remote` tests (`claude_tests.rs:432`, `codex_tests.rs:108`,
`gemini_tests.rs:263`, `opencode_tests.rs:149`) scan instruction text that carries the resolved
bundle folder of the running build. They passed on the box. They would fail the same way if that
folder ever resolved under a path with one of the words. Not changed here: the text is shell-quoted
(a `file://` entry for OpenCode), so cutting the path out safely needs a compile and run to confirm.

## Risk

Test-only. Compile doubts are in the report.
