This directory is the Claude Code plugin that ships inside Phosphor. Phosphor registers it
with Claude Code as a local-folder marketplace (`claude-code-warp`, plugin `warp`); nothing is
downloaded. The plugin writes OSC 777 notifications to the terminal only and makes no network
calls. Its version must equal `MINIMUM_PLUGIN_VERSION` in
`app/src/terminal/cli_agent_sessions/plugin_manager/claude.rs` (a unit test checks this).
Derived from an MIT-licensed upstream plugin; see LICENSE.
