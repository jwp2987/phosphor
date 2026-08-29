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

Two more sections follow: **§6.2** covers what Markdown and Mermaid actually
render and where (an honest answer, including the parts that are off by default),
and **§6.6** covers replacing the agent's own system prompt templates — the
escape hatch when none of the four mechanisms above is enough.

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

**GUI:** type `/add-mcp` in the input, or `/open-mcp-servers`, or go to
Settings → AI → MCP Servers → *Manage MCP servers*. There is also a rebindable
action, `workspace:open_mcp_servers`, which ships with **no default keystroke** —
assign one in the keybinding editor if you want it.

`/add-mcp` opens a JSON editor pane; paste exactly the same JSON shape shown
above and save. Servers added this way are stored in Phosphor's local object
store, not in a file you can edit directly — use *Edit* on the server's card to
change one later.

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

`<state dir>` is `~/.local/state/phosphor` on Linux and
`~/Library/Application Support/dev.phosphor.Phosphor` on macOS. On Windows the
path gains a `logs` component:
`%LOCALAPPDATA%\phosphor\Phosphor\data\logs\mcp\<server-uuid>.log`.

Logs rotate at 10 MiB with 5 rotations kept, so one misbehaving server is capped
at about 60 MiB.

- **GUI:** the server's card on the MCP Servers page has a **View logs** button
  and a *Show logs* tooltip icon.
- **TUI:** `/view-logs` bundles the whole app's logs into a zip and reveals it in
  your file manager. It is TUI-only.
- **Broken config file:** a file that fails to read or parse is reported as a
  diagnostic row rather than silently ignored — the TUI `/mcp` menu renders one
  non-selectable row per unhealthy config file, naming the file.

Troubleshooting order:

1. Is the config file where Phosphor looks? The global one is
   `~/.phosphor/.mcp.json`, **not** `~/.warp/.mcp.json`. This trips people up
   because Phosphor's own bundled `agent-add-mcp` skill still tells the agent to
   write `~/.warp/.mcp.json` — a leftover from upstream. If you asked the agent
   to add a server for you and nothing appeared, look there and move the file.
2. Does every `${VAR}` in it resolve in the environment Phosphor was launched
   from? An unset variable stops the server before it spawns.
3. Is it a project config? Those never auto-start; start it from the "Detected
   from …" section of the MCP Servers page.
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

**One skill per name, per root.** If the same skill name appears under two
providers of the same root — the same repository, or your home directory —
Phosphor offers only the higher-priority one to the agent. A
`<repo>/.agents/skills/deploy` hides a `<repo>/.claude/skills/deploy` **even if
their bodies differ**. This is a deliberate divergence from upstream, which would
list both; it exists to keep the system prompt byte-stable for prompt caching
(`DECLINED.md`). The Skills inventory panel groups skills by name and lists every
copy it found, so you can see which one is winning.

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
> `whoami`, and a `provider` subcommand that is hidden from `--help`. Ignore that
> line.

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
`.github/copilot-instructions.md`. It is watched live, so edits apply to the next
request. Two things to know about it: it is prepended to the *project* rules
block in the prompt (so it appears under `# Project rules`, named by its path,
not under `# User rules`), and it does **not** appear anywhere in Phosphor's Rules
pane even though it is in effect.

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

**The *Rules* toggle only controls saved rules.** Turning it off stops the
object-store rules from being collected; it does not stop project rule files or
`~/.agents/AGENTS.md` from being sent. Those are included unconditionally
whenever they are found. To stop a project rule file taking effect, rename or
delete it.

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

One more extensibility surface, worth knowing about because it is the escape
hatch for everything above: you can replace the system prompt templates and tool
descriptions Phosphor compiles in, without rebuilding.

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

---

## 6.7 Not available in Phosphor

Things a Warp user would go looking for in this part of the product and not find.
Each is a decision, not an oversight; `DECLINED.md` is the register.

| What | Why it is absent |
|---|---|
| **The MCP gallery / hosted server catalogue** | Warp's gallery is fetched from its cloud. `MCPGalleryManager` here is a deliberately gutted stub that always returns an empty list, so the catalogue has three local sources (config files, installations, saved templates) instead of four. `DECLINED.md`, "MCP gallery in the TUI `/mcp` catalog". |
| **Named "well-known" MCP servers** (`--mcp linear`, `--mcp notion`) | The set of recognised ids is owned by Warp's server. `MCPSpec::WellKnown` is deliberately not ported rather than shipped as a spec that parses and can never resolve. Define servers by JSON instead. |
| **Warp-managed MCP servers (`warp_id`)** | Same reason. The field validates but nothing here can resolve it; an entry using it is malformed by construction. Latent, not usable. |
| **Team- or org-shared MCP servers** ("Available from Phosphor and *team*") | `UserWorkspaces::has_teams()` is permanently `false`; there is no team backend. `DECLINED.md`, "Teams stay stubbed". |
| **Pre-registered OAuth clients for MCP servers** | The static client-id table ships empty, so a remote server whose authorization server does not support dynamic client registration cannot be authorized. |
| **Team- or org-administered skill policy** (`global_skills`) | Warp delivers a `SkillSpec` allowlist over its cloud auth channel; that mechanism was cut. Every skill comes from your own disk. `DECLINED.md`, "AI skills — global-spec filtering". |
| **The Oz platform plugins and their skills** (`oz-report-pr`, `oz-report-artifact`, `oz-report-plan`, `oz-notify-user`, `oz-finish-task`) | They shell out to `oz harness-support …` against warp-server. Removed with their installers; keeping them would make Codex launches demand a plugin whose commands this fork does not implement. `DECLINED.md`, "Oz platform plugins". |
| **`tui-migrate-setup` bundled skill** | It reconciles a separate GUI and TUI settings file. Phosphor shares one app id and one config directory between the two, so both halves of every path pair would render identically. The `tui-settings` skill answers the same question for this architecture instead. |
| **Warp's channel-gated skills** | They ship only on internal channels; the OSS channel matches none of them. |
| **Team workflows** ("Create a New Team Workflow", "Create a New Team Folder") | No team backend. Restoring the menu entries would give you rows that do nothing when clicked. `DECLINED.md`, "Teams stay stubbed" (re-affirmed). |
| **Cloud workflow / notebook / Drive sharing and sync** | Warp Drive here is backed by local SQLite only. Sharing and export are file-based. Nothing syncs to a server, because there is no server. |
| **Warp Packs** | The flag is on but its only behaviour is a folder icon swap, and no code path ever marks a folder as a pack. There is no pack format, installer, or registry. Treat packs as absent. |
| **Suggested rules and suggested workflows (the chips)** | Suggestions arrive only through a cloud protocol action that BYOP cannot synthesise; the producing path is explicitly disabled. The *Suggested Rules* setting still exists and still defaults on — it just has no effect. |
| **`/logout`** | There is no account to log out of; the dispatch target is a documented no-op, so the command is not registered rather than shown as a dead menu row. `DECLINED.md`. |
| **`/voice`** | The transcription backend is cloud and was dropped. `DECLINED.md`. |
| **Agent commit/PR attribution setting** | The client never read it even upstream — Warp's *server* decided whether attribution instructions entered the prompt. `DECLINED.md`. |

---

## 6.8 Reference: every setting, flag and file in this chapter

### Settings (`settings.toml`)

| TOML path | What it does | Default | UI location |
|---|---|---|---|
| `agents.mcp_servers.file_based_mcp_enabled` | Auto-spawn globally-scoped MCP servers found in third-party agents' config files. Phosphor's own global config always spawns regardless. | `false` | Settings → AI → MCP Servers, and the MCP Servers page |
| `agents.knowledge.rules_enabled` | Whether **saved** rules are used during requests. Does not affect project rule files or `~/.agents/AGENTS.md`. | `true` | Settings → Agents → Knowledge → *Rules* |
| `agents.warp_agent.active_ai.rule_suggestions_enabled` | Whether the agent suggests rules after responses. **Inert in this fork.** | `true` | Settings → Agents → Knowledge → *Suggested Rules* |
| `agents.warp_agent.prompt_template_dir` | Directory to hot-load system prompt templates and tool descriptions from. Empty uses the built-ins. | `""` | Settings → AI → *System prompt template directory* |
| `appearance.zero_state.show_mcp` | Whether the TUI zero state shows the MCP section. | `true` | `settings.toml` |
| `appearance.zero_state.show_project_info` | Whether the TUI zero state shows the project path and its discovered rules and skills. | `true` | `settings.toml` |

### Environment variables

| Variable | Effect |
|---|---|
| `WARP_SKILL_DIRS` | Comma-separated extra skills roots, treated as global. **CLI agent runs only.** |
| `ZAP_PROMPT_DIR` | Overrides `agents.warp_agent.prompt_template_dir`. |
| `ZAP_UNSTABLE_FEATURES` | Comma/whitespace-separated list of unstable feature names, or `all` / `*`. The only runtime enable path for the flags below. |
| `WARP_AGENT_CONFIG_FILE` | Default value for `phosphor-oss agent run -f`. |
| `WARP_OUTPUT_FORMAT` | Default value for `--output-format`. |
| `WARP_API_KEY` | Default value for `--api-key`. |

### Feature flags relevant to this chapter

| Flag | Default | How to change it |
|---|---|---|
| `FileBasedMcp` | **on** (`file_based_mcp` in `default`) | rebuild without the cargo feature |
| `McpServer`, `McpOauth`, `MCPGroupedServerContext` | **on** | rebuild |
| `ListSkills`, `BundledSkills`, `SkillArguments`, `OzPlatformSkills` | **on** | rebuild |
| `AIRules` | **on** (`ai_rules`) | rebuild |
| `SuggestedRules` | **on**, but inert | — |
| `AgentModeWorkflows`, `WorkflowAliases`, `DynamicWorkflowEnums`, `BlockToolbeltSaveAsWorkflow` | **on** | rebuild |
| `MarkdownTables`, `BlocklistMarkdownTableRendering`, `MarkdownMermaid`, `BlocklistMarkdownImages` | **on** | rebuild |
| `WarpControlCli` (the `warpctrl` bundled skill) | **off** | `ZAP_UNSTABLE_FEATURES=warp_control_cli` |
| `JupyterNotebookRendering` (`.ipynb` viewer) | **off** | `ZAP_UNSTABLE_FEATURES=jupyter_notebook_rendering` |
| `EditableMarkdownMermaid` | **off** | rebuild with `--features editable_markdown_mermaid`; no runtime path |
| `SuggestedAgentModeWorkflows` | **off** | rebuild with `--features suggested_agent_mode_workflows`; no runtime path, and still inert |
| `MarkdownImages` | **off**, and has no consumer in this tree | nothing to change |
| `WarpPacks` | on, inert | — |

**`DOGFOOD_FLAGS` membership enables nothing at runtime in this fork.** Upstream's
channel binaries pass that list to `with_additional_features`; none of this
repository's binaries do. A flag needs a cargo feature in `app/Cargo.toml`'s
`default`, membership in `RELEASE_FLAGS`, or an entry in `UNSTABLE_FEATURES`
(reachable via `ZAP_UNSTABLE_FEATURES`) to be on.

### Files and directories

| Path | What |
|---|---|
| `~/.phosphor/.mcp.json` | Global MCP server config. |
| `<repo>/.warp/.mcp.json` | Project MCP server config. |
| `~/.claude.json`, `<repo>/.mcp.json` | Claude's MCP configs, also read. |
| `~/.codex/config.toml`, `<repo>/.codex/config.toml` | Codex's MCP config, also read. |
| `~/.agents/.mcp.json`, `<repo>/.agents/.mcp.json` | "Other Agents" MCP config, also read. |
| `~/.phosphor/skills/` | Global Phosphor skills. |
| `<repo>/.warp/skills/` | Project Phosphor skills. |
| `~/.agents/skills/`, `~/.claude/skills/`, `~/.codex/skills/`, `~/.cursor/skills/`, `~/.gemini/skills/`, `~/.copilot/skills/`, `~/.factory/skills/`, `~/.github/skills/`, `~/.opencode/skills/` | Other providers' global skills, and the same names under a repo for project scope. |
| `~/.agents/AGENTS.md` | The only global rules **file**. |
| `<any dir>/WARP.md`, `AGENTS.md`, `CLAUDE.md` | Project rules, one per directory, in that priority order. |
| `~/.local/share/phosphor/workflows/` (Linux) · `~/.phosphor/workflows/` (macOS) · `%APPDATA%\phosphor\Phosphor\data\workflows\` (Windows) | Personal workflows. |
| `<repo>/.warp/workflows/` | Repository workflows. |
| `~/.phosphor/prompts/` | Suggested location for system prompt template overrides. |
| `~/.config/phosphor/settings.toml` (Linux) · `~/.phosphor/settings.toml` (macOS) · `%LOCALAPPDATA%\phosphor\Phosphor\config\settings.toml` (Windows) | The settings file. |
| `~/.local/state/phosphor/mcp/<uuid>.log` (Linux) · `~/Library/Application Support/dev.phosphor.Phosphor/mcp/<uuid>.log` (macOS) · `%LOCALAPPDATA%\phosphor\Phosphor\data\logs\mcp\<uuid>.log` (Windows) | Per-MCP-server logs. |

### Slash commands in this chapter

| Command | Surface | What it does |
|---|---|---|
| `/add-mcp` | GUI only | Open the pane for adding an MCP server. |
| `/open-mcp-servers` | GUI only | Open the MCP Servers settings page. |
| `/mcp` | TUI only | View and manage MCP servers. |
| `/view-logs` | TUI only | Bundle the app's logs into a zip. |
| `/skills` | GUI + TUI | Pick a skill; inserts `/<name> ` into the input. |
| `/open-skill` | GUI | Open a skill's `SKILL.md` in the editor. |
| `/<skill-name> [args]` | GUI + TUI | Invoke that skill. (`$<skill-name>` in a Codex session.) |
| `/init` | GUI + TUI | Generate or update an `AGENTS.md`. |
| `/add-rule` | GUI | Add a saved global rule. |
| `/open-rules` | GUI | Open the Rules pane. |
| `/open-project-rules` | GUI | Open the project rules file (opens `WARP.md`, see the note in §6.4). |
| `/prompts` | GUI + TUI | Search saved agent-mode workflows. |

### CLI

| Command | What it does |
|---|---|
| `phosphor-oss mcp list` | List installed MCP servers (UUID, name). |
| `phosphor-oss agent run --mcp <SPEC>` | Start MCP servers for this run. `<SPEC>` is a JSON file path or inline JSON; repeatable. |
| `phosphor-oss agent run --mcp-server <UUID>` | Legacy, hidden, and unresolvable here. Do not use. |
| `phosphor-oss agent run --skill <SPEC>` | Use a skill as the run's base prompt. |
| `phosphor-oss agent run -f <PATH>` | Load `name`, `model_id`, `base_prompt`, `mcp_servers`, `host`, `computer_use_enabled` from JSON or YAML. |
| `phosphor-oss --output-format {pretty,json,ndjson,text}` | Global output format; default `pretty`. |

There is no `skill`, `rules`, `workflow` or `schedule` subcommand. `provider` is
hidden from `--help` (its feature flag is off) but still parses.

<!-- SOURCES

## Binary / channel / paths
crates/warp_core/src/channel/mod.rs:38-47   Channel::cli_command_name; Oss => "phosphor-oss"
crates/warp_core/src/channel/state.rs:38-57 ChannelState::init: Channel::Oss, app id dev.phosphor.Phosphor
crates/warp_core/src/channel/state.rs:261-276 url_scheme: Oss => "phosphor"
app/Cargo.toml:25-28                        [[bin]] name = "phosphor-oss"
app/src/bin/phosphor_oss.rs:26-42           Channel::Oss, logfile "phosphor.log", mcp_static_config: None
app/src/workspace/cli_install.rs:11-13      CLI symlink /usr/local/bin/<cli_command_name>
crates/warp_core/src/paths.rs:37-47         base_warp_config_dir_name: Oss => ".phosphor"
crates/warp_core/src/paths.rs:62-90         warp_home_config_dir / warp_home_skills_dir / warp_home_prompts_dir / warp_home_mcp_config_file_path
crates/warp_core/src/paths.rs:132-158       data_dir / config_local_dir
crates/warp_core/src/paths.rs:174-184       state_dir
crates/warp_core/src/paths_tests.rs:5-116   concrete per-platform paths for data/config/cache/state, ~/.phosphor/skills, ~/.phosphor/.mcp.json
app/src/settings/mod.rs:648-654             user_preferences_toml_file_path = config_local_dir()/settings.toml
crates/warp_core/src/paths.rs:30            WARP_CONFIG_DIR = ".warp"

## MCP
crates/warp_cli/src/mcp.rs:8-11             MCPCommand::List is the only subcommand
crates/warp_cli/src/mcp.rs:17-24            MCPSpec has only Uuid and Json
crates/warp_cli/src/mcp.rs:47-77            UUID, then file path, then inline JSON
crates/warp_cli/src/lib.rs:353-373          CliCommand: Agent, MCP, Model, Whoami, Provider
crates/warp_cli/src/lib.rs:225-238          "Examples: <bin> mcp list"
crates/warp_cli/src/agent.rs:11-25          OutputFormat json/ndjson/pretty/text, default Pretty
crates/warp_cli/src/lib.rs:265-285          GlobalOptions --api-key / --output-format, env vars
crates/warp_cli/src/agent.rs:335-347        --mcp (repeatable), --mcp-server (legacy, hidden)
crates/warp_cli/src/config_file.rs:5-15     -f/--file, env WARP_AGENT_CONFIG_FILE
app/src/ai/agent_sdk/mcp.rs:16-45           `mcp list` prints UUID + Name from get_all_runnable_mcp_servers
app/src/ai/agent_sdk/mcp_config.rs:26-60    warp_id TRAP comment; WellKnown deliberately not ported (lines 40-48)
app/src/ai/agent_sdk/mcp_config.rs:136-215  validate_server_config: exactly one of warp_id/command/url; args/env/headers rules
app/src/ai/agent_sdk/mcp_config.rs:84-90    duplicate server name is an error
app/src/ai/agent_sdk/mcp_config.rs:92-102   optional outer braces
app/src/ai/agent_sdk/config_file.rs:8-30    AgentConfigSnapshotFile, deny_unknown_fields
app/src/ai/agent_sdk/config_file.rs:91-93   supported keys list
app/src/ai/agent_sdk/config_file.rs:139-141 precedence CLI > file > default
app/src/ai/mcp/mod.rs:69-115                MCPProvider, display_name "Phosphor", home/project config paths
app/src/ai/mcp/mod.rs:53-58                 home_config_file_path: Zap => warp_home_mcp_config_file_path()
app/src/ai/mcp/mod.rs:191-209               JSONTransportType: command/args/env/working_directory | url (alias serverUrl)/headers
app/src/ai/mcp/templatable.rs:66-108        wrapper keys /mcp/servers,/servers,/mcpServers,/mcp_servers; strict vs permissive
app/src/ai/mcp/parsing.rs:238-255           from_config_file_json uses the strict form
app/src/ai/mcp/parsing.rs:264-280           from_user_json uses the permissive form
app/src/settings_view/mcp_servers/edit_page.rs:504,773 GUI editor uses from_user_json
app/src/ai/mcp/parsing.rs:51-95             Codex TOML schema translated to Phosphor JSON
app/src/ai/mcp/file_mcp_watcher.rs:29-49    ${VAR} regex; home_subdir_to_watch
app/src/ai/mcp/file_mcp_watcher.rs:176-213  Zap config watched via warp_managed_mcp_config_path, other providers via home dirs
app/src/ai/mcp/file_mcp_watcher.rs:600-640  project roots check both home_config_path and project_config_path; substitute_env_vars errors on missing var
app/src/warp_managed_paths_watcher.rs:75-80 warp_managed_mcp_config_path root = home, config = ~/.phosphor/.mcp.json
app/src/ai/mcp/file_based_manager.rs:381-421 is_global_warp_server / scope_for_source
app/src/ai/mcp/file_based_manager.rs:424-465 spawn rules: global Zap always; global third-party per toggle; project never; TUI never
app/src/settings/ai.rs:2466-2476            file_based_mcp_enabled default false, toml agents.mcp_servers.file_based_mcp_enabled
app/src/ai/mcp/logs.rs:1-38                 log path <state>/mcp/<uuid>.log, 10 MiB x 5 rotations
crates/simple_logger/src/manager.rs:33-47   resolve_log_path; Windows inserts WARP_LOGS_DIR ("logs")
app/src/ai/mcp/gallery.rs:97-124            MCPGalleryManager gutted, gallery always empty
DECLINED.md:90                              MCP gallery declined; three local catalogue sources only
app/src/tui/mcp.rs:94-119                   TuiMcpServerSource labels: CLI local / saved template / "<provider> global" / "<provider> · <root>"
app/src/tui/mcp.rs:131-161                  TuiMcpServerStatus; can_log_out
app/src/tui/mcp.rs:208-215                  TuiMcpAction: Enable/Start/Stop/Retry/LogOut/ReopenAuthorization
crates/warp_tui/src/mcp_menu.rs:1-8         Enter = primary action, Ctrl+R = log out; one row per server
crates/warp_tui/src/mcp_install_flow.rs:1-33 free-text template variables are all masked
app/src/ai/mcp/file_based_manager.rs:79-96  config_diagnostics: one row per unhealthy config file
app/src/ai/mcp/templatable_manager/oauth.rs:245-249 redirect uri <scheme>://mcp/oauth2callback
app/src/ai/mcp/templatable_manager/oauth.rs:370-388 dynamic registration first; static client-id table is the fallback
app/src/search/slash_command_menu/static_commands/commands.rs:21-27  ADD_MCP "/add-mcp"
app/src/search/slash_command_menu/static_commands/commands.rs:250-257 OPEN_MCP_SERVERS
app/src/search/slash_command_menu/static_commands/commands.rs:511-521 MCP "/mcp", documented TUI-only
app/src/search/slash_command_menu/static_commands/commands.rs:1125-1139 test: /add-mcp is GUI-only
app/src/search/slash_command_menu/static_commands/mod.rs:344-357     is_tui_only set
app/src/search/slash_command_menu/static_commands/mod.rs:360-400     supports_tui set (no /add-mcp, no /open-mcp-servers)
app/src/terminal/input/slash_commands/mod.rs:509-511 /add-mcp -> OpenAddMCPPane
app/src/workspace/mod.rs:1421-1435          editable binding workspace:open_mcp_servers, no default keystroke
app/src/util/bindings.rs                    grep: no default keystroke entry for OpenMCPServerCollection
app/i18n/en/warp.ftl:671-752                MCP settings strings, "Detected from { $provider }", View logs, Auto-spawn toggle
app/src/settings_view/mcp_servers/list_page.rs:1116-1131 "Learn more." hyperlink has an EMPTY href
app/src/settings_view/ai_page.rs:6386-6400  file-based MCP toggle in Settings > AI

## Markdown / Mermaid / notebooks
app/Cargo.toml:480-662                      the `default` feature list
app/Cargo.toml:616-619                      markdown_tables, markdown_mermaid, blocklist_markdown_images, blocklist_markdown_table_rendering all in default
app/Cargo.toml:667                          editable_markdown_mermaid declared but NOT in default
app/src/lib.rs:3048-3058                    cfg mapping of the markdown flags
crates/warp_features/src/lib.rs:464-482     MarkdownImages / MarkdownMermaid / EditableMarkdownMermaid / MarkdownTables / JupyterNotebookRendering / Blocklist*
crates/warp_features/src/lib.rs:848         MarkdownImages in DOGFOOD_FLAGS only
crates/warp_features/src/lib.rs:868         EditableMarkdownMermaid in DOGFOOD_FLAGS only
crates/warp_features/src/lib.rs:881         JupyterNotebookRendering in DOGFOOD_FLAGS
crates/warp_features/src/lib.rs:806-828     DOGFOOD_FLAGS enables nothing at runtime in this fork
crates/warp_features/src/lib.rs:886-891     PREVIEW_FLAGS (MarkdownTables)
crates/warp_features/src/lib.rs:894-924     RELEASE_FLAGS (BlocklistMarkdownTableRendering)
app/src/lib.rs:3326-3352                    ZAP_UNSTABLE_FEATURES parsing, "all"/"*"
app/src/lib.rs:3355-3439                    UNSTABLE_FEATURES table (7 tokens)
grep MarkdownImages across the tree         only warp_features declares it; zero consumers
app/src/ai/agent/util.rs:268-290            ```mermaid fence -> AIAgentTextSection::MermaidDiagram
app/src/ai/blocklist/block/view_impl/common.rs:2122-2140 mermaid section requires BlocklistMarkdownImages + MarkdownMermaid, falls back to raw markdown
app/src/ai/blocklist/block/view_impl/common.rs:1501-1520 mermaid lightbox
app/src/ai/blocklist/block/view_impl/common.rs:1818-1832 blocklist image loading gated on BlocklistMarkdownImages
app/src/ai/agent/mod.rs:1501-1516           agent replies parsed with GFM tables when MarkdownTables is on
app/src/ai/agent/util.rs:40-56              BlocklistMarkdownTableRendering picks structured vs legacy table
crates/editor/src/content/text.rs:730-737   mermaid code-block type gated on MarkdownMermaid
Cargo.toml:201                              mermaid_to_svg = git warpdotdev/mermaid-to-svg
<cargo git checkout>/src/lib.rs:1-30,36-150 native Rust renderer, diagram families
<cargo git checkout>/src/lib.rs:156-159     is_mermaid_diagram: "mermaid" or "mermaid <params>"
<cargo git checkout>/src/parser.rs:11-12,142-154 graph / flowchart supported
app/src/notebooks/file/mod.rs:79-84         MarkdownDisplayMode {Rendered, Raw}
app/src/notebooks/file/mod.rs:268-272       default_mermaid_display_mode = Rendered
app/src/notebooks/file/mod.rs:296-318       markdown_display_mode defaults to Rendered
app/src/notebooks/editor/notebook_command.rs:632-660 per-block Raw/Rendered buttons on mermaid
app/src/util/openable_file_type.rs:77-83    renders_in_warp_notebook_viewer: markdown always, ipynb behind the flag
crates/warp_util/src/file_type.rs:14-20,132-150 md/markdown + README/CHANGELOG/LICENSE; .ipynb
app/src/notebooks/mod.rs:28-50              notebooks are object-store backed

## Skills
crates/ai/src/skills/skill_provider.rs:104-149 SKILL_PROVIDER_DEFINITIONS, order = precedence
crates/ai/src/skills/skill_provider.rs:152-169 provider_rank; home_skills_path (Zap -> warp_home_skills_dir)
crates/ai/src/skills/read_skills.rs:89-122     only direct children, only SKILL.md
crates/ai/src/skills/read_skills.rs:7-80       WARP_SKILL_DIRS: comma separated, ~ expanded, forced SkillScope::Home
app/src/ai/agent_sdk/driver.rs:973-993         WARP_SKILL_DIRS consumed by the CLI driver only
crates/ai/src/skills/parser.rs:23-82           frontmatter regex; flat string->string map only
crates/ai/src/skills/parse_skill.rs:14,34-55   name/description optional; 512-char cap on DERIVED descriptions only
crates/ai/src/skills/parse_skill.rs:149-221    directory-name default; first-paragraph default
grep allowed-tools / allowed_tools             no handling anywhere
app/src/ai/skills/skill_manager.rs:121-190     home always in scope; project dirs must be ancestors of cwd within the repo
app/src/ai/skills/skill_utils.rs:39-86         dedup key (name, dir), provider-rank tie-break, sorted for cache stability
DECLINED.md:178                                the (name, dir) dedup decision
app/src/ai/blocklist/controller/input_context.rs:104-120 skills list pushed every round when ListSkills is on
app/src/ai/agent_providers/prompts/partials/skills.j2    the <available_skills> block; read_skill instruction
app/src/ai/agent_providers/tools/skill.rs:31-48          read_skill tool takes {name}
app/src/ai/blocklist/action_model/execute/read_skill.rs:41-48 read_skill always autoexecutes
app/src/ai/agent_providers/chat_stream.rs:2531-2542       the InvokeSkill user message format
app/src/terminal/input/slash_command_model.rs:518-541     "/<name> <args>" split on the first space
app/src/terminal/cli_agent.rs:388-393                     "$" prefix in Codex sessions
app/src/search/slash_command_menu/static_commands/commands.rs:59-75 EDIT_SKILL "/open-skill", INVOKE_SKILL "/skills"
app/src/search/slash_command_menu/static_commands/commands.rs:774-777 both gated on ListSkills
app/src/search/slash_command_menu/static_commands/commands.rs:779-783 /pr-comments NOT registered when PRCommentsSkill is on
app/src/terminal/input.rs:3962-3971                       /skills inserts "/<name> "
app/src/ai/blocklist/controller/slash_command.rs:263-281  SkillArguments gate; args dropped when off
crates/warp_features/src/lib.rs:593                       flag doc claims $ARGUMENTS substitution; no such code exists
app/Cargo.toml:456,580,582,592,606,732,913,917            list_skills / bundled_skills / oz_platform_skills / skill_arguments in default
resources/bundled/skills/*/SKILL.md                       the 11 bundled skills' frontmatter
resources/bundled/mcp_skills/figma/*                      8 Figma skills
app/src/ai/skills/bundled.rs:39-67,607-618                activation: Always / RequiresFile / RequiresFeature(WarpControlCli) / RequiresMcp(Figma)
app/src/ai/skills/bundled.rs:223-228,397,420-423          read in place from bundled_resources_dir; never copied to $HOME
app/src/ai/skills/bundled.rs:495-565                      template variables
app/src/ai/skills/bundled.rs:584-600                      why tui-migrate-setup is not ported
app/src/ai/skills/skill_manager.rs:388-409,459-482        file-backed skills win by name; inventory lists only file-backed
script/copy_conditional_skills:45-52                      oss channel matches no gated skill
app/src/lib.rs:3429                                       ZAP_UNSTABLE_FEATURES=warp_control_cli
crates/warp_cli/src/agent.rs:311-323                      --skill <SPEC>
crates/warp_cli/src/skill.rs:33-49,79-98                  SPEC grammar
crates/warp_cli/src/agent.rs:321                          stale reference to `oz schedule create --skill`
app/src/ai/agent_sdk/mod.rs:147-150                       --skill rejected unless OzPlatformSkills
DECLINED.md:88                                            global-spec skill filtering removed
DECLINED.md:195-199                                       Oz platform plugins removed
DECLINED.md:212                                           remote_server bundled skills SHIPPED (row reversed)
crates/remote_server/src/setup.rs:434,462,471             remote bundled resources
app/src/ai/skills/mod.rs:70-81                            explicit refusal when the remote bundle is absent

## Rules
crates/ai/src/project_context/model.rs:12-25     RULES_FILE_PATTERN = WARP.md, AGENTS.md, CLAUDE.md; order = priority
crates/ai/src/project_context/model.rs:145-155   respected_rule(): one file per directory
crates/ai/src/project_context/model.rs:41-42     MAX_SCAN_DEPTH 3, MAX_FILES_TO_SCAN 5000
crates/ai/src/project_context/model.rs:64-65     MAX_WALK_DEPTH 6, FAST_PATH_BUDGET 20ms
crates/ai/src/project_context/model.rs:249-266   active = target path starts_with the rule file's dir; accumulate
crates/ai/src/project_context/model.rs:952-981   layer_global_rules: global first, project appended, no override
crates/ai/src/project_context/model.rs:1017-1083 synchronous findUp fast path
crates/ai/src/project_context/global_rules.rs:30-58 the only global source is ~/.agents/AGENTS.md
app/src/lib.rs:2305-2307                         global rules indexed at startup
app/src/ai/project_rules_persister.rs:80-87      project index fires on git repo detection
app/src/ai/agent_providers/prompts/partials/project_rules.j2 the "# Project rules" block
app/src/ai/agent_providers/prompts/partials/user_rules.j2    the "# User rules" block
app/src/ai/agent_providers/prompts/partials/footer.j2        includes both, plus skills/env/plan_mode
app/src/ai/agent_providers/prompts/system/local.j2           includes ONLY partials/env.j2 - no footer
app/src/ai/agent_providers/prompt_renderer.rs:600-604        pick_template: any Ollama provider -> system/local.j2
app/src/ai/agent_providers/prompt_renderer.rs:803-822        additional_rule_paths discarded before rendering
app/src/ai/agent/api.rs:257-269                  saved rules from AIFact::Memory, sorted for cache stability
app/src/settings/ai.rs:2226-2234                 memory_enabled -> agents.knowledge.rules_enabled, default true
app/src/settings/ai.rs:1975-1984                 rule_suggestions_enabled -> agents.warp_agent.active_ai.rule_suggestions_enabled, default true
app/src/settings_view/ai_page.rs:1655-1663,6462-6603 Knowledge section: Rules / Suggested Rules / Manage rules
app/src/ai/blocklist/passive_suggestions/maa.rs:389-400 ShowSuggestions cannot be synthesized under BYOP
app/src/ai/blocklist/suggested_agent_mode_workflow_modal.rs:236-240 same, for workflow suggestions
app/src/search/slash_command_menu/static_commands/commands.rs:230-248,287-294 /init, /open-project-rules, /open-rules
app/src/search/slash_command_menu/static_commands/commands.rs:90-97 /add-rule
app/src/terminal/view.rs:722                     WARP_MD_PATH = "WARP.md"
app/src/terminal/view.rs:26265-26277             OpenProjectRulesPane always opens <cwd>/WARP.md
app/i18n/en/warp.ftl:2653,2666,2667,2672         the four command descriptions
app/src/ai/agent_providers/prompts/commands/init_project.j2 the /init prompt
app/src/ai/facts/view/rule.rs:135,189-210,613-650 Rules pane tabs; global file not listed

## Workflows
app/src/workflows/workflow.rs:9-43               Workflow enum: AgentMode (tagged) / Command (untagged)
app/src/workflows/workflow.rs:285-293,344-386    Argument: name, type, description, default_value
app/src/workflows/command_parser.rs:18-21        {{arg}} and {{{escaped}}}
app/src/workflows/local_workflows.rs:19-21       workflows_dir(base) = base/workflows
app/src/workflows/local_workflows.rs:181-191     project workflows = <repo>/.warp/workflows
app/src/user_config/mod.rs:167-180               base_dir() = data_dir()
app/src/user_config/util.rs:17,70-107,191-212    .yaml/.yml, multi-doc, recursive walk, parse failures skipped
app/src/user_config/native.rs:94-98,178-183      live reload
app/src/util/bindings.rs:291                     CustomAction::Workflows = ctrl-shift-R
app/src/terminal/view/init.rs:479-489            terminal:toggle_workflows_modal = cmd/ctrl-shift-S, requires a selected block
app/src/terminal/input.rs:6629-6660              selecting a workflow replaces the buffer; AI vs Shell input type
app/src/terminal/input.rs:12153-12180            workflow aliases expand on Enter
app/src/workflows/aliases.rs:21-30               aliases stored as a private setting, no TOML path
app/src/search/data_source.rs:239-243,320-324    palette filters "workflows" and "prompts"
crates/warp_tui/src/prompts_menu.rs:1-9          TUI inserts raw query text, no argument box
app/src/tui_export.rs:420-434                    tui_list_prompts returns agent-mode workflows only
app/Cargo.toml:486,494,496,499,503               workflow cargo features in default
app/Cargo.toml:673                               suggested_agent_mode_workflows declared, NOT in default
app/src/lib.rs:2964-2995,3022-3035               cfg mapping for the workflow/rules flags
crates/warp_features/src/lib.rs:833              AgentModeWorkflows also in DOGFOOD_FLAGS (inert)
DECLINED.md:84,186                               team workflows / has_teams() permanently false
app/src/drive/items/folder.rs:48-55              WarpPacks only swaps a folder icon
app/src/cloud_object/update_manager.rs:1017-1018 is_warp_pack is never set true (TODO INT-789)

## Prompt template overrides
app/src/ai/agent_providers/prompt_renderer.rs:56          PROMPT_DIR_ENV = "ZAP_PROMPT_DIR"
app/src/ai/agent_providers/prompt_renderer.rs:64-230      EMBEDDED and EMBEDDED_RAW tables
app/src/ai/agent_providers/prompt_renderer.rs:333-362     seed_dir never overwrites
app/src/ai/agent_providers/prompt_renderer.rs:364-400     default_prompts_dir, OverrideStatus, env takes priority
app/src/settings/ai.rs:2736-2744                          prompt_template_dir, toml agents.warp_agent.prompt_template_dir, default ""
app/i18n/en/warp.ftl:1054-1060                            settings strings and status lines

## TUI zero state
app/src/settings/tui_zero_state.rs:189-206                show_project_info, show_mcp; both default true

-->
