# Phosphor fork — design decisions & direction

English design record for this fork. Companion to `AGENTS.md` (the code map)
and the `specs/` docs. Read this to understand *why* the fork is shaped
the way it is and where it is heading, before making architectural changes.

## 1. Identity & lineage

- **Upstreams (two, different roles):**
  - `warp` remote → `warpdotdev/warp` — **Warp OSS proper**. The original, still
    moving daily. Cloud-agent product.
  - `upstream` remote → `zerx-lab/zap` — the **"Zap" fork** this repo descends
    from. BYOP-native, but not actively developed (mostly bug-fix branches).
  - `origin` → this personal fork (`jwp2987/phosphor`, previously `jwp2987/zap`).
- **What this fork is:** a **BYOP** (Bring-Your-Own-Provider) terminal/agent. The
  cloud half of Warp (orchestration, Drive sync, auth/billing, remote agents) is
  stripped; the agent talks **directly to a user-configured OpenAI-compatible /
  Anthropic / Gemini / Ollama / Vertex endpoint** via the vendored `lib/rust-genai`.
  Primary test target: **FastFlowLM (FLM)**, a local small-model server.
- **North star — match Warp minus cloud.** Phosphor should stay as close to
  feature-parity with upstream Warp as possible, dropping **only genuinely
  cloud-dependent** features. When a Warp feature is missing from Phosphor, the default
  is to **build or port it for parity**, not stub or drop it. Drop only cloud:
  orchestration, remote/RunAgents/StartAgent child agents, `server_api`,
  connected self-hosted workers, ambient agents, cloud harness, oz child-launch,
  Drive sync, auth/billing. If a feature is cloud-*adjacent* (e.g. conversation
  restoration/selection may tie to cloud sync), check whether a local-only version
  is viable before dropping.
- **Direction:** started as personal tinkering; now moving toward a **real,
  distributable product**. This raises the bar: source-disclosure discipline
  (AGPL — see §6), upstream convergence, and maintainability all matter now.

## 2. BYOP architecture (the core of the fork)

- Requests go **straight to the configured `base_url`** through
  `chat_stream::build_client` (genai `ServiceTargetResolver`). No cloud proxy.
  Only external metadata call is the models.dev catalog fetch.
- **API keys** live in OS secure storage (`AgentProviderSecrets`), never in
  `settings.toml`. Provider list + non-secret config live in `settings.toml`
  under `agents.warp_agent.providers`.
- **Per-profile / per-prompt system prompt overrides**: Auto / built-in family /
  custom file per model slot, stored in the sqlite object store, hot-reloadable
  from a prompt template dir (`ZAP_PROMPT_DIR` or the settings panel). Path-guarded
  (no absolute / `..`). Motivation: keep small FLM-served models from drowning in
  a 9k-token default prompt.
- **Prompt-cache discipline**: volatile env (time, git) is kept out of the stable
  system-prompt prefix so upstream KV/prompt caches survive.

## 2a. What ships in each artifact — the CLI is the app minus the GUI

Worth knowing before reasoning about binary size, remote installs, or what a given
artifact can do, because the naming misleads.

**The desktop app and `phosphor-cli` are the same `--bin` target.** `script/linux/bundle`
(and its macOS/Windows siblings) build `phosphor-oss` for both; the only differences are
the compile profile, static linking, and two feature flags:

```sh
# script/linux/bundle
if   [[ "$ARTIFACT" == "cli" ]]; then FEATURES="$FEATURES,standalone"
elif [[ "$ARTIFACT" == "app" ]]; then FEATURES="$FEATURES,gui,nld_improvements"
fi
```

- **app** — `release-lto`, dynamically linked, `gui` on.
- **cli** — `release-cli`, statically linked against musl on Linux, `gui` off.

So everything not behind `#[cfg(feature = "gui")]` is in the CLI: the agent, the
terminal model, the block list, providers, MCP, the settings registry. It is the app
with the rendering layer compiled out, not a separate small tool. That is why the
published `phosphor-cli-linux-x86_64.tar.gz` is ~63 MB.

**Why this matters in practice.** The CLI tarball is what
`crates/remote_server/src/install_remote_server.sh` installs on a remote host for the
remote-server extension, digest-pinned and fail-closed. So "install the remote server"
means shipping a near-complete copy of Phosphor to the remote machine. That is a
reasonable consequence of reuse — it avoids forking the install path and the release
pipeline — but it should be a known consequence rather than a surprise, particularly if
the remote side ever grows beyond its current job of filesystem navigation and codebase
indexing.

**The TUI is a third thing.** `phosphor-tui` is a separate binary (`crates/warp_tui`)
with its own updater, and its published archive is larger still. Do not assume "CLI" and
"TUI" are the same artifact under different names.

## 3. Warp OSS sync strategy

The fork is ~1700 commits behind current Warp OSS across a hard product split, so
**wholesale rebase is impossible**. Instead:

- **General fixes → prefer `zerx-lab/zap`**, not Warp OSS. Same lineage, BYOP-native,
  clean cherry-picks. Warp-OSS picks conflict heavily (architecture rewrites).
- **Reactive, not systematic**: cherry-pick a fix when a bug actually bites, using
  `git cherry-pick -x` for provenance. The clean-apply zone is narrow (see
  `specs/warp-oss-sync/SCOPE.md` for the measured yield: ~12/23 candidates landed,
  mostly `context_chips`).
- **TUI → only Warp OSS has it.** No BYOP-native TUI exists upstream, so the TUI is
  a port from Warp OSS with cloud→BYOP rewiring (see §4).

## 4. TUI port — guiding principles

Porting Warp's `warp_tui` (a ratatui terminal front-end) onto Phosphor. The crate rides
on ~3 months of `warpui_core` refactoring Phosphor never took, so the port is really a
**targeted re-convergence of the UI core**. Governing rules:

- **Isolate, don't refactor the GUI.** Every change is either `#[cfg(feature =
  "tui")]`-gated or a pure widening. The default GUI build must stay green — this
  is verified with a **GUI regression gate** (`cargo check -p warp --features gui`)
  after every change that touches shared `warpui_core` code. This gate has already
  caught one real regression; trust it.
- **Widen, don't fork, core bounds.** The view read/update/storage path was relaxed
  `T: View` → `T: Entity` (View: Entity, so a pure widening) so TUI views
  (`TuiView: Entity`, not `View`) can use `ViewHandle`/`ViewContext`. This mirrors
  upstream and keeps one code path.
- **Separate storage over enum surgery.** Rather than convert the GUI's
  `Window.views` map to upstream's `StoredView` enum (~83 hot-path sites), TUI views
  live in a **separate gated `Window.tui_views` map**. Chosen deliberately to keep
  GUI blast radius at zero. (Trade-off: diverges from upstream on this one point;
  the tui-only files that reference it are ours to maintain anyway.)
- **Trim imported crates to what's actually used.** `warp_search_core` was imported
  as the **inline_menu subset only** — the Tantivy full-text `searcher`/`macros`
  half is dropped, since `warp_tui` only needs the menu system. Keeps the dep
  surface (and build) small.
- **Adapt small missing deps, don't import heavy ones.** `warp_errors` (a Sentry
  helper crate) is adapted away to `log::error!` rather than imported.
  `warp_core::async::debounce` (129 lines) is vendored locally rather than pulling
  a missing module.
- **English comments.** All comments/docs added in this fork's feature work are
  English (the maintainer does not read the Chinese in the inherited codebase).

### Phase status (see `specs/warp-oss-sync/SCOPE.md` for detail)

- Phase 1 (warpui_core tui foundation): **done**, GUI-verified.
- Phase 2 (app-crate `tui` feature): **done**.
- Phase 3 (build warp_tui, drop cloud, rewire agent→BYOP): **in progress**
  (3a: trimmed warp_search_core imported).
- Phase 4 (entry bins + workspace wiring + full build): pending.

## 5. Upstream convergence direction

The fork increasingly re-adopts upstream's *shared infrastructure* where it lowers
long-term cost, while keeping the BYOP product identity. Concretely:

- `warpui_core` migrated to edition 2024 (matching upstream).
- View-core bounds relaxed to match upstream's Entity-generic design.
- `warp_search_core` now exists in Phosphor as a shared crate (as upstream intended).
- **Deferred convergence project:** Phosphor still has its own `inline_menu` inside the
  app crate (~53 files). Upstream shares it via `warp_search_core`. Eventually the
  GUI could migrate onto the shared crate and the legacy app-crate inline_menu be
  dropped — but that is a **large, separate refactor with a different API**, kept
  decoupled from the TUI port on purpose. Not a quick follow-up.

## 6. Licensing guardrail

Dual-licensed: **AGPL-3.0-only** (the `app`/`warp` crate and most of `crates/*`)
and **MIT** (`warpui`, `warpui_core`); `lib/rust-genai` is MIT/Apache-2.0.
Copyright: Denver Technologies (Warp). Because the shipped binary links the AGPL
app crate, **the distributed whole is AGPL**. Implications for the product
direction:

- Distributing a modified build requires offering **complete corresponding source
  under AGPL**. No closed-source distribution of a modified Warp/Phosphor.
- **Do not** use Warp's name/branding/trademarks; pick a distinct fork name. AGPL
  §7 declines trademark grants.
- Keep copyright/license notices intact.
- A public source repo (like this fork) already satisfies AGPL for personal use;
  the obligations bite on *distribution* or a *networked service*.

## 7. Working guardrails for agents

- Never break the default GUI build. Run the GUI regression gate after shared-crate
  changes.
- Prefer additive + feature-gated changes; prefer widening over forking bounds.
- New comments in English.
- Commit in small, verified increments; `-x` provenance on cherry-picks.
- Work on a branch, push to `origin`; don't assume distribution rights beyond that.

## 8. The window footer bar

**Decision (2026-09-05): every terminal window gets a persistent, fixed-height
footer bar. Chrome that asks the user something goes there. Nothing is drawn over
a running program, and nothing is injected into the block list underneath one.**

### Why this exists

`top` was losing its five summary lines, and `watch` its header. Seven attempts
to fix that arithmetically were written and reverted, and the reason each failed
is the same: the chrome involved is *conditional*, and a terminal's row count
cannot be.

The pty is told how many rows it has, once, and a program sized from `$LINES`
paints exactly that many. Anything occupying part of that area which the row count
does not know about is space the program believes it has and cannot see, and it
loses the difference off the top. Two things were doing it:

- the **pinned snackbar header**, drawn *over* the block list. Fixed in
  `ec2e2d227` — it is no longer pinned while a command runs. Recorded under
  `IMPROVED` in `DECLINED.md`; the same defect is present at the pin and in
  shipping Warp.
- the **Use Agent bar**, inserted *into* the block list below the running block,
  along with roughly eight other rich-content items — the ssh warpify card, the
  tmux install prompt, ssh errors, the shell-terminated banner, the agent handoff
  offer, MAA code-diff suggestions.

Every arithmetic fix for the second one failed in a different way, and the failures
are worth keeping because they define the constraint:

| approach | why it failed |
|---|---|
| rows from the rendered content element | equals the pane in `PinnedToBottom`; a feedback loop in `Waterfall` |
| rows minus the block's `output_grid_offset()` | contains the echoed command grid — unbounded; a heredoc yields a 1-row pty |
| suppress the bar via its render predicate | that predicate is evaluated once at insertion, never again while a program paints |
| rows minus the bar's measured height | the helper has no type filter, so ssh cards resize the pty; and it leaked into `natural_rows`, which a shared-session viewer reports to the *sharer* |
| clamp the scroll to the output's top | froze auto-follow for every command over 50 ms emitting more than a screenful |
| clamp the scroll to the output's bottom | pushed the ssh choice prompt and CLI-agent toolbar below the viewport, unreachable |
| reserve a constant for the bar | the total it folds into is applied unconditionally, so **every** window lost the rows whether the bar was shown or not |

The pattern: **a conditional surface cannot be reserved for, measured, or scrolled
around without breaking something else.** A permanent one can simply be subtracted.

### What the bar is

- **Always present**, on every terminal window, whether or not it currently has
  anything to say. This is the load-bearing property. `rows = pane − bar` becomes a
  constant, so there is no resize when chrome appears, no measurement of a
  previous frame, and no predicate anywhere near the row calculation.
- **Fixed height.** Content adapts to the bar, never the reverse — the moment the
  bar can grow, the row count varies again and every failure above returns.
- **Outside the block list**, as a column sibling. This is what alt-screen mode has
  always done, and it is why alt screen never had this bug.
- **The home for questions.** Use Agent / tag-in, warpify and tmux prompts, ssh
  errors, handoff offers, shell-terminated notices. Anything that currently calls
  `append_rich_content(.., insert_below_long_running_block: true)` is a candidate.

### Where the constant-rows premise does not hold

**Waterfall mode.** `BlockListElement::layout` returns `constraint.max` for
`PinnedToBottom` and `PinnedToTop`, but for `Waterfall` it returns
`constraint.max.y().min(visible_height_px)` — i.e. the block list is *content*-sized
there. Shrinking the constraint by the bar therefore changes the reported size only
when the content is already at least as tall as the pane, so the bar is sometimes
subtracted and sometimes not, and rows in that mode are content-derived regardless.

This is **pre-existing** — it is the same content-sizing that made the very first
arithmetic attempt oscillate (row 1 of the table above) — and the footer bar inherits
it rather than causing it. But "`rows = pane − bar` is a constant" is stated above as
the load-bearing property, and it is only true in two of the three input modes. Anyone
reasoning from that sentence in a Waterfall context will reach a wrong conclusion.

Not fixed here, and deliberately so: making Waterfall's rows pane-derived is a separate
change with its own gap arithmetic to reconcile. Recorded so the premise is not quoted
more broadly than it holds.

### What it costs, stated plainly

- **Roughly two rows, permanently, on every window.** vim, tmux and VS Code all
  pay this for a status line; it is well-trodden, but it is a real cost and it is
  being taken deliberately rather than as a side effect.
- **Two surfaces need redesigning to fit.** The CLI-agent toolbar is built with
  `Wrap::row()` and currently grows to two or three rows in a narrow pane — in a
  fixed bar it must truncate, scroll, or overflow into a popover. The ssh install
  card is a paragraph plus a rendered command and does not fit a bar at all; it
  needs to become a compact prompt. Arguably an improvement either way: a card
  that displaces a running terminal is not good behaviour.
- **Express the height in pixels, not lines.** The bar is a button row plus fixed
  padding (~28px); a line-denominated constant is only correct at one font size
  and line-height ratio, and silently under-reserves at `line_height_ratio = 1.0`.

### Relationship to upstream

Warp has no footer bar, and has both underlying defects. Under the old reading of
`ORACLE.md` — pin as specification — that would have been an argument against this.
It is not one any more: as of 2026-09-05 the pin is a source of suggestions and
evidence, and a deliberate improvement needs a record rather than a justification.
This section is that record; see also `AGENTS.md` §5.10 and the `IMPROVED` section
of `DECLINED.md`.

### Open questions, not yet decided

- Whether the bar is visible when empty, or collapses to a hairline while still
  reserving its rows. Collapsing keeps the pixels honest without wasting the
  visual space; reserving-but-hiding is the simplest thing that preserves the
  constant.
- Whether it also carries passive status (cwd, git branch, agent state) or stays
  strictly interactive. The passive case argues for always-visible.
- Migration order. The Use Agent bar is the one with a reproducible bug and should
  move first; the ssh cards need their redesign before they can follow.

## 9. Standalone conversations (chat not tied to a terminal)

**Decision (2026-09-05): a conversation should be openable as its own pane, with no
terminal behind it, and should spawn one on demand the first time it actually needs
a shell.**

### Why it does not exist today

A conversation is currently a *view onto a terminal's block list*, not a thing in
its own right. `AgentViewController` is keyed on `terminal_view_id: EntityId`
(`app/src/ai/blocklist/agent_view/controller.rs:375`), `CLIAgentSessionsModel`
sessions are looked up by terminal view id, and `TypedPane` has `Terminal`, `Code`,
`Notebook`, `AIDocument` and no `Conversation`
(`app/src/workspace/view/vertical_tabs.rs`). The *data* is already independent —
conversations persist to SQLite under `general.persist_conversations` and surface in
`/conversations` — but the view cannot exist without a pty behind it.

So "chat with no terminal" is a new pane type, not a flag.

### The architectural decision: lazy **process**, eager **view**

The obvious design — make `terminal_view_id` optional and teach everything
downstream to cope — is the expensive one and touches a great deal of code that has
no business knowing about this feature.

**Instead: create the `TerminalView` immediately, and defer only the pty spawn.**

- Every consumer that needs a `terminal_view_id` keeps working unchanged, because
  one exists from the start.
- "Spawn on demand" means spawning the **process**, not the view. The model already
  distinguishes an unestablished session — `TerminalModel::pending_session_id()`
  returns `Option<SessionId>` (`app/src/terminal/model/terminal_model.rs:1674`) —
  so a view whose session has not started is a state the code already contemplates.
- The blast radius collapses from "decouple conversations from terminals" to "let a
  terminal view exist with no process until asked".

That is the whole design. Everything below is consequence.

### When the process spawns

On the **first tool call that needs a shell** — `run_shell_command`, or a file tool
on a host with no remote-server extension, which is the same thing by another route.
Text-only exchanges never spawn anything, which is the common case for a chat pane
and the reason this is worth doing at all.

Not on pane open, not on first message, not speculatively.

### Where the terminal appears

**Split into the same pane, below the conversation.** Not hidden, not a new tab.

The user has to see what ran — that is the entire premise of a block-based terminal,
and hiding execution behind a chat surface would be the wrong product in this app.
A new tab would be worse: it separates the command from the conversation that caused
it, and it would appear in the tab list as a second entry for one piece of work.

The tab keeps its `Agent` band in the vertical tab list (see the tab sectioning
work), because what it *is* has not changed.

### Working directory

A conversation pane needs a cwd before it has a terminal, since the agent will ask
about files. Inherit from the active tab at creation time and show it in the pane
header; the spawned process starts there. If the user has no active tab, fall back
to the workspace root.

### Lifetime

The process is owned by the conversation, not the reverse:

- Killing the shell does not end the conversation; the next tool call spawns a new
  one. This is the case the current architecture cannot express at all.
- Closing the conversation kills the process.
- Persistence restores the conversation with **no** process, exactly as if it had
  just been opened. Restoring a shell nobody asked for would defeat the point, and
  a restored cwd is enough context to spawn correctly later.

### What this costs

- **A new `TypedPane::Conversation` variant** and its pane implementation, plus every
  `match` on `TypedPane` — there are several, and the compiler will find them.
- **A terminal view with no process is a new state** for code that assumes a live
  session. `is_input_box_visible`, the block list, warpify detection and the Use Agent
  bar all reason about session state; each needs checking against "session pending,
  indefinitely" rather than "session pending, briefly, during bootstrap".
- **The footer bar (§8) applies here too.** A conversation pane gets the same bar, and
  gets it for free if §8 lands first — which is an argument for ordering them that way.

### Prior art: OpenDev (`opendev-to/opendev`, Rust, MIT)

A terminal-native coding agent that already does the decoupled half. Its `Session`
(`crates/opendev-models/src/session.rs`) has **no terminal, view or pty in it at all**:

```rust
pub struct Session {
    pub id: String,
    pub messages: Vec<ChatMessage>,
    pub context_files: Vec<String>,
    pub working_directory: Option<String>,
    pub parent_id: Option<String>,                   // forked from
    pub subagent_sessions: HashMap<String, String>,  // tool_call_id -> child session
    pub channel: String,                             // defaults to "cli"
    pub thread_id: Option<String>,
    pub delivery_context: HashMap<String, Value>,
    ...
}
```

Three things to take from it, and one to reject.

**Take: `working_directory` on the session.** Independent arrival at the same
conclusion as above — a conversation has a cwd before and without any terminal.

**Take: the surface is a field, not an ancestor.** `channel` (defaulting to `"cli"`)
alongside `thread_id` and `delivery_context` means a session is not *in* a TUI or a
web view; it *has* a delivery channel. This is a stronger decoupling than the
"standalone pane" framing above and it is the more useful idea: a Phosphor
conversation could then render in a terminal pane, in a chat pane, or somewhere not
yet built, without the session knowing which. **Adopting it changes the ask from "add
a pane type" to "give conversations a surface field, of which the existing terminal
pane is one value."** That is a bigger refactor and a better foundation; it is not
required for a first cut, and this section does not commit to it.

**Take: sub-agents are just sessions.** `subagent_sessions: HashMap<tool_call_id,
session_id>` plus `parent_id` means fan-out and forking cost no new type. Worth
remembering if local orchestration is ever revisited — Phosphor dropped Warp's
orchestrator as cloud, but OpenDev's is local, which is a different proposition.

**Reject: the execution model.** OpenDev runs `Command::new("sh").current_dir(..)`
per tool call (`crates/opendev-tools-impl/src/bash/foreground.rs:48`,
`background.rs:26`) — no persistent shell, no pty, no `cd` that survives, no
interactive programs. That is right for a coding agent and wrong here. Phosphor **is
a terminal**; the block list showing a real pty is the product. The "spawn a real
terminal on demand" design above is the correct answer for this app, and adopting
per-command `sh -c` would be a downgrade dressed as simplification.

### Open questions, not yet decided

- Whether to adopt `channel` as a session field now or keep the pane-type framing and
  retrofit later. Retrofitting is the usual way this gets expensive.
- Whether the split appears at spawn time or is pre-allocated at zero height. Appearing
  is honest; pre-allocating avoids a layout jump mid-answer.
- Whether a conversation can be *promoted* from an existing terminal tab (the inverse
  of the tab-sectioning migration) or only created fresh.
- What `/conversations` history does with conversations that never spawned a shell —
  they are strictly cheaper to restore and might deserve different retention.

### Working practice

This lands on its own branch. It is a new kind of work for this fork — a new pane
type, a terminal view that can exist without a process, and possibly a surface field
on conversations — and it should not share a branch with parity or bug-fix work.
