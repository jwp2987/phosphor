This directory is the Codex plugin that ships inside Phosphor. Phosphor registers it with Codex
as a local marketplace (`codex-warp`, plugin `warp`); nothing is downloaded. The plugin writes
OSC 777 notifications to the terminal only and makes no network calls. Its version must equal
`MINIMUM_PLUGIN_VERSION` in `app/src/terminal/cli_agent_sessions/plugin_manager/codex.rs` (a
unit test checks this). Derived from an MIT-licensed upstream plugin; see LICENSE.
