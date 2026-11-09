This directory is the Gemini CLI extension that ships inside Phosphor. Phosphor installs it
with `gemini extensions install <this folder> --consent` (a local copy; the extension is named
`gemini-warp`); nothing is downloaded. The hooks write OSC 777 notifications to the terminal
only and make no network calls. Its version must equal `MINIMUM_PLUGIN_VERSION` in
`app/src/terminal/cli_agent_sessions/plugin_manager/gemini.rs` (a unit test checks this).
Derived from an MIT-licensed upstream extension; see LICENSE.
