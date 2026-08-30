---
name: update-tab-config
description: Update existing Phosphor tab config TOML files from natural-language edit requests. Use when the user wants to modify a tab config that already exists or when editing a tab config file already open in Phosphor.
---

# update-tab-config

Update an existing Phosphor tab config in place.

## Required context

- Use the `tab-configs` skill as the canonical source of truth for:
  - schema details
  - validation rules
  - examples
  - common layout patterns

## Workflow

1. Read the existing tab config file before making changes.
2. Understand the requested edit.
3. If important details are missing or ambiguous, use the `ask_user_question` tool before editing. Do not guess about layout changes, command changes, parameters, or `on_close` behavior.
4. Make sure you are editing the right file, then update it so it remains valid according to the `tab-configs` schema.
   - A user's own tab configs live in `{{tab_configs_dir}}` — the directory this running build actually reads, resolved for its channel and platform. Do not assume a hardcoded base directory and do not glob `$HOME` for a `.warp*` directory.
   - The editable **templates** live in `{{default_tab_configs_dir}}`. `worktree.toml` there is what the "Worktree in…" submenu materializes every generated worktree config from, so a request to change the default worktree behaviour (its panes, its commands, the branch it creates) means editing that file, not a config in the directory above.
   - A config the user already has open in the editor may live somewhere else entirely; edit the file they pointed at.
5. Preserve the user's existing structure and naming where possible unless the requested change requires restructuring.
6. Briefly explain what changed.
