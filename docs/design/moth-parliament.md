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

### Step 2 — no execution, ever

**DECIDED 2026-09-07, superseding "spawn on demand".** A conversation pane never
spawns a shell. It is a chat surface with file access: **everything except
execution.**

The previous plan had the first tool call needing a shell spawn a process into a
split below the conversation. That is withdrawn. It made a conversation pane an
agent tab that had not spawned *yet*, which is a state, not a type — and left the
product question ("what is this pane?") answered by timing.

What this settles, each of which was previously open or fudged:

- **The tool set is the enforcement, not a refusal.** The conversation agent is
  configured without execution tools, so the model never has the option. A guard
  that refuses `run_shell_command` at call time is the same defect relocated: the
  agent still believes it can run commands and still tries.
- **Terminal mode is removed, not disabled.** `esc` must not offer a terminal
  prompt in a conversation pane. Offering a mode that cannot work is worse than
  not offering it — and note the earlier attempt at this hid the *input box*
  instead, which removed the only way to talk to the agent at all. Suppress the
  mode switch; never the composer.
- **`write_to_pty`'s refusal is permanent**, not scaffolding. Paste, Ctrl-C,
  Ctrl-D and drag-and-drop reach the pty without passing the composer, so the
  guard stays as the backstop for routes that bypass the UI.
- **`MockTerminalManager` is permanent.** §2's "create the view eagerly, defer
  only the pty" stops being a deferral: there is no pty, ever. The eager
  `TerminalView` still earns its place for the reason §2 gives — every consumer
  expecting a `terminal_view_id` keeps working — but it is now scaffolding for
  compatibility rather than a staging state on the way to spawning.

**MCP tools stay available. DECIDED 2026-09-07.** They run in the MCP server's own
process, so they do not depend on this pane having a pty, and a name-based rule
cannot tell a read-only MCP tool from one that shells out. That means "no
execution" is enforced for Phosphor's own tools and not for MCP -- an MCP server
can still run commands on the user's behalf. Accepted deliberately: MCP is
configured by the user, per server, and silently withdrawing it from one pane type
would be more surprising than the gap it closes.

**Done when:** the conversation agent's tool set contains no execution tool; `esc`
does not offer terminal mode in a conversation pane; and a conversation can read
and write files without any process existing.

**Consequence for §4a/§4b.** Remote execution was justified partly by conversations
being unbound from a location. If conversations never execute, that rationale does
not apply to them: execution location becomes a property of *agent tabs*, not of
conversations. Revisit those sections before building on them.

### Step 3 — working directory

A conversation has a cwd before it has a terminal. Inherit from the active tab at
creation, show it in the pane header. Fall back to the workspace root.

**Load-bearing after step 2's decision, not preparatory.** File tools resolve
relative paths against a working directory, so a conversation that can read and
write files *needs* one. This is no longer groundwork for a future spawn — it is
what makes the tools a conversation does have work correctly.

**Why the header matters here specifically.** A conversation pane has file tools
and no shell, so there is no prompt and no `pwd` -- nothing on screen tells the
user which directory the agent will read and write in. For a terminal the prompt
answers that; for a conversation the directory is invisible state that affects
real file writes.

**Done when:** a restored conversation with no process still knows where it is, a
new conversation inherits the active tab's directory, and the pane header shows it.

*Partially built:* `conversation_pane_data` takes a cwd and `TerminalPane::snapshot`
falls back to `session_startup_path`, so the directory survives a restart. Not done:
inheriting from the active tab at creation (both live call sites pass `None`), and
showing it in the pane header.

### Step 4 — adopt a typed `Surface` on conversations

**DECIDED 2026-09-05: adopt it.** Recorded here rather than left as a gate, because
the reason to decide early is that retrofitting it is the expensive path, and deciding
late is the same as deciding no.

A conversation records which of *this app's* surfaces it was **created on**, so it is
not bound to one `TerminalView` in one frontend.

**"Created on", not "currently rendering". DECIDED 2026-09-10.** An earlier wording here
said "is rendering it", present tense, which the implementation does not do and should
not. The GUI and TUI share one conversation database, so a conversation started in the
GUI and later opened in the TUI still reports `Gui` -- correct under this definition,
and contradictory under the old one. A field that rewrites itself on every view is
state, not identity; recording the origin is stable, cheap, and is what the creation
path can actually know.

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

**CITATION WITHDRAWN 2026-09-10.** This section credited OpenDev's session shape as
working prior art. `moth-idea.md` §8 read the code and found it does not hold up:

- **`delivery_context` is dead.** No readers and no writers outside `opendev-models`
  and its own tests. The channel router keeps a separate, in-memory, never-persisted
  map instead. The field is a transcription artifact from the Python original, not a
  mechanism.
- **The claim that OpenDev "delivers to Slack, webhooks and a CLI" is false.** There is
  exactly one `ChannelAdapter` in the tree -- Telegram. `"cli"` and `"web"` exist only
  as name strings with no adapter registered.
- **The router does not use `Session` at all.** `resolve_session` mints its own ad-hoc
  id rather than going through `SessionManager`, so the router's sessions and the
  history crate's sessions are different objects.

So the abstraction cited here is not load-bearing even in its own codebase, and none of
the three fields above should be copied.

**The idea survives the citation, on its own merits.** "The surface is a field, not an
ancestor" is still right for Phosphor -- a conversation is not *in* a frontend, it
*records* one -- and step 4 adopted it. It is justified by Phosphor already shipping two
surfaces (the GUI and `crates/warp_tui`), not by OpenDev having done it. Keep the
reasoning; drop the evidence.

Also worth taking if local orchestration is ever revisited: `subagent_sessions:
HashMap<tool_call_id, session_id>` plus `parent_id` means fan-out and forking cost no
new type.

**Explicitly rejected:** OpenDev's execution model. It runs `Command::new("sh")
.current_dir(..)` per tool call — no persistent shell, no pty, no surviving `cd`, no
interactive programs. Correct for a coding agent, wrong here. Phosphor **is** a
terminal; the block list showing a real pty is the product.

---

## 4a. Execution location is a session property, not a conversation's

**REWRITTEN 2026-09-07.** The previous version of this section argued that giving
conversations a lazily-spawned execution context is what makes "where does this run"
a question the code can ask. That was a category error, and the maintainer caught it.

Where work runs is a property of the **execution context** — the session and its pty.
A conversation is chat. Tying a location to a conversation only looked natural because
the conversation happened to be the thing that had not spawned yet, and an empty object
is a convenient place to hang a decision. But deferral is not ownership.

**Phosphor already models this correctly, in the right place.**
`SessionType::{Local, Remote, WarpifiedRemote}` is on the *session*, as is
`set_remote_host_id`. Location is already a session attribute. The old §4a proposed
moving that decision onto conversations, which would have taken a property off the
object that owns it and put it on one that does not.

### What is actually missing

Not an architectural seam — a **session-creation affordance.**

Today the only way to get a remote session is to type `ssh` inside a local one. There
is no way to *ask for* a session on a host: no "new tab on build-box". The target is
decided by what the user types into a shell, after the local session already exists.

That belongs next to `PanesLayout::SingleTerminal` and the new-session menu — the
places a session is created — and it is independent of this branch.

### What this means for the conversation work

**Nothing. That is the point.** The two are unrelated, and the previous version of this
document claimed a dependency that does not exist:

- Step 2's decision (a conversation never executes) does not block remote execution.
  It removes conversations from the question entirely.
- Remote execution does not need a deferred spawn. A session created against a remote
  target from the start is *simpler* than one that spawns locally and redirects.
- An earlier draft of this rewrite concluded "agent tabs will need deferred spawn
  instead". That inherited the same mistake. An agent tab does not need to defer
  anything; it needs to be **created** against a target.

### The surface axis, unchanged

Step 4's `Surface` remains a separate, still-valid axis, and its correction stands:

> Viewing a conversation that lives on another machine needs a transport, and there are
> only two: SSH in and view it there (works today, needs no surface field), or sync the
> conversation between machines (precisely the transport dropped with the cloud layer).

So `Surface` buys surface independence *within one running app* — GUI pane vs TUI — and
must not be justified by remote execution. Two independent axes, neither derived from
the other:

| axis | question it answers | where it lives |
|---|---|---|
| execution target | where the work runs — local, ssh host, container | the session |
| surface | which of this app's surfaces renders it — GUI pane, TUI | the conversation |

### This is not the cloud orchestration we dropped

Phosphor dropped Warp's orchestrator, `server_api`, RunAgents/StartAgent and connected
self-hosted workers. **That decision was about Warp's servers, not about remoteness.**
`DECLINED.md`'s false-positives list is explicit on the distinction:

> **`app/src/remote_server` / `crates/remote_server`** — Phosphor's SSH remote-host
> daemon, entirely local. Not Warp's cloud backend, despite the name.

An agent running on a host you own, over your own SSH, with your own provider keys, is
squarely BYOP. Do not file it as cloud, and do not let the word "remote" trigger
`script/check_cloud_boundary` reasoning by reflex.

### What this used to impose on step 2, and no longer does

The previous version added a clause to step 2's "done when": that the spawn entry point
must name its target explicitly, so remote execution would not be a retrofit.

**Void.** Step 2 was decided on 2026-09-07 as "no execution, ever" — there is no spawn
path in a conversation to give a target to. The retrofit hazard it was guarding against
applies to session creation instead, where the target belongs.

---

## 4b. Remote execution: the target, and what it needs

**DECIDED 2026-09-05: Model A is the target. Model B is parked, not rejected.**

**Revised 2026-09-07: Model C (a broker, laptop-driven) supersedes A as the target.** It
does everything A does, adds N x M decoupling, and makes Windows tractable rather than
excluded. A is not wrong -- C is A with the transport factored out of the panes.

**Scope corrected 2026-09-07.** This section previously read as a continuation of the
conversation work, on §4a's now-withdrawn claim that a conversation's deferred spawn was
the seam remote execution needed. It is not, and this section does not depend on this
branch at all: it is about creating a *session* against a remote target, and it could be
built with the conversation-pane work finished, unfinished, or abandoned.

Everything below stands on its own terms. Read "the agent" here as an agent tab — the
pane type that executes — not a conversation pane, which by step 2's decision never does.

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

### Model C — a broker. The laptop drives; one component knows the endpoints.

**PROPOSED 2026-09-07 by the maintainer.** Not a variant of B. Topology and control are
independent axes, and an earlier draft of this section wrongly treated a broker as
implying remote autonomy:

| | laptop drives | remote drives |
|---|---|---|
| **direct (1:1)** | Model A — ssh to a remote pty | Model B — remote owns the loop |
| **broker (N x M)** | **Model C** | broker + autonomy (still blocked as B is) |

A broker sits between the app's surfaces and its execution endpoints. Terminals, agent
tabs and conversations dispatch work to it; it knows how to reach a target and route
the work there. The remote end can be a dumb executor — it does not have to be an
autonomous agent.

**It inherits none of Model B's blockers**, because the laptop is still driving:

- **No credential distribution.** Keys stay local, exactly as in Model A. Nothing is
  sent to the remote.
- **No history reconciliation.** The laptop owns the conversation and its history,
  because it owns the loop.
- **cproxy is untouched**, for the reason it is always untouched: it names a tool and
  stops. Where the client runs that tool is none of its business — local pty, ssh'd
  pty, or dispatched through a broker, the conversation looks identical. It stays bound
  to loopback with no tunnel.

**What it buys over Model A** is the N x M decoupling. One component knows about
endpoints; every surface dispatches through it, instead of each pane owning its own
transport. That is the difference between adding a second execution target and adding a
second copy of the transport code.

**It does not require conversations to execute.** Dispatching is not executing, so this
is compatible with step 2's decision. A conversation asking a broker to run something is
messaging a peer, not spawning a shell. The line worth drawing explicitly, before anyone
builds this: the remote end is a peer with its own tools, not a shell we are puppeting.
If that line blurs, "no execution" becomes execution with extra steps.

### Model C's broker is a local daemon, not an in-app component

**DECIDED 2026-09-08.** The broker runs as a separate local process, on the cproxy
model: loopback-bound, one user, no tunnel, started and managed by the app.

The alternative -- a model living inside the app that owns endpoint handles -- is
simpler and wrong. It dies with the app. Closing Phosphor would kill in-flight
remote work, and each window would hold its own endpoints.

A daemon changes what the model buys, and this is the actual argument for it:

**Remote work survives the app.** Close Phosphor, reopen it, and the remote sessions
are still there, because the broker held them rather than the GUI process. That is
most of what people want from "remote agents" -- work that outlives the window --
**without** Model B's autonomy, and therefore without either of Model B's blockers.
No credential leaves the machine and no history needs reconciling, because the laptop
is still driving; it just no longer has to be the same laptop *process*.

It is also the one precedent this fork already runs: cproxy is a local daemon that
decouples the app from a provider. A broker is the same move on the execution axis.

### Two things the daemon must not break

**Output must stream, incrementally, from day one.** Phosphor's product is the block
list showing a real pty -- this document rejects OpenDev's execution model on exactly
that ground ("no persistent shell, no pty, no surviving `cd`, no interactive
programs"). A broker that collects a command's output and returns it as a completed
blob would look like it works in a demo and be the wrong architecture. The protocol is
streaming or it is not this product. `remote_server`'s framed protocol already carries
size limits and framing, so this is a constraint on the *design*, not a missing
capability.

**The local endpoint must be a passthrough, not a round trip.** Local execution cannot
get slower or lose fidelity because a broker exists. If routing local work through the
same abstraction makes it feel different from today, the abstraction has failed and
should be reworked rather than shipped.

### Build order: the session target first, the daemon second

Do **not** start with the broker. Start with the session-creation affordance from 4a --
"new tab on build-box" -- which creates a session whose target is remote from the
outset, using `SessionType::Remote` and the remote-server extension that already ship.

That first cut proves the two genuinely hard things: the transport, and giving
`remote_server` the session ownership it currently lacks (it assists a shell; it does
not own one). The broker is then a refactor of *where the transport lives*, which is
far easier once one endpoint works than designed in the abstract. With a single
endpoint and a single surface a broker is indirection for its own sake; it earns its
keep at N x M.

### Open question: does the broker route file tools, or only execution?

Unsettled, and worth deciding before code exists. If a conversation pane's `read_files`
against a remote target goes through the broker, then a conversation is reaching a
remote machine -- and step 2's "dispatch is not execution" line blurs. Either answer is
defensible; drifting into one by accident is not.

### Model C is what makes Windows tractable

This is the strongest argument for it, and it inverts the constraint below.

The tmux/ConPTY problem is about a **pty control channel**. A broker's dispatch path has
none: it exchanges framed messages with a small binary. DCS never enters the picture, so
ConPTY's gap stops mattering, and requirement 1 below ("cross-platform, Windows
included") is satisfied by construction rather than by careful avoidance.

The remote half is also closer to existing code than to anything new. The audit further
down found `remote_server` already has the framed protocol with size limits, install over
SSH with a build-time-pinned SHA-256 that fails closed, the proxy/daemon split, and
preinstall capability detection (`RemoteOs`, `RemoteArch`). What it lacks is session
ownership — which is exactly what a broker's remote end would add.

**Do not overclaim this.** Dispatch is not an interactive remote terminal. A broker
cleanly unlocks "run this, stream the output back" on Windows. A shell you *type into*,
with a live pty and reattach, still needs pty semantics on the remote side — the harder
problem the tmux constraint was originally about. So this unlocks Windows **agents**, not
automatically Windows **warpified interactive ssh**.

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

**Under Model C this is the broker's remote half**, and the requirements below are
unchanged by that: they were written for a component that owns the remote side of a
session, which is exactly what a broker dispatches to. Requirement 1 stops being a
constraint to design around and becomes a property of the dispatch path -- see "Model C
is what makes Windows tractable" above.

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

**DEFERRED 2026-09-05: Windows as a remote host is out of scope for this branch.** Not
declined — deferred, with the four gaps above as the known cost. It is recorded here so
that a later reader finds a decision rather than an oversight, and so that nobody
designs the session-ownership work in a way that forecloses it. Concretely, that means
the remote-side protocol should not assume a POSIX shell, `uname`, or a POSIX path
shape, even while Linux and macOS are the only targets that can be built and tested.
Costing nothing now to avoid a rewrite later is the same argument §4a makes for naming
the spawn target.

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
