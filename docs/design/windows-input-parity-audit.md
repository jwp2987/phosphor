<!-- Fable 5.2 read-only audit, 2026-11-09: Windows (ConPTY) vs Unix input path. Saved verbatim by the coordinator; one issue per finding. -->

Read-only audit: Windows (ConPTY) input path vs Unix, repo /home/winters/git/phosphor-merge @ merge/w0. No files touched.

## How Windows is wired (the root of every gap)

Unix installs five OS facts at spawn (`/home/winters/git/phosphor-merge/app/src/terminal/local_tty/terminal_manager.rs:1063-1127`): `ForegroundProbe{plain_program, prompt_fact, stream_drained}`, `set_interactive_probe` (tcgetpgrp/termios), `set_secret_read_probe` (canonical+echo-off), `set_tty_read_probe` (/proc, Linux only), `set_subshell_tty_probe`+flush, `set_foreground_sigint`. Windows (`terminal_manager.rs:1130-1213`) installs ONLY `ForegroundProbe::default().with_stream_drained_or_unknown(PeekNamedPipe)` plus the ConPTY prompt/enter hold flags. Everything else is `None`, and each consumer has its own default:

| missing fact | consumer default | fails |
|---|---|---|
| interactive probe | `InteractiveLatch::on_settled` → `NoProbeFallback` at 50 ms (`model/block/interactive_latch.rs:198-204`) | timer (owner-exempt) |
| secret read probe | `tty_reading_secret()==false` (`model/terminal_model.rs:2026-2028`); `secret_pass_through` always false | pass-through (plain terminal) |
| tty read probe | `gate.tty_read_detectable=false` → 15 s silence ceiling hands held keys back as editor text (`view.rs:997`, `view.rs:40472-40519`) | timer hand-back |
| subshell tty probe | `SubshellTtySample::unknown()` → leader `Unreadable` (`model/subshell_tty.rs:198-206`) → judge `Wait(Unknown)` (`view/subshell_ready.rs:164`) | CLOSED (see #5) |
| foreground sigint | ETX byte via write FIFO (`writeable_pty/pty_controller.rs:2623-2632`) | pass-through |
| PromptWatch / password poll / stalled prompt | every surface hook is `#[cfg(unix)]` (`writeable_pty/terminal_surface.rs:99-213`, `view.rs:37187-37398`) | CLOSED (see #1) |
| ConPTY peek unavailable | `stream_drained()==None` → plain-terminal Enter delivery, logged once (`terminal_manager.rs:1134-1160`, `typeahead_handoff.rs:3598-3603`) | pass-through (good) |
| ssh ownership (no tcgetpgrp) | `TTY_OWNED_AT_ARM = cfg!(all(windows, not(test)))` (`input_queue/ssh_phase.rs:52`): window born owned, nothing held before ownership | pass-through (good), but disables #1641 (see #3) |

Pane matrix: Git Bash (MSYS2 bash/zsh/fish) gets the ConPTY prompt hold + enter window (shell_signals, `terminal_manager.rs:1181-1197`); PowerShell gets `LineEditorReading` marker proof (`pty_controller.rs:1016-1021`), no hold; cmd/PS 5.1/WSL-unknown get nothing (pure pass-through). Table vs legacy: the only platform-aware switch is `conpty_enter_hold_from(env, table_mode)` (`typeahead_handoff.rs:380-392`); `TTY_OWNED_AT_ARM` is applied identically to the legacy `SshConnectGate::new` (`view.rs:40398`) and the edge machine (`ssh_phase.rs:188,196`), so no table/legacy divergence found. The ConPTY key coalescer (`local_tty/event_loop.rs:434-465`) only merges bytes ≥0x20 (`is_plain_key_text`, line 204), so it can never merge an Enter with the next line (the #1162/#1189 shape) — OK.

## Ranked findings

**1. HANG + KEY LOSS — agent-run interactive command never hands over on Windows.**
All take-over hooks are unix-only: `should_start_password_prompt_polling`, `pending_interactive_prompt`, `on_possible_password_prompt`, `on_stalled_interactive_prompt`, `on_command_may_be_waiting_for_input` (`view.rs:37187-37398`; trait defaults `terminal_surface.rs:99-213`). Nothing on Windows ever calls `switch_control_to_user(BlockedOnInput)` (`view.rs:37271, 37348`).
Scenario: PowerShell pane, agent runs `ssh host` / `gh auth login` / `Read-Host` / a `winget` Y/N prompt → the agent keeps the pty, user keystrokes are dropped, the tool call hangs until the 30-min `MAX_UNTIL_COMPLETION_DURATION`. Fails closed.
Minimal fix: (a) port the #1796 *notice* path to Windows unconditionally (`interactive_prompt_candidate` on the screen grid needs no termios); (b) event-driven hand-over for PowerShell: wrap `Read-Host`/`Get-Credential`/`$Host.UI.ReadLineAsSecureString` in the pwsh bootstrap to emit a `TtyRead`/`SecretRead` marker (same mechanism as `LineEditorReading`), and route that to `on_possible_password_prompt`. This marker is also the missing ConPTY "termios" for #2/#4/#10.
Port: `agent_interactive_password`, `agent_interactive_ssh`, `agent_lrc_no_false_takeover` (winmap NOT_WINDOWS "#1829/#1796 … ConPTY has no termios") with `Read-Host -AsSecureString` in place of `read -s`.

**2. SECRET — ConPTY prompt-hold give-up hands a secret back as visible editor text.**
Hold arms at every bash/zsh/fish precmd in a Git Bash pane and waits for the bracketed-paste signal; the 1500 ms cap hands keys back to the editor when `prompt_hold_can_hand_back` (`typeahead_handoff.rs:1576-1588`: at prompt, no user bytes in line) and `never_signalled || !segment_has_enter` (`view/paste_queue.rs:1066-1079`; expire log at `typeahead_handoff.rs:1519-1562`).
Scenario: Git Bash pane, `PROMPT_COMMAND` (or a precmd hook) runs `read -s -p "pin: "` / `gpg` pinentry-tty / a vault unlock. Precmd fires → hold arms → the shell is NOT at its editor, `read -s` is → user types the PIN (no echo) → no signal → 1.5 s → the PIN appears in the editor as text. On Unix the `shell_prompt_fact` sample sees canonical/echo-off (`NotAtPrompt`) and the hold does not exist. The `subshell_ready_secret_after_prompt` case covers exactly this on POSIX and is NOT_WINDOWS.
Minimal fix: make `can_hand_back` require `never_signalled` (a shell that has signalled before and is silent now means something else reads the tty): keep the keys via `keep_overdue_prompt_hold`; the `read -s` finishing redraws the prompt → bracketed-paste signal → release in typed order to the pty.
Port: `subshell_ready_secret_after_prompt` to the Git Bash pane as top-level bash with `PROMPT_COMMAND='read -s …'` (no `--rcfile` needed).

**3. MISROUTE — the #1641 managed-ssh wrapper hold never engages on Windows (keys typed into the remote bootstrap payload).**
`ssh_apply_wrapper_announcement` requires `(!gate.tty_owned || gate.tty_owned_by_event)` (`view.rs:11836-11843`); a window born owned (`TTY_OWNED_AT_ARM`) has `tty_owned=true, tty_owned_by_event=false` → announcement ignored → `ssh_on_wrapper_verdict` ignores `ManagedSSHSessionStarting` because `wrapper != AwaitingVerdict` (`view.rs:11887-11896`). `drops_keys` only starts holding at the remote `InitShell` (`pending`), so keys typed between the verdict and InitShell go to the remote shell while the bootstrap is being typed — the #1085/#1641 hazard. Also `tty_read_detectable=false` means the AwaitingVerdict hold, were it engaged, would hand keys back after 15 s.
Scenario: Git Bash pane with `enable_ssh_wrapper` on (`terminal_manager.rs:757-795`), `ssh host`, type during connect.
Minimal fix: the announcement and the verdict are EVENTS: in `ssh_apply_wrapper_announcement` treat `TTY_OWNED_AT_ARM && !tty_owned_by_event` as not-owned-by-event (take the `reclaimed` path, `TtyReclaimed` edge); accept a Managed verdict on such a window.
Port: `ssh_connect_typing`/`ssh_nested_typeahead` run on Windows but against Win32-OpenSSH without the wrapper; add a Git Bash pane variant with the wrapper on (the lab_* harness is already gitbash-only).

**4. MISROUTE (low) — no secret guard at ssh ownership/first-prompt release.**
`ssh_tty_reading_secret()` is always false (`view.rs:11800-11811`), `SshObservation::secret_pass_through` always false (`ssh_phase.rs:106-115, 305-313`). Mostly masked by the born-owned window, but the post-block / bootstrap hold still releases held keys at `FirstPrompt` or after `SSH_CONNECT_FIRST_PROMPT_GRACE` 2 s / `POST_BLOCK_GRACE` 5 s (`view.rs:40270, 40306`) regardless of whether the remote is reading a secret (remote `sudo` in a login script). Fails open = plain terminal; acceptable but weaker. Fix only via #1(b)-style markers from the remote bootstrap. Covered partly by `ssh_nested_typeahead` (already Windows).

**5. HANG (fail closed) — subshell auto-warpify never writes its setup in any Windows pane.**
`subshell_tty_sample()` → `unknown()` (leader `Unreadable`) → `subshell_ready` returns `Wait(Unknown)` on every look (`subshell_ready.rs:164`) until `PRE_READY_CEILING` 60 s (`subshell_ready.rs:23`) → `Cancel(NotAShell)` (`subshell_ready.rs:130-139`, leader not Shell/Relay) → misleading toast (`view.rs:23335`). Keys pass through meanwhile (not lost), but the subshell is never Phosphorized and the Written-state R9 flush/secret cancel (`view.rs:23200-23225, 23418`) are no-ops. `has_subshell_tty_probe()` (`terminal_model.rs:2062-2064`) has no non-test caller.
Scenario: PowerShell pane `bash`/`pwsh`/`wsl`; Git Bash pane `bash`. `gitbash_subshell` runs `--noprofile --norc -i`, so it never exercises warpify.
Minimal fix: a no-probe branch in `subshell_ready`: when `sample.tty.leader == Unreadable` (no probe) fall back to the screen facts (`Tail::ShellPrompt && prompt_moved_since_baseline && consumed_behind_prompt` → `Write(trigger)`), gated by `has_subshell_tty_probe()`.
Port: `subshell_ready_rc_questions`, `subshell_ready_rc_clear` to the Git Bash pane (bash `--rcfile` exists there).

**6. LATENCY (Ctrl-C ≤20 ms) — ETX can wait behind an in-flight ConPTY write; no console input flush.**
Windows writes ETX as a `UserKey` through the FIFO (`pty_controller.rs:2623-2632`, `3011-3017`); `DiscardQueuedInput` (`event_loop.rs:810-826`) drops queued discardables and `take()`s `self.writing`, but the overlapped write already handed to the pipe completes first (one write in flight, `local_tty/windows/mod.rs` `coalesces_key_writes`). Nothing flushes conhost's input buffer (no `FlushConsoleInputBuffer` anywhere; the #1777 comment at `typeahead_handoff.rs:1086-1092` documents lines surviving ^C under msys). 0x03 only becomes CTRL_C_EVENT for a child with ENABLE_PROCESSED_INPUT (#1570 fixed the inherited-ignore flag, `windows/ctrl_c.rs`).
Scenario: PowerShell or Git Bash pane, paste ≥32 KiB (`PASTE_WRITE_MIN_BYTES`, `pty_controller.rs:3265`) into `cat`/`more`, Ctrl-C.
Minimal fix: in `discard_queued_input` on ConPTY, `CancelIoEx` the in-flight discardable write before queueing ETX; measure.
Port: `ctrl_c_d` is on Windows but has no paste in flight; port `flood_typed_line` / the measurement half of `ctrl_c_flood_ab` (NOT_WINDOWS for the macOS gate only).

**7. KEY LOSS (specific chords) — no Ctrl-Break and no win32-input-mode.**
No `CTRL_BREAK`/Pause handling and no `\x1b[?9001h` / `\x1b[…_` encoding anywhere (grep of app/src and crates). PowerShell pane: a program with `TreatControlCAsInput` cannot be broken; PSReadLine chords that need win32-input-mode (Ctrl+Space, Shift+Enter, Ctrl+Enter, Ctrl+Alt+key beyond the `key_events.rs:116-141` fallback) degrade or vanish.
Fix: encode Ctrl+Break as a win32-input-mode key event (ConPTY accepts it once enabled), or `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT)` on the child's group. Port: new `ps_ctrl_break` / extend `keydown_count`.

**8. MISROUTE (accepted #933 cost, note the override) — young Enter on ConPTY.**
Barrier = `PeekNamedPipe` with documented blind spots (conhost-side and mio-wrapper buffers, `windows/mod.rs:426-437`): `Some(true)` false positive → `unsettled_evidence` None → Enter delivered while the Precmd is still coming. Separately the 50 ms `NoProbeFallback` → `note_command_interactive` → `end_young_hold_as_typeahead` (`typeahead_handoff.rs:1925-1933, 1958-1967`) writes a held young Enter + the keys behind it to the pty even when the barrier just said `PrecmdInFlight` — the timer overrides the one fact Windows has. Owner-exempt, but minimal tightening: in `note_command_interactive`, when `!samples_shell() && stream_drained()==Some(false)` leave the hold to `on_stream_applied`. Port: `ssh_enter_then_immediate_command`/`routing_table` are on Windows; add a fast-command young-Enter case for the PowerShell pane.

**9. LATENCY — PowershellSetup gate has no cap and `SignalThenKeepGate`.**
`gates.rs:181, 199-201`: Ctrl-C during a pwsh-in-pwsh warpify setup goes first but the typed keys stay held until `powershell_prompt_ready` or the 30 s `SUBSHELL_SETUP_QUIET_NET`. If the ^C aborted the setup so no prompt-ready event comes, keys return after 30 s.
Scenario: PowerShell pane, `pwsh`, type, Ctrl-C mid-setup. Fix: treat like `SubshellSetup` (`SignalThenRelease`) or release at the next Precmd. Port: `ctrl_c_d` pwsh7 variant with a `pwsh` subshell.

**10. SECRET (residual, documented) — PowerShell marker proof can re-baseline during `Read-Host -AsSecureString`.**
`powershell_marker_proves_reading` (`typeahead_handoff.rs:3896-3903`): a `LineEditorReading` marker for the previous prompt parsed after the Enter makes `counts_provably_stale` true → the hand-off believes the prompt is back while `Read-Host` reads → later keys go to the editor, visible. Fix: stamp the marker with the prompt sequence number; ignore one older than the last counted Enter. Port: new `ps_read_host_secure` (type `Read-Host -AsSecureString` + password fast; assert editor empty, secure-string length right).

**11. COSMETIC/VERIFY — IME on Windows.** `is_composing: false` is hard-coded on the Windows key conversion path (`crates/warpui/src/windowing/winit/event_loop/key_events.rs:175, 207`); all `intl_ime_*` cases are macOS-only. If winit Ime events are not routed, a composed CJK string may arrive as per-key chars mid-composition. Needs a Windows IME case before ranking higher.

## Fail-safe vs fail-closed summary
Fail safe (pass-through like a plain terminal): ConPTY peek unavailable; no secret probe; ssh ownership (born owned); Ctrl-C (ETX). Fail closed: agent hand-over (#1), subshell readiness (#5), PowershellSetup after ^C (#9). Timer hand-backs to the editor where Unix has an event: prompt-hold cap (#2, secret risk), ssh 15 s ceiling (#3), HandoffReport 3 s (same as Unix).

### Critical Files for Implementation
- /home/winters/git/phosphor-merge/app/src/terminal/local_tty/terminal_manager.rs (Windows probe wiring, 1130-1213; Unix reference 1063-1127)
- /home/winters/git/phosphor-merge/app/src/terminal/model/typeahead_handoff.rs (ForegroundProbe 153-230; prompt-hold cap/hand-back 1465-1600; note_command_interactive 1925; marker proof 3896)
- /home/winters/git/phosphor-merge/app/src/terminal/view.rs (ssh wrapper hold 11780-11990; CommandBecameInteractive 19503; subshell judge 22935-23010; unix-only agent hooks 37187-37398; SshConnectGate 40350-40530)
- /home/winters/git/phosphor-merge/app/src/terminal/view/subshell_ready.rs (Unknown → Wait, 130-164)
- /home/winters/git/phosphor-merge/app/src/terminal/local_tty/event_loop.rs (discard/abort 810-880; coalescer 434-465) and /home/winters/git/phosphor-merge/app/src/terminal/local_tty/windows/mod.rs (PeekNamedPipe barrier 426-449)
- /home/winters/git/phosphor-merge/script/realapp/appci/winmap.py (NOT_WINDOWS 72-132: the cases to port)
