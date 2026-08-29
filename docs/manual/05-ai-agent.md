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

| API type | Settings dropdown label | Default base URL |
|---|---|---|
| `OpenAi` (default) | OpenAI | `https://api.openai.com/v1/` |
| `OpenAiResp` | OpenAI-Response | `https://api.openai.com/v1/` |
| `Anthropic` | Anthropic | `https://api.anthropic.com/v1/` |
| `Gemini` | Gemini | `https://generativelanguage.googleapis.com/v1beta/` |
| `DeepSeek` | DeepSeek | `https://api.deepseek.com/v1/` |
| `Ollama` | Ollama | `http://localhost:11434/` |
| `Vertex` | Vertex AI | none — built from project + location |

The `OpenAi` type is the catch-all for anything speaking OpenAI Chat
Completions: OpenRouter, DeepInfra, Groq, SiliconFlow, Moonshot, Zhipu GLM,
DashScope's OpenAI-compatible endpoint, a local vLLM or llama.cpp server, and so
on. `OpenAiResp` is the newer `/v1/responses` API. Pick `DeepSeek` rather than
`OpenAi` if you use a DeepSeek *thinking* model (`deepseek-reasoner` and
friends) — the plain OpenAI adapter drops the `reasoning_content` field those
models require on multi-turn requests and the provider returns a 400.
`Vertex` is the one type with no static key: it mints a short-lived GCP bearer
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
   Rows are labelled `provider / model`, with `(key connected)` on providers
   that have a key stored.
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
one. Add the key afterwards through the settings page or `/api-keys`.

### Things that will bite you

- **A provider is hidden from the picker** if it is disabled, if *all* of its
  models are disabled, or if it has no endpoint (for Vertex: no GCP project ID).
  A provider with zero models counts as "all models disabled".
- **Plaintext HTTP strips your key.** If the base URL is `http://` and the host
  is not loopback (`localhost`, `127.0.0.0/8`, `::1`), Phosphor drops the
  `Authorization: Bearer` header rather than putting your key on the wire in
  clear. The request still goes out — it just goes out unauthenticated, and the
  provider's 401 is what you will see.
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

Type `/` in the agent input. The ones that matter for the agent itself:

| command | what it does |
|---|---|
| `/agent`, `/new` | Start a new conversation |
| `/model` | Switch the base agent model |
| `/api-keys` | Add, view, or clear a provider's API key |
| `/profile` | Switch the active execution profile |
| `/auto-approve` | Toggle auto-approve |
| `/plan` | Plan mode |
| `/queue <prompt>` | Queue a prompt to send after the agent finishes |
| `/compact` | Summarise the conversation to free context |
| `/compact-and <prompt>` | Compact, then send this prompt |
| `/fork`, `/fork-from`, `/fork-and-compact` | Branch the conversation |
| `/rewind` | Rewind to an earlier point |
| `/conversations` | Open conversation history |
| `/usage` | Context-window usage for this conversation |
| `/cost` | Token spend at *your* configured rates |
| `/skills` | Invoke a skill |
| `/init` | Generate or update an `AGENTS.md` |
| `/add-rule`, `/open-rules`, `/open-project-rules` | Manage agent rules |
| `/mcp`, `/add-mcp`, `/open-mcp-servers` | MCP servers |
| `/index` | Index this codebase |
| `/status` | Session status |
| `/export-to-file`, `/export-to-clipboard` | Export the conversation |
| `/open-settings-file` | Open `settings.toml` |

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
it with its own file tools, subject to the read permissions above. (Warp's
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

Eleven skills ship with Phosphor — `add-mcp-server`, `change-keybinding`,
`claude-api`, `create-skill`, `create-tab-config`, `modify-settings`,
`pr-comments`, `tab-configs`, `tui-settings`, `update-tab-config`, `warpctrl` —
some of which only activate on the surface or with the feature they are about.

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

1. The command string is split into its sub-commands, and each sub-command is
   expanded into every shell-equivalent spelling (quotes removed, leading
   environment assignments stripped, line continuations folded, embedded
   newlines flattened). **If any spelling of any sub-command matches the
   denylist, the command is refused** and you are asked.
2. If **auto-approve** is on for this conversation, the command runs.
3. Otherwise the profile's "Execute commands" setting decides:
   - **Always allow** → runs.
   - **Always ask** → runs only if *every* sub-command matches the allowlist;
     otherwise you are asked.
   - **Agent decides** (the default when set) → if the model marked the command
     non-risky, it runs. Otherwise: a command containing a redirection is
     refused; a command where every sub-command matches the allowlist runs; a
     command the model marked read-only runs; anything else is refused and you
     are asked.

The denylist therefore takes precedence over the allowlist and over "Always
allow" — with one deliberate exception, described next.

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

Writing has a hard floor no setting can lift: **MCP configuration files are
never auto-written**. `.mcp.json`, `~/.claude.json`, `~/.codex/config.toml` and
the other providers' config paths are matched by both exact path and filename
suffix, before any autonomy check, and always fall through to asking you. The
reason is direct: a config the agent can silently edit is a config through which
the agent can grant itself new tools.

### Two honest caveats

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
harness from it. It requires bash, zsh or fish (PowerShell is rejected). Codex
as a child is gated behind a flag with no enable path in shipped builds; Gemini
and OpenCode are rejected outright.

**There is no GUI harness picker.** A harness-selector menu exists in the source
but is never rendered, so in the GUI every agent run uses Phosphor's own agent.

#### What each harness needs

| harness | executable on `PATH` | authenticated by | command Phosphor runs |
|---|---|---|---|
| Claude Code | `claude` | your existing Claude Code login, or `ANTHROPIC_API_KEY` in the environment | `claude --session-id <uuid> --dangerously-skip-permissions [--append-system-prompt-file …] [--mcp-config …] < <prompt file>` |
| Codex | `codex` | `~/.codex/auth.json`, seeded from `OPENAI_API_KEY` if present | `codex --dangerously-bypass-approvals-and-sandbox --dangerously-bypass-hook-trust "$(cat <prompt file>)"` |
| Gemini CLI | `gemini` | your existing Gemini CLI auth | `gemini --yolo -i "$(cat <prompt file>)"` |

If the executable is missing, Phosphor tells you and links the vendor's install
docs. There is no minimum version requirement on the CLIs themselves.

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

