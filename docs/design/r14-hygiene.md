# Round 14: test hygiene (r14-hygiene)

Branch `fix/r14-hygiene`, from `integ/r14` `4bf571aae`. Nothing here was compiled or run with
cargo; Rust is `rustfmt --check` only, the shell and Python pieces were run directly. Wording:
"worked, unproven" until a run shows it. No app code changes: every item is a test, a harness, a
guard or a runbook.

## 1. #1907: `script/test_child_ps1` ran nowhere

Cause: `docs/testing.md` 2.4 lists it under "run by `precheck`", but `script/precheck`'s "shell
tests" step runs a fixed list (`script/precheck:359`) that did not name it, and `.gitlab-ci.yml`
`guards` runs only `macos/test_create_warpctrl_wrapper` and `test_warpctrl_early_dispatch`
(`.gitlab-ci.yml:132`). The brief said to copy how `test_init_shell_escaping` is wired: it is
wired nowhere either, which is how it stopped matching the bootstrap unnoticed until round 12.

Fix: both go into the `precheck` list; `test_child_ps1` also into `guards` (python3, bash, sh:
all on the box; about 12 s). `test_init_shell_escaping` is not added to `guards`: it wants zsh,
fish and pwsh and what the box has was not checked from here.

Found on the way, filed as #2055, not worked: six more harnesses run by nothing
(`test_fish_bootstrap`, `test_hook_forks`, `test_line_editor_markers`, `test_hijack_bootstrap`,
`test_bash_preexec_large_command`, `test_zsh_hex_encoder`). All six pass when run directly. The
three slow ones add about 3.5 minutes to a gate meant to take seconds, so where they run is a
decision, not a one-line list change.

Rejected: a loop over `script/test_*` in `precheck` (it would pick up the slow ones and the
`.ps1`, and a new file would run before anyone looked at its runtime).

## 2. #2049: the five `webfetch_tests::real_*` live checks

Cause: `webfetch_tests.rs:409` `skip_real()` returns early when `WARP_RUN_WEB_INTEGRATION` is
unset, so each test is reported PASS having checked nothing.

Fix: the opt-in of `websearch_tests::real_exa_anonymous_search` (#2022): `#[ignore]` with the
instruction as its reason, and `require_live_opt_in(name)` first in each body, an `assert!` that
fails with the instruction when the variable is unset (before a client is built, so nothing is
sent). Assertions on the replies are unchanged. The unit pass count drops by five and the
skipped count rises by five; that is the point.

Rejected: one shared helper for both test files (they are separate modules of `web_runtime`; a
shared item would have to live in production code or a new test-support module for 6 lines).

## 3. View tests that read the pty recorder inside the update that wrote

Rule (r13-pin2): `ctx.emit(Event::WriteBytesToPty ..)` queues the event
(`warpui_core/src/core/view/context.rs:400`); subscribers run in `AppContext::flush_effects`
(`core/app.rs:4276`) when the outermost update returns. Inside `terminal.update(..)` a recorder
holds only what earlier updates wrote.

Audit of `app/src/terminal/view_test.rs` at `4bf571aae` (text scan, then each site read):

- 389 `pty_writes.borrow()` reads; 72 of them (51 assertions) are inside an `.update(` closure.
- 9 more statements are inside `with_ssh_hold_path` bodies: the helper runs the whole body in
  ONE update and handed it the recorder, so every read in a body was in-update. A scan for
  `.update(` cannot see these.
- The same pattern under other recorder names, also read: `written` (`subshell_pty_writes`) 6,
  `etx_writes` 1, `retries` 1, `executed` 8, `delivered` 1.
- No exact non-empty content assertion sits inside an update today (the one that did was the
  round 12 failure r13-pin2 fixed). Every in-update read is `is_empty()` or a length.

Verdict per statement. "Carried" means the claim is proven by other assertions, so the in-update
read is redundant, not wrong:

- **real read**: nothing is emitted before it in its update (it reads what earlier updates
  wrote).
- **carried by a later read**: a read after the update, before any release, is still empty
  (writes only append).
- **carried by state + once**: the same update asserts the held entries (a count, a non-empty
  hold) and the read after it has every byte exactly once, or is one exact chunk, or has an
  order an early write would break.
- **vacuous**: none of the above. Either nothing reads the recorder after the update at all
  (the test claimed "nothing reached the pty" and proved nothing), or a release follows in the
  same update and only a flag (`ssh_typeahead_holding`, a gate open) stands between "held" and
  "written early, then not written again", which the final read cannot tell apart.

| verdict | statements | what was done |
|---|---|---|
| vacuous, nothing read after the update | 13 in `.update(` closures, 9 in `with_ssh_hold_path` bodies | the same assertion, moved after the update (bodies: the helper asserts after its update) |
| vacuous, a release follows in the same update | 12 `pty_writes`, 1 `written`, 1 `retries`, 2 `written.len()` pairs | the update is ended at that point, the recorder read, the next update continues the same sequence |
| carried / real read | 27 `pty_writes` / `written` statements, `hold_a_young_enter`'s read, 2 in `view/*_tests.rs` | kept; a comment `In-update recorder read ...` says what carries each |
| other recorders, carried or diagnostic | `etx_writes` 1, `delivered` 1, `executed` 8 | kept unchanged (start-of-update reads, order or exact final reads, or message arguments) |

No assertion was removed or weakened. A moved "is empty" now covers the whole update instead of
its first half, which is stronger; a split one is checked at the same point of the sequence.

What a split changes: the events the first half emitted are delivered before the second half
runs (as in the app, where each key is its own update). Only the test's recorder subscribes to
the view's events in these tests (`test_util/terminal.rs` builds no pty controller), timers and
the model-event stream need the executor (an `await`), and the tests have none between the
halves. So the sequence is the same. No `drain_model_startup_events` was added: it would
dispatch the window's startup events before the setup, which these tests never had.

If a changed test fails in the next unit run, the old assertion could not fail, so the failure
is new information: either the app writes where the test said it does not (APP), or the test's
claim was never true of the sequence it builds (TEST). Read the recorder's contents in the
message first.

Guard: `script/check_recorder_reads` (text scan; `--self-test`). It flags a read of
`pty_writes` (or a variable bound from a recorder helper) inside an `.update(` closure in
`view_test.rs` and `view/*_tests.rs` unless a comment starting `In-update recorder read` sits
in the 8 lines above it. Limits, stated in its header: closures handed to a helper, helpers
taking the recorder as a parameter, and recorders under other names are not seen; it cannot
tell vacuous from carried, the marker is the author's statement.

Rejected: a synchronous recorder hooked into the view (test-only app code); removing the
carried in-update reads (it reads as weakening, and the comment is cheaper than the argument);
extending the scan to `&pty_writes` arguments (8 sound call sites of `hold_a_young_enter` would
need markers for no gain).

Risk: nothing was compiled. The edits are mechanical (an update closed and reopened, a value
returned from a closure in two tests, one helper signature); `rustfmt --check` passes, which
proves the braces, not the types.

## 4. #1836: prove the corpus catches #1796 / #1797 / #1799

The note in `docs/testing.md` said: branch from `ffdaa4536`, cherry-pick the fuzzer, run. Tried
by git alone in a scratch worktree (no cargo):

- The seven harness commits (`a9d68642c b9c32b7c4 04613e221 855f52bf0 82b0d3c0c d15f9488c
  2dcc91d2a`) cherry-pick onto `ffdaa4536` with conflicts only in `docs/testing.md`
  (`d15f9488c`, the hop lines, must be included or `2dcc91d2a` conflicts in `action_model.rs`).
- But that harness was written on top of the fixes and was never compiled as it stood. It took
  six later commits to build and run (`6f2940b79 4e34183dc 41e70bf6e 2c2b0aaad 32a65d635
  9cadef5f9`), each mixed with unrelated files; their harness hunks no longer apply to the
  picked tree (`rig.rs` conflicts, `mock_wiring.rs` does not exist there).
- So the pre-fix replay needs a compile loop of unknown length on the box, and would test the
  oracle of 2026-11-08 (I1-I7, before the I4 / I8 / Ctrl-C rework), not the one in use.

Chosen: put each bug back into the tree under test and replay today's corpus with today's
oracle (`script/ci/agent_fuzz_catches_wave0.py`). Each fix left a switch, so each bug is a
one- or two-line edit that only disables a branch (the tree still compiles):

| arm | edit | must fail |
|---|---|---|
| `m1799` | `rejection_cancels_rest_of_turn` not consulted; `transfer_resolves_without_approval` false | corpus `wave0_1799_reject_then_transfer` |
| `m1797_empty` | `has_no_answerable_questions` not consulted in `should_autoexecute`; the empty-list error return in `execute` off | corpus `wave0_1797_ask_empty_args` |
| `m1797_valid` | `execute` drops the staged answer | corpus `wave0_1797_ask_valid` |
| `m1796` | silence past the notice window counts as a tty read in `PromptDetector::observe` | `prompt_tests::a_prompt_lookalike_is_never_handed_over_while_the_foreground_is_not_reading` |

A control arm (no edit) must pass first. The script applies an edit only when its text occurs
exactly once (`--check` does that with no cargo, on any checkout), parses the corpus replay's
failure message for the scenario and its invariant, and prints CAUGHT / NOT CAUGHT /
INCONCLUSIVE per arm. A NOT CAUGHT is a result too: it means a later defence masks the bug or
the oracle has a gap, and gets its own issue.

Not known until it runs: which invariant each scenario fails with today (the 2026-11-08 note
predicted I1 / I2a), and whether a second defence added since (#1851, #1858-#1861) keeps a
scenario green with its first fix off. `wave0_1796_silent_then_handoff` cannot fail at L1 (no
pty); its proof is the prompt test.

Rejected: reverting the fix commits at HEAD (`git revert` of `83071b08a` conflicts in
`action_model.rs`, `block.rs` and three test files, and would remove tests other code now
depends on); a test-only switch in the app to bring the bugs back (app code for a proof).

Risk: the script's parsing of nextest output is tested on fixtures only (`--self-test`); the
first box run checks it against real output. It runs cargo and must only be run on the box.
