# r19-units: the unit and integration failures of round 14 (build/r14a, pipeline 3052606915)

Branch `fix/r19-units` from integ/r15 `4da375d0f`. Nothing was compiled or run when this was
written (`rustfmt --check` only): every change below is worked, unproven.

Evidence: the job traces, and for `test-windows` the job's `nextest-windows.log` artifact. The
trace of `test-windows` stops at GitLab's 4 MB log limit after the third failure; the artifact
has the whole run: **47 failed, 1 flaky**, not 3. The integration jobs keep no artifacts.
None of these jobs has a result in a recorded round before round 14 (`test-results/`: "manual,
not run"), so "did it fail in round 13" has no answer for any of them.

## 1. test-workspace-rest-windows: `remote_server` lib tests do not compile (TEST, #2121)

12 errors, all in `crates/remote_server/src/backend/contract_tests.rs` (5 x E0433 `std::os::unix`,
7 x E0599 `from_mode` / `mode` / `uid` / `gid`). The file is the contract harness: it runs every
assertion against a local POSIX `sh` standing in for a remote host, with unix permissions, owners
and symlinks. It was declared `#[cfg(test)]`, so Windows built it. rustc reported type errors
too, so the whole crate was checked: no other file of the crate fails.

Fix: `#[cfg(all(test, unix))] mod contract_tests`. Its two users follow: the router test that
takes its `LocalShExecutor` is `#[cfg(unix)]`, and `index_sync_tests.rs` (every test but one
drives `scratch_home_executor`) is declared for unix; the one test that runs no shell moved to
`index_sync_pure_tests.rs` and still runs everywhere. No assertion changed. On unix the same
tests run as before.

Rejected: per-test gates inside the harness (the executors themselves are unix code); running
the harness against Git Bash's `sh` on Windows (owner and mode assertions have no meaning there).
Not changed: `shell_git_runner_tests.rs` also starts `sh` and has no gate; it compiles on
Windows and has never been seen running there. The next Windows run shows whether it holds.

Risk: other crates of that job never got to compile after this error; the next run may show more.

## 2. test-windows (47 failed, 1 flaky)

| tests | verdict | evidence | change |
|---|---|---|---|
| 35 `agent_fuzz` (matrix 28, corpus replay 2, fuzz 4, allowlist 1) | TEST, #2121 | every panic: `I6: 1 ERROR log line(s) ... Failed to run command to check if in git repository: program not found`; the same run logs `No history file found for shell zsh` and `Generator for executable names failed: program not found`. `TerminalModel::new_for_test` announces a zsh session with no launch data, so `LocalCommandExecutor` starts `zsh`; the runner has none | the rig gives the session the app's own local executor with the bash found on PATH when zsh is not installed (`rig.rs`, `Sessions::set_command_executor_for_all_sessions`). With zsh installed nothing changes; with neither shell the rig panics with that reason. I6 and the oracle are untouched |
| `aws_creds::process::tests::splits_like_the_cli_for_quotes_and_plain_words` | TEST for the failure, APP next to it (#2112) | `left: [.., "'single", "quoted'"]`: on Windows the AWS CLI splits with the C runtime's rules, a single quote is a character; the test asked for POSIX grouping everywhere | the expectation is per platform. #2112: the Windows split had no backslash rule; it is now botocore's `_windows_shell_split` and is tested on every platform |
| `agent_shell_wrap::tests::real_shells::pwsh_bootstrap_and_prompts_add_no_error_records` | APP, #2111 | `after-prompt-0 errors=1 ... ERR: The term 'node' is not recognized`: `Get-Command ... node 2>$null` records the failed lookup in `$Error` | `-ErrorAction Ignore` on the three lookups (node twice, git); a new test runs the prompt with an empty PATH |
| `plugin_manager::codex::tests::update_of_the_current_bundle_registration_only_adds_the_plugin` | TEST, #2121 | calls were `marketplace add C:\Users\...` then `plugin add`: the registration was not seen. The fixture wrote the folder into `source = "C:\Users\..."`, which is not TOML | the fixtures write a TOML string (`toml_string`), with a test that a Windows folder reads back |
| `plugin_manager::opencode::tests::classifies_bundle_folder_entries_at_any_install_location`, `a_bundle_entry_from_another_install_location_is_moved_to_this_one` | TEST, #2121 | `file:///opt/phosphor/...` -> `None`; the old entry was left in place. `Url::to_file_path` refuses a file URL with no drive on Windows, and such an entry is not a folder of that machine | fixtures are built per platform (`file_url`), plus one Windows install URL checked everywhere |
| `plugin_manager::claude::tests::truncate_chars_cuts_by_characters_not_bytes` | UNVERIFIABLE | `left: ""`: bash printed nothing; the test dropped stderr and the exit status. Likely the verbatim path (`\\?\C:\...`) `canonicalize` gives, which bash cannot source; not provable from the log | the test passes the plain form of the path and reports status and stderr on failure, so the next run decides |
| `view::tests::an_ssh_window_survives_its_own_block_start_with_a_polluted_echo` | TEST, #2121 | `pending ssh host: None`, the three window assertions before it held. The block has no session, so the family is the machine's default; for PowerShell `is_compatible_subshell_command` takes an ssh block (banner arm, by design) and no pending ssh is recorded | the test names a POSIX startup shell and asserts the family |
| 4 question-card view tests, `e_unsupported_is_stream_ordered_...` | TEST, already on integ/r15 | same panics as on Linux | none here (`fac90eeca`, `e938026ae`) |
| `warp_tui ... shell_completion_source_warmup_loads_path_executables` (flaky, passed on try 3) | TEST, #2121 | failed at 5.46 s and 5.25 s (its 5 s deadline, the first with a leaked child), passed in 1.4 s | the give-up net is 45 s; the wait still ends when the load does |

On Linux and macOS the fuzz tests failed on I6 `unsettled_calls` (worked on integ/r15,
`ef91ed81d`). On Windows every one of them stopped at the git probe first, so the round 14
Windows run says nothing about the heal; the next one will.

## 3. integration-linux (6 scenarios, each failed 3 of 3)

| scenario | verdict | evidence | change |
|---|---|---|---|
| `test_escape_sequences_sent_to_focused_terminal` | TEST, #2121 | vim shows `leftbar`: the key name was typed. `with_keystrokes` sends `chars = key name` for keys outside its table; `offer_pane_key` (`core/app.rs`, #1411) reads printable `chars` as `will_type`, and `terminal_key_down` leaves such a key to its typed event | the driver sends no text for a named key that types nothing (`chars_for_keystroke`, unit tested) |
| `test_pass_control_sequences_to_long_running_block` | TEST, #2121 | same: F2 with `chars: "f2"` | same |
| `test_ssh_into_sh` | APP, #2120 | the line typed at the remote `$ ` prompt is not in the block 10 s later. Managed verdict + `SSH` hook for a shell the remote command does not bootstrap: nothing releases the window on Linux | **none here**: the fix belongs to the ssh window (gate and phase machine) and needs its design review; proposal in #2120 |
| `test_ssh_with_shell_override` | UNVERIFIABLE, #2122 | remote `InitShell`, then no `Precmd` / `Bootstrapped`, give-up after 7 s. No pty writes in the trace | none |
| `test_command_xray_hover` | UNVERIFIABLE, #2122 | x-ray state not set 5 s after one `MouseMoved`; the hover timer leaves no line | none |
| `test_autosuggestions_are_hidden_when_opening_tab_completions` | UNVERIFIABLE, #2122 | menu not open 10 s after Tab; the working directory and the editor text are not in the trace | none |

The round 14-17 changes named in the task: step 8 of #1353 is not the cause of the two key
scenarios (the sink's `will_type` is #1411's); #2051 and the MotD play no part in the ssh
scenarios (the MotD is printed, the prompt is reached).

## What the next run should show

- `test-workspace-rest-windows` compiles `remote_server`; anything behind it is new information.
- `test-windows`: the 35 fuzz tests get past the git probe; aws, pwsh (both tests), codex,
  opencode and the ssh window test pass; `truncate_chars` passes or prints bash's stderr.
- `integration-linux`: the two key scenarios pass; `test_ssh_into_sh` keeps failing until #2120.
