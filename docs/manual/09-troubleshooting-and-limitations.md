# 9. Troubleshooting, and what is not here

This chapter is two things. The first half is the diagnostic surface: where
Phosphor writes its logs, how to make them louder, how to package them up, and
what to check when a specific subsystem misbehaves. The second half is the
reference list of features a Warp user will look for and not find, with the
reason for each — because Phosphor is a fork of Warp with the Warp cloud removed,
and the fastest way to stop troubleshooting something is to learn that it was
deliberately taken out.

Nothing in Phosphor sends usage data anywhere. That claim is spelled out with
its evidence in [What leaves the machine](#what-leaves-the-machine) below,
because it is the one users most need to be able to check.

---

## Where Phosphor keeps its files

Phosphor uses three separate directory roots, and they are *not* the same
directory on Linux and Windows. Everything below is for the shipping OSS build,
whose application identity is `dev.phosphor.Phosphor`.

| Root | macOS | Linux | Windows |
|---|---|---|---|
| **config** (`settings.toml`, `keybindings.yaml`, `user_preferences.json`) | `~/.phosphor/` | `${XDG_CONFIG_HOME:-~/.config}/phosphor/` | `%LOCALAPPDATA%\phosphor\Phosphor\config\` |
| **data** (`themes/`, `workflows/`, …) | `~/.phosphor/` | `${XDG_DATA_HOME:-~/.local/share}/phosphor/` | `%APPDATA%\phosphor\Phosphor\data\` |
| **state** (database, PTY recordings, crash dumps) | `~/Library/Application Support/dev.phosphor.Phosphor/` | `${XDG_STATE_HOME:-~/.local/state}/phosphor/` | `%LOCALAPPDATA%\phosphor\Phosphor\data\` |
| **home dotfile** (`.mcp.json`, `skills/`, `prompts/`) | `~/.phosphor/` | `~/.phosphor/` | `%USERPROFILE%\.phosphor\` |

> **Older installs.** Before 2026-08-14 this project stored everything under a
> `zap` identity (`dev.zap.Zap`, `~/.config/zap`, `~/.zap`). There is no
> automatic migration — a pre-rename build and a post-rename build simply do not
> see each other's data. See `docs/migrate-from-warp.md`, and note that that
> document still names the old `zap` destinations.

---

## Logs

### Where they are written

The log directory is chosen per platform and then gets a subdirectory per
frontend, so a long-running TUI session cannot evict the GUI's rotated logs.

| | GUI (`phosphor-oss`) | TUI (`zap-tui-oss`) | CLI (`phosphor-oss agent …`, remote-server daemon) |
|---|---|---|---|
| **macOS** | `~/Library/Logs/phosphor.log` | `~/Library/Logs/warp-cli/phosphor-tui.log` | `~/Library/Logs/oz/phosphor.log` |
| **Linux** | `~/.local/state/phosphor/phosphor.log` | `~/.local/state/phosphor/warp-cli/phosphor-tui.log` | `~/.local/state/phosphor/oz/phosphor.log` |
| **Windows** | `%LOCALAPPDATA%\phosphor\Phosphor\data\logs\phosphor.log` | …`\logs\warp-cli\phosphor-tui.log` | …`\logs\oz\phosphor.log` |

The subdirectory names (`warp-cli`, `oz`) are lineage internals that were not
renamed; they are the real directory names on disk.

Each launch rotates the previous run's file: the active file becomes
`phosphor.log.old.0`, older ones shift up, and the oldest is dropped. The GUI
keeps **5** rotated files; the TUI and the CLI keep **10**. There is no
size-based rotation *within* a session — the app passes no size threshold — so a
very long session can produce a large active log.

### Making the logs louder

Logging is `env_logger`-based and the default level is **`info`**. Raise it with
the standard `RUST_LOG` environment variable, which accepts a global level or
per-module directives:

```sh
RUST_LOG=debug phosphor-oss
RUST_LOG=info,warp::ai=debug phosphor-oss
```

A few crates are pinned quieter than the global default, and `RUST_LOG` can
override them per module: `naga` at `info`, `wgpu_core` at `warn`, `wgpu_hal` at
`warn` (`error` on Windows), `tantivy` at `error`.

Two other switches matter:

| Variable | Effect |
|---|---|
| `ZAP_LOG_STDOUT=1` | GUI only. Sends logs to stderr instead of the log file, so you can watch them live. Without it the GUI *always* writes to a file, even when launched from a terminal. |
| `ZAP_BYOP_LOG_FULL_REQUEST=1` | Turns on full-content BYOP diagnostics: the entire outbound request (system prompt, conversation history, attached file contents, tool arguments and results) is written to the log. Off by default, where those fields are reduced to lengths, character-class counts and digests. |
| `WARP_STARTUP_TRACE=1` | Prints the startup interval-timer table once the first view is up. |

> **`RUST_LOG=debug` is a verbosity switch, not a privacy opt-in.** Some BYOP
> diagnostics that are dormant at `info` print raw tool-call arguments at
> `debug`. A log bundle collected under `RUST_LOG=debug` is wider than the
> default tier described below.

### What is in the log by default

The BYOP layer deliberately logs *shape, not content*: message and tool counts,
per-message roles and byte sizes, tool-call/tool-response pairing, request byte
size, and digests instead of free text. MCP tool names have their
user-configured server segment replaced by a per-process keyed digest, so
`mcp__acme-internal__search` is logged as `mcp__srv-<digest>__search`.

Known residuals that still print, each deliberately: the provider's own error
body, the configured provider endpoint URL, the URL an agent tried to fetch, and
the parser's own error text on a malformed tool call. A proxy URL is logged only
when it fails to parse, and its `user:password@` segment is replaced with
`<redacted-userinfo>@` first.

---

## Collecting a log bundle

The log bundle is a timestamped zip named `phosphor-YYYYMMDD-HHMMSS.zip`. It
contains the active log, its rotated `.old.N` siblings, and:

- `manifest.txt` — version, channel, execution mode, OS, arch, generation
  timestamp, and the log directory with `$HOME` collapsed to `~`;
- `warp-minidump.log` and `warp_update.log`, when those files exist;
- `mcp/<name>.log` — the current session's MCP server stderr.

Deliberately **not** included: `.dmp` minidump binaries (too large), the
prompt-chips log (it contains command output, and is only produced on debug
builds), and profiling artifacts.

| Surface | How |
|---|---|
| GUI | Command palette → **View Phosphor logs** (`workspace:view_logs`). Builds the zip and reveals it in your file manager. |
| GUI | **Settings → About → Export logs…**. Same contents, but you pick the destination with a native save dialog. It defaults to your Downloads folder. |
| TUI | `/view-logs`. Builds the zip and reveals it. TUI-only — the GUI refuses this command. |

Neither route uploads anything; both produce a file you choose whether to share.

---

## Other diagnostic commands

### `--dump-debug-info`

```sh
phosphor-oss --dump-debug-info
```

Prints a graphics/environment report and exits. It is the right first step for
"the window is blank", "text renders wrong", or "it will not start". It reports
the Phosphor version and `uname -a`; on Linux also the detected package/update
method, windowing system, window-manager name, the resolved GPU power preference
and backend preference, the full wgpu adapter list, and the output of `lspci`,
`vulkaninfo --summary` and `eglinfo` (with an install hint when a tool is
missing). On Windows it reports the GPU preferences and wgpu adapters.

The GUI has a command-palette entry, **Dump debug info**, which does not run the
command — it types it into the active session for you to run.

### `/copy-debugging-id`

Available in both the GUI and the TUI, whenever a conversation is active. It
copies a small JSON blob identifying the current conversation to the clipboard:

```json
{"conversation_id":"…"}
```

That is all it is — a local identifier you can quote in a bug report so a
maintainer can ask you about the right conversation. It is not a link, it
resolves to nothing on anyone else's machine, and copying it transmits nothing.
If the conversation has not received a reply yet it has no id, and the command
reports *"No debugging id for this conversation yet."*

### The BYOP wire inspector

`ctrl-alt-i` (Linux/Windows) or `cmd-ctrl-i` (macOS), or the button in the left
panel header. Opens a live capture of outbound and inbound LLM traffic — the
system prompt actually sent, the structured tool list, the environment block,
title-generation and one-shot calls, and streamed responses — filterable,
pausable and copyable. This is the tool for "the agent is not doing what the UI
implies I asked".

It is an in-memory ring buffer of the last 200 records, armed the first time you
open the window and cleared only by the explicit **Clear** action. Nothing is
written to disk. Capture does nothing for a model with no configured context
window.

---

## Crash reporting

**Nothing is ever uploaded.** `ChannelState::is_crash_reporting_available()` is
hard-coded `false`, and there is no crash-reporting endpoint in the build.

What the **Settings → Privacy → Send crash reports** toggle actually does is
install a panic hook. With it on, a Rust panic writes the thread name, source
location, message and a full backtrace into the local log before the default
handler runs. That is a genuinely useful thing to turn on before reproducing a
crash — and it is why the toggle is shown even though the upstream
"can this build ship crash reports" gate says no.

The toggle defaults to **off**. Warp defaults its equivalents on, because Warp is
an opt-out commercial product; Phosphor defaults them off, because leaving them
on would display "ON" while nothing goes anywhere.

The toggle only appears in builds compiled with the `crash_reporting` cargo
feature. Official release bundles are built with it; a plain `cargo run` is not.

### The `minidump-server` subcommand

The binary carries a hidden subcommand:

```
phosphor-oss minidump-server <socket-path>
```

It is a worker process, not something to run by hand. When started it listens on
the given socket and, on receiving a crash from a client process, writes a
`.dmp` file to `<state dir>/logs/crash-dumps/zap-minidump-<uuid>.dmp`, logs a
one-line summary — message, dump size, dump path and the accumulated tag map —
to `<state dir>/logs/warp-minidump.log`, and exits. It has no network code at
all: `send_crash_report` is a `log::error!` call and nothing else. Tags are
local diagnostic metadata (GPU device info, antivirus product, windowing system,
virtual-environment detection, application lifecycle stage); no user identity is
attached — `set_user_id` is an empty function.

**In practice you will not see these files.** The client half that spawns the
server (`local_minidump::init`, via `MinidumpGuard::start`) has no caller
anywhere in the tree, so no Phosphor process ever launches the minidump server.
Crash diagnostics in this build are the panic backtrace in the log, not a
minidump. This is worth knowing before you go hunting for a `crash-dumps`
directory that will not exist.

---

## What leaves the machine

This is the short version: **nothing, except the requests you configure.**

Outbound network traffic comes from exactly these sources, all of which are
things you asked for:

- **Your AI provider.** Phosphor is bring-your-own-provider: model calls go
  directly to the endpoint you configured with the key you supplied. Nothing is
  proxied through a Phosphor or Warp server.
- **MCP servers** you configured, and any URL an agent fetches with the web-fetch
  tool while you are watching it.
- **CLI-agent notification plugin installs**, which run the third-party agent's
  own CLI against GitHub or npm.

And that is the list. The specifics:

| Channel | Status |
|---|---|
| **Telemetry / analytics** | Physically removed. The `send_telemetry_*` macros are compile-time no-ops that type-check the event in a `if false` branch and evaluate nothing. `ChannelState::is_telemetry_available()` is hard-coded `false`, so the "Help improve Phosphor" toggle **never renders at all**. `should_collect_ai_ugc_telemetry()` returns `false` unconditionally, ignoring the setting. |
| **AI content / UGC collection** | Same function; always `false`. There is no `global_ai_analytics_collection` setting in this tree — that is a Warp server-side concept and has no counterpart here. |
| **Crash reports** | Never uploaded. See above. |
| **Settings sync** | `account.is_settings_sync_enabled` exists and defaults to `false`, but it has **no production consumer**: it only decides whether a "local only" badge is drawn next to a setting. Nothing syncs anywhere regardless of its value. |
| **Account / identity** | There is no account. `AuthState` is a local placeholder that always reports "logged in"; the placeholder id no longer appears in any outgoing header. |
| **Update checks** | The shipped build has no update channel at all — see [Updates](#updates). |

The `IsTelemetryEnabled` setting and its widget are kept in the tree
deliberately, so that the control would reappear on its own if a telemetry
channel were ever wired up. Today it is unreachable UI over a dead channel.

### Secret redaction

Separate from telemetry, and it *does* do something: **Settings → Privacy →
Secret redaction** scans blocks, Library object contents and agent prompts for
credentials and prevents them being saved or sent. You can add your own patterns
under **Custom secret redaction** (`privacy.custom_secret_regex_list`); they take
effect on the next command, and support the inline `(?i)` case-insensitivity
flag.

---

## Networking and proxies

### The default is "no proxy", including environment variables

This is the single most surprising networking behaviour in Phosphor, so it comes
first: `network.proxy_mode` defaults to **`off`**, and `off` means
`reqwest::ClientBuilder::no_proxy()` — *environment variables included*. If you
are behind a corporate proxy and have `HTTPS_PROXY` exported, Phosphor will
still not use it until you change this setting.

(The settings page's own description text says system mode is the default. That
text is wrong; the code default is `off`.)

| `network.proxy_mode` | Behaviour |
|---|---|
| `off` (default) | All proxying disabled, environment variables ignored. |
| `system` | Follow the platform's proxy configuration. On Linux that is the standard environment variables; on macOS, SystemConfiguration; on Windows, WinINET. |
| `custom` | Use `network.proxy_url`, with optional Basic auth and a no-proxy list. |

Set it in `settings.toml`:

```toml
[network]
proxy_mode = "system"
```

or in **Settings → Network**, which also offers a **Test connection** button —
in Custom mode it TCP-probes the proxy's host and port (reachability of the
proxy, not internet egress); otherwise it issues a real GET.

Any value that is not `system` or `custom` is read leniently as `off`.

### Environment variables, when `system` mode is on

WebSocket connections do their own proxy resolution (the underlying library has
no proxy support yet), and in `system` mode they read the conventional variables
directly. Both upper- and lower-case spellings are accepted:

| Target | Variables consulted, in order |
|---|---|
| `wss://` / TLS | `HTTPS_PROXY` → `https_proxy` → `ALL_PROXY` → `all_proxy` |
| `ws://` / plaintext | `HTTP_PROXY` → `http_proxy` → `ALL_PROXY` → `all_proxy` |
| bypass | `NO_PROXY` / `no_proxy` |

`NO_PROXY` entries are matched case-insensitively; `*` bypasses everything, a
leading-dot entry (`.internal`) matches any subdomain, and a bare entry matches
the host itself and its subdomains. In `custom` mode the same matching is
applied to the comma-separated `network.proxy_no_proxy` list instead.

Proxy credentials are split: the username lives in `settings.toml`
(`network.proxy_username`), the **password is stored in the OS keychain**
(macOS Keychain, Windows DPAPI, Linux keyring) under the key `ProxyPassword` and
never written to the settings file.

### When a proxy change takes effect

New requests — the BYOP model list, connection tests, conversation loading —
pick up the change immediately. Long-lived HTTP clients constructed at startup
do not, because a `reqwest::Client`'s proxy cannot be changed after
construction. Restart Phosphor if a proxy change appears not to have applied.

---

## Common problems

### The agent does not respond

Work through these in order.

1. **Is a provider and key configured?** Settings → AI → Agent providers, or the
   TUI's `/api-keys` menu. Keys live in the OS keychain, keyed by the
   `dev.phosphor.Phosphor` service name — a key saved by a pre-rename build is
   not visible to this one.
2. **Is the proxy setting blocking you?** See above: the default is `off`, which
   ignores `HTTPS_PROXY`.
3. **Open the wire inspector** (`ctrl-alt-i`) and send the message again. If
   nothing is captured, the request never left; if the request is there and the
   response is an error, the provider's own error text is in it.
4. **Check the log** for `[byop]` lines. `[byop] stream chunk error:` carries the
   provider's error body verbatim.
5. **If you need the full request**, restart with
   `ZAP_BYOP_LOG_FULL_REQUEST=1` and reproduce. Remember that this puts your
   prompt and conversation history in the log file.
6. `/usage` reports how much of the model's context window the conversation has
   consumed. A conversation that has filled its context window will fail in ways
   that look like a provider problem.

### Updates

There is nothing to troubleshoot, because the shipped build does not
auto-update. The update UI is hidden by a constant (`SHOW_AUTOUPDATE_UI =
false`), the `autoupdate` cargo feature is not in the default set and is not
enabled by the release bundle scripts, and with it off the
`workspace:check_for_updates` and `workspace:update_and_relaunch` commands are
never registered. The `updates.automatic_updates_enabled` setting exists and
defaults to `true`, but has no UI and no channel to check.

**To update, download the new release yourself** from the project's GitHub
releases page and install it the way you installed the current one.

The TUI's auto-updater is likewise not shipped: its release base URL is empty, so
the update path returns immediately. `WARP_TUI_DISABLE_AUTOUPDATE` and the
`general.autoupdate_enabled` setting are the switches that would disable it if it
were.

If a *previous* Windows install left a failed updater behind, its log
(`warp_update.log`) is collected into the log bundle when present.

### Graphics and rendering

Start with `phosphor-oss --dump-debug-info`, which prints the adapter list the
app will choose from.

| Setting | TOML path | Default |
|---|---|---|
| Prefer the integrated (low-power) GPU | `system.prefer_low_power_gpu` | `true` on Linux and FreeBSD; `true` on Windows unless the high-performance-GPU default is enabled; `false` on macOS |
| Preferred graphics backend (Windows only) | `system.preferred_graphics_backend` | unset, unless the Windows high-performance-GPU default is on, in which case `Vulkan`. Accepted values: `empty`, `dx12`, `vulkan`, `gl`, `metal`, `browser-web-gpu` |
| Force X11 instead of Wayland (Linux only) | `system.force_x11` | `true` under WSL, `false` otherwise |

On a discrete-GPU laptop where Phosphor picks the wrong adapter, flip
`system.prefer_low_power_gpu`. On Windows, `system.preferred_graphics_backend =
"dx12"` or `"vulkan"` selects the backend explicitly.

Both GPU settings apply to **new windows**; they are not retroactive, and
changing them shows a "changes apply to new windows" note rather than an error.

Three environment overrides exist:

- `WGPU_BACKEND` restricts the backend set wgpu may use (`vulkan`, `dx12`,
  `gl`, `metal`, comma-separated). It is wgpu's own variable, honoured but not
  surfaced anywhere in the UI.
- `WARP_ENABLE_WAYLAND=1` (Linux) forces Wayland regardless of
  `system.force_x11`.
- `WARP_USE_DIRECT_COMPOSITION=0` (or `false`) disables DirectComposition on
  Windows — worth trying for compositing artifacts or a black window.

**There is no software-rendering fallback.** If no usable GPU adapter is found,
startup fails; Phosphor will not fall back to a CPU rasterizer. It actively
deprioritises adapters known to be broken for it — old Mesa lavapipe, Raspberry
Pi V3D, some Intel UHD parts on Windows, old NVIDIA under Wayland — so on an
unusual setup `--dump-debug-info` may list adapters that Phosphor then declines
to use.

**Renderer failures at runtime are silent.** A lost surface or lost device is
logged and the renderer is transparently recreated. If recreation also fails the
window simply stops painting, with no dialog. The log is the only signal.

**Crash-at-startup recovery** (Linux and Windows). If Phosphor crashes during
startup it relaunches itself once with adjusted graphics settings, in this order:
switch to X11 (Linux, when Wayland is active and you have not explicitly set
`system.force_x11`), disable the OpenGL backend (Windows), force Vulkan
(Windows), then prefer the dedicated GPU (when you have not set
`system.prefer_low_power_gpu`). Only the X11 case tells you: a warning banner
noting that Xwayland can produce blurry text under fractional scaling. The
GPU-related recoveries deliberately say nothing, because they happen before there
is a window to say it in — which is worth knowing if Phosphor is quietly running
on a different adapter than you expect.

**Transparency.** If window opacity has no effect, the settings page will say
*"The selected graphics settings may not support rendering transparent
windows."* and point you at the graphics-backend and integrated-GPU settings.

### Shell integration is not detected

Phosphor drives the shell by writing a bootstrap script into the PTY and waiting
for the shell to answer with a `Bootstrapped` hook. Until that answer arrives the
session is only half-wired: blocks, the prompt, completions and command
detection all depend on it.

**What you will see.** Seven seconds after a session starts (sixty, if the
session came from an environment-variables object, since you may be typing
secret-manager passwords), an unbootstrapped session raises a banner reading
*"Seems like your shell is taking a while to start…"* with a **Show
initialization block** button. The banner auto-dismisses after thirty seconds, so
it does not become a permanent fixture on a shell that will never bootstrap (one
that `exec`s into `expect`, for instance).

If the shell exits before it bootstraps, the pane is deliberately kept open and
the hidden initialization block is force-shown, under a *"Shell process exited
prematurely!"* banner.

**Read the initialization block first.** It is hidden by default and contains
exactly the output that would tell you what went wrong. Three ways to see it:

- the **Show initialization block** button on the banner;
- **App → Show initialization block** in the menu;
- permanently, by setting `appearance.blocks.should_show_bootstrap_block = true`
  (default `false`).

If that is not enough, turn on **App → Debug → Enable Shell Debug Mode (-x) for
New Sessions** (debug builds only) and open a new session. The bootstrap script
re-execs the shell with `-x`, so every line it runs is echoed.

**Known-incompatible prompts.** When `terminal.input.honor_ps1` is on, the
bootstrap reports tags for shell configurations it knows it cannot work with, and
Phosphor raises an *"Your shell configuration is incompatible with Phosphor…"*
banner. Two are recognised today: a powerlevel10k too old to support the
integration (the banner links to p10k's own update instructions, and that link
works), and the `pure` prompt, which is unsupported.

**Environment variables.** Phosphor exports these into every local session; they
are the handshake, and a `.zshrc`/`.bashrc` that clobbers them will break
integration.

| Variable | Meaning |
|---|---|
| `TERM_PROGRAM=WarpTerminal` | How the shell and other tools identify the terminal. Not renamed in this fork; scripts that check for Warp will match. |
| `WARP_SESSION_ID` | Client-minted session id. Every hook quotes it back so Phosphor can validate the reply; block ids are `{session id}-{n}`. |
| `WARP_BOOTSTRAPPED` | Set to `1` at the end of the bootstrap. The whole script is wrapped in `if [[ -z $WARP_BOOTSTRAPPED ]]`, so this is the idempotence guard — if something in your rc files exports it early, the bootstrap never runs. |
| `WARP_IS_LOCAL_SHELL_SESSION=1` | Marks a local (non-recursive-SSH) session. The SSH integration logic only runs when it is set. |
| `WARP_HONOR_PS1` | `1`/`0`, mirroring `terminal.input.honor_ps1`. |
| `WARP_SHELL_DEBUG_MODE` | `1` when shell debug mode is on; the bootstrap adds `-x` to the re-exec. |
| `WARP_INITIAL_WORKING_DIR` | The bootstrap `cd`s here rather than the process setting its own cwd. |
| `WARP_IS_SUBSHELL` | Set to `1` by subshell initialization. When set, the bootstrap deliberately skips sourcing the login-shell rc files. |
| `WARP_USE_SSH_WRAPPER`, `WARP_SSH_REUSE_CONTROL_MASTER`, `SSH_SOCKET_DIR` | The legacy ControlMaster SSH wrapper. |
| `WARP_PATH_APPEND` | Extra `PATH` entries. |
| `HISTSIZE` / `HISTFILESIZE` and `WARP_INITIAL_HISTSIZE` / `WARP_INITIAL_HISTFILESIZE` | bash only. Phosphor sets a sentinel value so it can tell whether your rc files overrode it. |

`DESKTOP_STARTUP_ID` is deliberately removed from the child environment.

**Shell integration inside a subshell or over SSH ("Phosphorize").** Entering a
nested shell — a bare `bash`/`zsh`/`fish`, `docker run`/`docker exec`, `poetry
shell`, `pipenv shell`, `aws-vault exec`, `flox activate`, or `wsl` on Windows —
raises a footer offering to bootstrap it. The keybinding is
`terminal:warpify_subshell`. Two settings shape it:

| Setting | Default |
|---|---|
| `warpify.subshells.added_subshell_commands` — extra commands to treat as subshells | empty |
| `warpify.subshells.subshell_commands_denylist` — commands never to offer it for | empty |

For SSH, Phosphor can bootstrap the *remote* shell through tmux. That path is
gated on **three** conditions, and the middle one is off by default, so most
users will never see the offer:

| Setting | Default |
|---|---|
| `warpify.ssh.enable_ssh_warpification` | `true` |
| `warpify.ssh.use_ssh_tmux_wrapper` (macOS and Linux only) | **`false`** |
| `warpify.ssh.ssh_hosts_denylist` | empty |

Turn `use_ssh_tmux_wrapper` on and a **Phosphorize SSH session** button appears
in the footer once you are logged in (`terminal:warpify_ssh_session`). Phosphor
then probes the remote host, installs tmux through its package manager if
needed, and re-enters the session under tmux. It gives up after eight seconds.

Enabling the tmux wrapper **disables** the legacy ControlMaster wrapper — they
are mutually exclusive. If your completions have stopped working over SSH,
Phosphor raises a banner suggesting exactly this: *"Enabling tmux phosphorization
in settings may resolve this issue."*

When the remote side cannot be phosphorized you get a specific message —
tmux not installed, tmux older than 3.0, tmux failed to execute, a timeout, or an
unsupported shell (only bash, zsh and fish work; PowerShell does not) — plus two
escape hatches: **Phosphorize without TMUX** and **Continue without
Phosphorization**.

> **The "More info" and "Learn more" links in these banners go nowhere.** The
> known-issues, prompt-compatibility and ControlMaster-troubleshooting URLs are
> empty strings in this fork, because they pointed at Warp's documentation site.
> The powerlevel10k link is the one that still works.

**A coupling worth knowing about.** `terminal.input.honor_ps1` (default `false`)
and the input box type are wired together: turning `honor_ps1` on forces the
Classic input box, turning it off forces Universal, and selecting Universal
forces `honor_ps1` off. If you set one and the other changes underneath you, that
is why. At runtime, PS1 input is only used when the shell has actually signalled
PS1 support; otherwise Phosphor falls back to its own input.

### Fonts

**A font that cannot be loaded fails silently.** This is the single most
confusing font behaviour: if `appearance.text.font_name` names a font that is not
installed, or a font with no `m` glyph (Phosphor assumes one), the load returns
nothing, a warning goes to the log — `Failed to load font: <name> due to error …`
or `… because it didn't contain the character m` — and the display simply does
not change. There is no toast, no banner and no error on the settings page. If
you edited `settings.toml` by hand and nothing happened, check the log.

At startup an unresolvable monospace font falls back to the bundled **Hack**; an
unresolvable UI font falls back to bundled Roboto (Segoe UI on Windows).

The font picker in **Settings → Appearance** only lists fonts Phosphor
successfully enumerated, with Hack always re-inserted so you can get back to the
default. Picking from the list therefore cannot produce this failure — hand-
editing the TOML can. On Linux and FreeBSD the dropdown does not preview each
entry in its own font; that is deliberate, not a rendering bug.

| Setting | TOML path | Default |
|---|---|---|
| Terminal font | `appearance.text.font_name` | `Hack` |
| Per-glyph fallback font | `appearance.text.fallback_font_name` | empty (no extra fallback) |
| Font size | `appearance.text.font_size` | `13.0` |
| Font weight | `appearance.text.font_weight` | `normal` |
| Line height ratio | `appearance.text.line_height_ratio` | `1.2` |
| AI-pane font | `appearance.text.ai_font_name` | `Hack` |
| Match the AI font to the terminal font | `appearance.text.match_ai_font` | `false` |
| UI font | `appearance.text.ui_font_name` | empty (bundled Roboto, or Segoe UI on Windows) |
| UI font size | `appearance.text.ui_font_size` | `12.0` (accepted range 8–20) |
| Notebook font size | `appearance.text.notebook_font_size` | `14.0` (clamped to 5–25) |
| Match notebook size to terminal size | `appearance.text.match_notebook_to_monospace_font_size` | `true` |
| Minimum-contrast enforcement | `appearance.text.enforce_minimum_contrast` | named colours only |
| Thin strokes (macOS only) | `appearance.text.use_thin_strokes` | on high-DPI displays |
| Markdown heading scales | `appearance.text.markdown_heading_h1_scale` … `h6_scale` | `2.0`, `1.5`, `1.17`, `1.0`, `0.83`, `0.75` (each clamped to 0.1–5.0) |

Missing glyphs — emoji, CJK, Arabic, Devanagari — are handled by a bundled
fallback set plus the platform's own font fallback, independently of
`fallback_font_name`. `fallback_font_name` is a *per-cell* fallback for the
terminal grid; leave it empty unless you have a specific second font in mind.

### An MCP server will not start

**Read the server's log.** Every MCP server gets its own log file, named after
the server's template UUID:

| Platform | Directory |
|---|---|
| macOS | `~/Library/Application Support/dev.phosphor.Phosphor/mcp/` |
| Linux | `~/.local/state/phosphor/mcp/` |
| Windows | `%LOCALAPPDATA%\phosphor\Phosphor\data\logs\mcp\` |

The file holds the server's stderr plus transport events, rotates at 10 MiB with
5 kept generations, and **the whole directory is wiped the first time an MCP
server is spawned in a process** — so it always reflects the current run, and
there is nothing there to read after a restart until a server starts again.

In the GUI, **Settings → Agents → MCP servers** shows a card per server with a
**View logs** button. It does not open a viewer: it splits a terminal pane to the
right and pre-fills a `tail -f` on the log file, which you then run. The TUI's
`/mcp` menu has no log viewer at all — `tail` the path above by hand.

Note that the GUI's log bundle includes `mcp/*.log`; the TUI's `/view-logs`
bundle does not.

**Server states.** A card or `/mcp` row shows one of: offline, starting,
authenticating, running, shutting down, or failed. A failed server shows the
reason both on the card and inline in the TUI (`failed · <message>`), and Enter
retries it. The messages are specific enough to act on:

- *Failed to establish connection* — the command could not be launched, or the
  URL is unreachable.
- *Server returned an error. Please check server logs for details.*
- *Connection closed unexpectedly. The server may have crashed.*
- *Connection timed out after N seconds. The server may be unresponsive.*
- *Server sent an unexpected response. The server may be incompatible.*

Config-file problems are reported separately from server failures. A file that
cannot be read, cannot be parsed, or refers to a `${VAR}` that is not set appears
as its own non-selectable row: *"<provider> config error"* with the path and the
message.

**Where the config lives.**

| Provider | Global | Per project |
|---|---|---|
| Phosphor | `~/.phosphor/.mcp.json` | `<repo>/.warp/.mcp.json` |
| Claude | `~/.claude.json` | `<repo>/.mcp.json` |
| Codex | `~/.codex/config.toml` | `<repo>/.codex/config.toml` |
| Other agents | `~/.agents/.mcp.json` | `<repo>/.agents/.mcp.json` |

The project-scoped Phosphor directory is `.warp`, not `.phosphor` — a lineage
name that was not renamed.

**Three things that make a correctly-configured server "not start":**

1. **Third-party configs are off by default.** Reading Claude's, Codex's and
   other agents' config files requires
   `agents.mcp_servers.file_based_mcp_enabled = true` (default `false`;
   Settings → AI → *Auto-spawn servers from third-party agents*).
2. **Project-scoped servers never auto-spawn**, for any provider. You start them
   from the server card, every time.
3. **`PATH`.** stdio servers are launched with a `PATH` Phosphor captured from a
   terminal session, not the one your login shell builds. A server that runs fine
   when you type the command yourself but fails to launch here is almost always
   this. Give the command an absolute path in the config.

**From the CLI**, the entire MCP surface is one read-only command:

```sh
oz mcp list          # UUID and name for every runnable server
```

`--output-format json` (or `ndjson`, `text`) applies. There is no
`oz mcp add`/`remove`/`start`/`stop`/`logs`. To attach servers to a single agent
run, `oz agent run --mcp <spec>` takes a JSON file path or inline JSON, and may
be repeated.

> **Do not pass a bare UUID to `--mcp`.** It is accepted and written into the
> merged config as a Warp-managed server reference, which nothing in this fork
> can resolve — the entry ends up with no command and no URL, and the agent fails
> without an explanation.

### PTY-level debugging

Two `DebugSettings` toggles help when the problem is between Phosphor and the
shell itself, rather than in the UI. Both appear in the macOS **App → Debug**
submenu, and that submenu is only built in debug builds (`FeatureFlag::DebugMode`
comes from `DEBUG_FLAGS`, which the OSS binary adds only under
`cfg!(debug_assertions)`).

- **Enable Shell Debug Mode (-x) for New Sessions** sets `WARP_SHELL_DEBUG_MODE`
  in newly spawned sessions.
- **Enable PTY Recording Mode** writes raw PTY bytes to
  `<state dir>/pty_recordings/<timestamp>-<id>.pty.recording`.

These are private settings: they have no TOML path and are stored in
`user_preferences.json` in the config directory, not in `settings.toml`.

---

## Reference

### Settings

| Setting | What it does | Default | Where |
|---|---|---|---|
| `privacy.telemetry_enabled` | Retained for a telemetry channel that does not exist. The toggle never renders and the setting is never read. | `false` | `settings.toml`; no UI |
| `privacy.crash_reporting_enabled` | Installs the local panic hook that writes a full backtrace to the log. Uploads nothing. | `false` | Settings → Privacy → Send crash reports |
| `privacy.custom_secret_regex_list` | Extra regexes for secret redaction. | empty | Settings → Privacy → Custom secret redaction |
| `network.proxy_mode` | `off` / `system` / `custom`. `off` ignores proxy environment variables too. | `off` | Settings → Network |
| `network.proxy_url` | Proxy URL for `custom` mode, e.g. `http://proxy.corp:8080`. | empty | Settings → Network |
| `network.proxy_username` | Basic-auth username for `custom` mode. | empty | Settings → Network |
| `network.proxy_no_proxy` | Comma-separated bypass list, e.g. `localhost,127.0.0.1,.internal`. | empty | Settings → Network |
| *(proxy password)* | Basic-auth password. Stored in the OS keychain as `ProxyPassword`, never in `settings.toml`. | unset | Settings → Network |
| `system.prefer_low_power_gpu` | Prefer the integrated GPU. | platform-dependent (see above) | Settings, or `settings.toml` |
| `system.preferred_graphics_backend` | Force a wgpu backend. Windows only. | unset (or `Vulkan`) | `settings.toml` |
| `system.force_x11` | Force X11 instead of Wayland. Linux only. | `true` on WSL, else `false` | `settings.toml` |
| `terminal.input.honor_ps1` | Use your shell's PS1 prompt instead of Phosphor's. Also forces the input box type. | `false` | Settings → Appearance |
| `appearance.blocks.should_show_bootstrap_block` | Always show the shell initialization block. | `false` | `settings.toml` |
| `appearance.text.font_name` | Terminal font. An unresolvable name fails silently — see Fonts. | `Hack` | Settings → Appearance |
| `appearance.text.fallback_font_name` | Per-cell fallback font for the terminal grid. | empty | Settings → Appearance |
| `warpify.ssh.enable_ssh_warpification` | Allow bootstrapping a remote shell over SSH. | `true` | Settings → Phosphorize |
| `warpify.ssh.use_ssh_tmux_wrapper` | Required for the SSH "Phosphorize" offer to appear. macOS and Linux only. | `false` | Settings → Phosphorize |
| `warpify.ssh.ssh_hosts_denylist` | Hosts never to offer phosphorization for. | empty | Settings → Phosphorize |
| `warpify.subshells.added_subshell_commands` | Extra commands to treat as subshells. | empty | Settings → Phosphorize |
| `warpify.subshells.subshell_commands_denylist` | Commands never to offer phosphorization for. | empty | Settings → Phosphorize |
| `agents.mcp_servers.file_based_mcp_enabled` | Auto-detect MCP servers from third-party agents' config files. | `false` | Settings → AI → MCP servers |
| `updates.automatic_updates_enabled` | Would enable background update checks. Inert: this build has no update channel and no UI for it. | `true` | `settings.toml` only |
| `general.autoupdate_enabled` | TUI auto-updater. Inert for the same reason. | `true` | `settings.toml` only |
| `account.is_settings_sync_enabled` | Only controls whether a "local only" badge is drawn. Nothing syncs. | `false` | `settings.toml` |
| `DebugSettings.is_shell_debug_mode_enabled` | Sets `WARP_SHELL_DEBUG_MODE` in new sessions. | `false` | Private setting (`user_preferences.json`); App → Debug menu, debug builds |
| `DebugSettings.recording_mode` | Record raw PTY bytes to disk. | off unless built with the `recording_mode` feature | Private setting; App → Debug menu, debug builds |
| `DebugSettings.are_in_band_generators_for_all_sessions_enabled` | Use in-band generators for completions/highlighting in every session. | `false` | Private setting; App → Debug menu, debug builds |
| `DebugSettings.force_disable_in_band_generators` | Kill-switch: never use in-band generators. Takes precedence over the setting above. Breaks completions in sessions with no alternative. | `false` | Private setting (storage key `DisableInBandCommands`) |
| `DebugSettings.show_memory_stats` | Show memory stats. Only ever visible in debug/dogfood builds and never under test. | `true` | Private setting |

### Environment variables

| Variable | Effect |
|---|---|
| `RUST_LOG` | Log filter. Default level is `info`. |
| `ZAP_LOG_STDOUT` | GUI logs to stderr instead of the log file. |
| `ZAP_BYOP_LOG_FULL_REQUEST` | Log full BYOP request content instead of shapes and digests. Any value except empty, `0` or `false` enables it. |
| `WARP_STARTUP_TRACE=1` | Print the startup timing table. |
| `WARP_SHELL_DEBUG_MODE` | Set by Phosphor in spawned sessions when shell debug mode is on. |
| `WARP_ENABLE_WAYLAND=1` | Linux: force Wayland regardless of `system.force_x11`. |
| `WARP_USE_DIRECT_COMPOSITION=0` | Windows: disable DirectComposition. |
| `WGPU_BACKEND` | wgpu's own backend restriction, e.g. `vulkan` or `vulkan,dx12`. |
| `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY` (and lower-case forms) | Honoured only when `network.proxy_mode` is `system`. |
| `WARP_DATA_PROFILE` | Debug builds only: suffixes the config/data/state directories and the keychain namespace, so you can run an isolated instance. Ignored in release builds. |
| `OZ_AGENT_MAILBOX_ROOT` | Overrides the on-disk agent mailbox root (default `<state dir>/oz/…`). |
| `WARP_TUI_DISABLE_AUTOUPDATE` | Disables the TUI auto-updater for one launch. Moot: it is not shipped. |

### Commands

| Command | Surface | What it does |
|---|---|---|
| `phosphor-oss --dump-debug-info` | shell | Print the graphics/environment report and exit. |
| `phosphor-oss minidump-server <socket>` | worker | Hidden. Writes local minidumps. Never started by this build. |
| `phosphor-oss completions [shell]` | shell | Print shell completions to stdout. |
| **View Phosphor logs** | GUI palette | Build a log bundle and reveal it. |
| **Dump debug info** | GUI palette | Type the `--dump-debug-info` command into the active session. |
| **Toggle BYOP wire inspector** | GUI, `ctrl-alt-i` / `cmd-ctrl-i` | Open the live LLM traffic capture. |
| `/view-logs` | TUI only | Build a log bundle and reveal it. |
| `/copy-debugging-id` | GUI + TUI | Copy this conversation's local identifier. |
| `/usage` | GUI + TUI | Report context-window usage for the active conversation. |
| `/cost` | GUI + TUI | Report spend, using the token prices *you* configured for the provider. |
| `/status` | TUI only | Open the read-only status menu. |

---

## Coming from Warp: what is not here

Phosphor is Warp with Warp's backend removed. That is a deliberate design
decision, not a to-do list, and every entry below has a recorded rationale in
`DECLINED.md`. Where a local replacement exists, it is named — that is usually
the part you actually want.

### Account, billing, teams

| Warp feature | What it did | In Phosphor |
|---|---|---|
| **Warp account / login / logout** | Signed you in to Warp's backend; gated everything else. | **Absent.** There is no account and nothing to sign in to. The `/logout` slash command is deliberately not registered — its handler is a documented no-op, and a menu row that does nothing when selected is worse than no row. `oz whoami` still runs but prints a fixed local placeholder identity, not a real account. |
| **Credits, billing, paid tiers, upgrade flows** | Metered agent usage against a subscription. | **Absent.** You pay your provider directly, so there is no balance to show. **Replacement: `/usage` and `/cost`.** `/usage` reports the one budget a BYOP conversation actually spends against — the percentage of the model's context window used and remaining. `/cost` multiplies the token counts the provider reported by the per-model rates *you* configured. Where a rate is not configured it says so in words rather than rendering a plausible-looking `$0.00`. |
| **Teams and workspaces** | Shared folders, workflows, org policy, admin panels. | **Absent, permanently.** `UserWorkspaces::has_teams()` and `current_team()` are hard-coded to "none". The consequence to know about: org-level command denylists, workspace AI-autonomy policy and enterprise secret-redaction rules are inert, because there is no server to deliver them. |
| **Agent commit/PR attribution toggle** | A user preference uploaded to Warp's server, where the *server* decided whether to add a `Co-Authored-By` line. | **Absent.** The client never implemented the behaviour even upstream; there is no local attribution emitter to toggle. |
| **Settings sync across devices** | Synced settings through your account. | **Absent.** The setting key survives but nothing reads it. Copy `settings.toml` by hand. |

### Sharing and cloud sessions

| Warp feature | What it did | In Phosphor |
|---|---|---|
| **Session / block sharing, shareable links** | Published a session to Warp's servers for someone else to open. | **Absent.** Requires a backend to host the session and resolve recipients. |
| **`oz agent run --share`** | Shared an agent session with `team:` / `public:` / `user@…` recipients. | **Parses, does nothing, and is hidden from `--help`.** Kept parseable so an existing script does not break at the argument parser; the code hard-codes "not shared". |
| **Warp Drive cloud sync** | Cloud-stored, synced workflows, notebooks and prompts. | **The Library is local only.** Objects live in the local database and on disk; nothing syncs and nothing is fetched. `warp.dev/drive/...` links still parse but resolve to nothing. |
| **Shared-session heartbeat** | Kept a shared session alive against the server. | **Absent** — it served a layer that no longer exists. |
| **Cloud conversation storage / history** | Conversations stored server-side and available on any machine. | **Absent.** Conversations are local. The Privacy page's cloud-storage toggle was not ported because there is nothing local for it to control. |

### Agents and AI as a service

| Warp feature | What it did | In Phosphor |
|---|---|---|
| **Warp AI as a hosted service** | Warp ran the models and billed you for them. | **Replaced by BYOP.** You configure providers and keys; requests go straight from your machine to the endpoint you named. |
| **Cloud agents / RunAgents / hosted runners** | Ran agents on Warp's infrastructure, with a host picker and orchestration controls. | **Absent.** **Replacement: `/orchestrate`,** which runs child agents as local processes. Agent-*invoked* agent spawning is declined too: orchestration here is user-invoked. |
| **Agent-to-agent messaging via `oz run message`** | Routed messages between agents through Warp's server. | **Replaced by an on-disk mailbox.** `oz agent message send` / `oz agent message list` read and write a plain filesystem mailbox under `<state dir>/oz/`, overridable with `OZ_AGENT_MAILBOX_ROOT`. |
| **Warp Environments** | Cloud-defined dev environments tied to source repos and code forges. | **Absent.** The whole types layer lived in a crate this fork deleted. |
| **Cloud codebase indexing / codebase search** | Server-side index of your repository. | **Absent** from the TUI's command set by design. |
| **Warp's team-scoped "global skills" policy** | An admin-delivered list of skill specs filtered onto your machine. | **Absent.** Local skills work normally: per-directory skills and `~/.phosphor/skills/`. |
| **The InitProject wizard** | First-run onboarding for cloud agent mode. | **Absent.** |
| **"Oz updates" zero-state feed, feature-intro popovers, the Warp Agent CLI promo modal** | Warp-branded in-product content. | **Absent.** |

### MCP

| Warp feature | What it did | In Phosphor |
|---|---|---|
| **The hosted MCP gallery** | A curated catalog of MCP servers fetched from Warp's servers, surfaced in the `/mcp` catalog. | **Absent.** The gallery manager is a stub that always returns empty. The other three catalog sources — local config, locally saved templates, and running servers — are all present. |
| **"Well-known" MCP server ids** (`linear`, `notion`, …) | A bare id whose meaning was resolved by Warp's server. | **Not supported.** Configure the server explicitly in `.mcp.json` with its command or URL. |
| **Warp-managed MCP servers (`warp_id`)** | Server instances Warp hosted and managed for you. | **Unresolvable.** A hand-written `warp_id` entry will be accepted by the config parser and then fail at use, because nothing here can resolve it. |
| **Team-shared MCP templates** | Templates shared by a team member, attributed to their profile. | **Absent.** Locally saved templates all collapse to one plain "Template" source. |

### Platform integrations

| Warp feature | What it did | In Phosphor |
|---|---|---|
| **Oz platform plugins** (`oz-harness-support@claude-code-warp`, `orchestration@codex-warp`) | Installed skills into Claude Code / Codex that called back into `oz harness-support` and Warp's server. | **Removed.** Every skill in those plugins would fail here, because `oz harness-support` does not exist. Wiring them up would turn a working local Codex launch into a hard failure. |
| **CLI-agent notification plugins** (`claude-code-warp`, `codex-warp`, `gemini-cli-warp`, `opencode-warp`) | Let Claude Code / Codex / Gemini CLI / opencode report progress to the terminal. | **These work.** They are installed by running the agent's own CLI against GitHub or npm — never a Warp registry — and once installed they talk to the terminal over a local OSC 777 protocol. The Phosphor UI calls them "the notification plugin"; the exact package name to type is shown in the runnable command beside each instruction. |
| **Warp's SSH host manager** | Stored hosts and keys inside the app. | **Deliberately not implemented.** `~/.ssh/config` and `ssh-agent` are the source of truth and Phosphor never writes to them. SSH itself is fully supported, including the remote-server extension and the tmux wrapper (which Warp deprecated and Phosphor keeps). |
| **Voice input / transcription** | Recorded audio and transcribed it with Warp's hosted Wispr STT. | **Not shipped.** Audio capture code still exists and works; the transcriber is hard-disabled, because the BYOP protocol cannot carry audio. The `/voice` command and the TUI voice composer are deliberately not registered rather than shipped as a control that always fails. |
| **Screen and session recording** | Local `ffmpeg` capture, with artifact upload to Warp. | **Not shipped.** Not a cloud limitation — a product decision. |
| **xAI / Grok subscription OAuth** | Signed in with an xAI subscription instead of an API key. | **Not supported.** API keys only. An xAI key works — add it as a custom agent provider. This one is a product decision, not a cloud limitation: the OAuth flow never touched Warp's servers. |
| **AWS Bedrock OIDC role assumption** (`--bedrock-role-arn`) | Assumed an IAM role for Bedrock access. | **Absent.** |
| **Network log console** | An in-app console of the app's own HTTP traffic to Warp. | **Absent.** The BYOP wire inspector covers the equivalent need for model traffic. |
| **Warp's docs site, Slack and privacy-policy links** | In-app links to Warp's properties. | **Removed, and the menu entries that opened them now do nothing** — the URL constants are empty strings. |

### Telemetry, updates and crash reporting

| Warp feature | What it did | In Phosphor |
|---|---|---|
| **Usage telemetry / app analytics** | Opt-out collection of usage events and some console interactions. | **Channel physically removed**, and the toggle never renders. See [What leaves the machine](#what-leaves-the-machine). |
| **Uploaded crash reports** | Crashes sent to a crash service. | **Local only.** The toggle installs a panic hook that writes a backtrace to your log. |
| **Auto-update** | Background updates from Warp's release channel. | **Not shipped in this build.** Download new releases yourself. |

<!-- SOURCES

## Paths and identity
- app id `AppId::new("dev", "phosphor", "Phosphor")`: crates/warp_core/src/channel/state.rs:46 (default), app/src/bin/phosphor_oss.rs:30 (GUI binary), crates/warp_tui/src/bin/oss.rs:40 (TUI binary)
- Channel is `Channel::Oss` for both shipped binaries: app/src/bin/phosphor_oss.rs:27, crates/warp_tui/src/bin/oss.rs:16
- macOS config dir name `.phosphor` for Oss: crates/warp_core/src/paths.rs:116
- home dotfile dir name `.phosphor` for Oss: crates/warp_core/src/paths.rs:43
- config_local_dir / data_dir / state_dir: crates/warp_core/src/paths.rs:146, :132, :174
- Linux ProjectDirs app name lowercased to `phosphor`: crates/warp_core/src/paths.rs:325-331; ProjectDirs::from(qualifier, organization, app_name): crates/warp_core/src/paths.rs:341
- settings.toml path: app/src/settings/mod.rs:648 (`config_local_dir().join("settings.toml")`)
- user_preferences.json path (private settings): app/src/settings/mod.rs:643
- private settings have no toml_path and go to the JSON store: crates/settings/src/macros.rs:236-237, crates/integration/src/test/settings_private.rs:1-4
- storage-identity rename with no migration: README.md, "Migrating from Zap, OpenWarp, or Warp" section; docs/migrate-from-warp.md still names `zap` destinations (docs/migrate-from-warp.md:47-51)

## Logging
- LogFrontend subdirectories `warp-cli` (TUI) and `oz` (CLI), GUI at the base: crates/warp_logging/src/native.rs:18-19, :40-45
- rotation counts 5 (GUI) / 10 (TUI, CLI): crates/warp_logging/src/native.rs:16-17, :48-52
- default level Info, then `parse_default_env()` i.e. RUST_LOG: crates/warp_logging/src/native.rs:732, :752 (env_logger 0.10.2, Cargo.lock:4595)
- per-crate quieting of naga / wgpu_core / wgpu_hal / tantivy: crates/warp_logging/src/native.rs:736-750
- log base directory per platform: crates/warp_logging/src/native.rs:823-836 (macOS `~/Library/Logs/`, Linux `state_dir()`, Windows `state_dir()/logs`); WARP_LOGS_DIR = "logs": crates/warp_core/src/paths.rs:35
- logfile names `phosphor.log` / `phosphor-tui.log`: app/src/bin/phosphor_oss.rs:38, crates/warp_tui/src/bin/oss.rs:43
- LaunchMode -> LogFrontend mapping: app/src/lib.rs:608-616; log destination and ZAP_LOG_STDOUT: app/src/lib.rs:565-600
- LogConfig built with `..Default::default()`, so `max_file_size_bytes: None` and in-session rotation is off: app/src/lib.rs:947-958; field doc crates/warp_logging/src/lib.rs:31-38
- `.old.N` rotation naming: crates/warp_logging/src/native.rs:211-265
- log_panics installed when logging to a file: crates/warp_logging/src/native.rs:805-808
- WARP_STARTUP_TRACE: crates/warp_core/src/interval_timer.rs:64-71, app/src/lib.rs:1724

## BYOP diagnostics
- ZAP_BYOP_LOG_FULL_REQUEST const and reader: app/src/ai/agent_providers/chat_stream.rs:3184, :3188-3194
- two-tier default/opt-in description, and the "RUST_LOG=debug is a verbosity switch, not a privacy opt-in" note: app/src/ai/agent_providers/chat_stream.rs:3125-3183
- MCP tool-name server-segment digest: app/src/ai/agent_providers/chat_stream.rs:3057-3097
- proxy userinfo redaction: app/src/ai/agent_providers/chat_stream.rs:3028-3055
- wire inspector ring buffer, cap 200, armed on open, cleared only explicitly: app/src/ai/agent_providers/wire_log.rs:1-38
- wire inspector keybinding cmd-ctrl-i / ctrl-alt-i: app/src/workspace/mod.rs:662-669; label app/i18n/en/warp.ftl:1911

## Log bundle
- create_log_bundle_zip / write_log_bundle_zip_to / default_log_bundle_filename exports: crates/warp_logging/src/lib.rs:49-52; filename format crates/warp_logging/src/native.rs:533-540
- GUI "View logs" implementation: app/src/workspace/view.rs:5964-5997; "Export logs" with save dialog: app/src/workspace/view.rs:6006-6078
- bundle contents (manifest.txt, warp-minidump.log, warp_update.log, mcp/*.log) and exclusions: app/src/workspace/view.rs:6082-6182
- palette binding `workspace:view_logs`: app/src/workspace/mod.rs:1605-1611; label app/i18n/en/warp.ftl:2034
- About page "Export logs…" strings: app/i18n/en/warp.ftl:659-662; dispatch app/src/settings_view/about_page/mod.rs:119-123
- TUI `/view-logs` handler: crates/warp_tui/src/terminal_session_view.rs:4513-4545; command definition app/src/search/slash_command_menu/static_commands/commands.rs:547-556; TUI-only guard app/src/search/slash_command_menu/static_commands/mod.rs:344-358

## --dump-debug-info
- subcommand declaration with `long_flag = "dump-debug-info"`: crates/warp_cli/src/lib.rs:407-410
- output contents: app/src/debug_dump.rs:11-113
- GUI palette action types the command rather than running it: app/src/workspace/view.rs:7912-7961; fixed binding "Dump debug info": app/src/workspace/mod.rs:117-121

## /copy-debugging-id
- payload construction, dogfood vs user-facing channels: app/src/ai/agent/api.rs:63-93; Channel::Oss is not dogfood: crates/warp_core/src/channel/mod.rs:30-35
- assertion that Oss yields the JSON blob: app/src/ai/agent/api_tests.rs:167-190
- GUI dispatch: app/src/terminal/input/slash_commands/mod.rs:770-793; TUI dispatch: crates/warp_tui/src/terminal_session_view.rs:4589-4610
- command definition and availability: app/src/search/slash_command_menu/static_commands/commands.rs:455-462
- user-facing strings: app/i18n/en/warp.ftl:2693-2695; TUI hint constants crates/warp_tui/src/terminal_session_view.rs:333-334

## Crash reporting
- `is_crash_reporting_available()` hard-coded false: crates/warp_core/src/channel/state.rs:173-175
- crash_reporting::init reads and subscribes to the setting: app/src/crash_reporting/mod.rs:163-222
- init_local_crash_reporting installs only a panic hook writing a backtrace to the log: app/src/crash_reporting/mod.rs:259-296
- set_user_id is a no-op: app/src/crash_reporting/mod.rs:311
- privacy toggle gated on FeatureFlag::CrashReporting rather than the availability check, with rationale: app/src/settings_view/privacy_page.rs:24-31, :1538-1545
- CrashReporting is in RELEASE_FLAGS: crates/warp_features/src/lib.rs:912; crash_reporting cargo feature not in `default` (app/Cargo.toml:474-479, :480+) but set by the release bundler (script/linux/bundle:24)
- MinidumpServer subcommand, hidden: crates/warp_cli/src/lib.rs:298-303; dispatch app/src/lib.rs:832-838
- dump path `<state_dir>/logs/crash-dumps/zap-minidump-<uuid>.dmp`: app/src/crash_reporting/local_minidump.rs:119-131
- minidump server log path `<state_dir>/logs/warp-minidump.log`: app/src/crash_reporting/local_minidump.rs:98-102
- send_crash_report is a log::error! and nothing else: app/src/crash_reporting/local_minidump.rs:188-203
- module doc "without uploading them to a remote crash service": app/src/crash_reporting/local_minidump.rs:1-3
- crash tags are local diagnostics: app/src/crash_reporting/mod.rs:45-97
- local_minidump::init / MinidumpGuard::start have no caller: `grep -rn "local_minidump::init\|MinidumpGuard::start" app crates` returns only the definition at app/src/crash_reporting/local_minidump.rs:38-50 and its internal use at :42
- privacy toggle defaults flipped from Warp's on to off, with rationale: app/src/settings/privacy.rs:92-96; DECLINED.md "Privacy toggle defaults" row

## Telemetry
- send_telemetry_* macros are no-ops: crates/warp_core/src/telemetry.rs:1-57
- is_telemetry_available() hard-coded false: crates/warp_core/src/channel/state.rs:169-171; telemetry_file_name() empty: :165-167
- AppAnalyticsWidget::should_render returns false because of it: app/src/settings_view/privacy_page.rs:1404-1416
- should_collect_ai_ugc_telemetry returns false unconditionally: app/src/ai/blocklist/telemetry_banner.rs:4-6
- WarpDrivePrivacySettings group, toml paths and defaults: app/src/settings/privacy.rs:101-119
- no `global_ai_analytics_collection` anywhere in the tree (only a TODO.md mention at TODO.md:4385)
- server_root_url is a blackhole sentinel `http://192.0.2.0:9`: crates/warp_core/src/channel/state.rs:187-195, crates/warp_core/src/channel/config.rs:30
- is_cloud_disabled() returns true: crates/warp_core/src/channel/state.rs:208-210
- account.is_settings_sync_enabled has no production consumer: app/src/settings/cloud_preferences.rs:17-27; the only readers draw a "local only" badge (app/src/settings_view/settings_page.rs:522, features_page.rs:6469, keybindings.rs:1158, workspace/view.rs:20927); `is_setting_syncable_on_current_platform` (crates/settings/src/lib.rs:323) is referenced only by tests
- AuthState is a local placeholder: app/src/auth/mod.rs:1-20, :189-219; whoami reads it: app/src/ai/agent_sdk/admin.rs:37-63
- privacy page: what was deliberately not ported (cloud conversation storage, network log, data management, docs.warp.dev link): app/src/settings_view/privacy_page.rs:9-20
- crash-report toggle copy stating nothing is uploaded: app/i18n/en/warp.ftl:933-934

## Networking
- NetworkSettings group, toml paths, all defaults: app/src/settings/network.rs:77-110; ProxyMode default Off: app/src/settings/network.rs:44-48
- module doc claiming a `system` default is stale relative to the code: app/src/settings/network.rs:8-9 vs :46, :80
- settings-page description text also says system is the default: app/i18n/en/warp.ftl:874
- Off means reqwest `no_proxy()` including environment variables: crates/http_client/src/proxy.rs:14-16, :84-87
- ProxyMode::from_str_lenient falls back to Off: crates/http_client/src/proxy.rs:53-60
- System mode delegates to reqwest's platform proxy detection: crates/http_client/src/proxy.rs:9-13, :85
- websocket env-var precedence HTTPS_PROXY/ALL_PROXY and HTTP_PROXY/ALL_PROXY, both cases: crates/websocket/src/proxy.rs:88-141, and `read_env_var` handling of the lower-case spelling
- NO_PROXY matching rules: crates/websocket/src/proxy.rs:152-170
- proxy config pushed into both global slots at startup and on change: app/src/settings/init.rs:258-268, :380-401
- password stored in the OS keychain under `ProxyPassword`, never in settings.toml: app/src/settings/network_secrets.rs:1-14
- long-lived clients need a restart: crates/http_client/src/proxy.rs:22-24; app/i18n/en/warp.ftl:872
- Network page strings incl. Test connection semantics: app/i18n/en/warp.ftl:870-899
- WARP_EXTRA_HTTP_HEADERS is read only on Channel::Integration, so it is not a user-facing switch: crates/http_client/src/lib.rs:50-53

## Graphics
- GPUSettings group with toml paths and defaults: app/src/settings/gpu.rs:9-29
- GraphicsBackend variants: crates/warpui_core/src/platform/mod.rs:726-743
- ForceX11 setting, default `linux::is_wsl()`: app/src/settings/linux.rs:6-15
- WARP_ENABLE_WAYLAND: crates/warpui/src/platform/linux/mod.rs:66-69; consumed at app/src/lib.rs:1194-1198
- WARP_USE_DIRECT_COMPOSITION: crates/warpui/src/rendering/wgpu/mod.rs:35-41

## Updates
- SHOW_AUTOUPDATE_UI = false, with the reason: app/src/settings_view/about_page/mod.rs:62-66
- FeatureFlag::Autoupdate deliberately not in RELEASE_FLAGS: crates/warp_features/src/lib.rs:895-907
- `autoupdate` cargo feature exists but is not in `default`: app/Cargo.toml:449; release bundler features are `release_bundle,crash_reporting`: script/linux/bundle:24
- update keybindings registered only under FeatureFlag::Autoupdate: app/src/workspace/mod.rs:1169-1188
- AutoupdateSettings default true, toml path `updates.automatic_updates_enabled`: app/src/settings/autoupdate.rs:3-13
- TuiAutoupdateSettings default true, toml path `general.autoupdate_enabled`, WARP_TUI_DISABLE_AUTOUPDATE: app/src/settings/tui_autoupdate.rs:8-21
- TUI updater exits when releases_base_url is empty: crates/warp_tui/src/autoupdate.rs:660-663; releases_base_url is empty because autoupdate_config is None: crates/warp_core/src/channel/state.rs:177-185, app/src/bin/phosphor_oss.rs:39
- OSS GUI update path would use the GitHub Releases API for jwp2987/phosphor if enabled: app/src/autoupdate/github.rs:14-15, :99-104
- Linux update methods (AppImage in place, package manager -> manual): app/src/autoupdate/linux.rs:21-58, :288-312
- windows updater log `warp_update.log` collected into the bundle: app/src/workspace/view.rs:6138-6142

## Debug settings and PTY
- DebugSettings group, all private, defaults: app/src/settings/debug.rs:21-58
- macOS App > Debug submenu, only built when FeatureFlag::DebugMode is on: app/src/app_menus.rs:171-180, :691-694
- DEBUG_FLAGS contains DebugMode, added only under cfg!(debug_assertions): crates/warp_features/src/lib.rs:803, app/src/bin/phosphor_oss.rs:42-44
- WARP_SHELL_DEBUG_MODE exported to spawned sessions: app/src/settings/debug.rs:3-5
- PTY recording path `<state_dir>/pty_recordings/<timestamp>-<id>.pty.recording`: app/src/terminal/recorder.rs:15-17, :74-79
- WARP_DATA_PROFILE debug-only data isolation: crates/warp_core/src/channel/state.rs:110-118

## Shell integration
- BootstrapStage state machine and `is_bootstrapped()`: app/src/terminal/model/bootstrap.rs:3-15, :36-41; the hidden WarpInput stage :47-50
- DCS hook enum (InitShell / Bootstrapped / Precmd / Preexec / InitSubshell / InitSsh): app/src/terminal/model/ansi/dcs_hooks.rs:37-102
- `is_login_shell_bootstrapped` set on the Bootstrapped hook: app/src/terminal/view.rs:12803-12804
- BOOTSTRAP_FAILED_DURATION 7s / ENV_VAR_BOOTSTRAP_FAILED_DURATION 60s / SLOW_BOOTSTRAP_BANNER_AUTO_DISMISS_DURATION 30s: app/src/terminal/view.rs:651, :656, :664; selection at :8883-8888; timer fire at :14947-15037
- banner text and "Show initialization block" button: app/i18n/en/warp.ftl:3686-3687; construction app/src/terminal/view.rs:3680-3701
- shell exited prematurely keeps the pane open and force-shows the init block: app/src/pane_group/pane/terminal_pane.rs:585-596, app/src/terminal/view.rs:10817-10820, :25012-25017; strings app/i18n/en/warp.ftl:3690-3691
- incompatible-configuration banner, p10k and pure tags: app/src/terminal/view.rs:21355-21408; string app/i18n/en/warp.ftl:247; working p10k link app/src/terminal/view.rs:680-681
- completions-not-working banner suggesting the tmux wrapper: app/i18n/en/warp.ftl:241-246; app/src/terminal/view.rs:3717-3741
- empty help URLs KNOWN_ISSUES_URL / PROMPT_COMPATIBILITY_URL / CONTROLMASTER_ISSUES_URL: app/src/terminal/view.rs:668-677; SSH_DOCS_URL / SUBSHELL_DOCS_URL: app/src/terminal/warpify/render.rs:35-36
- App menu "Show initialization block": app/src/app_menus.rs:643-675; strings app/i18n/en/warp.ftl:213-214, :630-631
- `appearance.blocks.should_show_bootstrap_block` default false: app/src/settings/block_visibility.rs:8-16
- environment exported into a local PTY (TERM_PROGRAM=WarpTerminal, WARP_SESSION_ID, WARP_IS_LOCAL_SHELL_SESSION, WARP_HONOR_PS1, WARP_SHELL_DEBUG_MODE, WARP_INITIAL_WORKING_DIR, WARP_PATH_APPEND, HISTSIZE sentinels, DESKTOP_STARTUP_ID removal): app/src/terminal/local_tty/unix.rs:337-449; Windows names app/src/terminal/local_tty/windows/environment.rs:22-29
- WARP_BOOTSTRAPPED idempotence guard: app/assets/bundled/bootstrap/zsh_body.sh:7, :1296; bash_body.sh:7, :1409
- WARP_SESSION_ID placeholder substitution and hook validation: app/src/terminal/bootstrap.rs:248, :266, :329; app/src/terminal/model/terminal_model.rs:2689-2699
- WARP_IS_SUBSHELL skips login rc files: app/assets/bundled/bootstrap/bash_init_subshell.sh:7; zsh_body.sh:1154+, bash_body.sh:1214
- shell debug mode adds -x on re-exec: app/assets/bundled/bootstrap/zsh_body.sh:1074-1076, bash_body.sh:1176-1178
- `terminal.input.honor_ps1` default false: app/src/terminal/session_settings.rs:299-307; coupling to input box type app/src/settings/init.rs:236-254 and app/src/settings/initializer.rs:121-129; runtime fallback app/src/settings/input.rs:236-244
- subshell detection regexes: app/src/terminal/warpify/settings.rs:219-243 (+ wsl at :206-212), matcher :459-488; footer app/src/terminal/view.rs:11036-11052; keybinding app/src/terminal/view/init.rs:345; strings app/i18n/en/warp.ftl:394-395
- Warpify settings group and defaults (enable_ssh_warpification true, use_ssh_tmux_wrapper false, denylists empty): app/src/terminal/warpify/settings.rs:18-56, :85-93
- three-condition SSH gate: app/src/terminal/ssh/ssh_detection.rs:63-91
- tmux wrapper supersedes the ControlMaster wrapper: app/src/terminal/local_tty/terminal_manager.rs:688-696
- Phosphorize SSH button and keybinding: app/src/terminal/view.rs:24922-24951; app/src/terminal/view/use_agent_footer/warpify_footer.rs:87-89; app/src/terminal/view/init.rs:354
- 8s timeout: app/src/terminal/ssh/mod.rs:10; script selection and PowerShell unsupported: app/src/terminal/ssh/warpify.rs:184-196
- remote failure messages verbatim: app/src/terminal/ssh/error.rs:32-42; escape hatches app/i18n/en/warp.ftl:3665-3666

## Fonts
- FontSettings group, every toml_path and default: app/src/settings/font.rs:28-227; constants :15-26
- clamps (heading scales 0.1-5.0, notebook 5-25, UI font 8-20): app/src/settings/font.rs:21-22, :203-204, :262-275; crates/warp_core/src/ui/appearance.rs:21-23
- unresolvable font is a log warning and nothing else: app/src/appearance.rs:431-457; the live-change subscriber only acts inside `if let Some(...)`: app/src/appearance.rs:76-83
- `m`-glyph requirement: app/src/appearance.rs:441-449
- bundled Hack / Roboto / Segoe UI fallbacks: app/src/appearance.rs:347-371, :374-408, :489-491, :514
- font picker lists enumerated fonts plus Hack; no per-item preview on Linux/FreeBSD: app/src/settings_view/appearance_page.rs:2537-2545, :1740-1746
- bundled glyph fallback set: app/src/font_fallback.rs:17-80

## Rendering failure behaviour
- no software rasterizer; adapters deprioritised and "No usable wgpu adapter was found": crates/warpui/src/rendering/wgpu/resources.rs:104, :589-597, :765-804
- lost surface/device is logged and the renderer recreated silently: crates/warpui/src/windowing/winit/event_loop/mod.rs:1055-1074; crates/warpui/src/windowing/winit/window.rs:998-1012
- crash-recovery mechanism order and the WGPU_BACKEND child env: app/src/crash_recovery.rs:290-334, :362-370
- only the X11 recovery shows a banner; GPU recoveries deliberately show nothing: app/src/workspace/view/crash_recovery.rs:15-41
- "changes apply to new windows" on the GPU settings: app/src/settings_view/features_page.rs:7259-7275, :7418-7434
- transparency warning: app/i18n/en/warp.ftl:1479-1481; app/src/settings_view/appearance_page.rs:3714-3749
- WGPU_BACKEND read via wgpu::Backends::from_env(): crates/warpui/src/rendering/wgpu/mod.rs:149

## MCP
- log path resolution and the Windows-only `logs/` segment: crates/simple_logger/src/manager.rs:33-46; WARP_LOGS_DIR crates/warp_core/src/paths.rs:35
- secure_state_dir() is None on the Oss channel, so state_dir() is always used: crates/warp_core/src/paths.rs:224-239, crates/warp_core/src/paths_tests.rs:139
- pinned state_dir values per platform: crates/warp_core/src/paths_tests.rs:106-116
- log file named by template UUID: app/src/ai/mcp/logs.rs:22-29; call site app/src/ai/mcp/templatable_manager/native.rs:800-804
- 10 MiB x 5 rotation: app/src/ai/mcp/logs.rs:18-20; suffix scheme crates/simple_logger/src/lib.rs:269-309
- namespace purged on first registration (i.e. first MCP spawn in the process): app/src/ai/mcp/templatable_manager/native.rs:799; crates/simple_logger/src/manager.rs:88-101
- log contents are server stderr plus transport events: app/src/ai/mcp/templatable_manager/native.rs:1846, :1755, :896-899
- GUI "View logs" splits a pane and pre-fills tail: app/src/settings_view/mcp_servers/list_page.rs:562-591; command text app/src/workflows/local_workflows.rs:196-209; buttons app/src/settings_view/mcp_servers/server_card.rs:769-782, :834-848; strings app/i18n/en/warp.ftl:735, :740
- TUI /mcp actions contain no log viewer: app/src/tui/mcp.rs:208-215; crates/warp_tui/src/mcp_menu.rs:295-345
- TUI /view-logs passes LogBundleExtras::default(), so no mcp/*.log: crates/warp_tui/src/terminal_session_view.rs:4514-4521; the GUI bundle does include them: app/src/workspace/view.rs:6155-6179
- MCPServerState variants: app/src/ai/mcp/mod.rs:220-228; GUI card mapping app/src/settings_view/mcp_servers/server_card.rs:158-183, :222-343; TUI mapping app/src/tui/mcp.rs:131-142, :675-689
- error message wording: app/src/ai/mcp/templatable_manager/native.rs:121-165; stored per server app/src/ai/mcp/templatable_manager.rs:64, :225-229, native.rs:883, :903-907
- config diagnostics (Read / Parse / MissingEnvironmentVariable) rendered as their own row: app/src/ai/mcp/file_mcp_watcher.rs:646-658; app/src/ai/mcp/file_based_manager.rs:87-95; crates/warp_tui/src/mcp_menu.rs:265-278
- config paths per provider: app/src/ai/mcp/mod.rs:95-115; the Phosphor global path resolves through warp_home_mcp_config_file_path() to `~/.phosphor/.mcp.json`: app/src/ai/mcp/mod.rs:53-58, crates/warp_core/src/paths.rs:88-90, :42, :67-69
- auto-spawn policy (project-scoped never auto-spawns): app/src/ai/mcp/file_based_manager.rs:433-460
- spawn cwd from the config's directory: app/src/ai/mcp/file_based_manager.rs:584-613
- `mcp_execution_path` is the PATH prepended to stdio servers, private (no toml_path): app/src/settings/ai.rs:2279-2285; used at app/src/ai/mcp/templatable_manager/native.rs:741, :775-781; captured at app/src/terminal/view.rs:12840
- `agents.mcp_servers.file_based_mcp_enabled` default false: app/src/settings/ai.rs:2468-2476; gate app/src/settings/ai.rs:2954-2966; label app/i18n/en/warp.ftl:718-719
- `oz mcp list` is the whole CLI surface: crates/warp_cli/src/mcp.rs:8-13; registration crates/warp_cli/src/lib.rs:358-361; implementation app/src/ai/agent_sdk/mcp.rs:11-46
- `oz agent run --mcp <spec>`: crates/warp_cli/src/agent.rs:336-347; spec parser crates/warp_cli/src/mcp.rs:37-79
- a bare UUID spec becomes an unresolvable `warp_id` entry: app/src/ai/agent_sdk/mcp_config.rs:25-48 (issue #279)
- MCPGalleryManager is a permanently empty stub: app/src/ai/mcp/gallery.rs:97-123

## /usage and /cost
- rationale and BYOP semantics: app/src/ai/usage_cost.rs:1-31
- context-window report text: app/src/ai/usage_cost.rs:131-190
- registered on both surfaces: app/src/search/slash_command_menu/static_commands/mod.rs:395-401

## Declined features (Part 2)
All rows are drawn from DECLINED.md; the specific rows and their supporting code:
- accounts/onboarding/billing: DECLINED.md "Account-first onboarding, billing, paid tiers" (#11)
- /logout not registered: DECLINED.md "`/logout` slash command" (#338)
- teams permanently stubbed: DECLINED.md "Teams stay stubbed" (#445); app/src/workspaces/user_workspaces.rs:533
- workspace/team AI-autonomy and sandboxed-agent policy: DECLINED.md "Workspace / team AI-autonomy and sandboxed-agent overrides"
- agent attribution: DECLINED.md "Agent commit/PR attribution" (#445)
- session sharing / --share hidden but parseable: DECLINED.md "Agent session sharing"; crates/warp_cli/src/share.rs:9-27
- shared-session heartbeat: DECLINED.md "Shared-session heartbeat"
- Warp Drive link resolution kept as dead code: DECLINED.md "warp.dev Drive link resolution" (#267)
- cloud agent runners / RunAgents: DECLINED.md "RunAgents / cloud-runner orchestration" (#290)
- agent-invoked agent spawning declined in favour of user-invoked /orchestrate: DECLINED.md "Agent-invoked agent spawning" (#325)
- agent mailbox replacement: crates/warp_cli/src/agent_mailbox.rs:42, :63-72; app/src/ai/agent_sdk/agent_message.rs:1-14
- Warp Environments: DECLINED.md "Warp Environments" (#211)
- global skills policy filtering: DECLINED.md "AI skills - global-spec filtering" (#487)
- InitProject wizard: DECLINED.md "InitProject wizard"; "`InitProject` wizard, and `lsp_server_selector.rs` with it" (#11)
- Oz updates zero-state / FEATURE_INTROS / Warp Agent CLI promo modal: DECLINED.md "Oz updates zero-state section" (#321), "FEATURE_INTROS content" (#404), "Warp Agent CLI promotional launch modal"
- MCP gallery, well-known ids, warp_id, shared templates: DECLINED.md "MCP gallery in the TUI /mcp catalog"; app/src/ai/agent_sdk/mcp_config.rs:26-48
- Oz platform plugins removed: DECLINED.md "Oz platform plugins" (#595)
- CLI agent notification plugins kept and working: DECLINED.md "CLI agent notification plugins - Warp's packages, Phosphor's prose"
- SSH manager declined: DECLINED.md "SSH connection management - the system owns it, not the app"; tmux wrapper kept: DECLINED.md "SSH tmux wrapper - kept, deprecation not ported" (#322)
- voice: DECLINED.md "Voice input - recording exists, transcription is cloud and disabled" (#389, #352)
- screen/session recording: DECLINED.md "Screen recording" (#367), "computer_use session recording" (#350)
- Grok subscription OAuth: DECLINED.md "xAI / Grok subscription OAuth" (#319); rejection message crates/warp_tui/src/session.rs:167-176
- Bedrock OIDC role assumption: DECLINED.md "AWS Bedrock OIDC role assumption"
- network log console not ported: app/src/settings_view/privacy_page.rs:12-15
- empty docs/Slack/privacy-policy URLs: app/src/util/links.rs:5-11
- README's own user-facing "Not included, on purpose" table: README.md, "Not included, on purpose" section

-->
