---
name: create-tab-config
description: Create new Phosphor tab config TOML files from natural-language requests. Use when the user wants a new tab config, a new tab layout, or asks for a slash command to generate a tab config.
---

# create-tab-config

Create a new Phosphor tab config based on what the user wants.

## Required context

- Use the `tab-configs` skill as the canonical source of truth for:
  - schema details
  - validation rules
  - examples
  - common layout patterns

## Workflow

1. Understand what the user wants to create.
2. If important details are missing, use the `ask_user_question` tool to clarify them before writing anything. Do not guess about layout, commands, directories, parameters, or close-time behavior.
3. Generate valid TOML that matches the `tab-configs` schema.
4. Write the config into `{{tab_configs_dir}}`.
   That is the tab config directory this running build actually reads, resolved for its channel and platform — use it verbatim. Do not derive it by globbing `$HOME` for a `.warp*` directory and do not use `~/Library/Application Support/`; on Linux the directory is the XDG data directory, so no `$HOME` dotfile glob will find it. If the path above is blank, Phosphor could not resolve it — ask the user where their tab configs live instead of guessing.
   Create the directory if it does not exist.
   Write the file using a descriptive snake_case filename ending in `.toml`.
5. If the intended filename might conflict with an existing config and it is unclear whether to overwrite or create a new file, use the `ask_user_question` tool.
6. Briefly explain what you created, including the layout and any commands or parameters.
