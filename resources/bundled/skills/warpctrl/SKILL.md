---
name: warpctrl
description: Control and inspect the currently running local Phosphor application with the warpctrl CLI. Use this skill whenever the user asks the agent to manipulate Phosphor's own windows, tabs, panes, sessions, input buffer, themes, or UI surfaces; open a file in Phosphor; inspect local Phosphor state; or explain how to invoke Phosphor Control manually.
---

# Phosphor Control

Use `{{warpctrl_binary_name}}` to inspect or control the already-running local Phosphor application that provided this skill. The command name and wrapper path in this skill are injected for the current Phosphor channel, so do not inspect running processes or guess which channel is active.

Prefer `{{warpctrl_binary_name}}` when the requested action changes Phosphor itself rather than the user's project or operating system. Examples include creating a Phosphor tab, splitting a pane, staging text in Phosphor's input, opening Phosphor settings, or focusing a Phosphor window.

## How to invoke Phosphor Control

Phosphor Control is bundled into the Phosphor application. It is not a separate standalone binary; it is a hidden control mode served by the running Phosphor process.

- Command for the current Phosphor channel: `{{warpctrl_binary_name}}`
- Bundled wrapper for the current Phosphor channel: `{{warpctrl_wrapper_path}}`
- Optional PATH symlink: `/usr/local/bin/{{warpctrl_binary_name}}`

### Ensure the command is available

Before invoking Phosphor Control for the first time in a task, prefer the shortest available path and avoid unnecessary setup research:

1. If `command -v {{warpctrl_binary_name}}` succeeds, use `{{warpctrl_binary_name}}` for the rest of the task. Do not inspect the bundled wrapper or verify the symlink unless a later command fails.
2. If `command -v {{warpctrl_binary_name}}` fails, verify that `{{warpctrl_wrapper_path}}` exists and is executable. If it is missing, tell the user that this Phosphor build does not contain the expected wrapper and stop.
3. Inspect `/usr/local/bin/{{warpctrl_binary_name}}`. Treat setup as complete only when it is a symlink that resolves to the exact `{{warpctrl_wrapper_path}}` bundled wrapper.
4. If the expected symlink is missing, broken, or points elsewhere, use the `ask_user_question` tool to ask whether the user wants to install it at `/usr/local/bin/{{warpctrl_binary_name}}` pointing to `{{warpctrl_wrapper_path}}`. Offer **Install command** as the recommended option and **Not now** as the alternative. Do not create or replace a symlink without an affirmative response.
5. After approval, create or update only the expected symlink by running `ln -sf "{{warpctrl_wrapper_path}}" "/usr/local/bin/{{warpctrl_binary_name}}"`. Try without elevation first. If macOS permissions prevent the change, run that same command through `osascript` with administrator privileges; never request or expose the user's password directly.
6. Verify the result with `command -v {{warpctrl_binary_name}}`, `readlink /usr/local/bin/{{warpctrl_binary_name}}`, and `{{warpctrl_binary_name}} app version`.

If the user chooses **Not now**, do not create the symlink. Use the bundled wrapper at `{{warpctrl_wrapper_path}}` directly for the current task.

The Phosphor UI also exposes install and uninstall actions for the Phosphor Control CLI in the Command Palette (search for `warpctrl`), and an install control under **Settings > Scripting**.

## Workflow

Always prefer discovering commands from `{{warpctrl_binary_name}}` itself rather than guessing or inventing them. The CLI provides full help and an action catalog that is the authoritative source of truth for what the installed build supports.

### Execute serially and validate results

Run Phosphor Control commands serially. Never dispatch multiple `{{warpctrl_binary_name}}` commands through parallel shell-tool calls, even when the commands appear independent. They act on the same running app and may change the active target or the terminal context used to execute and observe later commands. For multi-step requests, prefer one shell-tool call that chains commands sequentially, or issue separate shell-tool calls one at a time.

After an action that creates, activates, navigates, or focuses a window, tab, pane, session, or surface, do not assume the active target is unchanged. Use explicit selectors for later commands when exact targeting matters, or rerun `{{warpctrl_binary_name}} app active` before continuing.

Validate that each result corresponds to the command that was invoked. If output describes a different action, reports an unexpected instance or channel, or otherwise conflicts with the request, stop and rerun `{{warpctrl_binary_name}} instance list` serially before retrying. Do not report success until the requested final state has been verified when a corresponding `list`, `inspect`, or `get` command is available.

### Route by intent

Before discovering commands, route the request to the narrowest matching top-level group:

1. Requests to open, show, view, or toggle a named Phosphor UI destination, panel, picker, or settings page use `surface`. Convert natural-language names to kebab case, such as "Library" to `warp-drive` and "code review" to `code-review`. Note that a few subcommand names still carry their original spelling even where the UI label was renamed, so confirm the exact name with `surface list` or `surface help` rather than deriving it from the visible label. Prefer `surface <name> open` when the requested final state is open. Do not infer an internal action name for a UI destination.
2. Requests about windows, tabs, panes, or sessions use the matching `window`, `tab`, `pane`, or `session` group.
3. Requests to stage or inspect editor input use `input`.
4. Requests to open a file in Phosphor use `file`.
5. Requests about themes, appearance, settings, or keybindings use the matching `theme`, `appearance`, `setting`, or `keybinding` group.
6. Use the generic `action` catalog only when no dedicated CLI group matches. Internal or catalog action names are not guaranteed to be reachable as standalone parser commands.

1. Discover running Phosphor instances from the current Phosphor channel:

   ```sh
   {{warpctrl_binary_name}} instance list
   ```

2. If exactly one same-channel instance is running, commands select it automatically. If multiple same-channel instances are running, select one explicitly with `--instance <instance_id>` or `--pid <pid>`.

3. Discover the exact command and parameters from the routed group instead of guessing. This is the preferred source of truth for the available command surface:

   ```sh
   {{warpctrl_binary_name}} help
   {{warpctrl_binary_name}} <group> help
   {{warpctrl_binary_name}} <group> <command> --help
   ```

   Only when no dedicated group matches, inspect the generic action catalog:

   ```sh
   {{warpctrl_binary_name}} action list
   {{warpctrl_binary_name}} action inspect <action.name>
   ```

4. Inspect the active target chain or list the relevant targets before changing them:

   ```sh
   {{warpctrl_binary_name}} app active
   {{warpctrl_binary_name}} window list
   {{warpctrl_binary_name}} tab list
   {{warpctrl_binary_name}} pane list
   {{warpctrl_binary_name}} session list
   ```

5. Invoke the narrowest action that satisfies the request, then verify the result with the corresponding `list`, `inspect`, or `get` command when useful.

## Common actions

These are frequently used commands that are safe to invoke directly. For less common commands, route by intent and use `{{warpctrl_binary_name}} <group> help` or `{{warpctrl_binary_name}} <group> <command> --help` to discover the exact syntax supported by the running build. Inspect the generic action catalog only when no dedicated group matches.

```sh
# Create and manage tabs and panes
{{warpctrl_binary_name}} tab create
{{warpctrl_binary_name}} tab create --type agent
{{warpctrl_binary_name}} tab rename "server logs"
{{warpctrl_binary_name}} pane split --direction right
{{warpctrl_binary_name}} pane navigate --direction next

# Stage text in Phosphor's input without submitting it
{{warpctrl_binary_name}} input insert "git status"
{{warpctrl_binary_name}} input replace "cargo test"

# Open or toggle Phosphor UI surfaces
{{warpctrl_binary_name}} surface list
{{warpctrl_binary_name}} surface settings open
{{warpctrl_binary_name}} surface command-palette open --query "theme"
{{warpctrl_binary_name}} surface command-search open
{{warpctrl_binary_name}} surface theme-picker open
{{warpctrl_binary_name}} surface keybindings open
{{warpctrl_binary_name}} surface warp-drive open
{{warpctrl_binary_name}} surface resource-center toggle
{{warpctrl_binary_name}} surface ai-assistant toggle
{{warpctrl_binary_name}} surface project-explorer open
{{warpctrl_binary_name}} surface global-search open
{{warpctrl_binary_name}} surface conversation-list open
{{warpctrl_binary_name}} surface code-review open
{{warpctrl_binary_name}} surface left-panel toggle
{{warpctrl_binary_name}} surface right-panel toggle
{{warpctrl_binary_name}} surface vertical-tabs open

# Open a file in Phosphor
{{warpctrl_binary_name}} file open ./src/main.rs --line 42

# Inspect and update supported state
{{warpctrl_binary_name}} theme get
{{warpctrl_binary_name}} theme set "Dracula"
{{warpctrl_binary_name}} appearance get
{{warpctrl_binary_name}} setting list
{{warpctrl_binary_name}} keybinding list
```

Add `--output-format json` when structured output is easier to consume:

```sh
{{warpctrl_binary_name}} --output-format json tab list
```

## Targeting

Target selectors can be combined when the action supports their scope:

- Instance: `--instance <instance_id>` or `--pid <pid>`
- Window: `--window <id>`, `--window-index <n>`, or `--window-title <exact-title>`
- Tab: `--tab <id>`, `--tab-index <n>`, or `--tab-title <exact-title>`
- Pane: `--pane <id>` or `--pane-index <n>`
- Session: `--session <id>`

Use IDs returned by `list`, `inspect`, or `app active` when exact targeting matters. If a selector is omitted, most scoped actions operate on the active target. Prefer explicit selectors when more than one target could reasonably match the user's request.

Use `surface list` before a walkthrough or multi-step UI workflow. It reports both available and unavailable destinations with stable names and reasons. The direct `surface ... open` commands are idempotent; use them instead of toggle commands when the final open state matters. `surface list` accepts `--instance` or `--pid` for process selection but rejects window, tab, pane, and session selectors.

## Safety and limitations

- Invoke close actions only when the user explicitly asks to close something. Close actions flow through normal Phosphor close behavior and may trigger existing app warnings.
- `input insert` and `input replace` only stage text. Phosphor Control intentionally does not provide an action that submits or runs the input.
- Do not invent unsupported commands. Use the matching group's `help` first, then use `action list` or `action inspect` only when no dedicated group matches.
- Some catalog actions are present but unimplemented in this build; `action inspect <action.name>` reports their status. Treat an unimplemented action as unavailable rather than retrying it.
- Phosphor Control affects only a running local Phosphor application owned by the same user. It does not control remote Phosphor instances.
- Each channel-specific Phosphor Control CLI lists and targets only Phosphor instances from its own channel.
- On Windows, local-control publication is disabled until authenticated broker transport is supported.

## Manual setup and troubleshooting

Phosphor Control availability depends on the build channel and the **Settings > Scripting** toggle. The local-control mode defaults to enabled on internal dogfood builds (for example the Dev channel) and disabled on public channels (Stable, Preview, Oss). On any channel, the final gate is the **Settings > Scripting** toggle. The installed `{{warpctrl_binary_name}}` wrapper invokes the matching channel-specific Phosphor executable.

If `{{warpctrl_binary_name}} instance list` is empty, confirm that a compatible same-channel Phosphor app is running and Scripting is enabled. If a command reports multiple instances, rerun it with `--instance <instance_id>`.

If the symlink is not on `PATH`, follow the confirmation-gated setup flow in **How to invoke Phosphor Control** or use `{{warpctrl_wrapper_path}}` directly.
