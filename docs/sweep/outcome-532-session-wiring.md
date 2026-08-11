# Outcome: #532 — PTY-spawn session-ID registration

Task package: port the pin's session-ID registration wiring at the four
production `register_session_id` call sites, then flip
`TerminalModel::should_validate_dcs_hook_session_id` to the pin's
`!self.shared_session_status().is_viewer()`, plus the two pin tests
(`sharer_rejects_dcs_hook_with_unregistered_session_id`,
`viewer_processes_dcs_hook_with_unregistered_session_id`).

Base branch: `working`. Build freeze in effect (no `cargo`/`rustc` in any
form) — every claim below is source-comparison against the pin
(`02b53fcd8`), not compilation or test execution.

## Summary of the decision

**Registration: wired for 3 of the 4 named sites. Gate: left at `false`, on
purpose, with new evidence for why.** The task's own fallback instruction
("if you cannot make registration work, leave the gate at false ... a wrong
flip is far worse than no change") is exercised here — not because
registration doesn't work, but because I measured a **second, larger
prerequisite gap** that the original issue text didn't capture: most DCS hook
types don't carry a `session_id` at all in this fork yet, so flipping the
gate would break ordinary command execution, not just shared-session
handling. See "Why the gate stays off" below for the evidence.

## What was wired (real, working session-ID registration)

The premise "both host files already exist, this is wiring not new
construction" undersold the actual dependency chain. `register_session_id`
had 0 production call sites because **nothing in this fork ever baked a
generated session ID into the shell launch** — the shell init scripts
self-generate their own `WARP_SESSION_ID` at runtime
(`WARP_SESSION_ID="$(command -p date +%s)$RANDOM"` in bash/zsh,
`(random)` in fish, `[int64]"$epoch$random"` in pwsh). Registering an
app-generated ID against a shell that will report back a *different,
self-generated* ID would have been inert at best and misleading at worst.
Making registration real required porting the pin's session-ID-baking
mechanism, not just adding the four `register_session_id()` calls:

- **`app/src/terminal/bootstrap.rs`**: added `generate_session_id()`
  (`rand::thread_rng()`, non-zero `u64`, matching the pin) and
  `SESSION_ID_PLACEHOLDER = "@@WARP_SESSION_ID@@"`. Threaded a `session_id:
  SessionId` parameter through `init_shell_script_for_shell`,
  `init_subshell_command`, `init_subshell_script_for_shell`, and
  `raw_init_shell_script_for_shell` (the Docker-sandbox path), each now
  substituting the placeholder for the real generated ID.
- **`app/assets/bundled/bootstrap/{bash,zsh,fish}_init_shell.sh`,
  `pwsh_init_shell.ps1`, `{bash,zsh,fish}_init_subshell.sh`**: swapped the
  shell's self-generated `WARP_SESSION_ID` for the `@@WARP_SESSION_ID@@`
  placeholder — a single-line change per file, mirroring the pin exactly.
  Nothing else in these files was touched (in particular, the pin's
  unrelated DCS-terminator encoding change, `\x9c` → `\x1b\x5c`, and the
  pwsh `-ErrorAction Ignore` removal, were both left alone as out of scope
  for #532).
  **`unknown_init_subshell.sh` was deliberately NOT touched** — it emits the
  `InitSubshell` hook, which doesn't carry a `session_id` field in this fork
  (or a consumed one in the pin — the pin's addition there is inert since
  `InitSubshellValue` has no `session_id` field), so there is nothing for a
  baked-in ID to be checked against.
- **`app/src/terminal/local_tty/shell.rs`**: added `session_id: SessionId`
  to `DirectShellStarter` and `WslShellStarter`, generated at every
  `ShellStarter` construction site (Direct, MSYS2, Environment, UserDefault,
  Fallback, WSL), with `session_id()` accessors. Threaded `session_id`
  through `arguments_for_session_spawning_command` and
  `wsl_arguments_for_session_spawning_command`.
- **`app/src/terminal/local_tty/docker_sandbox.rs`**: added `session_id:
  SessionId` to `DockerSandboxShellStarter`, derived from its wrapped
  `DirectShellStarter`, with a `session_id()` accessor.
- **`app/src/terminal/local_tty/unix.rs`**: `prepare_docker_sandbox` now
  passes `starter.session_id()` into `raw_init_shell_script_for_shell` —
  required once the shared `bash_init_shell.sh` asset stopped
  self-generating its ID, or Docker-sandbox sessions would have gotten a
  literal unexpanded `@@WARP_SESSION_ID@@` in their init script (a
  regression this change would otherwise have introduced).

**Production `register_session_id` call sites, before → after this
package:**

| site | pin | before | after |
|---|---|---|---|
| `local_tty/terminal_manager.rs` (`on_shell_determined`) — PTY spawn | yes | no | **yes** |
| `remote_tty/event_loop.rs` (`write_zsh_init_shell_script`) — remote TTY zsh init | yes | no | **yes** |
| `terminal/view.rs` (`write_init_subshell_bytes_to_pty`) — subshell spawn | yes | no | **yes** |
| `terminal_model.rs` (`fn ssh`) — SSH remote_session_id | yes | no | **no — see below** |

### `terminal_manager.rs` (PTY spawn — "the main one")

`on_shell_determined` now derives `generated_session_id` from the
`ShellStarter` (matching on all four variants) and calls
`manager.model().lock().register_session_id(generated_session_id)`
**before** `enqueue_init_script`/`create_pty` are called — i.e. before the
shell can write anything back, matching the pin's ordering and the task's
explicit safety requirement. `enqueue_init_script` now threads the same ID
through to `init_shell_script_for_shell` for the zsh/MSYS2 path.

### `remote_tty/event_loop.rs` (remote TTY)

`write_zsh_init_shell_script` now takes the `terminal_model` handle,
generates a session ID, registers it, then bakes it into the zsh init
script — same ordering guarantee.

### `terminal/view.rs` (subshell spawn)

`write_init_subshell_bytes_to_pty` generates and registers a session ID
before calling `init_subshell_command`, which now bakes it into the
subshell's init script. This hook is `InitShell` (with `is_subshell: true`),
not the separate `InitSubshell` hook — the shell scripts already emit it
that way — so it's covered by the same session-id-carrying path.

### `terminal_model.rs` `fn ssh` (SSH remote_session_id) — NOT wired

Left alone. This is the pin's `SSHValue.remote_session_id` registration for
a remote host's own bootstrap over an SSH ControlMaster wrapper. Wiring it
for real would require:
1. adding a `remote_session_id: HookSessionId` field to this fork's
   `SSHValue` (absent — confirmed by reading `dcs_hooks.rs`) plus a
   `populate_field` parse arm for it, and
2. emitting `remote_session_id` from the SSH-wrapper shell code in
   `bash_body.sh`/`zsh_body.sh` (large, delicate, already-existing
   production scripts I did not want to touch blind under the build
   freeze).

This is additional scope beyond what the issue described, touches the
riskiest shell-script surface in the codebase, and — per the finding below —
wiring it would not have unblocked the gate flip anyway. Left as explicit
unfinished work; see "Unfinished" below.

## Why the gate stays off (measured, not assumed)

Read `Performer::validate_hook_session_id` in
`app/src/terminal/model/ansi/mod.rs`:

```rust
fn validate_hook_session_id(&mut self, hook: &DProtoHook) -> bool {
    if !hook.requires_registered_session()
        || !self.handler.should_validate_dcs_hook_session_id()
    {
        return true;
    }
    let Some(session_id) = hook.session_id() else {
        // rejected: "missing session_id"
        return false;
    };
    ...
}
```

and `DProtoHook::session_id()` in `ansi/dcs_hooks.rs`, which returns `None`
**unconditionally** (not "unregistered", structurally absent) for
`CommandFinished`, `Preexec`, `Bootstrapped`, `PreInteractiveSSHSession`,
`SSH`, `InputBuffer`, `Clear`, `InitSubshell`, `FinishUpdate`, and this
fork's own SSH-bootstrap hooks. Only `InitShell`, `Precmd`, and `ExitShell`
carry a real `session_id` today. `ansi/mod_test.rs`'s own `MockHandler`
default already documents this exact gap (comment above
`should_validate_dcs_hook_session_id: false` in that file, predating this
package).

Separately: `SharedSessionStatus::is_viewer()` is `false` for
`NotShared` — an ordinary, non-shared session — not just for sharers. So
`!is_viewer()` (the pin's implementation) evaluates to `true` for every
normal session, not only screen-share sharers.

Combined: flipping the gate today would make `validate_hook_session_id`
reject **`CommandFinished` and `Preexec` — the exact two hooks the
pre-existing doc comment already named — for every ordinary,
non-shared session**, not just shared ones. This is a strictly larger and
more severe break than "registered_session_ids would be empty" (the
originally-documented reason); it would happen even with registration fully
and correctly wired, because the rejection happens on `hook.session_id() ==
None`, before registration is ever consulted. Command-lifecycle tracking
(block completion, prompt advancement) would break for every real terminal
session. This is unambiguously worse than the current inert `false`, so the
gate was **not** flipped. The doc comment on
`should_validate_dcs_hook_session_id` in `terminal_model.rs` was rewritten
to record this with the specific evidence (function names, file locations)
so a future agent doesn't have to re-derive it.

## Per-test outcome

Both tests were **written, unverified** (cannot compile or run under the
build freeze) in `app/src/terminal/model/terminal_model_test.rs` (the fork's
actual file name — the task text's `terminal_model_tests.rs`, plural, is the
pin's filename; this fork's is singular, `terminal_model_test.rs`, per its
existing `#[path = "terminal_model_test.rs"]` declaration).

- **`viewer_processes_dcs_hook_with_unregistered_session_id`** — ported
  unchanged (adjusted only for this fork's `CommandFinishedValue` having no
  `session_id` field to omit — it never had one, unlike the pin). This
  exercises the viewer branch of `should_validate_dcs_hook_session_id`,
  which evaluates to `false` under both the pin's real implementation and
  this fork's hardcoded `false` — **expected to pass** regardless of the
  gate decision above, since viewer behavior is unaffected by leaving the
  gate off.

- **`sharer_rejects_dcs_hook_with_unregistered_session_id`** — ported
  unchanged (same `CommandFinishedValue` adjustment). This asserts that a
  sharer session rejects an unregistered-session_id `Precmd` hook — which
  only happens under the pin's real gate logic. Since the gate was
  deliberately left at hardcoded `false` (see above), this hook is currently
  **accepted**, not rejected, so this test is **expected to fail** against
  the code as committed here. This is a known, intentional, documented
  consequence of the safety decision above, not an oversight — flipping the
  gate to make it pass was evaluated and rejected as unsafe. Per AGENTS.md
  §5.6/§5.10, the test was ported faithfully rather than weakened to match
  current behavior; the gap between pin behavior and fork behavior is real
  project debt, tracked here and in the rewritten doc comment, not hidden.

A helper, `hex_encoded_json_dcs`, was added to `terminal_model_test.rs`
(hex-encodes a JSON DCS payload the way the shell bootstrap scripts do). It
duplicates `ansi/mod_test.rs`'s private `hex_encoded_dcs_string` rather than
sharing it, since that helper isn't exported and this is the smaller/safer
change under the build freeze.

## Verification method

Source comparison against the pin (`git show 02b53fcd8:<path>`) for every
touched function's signature and body; `rustfmt --check --edition 2024` on
every touched file (pre-existing formatting drift unrelated to these edits —
confirmed line-by-line against `git diff` — was left alone per the
instruction not to reformat untouched code; `view.rs`'s check pulls in
`mod agent_view;` and other submodules via rustfmt's mod-tree resolution, so
diffs were filtered to `terminal/view.rs:` specifically); `script/check_cloud_boundary`
and `script/check_stub_coverage` (both pass, pure shell, permitted under the
build freeze).

## Unfinished (explicit)

- **SSH `remote_session_id` registration** (`terminal_model.rs` `fn ssh`,
  the pin's 4th production site) — not wired. Needs a new `SSHValue` field,
  a `populate_field` parse arm, and shell-script changes to
  `bash_body.sh`/`zsh_body.sh`'s SSH ControlMaster wrapper. Not attempted
  under the build freeze given the risk of blind edits to those large,
  delicate, already-production scripts, and because it would not by itself
  unblock the gate flip.
- **`should_validate_dcs_hook_session_id` stays `false`.** Safely flipping
  it needs `session_id` threaded through `CommandFinished`, `Preexec`,
  `Bootstrapped`, `PreInteractiveSSHSession`, `SSH`, `InputBuffer`, `Clear`,
  `InitSubshell`, and `FinishUpdate` (`DProtoHook` variants, their
  `populate_field` parsing, and the shell-script code in
  `bash_body.sh`/`zsh_body.sh`/`fish.sh`/`pwsh.ps1` that emits them) — a
  substantially larger port than this package's scope. `#532` should stay
  open for that follow-up; this package narrows it from "0 production
  registration sites, symbol present but fully inert" to "registration real
  and correct for the 3 hook-types that support it, gate correctly left off
  pending the rest."
- **`sharer_rejects_dcs_hook_with_unregistered_session_id`** is expected to
  fail as committed (see above) — this is the direct, intended consequence
  of not flipping the gate, not a separate defect.
- Nothing in this package was compiled or run. All claims are from reading
  the pin and the fork side by side; `rustfmt --check` and the two guard
  scripts are the only things actually executed.
