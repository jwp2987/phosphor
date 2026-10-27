# Investigation findings for #979 and #980

Branch: `fix/ssh-remote-shell-fixes-975-984`

---

## #979 — Agent in an unmanaged shell: drops first character, phantom tool call hangs, auto-sends empty lines, later writes skip approval

**Status:** Investigated, not fixed. Four symptoms across four layers.

---

### Symptom 1: `run_shell_command` called when not in offered tool list

The offered tool list excluded `run_shell_command` but the model called it. Approving the
action did nothing and the agent sat at "waiting for instructions".

**Where it lives:** Agent provider / tool filtering in `app/src/ai/agent_providers/`. The tool
catalog is assembled from signatures and execution profile permissions. The symptom suggests the
model was offered a tool that the execution profile blocked or that the blocklist executor rejected
silently. The error result was not surfaced to the conversation, so the agent loop got no signal to
retry and hung.

**Not fixed:** No runtime debug logging was added to confirm which layer dropped the result.

---

### Symptom 2: Agent write drops first character (`cho` instead of `echo`)

**Full PTY write path traced — all layers correct:**

1. `ShellCommandExecutor::execute()` dispatches
   `AIAgentActionType::WriteToLongRunningShellCommand` (`shell_command.rs:524-586`)
2. Emits `ShellCommandExecutorEvent::WriteToPty { input, mode }`
3. `TerminalView::write_agent_bytes_to_pty()` (`view.rs:9626-9636`) emits
   `Event::WriteAgentInputToPty`
4. Resolves to `PtyIntent::WriteAgentInput` which calls `controller.write_agent_bytes()`
5. `send_write_to_event_loop(PtyWrite::AgentInput { bytes, mode })` (`pty_controller.rs:1408-1421`)
6. `mode.decorate_bytes()` (`crates/ai/src/agent/action/mod.rs:666-703`):
   - `Line` mode prepends SOH (0x01) for "beginning of line"
   - `Raw` mode passes bytes through untouched
   - `Block` mode wraps in bracketed paste sequences
   - All three are byte-exact; nothing is dropped
7. Bytes delivered via `Message::Input` to the local PTY event loop (`event_loop.rs:441-443`)

**Likely root cause — PTY event loop write queue:**

The loss almost certainly occurs in the PTY event loop's `write_list` queue during a block
lifecycle transition:

- `can_write_to_pty()` gates writes on `is_line_editor_active()`. During a long-running command,
  this returns false.
- Writes are queued as `QueuedWrite` entries in `write_list`.
- `split_kill_buffer_write()` (`pty_controller.rs:1541-1546`) splits PowerShell kill-buffer bytes
  into two separate `Message::Input` sends. If a block transition fires between those two messages,
  the second write's leading byte could be consumed by the new block's precmd handling.
- The `refresh_reply_gate()` call (`event_loop.rs:633`) switches between canonical/raw mode gating.
  A mode switch mid-queue-drain can cause the first byte of a write to be treated as a gated reply
  instead of user input.

**What's needed:** Byte-count logging at `queue_input()` (event_loop.rs:213) and at
`pty_write()` (the actual `libc::write` call) to confirm whether the byte is dropped before or
after the event loop.

---

### Symptom 3: After first approval, later writes run as AlwaysAllow without asking

**Root: `WriteToPtyPermission::AskOnFirstWrite` semantics (`shell_command.rs:296-308`)**

```rust
WriteToPtyPermission::AskOnFirstWrite => terminal_model
    .block_list()
    .active_block()
    .has_agent_written_to_block(),
```

`AskOnFirstWrite` checks `has_agent_written_to_block()`. Once an agent write to a block has been
approved, this flag stays true for the lifetime of that block. Subsequent writes to the same block
bypass approval entirely. This is intentional per-block semantics.

The additional symptom — "pressing Enter on the card also dismissed the write-to-PTY speedbump and
set `ShouldShowAgentModeWriteToPtySpeedbump=false`" — suggests a UI-side side effect. The card's
Enter key handler (in `settings_view/ai_page.rs` or the action card view) may promote the
permission to `AlwaysAllow` globally, not just clear the speedbump. That coupling needs tracing.

**Not fixed:** The per-block semantics are correct as written. The global promotion path was not
traced deep enough to pinpoint.

---

### Symptom 4: After stopping the agent, next typed `exit` loses first character (`xit`)

**Same mechanism as symptom 2.** After the agent is stopped via `suspend_agent_work_for_close()` or
`stop_local_agent_conversation_with_interrupt_scope()`, the next user-typed command (delivered
through the input editor → event loop → pty) loses its leading character.

This suggests the PTY write queue has residual state from the agent's previous writes that corrupts
the first byte of the next write batch. The queue does not reset on agent stop.

---

### What would fix it

| Symptom | Fix direction | Risk |
|---------|---------------|------|
| 1 | Surface executor errors to conversation so agent retries | Low, but needs provider-layer tracing |
| 2 | Byte-count logging + confirm event loop write boundary | Medium; needs runtime instrumentation |
| 3 | Separate UI speedbump dismissal from global permission change | Low; card handler refactor |
| 4 | Reset write queue state on agent stop | Medium; may affect queued agent commands |

---

## #980 — Closed ssh tabs keep their local shell and ssh ControlMaster alive until the app quits

**Status:** Investigated, not fixed. Root cause identified: `cleanup_closed_pane` does not call
`shutdown_pty()`.

---

### The close path for a SSH tab

1. User closes the tab/pane
2. Pane is hidden (undo-close grace period) → `pane.detach(DetachType::HiddenForClose)`
3. Grace period expires → `cleanup_closed_pane(pane_id)` (pane_group/mod.rs:5057-5081)
4. `pane.detach(DetachType::Closed)` (terminal_pane.rs:332-405):
   - Calls `suspend_agent_work_for_close()` which stops agent conversations
   - Clears conversations and deletes blocks
   - Calls `manager.on_view_detached(Closed)` — only cleans up shared-session sharers
   - Subscribes are removed
5. `forget_llm_overrides_for_pane()` clears LLM overrides and history bookkeeping
6. Pane removed from `pane_contents` HashMap

**Where the PTY stays alive:** At no point does this path call `shutdown_pty()`. The PTY event loop
keeps running, the shell process stays alive, and the `ssh` client process remains connected.

### Why `ModelEvent::ExitShell` doesn't help

The `ExitShell` event handler (`view.rs:13997-14014`) deregisters the remote server session and
triggers `stop_control_master()`. But `ExitShell` fires only when the **shell exits naturally**
(user types `exit`, Ctrl-D, etc.). When the pane is forcibly discarded, the shell is still alive,
so `ExitShell` never fires.

### The ControlMaster socket issue

`deregister_session()` (`crates/remote_server/src/manager.rs:1324-1395`) calls
`stop_control_master()` which runs `ssh -O exit` on the ControlPath socket. Since
`deregister_session` is only called on `ExitShell`, it never runs for forcibly discarded SSH
panes. The ControlPath socket becomes stale.

### What would fix it

`cleanup_closed_pane` should call `shutdown_pty()` on the terminal view before discarding the pane
from `pane_contents`. This would:

1. Send `Message::Shutdown` to the PTY event loop
2. Terminate the local shell process (which terminates the `ssh` child via the signal cascade
   already fixed in #975)
3. The `ssh` process exit should trigger `ExitShell` on the remote side, which would fire the
   `deregister_session` path and `stop_control_master`

The change site is `terminal_pane.rs::detach()` or `cleanup_closed_pane()` in
`pane_group/mod.rs:5057`. The call should be guarded to only apply on `DetachType::Closed`
(permanent discard), not on `HiddenForClose` (where the pane may be restored).

### Risk assessment

- **Low:** Adding `shutdown_pty()` at the permanent-discard point is a single call to an existing
  method. It affects the cleanup path only, never the happy path.
- **Medium:** For managed (remote-server) sessions, the PTY controller is different and the path
  needs a separate check.

---

*Date: 2026-10-25*
*Repository: jwp2987/phosphor*
*Branch: fix/ssh-remote-shell-fixes-975-984*
