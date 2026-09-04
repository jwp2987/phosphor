# Migrating settings to Phosphor

This guide is for people who want to bring **settings-style configuration**
(custom keybindings, themes, workflows, MCP config, etc.) into Phosphor from a
previous install.

> **Note on names:** this project is now **Phosphor**, on disk as well as in the
> branding. The app id has been `dev.phosphor.Phosphor`
> (`crates/warp_core/src/channel/state.rs`) since 2026-08-14, so every destination
> path below uses `phosphor`. (Earlier names, oldest first: OpenWarp → Zap →
> Phosphor.)
>
> If you are coming from a **Zap** install that predates that rename, its
> directories are a *source*, not a destination — see "Zap source paths" below.
> This note previously claimed the `zap` app id was still in use and pointed every
> destination at it, which sent migrations to directories Phosphor never reads
> (#639).

There are two source installs this might apply to:

1. **OpenWarp / Zap** — this project's own previous names.
2. **Upstream [Warp](https://github.com/warpdotdev/warp)** — the project Phosphor
   is forked from.

The two cases have **different safety profiles** and are covered separately
below. Always migrate from OpenWarp first if both apply.

This guide deliberately does **not** cover command history, the SQLite
database, Drive objects, or any credentials. Those live in stores that are
either machine-bound (Keychain / DPAPI / libsecret) or schema-coupled and are
not safe to copy across forks.

---

## How on-disk state is laid out

Phosphor (and Zap / OpenWarp / upstream Warp before it) splits its on-disk state into
**three categories of directory**:

- **config** — `settings.toml`, `keybindings.yaml`
- **data** — `themes/`, `workflows/`, `launch_configurations/`, `tab_configs/`
- **home dotfile** — `.mcp.json`, `skills/`

On macOS all three categories coincide under a single home dotfile directory
(`~/.phosphor/` here; `~/.warp/`, `~/.openwarp/` or `~/.zap/` for the sources).
On Linux and Windows they live in
**three different places** following XDG conventions on Linux and the
`directories` crate layout on Windows. The migration scripts below take care
of placing each file in the correct destination per platform.

### Phosphor destination paths

| Category | macOS | Linux | Windows |
|---|---|---|---|
| config | `~/.phosphor/` | `${XDG_CONFIG_HOME:-~/.config}/phosphor/` | `%LOCALAPPDATA%\zap\Zap\config\` |
| data | `~/.phosphor/` | `${XDG_DATA_HOME:-~/.local/share}/phosphor/` | `%APPDATA%\zap\Zap\data\` |
| home dotfile | `~/.phosphor/` | `~/.phosphor/` | `%USERPROFILE%\.phosphor\` |

### Zap source paths

| Category | macOS | Linux | Windows |
|---|---|---|---|
| config | `~/.zap/` | `${XDG_CONFIG_HOME:-~/.config}/zap/` | `%LOCALAPPDATA%\zap\Zap\config\` |
| data | `~/.zap/` | `${XDG_DATA_HOME:-~/.local/share}/zap/` | `%APPDATA%\zap\Zap\data\` |
| home dotfile | `~/.zap/` | `~/.zap/` | `%USERPROFILE%\.zap\` |

Zap was this project under its previous name, so the formats are identical and
every file listed below copies across as-is. Substitute these paths for the
OpenWarp ones in the scripts below; `README.md`'s migration table covers the
same move in short form.

### OpenWarp source paths

| Category | macOS | Linux | Windows |
|---|---|---|---|
| config | `~/.openwarp/` | `${XDG_CONFIG_HOME:-~/.config}/openwarp/` | `%LOCALAPPDATA%\openwarp\OpenWarp\config\` |
| data | `~/.openwarp/` | `${XDG_DATA_HOME:-~/.local/share}/openwarp/` | `%APPDATA%\openwarp\OpenWarp\data\` |
| home dotfile | `~/.openwarp/` | `~/.openwarp/` | `%USERPROFILE%\.openwarp\` |

### Upstream Warp source paths

| Category | macOS | Linux | Windows |
|---|---|---|---|
| config | `~/.warp/` | `${XDG_CONFIG_HOME:-~/.config}/warp-terminal/` | `%LOCALAPPDATA%\warp\Warp-Terminal\config\` |
| data | `~/.warp/` | `${XDG_DATA_HOME:-~/.local/share}/warp-terminal/` | `%APPDATA%\warp\Warp-Terminal\data\` |
| home dotfile | `~/.warp/` | `~/.warp/` | `%USERPROFILE%\.warp\` |

> The Linux directory name `warp-terminal` matches the Linux package name
> (e.g. `/opt/warpdotdev/warp-terminal/` on Debian/Ubuntu). The exact Windows
> organization folder may differ depending on how Warp was packaged; if you
> can't find `%APPDATA%\warp\Warp-Terminal` (or `%LOCALAPPDATA%\warp\Warp-Terminal`),
> check the actual `%APPDATA%` / `%LOCALAPPDATA%` location your Warp install
> uses.

---

## 1. From OpenWarp (recommended path for existing users)

OpenWarp **was** Zap. The rename commit (`feat: rename project Warp/OpenWarp →
Zap`) only renamed identifiers and on-disk paths — the **configuration file
formats and schemas did not change**, so the files below can be copied across
as-is.

### Files to copy

| File or folder | Category | What it controls |
|---|---|---|
| `settings.toml` | config | Public settings (the TOML-backed settings file). |
| `keybindings.yaml` | config | Custom keybindings. |
| `themes/` | data | Custom themes. |
| `workflows/` | data | Custom workflows. |
| `launch_configurations/` | data | Launch configurations. |
| `tab_configs/` | data | Tab configurations. |
| `.mcp.json` | home dotfile | MCP server configuration. |
| `skills/` | home dotfile | Agent skills. |

### Steps

> Quit Phosphor before copying, so no process is holding the files open.

**macOS**

```sh
mkdir -p "$HOME/.phosphor"
for f in settings.toml keybindings.yaml themes workflows launch_configurations tab_configs skills .mcp.json; do
  if [ -e "$HOME/.openwarp/$f" ] && [ ! -e "$HOME/.phosphor/$f" ]; then
    cp -R "$HOME/.openwarp/$f" "$HOME/.phosphor/$f"
  fi
done
```

**Linux**

```sh
src_config="${XDG_CONFIG_HOME:-$HOME/.config}/openwarp"
src_data="${XDG_DATA_HOME:-$HOME/.local/share}/openwarp"
src_home="$HOME/.openwarp"

dst_config="${XDG_CONFIG_HOME:-$HOME/.config}/phosphor"
dst_data="${XDG_DATA_HOME:-$HOME/.local/share}/phosphor"
dst_home="$HOME/.phosphor"
mkdir -p "$dst_config" "$dst_data" "$dst_home"

copy() {
  if [ -e "$1/$3" ] && [ ! -e "$2/$3" ]; then
    cp -R "$1/$3" "$2/$3"
  fi
}

copy "$src_config" "$dst_config" settings.toml
copy "$src_config" "$dst_config" keybindings.yaml
copy "$src_data"   "$dst_data"   themes
copy "$src_data"   "$dst_data"   workflows
copy "$src_data"   "$dst_data"   launch_configurations
copy "$src_data"   "$dst_data"   tab_configs
copy "$src_home"   "$dst_home"   .mcp.json
copy "$src_home"   "$dst_home"   skills
```

**Windows (PowerShell)**

```powershell
$src_config = "$env:LOCALAPPDATA\openwarp\OpenWarp\config"
$src_data   = "$env:APPDATA\openwarp\OpenWarp\data"
$src_home   = "$env:USERPROFILE\.openwarp"

$dst_config = "$env:LOCALAPPDATA\phosphor\Phosphor\config"
$dst_data   = "$env:APPDATA\phosphor\Phosphor\data"
$dst_home   = "$env:USERPROFILE\.phosphor"
New-Item -ItemType Directory -Force -Path $dst_config, $dst_data, $dst_home | Out-Null

function Copy-IfMissing($srcDir, $dstDir, $name) {
  $from = Join-Path $srcDir $name
  $to   = Join-Path $dstDir $name
  if ((Test-Path $from) -and -not (Test-Path $to)) {
    Copy-Item -Path $from -Destination $to -Recurse
  }
}

Copy-IfMissing $src_config $dst_config settings.toml
Copy-IfMissing $src_config $dst_config keybindings.yaml
Copy-IfMissing $src_data   $dst_data   themes
Copy-IfMissing $src_data   $dst_data   workflows
Copy-IfMissing $src_data   $dst_data   launch_configurations
Copy-IfMissing $src_data   $dst_data   tab_configs
Copy-IfMissing $src_home   $dst_home   .mcp.json
Copy-IfMissing $src_home   $dst_home   skills
```

The `[ ! -e ... ]` / `-not (Test-Path $to)` guard avoids overwriting anything
you might have already set in Phosphor. Drop it if you'd rather have OpenWarp's
values win.

After verifying Phosphor looks right, you can delete the OpenWarp directories above
to reclaim disk space. They're no longer used by anything.

---

## 2. From upstream Warp

Upstream Warp is a separate product with its own on-disk identity (see the
"Upstream Warp source paths" table above). Phosphor is built with channel `Oss`,
which gives it its own app ID (`dev.phosphor.Phosphor`) and its own per-platform layout.
The two installations cannot see each other's files, which is also what keeps
your Warp account / cloud state out of Phosphor.

The text-format files listed below have stable, compatible schemas, so copying
them across is safe. **Other state is not** — Warp evolves independently of
Phosphor, and binary / private stores can be tied to Warp's auth and bundle
identity.

### What to copy

Same eight items as above:

| File or folder | Category | What it controls |
|---|---|---|
| `settings.toml` | config | Public settings (the TOML-backed settings file). |
| `keybindings.yaml` | config | Custom keybindings. |
| `themes/` | data | Custom themes. |
| `workflows/` | data | Custom workflows. |
| `launch_configurations/` | data | Launch configurations. |
| `tab_configs/` | data | Tab configurations. |
| `.mcp.json` | home dotfile | MCP server configuration. |
| `skills/` | home dotfile | Agent skills. |

### What **not** to copy

- **`user_preferences.json`** — a private store under
  `~/Library/Application Support/dev.warp.Warp/` (macOS) or the equivalent
  state directory on Linux/Windows. Mixes user preferences with auth tokens,
  machine-bound IDs and cached cloud state. Copying it can leak identity and
  confuse Phosphor's auth state. Phosphor defaults are already privacy-friendly.
- **`warp.sqlite`** (and its `-wal` / `-shm` sidecars) — schema is coupled
  to upstream Warp and not guaranteed to be compatible with Phosphor's migrations.
- **Keychain / DPAPI / libsecret entries** — bound to the Warp bundle /
  service name, useless to Phosphor.

### Steps

> Quit both Warp and Phosphor before copying.

**macOS**

```sh
mkdir -p "$HOME/.phosphor"
for f in settings.toml keybindings.yaml themes workflows launch_configurations tab_configs skills .mcp.json; do
  if [ -e "$HOME/.warp/$f" ] && [ ! -e "$HOME/.phosphor/$f" ]; then
    cp -R "$HOME/.warp/$f" "$HOME/.phosphor/$f"
  fi
done
```

**Linux**

```sh
src_config="${XDG_CONFIG_HOME:-$HOME/.config}/warp-terminal"
src_data="${XDG_DATA_HOME:-$HOME/.local/share}/warp-terminal"
src_home="$HOME/.warp"

dst_config="${XDG_CONFIG_HOME:-$HOME/.config}/phosphor"
dst_data="${XDG_DATA_HOME:-$HOME/.local/share}/phosphor"
dst_home="$HOME/.phosphor"
mkdir -p "$dst_config" "$dst_data" "$dst_home"

copy() {
  if [ -e "$1/$3" ] && [ ! -e "$2/$3" ]; then
    cp -R "$1/$3" "$2/$3"
  fi
}

copy "$src_config" "$dst_config" settings.toml
copy "$src_config" "$dst_config" keybindings.yaml
copy "$src_data"   "$dst_data"   themes
copy "$src_data"   "$dst_data"   workflows
copy "$src_data"   "$dst_data"   launch_configurations
copy "$src_data"   "$dst_data"   tab_configs
copy "$src_home"   "$dst_home"   .mcp.json
copy "$src_home"   "$dst_home"   skills
```

**Windows (PowerShell)**

```powershell
$src_config = "$env:LOCALAPPDATA\warp\Warp-Terminal\config"
$src_data   = "$env:APPDATA\warp\Warp-Terminal\data"
$src_home   = "$env:USERPROFILE\.warp"

$dst_config = "$env:LOCALAPPDATA\phosphor\Phosphor\config"
$dst_data   = "$env:APPDATA\phosphor\Phosphor\data"
$dst_home   = "$env:USERPROFILE\.phosphor"
New-Item -ItemType Directory -Force -Path $dst_config, $dst_data, $dst_home | Out-Null

function Copy-IfMissing($srcDir, $dstDir, $name) {
  $from = Join-Path $srcDir $name
  $to   = Join-Path $dstDir $name
  if ((Test-Path $from) -and -not (Test-Path $to)) {
    Copy-Item -Path $from -Destination $to -Recurse
  }
}

Copy-IfMissing $src_config $dst_config settings.toml
Copy-IfMissing $src_config $dst_config keybindings.yaml
Copy-IfMissing $src_data   $dst_data   themes
Copy-IfMissing $src_data   $dst_data   workflows
Copy-IfMissing $src_data   $dst_data   launch_configurations
Copy-IfMissing $src_data   $dst_data   tab_configs
Copy-IfMissing $src_home   $dst_home   .mcp.json
Copy-IfMissing $src_home   $dst_home   skills
```

Your original Warp data is never touched — Warp itself keeps working.

---

## Verifying

Start Phosphor. You should see your custom themes in the theme picker, your
keybindings in the keybinding editor, and your workflows in the workflow
launcher. Settings UI values should reflect what was in `settings.toml`.

If something looks off, the offending file is one of the eight above — open
it in a text editor, or just delete it and let Phosphor fall back to defaults.

## Rolling back

Nothing in this guide is destructive: every file copied is something Phosphor will
recreate from defaults on next launch. To undo everything:

```sh
# macOS
rm -rf ~/.phosphor
```

```sh
# Linux
rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/phosphor"
rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/phosphor"
rm -rf "$HOME/.phosphor"
```

```powershell
# Windows
Remove-Item -Recurse -Force "$env:APPDATA\phosphor"
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\phosphor"
Remove-Item -Recurse -Force "$env:USERPROFILE\.phosphor"
```

The OpenWarp and Warp source directories are never touched by this guide.
