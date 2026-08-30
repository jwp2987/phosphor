# Shell integration and appearance

Two things most people configure in their first hour with Phosphor. **Shell
integration** — which the UI calls **Phosphorize** — is what turns a stream of
bytes from your shell into Phosphor's blocks: it is how the app knows where one
command ends and the next begins, what the exit status was, what directory you
are in, and what to feed the completion menu. **Appearance** is the theme, font,
padding and window chrome. They are grouped together here because they share a
trap: both have settings that only take effect in a *new* session, and both have
options whose defaults differ from what a Warp user expects.

Everything below is traced to code in this tree. Where Phosphor differs from
Warp — or where something is present but does nothing on Linux — it says so.

---

## Part 1 — Shell integration ("Phosphorize")

### What it actually is

Phosphor does not read your terminal output and guess. It injects a bootstrap
script into the shell it spawns, and that script reports structured events back
up the PTY as DCS/OSC escape sequences:

- `InitShell` — a session started; carries shell name, user, hostname, and a
  session id the app minted *before* the shell ran.
- prompt / precmd / preexec hooks — the block boundaries.
- `SourcedRcFileForWarp` — "an rc file I control just got sourced, adopt me".

The scripts live in `app/assets/bundled/bootstrap/`: `bash.sh` + `bash_body.sh`,
`zsh.sh` + `zsh_body.sh`, `fish.sh`, `pwsh.ps1`, plus the small
`*_init_shell` / `*_init_subshell` stubs that emit the first hook.

Once it is on you get: blocks, per-block exit status, the Phosphor input editor
(instead of your PS1), autosuggestions from history, the completion menu, syntax
highlighting, and correct working-directory tracking for new tabs and panes.

### How do I turn it on for a local shell?

You do not. Phosphor launches your login shell itself with the bootstrap
attached, so a local `bash`, `zsh`, `fish` or `pwsh` tab is integrated from the
first prompt. Concretely:

| shell | how Phosphor starts it |
|---|---|
| bash | `bash -c "exec -a bash <shell> --rcfile <(echo <init script>)"` — your `~/.bashrc` is then sourced by the bootstrap itself |
| zsh | `zsh -c "exec -a -zsh <shell> -g --no-rcs"`, then the bootstrap replays your startup files |
| fish | `fish --no-config -c "exec <shell> -f no-mark-prompt --login --init-command <init script>"` |
| pwsh | `-NoLogo -NoProfile -NoExit -EncodedCommand <init script>` |

Two consequences worth knowing. Bash sessions are **not login shells** (bash
refuses a custom rcfile in a login shell), so the bootstrap prints the MoTD
itself unless `~/.hushlogin` exists. And zsh's `/etc/zshenv` still runs — it is
the one startup file `--no-rcs` cannot suppress.

### How do I tell whether integration is active?

In order of effort:

1. The prompt is Phosphor's input box, not your PS1, and each command sits in
   its own block with a status chip.
2. `echo $TERM_PROGRAM` prints `WarpTerminal`. (Yes — the value is deliberately
   unchanged from upstream so shell plugins that check for it keep working.
   `TERM_PROGRAM_VERSION` and `WARP_CLIENT_VERSION` are also set.)
3. `echo $WARP_BOOTSTRAPPED` prints `1` in bash/zsh/fish once the body script has
   finished. In a subshell, `$WARP_IS_SUBSHELL` is `1`.
4. `echo $WARP_SESSION_ID` is non-empty.

If you are on a remote host over the SSH wrapper, `$WARP_IS_SSH` is `1`.

### Subshells: bash, zsh, fish, docker, and friends

When you run a command that Phosphor recognises as spawning an interactive
subshell, a banner appears on the running block offering to Phosphorize it.
Press **`ctrl-i`** (binding `terminal:warpify_subshell`) or click the banner.
Phosphor then pastes the subshell bootstrap into the running program's stdin.

The built-in patterns (`app/src/terminal/warpify/settings.rs`) are:

- a bare shell path: `bash`, `zsh`, `fish`, `/bin/zsh`, `./some/path/fish`
- `docker run … bash` and `docker exec … bash` (also `zsh`/`fish`, optionally
  quoted)
- `poetry shell`, `pipenv shell`
- `aws-vault exec …`
- `flox activate`
- on Windows only, `wsl` / `wsl.exe` when it is opening an interactive shell

`ssh`-like commands are also treated as subshell-compatible **when the tmux
wrapper is off** — see the SSH section.

**Adding your own.** Settings → Phosphorize → Subshells → *Added commands*
accepts a regex per line, stored at
`warpify.subshells.added_subshell_commands`. **Denylisting** (the "don't ask
again" button, or *Denylisted commands* in the same page) writes to
`warpify.subshells.subshell_commands_denylist`. Both are regexes, both are
matched against the trimmed command line, and an invalid regex is silently
skipped rather than failing the session.

**Making it stick.** Phosphorizing a subshell by banner lasts only for that
subshell. To make a container or remote host adopt Phosphor automatically,
append the "Auto-Warpify" snippet to the rc file *inside* it. Phosphor prints
the exact command in the success block after a manual Phosphorize; it looks like
this for bash/zsh:

```sh
echo -e '\n# Auto-Warpify\n[[ "$-" == *i* ]] && printf '\''\eP$f{"hook": "SourcedRcFileForWarp", "value": { "shell": "bash", "uname": "'$(uname)'" }}\x9c'\'' ' >> ~/.bashrc
```

and this for fish:

```fish
echo -e '\n# Auto-Warpify\nstatus --is-interactive; and printf '\''\eP$f{"hook": "SourcedRcFileForWarp", "value": { "shell": "fish", "uname": "'$(uname)'" }}\x9c'\'' ' >> ~/.config/fish/config.fish
```

The hook payload accepts an optional `"tmux": false` field, which tells Phosphor
not to try the tmux route for that session. Phosphor emits that variant for you
when the session it is describing had tmux disabled.

There is no Auto-Warpify snippet for PowerShell — `get_subshell_bootstrap_success_block_path`
returns nothing for `ShellType::PowerShell`, so the success block is empty there.

### SSH

There are three different mechanisms, and which one you get depends on settings
that are read **when the session's PTY is created**. This is the single most
common source of confusion.

#### 1. The SSH extension (remote server)

Phosphor's own small daemon, installed on the remote host, giving the fullest
experience (including agent file tools on the remote). Behaviour when the host
does not have it is controlled by
`warpify.ssh.ssh_extension_install_mode`, one of `always_ask` (default),
`always_install`, `never_install`. Set it in Settings → Phosphorize → SSH →
*Install SSH extension*.

This is entirely local machinery — `app/src/remote_server` is Phosphor's own SSH
remote-host daemon, not a Warp cloud service, despite the name.

#### 2. The legacy ControlMaster wrapper (default)

With `warpify.ssh.enable_ssh_warpification = true` (default) and
`warpify.ssh.use_ssh_tmux_wrapper = false` (default), the bootstrap installs a
shell function named `ssh` that intercepts interactive invocations, opens an SSH
`ControlMaster` socket under `~/.ssh/`, and passes a remote command that
re-bootstraps the far-side shell.

Its limits are structural, and it falls back to plain `ssh` (no integration, no
error) in each of these cases:

- the remote login shell is **not** bash or zsh — anything else gets MoTD +
  `/etc/profile` + `exec $SHELL` and no Phosphor hooks;
- your `~/.ssh/config` sets a `RemoteCommand` for that destination — OpenSSH
  refuses to run both a config `RemoteCommand` and a command-line one;
- `/dev/urandom` could not produce a session token.

`warpify.ssh.reuse_existing_control_master` (default `false`) makes the wrapper
attach to a live ControlMaster you already run for that host — resolved with
`ssh -G` and probed with `ssh -O check` — instead of creating a Phosphor-owned
one. **Takes effect in new tabs only.**

#### 3. The tmux wrapper (`ssh_tmux_wrapper`)

`ssh_tmux_wrapper` is in this fork's `default` Cargo feature list, so
`FeatureFlag::SSHTmuxWrapper` is on. It is still **opt-in per user**:
`warpify.ssh.use_ssh_tmux_wrapper` defaults to `false`. Turn it on at
Settings → Phosphorize → SSH → *Use Tmux Phosphorization*.

What it does: rather than wrapping `ssh` locally, Phosphor detects that you have
run an interactive SSH command, shows a "Phosphorizing SSH Session…" block, and
drives a tmux **control-mode** session on the remote host as the transport for
block boundaries. Because it does not depend on rewriting the remote shell's
startup, it works in many situations the ControlMaster wrapper does not.

Phosphor keeps this wrapper deliberately — upstream Warp deprecated it in favour
of the extension; this fork did not, because it should Phosphorize whatever host
you SSH into (`DECLINED.md`, "SSH tmux wrapper — kept, deprecation not ported").

**Its limits, honestly:**

- **It needs tmux ≥ 3.0 on the remote.** If tmux is missing or too old you get
  an install block offering a bundled script (Homebrew, or apt/dnf/pacman/yum/zypper
  with sudo, or a rootless install into `~/.warp/tmux/`). It trades installing
  Phosphor's binary for installing tmux — it is not a "nothing to install"
  option.
- **It is mutually exclusive with the legacy wrapper.** Turning it on disables
  the ControlMaster wrapper for the session, and vice versa; the choice is
  computed once, in `TerminalManager`, at PTY creation.
- **It only takes effect in new tabs.** The setting's own description says so.
  If a session is already running — including one in which you have started a
  CLI agent — toggling the setting will not Phosphorize it. You must exit that
  session and open a new tab. There is no way to retrofit the wrapper onto a
  live PTY.
- **It never engages on Windows.** tmux control mode needs DCS, and ConPTY does
  not support DCS. On Windows the SSH extension is the only route to an
  integrated SSH session. This is a documented, accepted platform asymmetry
  (`DECLINED.md`), not a bug to file.
- **The banner is suppressed** if the command also matches a subshell pattern,
  if the host is denylisted, or if SSH Phosphorization is off entirely.

Beyond literal `ssh`, the tmux route also recognises `gcloud compute ssh`,
`eb ssh` and `doctl compute ssh`. `ssh -T` and `ssh -W` are correctly treated as
non-interactive and ignored.

**Denylisting a host** (the banner's "don't ask again", or Settings →
Phosphorize → SSH → *Denylisted hosts*) writes a regex to
`warpify.ssh.ssh_hosts_denylist`.

#### The `enable_legacy_ssh_wrapper` trap

`warpify.ssh.enable_legacy_ssh_wrapper` is **deprecated and overloaded**. Until
#635 it was declared twice in the tree against the same TOML path and the same
storage key `EnableSSHWrapper`; it is now declared once, in `WarpifySettings`,
but the trapdoor below is unchanged. On startup, if you have explicitly set it to
`false`,
Phosphor runs a one-time migration that sets `warpify.ssh.enable_ssh_warpification = false`
— i.e. it turns off SSH integration entirely, not just the legacy wrapper — and
then resets itself to `true` so the migration cannot re-fire.

Do not use it. Use `warpify.ssh.enable_ssh_warpification` to turn SSH
integration off and `warpify.ssh.use_ssh_tmux_wrapper` to pick the mechanism.

### Per-shell notes

| shell | local | subshell | SSH (legacy wrapper) | notes |
|---|---|---|---|---|
| **bash** | yes | yes | yes | Uses a vendored copy of `bash-preexec`. Not a login shell; MoTD printed by the bootstrap. Bootstrap commands are hidden from history via `HISTCONTROL=ignorespace`. |
| **zsh** | yes | yes | yes | Started `--no-rcs -g`; `/etc/zshenv` still runs. `PS2` is blanked during bootstrap and restored as `ORIGINAL_PS2`. On the remote side the wrapper writes a temporary `ZDOTDIR`. |
| **fish** | yes | yes | no | Started with `-f no-mark-prompt` to suppress fish's OSC 133, whose partial implementation breaks block detection. The SSH helper's remote branch handles only bash and zsh, so an SSH into a fish login shell falls through to plain `ssh`. |
| **PowerShell** | yes | banner only | in-band only | `-NoProfile` plus an `-EncodedCommand` init script (UTF-16LE base64 — `-Command` quoting is broken on PS 7.6). If `ExecutionPolicy` is `Restricted` at machine or user scope, integration cannot start and PowerShell says so. No subshell success block, no Auto-Warpify snippet. The subshell banner is hard-coded to appear for interactive `ssh` commands from pwsh because in-band generators are the only workable route there. |

WSL is supported for bash/zsh/fish only; PowerShell under WSL is not
bootstrapped.

### Troubleshooting

**The prompt is not detected / everything is one giant block.**
Check `$WARP_BOOTSTRAPPED`. If it is empty, the bootstrap never finished —
usually because a startup file is interactive (asks a question, runs `exec`, or
re-sets `PROMPT_COMMAND`/`precmd` in a way that clobbers the hooks). Turn on
**Enable Shell Debug Mode (-x) for New Sessions** (App → Debug menu) to run the
bootstrap under `set -x`; it sets `WARP_SHELL_DEBUG_MODE=1` in new sessions.
Note this menu only exists in debug builds — `FeatureFlag::DebugMode` is added
by the `phosphor-oss` binary under `debug_assertions` only.

**I want to see what Phosphor is actually running.** Three settings unhide the
machinery, all default `false`:

```toml
[appearance.blocks]
should_show_bootstrap_block        = true   # the startup script's own block
should_show_in_band_command_blocks = true   # background completion/highlighting probes
should_show_ssh_block              = true   # the SSH connection block
```

**I use my own prompt and want it back.** `terminal.input.honor_ps1 = true`
(default `false`) renders your shell's PS1 instead of Phosphor's input box. It
also flips the input box to Classic mode automatically.

**The subshell banner never appears.** It requires the command to be the active
long-running command in the block, and it must match a built-in or user regex
and not be denylisted. A command started from inside another program (an agent,
a pager, a nested REPL) will not raise it.

**Completions and highlighting are dead over SSH.** That is everything
downstream of `is_bootstrapped`. Either the far side is not integrated (see the
fallback list above) or you are on a host where only the extension can work.

**"Learn more" links do nothing.** Known: `SSH_DOCS_URL` and
`SUBSHELL_DOCS_URL` are empty strings in this fork, as is the Settings →
Phosphorize page's link. There is no hosted documentation to point at.

### Shell-integration reference

| setting | what it does | default | where |
|---|---|---|---|
| `warpify.subshells.added_subshell_commands` | Extra regexes treated as subshell-spawning commands | `[]` | Settings → Phosphorize → Subshells → Added commands |
| `warpify.subshells.subshell_commands_denylist` | Regexes that never raise the subshell banner | `[]` | same page → Denylisted commands |
| `warpify.ssh.ssh_hosts_denylist` | Host regexes that never raise the SSH banner | `[]` | Settings → Phosphorize → SSH → Denylisted hosts |
| `warpify.ssh.enable_ssh_warpification` | Master switch for Phosphorizing SSH sessions | `true` | Settings → Phosphorize → SSH → *Phosphorize SSH Sessions* |
| `warpify.ssh.use_ssh_tmux_wrapper` | Use the tmux control-mode wrapper instead of the ControlMaster wrapper. macOS/Linux only. New tabs only | `false` | same page → *Use Tmux Phosphorization* |
| `warpify.ssh.reuse_existing_control_master` | Attach to your own live ControlMaster instead of creating one. Legacy wrapper only; new tabs only | `false` | same page → *Reuse existing SSH ControlMaster* |
| `warpify.ssh.ssh_extension_install_mode` | `always_ask` / `always_install` / `never_install` | `always_ask` | same page → *Install SSH extension* |
| `warpify.ssh.enable_legacy_ssh_wrapper` | **Deprecated.** Setting it `false` runs a one-time migration that disables `enable_ssh_warpification` | `true` | do not use |
| `terminal.input.honor_ps1` | Use your shell's PS1 instead of the Phosphor input box | `false` | Settings → Appearance → Input → *Input type* |
| `appearance.blocks.should_show_bootstrap_block` | Show the bootstrap script's block | `false` | TOML; App menu when debug features are on |
| `appearance.blocks.should_show_in_band_command_blocks` | Show background probe blocks | `false` | TOML; App menu |
| `appearance.blocks.should_show_ssh_block` | Show the SSH connection block | `false` | TOML; App menu → *Show Phosphorized SSH Blocks* |

Key bindings: `ctrl-i` = `terminal:warpify_subshell` (when the subshell banner is
up) and `terminal:warpify_ssh_session` (when the SSH banner is up). `ctrl-c`
interrupts an in-progress SSH Phosphorization.

---

## Part 2 — Appearance

Settings live at **`cmdorctrl-,`** → **Appearance**, with categories Themes,
Language, Icon (macOS only), Window, Input, Panes, Blocks, Text, Cursor, Tabs,
Full-screen Apps and Zoom. Everything is also editable in
`~/.config/phosphor/settings.toml` (Linux; `~/.phosphor/settings.toml` on macOS,
`%LOCALAPPDATA%\phosphor\Phosphor\config\settings.toml` on Windows).

### Themes

**Where they live.** Custom themes are `.yaml` or `.yml` files in the *data*
directory's `themes/` folder:

| platform | themes directory |
|---|---|
| Linux | `~/.local/share/phosphor/themes/` |
| macOS | `~/.phosphor/themes/` |
| Windows | `%APPDATA%\phosphor\Phosphor\data\themes\` |

The directory is walked recursively and symlinks are followed, so
`themes/catppuccin/mocha.yaml` works. Phosphor watches the directory and
reloads on change — no restart needed for edits to an existing theme file.

**Bundled themes.** 27 are compiled in: Dark, Light, Solarized Dark, Solarized
Light, Dracula, Fancy Dracula, Tokyo Night, One Dark, Gruvbox Dark, Gruvbox
Light, Jellyfish, Koi, Leafy, Marble, Pink City, Snowy, Dark City, Red Rock,
Cyber Wave, Willow Dream, Phenomenon, Solar Flare, Adeberry, WezTerm Classic,
VS Code 2026 Dark, **Phosphor Amber** and **Phosphor Green**.

**Phosphor Amber is the default** (`ThemeKind::default()`), which is a fork
change — upstream defaults to Dark, and the "new user gets Phenomenon → Adeberry"
override inherited from upstream never fires here because a fresh install never
sits on Phenomenon.

The repo also ships five of these as standalone YAML under `themes/` at the
project root (`one_dark.yaml`, `phosphor_amber.yaml`, `phosphor_green.yaml`,
`tokyo_night.yaml`, `vscode_2026_dark.yaml`) — useful as starting points to copy
and edit.

**Picking one.** Settings → Appearance → Themes, or the theme chooser
(`workspace:show_theme_chooser`, no default key binding — reach it from the
command palette, `cmdorctrl-shift-P`). *Sync with OS* follows the system
light/dark preference and lets you pick a theme for each.

**The theme creator.** From the theme chooser's "Create your own custom theme"
button. It is image-driven: you pick an image, Phosphor extracts a palette from
it, and on save it writes `<name>.yaml` plus a copy of the image into the themes
directory. If you want precise control, write the YAML by hand.

**Base16 themes** are a recognised special case: a custom theme whose `name`
starts with `Base16` is loaded as `CustomBase16`, which changes how ANSI bright
colours are derived.

#### Writing a custom theme

Save this as `~/.local/share/phosphor/themes/midnight_slate.yaml` — it will
appear as "Midnight Slate" without a restart.

```yaml
---
# `name` is optional; without it the file name is used, humanised.
name: Midnight Slate

# Terminal background. Either a hex string, or a gradient:
#   background: { top: '#11131a', bottom: '#0a0b10' }     # vertical
#   background: { left: '#11131a', right: '#0a0b10' }     # horizontal
background: '#11131a'

# Default text colour. Phosphor infers "is this a light or dark theme"
# from this value, so get it right before worrying about anything else.
foreground: '#c8d0e0'

# Selection highlight, focus rings, links. Also accepts a gradient.
accent: '#6ea8fe'

# Optional: cursor colour. Defaults to the accent if omitted.
cursor: '#ffd75f'

# How Phosphor derives its chrome (panels, borders, hover states) from the
# background: `darker`, `lighter`, or an explicit custom block.
details: darker

# The 16-colour ANSI palette. All 16 keys are required.
terminal_colors:
  normal:
    black:   '#11131a'
    red:     '#e06c75'
    green:   '#8fbf7f'
    yellow:  '#e5c07b'
    blue:    '#6ea8fe'
    magenta: '#c678dd'
    cyan:    '#56b6c2'
    white:   '#c8d0e0'
  bright:
    black:   '#4b5263'
    red:     '#ff8087'
    green:   '#a9d59a'
    yellow:  '#ffd894'
    blue:    '#8fc1ff'
    magenta: '#d8a0ea'
    cyan:    '#77d5df'
    white:   '#f2f5fa'

# Optional: override individual UI colours instead of letting `details`
# derive them. Every key is optional; hex may carry an alpha byte.
ui_colors:
  surface_1: '#171a23'
  surface_2: '#1d212c'
  border:    '#2a2f3d'
  main_text: '#c8d0e0'
  sub_text:  '#8b93a7'
  hint_text: '#5d6577'
  selection: '#6ea8fe40'
  hover:     '#ffffff10'
  error:     '#e06c75'
  warning:   '#e5c07b'
  success:   '#8fbf7f'
  link:      '#6ea8fe'

# Optional: a background image, relative to the themes directory.
# background_image:
#   path: midnight_slate.png
#   opacity: 30
```

The full `ui_colors` key list is `surface_1`, `surface_2`, `surface_3`,
`border`, `focus_border`, `split_pane_border`, `main_text`, `sub_text`,
`hint_text`, `disabled_text`, `selection`, `text_selection`, `hover`, `active`,
`warning`, `error`, `success`, `link`.

In `settings.toml`, a selected custom theme is stored as an inline table with a
path relative to the themes directory, so it survives moving between machines:

```toml
[appearance.themes]
theme = { custom = { name = "Midnight Slate", path = "midnight_slate.yaml" } }
```

A custom theme whose file sits *outside* the themes directory is stored with its
absolute path and is flagged as non-portable.

### Fonts

The default terminal font is **Hack** at **13 px**, weight **normal**, line
height **1.2**. Hack ships bundled (`app/assets/bundled/fonts/hack/`), so the
default works on a bare system. The UI font defaults to empty, meaning
"whatever the platform's UI font is", at 12 px (clamped 8–20).

Settings → Appearance → Text covers all of it, including a *View all available
system fonts* button.

**Ligatures.** Off by default. `appearance.text.ligature_rendering_enabled = true`
enables them; the UI warns that they may cost performance. The `ligatures` Cargo
feature is in this fork's default list, so the setting is live.

**Fallback fonts.** `appearance.text.fallback_font_name` (default empty =
"system fallback") names a second installed font to consult for glyphs the
terminal font cannot render. This is a *locally installed* font by name and it
works.

What does **not** work is the other fallback mechanism. `app/src/font_fallback.rs`
maps thousands of Unicode code points to a set of Noto / Hack Nerd Font families
that upstream Warp downloaded over HTTPS from its own static-asset server. The
de-Warping change rewrote those URLs to the app's private URL scheme
(`phosphor://assets/fallback-fonts/...`), and the asset loader fetches with
`reqwest::get`, which cannot resolve a non-HTTP scheme — and no such assets are
bundled. **Treat downloadable fallback fonts as non-functional:** install the
coverage you need (a Nerd Font, Noto Color Emoji, a CJK font) on the system and
name it in `fallback_font_name`.

`script/font_fallback/` (`generate-families.py`, `generate-mappings.py`) is a
maintainer tool that regenerates that table from Warp's Google Cloud Storage
bucket. It is not usable outside Warp and is not something a user runs.

**Glyph patching** — `script/patch_font_with_warp_glyph` — is a FontForge script
that injects an SVG (by default `script/warp.svg`) into a TTF at a Private Use
Area code point (`U+E500`, glyph name `warpLogo`). It is how the shipped Roboto
was patched. It is available if you want the logo glyph in your own font, but it
is not part of normal configuration.

**Other text options:** `appearance.text.enforce_minimum_contrast`
(`only_named_colors` by default — rewrites foreground colours only when the
program asked for a named ANSI colour; the alternatives are `always` and
`never`), the six markdown heading scales (H1 `2.0` down to H6 `0.75`, clamped
to 0.1–5.0), a separate AI/agent font that can be pinned to the terminal font,
and `use_thin_strokes` (**macOS only**).

Font size shortcuts: `ctrl-shift->` / `ctrl-shift-<` change the terminal font
size; `cmdorctrl-=` / `cmdorctrl--` / `cmdorctrl-0` change the app **zoom**
(because `ui_zoom` is a default feature, these two sets are distinct).

### App icon

`appearance.icon.app_icon` selects a dock icon from 17 variants (Default,
Aurora, Classic 1–3, Comets, Cow, Glass Sky, Glitch, Glow, Holographic, Mono,
Neon, Original, Starburst, Sticker, Phosphor 1).

**It is macOS-only.** The setting is declared `SupportedPlatforms::MAC`, and the
Appearance page only renders the Icon category when the setting is supported on
the running platform — so on Linux, this fork's primary platform, the category
does not appear and the setting does nothing. On macOS it also requires a
bundled app, and may need a restart to take effect.

### Layout, padding, panes and tabs

**Horizontal padding.** The `less_horizontal_terminal_padding` Cargo feature is
in this fork's default list, which sets the grid's left padding to **16 px**
instead of 20 px. There is no user setting for it — it is compiled in.

**Full-screen (alt-screen) apps.** `remove_alt_screen_padding` is likewise a
default feature. With it on, `appearance.full_screen_apps.alt_screen_padding`
decides how much padding a full-screen TUI gets:

```toml
[appearance.full_screen_apps]
# default: zero padding, so vim/htop fill the pane edge to edge
alt_screen_padding = { custom = { uniform_padding = 0.0 } }

# or: match the block list's padding
# alt_screen_padding = "match_blocklist"
```

The default is `custom` with `uniform_padding = 0.0`. Two programs — `k9s` and
`lazygit` — are hard-coded to always use block-list padding regardless, because
they misbehave otherwise.

**Block spacing.** `appearance.spacing` is `normal` (default) or `compact`.
`appearance.blocks.show_block_dividers` (default `true`) and
`appearance.blocks.show_jump_to_bottom_of_block_button` (default `true`) round
out the block chrome.

**Panes.** `appearance.panes.should_dim_inactive_panes` (default `false`) and
`appearance.panes.focus_pane_on_hover` (default `false`, i.e. focus-follows-mouse
is off).

**Tabs.** `appearance.tabs.tab_close_button_position` (`right` by default; the
setting is only honoured because `tab_close_button_on_left` is a default feature
— without that feature the position is forced back to `right`),
`appearance.tabs.show_indicators_button`,
`appearance.tabs.enable_tab_groups`, `appearance.tabs.preserve_active_tab_color`,
`appearance.tabs.directory_tab_colors` (colour tabs by directory or repo), and
`appearance.tabs.workspace_decoration_visibility` (`always_show` /
`hide_fullscreen` (default) / `on_hover` — the `full_screen_zen_mode` feature
is on by default).
A separate `appearance.vertical_tabs.*` group configures the vertical tab panel.

**Window.** `appearance.window.override_opacity` (0–100, default 100),
`appearance.window.open_windows_at_custom_size` with
`new_windows_num_columns` / `new_windows_num_rows` (80 × 40),
`appearance.window.zoom_level` (default 100).
`appearance.window.override_blur` is macOS-only and
`override_blur_texture` is Windows-only.

### Accessibility

`accessibility.accessibility_verbosity` controls how much a screen reader is
told: `VERBOSE` (default — announces the element's label *and* its help text) or
`CONCISE` (label only).

Two caveats, both important:

- **There is no settings UI for it.** No page in `settings_view/` reads
  `AccessibilitySettings`; the value comes from `settings.toml` (or the local
  control surface). Write it by hand.
- **On Linux it currently changes nothing.** Announcements are only emitted when
  the platform reports a screen reader as active, and the winit (Linux/Windows)
  delegate's `is_screen_reader_enabled()` returns "unknown", which is treated as
  *false*. Only the macOS delegate answers, so screen-reader support in practice
  means VoiceOver. There is no AT-SPI or AccessKit integration in the tree.

Phosphor draws its own UI rather than using platform widgets, so accessibility
is hand-built per element; the framework's own module documentation notes that
keyboard-activating an arbitrary focused control is still missing. The theme
chooser explicitly announces that it is not screen-reader compatible.

For low vision without a screen reader, the useful levers are
`appearance.text.enforce_minimum_contrast = "always"`, a larger
`appearance.text.font_size` / `ui_font_size`, `appearance.window.zoom_level`, and
a high-contrast custom theme. There is no dedicated high-contrast mode.

### Linux specifics

Linux is this fork's primary platform. One setting is Linux-only:

```toml
[system]
force_x11 = false   # default; true on WSL
```

`system.force_x11` makes Phosphor run under XWayland instead of Wayland. The
default is `false` except on WSL, where it is `true`. The UI presents the
inverse: Settings → Features → *Use Wayland for window management*, whose tooltip
warns that enabling Wayland disables global hotkey support and that text may
blur under fractional compositor scaling.

**Crash recovery will flip this behind your back.** If Phosphor crashes while
running on Wayland with `force_x11` explicitly `false`, the recovery process
relaunches under X11 and *writes* `force_x11 = true` to your preferences. If you
find yourself on X11 without having asked, that is why — set it back.

Two other Linux notes: `appearance.window.override_blur` and
`override_blur_texture` are macOS/Windows respectively and do nothing here; and
window transparency depends on your graphics stack, which is why the Appearance
page can show "Transparency is not supported with your graphics drivers."

### Appearance reference

| setting | what it does | default | where |
|---|---|---|---|
| `appearance.themes.theme` | Active theme | `phosphor_amber` | Appearance → Themes |
| `appearance.themes.system_theme` | Follow the OS light/dark setting | `false` | Appearance → Themes → *Sync with OS* |
| `appearance.themes.selected_system_themes` | `{ light = …, dark = … }` used when the above is on | Light / Dark | same |
| `appearance.text.font_name` | Terminal font | `Hack` | Appearance → Text |
| `appearance.text.font_size` | Terminal font size, px | `13.0` | Appearance → Text |
| `appearance.text.font_weight` | `thin`…`black` | `normal` | Appearance → Text |
| `appearance.text.fallback_font_name` | Second font for missing glyphs | `""` (system) | Appearance → Text |
| `appearance.text.line_height_ratio` | Line height multiplier | `1.2` | Appearance → Text |
| `appearance.text.ligature_rendering_enabled` | Render ligatures | `false` | Appearance → Text |
| `appearance.text.enforce_minimum_contrast` | `never` / `only_named_colors` / `always` | `only_named_colors` | Appearance → Text |
| `appearance.text.use_thin_strokes` | macOS glyph thinning | `on_high_dpi_displays` | macOS only |
| `appearance.text.ui_font_name` | UI font | `""` (system) | Appearance → Text |
| `appearance.text.ui_font_size` | UI font size, px (8–20) | `12.0` | Appearance → Text |
| `appearance.text.ai_font_name` | Font for agent output | `Hack` | Appearance → Text |
| `appearance.text.match_ai_font` | Pin the agent font to the terminal font | `false` | Appearance → Text |
| `appearance.text.notebook_font_size` | Notebook font size | `14.0` | Appearance → Text |
| `appearance.text.match_notebook_to_monospace_font_size` | Pin notebook size to terminal size | `true` | Appearance → Text |
| `appearance.text.markdown_heading_h1_scale` … `h6_scale` | Heading multipliers (clamped 0.1–5.0) | `2.0`, `1.5`, `1.17`, `1.0`, `0.83`, `0.75` | Appearance → Text |
| `appearance.icon.app_icon` | Dock icon | `default` | macOS only |
| `appearance.spacing` | `normal` / `compact` block spacing | `normal` | Appearance → Blocks |
| `appearance.full_screen_apps.alt_screen_padding` | Padding for full-screen TUIs | `{ custom = { uniform_padding = 0.0 } }` | Appearance → Full-screen Apps |
| `appearance.blocks.show_block_dividers` | Draw dividers between blocks | `true` | Appearance → Blocks |
| `appearance.blocks.show_jump_to_bottom_of_block_button` | Jump-to-bottom affordance | `true` | Appearance → Blocks |
| `appearance.panes.should_dim_inactive_panes` | Dim unfocused panes | `false` | Appearance → Panes |
| `appearance.panes.focus_pane_on_hover` | Focus follows mouse | `false` | Appearance → Panes |
| `appearance.window.override_opacity` | Window opacity, 0–100 | `100` | Appearance → Window |
| `appearance.window.open_windows_at_custom_size` | Use fixed rows/cols for new windows | `false` | Appearance → Window |
| `appearance.window.new_windows_num_columns` / `_rows` | That size | `80` / `40` | Appearance → Window |
| `appearance.window.zoom_level` | App zoom, % | `100` | Appearance → Zoom |
| `accessibility.accessibility_verbosity` | `VERBOSE` / `CONCISE` | `VERBOSE` | TOML only |
| `system.force_x11` | Use X11 instead of Wayland | `false` (`true` on WSL) | Features → *Use Wayland…* (inverted) |

---

## Not available in Phosphor

A Warp user will look for these and not find them.

- **Cloud settings sync / "my themes follow me between machines".** The
  `PreferencesSyncer` that synced `settings.toml` against Warp's server was
  physically removed; only local TOML loading remains. Settings still carry
  `SyncToCloud` metadata internally, but nothing consumes it. Move your
  `settings.toml` and `themes/` directory yourself.
- **Sharing a theme through Warp Drive, or browsing a theme gallery.** There is
  no cloud object store and no gallery backend. Custom themes are files on
  disk; share them as files.
- **Referral-reward themes.** Two exist internally (`SentReferralReward`,
  `ReceivedReferralReward`) but there is no referral programme to earn them —
  there are no accounts (`DECLINED.md`, "Account-first onboarding, billing,
  paid tiers").
- **An SSH connection manager.** Phosphor deliberately does not become a second
  source of truth for SSH hosts or keys, and never writes to `~/.ssh/config`.
  Manage hosts in `~/.ssh/config` and keys in `ssh-agent`; Phosphor reads what
  `ssh -G` tells it. Two upstream requests (SSH-manager split panes, pasting
  private-key text) are declined outright (`DECLINED.md`, "SSH connection
  management — the system owns it, not the app").
- **Downloadable fallback fonts.** Present in the code, non-functional in this
  fork — see the Fonts section above. Not a declined feature, just broken; use
  a system-installed fallback font instead.
- **Team- or org-enforced appearance policy.** `UserWorkspaces::has_teams()` is
  hard-coded `false` and `current_team()` returns `None`; there is no policy
  layer to enforce a theme (`DECLINED.md`, "Teams stay stubbed").
- **In-app documentation links.** The "Learn more" links on the Phosphorize page
  and in the SSH blocks are empty strings — there is no hosted docs site.
- **Alacritty settings import on Linux.** The importer exists but is behind the
  non-default `alacritty_settings_import` feature; the iTerm importer is macOS
  only. In practice Settings → import has nothing to offer a Linux user.

<!-- SOURCES
Bootstrap scripts and mechanism:
app/assets/bundled/bootstrap/bash.sh:1-80 (vendored bash-preexec, HISTCONTROL)
app/assets/bundled/bootstrap/bash_body.sh:7 (WARP_BOOTSTRAPPED guard)
app/assets/bundled/bootstrap/bash_body.sh:100-115 (WARP_IS_SSH, ExitShell trap)
app/assets/bundled/bootstrap/bash_body.sh:1018-1187 (warp_ssh_helper, ssh() wrapper, RemoteCommand + remote-shell fallbacks, ControlMaster reuse, remote bash/zsh branches)
app/assets/bundled/bootstrap/bash_body.sh:1195-1200 (MoTD, "Warp bash shells are _not_ login shells")
app/assets/bundled/bootstrap/zsh.sh:1-21 (PS2/ORIGINAL_PS2, --no-rcs)
app/assets/bundled/bootstrap/fish.sh:742 (TERM_PROGRAM='WarpTerminal'), :849 (WARP_BOOTSTRAPPED)
app/assets/bundled/bootstrap/pwsh_init_shell.ps1:1-33 (PSReadline removal, ExecutionPolicy Restricted error)
app/assets/bundled/bootstrap/bash_init_shell.sh, zsh_init_subshell.sh, fish_init_subshell.sh, unknown_init_subshell.sh (InitShell/is_subshell hooks, WARP_IS_SUBSHELL)
app/assets/bundled/bootstrap/bash_zsh_subshell_bootstrap_block_output.txt, fish_subshell_bootstrap_block_output.txt (Auto-Warpify rc snippets)
app/src/terminal/warpify/mod.rs:24-32 (PowerShell has no subshell success block), :39-91 (tmux:false template arg)
app/src/terminal/model/ansi/dcs_hooks.rs:901-905 (SourcedRcFileForWarpValue: shell/uname/tmux)

Shell launch:
app/src/terminal/local_tty/shell.rs:625-742 (per-shell spawn args: zsh --no-rcs -g, bash --rcfile <(...), fish --no-config/-f no-mark-prompt, pwsh -NoLogo -NoProfile -NoExit -EncodedCommand)
app/src/terminal/local_tty/shell.rs:744-770 (WSL: bash/zsh/fish only, todo!() for PowerShell)
app/src/terminal/local_tty/unix.rs:337-347 (TERM, TERM_PROGRAM=WarpTerminal, COLORTERM, TERM_PROGRAM_VERSION, WARP_CLIENT_VERSION)
app/src/terminal/local_tty/unix.rs:372-381 (WARP_USE_SSH_WRAPPER, WARP_SSH_REUSE_CONTROL_MASTER env vars)

Warpify settings and detection:
app/src/terminal/warpify/settings.rs:18-46 (added_subshell_commands, subshell_commands_denylist, ssh_hosts_denylist TOML paths + defaults)
app/src/terminal/warpify/settings.rs:48-56 (enable_ssh_warpification default true)
app/src/terminal/warpify/settings.rs:58-108 (EnableSshWrapper deprecated, the sole declaration of warpify.ssh.enable_legacy_ssh_wrapper / EnableSSHWrapper, SyncToCloud::Never)
app/src/terminal/warpify/settings.rs:85-94 (use_ssh_tmux_wrapper default false, MAC|LINUX only)
app/src/terminal/warpify/settings.rs:96-130 (SshExtensionInstallMode: AlwaysAsk default)
app/src/terminal/warpify/settings.rs:300-330 (one-time migration disabling enable_ssh_warpification)
app/src/terminal/warpify/settings.rs:390-420 (SUBSHELL_COMMAND_REGEXES: shells, docker run/exec, poetry, pipenv, aws-vault, flox; WSL_SUBSHELL_REGEX windows-only)
app/src/terminal/warpify/settings.rs:~470-500 (is_compatible_subshell_command: ssh-like when !use_ssh_tmux_wrapper; PowerShell hard-coded ssh banner)
app/src/settings/ssh.rs:28-44 (SshSettings::reuse_existing_control_master default false)
app/src/terminal/ssh/ssh_detection.rs:24-51 (evaluate_warpify_ssh_host gating)
app/src/terminal/ssh/util.rs:155-215 (SshWarpifyCommand: ssh, gcloud compute ssh, eb ssh, doctl compute ssh; -T/-W non-interactive)
app/src/terminal/local_tty/terminal_manager.rs:688-716 (tmux wrapper supersedes ControlMaster; computed at PTY creation)
app/src/terminal/ssh/install_tmux.rs:381-383 (tmux >= 3.0), :523-604 (bundled install scripts: brew/linux/apt/dnf/pacman/yum/zypper)
app/assets/bundled/ssh/bash_zsh/warpify_ssh_session.sh:45-60 (~/.warp/tmux/execute_tmux.sh, tmux version check)
app/src/terminal/view/init.rs:344-371 (ctrl-i for terminal:warpify_subshell and terminal:warpify_ssh_session, context predicates)
app/src/terminal/ssh/warpify.rs:38-45 (ctrl-c interrupts SshWarpifyBlock)
app/src/terminal/warpify/render.rs:35-36 (SSH_DOCS_URL and SUBSHELL_DOCS_URL are empty)
app/src/settings_view/warpify_page.rs:559-566 (empty "Learn more" hyperlink)
app/i18n/en/warp.ftl:847-866 (page title "Phosphorize", section labels, "Takes effect in new tabs")
app/Cargo.toml:641 ("ssh_tmux_wrapper" in default), :674 (feature decl)
app/src/lib.rs:2973 (FeatureFlag::SSHTmuxWrapper gated on the cargo feature)
DECLINED.md:218 (tmux wrapper kept deliberately; Windows/ConPTY cannot do DCS; tmux still required on remote)
DECLINED.md:236-255 (SSH connection management declined; nothing writes ~/.ssh/config)

Troubleshooting:
app/src/settings/block_visibility.rs:1-34 (three block-visibility settings, all default false)
app/src/settings/debug.rs:20-28 (is_shell_debug_mode_enabled, private, WARP_SHELL_DEBUG_MODE)
app/src/app_menus.rs:691-720 (Debug menu gated on FeatureFlag::DebugMode), :573,:611-630 (bootstrap/SSH block menu items)
crates/warp_features/src/lib.rs:803,818 (DEBUG_FLAGS added by phosphor-oss in debug builds only)
app/src/terminal/session_settings.rs:299-307 (honor_ps1 default false, toml_path terminal.input.honor_ps1)
app/src/settings/init.rs:242-252 (honor_ps1 flips input box to Classic)

Paths:
crates/warp_core/src/paths.rs:132-141 (data_dir), :256-258 (themes_dir = data_dir/themes), :143-158 (config_local_dir)
crates/warp_core/src/paths_tests.rs:5-38 (exact Linux/macOS/Windows data and config dirs for Channel::Oss)
app/src/settings/mod.rs:646-652 (settings.toml = config_local_dir/settings.toml)

Themes:
app/src/themes/theme.rs:27-64 (ThemeKind derives SettingsValue; PhosphorAmber is #[default])
app/src/themes/theme.rs:509-551 (27 bundled themes registered)
app/src/themes/theme.rs:106-121 (Custom / CustomBase16; "Base16" name prefix routing)
app/src/themes/theme.rs:184-232 (CustomTheme SettingsValue: name + path, path stored relative to themes dir)
app/src/settings/theme.rs:14-46 (ThemeSettings: appearance.themes.theme / system_theme / selected_system_themes; system_theme default false)
app/src/settings/initializer.rs:60-95 (Phenomenon->Adeberry override never fires; PhosphorAmber is the fork default)
crates/warp_core/src/ui/theme/mod.rs:590-610 (WarpTheme YAML fields: background, accent, foreground, cursor, background_image, details, terminal_colors, name, ui_colors)
crates/warp_core/src/ui/theme/mod.rs:283-289 (Fill is untagged: hex | vertical | horizontal gradient), :161-166 (top/bottom), :208-213 (left/right)
crates/warp_core/src/ui/theme/mod.rs:571-582 (Details: darker/lighter/custom; TerminalColors normal+bright)
crates/warp_core/src/ui/theme/mod.rs:26-37 (Image: path + opacity)
crates/warp_core/src/ui/theme/ui_colors.rs:14-67 (18 optional UI colour overrides)
app/src/user_config/util.rs:17 (.yaml/.yml), :144-157 (name from file when absent), :191-205 (WalkDir recursive, follow_links)
app/src/user_config/native.rs:83-92 (themes hot-reload on directory change)
app/src/themes/theme_creator_body.rs:150-230 (creator writes <name>.yaml + copies image into themes dir)
app/src/workspace/mod.rs:441-448 (workspace:show_theme_chooser, no default binding)
themes/phosphor_amber.yaml, themes/one_dark.yaml (worked bundled examples)

Fonts:
app/src/settings/font.rs:18-27 (Hack, 13.0, Weight::Normal, DEFAULT_UI_FONT_NAME "")
app/src/settings/font.rs:29-200 (all FontSettings toml paths and defaults)
crates/warp_core/src/ui/appearance.rs:18-23 (DEFAULT_UI_FONT_SIZE 12, min 8, max 20)
crates/warpui_core/src/elements/text.rs:35 (DEFAULT_UI_LINE_HEIGHT_RATIO 1.2)
crates/warpui_core/src/fonts.rs:27-48 (Weight variants, Normal default)
crates/warpui_core/src/rendering/mod.rs:20-30 (ThinStrokes, OnHighDpiDisplays default)
app/src/settings/mod.rs:528-536 (EnforceMinimumContrast, OnlyNamedColors default)
app/src/terminal/ligature_settings.rs:8-26 (default false, ANDed with FeatureFlag::Ligatures)
app/Cargo.toml:495 ("ligatures" in default)
app/assets/bundled/fonts/hack/ (Hack ships bundled)
app/src/font_fallback.rs:8-15 (url_for_font uses ChannelState::url_scheme())
crates/warp_core/src/channel/state.rs:261-275 (Channel::Oss url_scheme = "phosphor")
app/src/lib.rs:1567 (set_fallback_font_source_provider -> asset_cache::url_source)
crates/asset_cache/src/lib.rs:34-47,144-157 (url_source -> reqwest::get; no custom-scheme handling)
git show a186e0041 -- app/src/font_fallback.rs (URL rewritten from ChannelState::server_root_url() to the app URL scheme)
script/font_fallback/generate-families.py:1-20, generate-mappings.py:1-23 (read gs://warp-static-assets; maintainer-only)
script/patch_font_with_warp_glyph:1-30 (FontForge, U+E500, glyph "warpLogo")

App icon:
app/src/settings/app_icon.rs:29-66 (17 variants), :124-135 (SupportedPlatforms::MAC, toml_path appearance.icon.app_icon)
app/src/settings_view/appearance_page.rs:1440-1442 (Icon category rendered only when supported on current platform)
app/i18n/en/warp.ftl:1469-1471 (bundle + restart warnings)

Layout:
app/src/terminal/view.rs:750-755 (PADDING_LEFT 16 vs 20 under LessHorizontalTerminalPadding)
app/src/terminal/view.rs:622 (ALT_SCREEN_APPS_THAT_MUST_MATCH_BLOCKLIST_PADDING = k9s, lazygit)
app/src/terminal/view.rs:27433-27450 (alt-screen padding applied under RemoveAltScreenPadding)
app/src/terminal/settings.rs:39-82 (AltScreenPaddingMode, Custom{0.0} default), :141-168 (appearance.spacing, alt_screen_padding)
app/Cargo.toml:487-488 (remove_alt_screen_padding, less_horizontal_terminal_padding in default)
app/src/settings/pane.rs:6-21 (pane settings, both default false)
app/src/terminal/block_list_settings.rs:7-43 (block dividers, jump-to-bottom, both default true)
app/src/window_settings.rs:7-79 (opacity/blur/size/zoom defaults and platform scoping)
app/src/workspace/tab_settings.rs:55-58 (TabCloseButtonPosition, Right is #[default]), :67,104,196,284 (tab toml paths), :88-95 (WorkspaceDecorationVisibility, HideFullscreen default)
app/src/workspace/view.rs:17806-17810 (close-button position forced to default unless FeatureFlag::TabCloseButtonOnLeft)
app/src/settings_view/appearance_page.rs:2744-2770 (Appearance -> Input type radio sets InputBoxType and honor_ps1)
crates/warp_terminal/src/shell/mod.rs:343-360 (rc_file_paths: ~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)
app/src/workspace/mod.rs:334-338 (cmdorctrl-, opens settings), :342-421 (zoom vs font-size bindings under UIZoom)
DECLINED.md:183 (ctrl-shift-> font size vs file_tree hidden-files collision on Linux)

Accessibility:
app/src/settings/accessibility.rs:6-17 (a11y_verbosity, toml_path accessibility.accessibility_verbosity)
crates/warpui_core/src/accessibility.rs:78-98 (AccessibilityVerbosity; serde rename "VERBOSE"/"CONCISE", Verbose default)
crates/settings_value_derive/src/lib.rs:363-371 (file value = serde rename > container rename_all > snake_case) — hence VERBOSE/CONCISE in TOML while schemars advertises lowercase
crates/warpui_core/src/core/app.rs:1292-1320 (announcements gated on is_screen_reader_enabled().unwrap_or(false))
crates/warpui/src/windowing/winit/delegate.rs:564-567 (winit delegate returns None)
crates/warpui/src/platform/mac/delegate.rs:173,423 (only macOS answers)
crates/warpui_core/src/accessibility.rs:1-46 (module doc: custom UI framework, VoiceOver testing, missing keyboard activation)
app/src/themes/theme_chooser.rs:944 (theme chooser not screen-reader compatible)
No settings_view file references AccessibilitySettings (grep) — no UI surface.

Linux:
app/src/settings/linux.rs:5-17 (force_x11, default linux::is_wsl(), toml_path system.force_x11, LINUX only)
app/src/lib.rs:1189-1198 (force_x11 -> app_builder.force_x11)
app/src/crash_recovery.rs:296-306,443-452 (X11 recovery mechanism writes force_x11=true)
app/i18n/en/warp.ftl:1155-1223 (Use Wayland labels, hotkey/fractional-scaling warnings)

Not available:
app/src/lib.rs:2092-2098 (PreferencesSyncer physically removed; local TOML only)
DECLINED.md:74-90 (cloud out of scope), :189 (shared-session heartbeat), :228 (no account fields)
app/Cargo.toml:807 (alacritty_settings_import not in default)
app/src/settings/import/model.rs:20-27 (iTerm import is macOS-only)
-->
