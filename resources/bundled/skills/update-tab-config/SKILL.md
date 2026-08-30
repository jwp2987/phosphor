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
4. Make sure you are editing a config in `{{tab_configs_dir}}` — the tab config directory this running build actually reads, resolved for its channel and platform — rather than assuming a hardcoded base directory or globbing `$HOME` for a `.warp*` directory. (A config the user has open in the editor may live elsewhere; edit the file they pointed at.) Then update it so it remains valid according to the `tab-configs` schema.
5. Preserve the user's existing structure and naming where possible unless the requested change requires restructuring.
6. Briefly explain what changed.
