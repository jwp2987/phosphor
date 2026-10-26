# Issues filed during testing — 2026-10-24

Testing of Phosphor **v0.1.14** on a MacBook Pro (M1 Pro, macOS 26.4.1), over ssh from the Mac to the build box, and from a Linux client on a headless display.

## Fixes applied (branch `fix/ssh-remote-shell-fixes-975-984`)

| Issue | Status | Commit |
|-------|--------|--------|
| #975 | ✅ Fixed — process group signaling | `d6580411f` |
| #976 | ✅ Fixed — Enter routing during SSH warpifying | `a23991898` |
| #977 | ✅ Fixed — relaxed completion history gate | `2725e5187` |
| #978 | ✅ Fixed — no-op executor for remote fish | `2b1ac06be` |
| #979 | 🔍 Investigated, not fixed — multi-layer, needs runtime debug | — |
| #980 | 🔍 Investigated, not fixed — PTY not shutdown on pane discard | — |
| #981 | ⏺ Not investigated — need to find remote temp dir creation site | — |
| #982 | ⏺ Not investigated — user context not updated after sudo exit | — |
| #983 | ⏺ Not investigated — typeahead during SSH connect window | — |
| #984 | ⏺ Not investigated — requires NuShell ShellType variant | — |

## macOS local testing (latency and torture)

| # | Title |
|---|---|
| [#961](https://github.com/jwp2987/phosphor/issues/961) | macOS: typing lag is noticeable — keystroke-to-paint ~38 ms median vs ~29 ms in Terminal.app |
| [#962](https://github.com/jwp2987/phosphor/issues/962) | macOS: Phosphorizing a zsh subshell from a bash parent echoes a truncated bootstrap and leaves zsh at a continuation prompt |
| [#963](https://github.com/jwp2987/phosphor/issues/963) | Agent-started pager (less) can't be operated from the keyboard; `q` goes to the hidden agent input |
| [#964](https://github.com/jwp2987/phosphor/issues/964) | Routing: real commands whose arguments are English words go to the agent; bare `fish` goes to the agent even when installed |
| [#965](https://github.com/jwp2987/phosphor/issues/965) | macOS: prose starting with a macOS command word runs in the shell (`say hello` speaks aloud) |
| [#966](https://github.com/jwp2987/phosphor/issues/966) | fish: first command after Phosphorizing runs but its block never appears |
| [#967](https://github.com/jwp2987/phosphor/issues/967) | Subshell bootstrap commands are written to the user's persistent shell history (bash, fish, pwsh) |
| [#968](https://github.com/jwp2987/phosphor/issues/968) | Large paste (1 MB) makes the input editor slow, keeps memory after clearing, and autodetects as an agent prompt |
| [#969](https://github.com/jwp2987/phosphor/issues/969) | Conversation sidebar shows every conversation's directory as the pane's current cwd |
| [#970](https://github.com/jwp2987/phosphor/issues/970) | macOS: UriService IPC server fails to start on every launch (relative socket path, cwd `/`) |
| [#971](https://github.com/jwp2987/phosphor/issues/971) | Locked database at runtime/startup is only logged; saves fail silently |
| [#972](https://github.com/jwp2987/phosphor/issues/972) | Generated conversation titles reverted to the raw prompt after a SIGKILL restart |
| [#973](https://github.com/jwp2987/phosphor/issues/973) | Unicode rendering gaps: ZWJ emoji, flags, bidi, combining marks, a box-drawing glyph |
| [#974](https://github.com/jwp2987/phosphor/issues/974) | zsh: PRINT_EXIT_VALUE prints `zsh: exit 1` from Phosphor's bootstrap; user preexec stderr drawn inside the command header |

## Remote, unmanaged and root shells over ssh (Mac → build box)

| # | Title | Status |
|---|---|---|
| [#975](https://github.com/jwp2987/phosphor/issues/975) | Remote shells (bash --rcfile + sudo chains) outlive the ssh connection and Phosphor quitting | **Fixed** — terminal server `Children::drop` and `KillChildRequest` now signal the **process group** (`-PID`) instead of individual PID; `DirectPtyHandle::kill()` likewise. Since PTY spawner calls `setsid()`, PGID == PID, so all descendants (ssh clients, sudo chains, subshells) receive SIGHUP/SIGKILL. Commit `d6580411f`. |
| [#976](https://github.com/jwp2987/phosphor/issues/976) | Cmd-Enter is sent to the shell as Enter while the "Phosphorize subshell" banner is shown; following prose runs as a command | **Fixed** — in `keydown_on_terminal()`, while the SSH Warpifying banner is active, Enter keystrokes route to `TerminalAction::SetInputModeAgent` instead of being written to the PTY. Commit `a23991898`. |
| [#977](https://github.com/jwp2987/phosphor/issues/977) | No Tab completion or folder suggestions in Phosphorized remote subshells | **Fixed** — replaced `can_query_history()` gate (requires histfile read complete) with lighter `session_bootstrapped_for_completions()` (bootstrapped + registered with Sessions model). Completions now work immediately even when history initialization lags behind bootstrap. Commit `2725e5187`. |
| [#978](https://github.com/jwp2987/phosphor/issues/978) | Remote fish subshell: completion generator command leaks into the input and fish history | **Fixed** — remote WarpifiedRemote fish sessions now use `NoOpCommandExecutor` instead of `InBandCommandExecutor`. In-band generators interfere with fish's bracketed paste handling on remote PTYs, leaking generator command text into the terminal. Commit `2b1ac06be`. |
| [#979](https://github.com/jwp2987/phosphor/issues/979) | Agent in an unmanaged shell: drops first character, phantom tool call hangs, auto-sends empty lines, later writes skip approval | **Investigated, not fixed.** 4 symptoms across 4 layers: (1) `run_shell_command` called when not in offered tool list — tool filtering in agent provider; (2) agent write drops first char (`cho` instead of `echo`) — traced agent PTY path through `write_agent_bytes` → `PtyWrite::AgentInput` → `decorate_bytes` → `send_write_to_event_loop`. The `AIAgentPtyWriteMode::Line` decoration prepends SOH (0x01) for "beginning of line", which is correct; the dropped byte likely occurs in the local PTY event loop when the shell is in a raw/long-running-command state and the input queue races with block transitions; (3) later writes become `AlwaysAllow` after first approval — `WriteToPtyPermission::AskOnFirstWrite` checks `block.has_agent_written_to_block()` at line 302-306 of `shell_command.rs` — once set, subsequent writes to the same block skip approval; (4) user typing after Stop drops first char — same mechanism as symptom 2, likely the PTY write queue interacting with line editor active/inactive transitions. **Requires runtime debugging with logging to pinpoint the dropped byte.** |
| [#980](https://github.com/jwp2987/phosphor/issues/980) | Closed ssh tabs keep their local shell and ssh ControlMaster alive until the app quits; stale ControlPath sockets | **Investigated, not fixed.** Traced the full close path: `cleanup_closed_pane` → `pane.detach(DetachType::Closed)` → `manager.on_view_detached(Closed)` → `forget_llm_overrides_for_pane`. The `terminal_pane::detach` handler calls `suspend_agent_work_for_close` and clears conversations, but does **not** call `shutdown_pty()` on `DetachType::Closed`. The `cleanup_closed_pane` flow discards the pane from `pane_contents`, which drops the view entity. The PTY event loop survives until the app quits because nothing explicitly terminates it on pane discard. The `ModelEvent::ExitShell` handler deregisters the remote server session and triggers `stop_control_master`, but `ExitShell` fires only when the shell exits — not when the pane is forcibly discarded. **Gap: `cleanup_closed_pane` should emit `Event::ShutdownPty` (or call `shutdown_pty`) for SSH sessions before discarding the pane.** |
| [#981](https://github.com/jwp2987/phosphor/issues/981) | Remote server leaves an empty /tmp/.tmpXXXXXX directory per ssh session | **Not investigated.** Need to find where the remote-side temp directory is created. Likely in bootstrap/shell-integration scripts on the remote host. |
| [#982](https://github.com/jwp2987/phosphor/issues/982) | Tab title stays root@host after leaving sudo -i in an unmanaged shell | **Not investigated.** The session user/hostname is determined at InitShell time; no code path updates it when `Preexec` or `Precmd` hooks report a different user after `sudo -i` exits. |
| [#983](https://github.com/jwp2987/phosphor/issues/983) | Typing while ssh connects splits keystrokes between the local and remote shell | **Not investigated.** The SSH connect window — between ssh start and remote InitShell — is a period where `can_write_to_pty()` returns true (local shell line editor active) and keystrokes go to the local PTY. Once the remote subshell bootstraps, the active session changes, but queued keystrokes may have already been delivered to the local shell. |
| [#984](https://github.com/jwp2987/phosphor/issues/984) | nushell: Tab completion escapes spaces bash-style (`dir\ with\ spaces/`) | **Not investigated.** `ShellType::from_name()` returns `None` for nushell (only Zsh, Bash, Fish, PowerShell are recognized). Unrecognized shells default to `Null` shell type which maps to `ShellFamily::Posix` / `EscapeChar::Backslash`. Nushell uses quotes, not backslash escaping. **Requires adding a NuShell variant to `ShellType` or configuring nushell-appropriate escape behavior.** |

## ksu typing lag and copy/paste (Linux client → build box)

| # | Title |
|---|---|
| [#985](https://github.com/jwp2987/phosphor/issues/985) | ssh: keystrokes to raw-input programs arrive 40–100 ms late because the remote-server proxy shares the interactive ssh connection |
| [#986](https://github.com/jwp2987/phosphor/issues/986) | ssh: `clear; ./ksu` (or `clear; cat`) leaves the running block completely blank; input still reaches the shell blind |
| [#987](https://github.com/jwp2987/phosphor/issues/987) | Can't select or copy text in a running block when the program has mouse reporting on: Shift-drag selects blocks instead of text |
| [#988](https://github.com/jwp2987/phosphor/issues/988) | Can't drag-select text in a conversation opened from the sidebar; a running command blocks reaching the agent response in the pane |

## Comments added to existing issues

| # | Comment |
|---|---|
| [#955](https://github.com/jwp2987/phosphor/issues/955#issuecomment-6385185707) | macOS: a 3 MB single-word paste freezes the app for 5+ minutes (CoreText line breaking); timings by length |
| [#960](https://github.com/jwp2987/phosphor/issues/960#issuecomment-6386452969) | The root-subshell hang also happens over ssh; stuck root shells survive closing the tab |
| [#964](https://github.com/jwp2987/phosphor/issues/964#issuecomment-6386453493) | Routing differs between macOS and the Linux remote (`cat this that`, bare `fish`) |
| [#966](https://github.com/jwp2987/phosphor/issues/966#issuecomment-6386453215) | The missing first fish block also happens on a remote host |
| [#961](https://github.com/jwp2987/phosphor/issues/961#issuecomment-6391242347) | The lag felt over ssh is mostly key delivery — tracked in #985 |

## Reports

- macOS and remote-shell report (Parts 1–4): `~/phosphor-profile/torture/REPORT.md` on the Mac
- ksu latency and copy/paste report (Parts 5–7): `REPORT-linux-ksu.md` in the session scratchpad (`ksu-linux/`)
