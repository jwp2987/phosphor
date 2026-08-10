---
name: tui-settings
description: Explain which Phosphor settings actually drive the terminal UI, and which are GUI-only, given that the GUI and the TUI share one settings file. Use when the user asks how to configure the TUI, whether a setting applies to the TUI, why changing something in one surface showed up in the other, where the TUI's theme / statusline / model / API keys come from, or how to "migrate" a GUI setup to the TUI.
---

# tui-settings

Phosphor's desktop GUI and its terminal UI are **one application with one
identity**. They share a single settings file, a single secure-storage
namespace, and a single MCP config. There is no separate TUI settings file, and
there is nothing to migrate or copy between the two — if the user asks to move
their GUI setup into the TUI, the answer is that it is already there.

The trade-off is that there is **no TUI-only override**: a shared key changed in
either surface changes both. Say so before editing one.

## The settings file

Both surfaces read and write:

```
{{settings_file_path}}
```

Phosphor hot-reloads this file, so an edit takes effect without a restart. To
look up a key's exact path, type, or valid values, use the `modify-settings`
skill — it drives the generated JSON schema, which is the source of truth for
everything below.

## Keys the TUI reads

Prefer the in-TUI slash command when one exists: it writes the same key, applies
immediately, and cannot produce a malformed value.

### TUI-only — the GUI ignores these

| TOML | Effect | In-TUI |
|---|---|---|
| `[appearance] theme` | `auto` \| `light` \| `dark`. `auto` follows the host terminal's background luminance. | `/theme <auto\|light\|dark>` |
| `[appearance.zero_state] object`, `rotation_period_seconds`, `extrusion_depth` | The rotating object on the TUI's empty screen. | — |
| `[general] autoupdate_enabled` | The TUI's background auto-updater. Read once at TUI startup. | — |
| `agents.statusline` | A table of `order` and `enabled` controlling the bottom statusline. | `/statusline` |

### Shared with the GUI — changing one changes both

| TOML | Effect in the TUI | In-TUI |
|---|---|---|
| `[agents.warp_agent] providers` | The configured BYOP providers, and therefore the models `/model` offers. | `/api-keys` |
| `[agents.byop] last_used_model_id`, `last_used_reasoning` | The model and reasoning depth new sessions start with. `/model` writes them, so a model picked in the TUI is also what the GUI hydrates new tabs from. | `/model` |
| `[agents.mcp_servers] file_based_mcp_enabled` | Whether file-based MCP servers are loaded at all. | `/mcp` |
| `[agents.warp_agent.input] ai_auto_detection_enabled` | Whether typed input is classified as a prompt vs. a command. | `/natural-language-detection` |
| `[text_editing] vim_mode_enabled` | Vim keybindings in the TUI's input box. | `/vim-mode` |
| `[appearance] language` | The UI language. | — |
| `[session] new_session_shell_override` (legacy fallback: `[session] startup_shell_override`) | Which shell the TUI spawns for a session. | — |
| `[terminal.input] honor_ps1` | Whether the spawned shell's own prompt is honored. | — |
| `[terminal.smart_select] enabled`, `word_char_allowlist` | Word boundaries when selecting text in the transcript. | — |
| `[agents.warp_agent] prompt_template_dir` | Overrides the built-in agent prompt templates. | — |

## Shared, but not in the settings file

- **Provider API keys** live in the OS secure store, not on disk. One namespace
  serves both surfaces, so a key added in the GUI works in the TUI with no
  reconfiguration, and vice versa. A key written by the TUI's
  `--set-provider-api-key` command is picked up by already-running Phosphor
  processes without a restart.
- **MCP servers** come from the shared global and project `.mcp.json` files. Use
  the `add-mcp-server` skill; do not hand-resolve the global path.
- **Rules and skills** are discovered from the same paths in both surfaces.

## What does not reach the TUI

Do not promise these — they have no TUI equivalent.

- **`[appearance.themes] theme` / `system_theme` / `selected_system_themes`.**
  These are the GUI's named-theme selection. The TUI overrides the app theme at
  startup with its own light/dark resolution, so a named theme has no effect
  there. Note how close the two keys look: `[appearance] theme` is the TUI's,
  `[appearance.themes] theme` is the GUI's. Confirm which one is meant.
- **`[updates] automatic_updates_enabled`** is the GUI updater. The TUI's is
  `[general] autoupdate_enabled`.
- **Font settings.** The TUI renders in the host terminal's cells and uses the
  terminal's own font.
- **`{{keybindings_file_path}}`.** This build's TUI does not load custom
  keybindings; its bindings are the ones registered in the TUI process. Edits
  here affect the GUI only.
- **Anything naming a GUI surface** — panes, tabs, blocks, the command palette,
  notifications, window chrome. The TUI does not render them.

## Workflow

1. Identify the key with the `modify-settings` skill before editing anything.
2. If it is in the "does not reach the TUI" list, say so plainly rather than
   editing a key that will not do what the user wants.
3. If an in-TUI command exists for it, recommend that instead of a file edit.
4. If it is a shared key, tell the user it also changes the other surface before
   you write it.
