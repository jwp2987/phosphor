# The AI agent

Phosphor has an AI agent built into the terminal. You describe a task in plain
English; the agent proposes and — with your permission — runs shell commands,
reads and edits files, greps the tree, calls MCP tools and reports back. Every
step appears as a block you can read, approve, reject or interrupt.

The thing to understand before anything else: **Phosphor is
bring-your-own-provider (BYOP)**. There is no Phosphor account, no hosted
backend, no credits and no subscription. You supply an API key (or point
Phosphor at a local endpoint), Phosphor talks to that provider directly from
your machine, and the provider bills you. Nothing about the agent works until
you have configured at least one provider. Several agent features a Warp user
would look for do not exist here at all; they are listed under
[Not available in Phosphor](#not-available-in-phosphor).

Two surfaces run the same agent:

- the **GUI**, `phosphor-oss`
- the **terminal UI**, distributed as `phosphor-tui` (the cargo target is named
  `zap-tui-oss`; the release workflow renames it on the way out)

They share one app identity (`dev.phosphor.Phosphor`), so they share one
`settings.toml` and one keychain entry. A provider or key you add in either
appears in the other without a restart.

---

## Set up a provider (BYOP)

### What "provider" means here

A provider is: a **name**, an **API protocol**, a **base URL**, an optional
**API key**, and a list of **models** you want to see in the picker. Provider
metadata lives in plain text in `settings.toml`; the API key does not — it goes
into your OS secure storage.

| Settings dropdown label | `api_type` in `settings.toml` | Default base URL |
|---|---|---|
| OpenAI (default) | `open_ai` | `https://api.openai.com/v1/` |
| OpenAI-Response | `open_ai_resp` | `https://api.openai.com/v1/` |
| Anthropic | `anthropic` | `https://api.anthropic.com/v1/` |
| Gemini | `gemini` | `https://generativelanguage.googleapis.com/v1beta/` |
| DeepSeek | `deep_seek` | `https://api.deepseek.com/v1/` |
| Ollama | `ollama` | `http://localhost:11434/` |
| Vertex AI | `vertex` | none — built from `vertex_project` + `vertex_location` |

The OpenAI type is the catch-all for anything speaking OpenAI Chat
Completions: OpenRouter, DeepInfra, Groq, SiliconFlow, Moonshot, Zhipu GLM,
DashScope's OpenAI-compatible endpoint, a local vLLM or llama.cpp server, and so
on. OpenAI-Response is the newer `/v1/responses` API. Pick DeepSeek rather than
OpenAI if you use a DeepSeek *thinking* model (`deepseek-reasoner` and
friends) — the plain OpenAI adapter drops the `reasoning_content` field those
models require on multi-turn requests and the provider returns a 400.
Vertex AI is the one type with no static key: it mints a short-lived GCP bearer
token from your `gcloud` credentials, and its endpoint is built from a GCP
project ID and location instead of a base URL.

### Where credentials are stored

| what | where |
|---|---|
| provider metadata (name, base URL, API type, models, headers, prices) | `settings.toml`, under `agents.warp_agent.providers` |
| **API keys** | OS secure storage, service `dev.phosphor.Phosphor`, key `AgentProviderSecrets` |

On macOS that is the login Keychain; on Linux it is the Secret Service, with an
encrypted-file fallback under the state directory if no Secret Service is
running; on Windows it is DPAPI-protected storage under the state directory.
Keys are never written to `settings.toml`, and they are not synced anywhere.

`settings.toml` itself lives at:

| platform | path |
|---|---|
| Linux | `~/.config/phosphor/settings.toml` |
| macOS | `~/.phosphor/settings.toml` |
| Windows | under `%LOCALAPPDATA%\phosphor\Phosphor\config\` |

If you would rather not guess, run `/open-settings-file` from the command
palette or the agent input — it opens the right file.

**No environment variable supplies a BYOP key.** `ANTHROPIC_API_KEY`,
`OPENAI_API_KEY` and similar are read only by the third-party CLI harnesses
described further down; Phosphor's own agent ignores them.

### Walkthrough: your first agent response

This is the shortest path from a fresh install to a reply. It uses the GUI; the
TUI equivalent is noted at each step.

1. **Open the provider settings.** GUI: *Settings → AI → Providers*. You will
   see `No providers configured yet. Click [+ Add provider] in the top-right to
   add one.`
2. **Add the provider.** Click **+ Add provider**, then fill in:
   - **Name** — anything, e.g. `Anthropic`. It is only a label.
   - **API Type** — pick the protocol from the table above.
   - **Base URL** — pre-filled with the default for the API type; change it for
     a gateway or a local server. It must start with `http://` or `https://`.
   - **API Key** — paste it. The placeholder says
     `sk-... (optional, leave empty for local providers like ollama)`, and it
     means it: an empty key simply sends no `Authorization` header, which is
     what a local Ollama, LM Studio or vLLM wants.
3. **Give it at least one model.** A provider with no enabled models is hidden
   from the model picker entirely, so this step is not optional. Three ways:
   - **+ Add model** and type the model ID — the exact string sent in the API's
     `model` field, e.g. `claude-sonnet-4-5` or `deepseek-chat`.
   - **Fetch from API** — asks the provider for its own model list
     (`GET {base}/models`; Ollama is special-cased to `GET {root}/api/tags`).
     This fills in IDs; only Ollama also fills the context window.
   - **Sync from models.dev** — fills in context window, output limit,
     reasoning/tool-call support and image/PDF/audio capability from the
     public models.dev catalog, for models it recognises.
   There is also a **Quick add** chip row at the top of the page, built from the
   models.dev catalog, which creates a provider pre-filled for a well-known
   service in one click.
4. **Set the context window** if it is still zero. Nothing breaks without it,
   but the context-usage indicator, `/usage` and automatic compaction all need
   it, and the wire inspector will not record without it.
5. **Save.** The page shows a `Saved` toast.
6. **Pick the model.** In the agent input, type `/model` and choose an entry.
   Rows are labelled `provider / model`. A provider with a usable stored key is
   marked — in the GUI with a key icon, in the TUI with the text suffix
   `(key connected)`.
7. **Ask something.** In the GUI press `cmd-enter` (macOS) / `ctrl-shift-enter`
   (Linux, Windows) from the terminal to start a new agent conversation, or
   `cmd-i` / `ctrl-i` to switch the existing input box into agent mode. Type a
   request and press Enter.

**TUI equivalent of steps 1–2 and 6:** run `phosphor-tui`, then `/api-keys` to
add, replace or clear a key for a provider (the entry field is masked; a blank
entry means "no change", not "clear"), and `/model` to choose the model. The TUI
has no provider *creation* UI — a provider must exist first, added in the GUI or
written into `settings.toml` by hand.

### Doing it by hand in `settings.toml`

```toml
[[agents.warp_agent.providers]]
name = "DeepSeek"
api_type = "deep_seek"
base_url = "https://api.deepseek.com/v1"

[[agents.warp_agent.providers.models]]
name = "DeepSeek Chat"
id = "deepseek-chat"
context_window = 65536

  [agents.warp_agent.providers.models.token_price]
  input_usd_per_million_tokens = 0.27
  output_usd_per_million_tokens = 1.10
```

The `id` field is generated for you on first save and is what associates the
provider with its stored key, so let Phosphor write it rather than inventing
one. Add the key afterwards through the GUI's *Settings → AI → Providers* page,
or the TUI's `/api-keys` menu. `/api-keys` is offered in the **GUI** palette too,
but has no GUI handler and does nothing there (issue #628) — in the GUI, use the
settings page.

### Things that will bite you

- **A provider is hidden from the picker** if it is disabled, if *all* of its
  models are disabled, or if it has no endpoint (for Vertex: no GCP project ID).
  A provider with zero models counts as "all models disabled".
- **Plaintext HTTP strips your key.** If the base URL is `http://` and the host
  is not loopback (`localhost`, `127.0.0.0/8`, `::1`), Phosphor drops the
  `Authorization: Bearer` header rather than putting your key on the wire in
  clear. The request still goes out — it just goes out unauthenticated, and the
  provider's 401 is what you will see.
- **`api_type = "ollama"` costs you rules, skills and plan mode.** That API type
  selects a deliberately short system prompt that omits the shared prompt
  footer, so none of them are injected. See the callout under
  [What the agent can see](#what-the-agent-can-see) for the two workarounds.
- **Model IDs are trimmed** — a trailing space used to be sent verbatim and
  rejected upstream.
- **Vertex needs `gcloud`.** There is no service-account-JSON or headless path.
  The settings page has a **Log in with gcloud** button; when the token expires
  the agent tells you to run `gcloud auth login`.
- If a request fails and you want to see exactly what went on the wire, open the
  **wire inspector** from the provider settings. It only records while it has
  been opened, and only when the active model has a non-zero context window.

---

## Talking to the agent

### Getting into agent mode

| action | GUI default | notes |
|---|---|---|
| Start a new agent conversation | `cmd-enter` / `ctrl-shift-enter` | from the terminal, when no command is running |
| Tag the agent into a running command | `cmd-enter` / `ctrl-shift-enter` | same keys, when a long-running command is in the foreground |
| Switch the input box to agent mode | `cmd-i` / `ctrl-i` (`terminal:set_input_mode_agent`) | editable |
| Switch the input box back to the shell | `cmd-i` / `ctrl-i` (`terminal:set_input_mode_terminal`) | editable |
| Toggle auto-approve for this conversation | `cmdorctrl-shift-I` (`terminal:toggle_autoexecute_mode`) | editable |
| Stop / interrupt | `ctrl-c` (`terminal:cancel_command`) | editable; see [Interrupting](#interrupting-the-agent) |

Every binding named above is editable in *Settings → Keyboard shortcuts*, under
the AI group. In the TUI the agent input is the default input — there is no mode
to switch into.

Phosphor can also route a plain-English line typed into the terminal input to
the agent automatically. That is **off by default**
(`agents.warp_agent.input.ai_auto_detection_enabled = false`); the related
in-terminal natural-language detection is on
(`agents.warp_agent.input.nld_in_terminal_enabled = true`).

### Slash commands

Type `/` in the agent input. The ones that matter for the agent itself — the
**surface** column matters, because a command missing from a surface is filtered
out of that surface's palette entirely. §7.7 has the complete split; this table
is the agent-relevant subset.

| command | surface | what it does |
|---|---|---|
| `/agent`, `/new` | both | Start a new conversation |
| `/model` | both | Switch the base agent model |
| `/api-keys` | both, but **TUI only in practice** | Add, view, or clear a provider's API key. Listed in the GUI palette with no handler (issue #628); use *Settings → AI → Providers* in the GUI. |
| `/profile` | both | Switch the active execution profile |
| `/auto-approve` | **TUI only** | Toggle auto-approve |
| `/plan` | **GUI only** | Plan mode |
| `/queue <prompt>` | both | Queue a prompt to send after the agent finishes |
| `/compact` | both | Summarise the conversation to free context |
| `/compact-and <prompt>` | both | Compact, then send this prompt |
| `/fork`, `/fork-from`, `/fork-and-compact` | both | Branch the conversation |
| `/rewind` | both | Rewind to an earlier point |
| `/conversations` | both | Open conversation history |
| `/usage` | both | Context-window usage for this conversation |
| `/cost` | both | Token spend at *your* configured rates |
| `/skills` | both | Invoke a skill |
| `/init` | both | Generate or update an `AGENTS.md` |
| `/add-rule`, `/open-rules`, `/open-project-rules` | **GUI only** | Manage agent rules. `/open-project-rules` opens `<pwd>/WARP.md` specifically, whatever its palette description says — see §6.4. |
| `/mcp` | **TUI only** | View and manage MCP servers |
| `/add-mcp`, `/open-mcp-servers` | **GUI only** | MCP servers |
| `/index` | **GUI only** | Index this codebase — only listed when codebase indexing is on, which it cannot be in a stock build; see [Codebase search](#codebase-search--read-this-before-expecting-it) |
| `/status` | **TUI only** | Session status |
| `/export-to-file`, `/export-to-clipboard` | both | Export the conversation |
| `/open-settings-file` | **GUI only** | Open `settings.toml` |

Availability is contextual: `/model` and `/profile` require the agent view;
`/queue`, `/fork*` and `/rewind` require an active conversation and that the
agent is not currently driving a long-running command; everything AI-related
disappears if `agents.warp_agent.is_any_ai_enabled` is `false`.

---

## Conversations and blocks

### How output appears

An agent turn is rendered as one **block** per exchange: your submitted prompt
on a highlighted row prefixed with `>`, then the agent's response beneath it.
Within a response the agent's work is broken into sections — reasoning, a
to-do list, and one card per tool call (shell command, file edit, grep, MCP
call, question). Tool-call cards are collapsible; set
`agents.warp_agent.appearance.hide_completed_tool_cards = true` to collapse
finished ones automatically.

The tools the agent can call, and therefore the cards you will see, are:
`run_shell_command`, `read_files`, `apply_file_diffs`, `grep`, `file_glob`,
`read_skill`, `ask_user_question`, `websearch`, `webfetch`, MCP tool and
resource calls, document read/edit/create, `write_to_long_running_shell_command`
and `read_shell_command_output`, `transfer_shell_command_control_to_user`, and
computer use (off by default).

### Active vs passive conversations

Phosphor's "Active AI" features can start a conversation *without you asking* —
a suggested prompt after a command fails, a suggested code diff. Internally a
conversation whose every exchange came from one of those triggers, with no query
you typed, is a **passive** conversation. A conversation stops being passive the
moment you send a query into it.

The distinction is visible in one place: **passive-only conversations are hidden
from conversation navigation and history.** They are suggestions, not
conversations you started, so `/conversations` and the history list skip them
until you reply to one. Also hidden for the same reason: conversations that were
started by a CLI subagent and never continued, and child agents spawned by
`/orchestrate` (those live on the parent's status card instead).

Active AI is on by default (`agents.warp_agent.active_ai.enabled = true`) and
each sub-feature has its own toggle — see the settings table.

### Following up

- Just type again. The reply goes into the same conversation.
- If the agent is still working, `/queue <prompt>` (or the auto-queue toggle in
  the working indicator) files your prompt to be sent when it finishes. Queued
  prompts are FIFO and can be reordered, edited and deleted from the queue
  panel.
- `/fork` branches the conversation into a new pane or tab, leaving the original
  intact. `/fork-from` branches from a chosen earlier point;
  `/fork-and-compact` branches and summarises in one step.
- `/rewind` moves the conversation back to an earlier point.
- `/compact` summarises the history to free up context.

### Automatic compaction

On the BYOP path Phosphor compacts a conversation on its own when the token
count reaches the usable part of the model's context window. This is **on by
default** and configured under `agents.byop_compaction.*`:

| key | default | meaning |
|---|---|---|
| `auto` | `true` | compact automatically on overflow |
| `prune` | `true` | clear old tool output instead of deleting messages |
| `tail_turns` | `2` | how many recent turns are kept verbatim |
| `preserve_recent_tokens` | `0` | `0` = auto |
| `reserved` | `0` | `0` = auto (a 20 000-token buffer, capped by the model's max output) |
| `model.provider_id`, `model.model_id` | empty | use a different, cheaper model to write the summary |

Auto-compaction cannot trigger if the model's `context_window` is `0`, because
overflow is undecidable without it — another reason to fill that field in.

---

## What the agent can see

> **Exception, and it is a big one: `api_type = "ollama"` providers.** Phosphor
> picks the system-prompt template by API type before it picks by model, and any
> provider whose `api_type` is `ollama` gets the short `system/local.j2`
> template. That template includes only the environment block — **not** the
> shared prompt footer, so your **rules files, the skills listing and plan mode
> are all silently absent** on an Ollama provider. The short template exists so
> a 9k-token prompt does not swamp a small local model, but the effect on rules
> and skills is real and there is no toggle for it. Two workarounds: point at
> the same Ollama server with `api_type = "open_ai"` and base URL
> `http://localhost:11434/v1/` (Ollama speaks the OpenAI protocol, and that
> route takes a full template), or export the prompt templates and add
> `{% include "partials/footer.j2" %}` to your own `system/local.j2` — see
> §6.6. Everything in the rest of this section assumes a non-Ollama provider.

### Always, on every request

- **Your working directory, git branch and the date**, sent as a standalone
  environment block at the end of the message list. The system prompt tells the
  model to trust that block over anything earlier in the conversation.
- **Your shell and platform**, in the system prompt.
- **Your rules files** (below), and a listing of every in-scope skill's name and
  description, so the model knows what it can pull in.

### Rules files

Phosphor reads three filenames, in this priority order — earlier wins, and only
the highest-priority file in a given directory is used:

1. `WARP.md`
2. `AGENTS.md`
3. `CLAUDE.md`

It looks in the working directory and its ancestors (up to six levels on the
fast synchronous path), and an async pass indexes the repository to a depth of
three. **Global rules** come from `~/.agents/AGENTS.md` and are layered on top of
the project's. Over SSH, the remote host's own `~/.agents/AGENTS.md` is picked
up for that host.

Everything found is rendered into the system prompt under a "Project rules"
heading, described to the model as authoritative project conventions.

There is no on/off switch for these files. `/init` generates or updates an
`AGENTS.md` for the current project; `/open-project-rules` opens it;
`/open-rules` lists everything the agent is currently applying.

A separate, unrelated feature also called "Rules" lives in *Settings → AI →
Knowledge* — those are Library objects you `@`-mention, not files on disk. That
one has a toggle: `agents.knowledge.rules_enabled`, default `true`.

### `@`-mentions

Type `@` at the start of the input, or right after a non-alphanumeric character,
to open the context menu. Available categories:

| category | what it inserts | requires |
|---|---|---|
| Files and folders | the path, as text | repo-wide inside a git repo, otherwise the current folder |
| Blocks | a past terminal command block | — |
| Code | a symbol from the repo outline | a git repo, and `terminal.input.outline_codebase_symbols_for_at_context_menu` (default `true`) |
| Workflows, Notebooks, Plans | a Library object | Library enabled |
| Diff sets | a diff set | a git repo |
| Conversations | an earlier conversation | — |
| Rules | a Library rule object | — |
| Skills | rewrites the `@` into `/skill-name` | — |

Mentioning a file inserts its **path**, not its contents; the agent then reads
it with its own file tools, subject to the read permissions described in
[Permissions and safety](#permissions-and-safety). (Warp's
"Commands" category exists in the source but is not compiled into this build.)

### Skills

A skill is a directory containing a `SKILL.md`, one level below a skills root:
`<root>/<skill-name>/SKILL.md`. Phosphor reads roots from many ecosystems, in
this precedence order, so a repo that already has Claude Code or Codex skills
works unchanged:

`.agents/skills`, `.warp/skills`, `.claude/skills`, `.codex/skills`,
`.cursor/skills`, `.gemini/skills`, `.copilot/skills`, `.factory/skills`,
`.github/skills`, `.opencode/skills`

Each root is looked for both in the directories above your working directory
(project skills) and in `$HOME` (global skills). The one exception is Phosphor's
own root, which lives in the app config directory: **`~/.phosphor/skills`**, not
`~/.warp/skills`.

Eleven skills ship with Phosphor — `agent-add-mcp`, `change-keybinding`,
`claude-api`, `create-skill`, `create-tab-config`, `modify-settings`,
`pr-comments`, `tab-configs`, `tui-settings`, `update-tab-config`, `warpctrl` —
some of which only activate on the surface or with the feature they are about.

Those are the names you invoke. One of them does not match its directory: the
MCP skill lives in `resources/bundled/skills/add-mcp-server/` but its
frontmatter `name:` is `agent-add-mcp`, and frontmatter wins, so `/add-mcp-server`
finds nothing — type `/agent-add-mcp`. (Its body also still tells the agent to
write `~/.warp/.mcp.json`; the real global path is `~/.phosphor/.mcp.json`. See
§6.1.)

To use one: `/skills` opens a picker, `/<skill-name>` invokes it directly, `@`
→ Skills does the same, and `/open-skill` opens its file for editing. There is
also a Skills panel in the left tools panel. You do not have to invoke a skill
explicitly — every request lists the in-scope skills to the model, which can
pull one in with its `read_skill` tool.

The one skills feature that was removed is team/workspace *policy filtering* of
skills, which was delivered over Warp's cloud. Local and project skills are
unaffected.

### Attachments

Attach with the image button in the input, by dragging files onto the window, or
by pasting from the clipboard.

| kind | accepted | limits |
|---|---|---|
| Images | `png`, `jpeg`/`jpg`, `gif`, `webp` | 3.75 MB per image; auto-downscaled above ~1.15 MP or 2000 px; 20 per query, 200 per conversation |
| Text-like files | by MIME or filename, including extensionless `Dockerfile`, `Makefile`, `.env` | inlined into the prompt up to 256 KB; larger files are skipped |
| Other binaries (PDF, audio, …) | any | sent base64 up to 10 MB |

Whether a binary is sent at all depends on the model: image, PDF and audio
support are resolved from the models.dev catalog, with a per-model override you
can force on or off in the provider editor. If the selected model has no vision
support, attached images are dropped and you get a toast saying so.

### Terminal output

In the fullscreen agent view, **completed commands you ran yourself are attached
automatically** to your next query — command, output (truncated), exit code and
timestamps. Commands the *agent* ran are not re-attached this way, since they
are already in the conversation. This is on in every shipped build and there is
no setting to turn it off; if you would rather attach blocks deliberately, use
`@` → Blocks in a surface where auto-attach is not active. Your own
secret-redaction settings still apply to blocks you attach explicitly.

### Codebase search — read this before expecting it

Two different features share the name, and **neither is available in a stock
build**:

- **Embedding-based codebase indexing** (`/index`, "Codebase indexing" in
  *Settings → Code*, auto-indexing of new folders) is gated on a feature flag
  whose only enable path is the environment variable
  `ZAP_UNSTABLE_FEATURES=full_source_code_embedding`. Without it the settings
  rows do not render and nothing is ever indexed. Note also that embeddings are
  **not** computed locally: they are `POST {base}/embeddings` calls against a
  BYOP provider you configure, so turning this on means paying for embeddings —
  and, for a repository on a remote host, shipping your embedding endpoint and
  key to that host. The index is stored in Phosphor's own SQLite database, not a
  separate directory. The setting itself is `code.indexing.agent_mode_codebase_context`
  (default `false`), with `code.indexing.agent_mode_codebase_context_auto_indexing`
  (default `false`) alongside it.
- **The `search_codebase` tool**, a purely local fuzzy search over code *symbol
  names*, is gated on the execution profile's `codebase_context_enabled`, which
  defaults to `false` and has **no setting, no editor control and no TOML path**
  in this build. It is effectively unreachable.

What *does* work by default is the symbol **outline** those features would have
searched: it is built for the repositories you have open, and it is what powers
`@` → Code. Its setting is
`terminal.input.outline_codebase_symbols_for_at_context_menu`, default `true`.

---

## Permissions and safety

This is the part worth reading carefully. The agent runs commands on your
machine.

### Where the policy lives

Permissions belong to an **execution profile**, not to a global switch. You edit
profiles at *Settings → AI → Profiles*, and switch the active profile per
session with `/profile`. Profiles are stored in Phosphor's local object store,
not in `settings.toml`.

The `agents.profiles.*` keys in `settings.toml` are **seed values only**: they
populate the Default profile the first time it is created, and after that the
profile is the source of truth. Editing them later will not change an existing
profile. This trips people up; use the profile editor.

### The knobs, and their defaults

| setting | values | default |
|---|---|---|
| Apply code diffs | Agent decides / Always allow / Always ask | **Agent decides** |
| Read files | Agent decides / Always allow / Always ask | **Agent decides** |
| Execute commands | Agent decides / Always allow / Always ask | **Always ask** |
| Write to a running command's input | Always allow / Always ask / Ask on first write | **Always ask** |
| MCP tools | Agent decides / Always allow / Always ask | **Agent decides** |
| Ask the user questions | Never / Ask except in auto-approve / Always ask | **Ask except in auto-approve** |
| Computer use | Never / Always ask / Always allow | **Never** |
| Web search | on / off | **on** |
| Codebase context (`search_codebase`) | on / off | **off** |
| Command allowlist | list of anchored regexes | empty |
| Command denylist | list of anchored regexes | the built-in list below |
| Directory read allowlist | list of paths | empty |

The built-in **command denylist** on a fresh Default profile, each entry
matching the command and any arguments:

`bash`, `sh`, `zsh`, `fish`, `pwsh`, `eval`, `exec`, `source`, `curl`, `wget`,
`dig`, `nslookup`, `host`, `ssh`, `scp`, `rsync`, `telnet`, `rm`

The built-in **allowlist** — used when "Execute commands" is *Always ask* — is
`cat`, `echo`, `find`, `grep`, `ls`, `which`.

### How a command decision is actually made

For each command the agent wants to run, in this order:

1. **Denylist.** The command string is split into its sub-commands, and each
   sub-command is expanded into every shell-equivalent spelling (quotes removed,
   leading environment assignments stripped, line continuations folded, embedded
   newlines flattened). If any spelling of any sub-command matches the denylist,
   the command is refused and you are asked. *(Exception: when auto-approve is on
   and `auto_approve_bypasses_command_denylist` is left at its default `true`,
   your denylist is skipped at this step. See below.)*
2. **Auto-approve.** If auto-approve is on for this conversation, the command
   runs.
3. **The profile's "Execute commands" setting** decides the rest:
   - **Always allow** → runs.
   - **Always ask** → runs only if *every* sub-command matches the allowlist;
     otherwise you are asked.
   - **Agent decides** → if the model marked the command non-risky, it runs.
     Otherwise: a command containing a redirection is refused; a command where
     every sub-command matches the allowlist runs; a command the model marked
     read-only runs; anything else is refused and you are asked.

So the denylist outranks both the allowlist and "Always allow" — with the one
deliberate exception noted in step 1, described next.

### Auto-approve

Auto-approve ("run to completion", "fast forward") is per-conversation, not a
setting: toggle it with `cmdorctrl-shift-I`, `/auto-approve`, or the control in
the working indicator. While it is on, commands run, files are read and files
are written without asking.

**By default auto-approve also bypasses your command denylist.** That is
`agents.warp_agent.other.auto_approve_bypasses_command_denylist`, default
`true`. If you want the denylist to hold even under auto-approve, set it to
`false`:

```toml
[agents.warp_agent.other]
auto_approve_bypasses_command_denylist = false
```

Auto-approve does **not** silence the agent's questions unless you set "Ask the
user questions" to *Never*; the default only auto-approves execution-type tools.

### Reading and writing files

Reading follows the same three-way setting, plus two extras: a file you have
already approved stays approved for the rest of that conversation, and under
*Always ask* a path under the profile's directory allowlist is read without
asking.

**Be clear about what "Agent decides" means for reads: it always allows.** There
is no model judgement in that branch — the code returns "allowed" unconditionally
and never asks you. Since *Agent decides* is the default for **Read files**, a
stock profile lets the agent read any file it can name without a prompt. If you
want reads gated, set *Always ask* and populate the directory read allowlist.

Writing has a floor no setting can lift: **MCP configuration files are never
auto-written**. `.mcp.json`, `~/.claude.json`, `~/.codex/config.toml` and the
other providers' config paths are matched by both exact path and filename
suffix, before any autonomy check, and always fall through to asking you. The
reason is direct: a config the agent can silently edit is a config through which
the agent can grant itself new tools.

That floor is a lexical path check, and its own doc comment names the residue:
it normalises `~`, `.` and `..` but does **not** resolve symlinks or `$HOME`-style
variable spellings, cannot resolve a cwd-relative path at all (it is handed the
raw strings the model emitted, not the resolved ones), and never sees a *rename
destination* — a V4A edit's `move_to` is not in the path list the guard
receives, while the writer renames onto it. So: a real and useful floor, and
still not a boundary.

### Three honest caveats

- **None of this applies to a third-party harness run.** Everything in this
  section — profile, allowlist, denylist, auto-approve, the protected-write
  floor — governs *Phosphor's own agent only*. When Phosphor drives Claude Code,
  Codex or Gemini CLI, it consults none of it, and it goes further: it launches
  the harness with that vendor's own safety prompts disabled
  (`--dangerously-skip-permissions`, `--dangerously-bypass-approvals-and-sandbox`,
  `--yolo`) and pre-writes trust into the vendor's config files first. **A
  harness run is an unsupervised agent with full permissions in that
  directory.** See
  [How the harness path differs](#how-the-harness-path-differs-from-phosphors-own-agent).
  This is reachable only from `phosphor-oss agent run --harness …` and
  `/orchestrate`, never from an ordinary GUI conversation.
- **The denylist is defence in depth, not a boundary.** Phosphor normalises far
  more spellings than upstream Warp does — quoted command names, escapes,
  leading environment assignments, line continuations, embedded newlines — and
  each of those is covered by a test. It still cannot catch everything that is
  only decidable by running the shell: `$'\x72m'`, brace expansion (`{rm,-rf,~}`),
  redirection glued to the command name (`>/dev/null rm -rf ~`), control-flow
  wrappers (`if true; then rm -rf ~; fi`), command prefixes (`sudo rm`, `env rm`,
  `xargs rm`), variable indirection (`R=rm; $R …`), path spellings (`/bin/rm`),
  and payloads piped into an interpreter. Treat it as a speed bump.
- **Organisation/workspace policy is inert.** The code still has a workspace
  autonomy layer that could impose a denylist an individual user cannot bypass.
  In Phosphor there are no teams, so it never has anything to enforce. Do not
  plan around it.

---

## Cost and usage

Phosphor shows you **context-window occupancy**, not credits and not a bill.
Warp's usage surfaces read a server-computed credit balance and dollar cost;
neither exists here, so the footer answers the question a BYOP session can
actually answer — how much of the model's context this conversation is using.

- The **footer indicator** reads e.g. `18% context`. It is informational: not
  clickable, with no display-mode setting behind it. It hides itself when the
  provider has reported no usage yet.
- **`/usage`** says the same thing in words:
  `Context window: 18% used, 82% remaining — Claude Sonnet (Anthropic), 200,000 token context window`.
  If nothing has been reported yet it tells you why — either "send a message
  first", or, if the model has no configured context window, "set a context
  window for this model in Settings > AI > Agent providers, then send a
  message".
- **`/cost`** is a local estimate, not an invoice. Your provider returns token
  counts and never a price, so `/cost` multiplies those counts by rates **you**
  entered on the provider or the model. With rates configured it reports a
  dollar figure plus the token breakdown; with none it reports the token counts
  and says explicitly that no rate is configured, rather than showing a
  misleading `$0.00`. If some models in the conversation are priced and others
  are not, it gives the partial figure and names the models it excluded. Rates
  are USD per one million tokens, entered as `token_price` on the provider (a
  default for all its models) or on an individual model (which overrides the
  provider's whole table). Token counts are per-session — a conversation
  restored from a previous run starts with none, and `/cost` says so.

There is no credits display, no spend limit, no quota and no billing page.

---

## Third-party CLI agents

Two different things share this name in Phosphor. Keep them apart.

### 1. Running a CLI agent in the terminal (the common case)

When you type `claude`, `codex`, `gemini`, `opencode`, `amp`, `droid`,
`copilot`, `pi`, `auggie`, `agent` (Cursor CLI), `goose`, `deepseek`, `agy`
(Antigravity), `omp`, `hermes` or `vibe` into a Phosphor terminal, Phosphor
recognises it and offers extra affordances around it — a footer toolbar, and a
"Rich Input" composer for multi-line prompts. This is not Phosphor's agent; it
is that tool, running normally, with a nicer input box around it. Settings live
at *Settings → AI → Third party CLI agents* and under `agents.third_party.*`:

| key | default | meaning |
|---|---|---|
| `should_render_cli_agent_toolbar` | `true` | show the footer toolbar for these commands |
| `auto_toggle_composer` | `true` | auto-close/reopen Rich Input as the agent blocks and unblocks |
| `auto_open_composer_on_cli_agent_start` | `false` | open Rich Input when a session starts |
| `auto_dismiss_composer_after_submit` | `false` | close Rich Input after submitting |
| `submit_on_ctrl_enter` | `false` | Rich Input submits on Ctrl+Enter, Enter inserts a newline |
| `cli_agent_toolbar_enabled_commands` | empty | map custom command regexes to a CLI agent |
| `per_agent` | empty | per-agent `toolbar` / `tabmenu` / `titlebar` visibility |

These surfaces are deliberately **not** gated on
`agents.warp_agent.is_any_ai_enabled`. That switch governs Phosphor's own AI;
Claude Code and Codex are programs you installed yourself, and hiding a
quick-launch button for a command you can type anyway would withhold nothing.
Their visibility is controlled only by the per-agent settings above.

Richer status (a working indicator that tracks the agent's turn) needs the
vendor's Phosphor notification plugin: `warpdotdev/claude-code-warp` ≥ 2.0.0 for
Claude Code, `warpdotdev/codex-warp` ≥ 0.4.0 for Codex,
`warpdotdev/gemini-cli-warp` ≥ 1.0.0 for Gemini CLI. Claude's installs
automatically; Codex's is behind a build feature that is on by default; Gemini's
needs an opt-in flag and is effectively off.

### 2. Driving a CLI agent *as* Phosphor's harness

Phosphor can also hand a task to Claude Code, Codex or Gemini CLI as the
execution engine for an agent run — the "harness". This path is real, but it is
narrower than you might expect, so here is what is actually reachable.

**From the CLI:**

```
phosphor-oss agent run --harness claude -p "add a regression test for the parser"
```

`--harness` accepts `oz` (the default — Phosphor's own agent), `claude` (alias
`claude-code`), `gemini` and `codex`. `opencode` parses but is rejected at
runtime. The flag is **hidden from `--help`**, which is why you will not find it
by exploring. Useful companions: `-C/--cwd`, `-n/--name`, `--profile`,
`--skill`, `--mcp`, `--idle-on-complete`. The run opens a real Phosphor terminal
session and executes the harness CLI in it.

**From `/orchestrate`:** `/orchestrate <task>; <task>` spawns local child agents.
This path always uses **Claude Code** — there is no way to choose another
harness from it. It requires bash, zsh or fish (PowerShell is rejected). Of the
other harnesses: **Gemini** and Phosphor's own **`oz`** are rejected outright by
the child-harness parser; **Codex** parses but is gated behind a flag with no
enable path in shipped builds, so a launch fails on its precondition; and
**OpenCode** parses and is not rejected, but has no launch implementation and
nothing can select it, because `/orchestrate` hardcodes `claude` and takes no
flags at all. In practice Claude Code is the only child harness.

**There is no GUI harness picker.** A harness-selector menu exists in the source
but is never rendered, so in the GUI every agent run uses Phosphor's own agent.

#### What each harness needs

| harness | executable on `PATH` | authenticated by | command Phosphor runs |
|---|---|---|---|
| Claude Code | `claude` | your existing Claude Code login, or `ANTHROPIC_API_KEY` in the environment | `claude --session-id <uuid> --dangerously-skip-permissions [--append-system-prompt-file …] [--mcp-config …] < <prompt file>` |
| Codex | `codex` | `~/.codex/auth.json`, seeded from `OPENAI_API_KEY` if present | `codex --dangerously-bypass-approvals-and-sandbox --dangerously-bypass-hook-trust "$(cat <prompt file>)"` |
| Gemini CLI | `gemini` | an API key — Phosphor **rewrites** `~/.gemini/settings.json` to `security.auth.selectedType = "gemini-api-key"` before every run | `gemini --yolo -i "$(cat <prompt file>)"` |

If the executable is missing, Phosphor tells you and links the vendor's install
docs. There is no minimum version requirement on the CLIs themselves.

**The Gemini row is not a passive read of your config — it is a write.** Every
Gemini harness run sets `selectedType` to `gemini-api-key` in your own
`~/.gemini/settings.json`, so if you had signed the Gemini CLI in with a Google
account, a Phosphor harness run switches it to API-key mode and leaves it that
way for your own later `gemini` invocations. The value is never restored.

#### How the harness path differs from Phosphor's own agent

This is the important part, and it is not a small difference.

- **It does not use your BYOP providers or keys.** The harness authenticates
  itself, with its own credentials. `AgentProviderSecrets` is never read on this
  path. Configure the CLI as its vendor documents.
- **Your Phosphor permission profile does not apply.** None of the
  allowlist/denylist/auto-approve machinery above is consulted. Worse, Phosphor
  *actively disables the harness's own approval prompts* so the run is
  non-interactive: it passes `--dangerously-skip-permissions` (Claude),
  `--dangerously-bypass-approvals-and-sandbox` (Codex) or `--yolo` (Gemini), and
  it pre-writes trust into the harness's config — `hasTrustDialogAccepted` and
  `skipDangerousModePermissionPrompt` for Claude, `trust_level = "trusted"` for
  Codex (for the working directory *and every child git repository*),
  `trustedFolders` for Gemini. **A harness run is an unsupervised agent with
  full permissions in that directory.** Use it deliberately.
- **MCP servers are translated per harness.** Claude gets them as a
  `--mcp-config` file, Codex gets them written into `config.toml`, and Gemini
  **ignores them entirely**.
- **Failure detection is by scraping output.** Phosphor watches the block's text
  for a small list of known error strings from Claude and Codex and ends the run
  when it sees one. Gemini has no such list.
- **Transcripts are not exported.** Codex's rollout file is located and then
  deliberately not uploaded; Claude's transcript reader is present but unused.
  Warp's hosted transcript/artifact upload was cut with the rest of the cloud.
- **Environment.** Phosphor exports `OZ_CLI` and `OZ_HARNESS` into the harness
  process, plus `OZ_RUN_ID` / `OZ_PARENT_RUN_ID` for `/orchestrate` children
  (not for `agent run`), and `ANTHROPIC_MODEL` for Claude. Nothing in Phosphor
  reads `OZ_HARNESS`; it is there for your own hooks.

---

## Interrupting the agent

`ctrl-c` is the stop key everywhere (binding `terminal:cancel_command`,
editable). What it does depends on what is running.

**Phosphor's own agent, GUI.** `ctrl-c` — or the Stop button on the agent block,
which displays this same keystroke — cancels the in-flight model request, or the
pending/running tool action, or, if a subagent is running with neither of those,
the whole conversation's progress. If the agent is mid-summarisation you get a
confirmation first; a second `ctrl-c` confirms. If the agent had taken control
of a long-running command, `ctrl-c` first hands control back to you (the
conversation stays alive and resumes when the command finishes); a second
`ctrl-c` stops the conversation and forwards `0x03` to the command.

**Phosphor's own agent, TUI.** `ctrl-c` cancels a running conversation; if there
is nothing to cancel it clears the input. Either way it arms a one-second window
and shows `ctrl-c again to exit` in the footer — a second `ctrl-c` within that
second quits the TUI. When an orchestration child is selected, `ctrl-c` kills
that child instead (immediately from the tab bar, or first-press-arms /
second-press-kills when you are viewing the child).

**A third-party harness — read this before relying on it.** When Phosphor is
driving Claude Code, Codex or Gemini CLI, `ctrl-c` forwards the raw `0x03` byte
to that CLI's PTY. Whatever the CLI does with an interrupt is what happens.
Phosphor sends no signal of its own, does not tear the process down, and does
**not** mark the run cancelled.

There is code in the tree for exactly that missing behaviour — a feature that
watches for a `ctrl-c` write and, after a two-second grace period with no
further activity from the agent, resolves the session and its task to
`Cancelled`. **It cannot fire in any shipped build**, for two independent
reasons: the function that observes the keystroke has no production caller (its
upstream caller was a shared-session viewer input path this fork does not have),
and the feature flag gating it has no enable path in any binary this repository
builds — it is not a release flag, not a cargo feature, and not in the
`ZAP_UNSTABLE_FEATURES` list. This is recorded as known in `TODO.md`. Do not
expect a "Cancelled" status after interrupting a harness run; the honest
expectation is that the CLI handles the interrupt and Phosphor's status stays
whatever it was.

(One live route to a `Cancelled` status does exist: an agent's notification
plugin can report a cancelled turn over OSC 777, and Phosphor renders that as a
neutral stop rather than an error. Whether the published Claude Code and Codex
plugins actually send it on user interrupt is a property of those plugins, not
of Phosphor, and is not verifiable from this repository.)

---

## Reference

### Settings

All paths are keys in `settings.toml`. Open it with `/open-settings-file`.

#### Master switches

| TOML path | type | default | what it does |
|---|---|---|---|
| `agents.warp_agent.is_any_ai_enabled` | bool | `true` | Turns every *Phosphor* AI feature on or off. When `false`, the AI settings page greys out and AI slash commands disappear. It deliberately does **not** disable the third-party CLI agent surfaces — see [Third-party CLI agents](#third-party-cli-agents). |
| `agents.warp_agent.active_ai.enabled` | bool | `true` | Active AI (proactive suggestions) as a group |

#### Providers and models

| TOML path | type | default | what it does |
|---|---|---|---|
| `agents.warp_agent.providers` | list of providers | empty | Your BYOP providers. Keys are **not** stored here. |
| `agents.warp_agent.catalog_provider_visibility_overrides` | list of strings | empty | Which models.dev quick-add chips are shown or hidden |
| `agents.byop.last_used_model_id` | string | empty | Remembers the model you last picked |
| `agents.byop.last_used_reasoning` | table | empty | Remembers reasoning effort per (API type, model) |
| `agents.warp_agent.prompt_template_dir` | string | empty | Directory of overriding system-prompt templates; empty uses the built-ins. Overridden by the `ZAP_PROMPT_DIR` environment variable. Suggested location `~/.phosphor/prompts`. |

Per-provider fields: `name`, `api_type`, `base_url`, `models`, `extra_headers`,
`vertex_project`, `vertex_location`, `disabled`, `token_price`.
Per-model fields: `name`, `id`, `context_window` (0 = unknown),
`max_output_tokens` (0 = unspecified), `reasoning` (default `false`),
`tool_call` (default `true`), `image`/`pdf`/`audio` (unset = auto-detect),
`disabled`, `token_price`.
`token_price` fields: `input_usd_per_million_tokens`,
`output_usd_per_million_tokens`, `cache_read_usd_per_million_tokens`,
`cache_write_usd_per_million_tokens`.

#### Automatic compaction

| TOML path | type | default |
|---|---|---|
| `agents.byop_compaction.auto` | bool | `true` |
| `agents.byop_compaction.prune` | bool | `true` |
| `agents.byop_compaction.tail_turns` | u32 | `2` |
| `agents.byop_compaction.preserve_recent_tokens` | u32 | `0` (auto) |
| `agents.byop_compaction.reserved` | u32 | `0` (auto) |
| `agents.byop_compaction.model.provider_id` | string | empty |
| `agents.byop_compaction.model.model_id` | string | empty |

#### Permissions (seed values for the Default profile only)

| TOML path | type | default |
|---|---|---|
| `agents.profiles.agent_mode_command_execution_allowlist` | list of regexes | `cat`, `echo`, `find`, `grep`, `ls`, `which` |
| `agents.profiles.agent_mode_command_execution_denylist` | list of regexes | the shell/network/`rm` list above |
| `agents.profiles.agent_mode_execute_readonly_commands` | bool | `false` |
| `agents.profiles.agent_mode_coding_permissions` | enum | `AlwaysAskBeforeReading` |
| `agents.profiles.agent_mode_coding_file_read_allowlist` | list of paths | empty |
| `agents.warp_agent.other.auto_approve_bypasses_command_denylist` | bool | `true` |

#### Input and active AI

| TOML path | type | default |
|---|---|---|
| `agents.warp_agent.input.ai_auto_detection_enabled` | bool | `false` |
| `agents.warp_agent.input.nld_in_terminal_enabled` | bool | `true` |
| `agents.warp_agent.input.ai_command_denylist` | string | empty |
| `agents.warp_agent.input.include_agent_commands_in_history` | bool | `false` |
| `agents.warp_agent.active_ai.intelligent_autosuggestions_enabled` | bool | `true` |
| `agents.warp_agent.active_ai.agent_mode_query_suggestions_enabled` | bool | `true` |
| `agents.warp_agent.active_ai.code_suggestions_enabled` | bool | `true` |
| `agents.warp_agent.active_ai.natural_language_autosuggestions_enabled` | bool | `true` |
| `agents.warp_agent.active_ai.git_operations_autogen_enabled` | bool | `true` |
| `agents.warp_agent.active_ai.rule_suggestions_enabled` | bool | `true` |

#### Presentation and knowledge

| TOML path | type | default |
|---|---|---|
| `agents.warp_agent.other.show_conversation_history` | bool | `true` |
| `agents.warp_agent.other.show_agent_notifications` | bool | `true` |
| `agents.warp_agent.other.should_render_use_agent_toolbar_for_user_commands` | bool | `true` |
| `agents.warp_agent.appearance.hide_completed_tool_cards` | bool | `false` |
| `agents.knowledge.rules_enabled` | bool | `true` |
| `agents.knowledge.warp_drive_context_enabled` | bool | `true` |
| `agents.mcp_servers.file_based_mcp_enabled` | bool | `false` |
| `agents.statusline` | table | default statusline |

#### Third-party CLI agents

| TOML path | type | default |
|---|---|---|
| `agents.third_party.should_render_cli_agent_toolbar` | bool | `true` |
| `agents.third_party.auto_toggle_composer` | bool | `true` |
| `agents.third_party.auto_open_composer_on_cli_agent_start` | bool | `false` |
| `agents.third_party.auto_dismiss_composer_after_submit` | bool | `false` |
| `agents.third_party.submit_on_ctrl_enter` | bool | `false` |
| `agents.third_party.cli_agent_toolbar_enabled_commands` | table | empty |
| `agents.third_party.per_agent` | table | empty |

#### Context sources living outside the `agents.` tree

| TOML path | type | default |
|---|---|---|
| `terminal.input.outline_codebase_symbols_for_at_context_menu` | bool | `true` |
| `code.indexing.agent_mode_codebase_context` | bool | `false` (and the UI is hidden without `ZAP_UNSTABLE_FEATURES=full_source_code_embedding`) |
| `code.indexing.agent_mode_codebase_context_auto_indexing` | bool | `false` (same gate) |

### Environment variables

| variable | read by | effect |
|---|---|---|
| `EXA_API_KEY` | Phosphor's `websearch` tool | Uses your Exa account instead of the anonymous public endpoint |
| `ZAP_PROMPT_DIR` | prompt renderer | Overrides `agents.warp_agent.prompt_template_dir` |
| `ZAP_BYOP_LOG_FULL_REQUEST` | BYOP request logging | Logs full request bodies rather than counts and digests |
| `ZAP_UNSTABLE_FEATURES` | startup | Comma-separated opt-in feature list, or `all` |
| `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` | the Claude Code / Codex **harnesses only** | Not read by Phosphor's own agent |

### Command-line

`phosphor-oss` accepts the agent CLI subcommands. Both the usage lines and the
examples in `--help` are built from argv\[0\], so they say `phosphor-oss` (clap
overwrites the internal `oz` command name with the invoked name; verified against
the built binary via a renamed symlink). What *is* stale is a handful of help
*strings*: `--model`'s help says "Use `warp model list`" and `completions`
shows `path/to/warp completions bash`. Read those as `phosphor-oss`.

| command | what it does |
|---|---|
| `phosphor-oss agent run -p "<prompt>"` | Run a task with Phosphor's own agent |
| `phosphor-oss agent run --harness claude -p "…"` | Run it under Claude Code instead (hidden flag; also `gemini`, `codex`) |
| `phosphor-oss agent list` | List available agents |
| `phosphor-oss agent profile …` | Manage agent profiles |
| `phosphor-oss agent message send/list` | Local on-disk mailbox used by `/orchestrate` children |
| `phosphor-oss model list` | Print every model ID the picker offers (BYOP entries look like `byop:<provider-uuid>:<model-id>`) |

There is a `provider` subcommand in the parser, but it is **not** about AI
providers — it is a Linear/Slack integration stub. It is hidden from `--help`
and disabled in shipped builds, so `phosphor-oss provider …` exits with
`error: unrecognized subcommand 'provider'`. Provider setup happens in the
settings page (GUI) or via `/api-keys` (TUI only in practice — see §7.13), never
here.

`phosphor-tui` accepts `--resume <id>`, `--auto-approve`, `--api-key`
(`WARP_API_KEY`; the app's own auth flag, not a provider key), and `--set-provider-api-key` / `--clear-provider-api-key`.
Those last two take one of four provider slugs — `openai`, `anthropic`,
`google`, `grok` — and **all four are now refused** (issue #629). They used to
write the **legacy fixed key store** (`AiApiKeys` in secure storage, a different
keyring entry from BYOP's `AgentProviderSecrets`), which the BYOP send path
never reads, so `--set-provider-api-key anthropic` reported success and left the
agent with no usable key. They now fail with a message pointing at `/api-keys`
or the Providers settings page — the two surfaces that store a key the agent
will actually send. Repointing the flags was not an option: `AgentProviderSecrets`
is keyed by the UUID of a provider entry you defined, so a fixed slug like
`anthropic` has nothing to write to.

---

## Not available in Phosphor

Each of these exists in Warp and is deliberately absent here. Where a decision
was recorded, `DECLINED.md` has the reasoning.

- **Cloud agents and hosted runners.** There is no server to run an agent on.
  The "RunAgents"/host-picker orchestration UI needs Warp's GraphQL runner API
  and the `CloudAgentRunners` flag; it was not ported. Local child agents via
  `/orchestrate` are the substitute, and they run as ordinary processes on your
  machine.
- **Ambient agents.** The spawn API is present but returns
  `Agent spawning is disabled in Phosphor` on every call. Ambient agents needed
  a server to host the run and a shared session to watch it.
- **Agent session sharing.** `--share` still parses (hidden, so old scripts do
  not break) and does nothing; sharing needed Warp's backend to host the session
  and resolve `team:` / `public:` / `user@host` recipients.
- **Teams, organisations and org policy.** `has_teams()` is permanently `false`
  and `current_team()` always returns `None`. The workspace-level agent denylist,
  sandboxed-agent policy and agent attribution settings therefore have nothing
  to enforce and no UI.
- **Anything `warp-server`.** No account, no login, no sync, no cloud
  preferences, no hosted transcript or artifact upload, no Warp Drive
  (the local Library remains).
- **Credits, billing, spend limits and usage quotas.** There is no billing
  relationship to report on. The credits widget in the AI settings page is
  compiled but never rendered. `/usage` reports context-window occupancy and
  `/cost` reports an estimate from rates you supply — see
  [Cost and usage](#cost-and-usage). The provider-cost baseline that Warp
  restores with a conversation, and its OpenAI long-context pricing warning, are
  both declined for the same reason.
- **Agent commit/PR attribution** (`Co-Authored-By`). The toggle was removed
  because the decision was made server-side; nothing local ever emitted the
  line.
- **`/logout`.** BYOP has no account to log out of, so the command is not
  registered rather than registered as a no-op.
- **`/voice` and voice input for the agent.** Audio capture works; the
  transcription backend was Warp's cloud Wispr service and is disabled. The BYOP
  protocol cannot carry audio.
- **xAI / Grok subscription OAuth.** Phosphor takes API-key credentials only. An
  xAI API key works — add it as an ordinary provider.
- **Multi-agent orchestration beyond local children.** Cloud-runner children,
  credit rollup, the orchestration topology view and the child-agent cycling
  keybindings are out. `/orchestrate` with local Claude Code children is in.
- **Screen recording and computer-use session recording.** Not cloud — just not
  shipped. Computer *use* itself works and is off by default.
- **The `InitProject` first-run wizard.** Superseded by `/init`, which generates
  or updates an `AGENTS.md` and works in both the GUI and the TUI.
- **A GUI harness picker.** Choosing Claude Code or Codex as the agent harness
  is CLI-only (`agent run --harness`) or `/orchestrate` (Claude only).
- **A working Ctrl-C cancel for third-party harnesses.** See
  [Interrupting the agent](#interrupting-the-agent) — the keystroke reaches the
  harness, but Phosphor synthesises no cancellation of its own, and the code
  that would has no reachable entry point in any shipped build.

Two more that are *not* declined but are unreachable in a stock build, so do not
plan around them: embedding-based **codebase indexing** (needs
`ZAP_UNSTABLE_FEATURES=full_source_code_embedding`) and the **`search_codebase`
tool** (per-profile flag with no way to set it).

### Names you may notice are stale

- The TUI's cargo target is `zap-tui-oss`; the binary you download and run is
  `phosphor-tui`.
- `phosphor-oss --help` shows `phosphor-oss` in its usage lines (clap takes the
  name from argv\[0\], not from the internal `oz` command name), but two help
  *strings* were never rebranded: `--model`'s help refers to `warp model list`,
  and `completions`' help shows `path/to/warp completions bash`.
- Project skills are still read from `.warp/skills`, and files an agent-SDK
  session downloads land in `<working dir>/.warp/attachments`. The home skills
  directory, by contrast, is `~/.phosphor/skills`.

<!-- SOURCES
Orientation / binaries / identity
- app/Cargo.toml:25-28 (bin phosphor-oss), crates/warp_tui/Cargo.toml:9,14-16 (bin zap-tui-oss, autobins=false)
- .github/workflows/phosphor_release.yml:408-434,808-832,890-911 (zap-tui-oss renamed to phosphor-tui on release)
- crates/warp_tui/src/bin/oss.rs:16-45 (Channel::Oss, AppId dev.phosphor.Phosphor, shared config/secrets with GUI)
- app/src/bin/phosphor_oss.rs:30,38 (same app id; phosphor.log)
- crates/warp_core/src/paths.rs:37-46 (.phosphor for Oss channel), :146-158 (config_local_dir), :298-342 (project_dirs, linux name "phosphor"), :71-73 (warp_home_skills_dir), :75-86 (warp_home_prompts_dir), :174-184 (state_dir)
- app/src/settings/mod.rs:648-654 (settings.toml = config_local_dir()/settings.toml)
- README.md:210-216 (app id / binary / config path rename table), :227 (API keys cannot be copied)

Providers / BYOP
- app/src/settings/ai.rs:1012-1040 (AgentProviderApiType variants + docs), :1112-1122 (dropdown labels), :1151-1164 (default_base_url), :1010 (serde snake_case), :992-996 (AgentProviderKind)
- app/src/settings/ai.rs:1184-1246 (AgentProvider fields), :1483-1543 (AgentProviderModel), :1266-1286 + :1259-1265 (TokenPrice + TOML example), :1359-1375 (resolved_base_url / has_endpoint), :1415-1428 (is_usable / effectively_disabled), :1437-1463 (vertex endpoint + model family), :1560-1565 (model id trimmed)
- app/src/settings/ai.rs:2599-2607 (agents.warp_agent.providers, SyncToCloud::Never), :2598 (api_key not persisted here)
- app/src/ai/agent_providers/secrets.rs:1-13 (keyring key "AgentProviderSecrets"), :98-116 (JSON map provider_id -> key)
- app/src/lib.rs:1317-1330 (secure storage registration per platform)
- crates/warp_core/src/channel/state.rs:128-133 + crates/warp_core/src/app_id.rs:78-86 (keyring service = app id)
- app/src/ai/agent_providers/mod.rs:57-77 (bearer stripped on non-loopback http://), :159-163 and :303-310 (empty key = no auth header), :207-208 (picker label), :234-253 (empty-picker fallback), :289-312 (lookup_byop), :320-331 (active_context_window)
- app/src/ai/agent_providers/openai_compatible.rs:81-94 (normalize_base_url), :102+ (GET {base}/models), :172+ (Ollama /api/tags), :53-68,:111-120,:184-187 (strip-and-continue)
- app/src/ai/agent_providers/models_dev.rs:33-36 (URL, cache file, 24h TTL, 15s timeout), :385-410 (into_agent_provider_model; token_price never filled), :431-443 (infer_api_type), :365-376 (quick-add visibility)
- app/src/ai/agent_providers/vertex_auth.rs:24-52 (token TTL, gcloud auth login hint, debounce), :14-16 (no service-account path)
- app/i18n/en/warp.ftl:1622-1717 (provider page strings incl. empty state, api-key placeholder, Fetch from API, Sync from models.dev, Quick add, Saved toast, vertex login)
- app/src/ai/agent_providers/wire_inspector.rs:1-7 and wire_log.rs:10-20 (inspector arms only when opened; needs non-zero context window)
- app/src/ai/llms.rs:697-728 (model precedence: per-view override, byop_last_used_model_id, profile base_model, default)
- DECLINED.md:96 (xAI/Grok subscription OAuth declined; API keys only)
- crates/ai/src/llm_provider.rs:43-60 (LLMProvider; Xai excluded from pasted-key support), :29-41 (module doc: as of #629 no provider reaches the AiApiKeys store)
- crates/warp_tui/src/session.rs:50 (CLI_NAME "phosphor-tui"), :55-92 (TuiArgs), reject_provider_api_key_flags (both key flags refused for every provider, #629)
- crates/ai/src/api_keys.rs:7,21-26 (keyring key "AiApiKeys"; fixed four providers), :121-181 (api_keys_for_request)
- app/src/ai/agent_providers/chat_stream.rs (no api_keys reference; BYOP path never reads ApiKeyManager) and app/src/ai/agent/api.rs:508-511 (feeds RequestParams::api_keys, which nothing reads and which is not serialized)
- app/src/ai/llms.rs:26-42 (is_using_api_key_for_provider does read AiApiKeys, from six call sites — but only for OpenAI/Anthropic/Google, and every LLMInfo this fork builds carries LLMProvider::Unknown, so it always returns false)

Slash commands / keybindings
- app/src/search/slash_command_menu/static_commands/commands.rs:12-625 (all commands), :304-311 (/model), :313-324 (/api-keys, fork-native, AI_ENABLED), :326-333 (/profile), :379-388 (/queue), :599-625 (/usage, /cost with rationale)
- app/src/search/slash_command_menu/static_commands/mod.rs:27-47 (Availability bits)
- app/i18n/en/warp.ftl:2642-2705 (slash-command descriptions)
- app/src/terminal/view/init.rs:1126-1170 (cmd-enter / ctrl-shift-enter StartNewAgentConversation + SetInputModeAgent; cmd-i/ctrl-i pair), :385-395 (terminal:cancel_command = ctrl-c), :999-1006 (terminal:toggle_autoexecute_mode = cmdorctrl-shift-I, gated on FastForwardAutoexecuteButton), :45-49 (binding name constants)
- app/Cargo.toml:480-661 (default features; includes agent_mode, agent_view, agent_harness, fast_forward_autoexecute_button, agent_decides_command_execution, codex_plugin, list_skills, bundled_skills, agent_view_block_context, ai_context_menu_code, drive_objects_as_context, diff_set_as_context, conversations_as_context)
- app/src/lib.rs:2926-3350 (cargo-feature -> FeatureFlag mapping), :3366-3439 (UNSTABLE_FEATURES / ZAP_UNSTABLE_FEATURES)

Conversations / blocks
- crates/warp_tui/src/agent_block.rs:1-6 (one exchange per block), agent_block_sections.rs:28-35 ("> " input prefix, section renderers)
- crates/ai/src/agent/action/mod.rs:41-166 (agent action types)
- app/src/ai/agent_providers/tools/*.rs (tool names: shell.rs:120 run_shell_command, files.rs:153 read_files, edit.rs:259 apply_file_diffs, search.rs:83 grep, :238 file_glob, ask.rs:256 ask_user_question, skill.rs:88 read_skill, long_shell.rs:149,236, markers.rs:49,126, documents.rs:127,216,323, websearch.rs:21, webfetch.rs)
- app/src/ai/agent/conversation.rs:1297-1330 (is_entirely_passive / is_single_passive_exchange), :1352-1367 (should_exclude_from_navigation)
- app/src/ai/blocklist/history_model.rs:1072-1076 (is_entirely_passive_conversation)
- app/src/ai/agent_providers/active_ai/mod.rs:1-22 (what Active AI does; silently no-ops without a BYOP model)
- app/src/ai/blocklist/queued_query.rs:25-50 (queued prompt origins, FIFO)
- app/src/ai/byop_compaction/mod.rs:1-33 (compaction entry points and constants), overflow.rs:1-40 (is_overflow needs a non-zero context window)
- app/src/settings/ai.rs:2628-2715 (agents.byop_compaction.* defaults)

Permissions
- app/src/ai/blocklist/permissions.rs:886-1010 (can_autoexecute_command decision order), :676-770 (can_read_files / can_write_files), :1263-1310 (protected write paths = MCP configs), :1389-1560 (denylist_match_candidates: handled and explicitly-not-handled bypasses), :341-456 (profile/workspace resolution; workspace denylist merges, never replaces), :236-247 (workspace_autonomy_settings)
- app/src/ai/execution_profiles/mod.rs:341-398 (AIExecutionProfile fields), :400-430 (defaults incl. execute_commands AlwaysAsk, computer_use Never, web_search true, codebase_context false), :33-146 (ActionPermission / WriteToPtyPermission / ComputerUsePermission / AskUserQuestionPermission variants + descriptions), :463-484 (create_default_from_legacy_settings: settings.toml seeds the Default profile), :519-570 (create_default_cli_profile is fully permissive)
- app/src/settings/ai.rs:935-978 (DEFAULT_COMMAND_EXECUTION_ALLOWLIST / DENYLIST contents), :824-835 (AgentModeCodingPermissionsType), :838-853 (anchored-regex predicate), :2048-2113 (agents.profiles.* seeds), :2508-2516 (auto_approve_bypasses_command_denylist default true)
- app/src/ai/agent/conversation.rs:4771-4778 (AIConversationAutoexecuteMode RunToCompletion = auto-approve)
- app/src/ai/execution_profiles/profiles.rs:116-146 (profiles live in the local object store, not settings.toml)
- app/src/workspaces/user_workspaces.rs:376-378 (is_byo_api_key_enabled always true), :428-460 (org autonomy settings come from current_team), :533 (has_teams false)
- DECLINED.md:81-84 (cloud teams / org policy inert; UserWorkspaces::current_team() returns None)

Cost / usage
- crates/warp_tui/src/usage.rs:1-36 (BYOP substitution rationale; "{pct}% context"; informational only)
- app/src/ai/usage_cost.rs:1-31 (module rationale), :133-190 (/usage report + the two "nothing reported yet" hints), :202-300 (/cost: provider rates, unpriced-model call-out, cache caveat)
- app/src/settings_view/ai_page.rs:1667,1685,1759 (UsageWidget only when !is_byo_api_key_enabled, i.e. never), :4828-4870 (the credits widget it would have rendered)
- DECLINED.md:215 (provider-cost baselines declined; cites crates/warp_tui/src/usage.rs:1-12), :216 (OpenAI long-context pricing warning declined)

Context
- crates/ai/src/project_context/model.rs:12-25 (RULES_FILE_PATTERN order WARP.md > AGENTS.md > CLAUDE.md), :41-42,:62-63,:1009-1060 (search depth/budget), :947-955 (global rules layered)
- crates/ai/src/project_context/global_rules.rs:1-58 (~/.agents/AGENTS.md)
- app/src/ai/blocklist/controller/input_context.rs:60-76 (per-SSH-host global rules), :83-120 (system prompt: shell/platform + skills listing)
- app/src/ai/agent_providers/prompts/partials/project_rules.j2, env.j2, skills.j2
- app/src/ai/project_rules_persister.rs:1-92 (project_rules table, repo scan)
- app/src/search/ai_context_menu/view.rs:98-144 (categories), :386-520 (availability), search.rs:1-17 (menu close rules)
- app/src/terminal/input.rs:9810-9840 (a file mention inserts its path), :9915-9918 (skill mention rewrites to /name), :10649-10705 (@ trigger rules)
- app/Cargo.toml:852 (ai_context_menu_commands exists but is not in default)
- crates/ai/src/skills/read_skills.rs:54-58,110-111 (<root>/<name>/SKILL.md, direct children only)
- crates/ai/src/skills/skill_provider.rs:100-169 (roots and precedence; Phosphor root = warp_home_skills_dir)
- app/src/ai/skills/skill_manager.rs:126-160 (project + home scope)
- app/src/ai/skills/bundled.rs:38-68,390-396 (bundled skills + activation conditions); resources/bundled/skills/ (the 11 directories)
- app/src/skill_manager/panel.rs:47-70 and app/src/workspace/view/left_panel.rs:245-255 (Skills panel)
- app/src/util/image.rs:14,41-59 (image MIME list, 3.75 MB cap, downscale thresholds), :47 (20 per query)
- app/src/editor/view/mod.rs:147 (200 images per conversation), :5076-5117 (picker, vision check), :5090-5102 (image/non-image split), :8163-8168 (attach button)
- app/src/terminal/input.rs:9926-9950 (drag and drop), :9969-10015 (paste), :10216-10262 (over-limit toast)
- app/src/ai/blocklist/context_model.rs:70-80,241,272,296 (256 KB text inline, 10 MB binary), :467-541,:894-901,:961-972 (auto-attached user blocks; directory context always sent), :487-491 (truncation and redaction)
- app/src/ai/agent_providers/attachment_caps.rs:26-90 (image/pdf/audio capability resolution)
- app/src/ai/block_context.rs:12-58 (what an attached block carries)
- app/src/ai/agent_providers/user_context.rs:206-259 and chat_stream.rs:2621-2682 (environment block)
- app/src/settings/input.rs:145-153 (terminal.input.outline_codebase_symbols_for_at_context_menu default true)
- app/src/settings/code.rs:85-105 (code.indexing.* defaults false)
- app/src/settings_view/code_page.rs:1504-1591 (all indexing rows gated on FeatureFlag::FullSourceCodeEmbedding)
- app/src/ai/codebase_auto_indexing.rs:7-21,53-82 (same flag gates indexing; the false default is the consent mechanism)
- crates/warp_features/src/lib.rs:861-866 (FullSourceCodeEmbedding / CodebaseIndexPersistence enable path is ZAP_UNSTABLE_FEATURES)
- app/src/lib.rs:3420-3428 (those two entries in UNSTABLE_FEATURES)
- app/src/ai/agent_providers/embeddings.rs:1-31,73-80 (embeddings are POST {base}/embeddings against a BYOP provider)
- app/src/ai/codebase_embeddings.rs:1-16 (index stored in the app SQLite DB), :698 (SSH host receives the endpoint + key)
- app/src/ai/agent_providers/tools/codebase.rs:21-60 and codebase_runtime.rs:24-26,85-252 (search_codebase is local symbol-name fuzzy search)
- app/src/ai/execution_profiles/mod.rs:388-393,427 (codebase_context_enabled default false); no set_codebase_context_enabled anywhere; app/src/ai/execution_profiles/editor/ contains no codebase control (contrast web_search at editor/ui_helpers.rs:877, editor/mod.rs:1656)
- app/src/ai/agent_providers/tools/web_runtime.rs:18 and chat_stream.rs:7721 (Exa anonymous endpoint; EXA_API_KEY)
- app/src/ai/attachment_utils.rs:1-11 (<cwd>/.warp/attachments, agent-SDK downloads only)

Third-party CLI agents (terminal detection)
- app/src/terminal/cli_agent.rs:174-250 (CLIAgent variants and command prefixes; PhosphorTui prefixes list zap-tui-oss, not phosphor-tui)
- app/src/settings/ai.rs:2322-2400 (agents.third_party.* toolbar/composer settings), :1803-1830 (PerAgentSettings), :2774-2784 (agents.third_party.per_agent)
- app/src/terminal/cli_agent_sessions/plugin_manager/claude.rs:21-26,61-63; .../codex.rs:17-26,86-88; .../gemini.rs:21,56-58 (plugin repos, minimum versions, auto-install)
- app/i18n/en/warp.ftl:613,952 (Settings > AI > Third party CLI agents)

Harness driver
- app/src/ai/agent_sdk/driver/harness/mod.rs:48-158 (ThirdPartyHarness, HarnessKind, harness_kind dispatch), :162-177 (validate_cli_installed), :210-313 (OZ_* and model env vars)
- app/src/ai/agent_sdk/driver/harness/claude_code.rs:63-91 (docs URL, error patterns), :157-182 (launch command), :530-568 (.claude.json + settings.json trust pre-writes), :621-651 (ANTHROPIC_API_KEY)
- app/src/ai/agent_sdk/driver/harness/codex.rs:91-152 (flags, error patterns), :237-249 (launch command), :432-434 (transcript export skipped), :441-476,:548-556 (auth.json seeded from OPENAI_API_KEY), :667-692 (trust_level trusted for cwd and child repos)
- app/src/ai/agent_sdk/driver/harness/gemini.rs:44-49 (docs URL; MCP ignored), :91-93 (launch command), :201-241 (settings.json auth type + trustedFolders)
- app/src/ai/harness_display.rs:16-25 (user-visible harness names)
- crates/warp_cli/src/agent.rs:122-145 (Harness value names/aliases), :301-398 (RunAgentArgs incl. --harness hide=true), :53-70 (--prompt/-p)
- crates/warp_cli/src/lib.rs:88-100 (clap name "oz", display_name "Phosphor"), :353-373 (CliCommand: Agent/MCP/Model/Whoami/Provider), :229-246 (AgentCommand)
- crates/warp_cli/src/model.rs (ModelCommand::List only; its own help text says "warp model list"), crates/warp_cli/src/provider.rs:4-58 (ProviderCommand is Linear/Slack)
- crates/warp_cli/src/lib.rs:168-175 (provider subcommand rejected when FeatureFlag::ProviderCommand is off), :217-220 (hidden from help), :224-238 (examples use the real binary name; usage still says "oz"); app/src/ai/agent_sdk/mod.rs:98-103 (same gate at dispatch); crates/warp_features/src/lib.rs:847 (ProviderCommand only in DOGFOOD_FLAGS, i.e. dark)
- crates/warp_core/src/channel/mod.rs:38-47 (cli_command_name: Oss -> "phosphor-oss")
- app/src/ai/agent_sdk/mod.rs:94-98 (dispatch), :151-157 (AgentHarness gate; opencode rejected), :512 (secrets empty on the agent-run path), :530-536 (task_id None so OZ_RUN_ID is not set)
- app/src/ai/agent_sdk/provider.rs:1,60,72-98 ("Provider OAuth setup ... is disabled in Phosphor")
- app/src/ai/agent_sdk/driver.rs:401-404 (NotLoggedIn gate) with app/src/auth/mod.rs:294-296 (is_logged_in always true, so the gate never fires), :1109-1134 (setup/prepare/run harness), :1266-1275 (MCP translated per harness), :1330-1440 (error-pattern output monitor)
- app/src/ai/agent_sdk/driver/terminal.rs:102-118,308-353 (harness runs as an ordinary command in a real terminal session)
- app/src/pane_group/pane/local_harness_launch.rs:113-128 (/orchestrate argument syntax), :103-111 (ORCHESTRATE_DEFAULT_HARNESS = "claude"), :150-162 (bash/zsh/fish only), :295-298 (Codex precondition), :300-373 (child launch commands), :301,:372 (oz and gemini unreachable, i.e. filtered out earlier), :313-317 (children inherit local harness auth)
- crates/warp_cli/src/agent.rs:153-158 (parse_local_child_harness accepts Claude/OpenCode/Codex; rejects Oz/Gemini/Unknown)
- app/src/ai/local_harness_setup.rs:50-51,83-95 (Codex child gated on LocalClaudeCodexChildHarnesses), :74-109 (dead selectable/product-enabled helpers)
- crates/warp_features/src/lib.rs:806-828 (LOCAL_FLAGS / DOGFOOD_FLAGS enable nothing in this fork), :926 (RUNTIME_FEATURE_FLAGS)
- app/src/terminal/view/ambient_agent/harness_selector.rs (exists), app/src/terminal/input.rs:2251-2257,1652,3327 (constructed and stored but never rendered as a child view)

Interrupt
- app/src/terminal/view.rs:8435-8454 (write_viewer_bytes_to_pty + CtrlCCancelsThirdPartyHarness observation) with only test callers at app/src/terminal/view_test.rs:6742,6802
- app/src/terminal/cli_agent_sessions/mod.rs:22,46,597-712 (2s window, force_cancel, Cancelled status), :66-72 (OSC 777 "cancelled" error type maps to Cancelled)
- crates/warp_features/src/lib.rs:778-785 (flag doc: purely client-side status synthesis; keystroke always forwarded, process never signaled), :882 (only in DOGFOOD_FLAGS)
- TODO.md:571-582 (the audit: "no production caller ... a path that cannot execute")
- app/src/terminal/view.rs:7812-7851 (handle_ctrl_c_input_event), :7996-8048 (ctrl_c_to_active_block: takeover, stop, raw 0x03), :8105-8141 (route to the agent status bar), :7748-7806 (stop_local_agent_conversation)
- app/src/ai/blocklist/block/status_bar.rs:405-429 (handle_ctrl_c incl. summarization confirm), :567-623 (cancel request/action/conversation), :268,278,844-848 (Stop button shows the terminal:cancel_command keystroke)
- crates/warp_tui/src/terminal_session_view.rs:3179-3285 (TUI ctrl-c priority order), :3303-3325 (cancel_active_conversation), :175 (CTRL_C_EXIT_HINT) and crates/warp_tui/src/exit_confirmation.rs:14 (1s window)
- app/src/terminal/model/block.rs:79 (LONG_RUNNING_COMMAND_DURATION_MS = 50)

Not available
- app/src/ai/ambient_agents/spawn.rs:68-73 ("Agent spawning is disabled in Phosphor")
- DECLINED.md:80 (agent attribution), :81 (Warp Environments), :82 (RunAgents / cloud runners), :83-84 (teams/org policy), :85 (account-first onboarding, billing, paid tiers), :86 (/logout), :87 (/voice), :88 (skills global-spec filtering), :215 (provider-cost baselines; cites usage.rs:1-12), :216 (OpenAI long-context pricing warning), :227 (agent session sharing --share hidden), :206 (screen recording), :214 (InitProject wizard), :213 (multi-agent orchestration: local back in scope, cloud half declined), :168 (master AI switch does not gate third-party CLI agents), :170 (indexing consent = the false default), :96 (xAI/Grok OAuth)
- crates/warp_features/src/lib.rs:949-966 (FORCE_DISABLED_FLAGS: ForceLogin, AvatarInTabBar, HOARemoteControl)
-->
