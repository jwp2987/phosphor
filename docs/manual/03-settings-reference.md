# Settings reference

Phosphor keeps almost all of its configuration in a single, hand-editable TOML
file. Every option in the Settings window writes to that file, and every option
in that file can be set without opening the Settings window at all. This section
lists what each setting is called in the file, what type it takes, what it
defaults to, and what it does. It is meant to be searched rather than read start
to finish — if you know roughly what a setting is called, use your reader's find
function on the TOML path.

It is **not** complete. The app registers about fifty settings groups
(`app/src/settings/init.rs`) and this section has a table for a little over
thirty of them; the rest are listed under
[Groups not covered here](#groups-not-covered-here) with a pointer to wherever
in this manual they *are* described. If find turns up nothing for a key, check
that list before concluding the key does not exist.

Two things to know before you start editing.

**Phosphor is a de-Warped fork.** Some settings you will find in the file are
upstream Warp artifacts that gate a feature this build does not have. Where that
is the case the table says so instead of describing the setting as useful. See
[Settings that exist but do nothing](#settings-that-exist-but-do-nothing) and
[Not available in Phosphor](#not-available-in-phosphor).

**`sync_to_cloud` is meaningless here.** If you read the source you will find
every setting carries a `sync_to_cloud` field. It is an upstream artifact.
Phosphor has no cloud backend, no account, and no settings syncer — the field is
read by exactly one thing, a small "this value is local only" icon in the
Settings window, and nothing in this application ever transmits a setting
anywhere. Your settings file never leaves your machine.

---

## Where the settings file lives

The public settings file is always named `settings.toml` and always lives in
Phosphor's local-config directory:

| Platform | Path |
|---|---|
| macOS | `~/.phosphor/settings.toml` |
| Linux / FreeBSD | `~/.config/phosphor/settings.toml` (`$XDG_CONFIG_HOME/phosphor/settings.toml`) |
| Windows | `%LOCALAPPDATA%\phosphor\Phosphor\config\settings.toml` |

Two neighbours live in the same directory:

- `keybindings.yaml` — custom key bindings (covered in the keybindings section
  of this manual, not here).
- `user_preferences.json` — the *private* settings store on Linux and FreeBSD
  (see below).

Portable user data — themes, workflows, launch configs, tab configs — lives in a
different directory (`~/.phosphor` on macOS, `~/.local/share/phosphor` on Linux,
`%APPDATA%\phosphor\Phosphor\data` on Windows). Skills, prompt templates and the
global `.mcp.json` live under `~/.phosphor/` on every platform.

The file is created on demand. If it does not exist, every setting is at its
default; the first change you make through the UI creates it.

### Format

Plain TOML. Keys are snake_case, sections come from the dotted TOML path, and
values are native TOML types — booleans, integers, floats, strings, arrays,
tables — not JSON-encoded strings. A path like
`appearance.text.font_name` means:

```toml
[appearance.text]
font_name = "Hack"
```

Count the full depth of the path before writing the section header; stopping one
level early is the most common editing mistake. `appearance.themes.theme` is
`[appearance.themes] theme = …`, not `[appearance] themes = …`.

Enum-valued settings are written as snake_case strings (`"pinned_to_bottom"`,
`"only_named_colors"`, `"always_ask_before_reading"`). A handful of settings are
rendered as inline tables rather than section headers, because their shape
changes between variants — those are called out in the tables below.

Phosphor preserves your formatting and comments when it writes to the file.

### Hot reload

Phosphor watches the settings file. Saving an edit applies it without a restart.
Two exceptions, both noted again in their tables:

- `appearance.language` — takes full effect only after a restart, because
  already-rendered text is not re-laid-out.
- `appearance.zero_state.object` when it points at an ASCII file — changing the
  *setting* reloads the object, but editing the linked *file* requires a
  restart.

### What happens when the file is wrong

Phosphor never rewrites or rejects your file. It reports and carries on. Three
cases, in decreasing order of severity — only the worst one is surfaced at a
time:

1. **The file is not valid TOML.** Nothing in it takes effect; every setting
   falls back to its default. Writes are inhibited so a broken file is not
   overwritten with defaults — fix the syntax and the next reload recovers.
2. **A value has the right key but the wrong type or an out-of-range value.**
   That one setting falls back to its default; the rest of the file loads
   normally. Writes to that specific key are inhibited so your
   broken-but-fixable value is preserved.
3. **A key matches no setting in this build.** The line is inert — nothing falls
   back, nothing changes. Phosphor logs a warning naming the key. This is the
   *expected* state if you carried a `settings.toml` over from Warp, because
   Phosphor has deliberately removed a number of upstream settings. It is also
   expected for platform-specific settings when you share one settings file
   between machines: `system.force_x11` is a real setting on Linux and an
   unknown key on macOS.

### Private settings

Settings marked `private` in the source are deliberately **not** in
`settings.toml`. They are internal state — "has this banner been dismissed",
"how wide did the user drag this menu" — not configuration, and they are stored
in the platform-native store instead:

| Platform | Private store |
|---|---|
| macOS | `UserDefaults`, domain `dev.phosphor.Phosphor` |
| Linux / FreeBSD | `user_preferences.json`, next to `settings.toml` |
| Windows | Registry, `HKCU\Software\Zap\Phosphor` |

One setting goes further and lives in the OS secure store (Keychain / Credential
Manager / Secret Service): `LocalControlSettings` — see that group's entry.

Private settings are listed in the tables below for completeness, marked
**internal — no TOML key**. There is nothing to edit; they are here so you know
what a key you find in `user_preferences.json` or the registry actually is.

---

## How do I…

**…change a setting?** Either open Settings in the app (the sidebar sections are About,
Appearance, Features, Keyboard shortcuts, Library, Phosphorize, MCP servers,
Agents — with Phosphor Agent, Profiles, MCP servers, Providers, Knowledge and
Third party CLI agents subpages — Network, Privacy, Scripting (only when the
`WarpControlCli` feature is on), and Editor and Code Review), or edit
`settings.toml` directly. Both write the same file.

**…open the settings file quickly?** Type `/open-settings-file` in the input.
It opens `settings.toml` in a code editor pane.

**…find out what a key is called or what values it accepts?** Phosphor ships a
generated JSON Schema of every user-facing setting at
`settings_schema.json` in its bundled resources directory, and a bundled
`modify-settings` skill that drives it. Asking the agent to change a setting
uses that schema, so it will not invent a key.

**…find out whether a setting affects the terminal UI?** Ask the agent — the
bundled `tui-settings` skill answers exactly that. The GUI and the TUI are one
application sharing one settings file, so most keys affect both; a few are
TUI-only and a few are GUI-only. Both are marked in the tables below.

**…reset a setting?** Delete its line from `settings.toml`. The default is used
whenever a key is absent. Deleting the whole file resets every public setting.
Private settings are unaffected by deleting `settings.toml`.

**…keep one settings file across several machines?** You can. Platform-specific
keys will be reported as unknown on the platforms that do not compile them, which
is a warning and not an error.

---

## How to read the tables

Each setting is declared in the source with a fixed set of fields. Here is what
each one means, and how it appears here.

| Field | Meaning | Shown as |
|---|---|---|
| `toml_path` | The dotted path in `settings.toml`. Absent for private settings. | The **TOML path** column |
| `type` | The Rust value type. | The **Type** column, translated to TOML terms |
| `default` | The value used when the key is absent. | The **Default** column |
| `description` | The user-facing description, also emitted into the JSON Schema. | The **What it does** column, rewritten where it was written for a developer |
| `supported_platforms` | Which platforms the setting applies on. `ALL` everywhere; `DESKTOP` means every non-web build, i.e. every build you can actually download; `MAC` / `LINUX` / `WINDOWS` as named; `WEB` means the WebAssembly build only. | Called out per setting where it is not `ALL`/`DESKTOP` |
| `private` | Whether the setting is hidden from `settings.toml`. | **internal — no TOML key** |
| `sync_to_cloud` | Upstream cloud-sync classification. | **Not shown.** It has no effect here |
| `storage_key` | The key used in the private/native store, when it differs from the field name. | Not shown except where it explains a surprising registry or `UserDefaults` key |
| `max_table_depth` | How deeply the value is rendered as TOML section tables. `0` means the value is written as an inline table. | Noted where it is set |
| `feature_flag` | Gates the setting's *appearance in the JSON Schema*, not its runtime behaviour. | Noted where it is set |

`DESKTOP` and `ALL` are equivalent in practice for the builds Phosphor ships;
`WEB`-only settings are inert in every downloadable build.

---

# The settings groups

Groups are listed alphabetically by their source name. The name is a Rust type,
not something you type anywhere — it is here so you can match this reference
against the code.

## Groups not covered here

These groups are registered by the app (`app/src/settings/init.rs`) and their
keys are real, but they have no table in this section. Where another chapter
documents them, it is named.

| Group | Source | Keys | Documented in |
|---|---|---|---|
| `AltScreenReporting` | `app/src/terminal/alt_screen_reporting.rs` | `terminal.mouse_reporting_enabled`, `terminal.scroll_reporting_enabled`, `terminal.focus_reporting_enabled` | §2, *What full-screen programs receive* |
| `BlockListSettings` | `app/src/terminal/block_list_settings.rs` | `appearance.blocks.show_jump_to_bottom_of_block_button`, `appearance.blocks.show_block_dividers`, `general.snackbar_enabled`, `general.preserve_input_focus_on_block_selection` | §2, *Blocks* |
| `CommandSearchSettings` | `app/src/search/command_search/settings.rs` | `workflows.show_global_workflows_in_universal_search` | **nowhere** |
| `EditorSettings` | `app/src/util/file/external_editor/settings.rs` | `code.editor.open_file_editor`, `code.editor.open_code_panels_file_editor`, `code.editor.open_file_layout`, `code.editor.prefer_markdown_viewer`, `code.editor.prefer_tabbed_editor_view`, `agents.warp_agent.other.open_conversation_layout_preference` | **nowhere** |
| `GeneralSettings` | `app/src/terminal/general_settings.rs` | `general.restore_session`, `general.link_tooltip`, `general.show_warning_before_quitting`, `general.quit_on_last_window_closed` (macOS), `general.persist_conversations`, `general.login_item`, `code.editor.auto_open_code_review_pane_on_first_agent_change` | §2 (first two only) |
| `KeysSettings` | `app/src/terminal/keys_settings.rs` | `keys.ctrl_tab_behavior_setting`, `terminal.input.extra_meta_keys`, `global_hotkey.dedicated_window.*`, `global_hotkey.toggle_all_windows.*` | §2 (first two only) |
| `LigatureSettings` | `app/src/terminal/ligature_settings.rs` | `appearance.text.ligature_rendering_enabled` | §2, *Ligatures* |
| `SafeModeSettings` | `app/src/terminal/safe_mode_settings.rs` | `privacy.secret_redaction.*` | below, under `WarpDrivePrivacySettings` |
| `SemanticSelection` | `crates/warp_core/src/semantic_selection/mod.rs` | `terminal.smart_select.enabled`, `terminal.smart_select.word_char_allowlist` | §2, *Clicking and dragging* |
| `SessionSettings` | `app/src/terminal/session_settings.rs` | `terminal.input.honor_ps1`, `general.should_confirm_close_session`, `session.startup_shell_override`, `session.new_session_shell_override`, `notifications.preferences`, `notifications.toast_duration_secs`, and the agent-toolbar chip selections | §2 (first two only) |
| `SharedSessionSettings` | `app/src/terminal/shared_session/settings.rs` | all **private** — no TOML keys. Session-sharing inactivity timers and an onboarding-block flag; session sharing itself has no backend in this fork | §2, *Not available in Phosphor* |
| `TabSettings` | `app/src/workspace/tab_settings.rs` | `general.new_tab_placement`, `appearance.tabs.*`, `appearance.vertical_tabs.*`, `code.editor.show_code_review_*` | §2 (partly) |
| `TerminalSettings` | `app/src/terminal/settings.rs` | `terminal.use_audible_bell`, `terminal.maximum_grid_size`, `terminal.show_terminal_zero_state_block`, `terminal.osc52_clipboard_access`, `appearance.spacing`, `appearance.full_screen_apps.alt_screen_padding`, `experimental.async_find_enabled` | §2 |
| `UndoCloseSettings` | `app/src/undo_close/settings.rs` | `general.undo_close.enabled`, `general.undo_close.grace_period` | §2, *Tabs* |
| `WarpDriveSettings` | `app/src/drive/settings.rs` | `warp_drive.enabled`, `warp_drive.sorting_choice` | **nowhere** |
| `WarpifySettings` | `app/src/terminal/warpify/settings.rs` | `warpify.subshells.*`, `warpify.ssh.ssh_hosts_denylist`, `warpify.ssh.enable_ssh_warpification`, `warpify.ssh.use_ssh_tmux_wrapper`, `warpify.ssh.ssh_extension_install_mode` | **nowhere** (`SshSettings` below is a different group) |
| `WindowSettings` | `app/src/window_settings.rs` | `appearance.window.override_opacity`, `appearance.window.override_blur`, `appearance.window.zoom_level`, `appearance.window.open_windows_at_custom_size`, `appearance.window.new_windows_num_columns` / `_rows`, `appearance.window.left_panel_visibility_across_tabs` | §8 (partly) |
| `WorkflowAliases` | `app/src/workflows/aliases.rs` | **private** — no TOML key. The `WorkflowAliases` list of Library workflow aliases, stored in the native store | **nowhere** |

## AccessibilitySettings

Screen-reader behaviour.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `accessibility.accessibility_verbosity` | `"verbose"` \| `"concise"` | `"verbose"` | How much detail screen-reader announcements carry. `verbose` includes the help string for a control; `concise` announces only the value. **macOS only in practice** — see below. |

`supported_platforms` is `ALL` and the value is read on every platform
(`app/src/lib.rs:1701`), but it only reaches an assistive technology on macOS.
The announcement is delivered through `Delegate::set_accessibility_contents`,
which is implemented on the macOS delegate
(`crates/warpui/src/platform/mac/delegate.rs:302`) and is an empty function on
the winit delegate used by Linux, FreeBSD and Windows
(`crates/warpui/src/windowing/winit/delegate.rs:543`). Changing this key on
those platforms has no observable effect.

## AISettings

By far the largest group: the Phosphor Agent, natural-language detection, agent
permissions, BYOP providers and compaction, CLI-agent integration, and the TUI
statusline. Phosphor is bring-your-own-provider — the models come from providers
you configure yourself.

### The master switches

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.warp_agent.is_any_ai_enabled` | boolean | `true` | The master AI switch. Turns off Phosphor's own AI: the agent, model calls, handoff, orchestration, rich input. It deliberately does **not** disable third-party CLI agents you installed yourself (Claude Code, Codex, Gemini, Antigravity) — those are governed only by their per-agent settings. Every other AI setting is subordinate to this one. |
| `agents.warp_agent.active_ai.enabled` | boolean | `true` | Proactive AI: suggestions offered without you asking. |

### Natural-language detection and input

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.warp_agent.input.ai_auto_detection_enabled` | boolean | `false` | Whether typed input is automatically classified as natural language rather than a command. Opt-in. |
| `agents.warp_agent.input.nld_in_terminal_enabled` | boolean | `true` | Whether that classification runs in the terminal input specifically. On by default here (upstream defaults it off) so that typing a non-English sentence in the terminal switches to AI input rather than being run as a command. |
| `agents.warp_agent.input.ai_command_denylist` | string | `""` | Commands to exclude from natural-language autodetection. |
| `agents.warp_agent.input.include_agent_commands_in_history` | boolean | `false` | Whether commands the agent ran appear in your shell history (up-arrow, Ctrl-R, the history menu). |
| `agents.warp_agent.input.show_agent_tips` | boolean | `true` | Whether agent tips appear in the input. |
| `agents.warp_agent.input.show_zero_state_hints` | boolean | `true` | Whether the Agent view's empty-state shortcut hints and message-bar hints are shown. Re-enable under Settings → Phosphor Agent → AI Input. |

### Suggestions

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.warp_agent.active_ai.intelligent_autosuggestions_enabled` | boolean | `true` | AI-powered command autosuggestions. |
| `agents.warp_agent.active_ai.agent_mode_query_suggestions_enabled` | boolean | `true` | Prompt suggestions in agent mode. (The key name is legacy — the feature was renamed, the key was not, so existing files keep working.) |
| `agents.warp_agent.active_ai.code_suggestions_enabled` | boolean | `true` | AI code suggestions. |
| `agents.warp_agent.active_ai.natural_language_autosuggestions_enabled` | boolean | `true` | Ghosted-text autosuggestions while typing an AI prompt. Schema-gated on the `PredictAMQueries` flag. |
| `agents.warp_agent.active_ai.git_operations_autogen_enabled` | boolean | `true` | Whether commit messages and PR titles/bodies are auto-generated in the code-review dialogs. |
| `agents.warp_agent.active_ai.rule_suggestions_enabled` | boolean | `true` | Whether the agent offers to save a rule after a response. Schema-gated on the `SuggestedRules` flag. |

### Agent permissions

These are what stand between the agent and your shell. Read them carefully.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.profiles.agent_mode_command_execution_allowlist` | array of regex strings | `["cat(\s.*)?", "echo(\s.*)?", "find .*", "grep(\s.*)?", "ls(\s.*)?", "which .*"]` | Commands the agent may run without asking. Each entry is a regex anchored to the whole command string. |
| `agents.profiles.agent_mode_command_execution_denylist` | array of regex strings | `bash`, `fish`, `pwsh`, `sh`, `zsh`, `curl`, `eval`, `exec`, `source`, `wget`, `dig`, `nslookup`, `host`, `ssh`, `scp`, `rsync`, `telnet`, `rm` — each with an optional-arguments suffix | Commands the agent must always ask about, even if something else would have allowed them. |
| `agents.profiles.agent_mode_execute_readonly_commands` | boolean | `false` | Whether the agent may auto-run commands it classifies as read-only without asking. |
| `agents.profiles.agent_mode_coding_permissions` | `"always_ask_before_reading"` \| `"always_allow_reading"` \| `"allow_reading_specific_files"` | `"always_ask_before_reading"` | How much file-reading the agent may do without asking. Note that granting read-only *command* execution above also implicitly grants file reads for coding tasks. |
| `agents.profiles.agent_mode_coding_file_read_allowlist` | array of absolute paths | `[]` | The specific files the agent may read when `agent_mode_coding_permissions` is `allow_reading_specific_files`. Store absolute paths. |
| `agents.warp_agent.other.auto_approve_bypasses_command_denylist` | boolean | `true` | Whether auto-approve (fast-forward) is allowed to run commands that match the denylist. Set to `false` to make the denylist absolute. |

### Conversation behaviour

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.warp_agent.other.thinking_display_mode` | `"show_and_collapse"` \| `"always_show"` \| `"never_show"` | `"show_and_collapse"` | How reasoning traces are displayed: shown while streaming then collapsed, kept expanded, or never shown. |
| `agents.warp_agent.other.orchestration_message_display_mode` | `"show_and_collapse"` \| `"always_show"` \| `"always_collapse"` | `"always_collapse"` | The same choice for child-agent message bodies during orchestration. |
| `agents.warp_agent.other.default_prompt_submission_mode` | `"interrupt"` \| `"queue"` | `"interrupt"` | What happens when you submit a prompt while the agent is still responding: cancel the in-flight response, or hold the prompt until it finishes. A conversation can override this. |
| `agents.warp_agent.other.long_running_command_submission_mode` | `"send_immediately"` \| `"queue_until_command_completes"` | `"queue_until_command_completes"` | What happens when you submit a prompt while the agent is driving a long-running command. Only consulted when the setting above is `interrupt`. |
| `agents.knowledge.rules_enabled` | boolean | `true` | Whether your saved rules are included in agent requests. |
| `agents.knowledge.warp_drive_context_enabled` | boolean | `true` | Whether Library content is included as context in AI requests. |
| `agents.warp_agent.other.show_conversation_history` | boolean | `true` | Whether conversation history appears in the tools panel. |
| `agents.warp_agent.other.show_agent_notifications` | boolean | `true` | Whether agent notifications appear (mailbox button, toasts, notification items). |
| `agents.warp_agent.appearance.hide_completed_tool_cards` | boolean | `false` | When on, tool cards that have finished (read files, grep, codebase search, requested commands) are hidden once complete. In-progress and errored cards are always shown. Useful for long sessions. |
| `agents.warp_agent.other.should_render_use_agent_toolbar_for_user_commands` | boolean | `true` | Whether the "Use Agent" footer appears under terminal commands you ran yourself. |

### BYOP providers and models

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.warp_agent.providers` | array of tables | `[]` | Your configured AI providers. Each entry carries `id`, `name`, `api_type` (TOML values `open_ai`, `open_ai_resp`, `gemini`, `anthropic`, `ollama`, `deep_seek`, `vertex`), `base_url`, a `models` list of `{name, id}` pairs, optional `extra_headers`, an optional provider-wide `token_price`, a `disabled` flag that hides the provider from the model picker without deleting it, and Vertex-only `vertex_project` / `vertex_location`. Each model entry may carry its own `token_price`, which overrides the provider-wide one; with no rate configured, `/cost` reports token counts only rather than guessing a price. **API keys are not stored here** — they go to the OS secure store. |
| `agents.warp_agent.catalog_provider_visibility_overrides` | array of strings | `[]` | Catalog provider ids whose default visibility in the "Quick add" chip row you have flipped. Membership hides an otherwise-common provider, or pins an otherwise-uncommon one visible. Does not configure the provider. |
| `agents.byop.last_used_model_id` | string | `""` | The last model you picked. New tabs and restarts hydrate from this, so a switch in the model picker carries over. Empty falls back to the profile default. This carry-over is a Phosphor-specific behaviour; upstream has no equivalent. |
| `agents.byop.last_used_reasoning` | table | `{}` | Per-`(api_type, model)` reasoning-effort memory, written when you switch in the picker. Rendered with nested values inline. |
| `agents.warp_agent.prompt_template_dir` | string (directory path) | `""` | A directory to hot-load agent system-prompt templates from, instead of the built-in ones compiled into the binary. Missing files and syntax errors fall back to the built-in version individually rather than failing. The `ZAP_PROMPT_DIR` environment variable overrides this setting. |

### BYOP conversation compaction

Compaction is how a long conversation is kept inside the model's context window.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.byop_compaction.auto` | boolean | `true` | Summarize automatically on context overflow. With this off, compaction only happens when you run `/compact` or `/compact-and`. |
| `agents.byop_compaction.prune` | boolean | `true` | Clear old tool output (replacing it with a placeholder) before each request. |
| `agents.byop_compaction.tail_turns` | integer | `2` | How many recent user turns to keep verbatim. Everything before that is fed to the summarizer. `0` disables compaction. |
| `agents.byop_compaction.preserve_recent_tokens` | integer | `0` | Override the recent-token preservation budget. `0` computes it automatically. |
| `agents.byop_compaction.reserved` | integer | `0` | Reserved buffer tokens subtracted from the input limit when checking for overflow. `0` computes it automatically. |
| `agents.byop_compaction.model.provider_id` | string | `""` | Use a dedicated provider for summarization calls. Empty uses the conversation's current model. |
| `agents.byop_compaction.model.model_id` | string | `""` | Use a dedicated model for summarization calls. Empty uses the conversation's current model. |

### Third-party CLI agents

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.third_party.should_render_cli_agent_toolbar` | boolean | `true` | Whether the CLI-agent footer appears for coding-agent commands. Independent of the "Use Agent" footer. |
| `agents.third_party.per_agent` | table of tables | `{}` | Per-agent visibility, keyed by the agent's serialized name. Each value has `toolbar` (default `true`), `tabmenu` (default `true`) and `titlebar` (default on for Claude, Codex, Gemini and Antigravity). Nested values rendered inline. |
| `agents.third_party.cli_agent_toolbar_enabled_commands` | table | `{}` | Maps a command regex to a specific CLI agent. An empty value means any agent. Nested values rendered inline. |
| `agents.third_party.auto_toggle_composer` | boolean | `true` | When a CLI-agent session has a plugin listener, Rich Input auto-closes when the agent needs direct keyboard interaction and auto-reopens when it does not. |
| `agents.third_party.auto_open_composer_on_cli_agent_start` | boolean | `false` | Whether Rich Input opens once automatically when a CLI-agent session starts. |
| `agents.third_party.auto_dismiss_composer_after_submit` | boolean | `false` | When there is **no** plugin listener, whether Rich Input closes after you submit. No effect when a listener is present. |
| `agents.third_party.submit_on_ctrl_enter` | boolean | `false` | When on, Rich Input submits on Ctrl+Enter and Enter inserts a newline. |

### MCP

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.mcp_servers.file_based_mcp_enabled` | boolean | `false` | Whether MCP servers configured by other AI tools (Claude, Codex, …) are detected and spawned. Phosphor's own `.warp/.mcp.json` files are always detected regardless of this setting. Desktop only. |

### Sessions

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `general.default_session_mode` | `"terminal"` \| `"agent"` \| `"ambient_agent"` \| `"tab_config"` \| `"docker_sandbox"` | `"terminal"` | What a new session starts as. `ambient_agent` is a cloud mode with no backend here. `docker_sandbox` needs the `LocalDockerSandbox` feature and falls back to `terminal` when it is off. |
| `general.default_tab_config_path` | string | `""` | The tab config file to open when `default_session_mode` is `tab_config`. Ignored otherwise. Machine-local. |

### Voice

Voice input records audio but cannot transcribe it in this build — see
[Not available in Phosphor](#not-available-in-phosphor). These settings are
listed for completeness.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.voice.voice_input_enabled` | boolean | `true` | Whether voice input is offered. Desktop only. Transcription is disabled in this build, so a recording always ends in a transcription failure. |
| `agents.voice.voice_input_toggle_key` | `"none"` \| `"fn"` \| `"alt_left"` \| `"alt_right"` \| `"control_left"` \| `"control_right"` \| `"super_left"` \| `"super_right"` \| `"shift_left"` \| `"shift_right"` | `"none"` | The physical key that toggles voice input. Desktop only. Same caveat as above. |

### AWS Bedrock

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `cloud_platform.third_party_api_keys.aws_bedrock_credentials_enabled` | boolean | `false` | Use your locally-configured AWS credentials for Bedrock requests. Desktop only. |
| `cloud_platform.third_party_api_keys.aws_bedrock_profile` | string | `"default"` | Which AWS profile to load credentials from. Desktop only. |
| `cloud_platform.third_party_api_keys.aws_bedrock_auto_login` | boolean | `false` | Run the refresh command automatically when Bedrock credentials expire, instead of asking first. Desktop only. |
| `cloud_platform.third_party_api_keys.aws_bedrock_auth_refresh_command` | string | `"aws login"` | The command run to refresh AWS credentials. Desktop only. |

### TUI statusline

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `agents.statusline` | table with `order` and `enabled` arrays | `order` = all 13 items; `enabled` = `["auto_approve", "model", "working_directory", "git_branch", "git_diff_status"]` | The order and visibility of the terminal UI's bottom statusline. Available items: `auto_approve`, `auto_queue`, `model`, `working_directory`, `git_branch`, `git_branch_status`, `git_diff_status`, `github_pull_request`, `context_window_usage`, `date`, `time_12_hour`, `time_24_hour`, `agent_todo_list`. `git_branch_status` supersedes `git_branch` when both are enabled. `github_pull_request` resolves through your local `gh` CLI. **TUI only.** Editable in the TUI with `/statusline`. |

### Upstream artifact

| TOML path | Type | Default | Status |
|---|---|---|---|
| `cloud_platform.third_party_api_keys.can_use_warp_credits_with_byok` | boolean | `false` | **Inert.** There are no credits in Phosphor — you pay your provider directly. The value is placed in an agent request context struct and never read. Leave it alone. |

### Internal state (no TOML key)

None of these is configuration; they are here so you can identify them if you
find them in `user_preferences.json` or the registry.

| Name | Type | Default | What it tracks |
|---|---|---|---|
| `EnteredAgentModeNumTimes` | integer | `0` | How many times you have entered agent mode. |
| `DismissedVoiceInputNewFeaturePopup` | boolean | `false` | Whether the voice-input feature popup was dismissed. Desktop only. |
| `ExplicitlyInteractedWithVoice` | boolean | `false` | Whether you have ever used voice input, used to decide whether to show a one-time toast. Desktop only. |
| `HasShownAgentModeProfileCommandAutoexecutionSpeedbump` | boolean | `false` | Whether the profile-level auto-execution warning has been shown. |
| `ShouldShowAgentModeModelExecuteReadonlyCommandsSpeedbump` | boolean | `true` | Whether to show the read-only-auto-execute warning. |
| `ShouldShowAgentModeWriteToPtySpeedbump` | boolean | `true` | Whether to show the write-to-terminal warning. |
| `ShouldShowAgentModeCodingReadPermissionsNudge` | boolean | `true` | Whether to show the auto-read-files warning. |
| `ShouldShowAgentModeAskUserQuestionSpeedbump` | boolean | `true` | Whether to show the one-time warning on Ask-User-Question cards. |
| `AwsBedrockLoginBannerDismissed` | boolean | `false` | Whether the Bedrock login banner was permanently dismissed. Desktop only. |
| `AgentModeSetupBannerShownForRepoPaths` | array of paths | `[]` | Repos whose agent-mode setup banner has already been shown. |
| `AIRequestQuotaInfoSetting` | table | empty | Upstream quota/usage bookkeeping. No quota exists here. |
| `ShouldShowCodeSuggestionSpeedbump` | boolean | `true` | Whether to show the code-suggestion-banner warning. |
| `MCPExecutionPath` | optional string | none | Cached MCP execution path. |
| `DidShowAgents3LaunchModal` | boolean | `false` | One-time launch-modal flag. Upstream promotional modal; not shown here. |
| `DidDismissAgentManagementHelpPage` | boolean | `false` | Upstream paid-plan help-page dismissal. No paid plans here. |
| `FtuModelCalloutDismissed` | boolean | `false` | Whether the first-time model-picker callout has been shown. Despite the name it means "shown", not "dismissed". |
| `HasAutoOpenedConversationList` | boolean | `false` | Whether the one-time conversation-list auto-open has happened. |
| `AmbientAgentTrialWidgetDismissed` | boolean | `false` | Upstream trial-widget dismissal. No trials here. |
| `SeenFeatureIntroIds` | table of string→boolean | `{}` | Which one-time feature-intro popovers you have seen. |
| `PluginInstallChipDismissedMap` | table of string→boolean | `{}` | Per-agent, per-host dismissal of the plugin-install chip. Desktop only. |
| `PluginUpdateChipDismissedForVersionMap` | table of string→string | `{}` | Per-agent, per-host dismissal of the plugin-update chip, keyed by version. Desktop only. |
| `CLIAgentScanCompleted` | boolean | `false` | Whether a CLI-agent installation scan has completed. Triggers the first automatic sync. |

## AliasExpansionSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `terminal.input.alias_expansion_enabled` | boolean | `false` | Whether shell aliases are expanded inline in the input as you type. |

## AppEditorSettings

Cursor, Vim mode, and input-editing behaviour.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.cursor.cursor_blink` | `"enabled"` \| `"disabled"` | `"enabled"` | Whether the cursor blinks. |
| `appearance.cursor.cursor_display_type` | `"bar"` \| `"block"` \| `"underline"` | `"bar"` | The cursor's shape. |
| `text_editing.vim_mode_enabled` | boolean | `false` | Vim keybindings in the input. Also applies in the terminal UI, where `/vim-mode` toggles it. |
| `text_editing.vim_unnamed_system_clipboard` | boolean | `false` | Whether Vim's unnamed register is the system clipboard. |
| `text_editing.vim_status_bar` | boolean | `true` | Whether the Vim status bar is displayed. |
| `text_editing.code_editor_line_number_mode` | `"absolute"` \| `"relative"` | `"absolute"` | Line numbering style in code editors. |
| `text_editing.autocomplete_symbols` | boolean | `true` | Auto-close brackets and quotes. |
| `terminal.input.autosuggestions.enabled` | boolean | `true` | Whether command autosuggestions are shown. |
| `terminal.input.autosuggestions.keybinding_hint` | boolean | `true` | Whether the keybinding hint is shown next to an autosuggestion. |
| `terminal.input.autosuggestions.show_ignore_button` | boolean | `false` | Whether an ignore button is shown on autosuggestions. |

## AppIconSettings

**macOS only.**

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.icon.app_icon` | see below | `"default"` | The dock icon. Values: `default`, `aurora`, `classic_1`, `classic_2`, `classic_3`, `comets`, `cow`, `glass_sky`, `glitch`, `glow`, `holographic`, `mono`, `neon`, `original`, `starburst`, `sticker`, `warp_one` (displayed as "Phosphor 1"). |

## AutoupdateSettings

**Desktop.** See the caveat below the table.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `updates.automatic_updates_enabled` | boolean | `true` | Whether Phosphor checks for and downloads updates in the background. **Live on macOS and Windows** (both release bundlers compile the `autoupdate` feature in). **Inert on Linux**, where the bundler does not, so the `true` default gates a check that never runs. |

**Caveat.** This is the *GUI* updater, and whether it is compiled in is decided
per platform by the release bundler, not by `app/Cargo.toml`. The `autoupdate`
Cargo feature is **not** in the default feature set, and `FeatureFlag::Autoupdate`
was deliberately dropped from `RELEASE_FLAGS`; the flag is set only by
`extra_flags` under `#[cfg(feature = "autoupdate")]`. The macOS bundler
(`script/macos/bundle:358`) and the Windows bundler
(`script/windows/bundle.ps1:125`) both pass that feature, so shipped macOS and
Windows builds do poll GitHub Releases and this setting gates the poll. The
Linux OSS bundler resets the feature list to `release_bundle`
(`script/linux/bundle:198-203`), so on Linux nothing polls, the two update
commands are never registered, and this setting has nothing to gate. A local
`cargo build` without `--features autoupdate` behaves like the Linux build on
every platform. Do not confuse this with `general.autoupdate_enabled`, which is
the terminal UI's separate updater.

## BlockVisibilitySettings

Visibility of blocks Phosphor generates itself, rather than blocks you created
by running a command. All three default to hidden.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.blocks.should_show_bootstrap_block` | boolean | `false` | Show the shell-bootstrap block. |
| `appearance.blocks.should_show_in_band_command_blocks` | boolean | `false` | Show in-band command blocks (commands Phosphor issues to your shell to power completions and highlighting). |
| `appearance.blocks.should_show_ssh_block` | boolean | `false` | Show the SSH connection block. |

## CodeSettings

The code editor, project explorer, and codebase indexing.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `code.editor.use_warp_as_default_editor` | boolean | `false` | Register Phosphor as the system default code editor. |
| `code.editor.format_on_save` | boolean | `true` | Whether the language server reformats a file on save. Other LSP features (hover, go-to-definition, references, diagnostics) are unaffected by this setting. |
| `code.editor.show_project_explorer` | boolean | `true` | Show the project explorer / file tree in the tools panel. |
| `code.editor.show_global_search` | boolean | `true` | Show global file search in the tools panel. |
| `code.editor.show_hidden_files` | boolean | `true` | Show dotfiles in the project explorer. |
| `code.indexing.agent_mode_codebase_context` | boolean | `false` | Whether the agent may use the codebase embedding index as context. **Unreachable in a stock build:** both this row and the one below only render when `FeatureFlag::FullSourceCodeEmbedding` is on, whose sole enable path is `ZAP_UNSTABLE_FEATURES=full_source_code_embedding` (`app/src/settings_view/code_page.rs:1503-1510`), and nothing indexes without it whatever the setting says. **Desktop only, and off by default here on purpose** — upstream defaults it on because indexing ran on Warp's servers, whereas here it spends your own embedding-provider quota and, on the remote surface, sends your provider API key to whichever host you installed the daemon on. Turning this on *is* the consent step. See §5 “Codebase search”. |
| `code.indexing.agent_mode_codebase_context_auto_indexing` | boolean | `false` | Whether repositories are indexed automatically as you open them, rather than only on explicit request. Desktop only. |

### Internal state (no TOML key)

| Name | Type | Default | What it tracks |
|---|---|---|---|
| `DismissedCodeToolbeltNewFeaturePopup` | boolean | `false` | Whether the code toolbelt feature popup was dismissed. |

## DebugSettings

**Internal — developer diagnostics.** None of these has a TOML key; all are
private. They exist for debugging Phosphor itself and are not part of the
supported configuration surface. Do not rely on them.

| Name | Type | Default | What it does |
|---|---|---|---|
| `IsShellDebugModeEnabled` | boolean | `false` | Sets `WARP_SHELL_DEBUG_MODE` in newly spawned sessions. |
| `AreInBandGeneratorsForAllSessionsEnabled` | boolean | `false` | Forces in-band generators (used for completions and syntax highlighting) in all new sessions. |
| `ForceDisableInBandGenerators` (native key `DisableInBandCommands`) | boolean | `false` | Kill switch: never use in-band generators in any new session. Takes precedence over the setting above. Sessions with no alternative (e.g. remote non-SSH subshells) lose completions and highlighting entirely. |
| `RecordingModeEnabled` | boolean | on if built with the `recording_mode` feature, otherwise `false` | Recording mode; also toggled from the macOS App → Debug menu. |
| `ShowMemoryStats` | boolean | `true` | Show memory statistics. Only actually rendered in dogfood builds and never in tests. |

## EmacsBindingsSettings

**Linux only. Internal — no TOML key.** Not really a setting: a record of a user
action, persisted so a banner is not shown twice.

| Name | Type | Default | What it tracks |
|---|---|---|---|
| `EmacsBindingsBannerState` | `not_dismissed` \| dismissed states | `not_dismissed` | Whether you have already been offered Emacs bindings. |

## FontSettings

Fonts and text metrics. **These apply to the GUI only** — the terminal UI
renders in your host terminal's cells and uses that terminal's font.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.text.font_name` | string | `"Hack"` | The monospace font used in the terminal. |
| `appearance.text.fallback_font_name` | string | `""` | The font used when the terminal font cannot render a character. Empty means no explicit fallback. |
| `appearance.text.font_size` | float | `13.0` | Monospace font size. `13.0` on **every** platform. The source contains a rule that would start a new Windows user at `16.0` to match Windows Terminal (`app/src/settings/initializer.rs:80-105`), but it sits inside the `auth_state.is_onboarded() == Some(false)` block, and the local placeholder user hardcodes `is_onboarded: true` (`app/src/auth/mod.rs:213`), so that branch never runs. |
| `appearance.text.font_weight` | `"thin"` \| `"extra_light"` \| `"light"` \| `"normal"` \| `"medium"` \| `"semibold"` \| `"bold"` \| `"extra_bold"` \| `"black"` | `"normal"` | Monospace font weight. |
| `appearance.text.line_height_ratio` | float | `1.2` | Line height as a multiple of font size. |
| `appearance.text.ai_font_name` | string | `"Hack"` | The font used for AI-generated content. |
| `appearance.text.match_ai_font` | boolean | `false` | Make the AI font follow the terminal font automatically. |
| `appearance.text.notebook_font_size` | float | `14.0` | Font size in notebooks. Clamped to 5–25 when adjusted with the increase/decrease commands. |
| `appearance.text.match_notebook_to_monospace_font_size` | boolean | `true` | Make the notebook font size follow the terminal font size. |
| `appearance.text.ui_font_name` | string | `""` | The font used for UI chrome. Empty means the built-in UI font. |
| `appearance.text.ui_font_size` | float | `12.0` | Base UI font size. |
| `appearance.text.enforce_minimum_contrast` | `"never"` \| `"only_named_colors"` \| `"always"` | `"only_named_colors"` | Whether Phosphor adjusts foreground colours to keep text readable. `only_named_colors` adjusts only when the foreground was specified with a named/default colour. |
| `appearance.text.use_thin_strokes` | `"never"` \| `"on_low_dpi_displays"` \| `"on_high_dpi_displays"` \| `"always"` | `"on_high_dpi_displays"` | Thin glyph strokes. **macOS only.** |
| `appearance.text.markdown_heading_h1_scale` | float | `2.0` | Size multiplier for Markdown `#` headings. Valid range 0.1–5.0. |
| `appearance.text.markdown_heading_h2_scale` | float | `1.5` | Size multiplier for `##` headings. |
| `appearance.text.markdown_heading_h3_scale` | float | `1.17` | Size multiplier for `###` headings. |
| `appearance.text.markdown_heading_h4_scale` | float | `1.0` | Size multiplier for `####` headings. |
| `appearance.text.markdown_heading_h5_scale` | float | `0.83` | Size multiplier for `#####` headings. |
| `appearance.text.markdown_heading_h6_scale` | float | `0.75` | Size multiplier for `######` headings. |

## GPUSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `system.prefer_low_power_gpu` | boolean | `true` on Linux/FreeBSD and on Windows; `false` on macOS | Prefer the integrated (low-power) GPU. On Windows the default flips to `false` if the build enables the high-performance-GPU default, which is not on in stock builds. |
| `system.preferred_graphics_backend` | `"empty"` \| `"dx12"` \| `"vulkan"` \| `"gl"` \| unset | unset | Force a graphics backend. **Windows only.** |

## InputModeSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.input.input_mode` | `"pinned_to_bottom"` \| `"pinned_to_top"` \| `"waterfall"` | `"pinned_to_bottom"` | Where the input sits. `pinned_to_bottom` puts newest blocks at the bottom; `pinned_to_top` inverts that; `waterfall` starts the input at the top and pushes it down as commands accumulate. |

Note: the source carries a comment (`app/src/settings/input_mode.rs:9-11`)
saying new users are defaulted to `waterfall` by the settings initializer. The
comment is stale — no such override exists anywhere in this tree, and the
`DefaultWaterfallMode` feature flag it belonged to is registered but has no
reader. The effective default really is `pinned_to_bottom`, for everyone.

## InputSettings

The terminal input box: completions, highlighting, corrections, and the input's
own chrome.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `terminal.input.show_hint_text` | boolean | `true` | Show hint text in an empty input. |
| `terminal.input.input_box_type_setting` | `"classic"` \| `"universal"` | `"classic"` | The input style. `classic` is terminal-first; `universal` is AI-first. |
| `terminal.input.classic_completions_mode` | boolean | `false` | Use the classic completions behaviour. |
| `terminal.input.completions_open_while_typing` | boolean | `false` | Open the completions menu automatically as you type rather than on demand. |
| `terminal.input.error_underlining_enabled` | boolean | `true` | Underline commands that will not run. Desktop only. |
| `terminal.input.syntax_highlighting` | boolean | `true` | Syntax-highlight the input. Desktop only. |
| `terminal.input.command_corrections` | boolean | `true` | Suggest corrections for mistyped commands. |
| `terminal.input.at_context_menu_in_terminal_mode` | boolean | `true` | Whether the `@` context menu is available in terminal mode. |
| `terminal.input.outline_codebase_symbols_for_at_context_menu` | boolean | `true` | Whether codebase symbols appear in the `@` menu. |
| `terminal.input.enable_slash_commands_in_terminal` | boolean | `true` | Whether `/` slash commands are available in the terminal input. |
| `terminal.input.enable_ai_command_search_hash_trigger` | boolean | `true` | Whether typing `#` at the start of the input opens AI Command Search. |
| `terminal.input.show_terminal_input_message_bar` | boolean | `true` | Whether the contextual hint bar under the terminal input is shown. Only applicable when the Agent view is enabled. |

### Internal state (no TOML key)

| Name | Type | Default | What it tracks |
|---|---|---|---|
| `WorkflowsBoxExpanded` (native key `WorkflowsBoxOpen`) | boolean | `true` | Whether the workflows box is expanded. |
| `AutosuggestionAcceptedCount` | small integer | `0` | How many autosuggestions you have accepted. |
| `CompletionsMenuWidth` | float | `330.0` | Drag-to-resize width of the completions menu. |
| `CompletionsMenuHeight` | float | `185.0` | Drag-to-resize height of the completions menu. |
| `InlineMenuCustomContentHeights` | table of menu→float | `{}` | Per-menu drag-to-resize heights. |

## LanguageSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.language` | `"system"` \| `"english"` \| `"simplified_chinese"` \| `"japanese"` | `"system"` | The UI language. `system` follows your OS locale and falls back to English if it is not one of the supported languages. Untranslated strings fall back to English. **Takes full effect only after a restart** — already-rendered text is not re-laid-out. This setting also selects the language the agent is asked to answer in. |

## LinuxAppConfiguration

**Linux / FreeBSD only.** On other platforms this key is reported as unknown.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `system.force_x11` | boolean | `true` under WSL, `false` otherwise | Force X11 instead of Wayland. |

## LocalControlSettings

The opt-in for the `warpctrl` local-control stack — scripting Phosphor from
outside the app. Shown in Settings → Scripting when the `WarpControlCli` feature
is on.

| Name | Type | Default | Where it is stored |
|---|---|---|---|
| `LocalControlModeSetting` (key `LocalControlMode`) | `"disabled"` \| `"enabled"` | `"disabled"` in the public build | **Not in `settings.toml` and not in the normal private store.** It is written to the OS secure store (Keychain / Credential Manager / Secret Service), using an owner-only-readable write path, because it gates a local automation surface. Desktop only. Set it from Settings → Scripting. |

Internal dogfood builds default this to `enabled`; the build you download
defaults to `disabled`.

## MigrationTestSettings

**Internal — test fixture.** This group exists only inside Phosphor's own test
suite, to verify that the one-time migration from the native store to
`settings.toml` copies public settings and not private ones. It is not
registered in the running application and its keys will never appear in your
file. Listed here only so the reference is complete.

| TOML path | Type | Default |
|---|---|---|
| `migration_test.public_setting` | boolean | `false` |
| `migration_test.public_string_setting` | string | `""` |
| `PrivateSetting` (internal — no TOML key) | boolean | `false` |

## NativePreferenceSettings

**Web build only.** Inert in every desktop build you can download; the key will
be reported as unknown.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `general.user_native_preference` | `"not_selected"` \| `"web"` \| `"desktop"` | `"not_selected"` | Whether to prefer the native desktop app or the web app. |
| `UserNativePreferenceDialogDismissed` (internal — no TOML key) | boolean | `false` | Whether the preference dialog was dismissed. |

## NetworkSettings

**Desktop.** A global HTTP/WebSocket proxy applied to every outbound request
Phosphor makes — provider calls, updates, MCP OAuth, everything. Configured in
Settings → Network.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `network.proxy_mode` | `"off"` \| `"system"` \| `"custom"` | `"off"` | `off` disables proxying entirely, *including* environment variables like `HTTP_PROXY` — this is the default deliberately, so an unexpected system proxy cannot intercept local calls. `system` follows the system/environment configuration. `custom` uses the URL below. |
| `network.proxy_url` | string | `""` | The proxy URL used in `custom` mode, e.g. `http://proxy.corp:8080`. |
| `network.proxy_username` | string | `""` | The proxy username for `custom` mode. Empty means no basic auth. |
| `network.proxy_no_proxy` | string | `""` | Comma-separated host exceptions, e.g. `localhost,127.0.0.1,.internal`. |

The proxy **password is not in this file.** It goes to the OS secure store,
managed through the Network settings page, the same way provider API keys are.

## NotificationsMigrationTestSettings

**Internal — test fixture.** Like `MigrationTestSettings`, this group exists only
in Phosphor's test suite. It pins the behaviour of migrating a
`NotificationsSettings` value out of the native store, checking that a value in
the old serde format is rejected rather than silently defaulted. Not registered
in the running application.

| TOML path | Type | Default | Notes |
|---|---|---|---|
| `migration_test.notifications` | table | the `NotificationsSettings` default | Written with nested values inline. |

## PaneSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.panes.should_dim_inactive_panes` | boolean | `false` | Dim panes that do not have focus. |
| `appearance.panes.focus_pane_on_hover` | boolean | `false` | Focus a pane when the mouse moves over it, without clicking. |

## PreferencesSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `account.is_settings_sync_enabled` | boolean | `false` | **Largely inert.** Upstream this enabled cloud settings sync. Phosphor has no cloud and no syncer — nothing is uploaded whether this is on or off. Its only remaining effect is cosmetic: when it is `true`, the Settings window draws a small "this setting is local only" icon next to settings that upstream would not have synced. Leave it at `false`. |

## SameLinePromptBlockSettings

**Internal — no TOML key. Inert in Phosphor.** A record of whether an onboarding
block has been shown, not a preference — and the onboarding block it tracked was
never ported. The group is registered (`app/src/settings/init.rs:108`) and
nothing else in the tree reads or writes it, so the value stays `not_shown`
forever. Listed only so you can identify the name.

| Name | Type | Default | What it tracks |
|---|---|---|---|
| `SameLinePromptBlockState` | `not_shown` \| `shown` \| `do_not_show` | `not_shown` | Upstream: whether the same-line-prompt onboarding block has been shown, with `do_not_show` for "not applicable" (e.g. you are not using PS1). Here: never written. |

## ScrollSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `general.mouse_scroll_multiplier` | float | `3.0` | Scroll-speed multiplier for mouse wheel events. |

## SelectionSettings

Selecting and pasting with the mouse.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `terminal.copy_on_select` | boolean | `true` | Copy to the clipboard as soon as text is selected. |
| `terminal.input.right_click_behavior` | `"context_menu"` \| `"paste"` | `"context_menu"` | What a bare right-click does. With `paste`, Shift+right-click opens the context menu instead. |
| `system.linux_selection_clipboard` | boolean | `true` | Honour the X11/Wayland primary selection clipboard. **Linux/FreeBSD only.** |
| `terminal.input.middle_click_paste_enabled` | boolean | `true` | Middle-click pastes from the regular clipboard. **Windows and macOS only** — on Linux, middle-click is mapped to the primary selection clipboard and controlled by the setting above. |

## SshSettings

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `warpify.ssh.enable_legacy_ssh_wrapper` | boolean | `true` | Whether the legacy SSH wrapper is used for SSH sessions. Phosphor deliberately keeps this wrapper; upstream's deprecation of it was not adopted. |
| `warpify.ssh.reuse_existing_control_master` | boolean | `false` | Whether the wrapper attaches to an existing SSH `ControlMaster` for the destination host instead of always opening its own connection. |

## ThemeSettings

The GUI colour theme. **These do not affect the terminal UI**, which resolves its
own light/dark theme at startup — see `TuiThemeSettings` below, and mind how
similar the two keys look: `[appearance.themes] theme` is this one,
`[appearance] theme` is the TUI's.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.themes.theme` | theme name string, or a table for a custom theme | `"phosphor_amber"` | The colour theme. Built-in names include `phosphor_amber`, `phosphor_green`, `adeberry`, `phenomenon`, `dark`, `light`, `dracula`, `fancy_dracula`, `tokyo_night`, `one_dark`, `cyber_wave`, `solar_flare`, `solarized_dark`, `solarized_light`, `willow_dream`, `dark_city`, `pink_city`, `gruvbox_dark`, `gruvbox_light`, `red_rock`, `jelly_fish`, `leafy`, `wez_term_classic`, `vs_code_2026_dark`, `koi`, `snowy`, `marble`. Custom themes are referenced as a table. Written as an inline table when it is not a plain name. |
| `appearance.themes.system_theme` | boolean | `false` | Follow the system light/dark setting instead of the fixed theme above. |
| `appearance.themes.selected_system_themes` | table with `light` and `dark` | `{ light = "light", dark = "dark" }` | Which theme to use in each system mode when `system_theme` is on. Written as an inline table. |

A custom theme is only portable between machines if its file path can be stored
relative to the themes directory; an absolute path to a local file is not.

## TuiAutoupdateSettings

**Desktop. Terminal UI only.**

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `general.autoupdate_enabled` | boolean | `true` | Whether the terminal UI installs updates in the background. Read once at TUI startup. `WARP_TUI_DISABLE_AUTOUPDATE` also disables updates for a single launch. |

**Caveat.** Although this defaults to `true`, the shipped TUI never runs the
updater. Eligibility is decided once at startup
(`crates/warp_tui/src/autoupdate.rs:282-308`) and the check that stops it is
**"not running from a managed install"**: background updates only run for a
binary sitting inside a `versions/<version>/` tree laid down by an install
script, and Phosphor publishes the TUI as a plain tarball. Two further guards sit
downstream and are never reached — the `oss` channel has no TUI release artifacts
(`:695-704`) and `releases_base_url` is empty (`:660-663`). The setting is
functional in the sense that it is read; there is nothing behind it to gate. This
is a *different* updater from `updates.automatic_updates_enabled`.

## TuiThemeSettings

**Terminal UI only.** The GUI ignores this key.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.theme` | `"auto"` \| `"light"` \| `"dark"` | `"auto"` | The terminal UI's colour theme. `auto` follows the host terminal's background luminance. Changeable in the TUI with `/theme <auto\|light\|dark>`. |

## TuiZeroStateSettings

**Desktop. Terminal UI only.** The empty/idle screen the TUI shows before you
start a conversation.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `appearance.zero_state.object` | inline table | `{ type = "built_in" }` | The rotating object. Either `{ type = "built_in" }` or `{ type = "ascii_file", path = "…" }` with a path relative to the TUI settings directory. Changing the setting reloads the object; editing the linked file requires a restart. Written as an inline table. |
| `appearance.zero_state.rotation_period_seconds` | float | `5.0` | Seconds per rotation. Valid range 1–60; a value outside it is rejected and the default is used. |
| `appearance.zero_state.extrusion_depth` | float | `0.18` | Normalized half-depth of the extruded object. Valid range 0.02–0.5. |
| `appearance.zero_state.show_animation` | boolean | `true` | Show the rotating object and its starfield. |
| `appearance.zero_state.freeze_animation_when_unfocused` | boolean | `false` | Stop repainting the animation while the terminal is not focused. |
| `appearance.zero_state.show_changelog` | boolean | `true` | Show the "What's new" section. |
| `appearance.zero_state.show_project_info` | boolean | `true` | Show the project path and the rules and skills discovered for it. |
| `appearance.zero_state.show_mcp` | boolean | `true` | Show the MCP section. |

The title and version lines are always shown and have no toggle. Upstream also
has a `show_signed_in_user` toggle for an account line; there are no accounts
here, so it is not present.

## UserAppInstallDetectionSettings

**Web build only. Internal — no TOML key.** Inert in every desktop build.

| Name | Type | Default | What it tracks |
|---|---|---|---|
| `UserAppInstallationDetected` (key `UserAppInstallStatus`) | `not_detected` \| `detected` | `not_detected` | Whether the web app has detected a desktop installation. |

## VimBannerSettings

**Internal — no TOML key.** A record of a dismissal, not a preference.

| Name | Type | Default | What it tracks |
|---|---|---|---|
| `VimKeybindingsBannerState` | `not_dismissed` \| dismissed states | `not_dismissed` | Whether the Vim keybindings banner was dismissed. |

## WarpDrivePrivacySettings

The two privacy toggles. Both default **off** here; upstream defaults both on,
because upstream is a commercial product with an opt-out model. Phosphor
physically removed the outbound channels, so leaving them on would show "ON"
next to something that transmits nothing.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `privacy.telemetry_enabled` | boolean | `false` | **No effect.** The telemetry channel was removed; nothing is ever sent and the toggle is not even rendered in the Settings window. The setting is retained so the control reappears automatically if a telemetry channel is ever added. |
| `privacy.crash_reporting_enabled` | boolean | `false` | **Inert in every shipped build, on every platform.** Nothing is uploaded — there is no crash-report endpoint — and the local panic hook it would install (`app/src/crash_reporting/mod.rs:275`) is not compiled in: `crash_reporting::init` sits behind `#[cfg(feature = "crash_reporting")]` (`app/src/lib.rs:1551-1557`), that cargo feature is not in `app/Cargo.toml`'s default list, and every bundler's `oss` branch resets `FEATURES` without it (`script/macos/bundle:358`, `script/linux/bundle:203`, `script/windows/bundle.ps1:125` — each script's *default* `FEATURES` line does mention `crash_reporting`, but that is the dev value and `oss` overwrites it). The Settings → Privacy toggle nonetheless renders, because `should_render` gates on `FeatureFlag::CrashReporting` (`app/src/settings_view/privacy_page.rs:1546`), which is in `RELEASE_FLAGS` (`crates/warp_features/src/lib.rs:912`, `app/src/lib.rs:2906-2907`). Build with `--features crash_reporting` and the setting becomes live. Independently of all this, plain panic backtraces go to the log on every platform via `log_panics` (`crates/warp_logging/src/native.rs:804-807`), with no setting involved. |

### Secret redaction (`SafeModeSettings`, `PrivacySettings`)

Secret redaction lives in two other groups, but its keys land in the same
`[privacy]` section of your file, so they are listed here.

**Redaction is off by default.** `privacy.secret_redaction.enabled` is the master
switch and defaults to `false`; adding patterns to
`privacy.custom_secret_regex_list` does nothing until you also turn that on. All
four are configured in Settings → Privacy.

| TOML path | Type | Default | What it does |
|---|---|---|---|
| `privacy.secret_redaction.enabled` | boolean | `false` | Master switch. Detects secrets in terminal output and obscures them, and blocks saving an MCP server config that contains one. Group `SafeModeSettings`. |
| `privacy.secret_redaction.secret_display_mode_setting` | `"asterisks"` \| `"strikethrough"` \| `"always_show"` | `"strikethrough"` | How a detected secret is drawn. `always_show` still detects and redacts it, but applies no visual treatment. Group `SafeModeSettings`. |
| `privacy.secret_redaction.hide_secrets_in_block_list` | boolean | `false` | **Legacy.** Superseded by `secret_display_mode_setting` and kept only for backward compatibility: it is consulted *only* when the display mode is still at its `strikethrough` default, in which case `true` upgrades it to `asterisks`. Setting the display mode explicitly makes this key inert. Group `SafeModeSettings`. |
| `privacy.custom_secret_regex_list` | array of tables | `[]` | Your own regex patterns for detecting and redacting secrets before they reach a model. Group `PrivacySettings`. |
| `HasInitializedDefaultSecretRegexes` (internal — no TOML key) | boolean | `false` | Whether the built-in default secret-redaction patterns have been seeded into your list. New users start without them, by design — adding them is an explicit action. |

---

## Settings that exist but do nothing

Collected in one place, because finding one of these in the file and assuming it
works is the most likely way to waste an afternoon.

| Key | Why it is inert |
|---|---|
| `account.is_settings_sync_enabled` | No cloud, no syncer. Only draws a "local only" icon. |
| `privacy.telemetry_enabled` | Telemetry channel physically removed; toggle never rendered. |
| `privacy.crash_reporting_enabled` | The toggle renders, but `crash_reporting::init` is compiled out of every OSS bundle (`app/src/lib.rs:1551-1557`; no `oss` bundler branch sets the cargo feature). Live only in a `--features crash_reporting` build. |
| `cloud_platform.third_party_api_keys.can_use_warp_credits_with_byok` | There are no credits. The value is computed and never read. |
| `updates.automatic_updates_enabled` | **Inert on Linux only.** The macOS and Windows bundlers add the `autoupdate` Cargo feature explicitly (`script/macos/bundle:358`, `script/windows/bundle.ps1:125`), so the setting is live there. The Linux OSS bundler resets the feature list to `release_bundle` (`script/linux/bundle:198-203`), so nothing polls. Note `FeatureFlag::Autoupdate` is **not** in `RELEASE_FLAGS` (`crates/warp_features/src/lib.rs:896` says so explicitly); it is set only by `extra_flags` under `#[cfg(feature = "autoupdate")]`. See §AutoupdateSettings. |
| `general.autoupdate_enabled` | The shipped TUI configures no update endpoint. |
| `agents.voice.voice_input_enabled`, `agents.voice.voice_input_toggle_key` | Audio capture works; transcription is disabled because the BYOP protocol cannot carry audio. Recording always ends in a transcription failure. |
| `general.default_session_mode = "ambient_agent"` | Ambient (cloud) agents do not exist here. |
| `general.user_native_preference`, `UserAppInstallStatus` | Web-build-only settings; inert in every downloadable build. |
| `migration_test.*` | Test-fixture keys. Never registered in the running app. |
| `SameLinePromptBlockState` (internal) | Registered, never read or written. The onboarding block it tracked was not ported. |
| `accessibility.accessibility_verbosity` **on Linux, FreeBSD and Windows** | Read, but the winit delegate's `set_accessibility_contents` is an empty function; only the macOS delegate forwards announcements. Live on macOS. |
| `experimental.async_find_enabled` | Opt-in for async find on channels where `FeatureFlag::AsyncFind` is off. The flag is in this build's default feature list, so `is_async_find_enabled()` already returns `true` and the setting changes nothing. |

---

## Not available in Phosphor

Settings a Warp user would go looking for and not find. Each is a deliberate
decision recorded in `DECLINED.md`, not an oversight.

- **Cloud settings sync.** There is no Warp account, no backend, and no settings
  syncer. Your `settings.toml` is local, full stop. The `sync_to_cloud` field on
  every setting in the source is an upstream artifact with no runtime effect
  here.
- **Per-surface settings (`SettingSurfaces` / `SettingsMode`).** Upstream tags
  each setting as GUI-only or TUI-only and keeps separate GUI and TUI settings
  files. Phosphor dropped that: one app identity, one config directory, one
  `settings.toml` shared by both surfaces. There is no TUI-only override — a
  shared key changed in either surface changes both. The tables above mark which
  keys each surface actually reads.
- **Agent commit/PR attribution (`agent_attribution_enabled`).** The setting and
  its widget were removed. Upstream never read it client-side; the *server*
  decided whether attribution instructions entered the prompt. With no server
  and no local attribution emitter, restoring the toggle would add a switch that
  changes nothing.
- **Team / organization policy settings.** No teams exist; the org command
  denylist, workspace AI-autonomy overrides, and admin enablement settings have
  no source of policy. `code.indexing.agent_mode_codebase_context` here is your
  setting alone, with no org override above it.
- **Billing, credits, and quota settings.** No paid tiers, no credits, no quota.
- **A `voice_input_language` setting.** It configured a cloud transcription
  backend that cannot run here.
- **Screen-recording settings.** The subsystem was never ported.
- **A "Signed in as…" toggle in the TUI zero state.** No accounts.
- **Custom keybindings in the terminal UI.** `keybindings.yaml` is read by the
  GUI only; the TUI's bindings are the ones registered in the TUI process.
- **Font settings for the terminal UI.** The TUI renders in your host terminal's
  cells and uses that terminal's font. Everything under `appearance.text` is
  GUI-only.

<!-- SOURCES
Settings macro and field semantics:
  crates/settings/src/macros.rs:196-232 (define_setting! arms: name/type/default/supported_platforms/group/storage_key/sync_to_cloud/private/toml_path/max_table_depth/description/feature_flag)
  crates/settings/src/macros.rs:518-560 (implement_setting_for_enum!; enum settings take Default::default())
  crates/settings/src/macros.rs:425-510 (maybe_define_setting!)
  crates/settings/src/lib.rs:240-330 (Setting trait: toml_path, toml_key, hierarchy, max_table_depth, supported_platforms, sync_to_cloud, is_private)
  crates/settings/src/lib.rs:161-169 (SupportedPlatforms variants)
  crates/settings/src/lib.rs:171-193 (SyncToCloud / RespectUserSyncSetting)
  crates/settings/src/lib.rs:197-222 (matches_current_platform; DESKTOP = non-wasm, WEB = wasm)
  crates/settings/src/lib.rs:294-300 (max_table_depth: Some(0) = inline table)
  crates/settings/src/lib.rs:311-314 (is_private => platform-native store, never in TOML)
  crates/settings/src/schema.rs:26-31 (feature_flag gates schema inclusion only)
  crates/settings_value/src/lib.rs:42-62 (derive converts enum variants to snake_case; AgentModeCommandExecutionPredicate serializes as a plain regex string)

Settings file location, format, backends:
  app/src/settings/mod.rs:648-653 (user_preferences_toml_file_path = config_local_dir()/settings.toml)
  app/src/settings/mod.rs:643-645 (user_preferences_file_path = config_local_dir()/user_preferences.json)
  crates/warp_core/src/paths.rs:132-158 (data_dir / config_local_dir; macOS uses home/macos_config_dir_name())
  crates/warp_core/src/paths.rs:113-126 (macos_config_dir_name_for: Channel::Oss => ".phosphor")
  crates/warp_core/src/paths.rs:298-341 (project_dirs_for_app_id; Linux "Phosphor" => "phosphor")
  crates/warp_core/src/paths_tests.rs:236-260 (pins dev.phosphor.Phosphor / phosphor / phosphor\Phosphor)
  app/src/bin/phosphor_oss.rs:26-40 (Channel::Oss, AppId::new("dev","phosphor","Phosphor"), autoupdate_config: None)
  directories-6.0.0/src/lib.rs config_local_dir table (Linux $XDG_CONFIG_HOME/<project>, Windows %LOCALAPPDATA%\<org>\<app>\config)
  crates/warp_core/src/paths.rs:37-69 (warp_home_config_dir = ~/.phosphor for Oss; skills/prompts/.mcp.json)
  crates/warpui_extras/src/user_preferences/toml_backed.rs:25-56 (native TOML types, hierarchy sections, snake_case keys, formatting/comments preserved, write_inhibited, write_inhibited_keys)
  app/src/settings/init.rs:495-525 (private store: Linux file_backed user_preferences.json, Windows registry_backed, macOS UserDefaults)
  crates/warpui_extras/src/user_preferences/registry_backed.rs:12,28 (HKCU\Software\Zap\<app>)
  app/src/settings/init.rs:539-570 (public store = TomlBackedUserPreferences when FeatureFlag::SettingsFile)
  app/src/lib.rs:2957-2958 + app/Cargo.toml:640,785 ("settings_file" is in the default feature list)
  app/src/keyboard.rs:35,101-103 (keybindings.yaml in config_local_dir)
  app/src/user_config/mod.rs:74-79 (filesystem watchers on data_dir and config_local_dir; settings.toml, keybindings.yaml, user_preferences.json)
  app/src/settings/mod.rs:86-107 (SettingsFileError variants and their relative severity)
  app/src/settings/init.rs:158-200 (priority order: parse failure > invalid values > unknown keys; always logged)
  app/src/settings/settings_file_diagnostics.rs:1-32 (unknown keys warn, never fail; cfg-gated groups are a known false positive)
  app/src/settings/local_control.rs:77-145 (secure-storage read/write/clear, owner-only fallback)

Discoverability:
  app/src/search/slash_command_menu/static_commands/commands.rs:259-266 (/open-settings-file)
  app/src/workspace/action.rs:766-767 (OpenSettingsFile opens settings.toml in a code editor pane)
  app/src/ai/skills/bundled.rs:555-560,612-616 (settings_schema.json in bundled_resources_dir; modify-settings requires it)
  resources/bundled/skills/modify-settings/SKILL.md (schema is source of truth; full TOML nesting)
  resources/bundled/skills/tui-settings/SKILL.md (which keys the TUI reads; the [appearance] theme vs [appearance.themes] theme collision; no TUI keybindings; fonts GUI-only)
  app/src/settings_view/mod.rs:196-235 (SettingsSection list = Settings window sections)

Group declarations (all defaults/paths/platforms above are read from these):
  app/src/settings/accessibility.rs:6-17 + crates/warpui_core/src/accessibility.rs:88-98 (Verbose default)
  app/src/settings/ai.rs:1847-2797 (AISettings group)
  app/src/settings/ai.rs:139-149 (VoiceInputToggleKey), :309-316 (DefaultSessionMode), :359-366 (ThinkingDisplayMode), :425-432 (OrchestrationMessageDisplayMode), :504-511 (PromptSubmissionMode), :565-572 (LongRunningCommandSubmissionMode)
  app/src/settings/ai.rs:629-668 (TuiStatuslineItem::ALL, 13 items), :700-718 (TuiStatuslineConfig default enabled set)
  app/src/settings/ai.rs:824-835 (AgentModeCodingPermissionsType default AlwaysAskBeforeReading)
  app/src/settings/ai.rs:944-975 (DEFAULT_COMMAND_EXECUTION_ALLOWLIST / DENYLIST contents)
  app/src/settings/ai.rs:1184-1235 (AgentProvider fields; api_key not persisted here)
  app/src/settings/ai.rs:1803-1830 (PerAgentSettings: toolbar/tabmenu default true, titlebar default on for Claude/Codex/Gemini/Antigravity)
  app/src/settings/alias_expansion.rs:5-15
  app/src/settings/app_icon.rs:32-70,125-136 (AppIcon variants, MAC only)
  app/src/settings/app_installation_detection.rs:27-35 (WEB, private)
  app/src/settings/autoupdate.rs:3-13
  app/src/settings/block_visibility.rs:7-34
  app/src/settings/cloud_preferences.rs:17-27 (PreferencesSettings)
  app/src/settings/code.rs:5-118 (incl. the deliberate default:false on codebase_context_enabled and why)
  app/src/settings/debug.rs:21-58 (all private)
  app/src/settings/editor.rs:24-28,53-58,100-104,183-290
  app/src/settings/emacs_bindings.rs:13-21 (LINUX, private)
  app/src/settings/font.rs:17-27 (DEFAULT_MONOSPACE_FONT_NAME "Hack", size 13.0, weight Normal, UI font name ""), :28-215
  crates/warpui_core/src/elements/text.rs:35 (DEFAULT_UI_LINE_HEIGHT_RATIO 1.2)
  crates/warp_core/src/ui/appearance.rs:18 (DEFAULT_UI_FONT_SIZE 12.0)
  crates/warpui_core/src/fonts.rs:37-48 (Weight variants, Normal default)
  crates/warpui_core/src/rendering/mod.rs:20-30 (ThinStrokes, OnHighDpiDisplays default)
  app/src/settings/gpu.rs:5-29
  app/src/settings/input.rs:26-33 (InputBoxType Classic default), :35-210
  app/src/settings/input_mode.rs:6-19 + app/src/terminal/block_list_viewport.rs:261-273 (PinnedToBottom default)
  app/src/settings/initializer.rs:44-49 (the is_onboarded()==Some(false) block is unreachable; local user hardcodes is_onboarded: true)
  app/src/settings/initializer.rs:80-105 (Windows font size 16.0 for new users)
  app/src/settings/language.rs:36-46,100-111
  app/src/settings/linux.rs:4-17 (default linux::is_wsl())
  app/src/settings/local_control.rs:46-58,149-151,169-183 + crates/warp_core/src/channel/mod.rs:30-35 (Oss is not dogfood => Disabled)
  app/src/settings/init_tests.rs:22-45 (MigrationTestSettings), :398-409 (NotificationsMigrationTestSettings)
  app/src/settings/native_preference.rs:20-46 (WEB)
  app/src/settings/network.rs:44-56,77-113 (ProxyMode::Off default and why)
  app/src/settings/pane.rs:5-24
  app/src/settings/privacy.rs:96-141 (defaults flipped true->false; PrivacySettings' custom_secret_regex_list and HasInitializedDefaultSecretRegexes)
  app/src/settings/same_line_prompt_block.rs:39-47 (private; registered at app/src/settings/init.rs:108 and read/written nowhere else -- inert)
  app/src/terminal/safe_mode_settings.rs:72-101 (SafeModeSettings: privacy.secret_redaction.enabled default false, secret_display_mode_setting default Strikethrough, hide_secrets_in_block_list default false)
  app/src/terminal/safe_mode_settings.rs:104-132 (get_secret_obfuscation_mode gates on safe_mode_enabled; get_effective_secret_display_mode consults the legacy key only while the new one is at its default)
  app/src/settings/init.rs (the ~50 ::register() calls -- the full group list this section is measured against)
  crates/warpui/src/platform/mac/delegate.rs:302 vs crates/warpui/src/windowing/winit/delegate.rs:543 (set_accessibility_contents implemented on macOS, empty on the winit delegate used by Linux/FreeBSD/Windows)
  app/src/lib.rs:1701 (a11y verbosity is read on every platform)
  app/src/terminal/settings.rs:192-223 + app/Cargo.toml:648 (experimental.async_find_enabled is ORed with FeatureFlag::AsyncFind, which is a default feature)
  app/src/settings/initializer.rs:80-105 + app/src/auth/mod.rs:213 (the Windows 16.0 font-size override is inside is_onboarded()==Some(false), which is never true)
  app/src/settings/input_mode.rs:9-11 + app/src/lib.rs:2955-2956 (the "new users default to waterfall" comment is stale; DefaultWaterfallMode has no reader)
  app/src/settings/scroll.rs:3-13
  app/src/settings/select.rs:26-32,44-86 (platform sets: LINUX for selection clipboard, WINDOWS|MAC for middle click)
  app/src/settings/ssh.rs:5-30
  app/src/settings/theme.rs:14-49 + app/src/themes/theme.rs:42-73 (PhosphorAmber at :64 is #[default]), :594-617 (SelectedSystemThemes default light/dark), :36-40 (theme portability)
  app/src/settings/tui_autoupdate.rs:1-21
  app/src/settings/tui_theme.rs:1-14,42-47,97-107 (TuiTheme::Auto default)
  app/src/settings/tui_zero_state.rs:17-22,24-32,143-183 (bounds 1-60 and 0.02-0.5; show_signed_in_user not ported)
  app/src/settings/vim_banner.rs:12-20 (private)
  app/src/drive/settings.rs (WarpDriveSettings — separate group, referenced only for warp_drive_context_enabled's meaning)

Inertness claims:
  app/src/settings_view/features_page.rs:6469 and app/src/settings_view/keybindings.rs:1158 and app/src/settings_view/settings_page.rs:522 (settings_sync_enabled only drives a "local only" icon); no CloudPreferencesSyncer exists in the tree
  app/src/ai/agent/api.rs:512-513,642 + grep: allow_use_of_warp_credits_with_byok is never read after being set
  app/Cargo.toml ("autoupdate" not in default) + script/macos/bundle:358 and script/windows/bundle.ps1:125 (both ADD the feature) + script/linux/bundle:198-203 (oss resets FEATURES to "release_bundle", dropping the crash_reporting default set at :24) + crates/warp_features/src/lib.rs:895-907 (RELEASE_FLAGS: FeatureFlag::Autoupdate is deliberately NOT present) + app/src/lib.rs:2926-2929 (extra_flags adds it under #[cfg(feature = "autoupdate")])
  app/src/autoupdate/mod.rs:273-274,313-317 (both autoupdate paths gated on FeatureFlag::Autoupdate / can_autoupdate)
  app/src/autoupdate/github.rs:1-6 (Oss autoupdate source is GitHub Releases)
  DECLINED.md "TUI autoupdate" row (crates/warp_tui/src/bin/oss.rs:42 hardcodes autoupdate_config: None; server_root_url is a 192.0.2.0:9 sentinel)
  DECLINED.md "Telemetry and crash reporting" table (is_telemetry_available hard-coded false, widget never renders). NOTE: the crash-reporting toggle *renders* (should_render gates on FeatureFlag::CrashReporting, which is in RELEASE_FLAGS -- crates/warp_features/src/lib.rs:912, app/src/lib.rs:2906-2907) but does nothing in a shipped build: crash_reporting::init sits behind #[cfg(feature = "crash_reporting")] (app/src/lib.rs:1551-1557) and no OSS bundler sets that cargo feature. Verify this at the `oss` BRANCH, not at the script's default FEATURES line -- script/linux/bundle:24 and script/windows/bundle.ps1:17 do list crash_reporting, but those are the dev-channel values and the oss branches OVERWRITE them: script/macos/bundle:358 = release_bundle,extern_plist,autoupdate; script/linux/bundle:203 = release_bundle; script/windows/bundle.ps1:125 = release_bundle,gui,nld_improvements,autoupdate. Panic backtraces reach the log via log_panics regardless (crates/warp_logging/src/native.rs:804-807). See section 9.
  app/Cargo.toml:474-479 ("crash_reporting" is not in the default feature list)
  app/src/crash_reporting/mod.rs:189-200,270-290 (init reads the setting and installs the local panic hook)
  app/src/settings_view/privacy_page.rs:1538-1550 (should_render gate)
  DECLINED.md "Privacy toggle defaults" row
  DECLINED.md "Voice input" section (VoiceTranscriber::disabled() since 9d92598c4; TranscribeError::Disabled: "the BYOP genai protocol can't carry audio")
  DECLINED.md "Voice input language preference" row (#352)
  DECLINED.md "SettingSurfaces / SettingsMode" row (:207)
  DECLINED.md "Agent commit/PR attribution (agent_attribution_enabled)" row (#445)
  DECLINED.md "Cloud teams / org policy" and "Teams stay stubbed" rows (#445)
  DECLINED.md "Account-first onboarding, billing, paid tiers" row (#11)
  DECLINED.md "Screen recording" row (#367)
  DECLINED.md "The master AI switch scopes Zap's own AI, not third-party CLI agents" row (:168)
  DECLINED.md "BYOP last-used-model carry-over (byop_last_used_model_id)" row (:160)
  DECLINED.md "Codebase index 'Index Codebase?' speedbump banner" row
  DECLINED.md "SSH tmux wrapper — kept, deprecation not ported" row
  DECLINED.md "Three gated-but-not-default feature flags" row (WindowsHighPerformanceGpuDefault reachable only via UNSTABLE_FEATURES)
-->
