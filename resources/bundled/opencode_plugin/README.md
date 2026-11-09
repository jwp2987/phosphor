This directory is the OpenCode plugin that ships inside Phosphor. Phosphor points the `plugin`
array of OpenCode's global config at this folder (a `file://` entry); nothing is downloaded and
no npm install runs. The plugin writes OSC 777 notifications to the terminal only and makes no
network calls. The built `dist/` is kept as shipped. Its version must equal
`MINIMUM_PLUGIN_VERSION` in `app/src/terminal/cli_agent_sessions/plugin_manager/opencode.rs`
(a unit test checks this). Derived from an MIT-licensed upstream plugin; see LICENSE.
