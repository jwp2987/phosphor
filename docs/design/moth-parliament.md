# moth-parliament — conversations that are not terminals

**Branch:** `moth-parliament`. **Started:** 2026-09-05.
**Status:** design agreed, implementation not started.

A working name deliberately chosen to say nothing about the contents, because the
scope of this branch is expected to move and a descriptive name would date badly.

---

## 1. What this branch is for

Today a conversation in Phosphor is a *view onto a terminal's block list*. It cannot
exist without a pty behind it. This branch makes a conversation a thing in its own
right, openable as its own pane, spawning a terminal **on demand** the first time it
actually needs a shell.

The full design rationale is `docs/DESIGN-PHOSPHOR-FORK.md` §9. This file is the
delivery plan: what gets built, in what order, and what "done" means for each piece.

### Why it is its own branch

This is a new *kind* of work for this fork. Everything to date has been parity
porting, cloud removal, or bug fixing against a pinned oracle. This adds a pane type
that upstream does not have, a terminal view that can exist without a process, and
possibly a surface abstraction on conversations. It should not share a branch with
parity work, and it should not be reviewed as if it were parity work.

### Why it is possible now and was not before

`ORACLE.md` was revised on 2026-09-05: the pin is a source of **suggestions and
evidence, not a specification**. Under the old reading, "Warp has no standalone
conversation pane" was an argument against building one. It is not any more. See
`AGENTS.md` §5.10 — intentional divergence needs a *record*, not a justification
against a deficit.

---

## 2. The one architectural decision everything else follows from

**Create the `TerminalView` eagerly. Defer only the pty spawn.**

The obvious design is to make `AgentViewController`'s `terminal_view_id` optional and
teach every consumer to cope. That is the expensive path and it drags a large amount
of code into knowing about this feature.

Instead, a conversation pane owns a `TerminalView` from the moment it opens — there
just is no process behind it yet. Every consumer that needs a `terminal_view_id`
keeps working unchanged. "Spawn on demand" means spawning the **process**, not the
view.

`TerminalModel::pending_session_id()` already returns `Option<SessionId>`, so a view
whose session has not started is a state the code contemplates. What is new is that
the state persists indefinitely rather than briefly during bootstrap — and that is
the single riskiest assumption on this branch. See §5.

---

## 3. Delivery order

Each step is independently shippable and independently revertible. Do not start a
step before its predecessor has been built AND verified on the build box — the
`top`-clipping saga on `main` produced seven reverted fixes largely because changes
were stacked faster than they were verified.

### Step 0 — the footer bar (prerequisite, lives on `main`)

`docs/DESIGN-PHOSPHOR-FORK.md` §8. A permanent fixed-height bar on every window, so
chrome that asks the user something stops taking rows from running programs. A
conversation pane wants the same bar and gets it free if this lands first.

**Not part of this branch.** Rebase onto it once it is on `main`.

### Step 1 — `TypedPane::Conversation`

A new pane variant and its pane implementation, rendering the existing agent view
against a `TerminalView` with no process. No spawning yet; tool calls fail loudly
rather than silently.

**Done when:** a conversation pane opens, holds a conversation, persists and restores,
and the compiler has been made to account for the new variant everywhere it matches.
The vertical-tab sectioning work classifies it as `Agent` with no change.

### Step 2 — spawn on demand

The first tool call needing a shell (`run_shell_command`, or a file tool with no
remote-server extension) spawns the process into a split below the conversation.

**Done when:** a text-only conversation never spawns anything; a conversation that
runs a command gets a real block list with real output; killing the shell leaves the
conversation alive and the next command spawns a fresh one.

### Step 3 — working directory

A conversation has a cwd before it has a terminal. Inherit from the active tab at
creation, show it in the pane header, spawn there. Fall back to the workspace root.

**Done when:** a restored conversation with no process still knows where it is, and
spawning later lands in the right place.

### Step 4 — adopt a typed `Surface` on conversations

**DECIDED 2026-09-05: adopt it.** Recorded here rather than left as a gate, because
the reason to decide early is that retrofitting it is the expensive path, and deciding
late is the same as deciding no.

A conversation carries which of *this app's* surfaces is rendering it, so it is not
bound to a particular pane and can be reopened or moved between them.

**Two deliberate departures from OpenDev's version:**

- **A typed enum, not `channel: String`.** OpenDev defaults to the string `"cli"`
  because it delivers to Slack, webhooks and a CLI, and an open set suits that. Here an
  unknown surface should be a compile error, not a silent mismatch, so: `Surface::Gui`,
  `Surface::Tui`, extended as surfaces are added.
- **Named `Surface`, not `Channel`.** "Channel" is right for OpenDev because it is a
  *delivery destination* — they push to it. Phosphor's conversations are pulled and
  viewed. Calling it a channel would imply a delivery mechanism that does not exist and
  invite someone to build against it.

**The honest justification, since an earlier draft used a wrong one:** this is for
surface independence within the app — Phosphor already ships two surfaces, the GUI and
`crates/warp_tui`, and a conversation bound to one specific `TerminalView` in one of
them is the constraint being removed. It is **not** for remote agents; see §4a.

**Done when:** a conversation records its surface, the GUI and TUI both set it, and
nothing reads a hardcoded assumption about which surface a conversation lives on.

---

## 4. Prior art, and the idea we have not committed to

OpenDev (`opendev-to/opendev`, Rust, MIT) already does the decoupled half. Its
`Session` has no terminal, view or pty in it — just messages, `context_files`,
`working_directory`, `parent_id`, `subagent_sessions`, and:

```rust
pub channel: String,                             // defaults to "cli"
pub thread_id: Option<String>,
pub delivery_context: HashMap<String, Value>,
```

**The surface is a field, not an ancestor.** A session is not *in* a TUI; it *has* a
delivery channel. That reframes this branch's ask from "add a pane type" to "give
conversations a surface field, of which the existing terminal pane is one value."

Bigger refactor, better foundation, and explicitly **not committed to** — it is
step 4's decision. Recorded here so it is a choice rather than an omission.

Also worth taking if local orchestration is ever revisited: `subagent_sessions:
HashMap<tool_call_id, session_id>` plus `parent_id` means fan-out and forking cost no
new type.

**Explicitly rejected:** OpenDev's execution model. It runs `Command::new("sh")
.current_dir(..)` per tool call — no persistent shell, no pty, no surviving `cd`, no
interactive programs. Correct for a coding agent, wrong here. Phosphor **is** a
terminal; the block list showing a real pty is the product.

---

## 4a. What this unlocks: execution location becomes a property

The framing above — and `DESIGN-PHOSPHOR-FORK.md` §9 — describes this as a UI change:
chat not tied to a terminal. That undersells it. **The same seam makes "where does
this conversation execute" a question the code can ask.**

Today it cannot. A conversation *is* a view onto a local `TerminalView` with a local
pty; the answer is hardcoded by structure, so there is nowhere to put a different one.
Once a conversation owns a lazily-spawned execution context instead of being owned by
a terminal, the target of that spawn is a value.

**And the remote machinery already exists.** Phosphor knows how to have non-local
terminals — `SessionType::{Local, Remote, WarpifiedRemote}`, the remote-server
extension, the ssh wrapper. It is currently reached by the user typing `ssh`, not by a
conversation choosing where to run. A conversation on a laptop spawning its terminal
on a build box is not a new subsystem; it is the existing one reached through a new
seam.

With step 4's `Surface` field these become two independent axes:

| axis | question it answers |
|---|---|
| execution context | where the work runs — local, ssh host, container |
| surface | which of *this app's* surfaces is rendering it — GUI pane, TUI |

**Correction, 2026-09-05:** an earlier draft of this section claimed those axes being
separable is "what remote agent actually means: work running on a machine you own,
viewed from another, surviving the viewer going away." **That was overstated and is
wrong.** Viewing a conversation that lives on another machine needs a transport, and
there are only two:

- **SSH in and view it there.** Works today via warpified remote, and needs no surface
  field at all.
- **Sync the conversation between machines.** That is precisely the transport dropped
  with the cloud layer, and nothing on this branch reinstates it.

So the surface field does **not** buy cross-device viewing. It buys surface
independence *within one running app*. The execution-location argument above stands on
its own — it is about where the spawned terminal lives, and Phosphor already has
`SessionType::Remote` for that — but it does not depend on `Surface` and should not be
justified by it.

### This is not the cloud orchestration we dropped

Phosphor dropped Warp's orchestrator, `server_api`, RunAgents/StartAgent and connected
self-hosted workers. **That decision was about Warp's servers, not about remoteness.**
`DECLINED.md`'s false-positives list is explicit on the distinction:

> **`app/src/remote_server` / `crates/remote_server`** — Phosphor's SSH remote-host
> daemon, entirely local. Not Warp's cloud backend, despite the name.

An agent running on a host you own, over your own SSH, with your own provider keys, is
squarely BYOP. Do not file it as cloud, and do not let the word "remote" trigger
`script/check_cloud_boundary` reasoning by reflex.

### What this changes about step 2

**Design the spawn path to take a target from the start.** Not because remote
execution is in scope for the first cut — it is not — but because "assume local, add a
target later" is a retrofit, and this branch already has one retrofit hazard it is
trying to avoid (the `channel` decision at step 4). A spawn function that takes an
explicit local target costs nothing now and is the difference between a later feature
and a later rewrite.

Concretely, step 2's "done when" gains a clause: the spawn entry point names its
target explicitly, even though `Local` is the only value it can currently be given.

---

## 4b. Remote execution: the target, and what it needs

**DECIDED 2026-09-05: Model A is the target. Model B is parked, not rejected.**

### Model A — remote *execution*. The laptop drives.

Conversation, LLM calls and credentials stay local. Only tool execution goes to the
remote host.

- **Auth is already solved.** Transport is the user's own SSH keys and agent. No
  provider credential ever leaves the machine — which matters, because "credentials
  stay on disk, privacy-first" is §1 of this document's parent.
- **cproxy keeps working unchanged**, and this is not incidental. cproxy never executes
  tools; its entire design is to name one and stop, leaving the client to run it. Where
  the client runs it is none of its business — local pty, SSH'd pty, container, the
  conversation looks identical. It stays bound to loopback, one user, no tunnel, which
  is the property its ToS position rests on. cproxy lives in a separate repository and
  nothing in this tree mentions it, so it is recorded here or it is forgotten.
- If the laptop sleeps, the agent pauses and the remote holds an idle shell.

### Model B — remote *agents*. The remote drives its own loop.

Parked, with two named blockers:

- **Credential distribution.** The remote needs a provider key: forwarded per session
  (exposed to the remote process), provisioned per box (N boxes, keys at rest somewhere
  unwatched), or called back through a broker on the laptop. The third is the least bad
  and is what cproxy already is — but a remote reaching it needs a reverse tunnel, which
  widens an endpoint deliberately scoped to one process on one machine.
- **History reconciliation.** Messages accumulating remotely while the laptop is off
  means two histories to merge. That is the sync transport dropped with the cloud layer.

Note the cheap version of B collapses into A: if the remote calls back to a broker on
the laptop, the laptop must be awake, which is Model A wearing a hat.

**This is not the cloud orchestration that was dropped.** That decision was about
Warp's servers, not about remoteness — `DECLINED.md` is explicit that the remote-server
daemon is "entirely local. Not Warp's cloud backend, despite the name."

### The transport must not be tmux

`DECLINED.md` records keeping the SSH tmux wrapper permanently, and it is a fine
*terminal* feature. **It cannot be the reattach mechanism for remote execution**, for a
reason already documented there:

> The tmux flow needs tmux control mode, which needs DCS, which **ConPTY does not
> support** ... So on Windows the remote-server extension is the **only** route to a
> warpified SSH session.

A remote-execution design resting on tmux is a design that does not work on Windows,
and the same entry records that this asymmetry was accepted for *terminal warpification*
specifically — not as licence to build every future remote feature on a Unix-only
substrate. Reattach is also not what tmux is for here: we need to reattach a **session
the app owns**, not a shell the user started.

### What is actually needed

A lightweight remote agent — a small binary the app can install and speak to over SSH,
owning the remote side of a session and supporting clean reattach.

Requirements, in priority order:

1. **Cross-platform.** Windows included, which rules out tmux and anything else needing
   DCS or a Unix-only pty control channel.
2. **Lightweight.** A single static binary, installed over the existing SSH path, no
   runtime dependency on the remote and nothing to configure by hand.
3. **Secure by construction.** No listening socket of its own; everything over the SSH
   channel the user already authenticated. No credential at rest on the remote — which
   falls out of Model A, since none is sent.
4. **Reattachable.** Survives the client disconnecting, so a dropped laptop does not
   kill an in-flight command, and can be re-adopted on reconnect.
5. **Discoverable and disposable.** The app can tell whether it is installed, install
   it, upgrade it, and remove it, without the user managing versions.

**The precedent exists.** `crates/remote_server` and `app/src/remote_server` are exactly
this shape — Phosphor's SSH remote-host daemon, installed over SSH, "entirely local",
already the only warpification route on Windows. The right first question is not "what
should we build" but **"what does the remote-server extension already do, and what is
missing for it to own a session rather than assist a shell?"**

### What the remote-server extension already does — answered 2026-09-05

**It has, and these are the tedious parts:** an install path over SSH that downloads a
published tarball and verifies it against a SHA-256 pinned into the client at build
time, **fail-closed** — no digest, no install (`install_remote_server.sh`, and the
release workflow's "Pin CLI tarball digests" step). A proxy/daemon split with
identity-scoped sockets (`LaunchMode::RemoteServerProxy` / `RemoteServerDaemon`,
`warp_cli/src/lib.rs:309,316`). A framed protocol with size limits
(`remote_server/src/protocol.rs`). Preinstall capability detection (`RemoteOs`,
`RemoteArch`, `UnsupportedReason`, `PreinstallStatus`).

**It lacks session ownership entirely.** The whole operation surface
(`remote_server/src/manager.rs:111`):

```rust
pub enum RemoteServerOperation {
    NavigateToDirectory,
    LoadRepoMetadataDirectory,
    IndexCodebase,
    ResyncCodebase,
    DropCodebaseIndex,
    GetFragmentMetadataFromHash,
}
```

Filesystem navigation and codebase indexing. No pty, no process lifecycle, no reattach.
It **assists a shell the user started; it does not host one.** That is precisely the
gap, and it is why the file tools are withdrawn when it is absent (`6a021357f`).

So the work is not "build a remote agent". It is **add session ownership to something
that already solves install, transport, identity and platform detection.**

### What is actually in `phosphor-cli` — and it is close to the whole app

`script/linux/bundle:220-224` is the answer:

```sh
if   [[ "$ARTIFACT" == "cli" ]]; then FEATURES="$FEATURES,standalone"
elif [[ "$ARTIFACT" == "app" ]]; then FEATURES="$FEATURES,gui,nld_improvements"
fi
```

Same `--bin phosphor-oss` for both. The CLI is the *same binary target* as the desktop
app, built with `release-cli` instead of `release-lto` and statically linked against
musl, with `gui` off and `standalone` on. Everything not behind `#[cfg(feature =
"gui")]` ships: the agent, the terminal model, the block list, providers, MCP.

That is why the published `phosphor-cli-linux-x86_64.tar.gz` is **62.7 MB**. Against
§4b requirement 2 — "a single static binary, nothing to configure" — it passes on shape
and fails on weight, and it is worth being honest that this is a consequence of reusing
what existed rather than of what a remote daemon needs.

**Not an argument to rewrite it.** A purpose-built remote binary is a bigger change than
reusing the CLI and would fork the install path. But if the daemon grows to own
sessions, "what is in this binary and does the remote need all of it" becomes a real
question rather than a rhetorical one.

### Windows: there is currently no route at all

This is the finding that matters most for the requirement above, and it is worse than
"tmux does not work there".

`remote_server/src/setup.rs:196`:

```rust
pub enum RemoteOs { Linux, MacOs }
```

No Windows. Detection is `parse_uname_output` over `uname -sm`, which a Windows host
does not answer. And the published artifacts confirm it — the release ships
`phosphor-cli-{linux-x86_64, macos-x86_64, macos-aarch64}.tar.gz` and **no Windows CLI
at all**; the only Windows asset is `phosphor-tui-windows-x64.zip`, which is the TUI,
not the remote server. The digest pins match: `PHOSPHOR_CLI_SHA256_` exists for
`LINUX_X86_64`, `LINUX_AARCH64`, `MACOS_X86_64`, `MACOS_AARCH64`, and nothing for
Windows.

So for a Windows **remote host**: tmux cannot work (ConPTY has no DCS), and the
remote-server extension cannot install. **Both routes are closed.** `DECLINED.md`'s
accepted Windows asymmetry is about Windows as the *local* machine, where the extension
is the only warpification route — it says nothing about Windows as a *target*, and that
gap has not been decided, only left.

Installing there needs four things that do not exist:

1. **A published Windows CLI artifact.** The release builds a Windows TUI but no CLI.
2. **A digest pin for it**, or the fail-closed check refuses the install — correctly.
3. **Platform detection that is not `uname`.** PowerShell `$PSVersionTable`, or `cmd /c ver`.
4. **An installer that is not a POSIX shell script.** `install_remote_server.sh` is `sh`.

None is hard individually. Together they are a deliberate piece of work, and none of it
is started.

---

## 5. Known risks, stated before they bite

- **"Session pending, indefinitely" is a new state.** `is_input_box_visible`, the
  block list, warpify detection and the Use Agent bar all reason about session state
  and were written assuming "pending" is brief. Each needs checking. This is the most
  likely source of subtle breakage on this branch.
- **`TypedPane` has many `match` sites.** The compiler finds them, but each is a
  decision about what a conversation pane should do, not a mechanical fill-in.
- **Persistence.** Conversations already persist; conversation *panes* do not.
  Restoring one must not resurrect a shell nobody asked for.
- **A conversation pane with no process still renders a block list.** What an empty
  block list looks like, and whether the zero-state is the right surface, is a
  product question nobody has answered.

---

## 6. Working practice on this branch

- Same rules as `main`: no local builds without the maintainer's say-so, agents never
  build, `rustfmt --check` is the agent-level gate, and the build box does the real
  verification.
- **Refute before building.** Every non-trivial change on this branch gets an
  adversarial review pass before it goes near the build box. On `main` that process
  caught a 1-row pty, a viewer resizing the sharer's terminal, frozen auto-follow, and
  an unreachable ssh prompt — all of which would otherwise have shipped.
- Divergences from the pin get recorded in `DECLINED.md` under `IMPROVED`, per the
  revised §5.10. Everything on this branch is a divergence; that is the point.
