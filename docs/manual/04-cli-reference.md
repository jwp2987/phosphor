# 4. Command-line reference

Phosphor ships one executable that is three things at once: the GUI terminal,
a set of hidden worker processes it re-executes itself as, and a scriptable
command-line interface. This chapter documents the third of those — the
subcommands you can type at a shell to run an agent headlessly, list the models
and MCP servers Phosphor knows about, pass messages between local agents, and
generate shell completions. Everything here is local: there is no account to
log in to, no server to talk to, and no hosted run to poll.

---

## 4.1 What the command is actually called

This is the first thing to get right, because the source tree, the packages and
the help output do not all agree.

| where you look | name |
|---|---|
| Cargo binary target (`app/Cargo.toml`) | `phosphor-oss` |
| `Channel::cli_command_name()` for the shipped `Oss` channel | `phosphor-oss` |
| clap's internal command `name` | `oz` |
| clap's `display_name` | `Phosphor` |
| Debian package `phosphor` (the app) symlinks | `/usr/bin/phosphor` |
| Debian package `phosphor-cli` symlinks | `/usr/bin/phosphor-oss` |
| macOS bundle CLI wrapper | `…/Contents/Resources/bin/phosphor-oss` |
| GUI's "install CLI" action | `/usr/local/bin/phosphor-oss` |

**What you type depends on what you installed.**

* **The Linux desktop package** (`.deb`/`.rpm`/Arch) puts only **`phosphor`** on
  your `PATH` — a symlink (Arch: a wrapper script) to `/opt/phosphor/phosphor-oss`.
  `phosphor-oss` itself is *not* on `PATH` from that package
  (`resources/linux/debian/app/postinst.template:13`,
  `resources/linux/rpm/app/warp.spec.template:62`). Type `phosphor`.
* **The separate CLI package/tarball** (`phosphor-cli`) installs
  `/usr/bin/phosphor-oss`.
* **macOS** exposes `phosphor-oss` — from the bundle's
  `Contents/Resources/bin/`, or at `/usr/local/bin/phosphor-oss` after the GUI's
  "install CLI" action.

Both names are the same executable and accept the same subcommands. The name `oz`
is Warp's upstream name for this CLI; it survives only as clap's internal command
identifier and never as an installed command. Nothing on your `PATH` after a
Phosphor install is called `oz`, `warp`, or `zap` (the string `oz` does survive
in one place on disk: the agent-mailbox directory, §4.5).

Two consequences worth knowing:

* Help output, examples and generated completions are built from **argv\[0\]**,
  not from the internal name. Invoke it as `phosphor` and the examples in
  `--help` say `phosphor`; invoke it as `phosphor-oss` and they say
  `phosphor-oss`. There is one exception. `oz` leaks into the `Usage:` line of
  the help that a **bare invocation** prints in CLI mode — the CLI-only
  (`standalone`) build, an argv\[0\] starting with `oz`, or `WARP_CLI_MODE`
  set. That path calls clap's `print_help()` on a command clap never parsed
  argv with, so clap has no `bin_name` to substitute and falls back to the
  internal `name`, printing `Usage: oz [OPTIONS] [COMMAND]`. An explicit
  `--help` (or any real parse) goes through clap's argument parser, which does
  set `bin_name` from argv\[0\], so `phosphor-oss --help` says
  `Usage: phosphor-oss …` as expected. The `Examples:` block is built from
  argv\[0\] on both paths.
* Some **help strings in the source still say `warp` or `oz`** and were not
  rebranded: the `completions` help shows `path/to/warp completions bash`, and
  `--model`'s help says "Use `warp model list`". Read those as `phosphor-oss`.
  See §4.13 for the one that is actually wrong rather than merely stale.

Throughout this chapter examples use `phosphor-oss`. On a Linux desktop-package
install, substitute `phosphor`.

### Running the CLI vs. launching the GUI

The same executable decides what to do from its arguments:

* `phosphor-oss <subcommand> …` — runs that subcommand headlessly and exits.
* `phosphor-oss` with no subcommand — launches the GUI terminal.
* `phosphor-oss` with no subcommand, **when the binary came from the CLI-only
  package** (built with the `standalone` feature), or when argv\[0\] starts with
  `oz`, or when `WARP_CLI_MODE` is set to anything — prints help and exits
  instead of launching a window.

`agent run` is the one subcommand that can put a window on screen, via the
hidden `--gui` flag; without it, `agent run` is headless.

---

## 4.2 Global options

These are accepted before or after any subcommand.

| flag | what it does | default |
|---|---|---|
| `--api-key <KEY>` | Sets a local placeholder credential. **Inert in Phosphor** — the value is accepted (the `api_key_authentication` cargo feature is in the default build) and stored as a local `Credentials::ApiKey`, but nothing reads it: there is no server to authenticate against. It does *not* set your AI provider key; see §4.7. | unset |
| `--output-format <FORMAT>` | `pretty`, `text`, `json`, or `ndjson`. See §4.6. | `pretty` |
| `--debug` | Enable debug logging. | off |
| `-h`, `--help` | Help for the command or subcommand. | — |
| `-V`, `--version` | Prints the build's release tag, or `<unknown>` for an untagged local build. | — |

`--api-key` reads `WARP_API_KEY` and `--output-format` reads
`WARP_OUTPUT_FORMAT` from the environment. `--api-key`'s resolved value is
deliberately **not** echoed in `--help`, so exporting `WARP_API_KEY` will not
leak the key into your scrollback.

Global flags may precede a subcommand:

```console
$ phosphor-oss --output-format json model list
$ phosphor-oss --debug agent run -p "explain this repo"
```

---

## 4.3 Subcommand map

```
phosphor-oss
├── agent
│   ├── run  (alias: r)          run an agent headlessly
│   ├── profile list             list agent profiles
│   ├── list                     (present, but disabled — see §4.13)
│   └── message
│       ├── send                 write to a local agent's on-disk mailbox
│       └── list                 read a local agent's on-disk mailbox
├── mcp list                     list runnable MCP servers
├── model list                   list selectable model IDs
├── whoami                       print the local placeholder identity
├── completions [SHELL]          emit shell completion script
└── dump-debug-info              print environment/GPU diagnostics
```

Not shown, because they are hidden worker modes Phosphor re-executes itself as
and are not for interactive use: `terminal-server`, `minidump-server`,
`remote-server-proxy`, `remote-server-daemon`, `ripgrep-search`, and (only in
builds compiled with the `plugin_host` feature, which the default build is not)
`plugin-host`. They are documented here as internal and are omitted from the
reference tables below.

There is also a hidden `--warpctrl` flag that switches the binary to an entirely
separate "local control" argument parser. That surface is gated off by default
and is not covered in this chapter.

---

## 4.4 How do I run an agent from the shell?

`agent run` starts a headless agent in a working directory, streams its progress
to stdout, and exits when the conversation finishes.

```console
$ phosphor-oss agent run --prompt "Add a --version flag and a test for it"
```

`-p` is the short form of `--prompt`, and `r` is a visible alias for `run`:

```console
$ phosphor-oss agent r -p "summarise the open TODOs"
```

**One of `--prompt`, `--saved-prompt`, or `--skill` is required.** `--prompt`
and `--saved-prompt` are mutually exclusive; either may be combined with
`--skill`, in which case the skill supplies the base context and the prompt
supplies the task.

### Choosing where it runs

```console
$ phosphor-oss agent run -C ~/src/myproject -p "run the tests and fix what fails"
```

`-C` / `--cwd` is resolved to an absolute path before the run starts; if the
path does not exist the run fails immediately with `Unable to resolve <path>`.
With no `--cwd`, the agent runs in your current directory.

### Choosing a model

```console
$ phosphor-oss model list
$ phosphor-oss agent run --model <MODEL_ID> -p "refactor the parser"
```

`--model` takes an ID from `model list` and is validated before the run starts,
so a typo fails fast rather than halfway through. See §4.7.

### Naming the run

`--name` / `-n` labels the agent task. If you also pass `--skill`, the skill's
name is used when `--name` is absent; a config file's `name` is the last
fallback.

### Keeping the session alive afterwards

`--idle-on-complete [DURATION]` keeps the agent's session open after the
conversation completes, for follow-up interaction. Passing the flag with no
value means **45 minutes**; passing a value overrides it:

```console
$ phosphor-oss agent run --idle-on-complete 10m -p "start a review"
```

This flag is functional but **hidden from `--help`**. Durations use
`humantime` syntax (`30s`, `10m`, `2h`).

### Attaching MCP servers to a single run

`--mcp <SPEC>` may be repeated. A spec is one of:

* a UUID of a server already configured in Phosphor (as printed by `mcp list`),
* a path to a JSON file containing MCP configuration, or
* inline JSON.

```console
$ phosphor-oss agent run \
    --mcp ./mcp-servers.json \
    --mcp '{"docs":{"command":"my-docs-mcp","args":["--stdio"]}}' \
    -p "look up the API docs and update the call site"
```

### Reusing a configuration file

`-f` / `--file` loads a YAML or JSON file of run defaults. The extension decides
the parser (`.json` → JSON, `.yml`/`.yaml` → YAML, anything else → JSON then
YAML). Unknown keys are **rejected**, and the accepted keys are exactly:

| key | type | meaning |
|---|---|---|
| `name` | string | run name |
| `model_id` | string | base model |
| `base_prompt` | string | prepended to your `--prompt` |
| `mcp_servers` | object | unwrapped `{ "<name>": { … } }` server map; an entry with a `warp_id` field is treated as a UUID reference |
| `host` | string | worker host |
| `computer_use_enabled` | bool | computer-use override |

```yaml
# review.yaml
name: nightly-review
model_id: <MODEL_ID>
base_prompt: |
  You are reviewing a Rust workspace. Prefer minimal diffs.
```

```console
$ phosphor-oss agent run -f review.yaml -p "review today's commits"
```

Merge precedence is **file < command line < skill**, so `--model` on the command
line beats `model_id` in the file, and a skill's instructions beat the file's
`base_prompt`.

`-f` also reads `WARP_AGENT_CONFIG_FILE` from the environment.

### Using a skill as the base prompt

```console
$ phosphor-oss agent run --skill code-review -p "review the staged diff"
```

A skill spec is `skill_name`, `repo:skill_name`, or `org/repo:skill_name`, and
the identifier may instead be a full path to a `SKILL.md`. Bare names are
searched, in precedence order, under `.agents/skills/`, `.warp/skills/`,
`.claude/skills/`, and `.codex/skills/`.

### Selecting an agent profile

`--profile <ID>` is accepted but see the caveat in §4.12 — the IDs printed by
`agent profile list` are not, in a normal Phosphor install, in the form
`--profile` requires.

---

## 4.5 How do I pass messages between local agents?

Phosphor replaces upstream Warp's server-backed cross-run mailbox with a plain
on-disk one. It is a directory of JSON files keyed by run ID, readable and
writable by any process running as you, with **no app instance and no network
listener required**.

Each agent process is given its own run ID in `OZ_RUN_ID`, and (for child
agents) its parent's in `OZ_PARENT_RUN_ID`. Those are the IDs you address.

```console
$ phosphor-oss agent message send \
    --sender-run-id "$OZ_RUN_ID" \
    --to "$OZ_PARENT_RUN_ID" \
    --subject "tests green" \
    --body "All 41 integration tests pass on the branch."
Sent message 1f0c2b6e-… to 8c31e0aa-…
```

`--to` may be repeated, or comma-delimited, to fan out:

```console
$ phosphor-oss agent message send --sender-run-id "$OZ_RUN_ID" \
    --to run-a,run-b --subject "status" --body "blocked on review"
```

Reading a mailbox:

```console
$ phosphor-oss agent message list "$OZ_RUN_ID"
╭──────────────┬──────────┬────────────┬──────────────────────┬───────────────────────────╮
│ Message ID   ┆ From     ┆ Subject    ┆ Body                 ┆ Sent At                   │
╞══════════════╪══════════╪════════════╪══════════════════════╪═══════════════════════════╡
│ 1f0c2b6e-…   ┆ 8c31e0aa ┆ tests green┆ All 41 integration … ┆ 2026-08-29T09:12:44+00:00 │
╰──────────────┴──────────┴────────────┴──────────────────────┴───────────────────────────╯
```

`-L` / `--limit` caps how many are returned, keeping the **most recent**;
the default is `25` and the value must be at least 1. Messages are listed oldest
first. A run with no mailbox yet lists nothing rather than erroring.

Messages live under the per-user state directory, at
`<state dir>/oz/agent-mailbox/<run-id>/` — on Linux that is
`~/.local/state/phosphor/oz/agent-mailbox/`; macOS and Windows fall back to the
data directory. `OZ_AGENT_MAILBOX_ROOT` overrides the root, which is useful for
isolating a test harness.

Writes are atomic (write-then-rename), so a reader never sees a half-written
message. Files are named with a zero-padded nanosecond timestamp so a plain
directory sort is send order.

---

## 4.6 How do I get machine-readable output?

`--output-format` takes four values.

| value | shape |
|---|---|
| `pretty` (default) | Unicode box-drawn table for list commands; human prose for `agent run`. |
| `text` | Tab-separated, column-aligned table for list commands; the same prose as `pretty` for `agent run`. |
| `json` | For list commands: a single JSON array on one line, **with no trailing newline**. For `agent run`: newline-delimited events (identical to `ndjson`). |
| `ndjson` | One JSON object per line. |

```console
$ phosphor-oss --output-format text model list
MODEL ID
claude-…
gpt-…

$ phosphor-oss --output-format ndjson mcp list
{"uuid":"3f2a…","name":"filesystem"}
{"uuid":"9c07…","name":"github"}
```

Agent runs emit a typed event stream under `json`/`ndjson`. Every record has a
`type` discriminator:

```console
$ phosphor-oss --output-format ndjson agent run -p "list the crates"
{"type":"system","event_type":"conversation_started","conversation_id":"…"}
{"type":"agent_reasoning","text":"I should read the workspace manifest."}
{"type":"tool_call","tool":"read_files","files":[{"…"}]}
{"type":"tool_result","tool":"read_files","…":"…"}
{"type":"agent","text":"The workspace has 41 crates…"}
```

Record types are `agent`, `agent_reasoning`, `tool_call`, `tool_result`,
`tool_canceled`, `tool_error`, `update_todos`, `complete_todos`, `Subagent`,
`system`, `num_comments_addressed`, `artifact_created`, and `SkillInvoked`.
Under `pretty`/`text` the same stream is rendered as prose (`Running \`cargo
test\``, `Reading src/main.rs`, and so on).

Note that `whoami` **rejects** `--output-format ndjson`.

---

## 4.7 How do I register a provider and pick a model?

This is where Phosphor diverges most sharply from what a Warp user — or the
subcommand list — would lead you to expect.

**`phosphor-oss provider …` is not the bring-your-own-provider surface, and in a
normal build it does not exist at all.** The `provider` subcommand in the source
is upstream Warp's integration linker for **Linear and Slack** (`provider setup
linear --team`, `provider list`). It is gated behind a feature flag that no
shipped Phosphor build turns on, so the argument parser rejects it up front:

```console
$ phosphor-oss provider list
error: unrecognized subcommand 'provider'

For more information, try '--help'
```

Nor is there a CLI command for adding an AI provider key. **API keys are entered
in the app, not on the command line**, through Phosphor's arbitrary-provider
BYOP store (`AgentProviderSecrets`): the GUI's *Settings → AI → Providers* page,
or the TUI's `/api-keys` picker. `--api-key` on this CLI is unrelated — it is
Warp's account credential and does nothing here.

`phosphor-tui`'s `--set-provider-api-key` / `--clear-provider-api-key` flags are
**not** a route into that store, despite the name: they write `ApiKeyManager`'s
`AiApiKeys` keyring entry (the pin's fixed four providers), which the BYOP send
path never reads. See §7.1 and issue #629.

What the CLI *does* give you is the model side. Once a provider is configured in
the app, its models appear in `model list`, and that list is built entirely from
your local provider configuration:

```console
$ phosphor-oss model list
╭──────────────────────────────╮
│ MODEL ID                     │
╞══════════════════════════════╡
│ claude-…                     │
│ gpt-…                        │
╰──────────────────────────────╯

$ phosphor-oss --output-format text model list | tail -n +2 | head -1
claude-…

$ phosphor-oss agent run --model "$(phosphor-oss --output-format text model list | sed -n 2p)" \
    -p "explain the build graph"
```

`--model` is validated against exactly this list, so an ID that is not in
`model list` fails before the agent starts.

---

## 4.8 How do I …

### … see which MCP servers a run can use?

```console
$ phosphor-oss mcp list
╭──────────────────────────────────────┬──────────────╮
│ UUID                                 ┆ Name         │
╞══════════════════════════════════════╪══════════════╡
│ 3f2a…                                ┆ filesystem   │
╰──────────────────────────────────────┴──────────────╯
```

The UUIDs are directly usable as `--mcp <UUID>` on `agent run`.

### … see my agent profiles?

```console
$ phosphor-oss agent profile list
╭───────────┬──────────────────╮
│ ID        ┆ Name             │
╞═══════════╪══════════════════╡
│ Unsynced  ┆ Default          │
╰───────────┴──────────────────╯
```

The `ID` column shows a profile's sync ID when it has one and `Unsynced`
otherwise. See §4.12 for why that matters for `--profile`.

### … check who Phosphor thinks I am?

```console
$ phosphor-oss whoami
User ID: test_user_uid
Email: test_user@warp.dev
```

That output is not a bug you can fix by signing in — there is nothing to sign in
to. Phosphor's auth state is a hard-coded local placeholder, so `whoami` is only
useful as a smoke test that the binary starts. It supports `pretty`, `text`
(`user:test_user_uid`) and `json`, but not `ndjson`.

### … collect diagnostics for a bug report?

```console
$ phosphor-oss dump-debug-info
Phosphor version: Some("v2026.08.28")
uname(1) output: Linux … x86_64 GNU/Linux
Package type: …
Windowing system: Wayland
gpu_power_preference: HighPerformance
backend_preference: None
```

`--dump-debug-info` is accepted as a long flag equivalent to the subcommand.

---

## 4.9 Shell completions

`completions` writes a completion script for your shell to **stdout**. With no
argument it infers the shell from your environment and errors out if it cannot:

```console
$ phosphor-oss completions
Could not determine shell from environment. Please provide a shell argument.
```

The subcommand's built-in help documents **bash, zsh, fish and PowerShell**. The
value list is `clap_complete`'s own shell enum, so run `phosphor-oss completions
--help` to see exactly what your build accepts.

The generated script is named after **argv\[0\]**, so generate it under the same
name you will invoke — if you installed the desktop package and type `phosphor`,
generate with `phosphor completions …`, not `phosphor-oss completions …`.

**bash** — add to `~/.bashrc`:

```bash
source <(phosphor-oss completions bash)
```

or install it once, which is faster to start:

```bash
mkdir -p ~/.local/share/bash-completion/completions
phosphor-oss completions bash > ~/.local/share/bash-completion/completions/phosphor-oss
```

**zsh** — add to `~/.zshrc`:

```zsh
source <(phosphor-oss completions zsh)
```

or install into a directory on your `$fpath`:

```zsh
mkdir -p ~/.zfunc
phosphor-oss completions zsh > ~/.zfunc/_phosphor-oss
# then, before `compinit` in ~/.zshrc:
fpath=(~/.zfunc $fpath)
```

**fish** — add to `~/.config/fish/config.fish`:

```fish
phosphor-oss completions fish | source
```

or install it once:

```fish
mkdir -p ~/.config/fish/completions
phosphor-oss completions fish > ~/.config/fish/completions/phosphor-oss.fish
```

**PowerShell** — add to `$PROFILE`:

```powershell
phosphor-oss completions powershell | Out-String | Invoke-Expression
```

> The built-in help text for this subcommand is partly stale: its bash, zsh and
> fish lines still say `path/to/warp` (the binary is `phosphor-oss`). Its
> PowerShell line was rebranded and reads
> `path\to\phosphor-oss completions powershell | Out-String | Invoke-Expression`,
> which is correct. The commands above are correct for every shell.

---

## 4.10 Complete flag reference

### `agent run` (alias `r`)

| flag | what it does | default | visible in `--help`? |
|---|---|---|---|
| `-p`, `--prompt <TEXT>` | The task for the agent. | — | yes |
| `--saved-prompt <ID>` | Run a saved prompt by ID instead. Conflicts with `--prompt`. | — | yes |
| `--skill <SPEC>` | Use a skill as the base prompt. | — | yes |
| `--model <MODEL_ID>` | Base model override; must be an ID from `model list`. | profile default | yes |
| `-f`, `--file <PATH>` | YAML/JSON config file. Env: `WARP_AGENT_CONFIG_FILE`. | — | yes |
| `-n`, `--name <NAME>` | Name for the agent task. | skill name, then file `name` | yes |
| `-C`, `--cwd <PATH>` | Working directory. | current directory | yes |
| `--mcp <SPEC>` | MCP server: UUID, file path, or inline JSON. Repeatable. | none | yes |
| `--profile <ID>` | Agent profile to configure the session. | active profile | yes |
| `--idle-on-complete [DURATION]` | Keep the session open after completion. | off; `45m` if the flag is given without a value | **no** (hidden) |
| `--computer-use` / `--no-computer-use` | Force computer use on/off for this run. Mutually exclusive. | profile setting | **no** (hidden) |
| `--harness <HARNESS>` | Execution harness: `oz`, `claude`, `opencode`, `gemini`, `codex`. | `oz` | **no** (hidden) |
| `--gui` | Show the run's progress in the Phosphor window instead of running headlessly. | off | **no** (hidden) |
| `--sandboxed` | Marks the run as sandboxed. | off | **no** (hidden) |
| `--share [RECIPIENTS]` | **Inert. Parses and does nothing.** See §4.13. | — | **no** (hidden) |
| `--mcp-server <UUID>` | Legacy form of `--mcp` for UUIDs only. | none | **no** (hidden) |
| `--bedrock-inference-role <ROLE_ARN>` | AWS Bedrock federated-credential role. Requires `--bedrock-role-region`. | — | **no** (hidden) |
| `--bedrock-role-region <REGION>` | Region for the Bedrock `AssumeRoleWithWebIdentity` call. Requires `--bedrock-inference-role`. | — | **no** (hidden) |

`--harness opencode` is rejected at run time with *"The opencode harness is only
supported for local child agent launches"*; `claude`, `gemini` and `codex`
delegate the prompt to that CLI, which must be installed and authenticated. When
a non-`oz` harness is selected, `--model` names a model of *that* CLI and is not
checked against `model list`.

### `agent message send`

| flag | what it does | default |
|---|---|---|
| `--sender-run-id <ID>` | The sending run's ID. Required. | — |
| `--to <ID>…` | Recipient run IDs. Repeatable or comma-delimited. Required. | — |
| `--subject <TEXT>` | Message subject. Required. | — |
| `--body <TEXT>` | Message body. Required. | — |

### `agent message list`

| argument / flag | what it does | default |
|---|---|---|
| `<RUN_ID>` (positional) | Mailbox to read. Required. | — |
| `-L`, `--limit <N>` | Maximum messages, most recent kept. Must be ≥ 1. | `25` |

### `agent list`

| flag | what it does | default |
|---|---|---|
| `-r`, `--repo <REPO>` | List skills from `owner/repo` or a GitHub URL. | — |

The command parses, but the handler returns
`Agent skill listing is disabled in Phosphor` — see §4.13.

### `agent profile list`, `mcp list`, `model list`, `whoami`

No flags of their own; they take the global options only.

### `completions`

| argument | what it does | default |
|---|---|---|
| `<SHELL>` (positional, optional) | A `clap_complete` shell name; the help documents `bash`, `zsh`, `fish`, `powershell`. | inferred from the environment; error if it cannot be |

---

## 4.11 Environment variables

Phosphor's CLI reads a small, specific set. **The `WARP_*` names below are the
real ones** — they are the literal `env =` attributes in the argument parser, and
no `OZ_*` alias exists for them. Conversely, the `OZ_*` names are the run-identity
and mailbox variables, and no `WARP_*` alias exists for *those*. Do not guess in
either direction.

| variable | read by | effect |
|---|---|---|
| `WARP_API_KEY` | `--api-key` | Supplies the (inert) account credential. Its value is never printed in `--help`. |
| `WARP_OUTPUT_FORMAT` | `--output-format` | Default output format when the flag is absent. |
| `WARP_AGENT_CONFIG_FILE` | `agent run -f` | Default config-file path. |
| `WARP_CLI_MODE` | launch dispatch | If set to anything, a bare invocation prints help instead of launching the GUI. |
| `OZ_RUN_ID` | set by Phosphor for a spawned agent | That agent's own run ID; the mailbox address to send *from*. |
| `OZ_PARENT_RUN_ID` | set by Phosphor for a spawned child agent | The parent's run ID; the mailbox address to report *to*. Absent for a top-level run. |
| `OZ_CLI` | set by Phosphor for a spawned agent | Absolute path to the Phosphor executable, so a child harness can re-invoke it. |
| `OZ_HARNESS` | set by Phosphor for a spawned agent | Name of the harness the child was launched under (`oz`, `claude`, …). Exported for hooks; nothing in Phosphor reads it back. |
| `OZ_AGENT_MAILBOX_ROOT` | `agent message send`/`list` | Overrides the mailbox root directory. Ignored when empty. |
| `OZ_MESSAGE_LISTENER_MANAGED_EXTERNALLY`, `OZ_MESSAGE_LISTENER_STATE_ROOT` (legacy aliases `OZ_PARENT_LISTENER_MANAGED_EXTERNALLY`, `OZ_PARENT_STATE_ROOT`) | set by Phosphor for a Claude child harness | Tell the Claude plugin that Phosphor owns the message-listener lifecycle, and where the shared state lives. |
| `ZAP_UNSTABLE_FEATURES` | feature-flag init | Comma-separated unstable feature names, or `all`/`*`. This is the only enable path for a handful of off-by-default subsystems; it does **not** enable the `provider` subcommand. |
| `SHELL` | `completions` with no argument | Used to infer which shell to generate for. |

`WARP_DATA_PROFILE` also exists but is honoured only in debug builds, and
`ZAP_LOG_STDOUT` redirects the GUI's logging to stdout; neither is a CLI
argument.

---

## 4.12 Known rough edges

Three things in this surface are inconsistent enough to be worth calling out
before you hit them.

**`--profile <ID>` cannot address a locally created profile.** `agent profile
list` prints `Unsynced` for any profile that has only a local ID, which in a
normal Phosphor install is all of them; but `--profile` insists on a
22-character legacy server-style ID and fails with a profile error for anything
else. In practice this means `--profile` is unusable unless you already have a
profile carrying a server-style ID. Select profiles in the app instead.

**`--output-format json` is not JSON for `agent run`.** For list commands it
emits a JSON array; for `agent run` it emits the same newline-delimited stream
as `ndjson`. If you are piping a run into `jq`, use `jq -c` line-at-a-time
semantics, not a whole-document parse.

**List commands under `--output-format json` emit no trailing newline.** The
array is written and the process exits. Shell pipelines that expect a final
newline should tolerate its absence.

---

## 4.13 Not available in Phosphor

Every entry below exists in upstream Warp's `oz` CLI and is deliberately absent
here. Each is a recorded decision in `DECLINED.md`, not an oversight.

| what a Warp user looks for | status here |
|---|---|
| `oz login` / `oz logout` | **Removed.** There is no account. BYOP has nothing to log in or out of. |
| `oz run …`, `oz run message send/list/watch/read/mark-delivered` | **Removed.** These were clients of Warp's server-side hosted-task registry, which addresses runs by server-assigned ID. Phosphor's replacement is `agent message send|list` over a local on-disk mailbox (§4.5). A test asserts `run` stays removed, so it will not come back. |
| `oz agent run-cloud` (and `run --task-id`, `--conversation`, snapshot flags) | **Removed.** Dispatch to Warp's hosted agent infrastructure. Runs are local processes here. |
| `oz agent create/get/update/delete` | **Removed.** CRUD on named agents stored server-side. |
| `oz environment …` | **Removed** with the cloud ambient-agent subsystem (cloud dev-environment provisioning, AWS/GCP OIDC). The parser reports it as an unrecognized subcommand. |
| `oz schedule …` | **Removed.** Cron-scheduled cloud agents. Note that `--skill`'s own help text still points at `oz schedule create --skill …`; that command does not exist. |
| `oz secret …` | **Removed**, and explicitly rejected by name before parsing. It was a client for a server-side secret store. |
| `oz federate …` | **Removed**, and explicitly rejected by name before parsing. |
| `oz memory` / `oz memory-store …` | **Removed.** Team-shared memory synced to the server. |
| `oz integration …` | **Removed.** Slack-triggered cloud agent runs. |
| `oz artifact upload/download/get` | **Removed.** Cloud snapshot/artifact storage keyed by cloud run ID. |
| `oz harness-support …` | **Removed.** Status callbacks a hosted harness reports to Warp's backend. This is why Warp's `oz-harness-support` and `orchestration` agent plugins are not installed by Phosphor — their scripts call this command. |
| `oz agent run --share` | **Accepted but inert.** Sharing needs Warp's backend to host the session and resolve `team:` / `public:` / email recipients. The flag is hidden and validates its recipient grammar helpfully, then does nothing — sharing is hard-coded off. It was hidden rather than deleted so an existing script that passes it keeps parsing. **If you pass `--share`, nothing is shared and nothing tells you so.** |
| `oz agent list` | **Present but disabled.** It parses, including `--repo`, then fails with `Agent skill listing is disabled in Phosphor`. |
| `oz provider setup linear\|slack`, `oz provider list` | **Not reachable.** The subcommand is feature-gated off in every shipped build and the parser rejects it. It was never the AI-provider surface — see §4.7. |
| `oz whoami` showing a real account | **Placeholder only.** It prints a fixed local identity (`test_user_uid` / `test_user@warp.dev`). Organisation and team fields are never populated, because teams are permanently absent. |
| `WARP_SERVER_ROOT_URL`, `WARP_WS_SERVER_URL`, `WARP_SESSION_SHARING_SERVER_URL` | **Do not exist.** These pointed at Warp's GraphQL and session-sharing backends. There is no backend to point at. |

<!-- SOURCES

Binary / command naming
- app/Cargo.toml:25-28 — [[bin]] name = "phosphor-oss", path = src/bin/phosphor_oss.rs
- app/Cargo.toml:3 — default-run = "phosphor-oss"
- app/Cargo.toml:18-23 — comment: target name is the on-disk executable name, distinct from Channel::cli_command_name()
- crates/warp_core/src/channel/mod.rs:37-46 — cli_command_name(); Channel::Oss => "phosphor-oss"
- crates/warp_core/src/channel/state.rs:38-57 — ChannelState::init() hard-codes Channel::Oss, display_name "Phosphor", app id dev.phosphor.Phosphor
- crates/warp_cli/src/lib.rs:90-99 — #[command(name = "oz", display_name = "Phosphor", about = "Phosphor local agent CLI…")]
- crates/warp_cli/src/lib.rs:226-240 — after_help examples substitute binary_name() (argv[0]) at runtime
- crates/warp_cli/src/lib.rs:516-522 — binary_name() reads argv[0]
- clap_builder 4.6.0 (Cargo.lock) src/builder/command.rs:934-944 `print_help()` calls `_build_self`, which never assigns `bin_name`; :905 is the only place argv[0] populates it (inside `try_get_matches_from_mut`), and output/usage.rs:157 + command.rs:3719-3746 fall back `usage_name -> bin_name -> get_name()`. So `Args::clap_command().print_help()` (app/src/lib.rs:793-795) prints `Usage: oz`, while a parsed `--help` prints `Usage: <argv[0]>`.
- crates/warp_cli/src/completions.rs:16-22 — generated completion script is named after binary_name()
- script/linux/bundle:198-215 — oss channel: WARP_BIN/BINARY_NAME = phosphor-oss; cli artifact keeps phosphor-oss (no warp→oz rename for oss)
- script/macos/bundle:340-350, 583-605 — oss: WARP_BIN=phosphor-oss, CLI wrapper at Contents/Resources/bin/phosphor-oss
- script/windows/bundle.ps1:113-114 — phosphor-oss / phosphor-oss.exe
- app/src/workspace/cli_install.rs:10-13 — GUI "install CLI" symlinks /usr/local/bin/<cli_command_name> = phosphor-oss
- resources/linux/debian/app/postinst.template — app package symlinks /usr/bin/phosphor
- resources/linux/debian/cli/postinst.template + control.template — phosphor-cli package symlinks /usr/bin/<BINARY_NAME> = phosphor-oss
- script/check_channel_command_names:12,23,49 — guard tying the bundle names to Channel::cli_command_name

Launch dispatch
- app/src/lib.rs:744-786 — Command::Worker / Completions / CommandLine / DumpDebugInfo dispatch
- app/src/lib.rs:789-796 — is_cli_binary: cfg!(feature="standalone") || argv[0] starts with "oz" || WARP_CLI_MODE set → print help
- app/Cargo.toml:790 — `standalone = []`, not in `default`
- script/linux/bundle:216-222 — cli artifact adds FEATURES=standalone; app artifact adds gui
- app/src/lib.rs:484-486 — is_headless(): agent run is headless unless --gui
- crates/warp_cli/src/lib.rs:415-421 — Command::prints_to_stdout

Global options
- crates/warp_cli/src/lib.rs:66-85 — GlobalOptions: --api-key (env WARP_API_KEY, hide_env_values), --output-format (env WARP_OUTPUT_FORMAT, default Pretty)
- crates/warp_cli/src/lib.rs:118-120 — --debug, global
- crates/warp_cli/src/lib.rs:242-245 — command.version(version_string())
- crates/warp_cli/src/lib.rs:524-530 — version_string(): ChannelState::app_version() or "<unknown>"
- crates/warp_cli/src/lib_tests.rs — help_hides_api_key_env_value, api_key_before_subcommand_parses, debug_before_subcommand_parses, multiple_global_flags_before_subcommand_parse
- crates/warp_cli/src/lib.rs:105-110 — subcommand_precedence_over_arg, so global flags may precede a subcommand
- DECLINED.md:165 — hide_env_values on --api-key is a deliberate divergence (credential leak in --help)

Subcommand inventory
- crates/warp_cli/src/lib.rs:352-368 — CliCommand: Agent, MCP, Model, Whoami, Provider
- crates/warp_cli/src/lib.rs:376-410 — Command: Worker(flatten), CommandLine(flatten), Completions, DumpDebugInfo
- crates/warp_cli/src/lib.rs:282-336 — WorkerCommand: TerminalServer, PluginHost, MinidumpServer, RemoteServerProxy, RemoteServerDaemon, RipgrepSearch (all hide=true or feature-gated)
- app/Cargo.toml:469-473, 766 — plugin_host is only pulled in by completions_v2; neither is in `default`
- app/src/lib.rs:735-742 — the hidden --warpctrl flag selects a separate parser
- crates/warp_cli/src/local_control/mod.rs:71-97 — CONTROL_MODE_FLAG stripping / warpctrl parser
- crates/warp_features/src/lib.rs — FeatureFlag::WarpControlCli is dogfood-only; app/src/lib.rs:3429 gives it a ZAP_UNSTABLE_FEATURES path only

agent run
- crates/warp_cli/src/agent.rs:291-300 — visible_alias "r"; ArgGroup prompt_group required over prompt/saved_prompt/skill
- crates/warp_cli/src/agent.rs:50-60 — PromptArg: --prompt/-p, --saved-prompt, group(multiple=false)
- crates/warp_cli/src/agent.rs:309-323 — --skill <SPEC>; search order .agents/skills, .warp/skills, .claude/skills, .codex/skills; long_help still references `oz schedule create --skill`
- crates/warp_cli/src/model.rs:11-17 (enum at :5) — ModelArgs --model <MODEL_ID>, help text still says "warp model list"
- crates/warp_cli/src/config_file.rs:5-14 — -f/--file, env WARP_AGENT_CONFIG_FILE
- crates/warp_cli/src/agent.rs:325-330 — --name/-n, --cwd/-C
- crates/warp_cli/src/agent.rs:331-333 — --gui, hide = true
- crates/warp_cli/src/agent.rs:334-347 — --mcp <SPEC> repeatable; --mcp-server <UUID> hidden legacy
- crates/warp_cli/src/mcp.rs:36-79 — MCPSpecParser: UUID, else existing file path read as JSON, else inline JSON
- crates/warp_cli/src/agent.rs:349-361 — --idle-on-complete, num_args=0..=1, default_missing_value "45m", hide = true
- crates/warp_cli/src/agent.rs:362-364 — --sandboxed, hidden
- crates/warp_cli/src/agent.rs:365-384 — --bedrock-inference-role / --bedrock-role-region, hidden, mutually requiring
- crates/warp_cli/src/agent.rs:99-110 — HiddenComputerUseArgs: --computer-use / --no-computer-use, hide = true, conflicting
- crates/warp_cli/src/agent.rs:388-398 — --profile <ID>; --harness <HARNESS> default oz, hide = true
- crates/warp_cli/src/agent.rs:120-160 — Harness value enum: oz, claude(+claude-code), opencode(+open-code), gemini, codex
- app/src/ai/agent_sdk/mod.rs:140-155 — --skill gated on OzPlatformSkills, --harness gated on AgentHarness, opencode rejected at runtime
- app/Cargo.toml:592, 647 — "oz_platform_skills" and "agent_harness" ARE in `default`, so --skill/--harness work
- app/src/ai/agent_sdk/mod.rs:216-232 — non-oz harness: --model targets the third-party CLI and skips model validation
- app/src/ai/agent_sdk/mod.rs:234-263 — merge precedence file < CLI < skill; model_override validated by validate_agent_mode_base_model_id
- app/src/ai/agent_sdk/mod.rs:480-486 — --cwd canonicalized, else std::env::current_dir; error "Unable to resolve <path>"
- app/src/ai/agent_sdk/driver.rs:176-200, 1669, 1808 — idle_on_complete is live (complete_with_optional_idle)
- app/src/ai/agent_sdk/config_file.rs:9-30 — AgentConfigSnapshotFile, deny_unknown_fields, keys name/model_id/base_prompt/mcp_servers/host/computer_use_enabled
- app/src/ai/agent_sdk/config_file.rs:37-73 — .json → JSON, .yml/.yaml → YAML, else JSON-then-YAML
- app/src/ai/agent_sdk/config_file.rs:104-118 — mcp_servers entries with `warp_id` become UUID specs
- crates/warp_cli/src/skill.rs:1-40 — SkillSpec formats
- crates/warp_cli/src/lib_tests.rs — agent_run_rejects_without_prompt_or_skill, agent_run_accepts_{prompt,saved_prompt,skill}_only, agent_run_rejects_prompt_and_saved_prompt, agent_run_accepts_idle_on_complete_{flag,duration}, agent_run_accepts_harness_flag, agent_run_defaults_harness_to_oz, agent_run_accepts_file_short_flag, agent_run_accepts_mcp, agent_run_rejects_bedrock_*

Mailbox
- crates/warp_cli/src/agent.rs:237-254 — AgentCommand::Message; Send/List
- crates/warp_cli/src/agent.rs:256-290 — --sender-run-id, --to (required, num_args 1.., value_delimiter ','), --subject, --body; list positional run_id, -L/--limit default 25, range 1..
- crates/warp_cli/src/agent_mailbox.rs:1-40 — module doc: local replacement for the pin's cloud `oz run message *`
- crates/warp_cli/src/agent_mailbox.rs:42 — AGENT_MAILBOX_ROOT_ENV = "OZ_AGENT_MAILBOX_ROOT"
- crates/warp_cli/src/agent_mailbox.rs:61-72 — mailbox_root(): env override, else state_dir()/oz/agent-mailbox
- crates/warp_core/src/paths.rs:174-184 — state_dir(); falls back to data_local_dir on macOS/Windows
- crates/warp_core/src/paths.rs:313-330 — Linux base app name "phosphor"
- crates/warp_cli/src/agent_mailbox.rs:95-110 — message_file_name(): zero-padded nanos prefix
- crates/warp_cli/src/agent_mailbox.rs:112-134 — write_atomically (temp file + rename)
- crates/warp_cli/src/agent_mailbox.rs:157-196 — list_messages: oldest first, missing dir → empty, drains to keep the most recent `limit`
- app/src/ai/agent_sdk/agent_message.rs:1-80 — CLI dispatch + TableFormat columns (Message ID, From, Subject, Body, Sent At); "Sent message {id} to {to}"
- DECLINED.md:232 — reversal row: the mailbox exists and is local; `oz run` removal is permanent

Output formats
- crates/warp_cli/src/agent.rs:9-32 — OutputFormat: json, ndjson, pretty (default), text
- app/src/ai/agent_sdk/output.rs:267-323 — write_list: Json = to_writer of a Vec (no trailing newline); Ndjson = one object per line; Pretty = comfy-table UTF8_FULL + rounded; Text = TabWriter
- app/src/ai/agent_sdk/driver.rs:1764-1773, 1871-1879 — agent run: Json and Ndjson both go to output::json (newline-delimited)
- app/src/ai/agent_sdk/driver/output.rs:525-578 — JsonMessage tagged "type"; JsonSystemEvent tagged "event_type"
- app/src/ai/agent_sdk/driver/output.rs:1151-1186 — write_message writes one object per line
- app/src/ai/agent_sdk/driver/output.rs:269-300, 463-473 — text rendering ("Running `{command}`", "Reading …", "Run ID: …", "New conversation started with debug ID: …")
- app/src/ai/agent_sdk/admin.rs:112-121 — whoami rejects ndjson

Provider / model / BYOP
- crates/warp_cli/src/provider.rs:5-11, 19-25 — ProviderCommand::Setup/List; ProviderType is Linear | Slack
- crates/warp_cli/src/lib.rs:168-176 — pre-parse rejection of `provider` when FeatureFlag::ProviderCommand is off ("error: unrecognized subcommand 'provider'", exit 2)
- crates/warp_cli/src/lib.rs:216-219 — provider subcommand hidden from help when the flag is off
- app/src/ai/agent_sdk/mod.rs:98-103 — dispatch also errors when the flag is off
- crates/warp_features/src/lib.rs:450, 847 — ProviderCommand exists only in DOGFOOD_FLAGS
- crates/warp_features/src/lib.rs:820-846 — DOGFOOD_FLAGS reaches no binary in this fork; a live flag needs a cargo `default` feature, RELEASE_FLAGS, or UNSTABLE_FEATURES
- app/src/lib.rs:3366-3439 — UNSTABLE_FEATURES table; ProviderCommand is not in it
- app/Cargo.toml — no `provider_command` cargo feature exists
- app/src/ai/agent_sdk/model.rs:26-60 — model list is built from LLMPreferences::get_base_llm_choices_for_agent_mode; column "MODEL ID"; item {id}
- DECLINED.md:177 — the fork's BYOP surface is AgentProviderSecrets (arbitrary providers with their own base_url), superseding the pin's fixed-four + CustomEndpoint
- DECLINED.md:225 — provider API keys are set in-process (warp_tui --set-provider-api-key / --clear-provider-api-key, /api-keys picker), not via a self-shelling CLI

Auth / whoami
- app/src/auth/mod.rs:31-32 — TEST_USER_EMAIL "test_user@warp.dev", TEST_USER_UID "test_user_uid"
- app/src/auth/mod.rs:205-221 — User::test() is the placeholder identity
- app/src/auth/mod.rs:261-291 — AuthState::new()/initialize(): api_key only sets a local Credentials::ApiKey
- app/src/auth/mod.rs:393-395, 437-439 — user_id() / principal_type()
- app/src/lib.rs:1352-1365 — api_key gated on FeatureFlag::APIKeyAuthentication; app/src/lib.rs:3157-3158 maps it to the `api_key_authentication` cargo feature, which IS in `app/Cargo.toml`'s `default` (line 558), so the CLI does accept the flag. `LaunchMode::CommandLine` is not dogfood-gated (unlike `App`/`Tui`). app/src/auth/mod.rs:272-289 — all it does is set a local `Credentials::ApiKey`.
- app/src/ai/agent_sdk/admin.rs:36-125 — whoami output shape per format
- DECLINED.md:83-86 — teams/current_team() are permanently None; login/logout removed

Profiles
- app/src/ai/agent_sdk/profiles.rs:29-58 — profile list; SyncId::ServerId → id string, otherwise "Unsynced"
- app/src/ai/agent_sdk/driver.rs:1454-1477 — configure_terminal: --profile must parse as ServerId, else AgentDriverError::ProfileError
- app/src/server/ids.rs:153-158, 212-223 — ServerId is exactly 22 chars
- app/src/server/ids.rs:62-69 — SyncId::ClientId is the locally-generated variant

MCP list
- crates/warp_cli/src/mcp.rs:8-12 — MCPCommand::List
- app/src/ai/agent_sdk/mcp.rs:28-68 — get_all_runnable_mcp_servers, sorted by uuid; columns UUID, Name

Completions
- crates/warp_cli/src/lib.rs:386-400 — Completions doc comment (verbatim_doc_comment): bash/zsh/fish lines still say `path/to/warp`; the PowerShell line at :398 reads `path\to\phosphor-oss completions powershell | Out-String | Invoke-Expression` (rebranded and complete — it was fixed by 0c9ef1c2f, and the earlier claim that it omitted `completions powershell` described the pre-fix text)
- crates/warp_cli/src/completions.rs:10-23 — Shell::from_env fallback and the "Could not determine shell from environment" error
- crates/warp_cli/Cargo.toml:24 — clap_complete 4.5.58 supplies aot::Shell; the accepted value list is that enum, not enumerated in this tree. The four shells named in the manual come from the Completions doc comment at crates/warp_cli/src/lib.rs:379-407.

dump-debug-info
- crates/warp_cli/src/lib.rs:409-410 — DumpDebugInfo with long_flag "dump-debug-info"
- app/src/debug_dump.rs:11-80 — printed fields

agent list disabled
- crates/warp_cli/src/agent.rs:410-420 — ListAgentConfigsArgs --repo/-r
- app/src/ai/agent_sdk/mod.rs:177-180 — returns "Agent skill listing is disabled in Phosphor"

--share
- crates/warp_cli/src/share.rs:11-27 — ShareArgs::share, hide = true, with the rationale in the doc comment
- app/src/ai/agent_sdk/mod.rs:500 — should_share = false, hard-coded
- DECLINED.md:227 — Agent session sharing declined; flag hidden 2026-08-10 rather than removed

Environment variables
- crates/warp_cli/src/lib.rs:29-32 — OZ_RUN_ID, OZ_PARENT_RUN_ID, OZ_CLI, OZ_HARNESS
- app/src/ai/agent_sdk/driver/harness/mod.rs:209-250 — task_env_vars_for_harness_name sets those four; OZ_HARNESS is exported but read by nothing here
- app/src/ai/agent_sdk/driver.rs:88-105 — OZ_MESSAGE_LISTENER_MANAGED_EXTERNALLY, OZ_MESSAGE_LISTENER_STATE_ROOT, legacy OZ_PARENT_* aliases
- app/src/lib.rs:3326-3350 — ZAP_UNSTABLE_FEATURES parsing (comma/whitespace separated, `all`/`*`)
- crates/warp_core/src/channel/state.rs:110-118 — WARP_DATA_PROFILE honoured only under debug_assertions
- README.md:157-158 — ZAP_LOG_STDOUT
- README.md:230-235 — WARP_* build variables and the TUI binary name deliberately not renamed

Removed cloud subcommands
- crates/warp_cli/src/lib.rs:168-190 — from_env() rejects `secret` and `federate` by name before parsing; `environment` is gone from the enum so clap reports it unrecognized
- crates/warp_cli/src/lib_tests.rs:6-105 — the per-command port audit listing Environment, MemoryStore/Memory, Login/Logout, Schedule, Secret, Integration, Artifact, HarnessSupport, RunCloud, agent CRUD, Run/task.rs, and the WARP_SERVER_ROOT_URL / WARP_WS_SERVER_URL / WARP_SESSION_SHARING_SERVER_URL env overrides
- crates/warp_cli/src/lib_tests.rs — run_command_is_removed
- DECLINED.md:199 — oz harness-support does not exist here; that is why the oz-harness-support / orchestration plugins are not installed

-->
