# 6. Extending Phosphor: MCP, skills, rules, workflows

Phosphor's agent knows how to run commands and edit files out of the box.
Everything else you want it to know — your APIs, your house style, your
repeatable chores — is added through four separate mechanisms, each with its own
files on disk:

| Mechanism | What it adds | Where it lives |
|---|---|---|
| **MCP servers** | New *tools* the agent can call (Model Context Protocol) | `.mcp.json` files |
| **Skills** | Reusable *instructions* the agent can load on demand | `SKILL.md` files |
| **Rules** | *Standing instructions* injected into every conversation | Markdown rule files |
| **Workflows** | Saved *shell commands* with named parameters | YAML files |

All four are local files. Phosphor is bring-your-own-provider: there is no
account, no server-side registry, and nothing here syncs anywhere. If a file is
not on your disk, Phosphor cannot see it.

Throughout this chapter the binary is written `phosphor-oss`. That is the real
command name for the open-source build — `Channel::Oss` maps to the CLI name
`phosphor-oss` and the URL scheme `phosphor://`. The application's config
directory is `~/.phosphor`. (Several strings inside the program still say
"Warp" or "Zap"; where that matters, this chapter says so.)

---

## 6.1 MCP servers

### What MCP is here

An MCP server is a separate process (or a remote HTTP endpoint) that exposes
tools and resources to the agent over a standard protocol. Phosphor starts the
process for you, keeps its logs, and offers its tools to the agent. Servers come
from three places:

1. **Config files on disk** — the main path. Phosphor watches a small set of
   `.mcp.json`/`config.toml` files and picks up servers as you save them.
2. **The MCP Servers settings page** — a JSON editor that saves a server into
   Phosphor's local object store.
3. **`--mcp` on a CLI agent run** — a one-shot server for that run only.

There is **no hosted server catalogue**. Upstream Warp ships an "MCP gallery"
fetched from its cloud; `MCPGalleryManager` here is a deliberately gutted stub
whose `get_gallery()` returns an empty list for the life of the process
(`DECLINED.md`). The "Available to install" section of the settings page shows
only templates you saved yourself.

### How do I add a server from a config file?

This is the recommended way. Create the file, save it, and Phosphor picks the
server up — no restart.

**Global (applies in every session):**

```
~/.phosphor/.mcp.json
```

**Project-scoped (applies only inside that repository):**

```
<repo-root>/.warp/.mcp.json
```

> The project directory really is `.warp`, not `.phosphor` —
> `MCPProvider::Zap::project_config_path()` is a hard-coded `.warp/.mcp.json`.
> Only the *global* path follows the `~/.phosphor` rename.

A worked example — one stdio server and one HTTP server:

```json
{
  "mcpServers": {
    "sqlite": {
      "command": "uvx",
      "args": ["mcp-server-sqlite", "--db-path", "./app.db"],
      "env": {
        "LOG_LEVEL": "info"
      }
    },
    "my-api": {
      "url": "https://mcp.example.com/mcp",
      "headers": {
        "Authorization": "Bearer ${MY_API_TOKEN}"
      }
    }
  }
}
```

Then, in a Phosphor session, open Settings → MCP Servers (or type
`/open-mcp-servers`) and you will see both under **Detected from Phosphor**.

**Accepted wrapper keys.** Phosphor looks for the server map under
`mcp.servers`, `servers`, `mcpServers`, or `mcp_servers`, in that order. Use
`mcpServers`; the others exist so you can paste a config written for another
tool.

In a **config file**, one of those keys is required — a file with no recognised
wrapper is treated as containing no servers. That is deliberate: it stops an
unrelated `~/.claude.json` settings blob from being read as a server map. In
**pasted JSON** (the settings editor, or `--mcp` on the CLI) Phosphor is more
permissive and will fall back to treating the whole top-level object as a bare
name → server map.

**Server object fields.**

| Field | Type | Meaning |
|---|---|---|
| `command` | string | Executable to spawn (stdio transport). Mutually exclusive with `url`. |
| `args` | array of strings | Arguments to `command`. Default `[]`. |
| `env` | object of string → string | Environment variables for the process. Default `{}`. |
| `working_directory` | string | Working directory for the process. Defaults to the directory the config was discovered in. |
| `url` | string | Endpoint for an HTTP/SSE server. Mutually exclusive with `command`. Also accepted under the alias `serverUrl`. |
| `headers` | object of string → string | Static request headers for a `url` server. Default `{}`. |
| `description` | string | Optional human description shown in the UI. |

**Secrets.** Any `${VAR}` in the file is substituted from your environment when
the server starts. If the variable is unset, the server is not started and the
config is reported as unhealthy — the substitution deliberately errors rather
than passing an empty string.

**Where servers are spawned from.** A stdio server runs from the directory its
config was found in: the repo root for a project config, your home directory for
a global one. Set `working_directory` to override.

### The other providers Phosphor reads

Phosphor also reads the MCP config files of three other agent tools, so a server
you already configured for Claude Code or Codex shows up here too:

| Provider (UI label) | Global config | Project config |
|---|---|---|
| Phosphor | `~/.phosphor/.mcp.json` | `<repo>/.warp/.mcp.json` |
| Claude | `~/.claude.json` | `<repo>/.mcp.json` |
| Codex | `~/.codex/config.toml` | `<repo>/.codex/config.toml` |
| Other Agents | `~/.agents/.mcp.json` | `<repo>/.agents/.mcp.json` |

The Codex file is TOML and uses Codex's own schema
(`[mcp_servers.<name>]` with `command`/`args`/`env`/`env_vars`/`cwd`, or
`url`/`bearer_token_env_var`/`http_headers`/`env_http_headers`); Phosphor
translates it. For the two non-Phosphor JSON files Phosphor requires a
recognised wrapper key — it will not treat an arbitrary `~/.claude.json`
settings blob as a server map.

### Which detected servers start automatically

This is the part that surprises people, so it is worth stating exactly:

| Where the server came from | Starts automatically? |
|---|---|
| Global Phosphor config (`~/.phosphor/.mcp.json`) | **Yes, always** — the `Auto-spawn` toggle does not apply. |
| Global third-party config (Claude/Codex/Agents, in your home dir) | Only if **Settings → AI → Auto-spawn servers from third-party agents** is on. Default **off**. |
| Any project-scoped config (any provider) | **Never.** Start it by hand from the "Detected from …" section of the MCP Servers page. |
| Anything at all, in the TUI | Never automatically; the TUI's `/mcp` menu requires an explicit start. |

The toggle is `agents.mcp_servers.file_based_mcp_enabled` in `settings.toml`,
default `false`.

### How do I add a server through the UI?

**GUI:** type `/add-mcp` in the input, or `/open-mcp-servers`, or Settings → AI
→ MCP Servers → *Manage MCP servers*. There is also a rebindable action,
`workspace:open_mcp_servers`, which ships with **no default keystroke** — assign
one in the keybinding editor if you want it. `/add-mcp` opens a JSON editor pane; paste exactly the
same JSON shape shown above and save. Servers added this way are stored in
Phosphor's local object store, not in a file you can edit directly (use *Edit*
on the server's card).

`/add-mcp` and `/open-mcp-servers` are **GUI-only**. Neither has a TUI
implementation; they are filtered out of the TUI's slash-command menu on
purpose, because they open a GUI pane that does not exist there.

**TUI:** type `/mcp`. That opens a searchable menu with one row per server,
drawn from three local sources and labelled accordingly: `CLI local` (an
installation), `saved template`, or the config file(s) it came from
(`Phosphor global`, `Claude global`, `Codex · my-repo`, …; a server defined in
two places lists both).
`Enter` runs the row's primary action, which depends on status —
*Enable* (for a catalogue entry that is not installed yet), *Start*, *Stop*,
*Retry*, or *Reopen authorization*. `Ctrl+R` logs out of a server and deletes
its stored credentials, on rows that have credentials. `/mcp` is **TUI-only**
and is not offered in the GUI.

If a server template asks for values (an API token, say), the TUI walks you
through them one at a time. Free-text values are masked on screen — every one of
them, because the template schema carries no marker saying which are secret.

### How do I see whether a server started, and why it didn't?

Each server writes its own log file:

```
<state dir>/mcp/<server-uuid>.log
```

where `<state dir>` is `~/.local/state/phosphor` on Linux,
`~/Library/Application Support/dev.phosphor.Phosphor` on macOS, and
`%LOCALAPPDATA%\phosphor\Phosphor\data\logs\mcp\` on Windows (Windows inserts a
`logs` component). Logs rotate at 10 MiB with 5 rotations kept, so a
misbehaving server is capped at 60 MiB.

- **GUI:** the server's card on the MCP Servers page has a **View logs** button
  and a *Show logs* tooltip icon.
- **TUI:** `/view-logs` bundles the whole app's logs into a zip and reveals it in
  your file manager. It is TUI-only.
- **Broken config file:** a file that fails to read or parse is reported as a
  diagnostic row rather than silently ignored — the TUI `/mcp` menu renders one
  non-selectable row per unhealthy config file, naming the file.

Troubleshooting order:

1. Is the config file where Phosphor looks? (Global is `~/.phosphor/.mcp.json`,
   **not** `~/.warp/.mcp.json` — see the known-issue note below.)
2. Does every `${VAR}` in it resolve in the environment Phosphor was launched
   from? An unset variable stops the server before it spawns.
3. Is it a project config? Those never auto-start; start it from the "Detected
   from …" section.
4. Read `<state dir>/mcp/<uuid>.log`.

### Remote servers and OAuth

`mcp_oauth` is on by default. When a `url` server returns a 401, Phosphor runs
the OAuth flow and stores the credentials in your OS keychain, redirecting back
to `phosphor://mcp/oauth2callback`.

**Phosphor registers itself dynamically** (RFC 7591 dynamic client
registration). Upstream Warp falls back to a table of pre-registered client IDs
per issuer when dynamic registration fails; this build ships that table empty
(`mcp_static_config: None`), so a server whose authorization server does *not*
support dynamic client registration cannot be authorized here. That includes
GitHub's.

### The `mcp` CLI subcommand

```
phosphor-oss mcp list
```

Lists installed MCP servers as UUID + name.

```
$ phosphor-oss mcp list
UUID                                  Name
0f1c…                                 sqlite
7a42…                                 my-api

$ phosphor-oss --output-format json mcp list
[{"uuid":"0f1c…","name":"sqlite"}, …]
```

`list` is the only `mcp` subcommand. `--output-format` is global and accepts
`pretty` (default), `json`, `ndjson`, `text`; it can also be set with
`WARP_OUTPUT_FORMAT`.

### Starting servers for one CLI agent run

`phosphor-oss agent run` takes `--mcp <SPEC>`, repeatable. A spec is:

- **a path to a JSON file** containing MCP config, or
- **inline JSON**.

```sh
phosphor-oss agent run \
  --prompt "Summarise the schema of app.db" \
  --mcp ./mcp/sqlite.json \
  --mcp '{"mcpServers":{"fetch":{"command":"uvx","args":["mcp-server-fetch"]}}}'
```

Inline JSON may omit the outer braces (`--mcp '"fetch": {"command":"uvx"}'`
is accepted), and a bare single-server object with `command` or `url` at the
top level is wrapped automatically under a generated name.

You can also put the servers in the run's config file
(`-f/--file`, or `WARP_AGENT_CONFIG_FILE`), as JSON or YAML, under a strict
`mcp_servers` key holding the *unwrapped* map:

```yaml
# agent.yaml
name: schema-review
mcp_servers:
  sqlite:
    command: uvx
    args: ["mcp-server-sqlite", "--db-path", "./app.db"]
```

```sh
phosphor-oss agent run -f agent.yaml --prompt "Summarise the schema"
```

Unknown keys in that file are rejected; the accepted set is `name`, `model_id`,
`base_prompt`, `mcp_servers`, `host`, `computer_use_enabled`. CLI flags win over
the file.

Validation applied to every server entry, from either source: exactly one of
`warp_id`, `command`, `url`; `command` non-empty; `args` an array of strings;
`url` non-empty; `env` and `headers` objects of string → string; duplicate server
names across specs are an error.

### `warp_id`: don't use it

The validator accepts a `warp_id` field holding a UUID. It identifies a
*Warp-managed* server living in Warp's cloud, which this fork does not have.
Nothing here can resolve one. No Phosphor path produces such an entry — you
would have to hand-write it — and if you do, the agent gets a server entry with
no `command` and no `url` and fails with nothing explaining why. Upstream's
companion "well-known id" form (`--mcp linear`, `--mcp notion`) is
**deliberately not ported**: those ids are resolved by Warp's server, so a
`MCPSpec::WellKnown` variant would be a second spec that parses and can never
run. `MCPSpec` here has exactly two variants, `Uuid` and `Json`.

**In short: named "well-known" servers are not supported. Define servers by
JSON, by file, or by config.**

---

## 6.2 Markdown, Mermaid and notebooks

This is small but people ask, so here is the honest state.

### In agent responses (GUI)

- **Tables.** GitHub-flavoured Markdown tables in an agent reply are parsed and
  drawn as real tables. On by default (`markdown_tables` and
  `blocklist_markdown_table_rendering` are both default features, and the latter
  is also in `RELEASE_FLAGS`).
- **Mermaid.** A ```` ```mermaid ```` fence in an agent reply is rendered as a
  diagram inline, with the source available and a lightbox for a larger view.
  On by default (`markdown_mermaid` + `blocklist_markdown_images`). The renderer
  is a **native Rust** implementation (`mermaid_to_svg`) — no browser, no
  network. It covers flowcharts, sequence, class, state, ER, gantt, gitgraph,
  pie, quadrant, radar, journey, mindmap, timeline, kanban, packet, sankey,
  requirement, block, C4 and xychart diagrams. If a diagram fails to parse or
  render, the block falls back to showing the raw Markdown source, not an error.
  Only the language tag `mermaid` (case-insensitive, optionally followed by
  parameters) triggers it.
- **Images.** Inline Markdown images in agent replies render by default
  (`blocklist_markdown_images`).

### In Markdown files you open

Open a `.md`/`.markdown` file (or a bare `README`, `CHANGELOG` or `LICENSE`) and
it opens in Phosphor's Markdown viewer with a **Rendered / Raw** toggle in the
header. The default is **Rendered**, and Mermaid blocks inside it default to
rendered too, each with its own per-block Raw/Rendered buttons.

### Editable Mermaid — off

`FeatureFlag::EditableMarkdownMermaid`, which makes Mermaid blocks behave
atomically while editing in the notebook and plan editors, is **not on**. It has
no entry in `app/Cargo.toml`'s `default` feature list, and it sits only in
`DOGFOOD_FLAGS` — and in this fork **`DOGFOOD_FLAGS` membership enables nothing
at runtime**: no binary this repository builds passes that list to
`with_additional_features`. It is also not in `UNSTABLE_FEATURES`, so there is no
`ZAP_UNSTABLE_FEATURES` token for it either. The only way to turn it on is to
rebuild with `--features editable_markdown_mermaid`.

The same is true of `FeatureFlag::MarkdownImages`, with an extra wrinkle: it has
**no consumer anywhere in this tree**. Turning it on would change nothing.

### Jupyter notebooks — behind an opt-in

`.ipynb` files render as a formatted, read-only notebook instead of raw JSON
only when `FeatureFlag::JupyterNotebookRendering` is on. It is off by default,
but unlike the Mermaid flag above it **has a runtime enable path**:

```sh
ZAP_UNSTABLE_FEATURES=jupyter_notebook_rendering phosphor-oss
```

`ZAP_UNSTABLE_FEATURES` takes a comma- or whitespace-separated list of names, or
`all` / `*`. It is the only runtime enable path for the flags registered in
`UNSTABLE_FEATURES`, and it works identically in debug and release builds — a
`cargo run` build does **not** turn these on by itself.

Other tokens accepted there, for completeness:
`windows_high_performance_gpu_default`, `gemini_notifications`,
`full_source_code_embedding`, `codebase_index_persistence`, `warp_control_cli`,
`jupyter_notebook_rendering`, `multi_level_orchestration`,
`local_docker_sandbox`.

### Phosphor's own notebooks

Phosphor also keeps "notebooks": Markdown documents held in its local object
store and edited in a rich-text editor rather than as files on disk. They are
entirely local — nothing about them is synced, shared or published. They render
Markdown tables and Mermaid under the same flags described above, and Mermaid
blocks there carry the same per-block Raw/Rendered buttons.

---

## 6.3 Skills

### What a skill is

A skill is a folder containing a `SKILL.md` file. The agent is shown a catalogue
of every skill's **name and description** on every turn; when a task matches one,
it calls the `read_skill` tool and the skill's body is loaded into the
conversation. So a skill is instructions the agent pulls in on demand, rather
than something you have to remember to paste.

You can also invoke one yourself, by name, with arguments.

### Where skill files live

A *skills directory* holds one folder per skill, and each folder must contain a
file called exactly `SKILL.md`:

```
<skills-dir>/
  my-skill/
    SKILL.md
    references/…      # anything else you want; only SKILL.md is indexed
```

Only **direct children** of a skills directory are scanned — no recursion, no
other filename.

Phosphor recognises ten skills directories, and reads each of them at both
**global** (home) and **project** scope. Precedence runs top to bottom:

| # | Provider | Project path | Global path |
|---|---|---|---|
| 1 | Agents | `<repo>/.agents/skills/` | `~/.agents/skills/` |
| 2 | Phosphor | `<repo>/.warp/skills/` | **`~/.phosphor/skills/`** |
| 3 | Claude | `<repo>/.claude/skills/` | `~/.claude/skills/` |
| 4 | Codex | `<repo>/.codex/skills/` | `~/.codex/skills/` |
| 5 | Cursor | `<repo>/.cursor/skills/` | `~/.cursor/skills/` |
| 6 | Gemini | `<repo>/.gemini/skills/` | `~/.gemini/skills/` |
| 7 | Copilot | `<repo>/.copilot/skills/` | `~/.copilot/skills/` |
| 8 | Droid | `<repo>/.factory/skills/` | `~/.factory/skills/` |
| 9 | GitHub | `<repo>/.github/skills/` | `~/.github/skills/` |
| 10 | OpenCode | `<repo>/.opencode/skills/` | `~/.opencode/skills/` |

Note the asymmetry on row 2: the **project** directory is `.warp/skills` (the
literal name, unchanged from upstream), while the **global** one follows the
Phosphor rename to `~/.phosphor/skills`.

Global skills are always in scope. Project skills are in scope when your working
directory is at or below the directory holding them, and inside the detected
repository root.

**One skill per name.** If the same skill name appears under two providers in the
same directory, Phosphor keeps only the higher-priority one — an `.agents/skills/deploy`
hides a `.claude/skills/deploy` even if their bodies differ. This is a
deliberate divergence from upstream (which would list both) made to keep the
system prompt byte-stable for prompt caching; see `DECLINED.md`. The Skills
inventory panel still shows you the shadowed copy so you can tell it is there.

**Extra directories via environment.** `WARP_SKILL_DIRS` takes a comma-separated
list of additional skills roots; `~` is expanded and relative entries resolve
against the agent's working directory. Skills found this way are treated as
global (always in scope). This is read by the **CLI agent driver only** — it does
not affect the GUI or TUI app.

```sh
WARP_SKILL_DIRS=~/team-skills,./local-skills \
  phosphor-oss agent run --skill deploy --prompt "Ship v2.3 to staging"
```

### Writing your own skill

`SKILL.md` is Markdown with optional YAML frontmatter. Exactly two frontmatter
keys are read:

| Key | Required | Default if absent |
|---|---|---|
| `name` | no | the containing directory's name |
| `description` | no | the first non-heading paragraph of the body, truncated to 512 characters at a sentence or word boundary |

Everything else in the frontmatter is parsed and then **ignored** — including
`allowed-tools`, which is honoured by some other agent tools but has no effect
here. The frontmatter must be a flat mapping of strings to strings; non-string
values are dropped silently.

A worked example. Create `~/.phosphor/skills/release-notes/SKILL.md`:

```markdown
---
name: release-notes
description: Draft release notes from the git log since the last tag. Use when the user asks for a changelog, release notes, or "what changed since <tag>".
---

# Release notes

1. Find the most recent tag with `git describe --tags --abbrev=0`.
2. Collect commits since that tag: `git log <tag>..HEAD --no-merges --pretty=format:'%h %s'`.
3. Group them under **Added**, **Fixed**, **Changed**, **Removed**.
4. Drop anything that only touches CI, tests, or formatting.
5. Write the result to `CHANGELOG.md` under a new heading for the next version.
```

Save it. No restart is needed — the skills directories are watched.

Write the `description` carefully: it is the *only* thing the model sees when
deciding whether to load the skill. Say what the skill does **and when to use
it**. Note that an explicit `description` is used verbatim and is **not**
truncated, so an overlong one costs you tokens on every turn.

### Invoking a skill

Type `/` followed by the skill's `name`, then any arguments:

```
/release-notes only include user-visible changes
```

- `/skills` opens a picker; choosing an entry inserts `"/<name> "` into your
  input so you can add arguments before pressing Enter.
- Typing `/` and searching also matches skills directly, mixed in with the
  built-in slash commands.
- The `@` menu has a **Skills** category, but picking one just rewrites the `@`
  into `/<name>` — it is a shortcut into the same form, not an attachment.
- `/open-skill` opens a skill's `SKILL.md` in Phosphor's editor instead of
  running it.
- In a Codex CLI-agent session the prefix is `$` rather than `/`.

Arguments are appended to the skill as a trailing instruction. The exact message
the model receives is:

```
Execute the task following the skill "<name>" guide below:

<the whole SKILL.md, frontmatter included>

---
Additional instruction from the user: <your arguments>
```

**There is no `$ARGUMENTS` or `$1` substitution.** The feature flag's own doc
comment claims there is, but no such code exists in this fork — placeholders you
write into a `SKILL.md` are passed through to the model literally.

The four skill feature flags — `list_skills`, `bundled_skills`,
`skill_arguments`, `oz_platform_skills` — are all in `app/Cargo.toml`'s `default`
list, so all of this is on out of the box.

### From the CLI

There is **no `skill` subcommand**. Skills reach the CLI through `--skill` on an
agent run:

```sh
phosphor-oss agent run --skill release-notes
phosphor-oss agent run --skill release-notes --prompt "target the 2.3 branch"
phosphor-oss agent run --skill .agents/skills/deploy/SKILL.md
phosphor-oss agent run --skill warp-internal:code-review
phosphor-oss agent run --skill myorg/warp-internal:code-review
```

With `--prompt`, the skill is the base context and the prompt is the task; either
alone is enough to drive a run. A bare name is searched with the provider
precedence above (home first, then the current repo, then repos beneath the
working directory); a path containing a separator resolves directly. Absolute
paths are rejected.

> The help text for `--skill` points at `oz schedule create --skill …`. **There is
> no `schedule` subcommand in Phosphor** — the CLI has `agent`, `mcp`, `model`,
> `provider` and `whoami`. Ignore that line.

### Bundled skills

Eleven skills ship inside the application and are always available without any
setup:

| Name | What it does |
|---|---|
| `agent-add-mcp` | Walks through adding an MCP server to the config files, global vs project scope. (Directory: `add-mcp-server`.) |
| `change-keybinding` | Remap, rebind or remove a keyboard shortcut by editing `keybindings.yaml`. |
| `claude-api` | Build, debug and optimise Claude API / Anthropic SDK code; ships per-language reference material. |
| `create-skill` | Author, edit, evaluate and optimise skills — including this kind of skill. |
| `create-tab-config` | Generate a new tab-config TOML from a plain-English description. |
| `modify-settings` | View, change or troubleshoot settings using the bundled JSON settings schema. |
| `pr-comments` | Fetch the current branch's GitHub PR review comments and render them in the review pane. Invoke it as `/pr-comments`: the standalone `/pr-comments` slash command is deliberately *not* registered when this skill is present, and both are on by default. |
| `tab-configs` | The canonical tab-config TOML schema reference; feeds the two tab-config skills. |
| `tui-settings` | Explains which settings drive the terminal UI vs the GUI, given the single shared settings file. |
| `update-tab-config` | Edit an existing tab-config TOML in place. |
| `warpctrl` | Drive the running Phosphor app — windows, tabs, panes, input, themes — through the `warpctrl` CLI. |

Two of them are conditional:

- **`modify-settings`** requires `settings_schema.json` to be present in the
  app's bundled resources. That file is generated at packaging time, so the skill
  is absent from a plain `cargo run` tree and present in a real build.
- **`warpctrl`** requires `FeatureFlag::WarpControlCli`, which is **off by
  default**. It is not in `app/Cargo.toml`'s `default` list; it sits in
  `DOGFOOD_FLAGS`, which **enables nothing at runtime in this fork** — no binary
  this repository builds passes that list to `with_additional_features`. Its real
  enable path is the environment variable:

  ```sh
  ZAP_UNSTABLE_FEATURES=warp_control_cli phosphor-oss
  ```

A further eight **Figma** skills (`figma-use`, `figma-implement-design`,
`figma-generate-design`, `figma-generate-library`, `figma-create-new-file`,
`figma-create-design-system-rules`, `figma-code-connect-components`,
`edit-figma-design`) ship separately and activate only while a Figma MCP server
is actually running.

Phosphor bundles **fewer skills than upstream Warp**. Warp's `tui-migrate-setup`
is deliberately not ported (it reconciles two settings files, and this fork has
one) — `tui-settings` replaces it. Warp's channel-gated skills only ship on
internal channels, and the OSS channel matches none of them, so nothing from
`resources/channel-gated-skills/` is in a shipped Phosphor. **No
Factory/Droid-authored skills are bundled**, though `.factory/skills/` is one of
the ten directories Phosphor reads, so Factory skills you install yourself are
picked up.

Bundled skills are **read-only**: they are read in place from the installed
application's resources directory and are never copied into your home directory,
so there is nothing to edit and an app update replaces them. To override one,
create a skill with the same `name` in a real skills directory — file-backed
skills win over bundled ones on name lookup. Bundled skills also never appear in
the Skills inventory panel, which lists only file-backed skills.

Bundled skill bodies are templated before use, so paths and command names are
correct for your install: `{{warp_cli_binary_name}}`, `{{warpctrl_binary_name}}`,
`{{warpctrl_wrapper_path}}`, `{{warp_url_scheme}}`, `{{settings_file_path}}`,
`{{keybindings_file_path}}`, `{{settings_schema_path}}`.

### Skills over SSH

The remote-server extension installs its own bundled resources on the host, so
skills work over SSH. If that tree is missing, the agent gets an explicit
refusal message rather than an empty skill list.

---

## 6.4 Rules

Rules are standing instructions. Unlike skills, they are not loaded on demand —
every active rule goes into the system prompt of every request.

There are two kinds, stored and discovered differently:

- **Project rules** — Markdown files in your repository.
- **Global rules** — either one file in your home directory, or short notes you
  save through Phosphor's Rules pane.

### Project rules

Phosphor looks for three filenames, in this priority order:

1. `WARP.md`
2. `AGENTS.md`
3. `CLAUDE.md`

**Only one file per directory is used** — the highest-priority one present. If a
directory has both `AGENTS.md` and `CLAUDE.md`, the `CLAUDE.md` is ignored.
`CLAUDE.md` is recognised so a repository migrated from Claude Code works with no
changes; `AGENTS.md` is the cross-tool convention; `WARP.md` is the native name
inherited from upstream.

**Which files apply.** A rule file applies to any path at or below its own
directory, and they **accumulate**: working in `repo/services/api/` picks up
`repo/AGENTS.md` *and* `repo/services/AGENTS.md` *and*
`repo/services/api/AGENTS.md`, all of them, as separate sections. Rule files in
sibling directories are discovered but not injected.

Discovery has limits worth knowing:

- The full index runs when Phosphor detects a **git repository**, scanning at
  most 3 directory levels and 5000 files.
- Until that index is ready — right after a `cd`, say — a fast path walks up to
  6 levels from your working directory with a 20 ms budget. So in a
  **non-git directory**, that fast path is the only thing that finds rules.
- Rule files have **no size limit**. Everything in them goes into every request.

### Global rules

Two independent mechanisms:

**A file.** `~/.agents/AGENTS.md` — and only that path. There is no
`~/.phosphor/AGENTS.md`, no `~/.warp/rules/`, no `.cursorrules`, no
`.github/copilot-instructions.md`. It is watched live, so edits apply to the
next request. Note that this file does **not** appear in Phosphor's Rules pane,
even though it is in effect.

**Saved rules.** Short named rules stored in Phosphor's local object store, added
with `/add-rule` or from Settings → Agents → Knowledge → *Manage rules*. These are
what the Rules pane's **Global** tab shows.

### Precedence

There is none, in the sense of one rule overriding another. Global rules are
collected first, project rules are appended, and **all of them are sent**. The
model sees each as its own section and resolves any conflict itself. Nothing is
merged, deduplicated or suppressed.

### What the agent actually receives

Two Markdown blocks in the system prompt — not an XML `<rules>` wrapper:

```
# User rules
The user has configured the following global rules in Phosphor (Settings → Agents → Rules). Treat them as authoritative and follow them in all responses.
## <rule name>
<rule content>

# Project rules
The user has the following project rules configured. Treat them as authoritative project conventions.
## <path to the rule file>
<file content>
```

Paths of rule files that were *discovered but not active* are deliberately not
included.

> **Important exception.** These blocks live in the shared prompt footer, and one
> system-prompt template does not include that footer: `system/local.j2`, which
> Phosphor selects for **any Ollama provider**. With an Ollama model, neither
> your rules nor your skills are injected into the system prompt. The short
> template exists so a 9k-token prompt does not swamp a small local model, but
> the effect on rules is real. If you rely on rules, use a non-Ollama provider —
> or override `system/local.j2` yourself (see §6.6).

### Worked example

```sh
mkdir -p ~/.agents
cat > ~/.agents/AGENTS.md <<'EOF'
# Personal conventions
- Never add a dependency without asking first.
- Write commit messages in the imperative mood, no trailing period.
- When you change behaviour, update the test in the same edit.
EOF
```

Then, in a repository:

```sh
cat > AGENTS.md <<'EOF'
# acme-api
- Rust 2024 edition. Run `cargo fmt` before you finish.
- Database migrations live in `migrations/`; never edit an applied one.
- Public API changes require an entry in `CHANGELOG.md`.
EOF
```

Both are now in every request made from inside that repository.

Or let the agent write the file for you: `/init` generates or updates an
`AGENTS.md` for the current repository, and takes an optional argument
(`/init focus on the test conventions`).

### Rules commands and settings

| Command | Surface | What it does |
|---|---|---|
| `/init` | GUI + TUI | Generate or update an `AGENTS.md` for this repository. |
| `/add-rule` | GUI | Opens the pane for adding a new saved global rule. |
| `/open-rules` | GUI | Opens the Rules pane — Global and Project-based tabs. |
| `/open-project-rules` | GUI (requires a repository) | Opens the project rules file in Phosphor's editor. |

There is no `/rules` command.

> **Known inconsistency:** `/open-project-rules` is described as "Open the project
> rules file (AGENTS.md)", but it always opens `<cwd>/WARP.md` regardless of which
> rule file the project actually has. If your repository uses `AGENTS.md`, open it
> yourself.

| Setting | TOML path | Default | Where |
|---|---|---|---|
| Rules | `agents.knowledge.rules_enabled` | `true` | Settings → Agents → Knowledge → *Rules* |
| Suggested Rules | `agents.warp_agent.active_ai.rule_suggestions_enabled` | `true` | Settings → Agents → Knowledge → *Suggested Rules* |

There is **no setting for a rules path** — the filenames and `~/.agents/AGENTS.md`
are fixed constants.

### Suggested rules: the toggle does nothing here

`Suggested Rules` is meant to put a chip under an agent response proposing a rule
to save. In Phosphor **it never fires**: suggestions only ever arrived through a
cloud protocol message that BYOP cannot synthesise, and the producing path is
disabled with an explicit comment saying so. The toggle exists, defaults on, and
has no observable effect. Treat it as absent.

---

## 6.5 Workflows and prompts

Workflows are saved commands with named parameters. Phosphor calls the two
variants by different names in the UI, and it helps to know which is which:

- **Workflow** = a saved **shell command** with `{{placeholders}}`.
- **Prompt** = a saved **agent query** — the same file format with
  `type: agent_mode` and a `query:` instead of a `command:`.

Both exist and both work locally. What does *not* exist is anything shared: see
"Not available", below.

### Where workflow files live

| Scope | Path |
|---|---|
| Personal | `<data dir>/workflows/` — Linux `~/.local/share/phosphor/workflows/`, macOS `~/.phosphor/workflows/`, Windows `%APPDATA%\phosphor\Phosphor\data\workflows\` |
| Repository | `<repo-root>/.warp/workflows/` |

Files are `.yaml` or `.yml`, scanned recursively (symlinks followed). One file may
hold several workflows as a multi-document YAML stream (`---` separated). A file
that fails to parse is skipped silently — check the app log if a workflow does not
appear. The personal directory is watched, so new files show up without a restart.

> The **repository** directory is the literal `.warp/workflows`, not
> `.phosphor/workflows`. That is deliberate: a repo's workflows stay portable
> between Warp and Phosphor. The **personal** directory did get renamed, so
> personal workflows do not carry across.

### The schema

Command workflow (no `type:` key):

```yaml
name: Tail a service log
command: journalctl -u {{service}} -n {{lines}} -f
description: Follow the last N lines of a systemd unit's log.
tags: [ops, logs]
arguments:
  - name: service
    description: systemd unit name
    default_value: nginx
  - name: lines
    description: how many lines of backlog
    default_value: "200"
shells: [bash, zsh]
author: you
source_url: https://example.com/runbook
```

| Field | Required | Default | Notes |
|---|---|---|---|
| `name` | yes | — | Shown in the workflow browser. |
| `command` | yes | — | `{{arg}}` interpolates an argument. `{{{literal}}}` escapes to a literal `{{literal}}`. |
| `description` | no | none | |
| `tags` | no | `[]` | Become categories in the browser. |
| `arguments` | no | `[]` | Each has `name` (required), `description`, `default_value`, and an optional type. |
| `shells` | no | `[]` (all) | Restrict the workflow to particular shells. |
| `author`, `author_url`, `source_url` | no | none | Attribution shown in the browser. |

Prompt / agent-mode workflow:

```yaml
type: agent_mode
name: Explain this failure
query: Read the last command's output and explain why it failed, then propose one fix.
description: Diagnose the previous command.
arguments: []
```

Arguments default to free text. They can instead reference a *workflow enum* — a
fixed list of choices, or (with `dynamic_workflow_enums`, on by default) a shell
command that produces the choices. A dynamic enum's command requires your explicit
approval before it runs.

### Running one

**GUI:**

- **`Ctrl+Shift+R`** opens the workflow browser (tabs: All / My Workflows /
  Repository Workflows / one per tag). Also reachable from the app menu and near
  the top of the command palette.
- Selecting a workflow **replaces your input buffer** with the command (or query)
  and opens an argument box to fill the placeholders. Phosphor switches the input
  into agent mode for a prompt workflow and shell mode for a command workflow.
- In the command palette, the `workflows` filter chip searches command workflows
  and the `prompts` chip searches agent-mode ones.
- **`Cmd/Ctrl+Shift+S`** with a block selected saves that block's command as a
  workflow. The block toolbelt also gains a save icon for this.
- *New Personal Workflow* exists in the menu but has **no default keystroke**.
- **Aliases** (on by default): give a saved workflow an alias, type the alias, press
  Enter, and it expands and runs. Aliases also feed shell autocompletion.

**TUI:** there is **no workflow browser and no way to run a command workflow**.
The only workflow surface is `/prompts`, which lists saved **agent-mode**
workflows only. It inserts the raw query text and leaves any `{{placeholders}}`
for you to edit by hand — the TUI has no argument-filling box. Repository
`.warp/workflows` files are not reachable from the TUI at all.

### Flags

| Cargo feature | Flag | Default |
|---|---|---|
| `am_workflows` | `AgentModeWorkflows` | **on** |
| `workflow_aliases` | `WorkflowAliases` | **on** |
| `dynamic_workflow_enums` | `DynamicWorkflowEnums` | **on** |
| `block_toolbelt_save_as_workflow` | `BlockToolbeltSaveAsWorkflow` | **on** |
| `warp_packs` | `WarpPacks` | on, but inert — see below |
| `suggested_agent_mode_workflows` | `SuggestedAgentModeWorkflows` | **off**, and compiled out |

`AgentModeWorkflows` is also listed in `DOGFOOD_FLAGS`; ignore that. In this fork
`DOGFOOD_FLAGS` membership enables nothing at runtime — no binary this repository
builds passes that list to `with_additional_features`. The cargo feature in
`default` is what turns it on.

`suggested_agent_mode_workflows` has no cargo feature in `default` and no
`ZAP_UNSTABLE_FEATURES` token, so the "save this as a workflow?" suggestion chip
is compiled out entirely. The only way to get it is a rebuild with
`--features suggested_agent_mode_workflows` — and even then it depends on the same
disabled cloud suggestion channel as suggested rules, so it would still not fire.

---

## 6.6 Overriding the agent's own prompts

A fork-native surface with no upstream equivalent: you can replace the system
prompt templates and tool descriptions Phosphor compiles in, without rebuilding.

Set **Settings → AI → System prompt template directory**
(`agents.warp_agent.prompt_template_dir` in `settings.toml`, default empty), then
press **Export built-in templates** to seed it. The suggested path is
`~/.phosphor/prompts`, but any directory works.

The seeded tree contains:

- `system/*.j2` — one system prompt per model family (`default`, `anthropic`,
  `gpt`, `codex`, `gemini`, `kimi`, `local`, `lean`, `beast`, `trinity`,
  `troubleshooting`). Phosphor picks one by substring-matching the model id and
  falls back to `default.j2` for anything unrecognised.
- `partials/*.j2` — the pieces every system prompt includes: `env`, `skills`,
  `project_rules`, `user_rules`, `tool_aliases`, `footer`, `thinking_language`,
  `plan_mode`.
- `commands/init_project.j2` — what `/init` sends.
- `tool_descriptions/*.md` — plain text handed to the model as each tool's
  description (`run_shell_command`, `read_files`, `grep`, `file_glob`,
  `apply_file_diffs`, `read_skill`, `ask_user_question`, …).

Behaviour worth knowing:

- Export **never overwrites**. Existing files are skipped, so it is safe to press
  repeatedly — a version upgrade's new templates get filled in and your edits
  survive.
- Overrides are **per file**. A file you delete or break falls back to the
  built-in version individually; it does not disable the whole directory.
- Edits are picked up on the **next request** — no restart. The settings panel
  shows a live status line: `○ Inactive`, or
  `● Active — N of M files loaded from <dir>`.
- The environment variable **`ZAP_PROMPT_DIR`** overrides the setting, for a
  temporary experiment. The status line says so when it is in effect.
- Not everything is overridable. Prompts hardcoded in Rust — the compaction
  prompt, for one — are not exported and cannot be replaced this way.

