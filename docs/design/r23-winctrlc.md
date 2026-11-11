# r23: Windows Ctrl-C paths (#1887, #1890, #2018): design note, no code

Status: nothing patched. Each item is either a second patch on the same path or needs a Windows run / an owner decision first.

## #1887 CancelIoEx the in-flight discardable write

- Path: `discard_queued_input` (`app/src/terminal/local_tty/event_loop.rs:828`) drops queued discardable entries and `take()`s `self.writing`, but on ConPTY the write is already handed to the pty's `mio::windows::NamedPipe` (`local_tty/windows/mod.rs:463-505`, writer = the pipe; one overlapped write in flight, `coalesces_key_writes` at :520).
- Why no code: the overlapped operation is owned inside mio. mio exposes no per-write cancel. `CancelIoEx(handle, NULL)` would cancel the pending READ too, and an aborted write completes with an error that mio stores and returns from the next write/read, which the event loop treats as a broken pty. Whether that is survivable (or loses/duplicates the cut chunk) can only be learned on Windows. The only existing CancelIoEx is on a private pipe we own (`writeable_pty/command_pipe.rs:362`), not this one.
- Also unmeasured: how long a ConPTY write actually blocks. The audit's scenario (paste >= 32 KiB into `cat`, ^C) has no number yet.
- Next step: measure first (log the write's start/completion time next to the ^C enqueue on the Windows VM; `ctrl_c_flood` now runs in the Git Bash pane). If it exceeds 20 ms, options are (a) bound the chunk handed to the pipe on ConPTY so an in-flight write is short (no cancel needed, a pure event-loop change testable on Linux), (b) a write wrapper we own instead of mio's. (a) is preferred.

## #1890 PowerShell setup gate after a ^C

- This is the second patch on the same path: `b5b1f8533` (one progress-based net) and `c9a6dd27a` (B-1) deliberately made `PowershellSetup` `SignalThenKeepGate` (`input_queue/gates.rs:181`, plan table `table_tests.rs:1283`), because releasing right after the ^C sends the held keys raw behind a ^C that the Windows console flushes (#1372/#1438). Switching to `SignalThenRelease` is exactly the bug B-1 fixed.
- What is actually missing: an event that ends the hold when the ^C aborts the setup and no `powershell_prompt_ready` follows (today only the 30 s quiet net / read marker). Candidate: end the hold on the next Precmd or `LineEditorReading` marker after the abort (the hold is already `AfterAbort`, `abort_in_flight_subshell_setup`), and keep the net only as the give-up. Needs a design pass on the `AfterAbort` hold's existing release conditions and a pwsh-in-pwsh Windows run (`ctrl_c_d` pwsh7 variant) before code.

## #2018 marker hands the editor input while a typed-ahead Enter is unread

- The issue says "needs a decision on the approach before any code". Option 1 (wrapper reports `[Console]::KeyAvailable` with the marker) changes the bootstrap protocol for pwsh 7 and Windows PowerShell 5.1 and needs a Windows run; option 2 is not recommended. Owner decision outstanding; no code.
