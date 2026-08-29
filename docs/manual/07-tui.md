# 7. The terminal UI (`phosphor-tui`)

Phosphor ships two front-ends over one codebase. Chapter 1–6 describe the
graphical app; this chapter describes the second one — a keyboard-driven
terminal front-end built on `ratatui`/`crossterm` that runs inside whatever
terminal emulator you already use. It is a real terminal (it starts your shell
on a PTY, renders blocks, and hands full-screen programs like `vim` the whole
window) with the Phosphor agent, its transcript, its permission prompts and most
of its slash commands layered on top. It is not a cut-down chat client, but it
*is* a smaller surface than the GUI: there is no settings window, no tabs or
split panes you can create, no code editor, and no mouse-first affordances
beyond click-to-focus and scroll.

Both front-ends share one application identity, so the models, execution
profiles, MCP servers, skills, prompts and BYOP API keys you configure in one
are the ones the other uses. There is no separate TUI configuration.

---

## 7.1 Launching it

The release archives contain a binary called **`phosphor-tui`**
(`phosphor-tui-linux-x86_64.tar.gz`, `phosphor-tui-macos-<arch>.tar.gz`,
`phosphor-tui-windows-x64.zip`). Unpack it, put it on your `PATH`, and run:

```console
$ phosphor-tui
```

That is the whole launch story: no subcommands, no config file to create first.

> **Naming.** Inside the repository the cargo binary is still called
> `zap-tui-oss` — a lineage internal that the release jobs rename to
> `phosphor-tui` before packaging. If you build from source you will run
> `cargo run -p warp_tui` (or `target/release/zap-tui-oss`) and the usage line
> will echo whichever name you invoked it by. Everything else is identical.

### Flags

`phosphor-tui` has its own small argument parser — it does **not** share the
GUI binary's `phosphor-oss <subcommand>` CLI, and it has no subcommands of its
own.

| Flag | What it does | Default |
|---|---|---|
| `--resume <TOKEN>` | Resume a conversation by server token. **Inert in Phosphor** — see §7.13. | none |
| `--auto-approve` | New conversations start in run-to-completion auto-approve instead of following your settings. Does *not* suppress the agent's questions. | off |
| `--api-key <KEY>` | Warp-account authentication key, also read from `WARP_API_KEY`. **Inert in Phosphor** — it is not a provider key and the shipped build discards it. | none |
| `--set-provider-api-key <openai\|anthropic\|google\|grok>` | One-shot: prompt (masked) for a provider key, store it in the OS keychain, print a confirmation, exit. Reads the key from stdin when stdin is not a TTY. Never launches the UI. | — |
| `--clear-provider-api-key <openai\|anthropic\|google\|grok>` | One-shot: remove that provider's stored key and exit. | — |
| `-h`, `--help` | Print help and exit. | — |
| `-V`, `--version` | Print the bare version string and exit. | — |

Notes that matter in practice:

* `--api-key` / `WARP_API_KEY` is **not** how you supply an OpenAI or Anthropic
  key. It fed Warp's account authentication, and the value is dropped outright
  on the release channel Phosphor ships. Use `--set-provider-api-key`,
  `/api-keys`, or the GUI's agent-provider settings instead.
* `--set-provider-api-key` and `--clear-provider-api-key` cannot be combined
  with each other or with `--resume`.
* `grok` parses but is **rejected at runtime** for both key flags, with a
  message pointing you at the arbitrary-provider store instead (Settings → AI →
  Agent providers in the GUI, or `/api-keys` in the TUI). Only OpenAI,
  Anthropic and Google have a pasted-key path through these flags.
* Keys written by these flags land in the shared keychain and a running GUI or
  TUI picks them up without a restart.
* `--help` deliberately hides the resolved value of `WARP_API_KEY`, so it is
  safe to paste help output into an issue. (Upstream Warp does not do this in
  its TUI; Phosphor diverges on purpose.)

### How it relates to the rest of the program

The TUI is driven by the shared runtime in `crates/warpui_core/src/runtime/`.
On start it puts your terminal into raw mode and the **alternate screen**, and
enables mouse capture, bracketed paste, focus-change reporting and — where the
terminal supports it — the Kitty keyboard-enhancement protocol. All of that is
restored when the process exits, including on panic. Because it uses the
alternate screen, quitting returns your terminal to exactly the scrollback you
left.

There is no login step. Phosphor is bring-your-own-provider, so the TUI comes up
already "signed in" and creates its first session immediately, in the directory
you launched it from.

---

## 7.2 First launch: the zero state

Before you have submitted anything, the transcript area is filled by the *zero
state*:

* the **Phosphor Agent title and version** (always shown — no toggle);
* a **"What's new"** changelog section, capped at three bullets;
* **project context** — the working directory, plus the rules and skills
  discovered for it;
* **MCP** server status;
* a slowly rotating ASCII object over a starfield, filling the space beside the
  copy.

The zero state is not a splash screen: it comes back whenever the transcript
empties out again (for example after `/clear`), and it disappears as soon as
your first submission produces a block.

Everything except the title/version is configurable under
`[appearance.zero_state]` in `settings.toml` (§7.10). The two you are most
likely to want:

```toml
[appearance.zero_state]
show_animation = false                  # drop the rotating object + starfield
freeze_animation_when_unfocused = true  # stop repainting while the terminal is unfocused
```

You can also substitute your own art:

```toml
[appearance.zero_state]
object = { type = "ascii_file", path = "logos/mine.txt" }
```

Relative paths resolve against the config directory. Changing the setting
hot-reloads; editing the art file itself needs a restart.

The empty prompt also carries a ghosted hint line that changes with context —
`? for shortcuts • / for commands • ← for conversations` on a fresh transcript,
`Ask the agent anything • ? for shortcuts • ! for shell mode • / for commands`
once there is history.

---

## 7.3 Getting around

The composer at the bottom is the centre of the UI. It has two modes and a set
of overlay menus.

### Agent mode vs shell mode

By default the input is **agent-first**: what you type is a prompt. Type `!` at
the very start of an empty input to switch to **shell mode**, where the line is
run as a shell command. `Esc`, or `Backspace` on the empty line, returns you to
agent mode. If natural-language autodetection is enabled (`/natural-language-detection`
toggles it), Phosphor may route an obvious command straight to the shell without
the `!`.

### The trigger characters

These only fire at the start of an empty, idle input — everywhere else they are
ordinary text.

| Key | Opens |
|---|---|
| `?` | The keyboard-shortcuts sheet (press `?` again, `Esc`, or `Ctrl-C` to close) |
| `/` | The slash-command palette |
| `!` | Shell mode |
| `←` | The conversation picker (`/conversations`) |
| `↑` | Prompt-and-command history (prompts *and* commands in agent mode; commands only in shell mode) |
| `Shift-↑` | The orchestration tab bar, when child agents exist |
| `Tab` | Shell/path completions for the token under the cursor — in both agent and shell mode |

Only one overlay menu is open at a time; opening another closes the first.
`Esc` closes whatever is open before it does anything else.

### The shortcuts sheet (`?`)

The `?` sheet is generated from the live keymap, so it always shows what is
actually bound *right now* in the state you are in — it shrinks to a single
line while a command is running, and disappears entirely while a permission
prompt or question is blocking. Sections are `Shortcuts`, `Terminal use` and
`Orchestration`.

### `/status`

`/status` opens a read-only session panel (model, directory, and related session
facts). Unlike Warp's, it has no organisation or account-email rows — there is
no account.

---

## 7.4 Keybinding reference

Every binding below is registered by the TUI itself. GUI keybindings cannot fire
here, and vice versa.

> **Remapping is not wired up yet.** The bindings are all registered under
> stable `tui:*` names precisely so that they can be remapped from
> `keybindings.yaml` — but the TUI process does not yet load overrides from that
> file. Treat the defaults below as fixed for this release. (`keybindings.yaml`
> continues to work for the GUI.)

Key names follow the source spelling: `ctrl-shift-I` means Ctrl+Shift+I.
`cmd-` chords are registered on every platform in the TUI, because a terminal
can deliver a Super chord as input on any OS — whether they arrive depends on
your terminal emulator.

### Session-wide

| Key | Action | When |
|---|---|---|
| `Ctrl-C` | Contextual: dismiss an open `?`/`/status` sheet → interrupt a running command → kill the focused child agent → cancel the running conversation → clear the input. Arms a one-second "press again to exit" window. | always |
| `Ctrl-C` again within 1 s | Exit Phosphor | after a first `Ctrl-C` |
| `Ctrl-D` | Exit when the prompt is empty; otherwise delete the character under the cursor | prompt focused |
| `Enter` | Submit the input | prompt focused |
| `Esc` | Contextual: close an open menu → vim mode change → leave shell mode | prompt focused |
| `Esc` | Cancel an in-progress conversation restore | while restoring |
| `Ctrl-Shift-I` | Toggle auto-approve | session focused |
| `Ctrl-Shift-P` | Expand/collapse the latest plan | session focused |
| `Ctrl-P` | Expand/collapse the latest visible plan — fallback binding, registered only when a plan is available *and* the terminal does not support keyboard enhancement | prompt/session |
| `Tab` | Trigger completions for the token under the cursor | no image attachments pending |
| `Tab` | Focus the image-attachment bar | image attachments pending |
| `Ctrl-V` (also `Alt-V` on Windows) | Paste from the clipboard: images become attachments, text is inserted | composer active |
| `Ctrl-Shift-Enter` | Hand the running command to the agent | a long-running command is attachable |
| `Esc` | Return control to the running command | agent attached to a running command |
| `Ctrl-G` | Hand control back, while you hold a terminal-use session | agent requested terminal use |
| `Ctrl-O` / `Ctrl-R` | Allow / reject a blocked long-running-command action | such an action is blocked |

### Composer editing

Registered twice — once for the prompt (`tui:input:*`) and once for the
multi-line editors embedded in prompts (`tui:editor:*`). Cursor-movement and
selection rows marked ● are prompt-only.

| Command | Keys |
|---|---|
| Insert newline | `Shift-Enter`, `Ctrl-J`, `Alt-Enter` |
| Delete previous character | `Backspace`, `Shift-Backspace`, `Ctrl-H` |
| Delete next character | `Delete`, `Ctrl-D` *(`Ctrl-D` is reserved for EOF in the prompt — see above)* |
| Delete previous word | `Ctrl-W`, `Ctrl-Backspace`, `Alt-Backspace` |
| Delete next word | `Alt-D`, `Alt-Delete`, `Ctrl-Delete` |
| Move left / right | `←` `Ctrl-B` / `→` `Ctrl-F` |
| Move up / down ● | `↑` `Ctrl-P` / `↓` `Ctrl-N` |
| Move one word left / right | `Alt-←` `Alt-B` `Ctrl-←` / `Alt-→` `Alt-F` `Ctrl-→` |
| Move to line start / end | `Home` `Ctrl-A` `Cmd-←` / `End` `Ctrl-E` `Cmd-→` |
| Extend selection left / right | `Shift-←` / `Shift-→` |
| Extend selection up / down ● | `Shift-↑` / `Shift-↓` |
| Extend selection one word left / right | `Ctrl-Shift-←` `Alt-Shift-←` / `Ctrl-Shift-→` `Alt-Shift-→` |
| Extend selection to line start / end | `Cmd-Shift-←` / `Cmd-Shift-→` |
| Select all | `Ctrl-Shift-A`, `Cmd-A` |
| Copy | `Ctrl-Shift-C`, `Alt-W`, `Cmd-C` |
| Cut | `Ctrl-X`, `Cmd-X` |
| Paste | `Cmd-V` *(use `Ctrl-V` — it also handles image attachments)* |
| Delete to end of line | `Ctrl-K` |
| Delete to start of line | `Ctrl-U`, `Cmd-Backspace` |
| Paste the last deleted text | `Ctrl-Y` |
| Undo / redo | `Ctrl-Z` `Cmd-Z` / `Ctrl-Shift-Z` `Cmd-Shift-Z` |

`Ctrl-P` and `↑` on the first row of an empty prompt open history rather than
moving the cursor; `←` on an empty prompt opens the conversation picker.

Vim mode (`/vim-mode`) is available in the TUI composer and takes priority over
these where they conflict; `Esc` becomes a vim mode change, and `Esc Esc` from
Normal mode leaves shell mode. Clicking in the input forces Insert mode.

### Option lists (permission prompts, questions, pickers)

| Key | Action |
|---|---|
| `↑` / `↓` | Previous / next option |
| `Enter` | Confirm the highlighted option |

### Permission prompts

| Key | Action |
|---|---|
| `Enter` | Confirm the selected response (`yes` / `no` / `Other`) |
| `e` | Focus the editable action body, where the tool call has one |
| `Esc` | Unwind "Other" editing; otherwise reject the request |
| `Ctrl-T` | Expand/collapse a shell command's output section, or a file-edit's primary section |
| `e` | On a blocked file-edits card: expand or collapse **all** diffs |
| `Enter` / `NumpadEnter` | Save an edited shell command |
| `Esc` | Leave the shell-command editor without cancelling the tool call |

### Ask-question cards

| Key | Action |
|---|---|
| `Enter` | Select or confirm the highlighted answer |
| `Shift-Enter` | Advance after selecting several answers (multi-select questions) |
| `←` / `→` | Previous / next question |
| `Tab` | Next question |
| `Ctrl-C` | Skip all remaining questions |

### Image attachments

| Key | Action |
|---|---|
| `Tab` / `→` | Next attachment |
| `Shift-Tab` / `←` | Previous attachment |
| `Backspace` / `Delete` | Remove the selected attachment |
| `Esc` / `Enter` | Return focus to the input |

### Orchestration tab bar

Reachable with `Shift-↑` when child agents exist.

| Key | Action |
|---|---|
| `←` / `→` | Previous / next tab in the current row |
| `Shift-Tab` / `Tab` | Previous / next agent in the whole tree |
| `Shift-←` / `Shift-→` | First / last child agent |
| `↓` / `Shift-↓` | Return focus to the session input |
| `Esc` | Return to the main agent and focus its input |
| `Ctrl-C` | Kill the selected child agent and its loaded subtree |

### The `/statusline` picker

| Key | Action |
|---|---|
| `Enter` | Toggle the highlighted item |
| `←` / `→` | Move the highlighted item left / right |
| `Esc` | Save and close |
| `Ctrl-C` | Cancel |

### The `/mcp` menu

| Key | Action |
|---|---|
| `Ctrl-R` | Log out of the selected MCP server and remove its credentials |

### Mouse

Mouse capture is on. You can click the input to focus it (which also moves the
caret), click and drag to select text, click statusline items that are
clickable (auto-approve, the to-do list, a pull-request link), click headers to
expand or collapse sections, and scroll menus and the transcript with the wheel.
There are no keyboard bindings for transcript scrolling.

---

## 7.5 Sessions, child agents, and focus

**You cannot open a second session yourself.** Phosphor's TUI creates exactly
one foreground session, in the directory you launched it from, and there is no
new-tab, new-pane or new-window command. Everything you do happens in that one
session.

Additional sessions exist, but only as **child agents**: `/orchestrate <task>`
spawns one or more local child agents, each backed by its own hidden session.
They are background sessions in the strict sense — they are never given focus
when created and they are not rendered; only the focused session's view is
drawn. You reach them through the orchestration tab bar (`Shift-↑`), which is
the only navigation surface for them, and you come back with `Esc` or `↓`.

Killing a child (`Ctrl-C` on its tab, or a double `Ctrl-C` while viewing its
conversation within one second) cancels its work, deletes its conversation, drops
its session, and returns focus to the root agent.

### Focus behaviour

Focus ownership was hardened for this release, and two rules are worth knowing
because they are what you will notice:

1. **Clicking the prompt focuses it.** A click or drag inside the input asks the
   session to reclaim focus, not just to move the caret. Without this, clicking
   while a background session held framework focus moved the cursor visibly but
   sent your keystrokes somewhere else.
2. **A background session cannot steal focus.** Every focus reconciliation is
   guarded on "am I the selected session?", so a child agent finishing work, a
   PTY changing state, or a prompt appearing in a background session cannot pull
   the keyboard away from the session you are looking at.

Within the focused session, focus goes to whichever component owns input right
now, in this order: a blocking permission prompt or question card → the
`/statusline` picker → the orchestration tab bar (if you focused it) → the
prompt. When a PTY program owns the terminal, the session view itself takes
focus and forwards your keystrokes to the program.

---

## 7.6 Working with the agent

### The transcript

The transcript renders the same canonical block list the GUI does: your prompts,
agent messages with markdown and syntax-highlighted code blocks, shell commands
with their output, file edits as diffs, plans, to-do lists, generic tool calls,
and child-agent sections. Long tool output collapses; headers can be clicked, or
toggled with `Ctrl-T`, to expand. When a PTY program switches to the alternate
screen (`vim`, `htop`, `less`), the block UI is replaced by that program's grid
rendered full-area, and restored when it exits.

### Permission prompts

When the agent asks to do something your settings do not auto-approve, a card
appears inline and takes focus. It offers **yes**, **no**, and an **Other**
footer row that lets you type replacement guidance instead of a plain yes/no.
`Enter` confirms, `Esc` rejects. Where the request has an editable body — most
importantly a shell command — `e` moves into it so you can edit the command
before approving; `Enter` saves the edit, `Esc` leaves the editor with **yes**
re-highlighted rather than cancelling the call (a second `Esc`, with the list
focused, cancels).

Auto-approve is toggled with `Ctrl-Shift-I`, `/auto-approve`, the statusline's
`▶▶` control, or started on with `--auto-approve`.

### Ask-question prompts

The agent's `ask_user_question` tool renders as a card with one page per
question, an option list, and an `Other…` row for free text. `←`/`→` or `Tab`
move between questions, `Enter` answers, `Shift-Enter` advances a multi-select
question, and `Ctrl-C` skips everything. **Auto-approve does not suppress
these** — Phosphor deliberately surfaces every question regardless of the
conversation's autoexecute mode.

### The slash-command palette

`/` opens a searchable palette of every command available on this surface.
Toggle commands show their current state inline — `/theme` renders as
`(currently auto: dark)`, and `/auto-approve`, `/natural-language-detection`
and `/vim-mode` show on/off. Commands that take no argument execute the moment
you select them; commands with arguments insert their text so you can finish
typing.

### The completions menu

`Tab` completes the command or path under the cursor and opens a popup when
there is more than one candidate. Unlike upstream Warp, Phosphor's TUI offers
completions in **both** agent and shell mode, not shell mode only. Completions
do not currently see per-session environment variables.

### Other inline menus

`/model` (model picker), `/profile` (execution profile), `/prompts` (saved
prompt library), `/skills` (skill picker), `/mcp` (server catalogue, with an
install flow that prompts for template variables), `/api-keys` (BYOP provider
keys), `/conversations` (history), `/fork-from` and `/rewind` (pick an exchange)
all render as the same inline menu above the prompt, driven by the input line as
a search field.

---

## 7.7 Slash commands: which surface gets what

Phosphor has a real split between the two front-ends. A command that is not
available on a surface is filtered out of that surface's palette entirely — you
will not see it and cannot invoke it.

### Available on both the GUI and the TUI

| Command | What it does |
|---|---|
| `/agent` | Start a new conversation |
| `/new` | Alias for `/agent` |
| `/init` | Generate or update an `AGENTS.md` file |
| `/model` | Switch the base agent model |
| `/api-keys` | Add, view, or clear a provider's API key |
| `/profile` | Switch the active execution profile |
| `/prompts` | Search saved prompts |
| `/skills` | Invoke a skill |
| `/conversations` | Open conversation history |
| `/compact` | Summarise the conversation to free context |
| `/compact-and` | Compact, then send a follow-up prompt |
| `/queue` | Queue a prompt to send after the agent finishes |
| `/fork` | Fork the conversation |
| `/fork-and-compact` | Fork, then compact the copy |
| `/fork-from` | Fork from a specific earlier query |
| `/rewind` | Rewind to an earlier point (reverting files edited this session) |
| `/orchestrate` | Spawn one or more local child agents |
| `/create-new-project` | Walk through creating a new coding project |
| `/export-to-clipboard` | Export the conversation to the clipboard as markdown |
| `/export-to-file` | Export the conversation to a markdown file |
| `/copy-debugging-id` | Copy the conversation's debugging id |
| `/usage` | Show how much of the model's context window this conversation uses |
| `/cost` | Show the conversation's cost at your configured provider rates |
| `/vim-mode` | Toggle vim mode in the composer |

### TUI-only

These have no GUI equivalent as a slash command — the GUI does the same job
through its settings window, menus or title bar.

| Command | What it does |
|---|---|
| `/statusline` | Open the statusline configuration picker |
| `/reset-statusline` | Restore the default statusline items and order |
| `/theme <auto\|light\|dark>` | Set the TUI colour theme |
| `/auto-approve` | Toggle auto-approve |
| `/natural-language-detection` | Toggle natural-language detection in the composer |
| `/mcp` | View and manage MCP servers |
| `/status` | Show session status |
| `/exit` | Exit Phosphor |
| `/view-logs` | Bundle the logs into a zip and reveal it |
| `/clear` | Clear the transcript and start a new conversation |

### GUI-only

Not offered in the TUI, because each one needs a window, an editor pane or a
file dialog that the TUI does not have.

| Command | Why it is GUI-only |
|---|---|
| `/add-mcp` | Opens the add-MCP pane. Use the TUI's `/mcp` instead. |
| `/open-mcp-servers` | Opens the MCP servers view |
| `/add-prompt` | Opens the new-prompt editor |
| `/add-rule` | Opens the new-global-rule editor |
| `/open-rules` | Opens the rules viewer |
| `/open-project-rules` | Opens `AGENTS.md` in the editor |
| `/open-file` | Opens a file in Phosphor's code editor |
| `/open-skill` | Opens a skill's markdown file in the editor |
| `/open-settings-file` | Opens `settings.toml` in the editor |
| `/open-code-review` | Opens code review |
| `/open-repo` | Switches to another indexed repository |
| `/plan` | Prompts the agent to research and produce a plan |
| `/rename-tab`, `/set-tab-color` | Act on GUI tabs |
| `/changelog` | Opens the changelog view *(off by default)* |
| `/docker-sandbox` | Creates a docker sandbox session *(off by default)* |
| `/index` | Indexes the codebase *(off by default)* |
| `/pr-comments` | Pulls GitHub PR review comments *(superseded by a skill; not registered by default)* |

> **`/api-keys` diverges from upstream deliberately.** Warp classifies it as
> TUI-only; Phosphor marks it as available on both surfaces because BYOP key
> management is the fork's whole identity. In practice the working
> implementation is the TUI's inline menu — see §7.13 for the caveat.

---

## 7.8 The statusline

The bottom line of the TUI is a configurable statusline. Items are drawn in a
configured order, grouped with `•` inside a group and `|` between groups, and
several of them are clickable.

Configure it with **`/statusline`**: a picker replaces the input box, `Enter`
toggles the highlighted item, `←`/`→` reorder it, `Esc` saves and closes,
`Ctrl-C` cancels. **`/reset-statusline`** puts it back to defaults. You can also
edit it directly under `[agents.statusline]` in `settings.toml`.

| Item (TOML name) | Shows | On by default |
|---|---|---|
| `auto_approve` | Clickable `▶▶` auto-approve toggle — muted when off, coloured when on | ✅ |
| `auto_queue` | Whether a `/queue`d follow-up prompt is pending | |
| `model` | The active model | ✅ |
| `working_directory` | The session's working directory, abbreviated | ✅ |
| `git_branch` | The current git branch | ✅ |
| `git_branch_status` | Branch plus upstream tracking (`⊢ main • ↑1 ↓2`). Supersedes `git_branch` when both are on | |
| `git_diff_status` | Files changed, additions, deletions | ✅ |
| `git_hub_pull_request` | The branch's GitHub PR as a clickable link, via the local `gh` CLI | |
| `context_window_usage` | Context-window occupancy, e.g. `18% context` | |
| `date` | e.g. `August 29, 2026` | |
| `time_12_hour` | e.g. `3:41pm` | |
| `time_24_hour` | e.g. `15:41` | |
| `agent_todo_list` | To-do progress (`❒ 2/5`, `✓ 5/5`); clicking it opens the to-do panel | |

Shell mode, vim mode and a few transient hints (loading a conversation, "press
Ctrl-C again to exit", "return control to the running command") also appear on
this line when relevant; they are not configurable items.

Date and time segments repaint themselves every 60 seconds without redrawing
the rest of the UI.

```toml
[agents.statusline]
order = ["model", "working_directory", "git_branch_status", "auto_approve", "context_window_usage"]
enabled = ["auto_approve", "model", "working_directory", "git_branch_status", "context_window_usage"]
```

Unknown or duplicated names are dropped on load, and any catalogue item you omit
from `order` is appended, so a partial list is safe.

---

## 7.9 Theming

TUI theming is deliberately much simpler than the GUI's, and it is a **separate
setting**: the TUI does not read the GUI's theme selection, and setting the TUI
theme does not change the GUI's.

| | GUI | TUI |
|---|---|---|
| Setting | `appearance.themes.theme` (+ `system_theme`, `selected_system_themes`) | `appearance.theme` |
| Value space | The full named-theme catalogue, including custom theme files | `auto`, `light`, `dark` |
| Where you set it | The settings window | `/theme <auto\|light\|dark>`, or `settings.toml` |

`auto` (the default) probes the host terminal's background colour with an OSC 11
query and picks the light or dark built-in theme from its luminance; it keeps
re-probing when the terminal regains focus, so switching your terminal's profile
mid-session is picked up. Choosing `light` or `dark` explicitly pins the choice
and stops further probing for that session. The `/theme` menu row shows what is
currently in force, e.g. `(currently auto: dark)`.

There are only those two palettes. The ANSI 16 colours the TUI renders terminal
output with come from them; the TUI's own chrome (borders, muted text, accents)
uses a fixed scheme-aware palette that is not user-configurable. **There is no
custom-theme support in the TUI** and no per-slot colour overrides.

---

## 7.10 Settings reference

All of these live in **`settings.toml`** in Phosphor's config directory
(`~/.config/phosphor/settings.toml` on Linux). The file is watched, so most
edits take effect immediately in a running TUI. None of these have a GUI
settings page.

| Key | Meaning | Type | Default |
|---|---|---|---|
| `appearance.theme` | TUI colour theme | `auto` \| `light` \| `dark` | `auto` |
| `appearance.zero_state.object` | Rotating zero-state object | `{ type = "built_in" }` or `{ type = "ascii_file", path = "…" }` | `{ type = "built_in" }` |
| `appearance.zero_state.rotation_period_seconds` | Seconds per full rotation (1.0–60.0) | float | `5.0` |
| `appearance.zero_state.extrusion_depth` | Extrusion half-depth (0.02–0.5) | float | `0.18` |
| `appearance.zero_state.show_changelog` | Show the "What's new" section | bool | `true` |
| `appearance.zero_state.show_project_info` | Show project path, rules and skills | bool | `true` |
| `appearance.zero_state.show_mcp` | Show the MCP section | bool | `true` |
| `appearance.zero_state.show_animation` | Show the rotating object and starfield | bool | `true` |
| `appearance.zero_state.freeze_animation_when_unfocused` | Stop animation repaints while the terminal is unfocused | bool | `false` |
| `agents.statusline` | Statusline `order` and `enabled` arrays (§7.8) | table | see §7.8 |
| `general.autoupdate_enabled` | Whether the TUI's background auto-updater runs (§7.11) | bool | `true` |

Out-of-range values for the two float settings are a load error, not a clamp;
Phosphor reports the failure in the footer and falls back to defaults rather
than writing over your file.

Everything else the TUI uses — models, providers, profiles, MCP servers, skills,
auto-approve policy — is the shared configuration described elsewhere in this
manual, and is edited from either front-end.

---

## 7.11 Autoupdate

`TuiAutoupdateSettings` exposes one key, `general.autoupdate_enabled` (default
`true`), and the environment variable `WARP_TUI_DISABLE_AUTOUPDATE` (set to any
value) disables updates for a single launch. The setting is read **once at
startup**, so changing it takes effect on the next run.

**In practice the updater never runs in Phosphor's shipped binary.** It only
activates for a release build installed into a managed `versions/…` + `current`
symlink layout, and the update endpoint it would poll is disabled at the channel
level. Phosphor ships plain tarballs and archives, so none of the preconditions
hold. Treat `general.autoupdate_enabled` as inert and update by downloading a
new release; the setting is documented here only because you will find it in the
schema.

---

## 7.12 When the host terminal goes away

If the terminal Phosphor's TUI is drawing to disappears — an SSH connection
drops, the terminal window is closed, the pty's master end goes away — **the TUI
exits cleanly**, with exit status zero and one line in the log. It does not keep
redrawing into a dead terminal, and it does not raise an error report, because a
vanished terminal is not a fault the program can act on. The same applies if the
terminal is already gone before the first frame is drawn.

A genuinely unexpected I/O failure is treated differently: it is reported and
exits non-zero.

This matters mostly for `ssh` users, who previously could leave an orphaned
process spinning on a broken pipe. If you are running the TUI over SSH and want
it to survive a disconnect, run it inside `tmux` or `screen` — the TUI has no
detach mechanism of its own.

---

## 7.13 Not available in Phosphor

Things a Warp user will look for in the TUI and not find. Each is a deliberate
decision, recorded in `DECLINED.md`.

| Missing | Why |
|---|---|
| **`--resume <token>` actually resuming anything** | The flag is live and parsed, but server conversation tokens are a Warp cloud concept; BYOP never produces one, and the loader that would consume one always returns nothing. The "to continue this conversation, run …" hint is therefore never printed either. |
| **`--api-key` / `WARP_API_KEY` doing anything** | It carried a Warp-account credential, and the release channel discards it before it reaches the auth state. Provider keys go through `--set-provider-api-key` or `/api-keys`. |
| **Sign-in, `/logout`, account status** | There is no Warp account. The login model is hardcoded to "logged in", and `/logout` is deliberately not registered rather than shown as a row that does nothing. `/status` correspondingly drops Warp's organisation and email rows. |
| **Credits, cost-in-dollars, and the billing pane** | No cloud credit accounting exists. `/usage` reports context-window occupancy, and `/cost` reports token spend at *your* configured provider rates. The statusline's `context_window_usage` item replaces Warp's clickable credits⇄cost entry and is informational only. |
| **Voice input in the composer** | The transcription backend is cloud, and it is turned off in this fork; the composer state machine and its statusline item were not ported. |
| **The hosted MCP gallery in `/mcp`** | Three of upstream's four MCP catalogue sources are local and are all present; the fourth is Warp's hosted gallery, which has no backend here. Team-shared MCP templates ("shared by a team member") go with it. |
| **Cloud agents, environments, docker-in-cloud, `/move-to-cloud`, remote control, `/continue-locally`** | The entire hosted-agent subsystem is absent. These command kinds exist in the enum but no command is ever registered for them. |
| **Agent-invoked agent spawning** | The model cannot decide to spawn agents. User-invoked `/orchestrate` is the supported route, and it spawns *local* children only. |
| **The orchestration configuration picker (harness/provider/auth-secret selection)** | It resolves through Warp's managed-secrets and cloud-catalogue services. `/orchestrate` uses a fixed local harness. |
| **A working auto-updater** | See §7.11 — inert by construction, not by a toggle. |
| **Remapping TUI keys via `keybindings.yaml`** | The binding *names* are stable and registered; loading user overrides in the TUI process is not implemented yet. |
| **Creating tabs, panes, or extra sessions** | The TUI has one foreground session. Background sessions exist only as `/orchestrate` children. |
| **Custom TUI themes** | Two built-in palettes only (§7.9). |
| **Codebase search / semantic indexing in the TUI** | `/index` is GUI-only and off by default; the indexing runtime it drove was removed. |

### Two rough edges in this release

* **`/api-keys` appears in the GUI palette but has no GUI handler.** It is
  marked as supported on both surfaces, and the TUI implementation is complete,
  but selecting it in the GUI falls through to the no-handler path and does
  nothing. Use it from the TUI, or Settings → AI → Agent providers in the GUI.
* **The `?` shortcuts sheet lists "toggle auto-approve" twice.** Cosmetic; the
  binding itself is fine.

<!-- SOURCES
Binary name / launch
- crates/warp_tui/Cargo.toml:1-20 (package name warp_tui; autobins=false; default-run and [[bin]] name = "zap-tui-oss", path src/bin/oss.rs)
- crates/warp_tui/src/bin/oss.rs:12-54 (single OSS bin; AppId dev.phosphor.Phosphor shared with GUI; display_name "Phosphor"; logfile phosphor-tui.log; autoupdate_config: None)
- crates/warp_tui/src/session.rs:41-51 (CLI_NAME = "phosphor-tui"; comment: cargo bin is zap-tui-oss, release jobs copy it to phosphor-tui)
- .github/workflows/phosphor_release.yml:408-434, 808-832, 890-912 (builds --bin zap-tui-oss, renames to phosphor-tui, packages phosphor-tui-{linux-x86_64.tar.gz,macos-<arch>.tar.gz,windows-x64.zip})
- README.md:141-149 (TUI overview; cargo run -p warp_tui), README.md:230-236 (zap-tui-oss deliberately not renamed)
- Verified by running the already-built debug binary: `./target/debug/zap-tui-oss --help` (full flag list reproduced in 7.1) and via a `phosphor-tui` symlink, which shows clap uses argv[0] for the usage line.

Flags
- crates/warp_tui/src/session.rs:56-91 (TuiArgs: --resume, --auto-approve, --api-key with env WARP_API_KEY + hide_env_values, --set-provider-api-key / --clear-provider-api-key with conflicts_with_all)
- app/src/lib.rs:1351-1366 (the Tui launch mode's api_key is taken only when ChannelState::channel().is_dogfood(), then further gated on FeatureFlag::APIKeyAuthentication, and it feeds AuthState — Warp-account auth, not a provider key)
- crates/warp_core/src/channel/mod.rs:30-35 (Channel::Oss is NOT dogfood, so the shipped binary discards --api-key)
- crates/warp_tui/src/session.rs:33-39 (CLI_VERSION), :140-155 (--version prints bare tag; --help)
- crates/warp_tui/src/session.rs:156-235 (Xai rejected at runtime for both key flags with the "add an xAI key as a custom agent provider" message; masked TTY prompt / piped stdin; notify_tui_api_keys_changed so running processes reload)
- crates/warp_tui/src/session.rs:100-121 (read_provider_api_key: masked prompt when stdin is a tty, else read stdin)
- crates/warp_tui/src/session.rs:236-248 (--auto-approve -> RunToCompletion; comment: does not suppress ask_user_question, DECLINED #373)
- DECLINED.md:165 (#588: --api-key hide_env_values diverges from the pin deliberately)

Runtime / driver
- crates/warpui_core/src/runtime/mod.rs:1-22 (alternate screen + raw mode, restored on drop; keymap-then-element dispatch)
- crates/warpui_core/src/runtime/mod.rs:32-40 (EnableBracketedPaste, EnableFocusChange, EnableMouseCapture, PushKeyboardEnhancementFlags)
- crates/warpui_core/src/runtime/mod.rs:1221 (terminal restored on drop, so a panic never strands it)
- crates/warp_tui/src/session.rs:135-145 (run(): version lease, worker re-exec dispatch)
- crates/warp_cli/src/lib.rs:1-33 (separate crate: the GUI binary's CLI/subcommands; the TUI does not use it for its own args)

No login
- app/src/tui/mod.rs:28-31, :73-80 (BYOP: only TuiLoginPhase::LoggedIn ever occurs; singleton registered always-LoggedIn)
- crates/warp_tui/src/session.rs:379-382 (already authenticated at mount -> create the first session now)
- crates/warp_tui/src/session.rs:409-437 (create_terminal_session_after_login: one focused session, std::env::current_dir())

Zero state
- crates/warp_tui/src/zero_state.rs:1-27 (title+version, What's new, project context, rotating object over starfield; session view owns visibility; returns when transcript empties)
- crates/warp_tui/src/zero_state.rs:62 (MAX_CHANGELOG_BULLETS = 3)
- crates/warp_tui/src/zero_state.rs:81-121 (ZeroStateSectionVisibility: changelog/project_info/mcp/animation; title+version have no toggle; signed-in-user line dropped)
- app/src/settings/tui_zero_state.rs:143-225 (all appearance.zero_state.* keys and defaults; object default built_in; rotation 5.0; extrusion 0.18; show_* true; freeze false)
- app/src/settings/tui_zero_state.rs:17-22, :53-100, :113-124 (ranges 1.0-60.0 and 0.02-0.5; out of range is a load error, not a clamp)
- crates/warp_tui/src/zero_state_animation_config.rs:9, :370-388 (ascii_file relative paths resolve against config_local_dir)
- crates/warp_tui/src/session.rs:326-360 (freeze_animation_when_unfocused read at start and live-updated)
- crates/warp_tui/src/input_hints.rs:10-60 (ghost hint strings and which appear when)

Input modes and triggers
- crates/warp_tui/src/input_mode_policy.rs:10-52 (agent-first; SHELL_LOCKED_CONFIG; autodetection driven by the AI setting)
- crates/warp_tui/src/input/view.rs:766-802 ('?' toggles the shortcuts sheet only on an empty idle input at offset 0; '!' enters shell mode at start of input)
- crates/warp_tui/src/input/view.rs:876-905 ('left' on empty agent input opens ConversationMenu; 'up' on first row opens PromptAndCommandHistory; backspace at start exits shell mode)
- crates/warp_tui/src/input/view.rs:818-821 (SelectUp / shift-up emits MoveFocusUp -> orchestration tabs)
- crates/warp_tui/src/input/view.rs:1344-1387 (handle_escape order: read-only menu, inline menu, vim, shell mode)
- crates/warp_tui/src/input_suggestions_mode.rs:11-40 (one mode at a time; full list of inline-menu modes incl. ReadOnlyMenu(Shortcuts|Status|Todos))
- crates/warp_tui/src/read_only_menu.rs:16-22 (TuiReadOnlyMenuKind::{Shortcuts, Status, Todos})
- crates/warp_tui/src/terminal_session_view/shortcuts.rs:22-26 ('?' sheet; status info deliberately lives in /status)
- crates/warp_tui/src/terminal_session_view/state.rs:529-650 (the shortcut sections, incl. the collapsed forms while a command runs or a blocker is up; "toggle auto-approve" pushed at :586-592 AND :598-604 -> duplicate row)
- crates/warp_tui/src/terminal_session_view/state_tests.rs:230 (test only asserts `contains`, so the duplicate is not caught)

Keybindings
- crates/warp_tui/src/keybindings.rs:1-20 (tui:* names are the stable contract; "loading overrides in the TUI process is a follow-up"), :74-95 (cmd- chords legitimately cross-platform in the TUI), :96-130 (init order)
- crates/warp_tui/src/input/view.rs:85-92 (same "once the TUI loads overrides" note)
- app/src/keyboard.rs:35, :102 (keybindings.yaml lives in config_local_dir; only the app crate reads it)
- crates/warp_tui/src/root_view.rs:36-43 (ctrl-c ExitApp on the root)
- crates/warp_tui/src/terminal_session_view.rs:770-912 (ctrl-d Eof; escape CancelRestore; ctrl-shift-I auto-approve; ctrl-shift-enter attach; escape detach; ctrl-shift-P plan; ctrl-p contextual plan; tab focus-attachments vs trigger-completions; ctrl-v / alt-v paste)
- crates/warp_tui/src/terminal_session_view.rs:239-245 (binding-name constants)
- crates/warp_tui/src/tui_cli_subagent_view.rs:29-42 (TAKE_CONTROL ctrl-c, HAND_BACK ctrl-g, ALLOW ctrl-o, REJECT ctrl-r)
- crates/warp_tui/src/editor_interaction.rs:161-375 (SHARED_EDITOR_BINDINGS: every editor command and its default keys; MoveUp/MoveDown/SelectUp/SelectDown are input-only; ctrl-k comment about cmd-delete)
- crates/warp_tui/src/keybindings.rs:158-170 (ctrl-d DeleteForward suppressed on the input; ctrl-p MoveUp gated by the plan/keyboard-enhancement flags)
- crates/warp_tui/src/input/view.rs:93-121 (tui:input:submit enter; handle_escape escape; MCP logout ctrl-r)
- crates/warp_tui/src/option_selector.rs:56-77 (up/down)
- crates/warp_tui/src/tui_permission_prompt.rs:31-59 (escape CancelOrBack fixed; enter confirm; 'e' edit body)
- crates/warp_tui/src/tui_shell_command_view.rs:46-89 (escape saves the command edit rather than cancelling; ctrl-t toggle expanded; enter/numpadenter save)
- crates/warp_tui/src/tui_file_edits_view.rs:65-90 (ctrl-t toggle primary section; 'e' expand/collapse all diffs while the permission card is focused)
- crates/warp_tui/src/tui_ask_question_view.rs:34-86 (ctrl-c SkipAll; enter; shift-enter multiselect; left/right/tab navigation)
- crates/warp_tui/src/attachment_bar/view.rs:63-129 (tab/right next, shift-tab/left previous, backspace/delete remove, escape/enter return focus)
- crates/warp_tui/src/orchestration_tab_bar.rs:180-247 (ctrl-c interrupt; left/right row nav; tab/shift-tab tree nav; shift-left/right first/last child)
- crates/warp_tui/src/terminal_session_view.rs:913-942 (down/shift-down focus input; escape focus main tab)
- crates/warp_tui/src/statusline_config_view.rs:29-71 (ctrl-c cancel; enter toggle; escape save; left/right reorder)
- crates/warp_tui/src/exit_confirmation.rs:1-14 (double ctrl-c, CTRL_C_EXIT_WINDOW = 1s)
- crates/warp_tui/src/terminal_session_view.rs:3195-3301 (handle_interrupt precedence: cancel restore, dismiss read-only sheet, terminal-use interrupt, kill focused child tab, arm child-kill window, exit window, cancel conversation, clear input; handle_eof exits on empty prompt)
- crates/warp_tui/src/input/view.rs:1354-1380, :913-925 (vim mode: Esc is a vim command first; Esc Esc leaves shell mode from Normal; clicking forces Insert)
- crates/warpui_core/src/runtime/event_conversion.rs:38-39 (wheel up/down); crates/warp_tui/src/terminal_session_view.rs:597, :5734 (InlineMenuMouseScrollBy)

Sessions and focus
- crates/warp_tui/src/session_registry.rs:1-6, :468-495 (registry owns lifetime and focus; only the focused session is rendered/routed)
- crates/warp_tui/src/pane_group.rs:1-46 (a "pane" in the TUI is a never-focused session, reachable only through the orchestration tab bar; /orchestrate materializes children; UI trigger is execute_tui_slash_command)
- crates/warp_tui/src/session_registry.rs:242-258 (create_restored_local_child_session: unfocused)
- crates/warp_tui/src/session.rs:409-437 (the one bootstrap session; no other creation path in the TUI)
- crates/warp_tui/src/input/view.rs:712-729 (click/drag in the prompt emits FocusRequested — comment explains the background-session focus bug it fixes)
- crates/warp_tui/src/terminal_session_view.rs:2100 (FocusRequested -> reconcile_focus)
- crates/warp_tui/src/terminal_session_view.rs:972-993 (focus_current_owner order: blocking child, statusline picker, orchestration tabs, input; PTY takes self-focus)
- crates/warp_tui/src/terminal_session_view.rs:994-1003 (focus_current_owner_if_active), :1005-1010 ("The is_focused_session guard ... is what stops a background session from pulling focus off the visible one")
- crates/warp_tui/src/terminal_session_view.rs:1163-1194 (kill_child_agent: cancel + delete + drop sessions + return focus to root)
- git log e287977f0 "port(tui): terminate on host-terminal disconnect and harden focus ownership"

Transcript / alt screen / permission cards
- crates/warp_tui/src/transcript_view.rs:1 (canonical terminal block-list order)
- crates/warp_tui/src/alt_screen_view.rs:1-14 (full-screen PTY apps render full-area instead of the block UI)
- crates/warp_tui/src/tui_permission_prompt.rs:100-140 (yes / no rows + "Other" custom-text footer, yes preselected)
- crates/warp_tui/src/tui_ask_question_view.rs:199-210 (option labels + "Other…" custom-text row)
- crates/warp_tui/src/terminal_session_view/completions.rs:1-14 (not gated to shell mode here, unlike the pin; no per-session env vars threaded in)
- crates/warp_tui/src/slash_commands.rs:307-336 (state suffix: /theme "(currently auto: dark)", /auto-approve, /natural-language-detection, /vim-mode)
- app/src/terminal/input/slash_commands/mod.rs:89-99 (Argument::should_execute_on_selection decides execute-vs-insert)
- crates/warp_tui/src/mcp_install_flow.rs, crates/warp_tui/src/input_suggestions_mode.rs:23-25 (McpInstall collects a template variable)

Slash-command split
- app/src/search/slash_command_menu/static_commands/mod.rs:329-331 (supports_gui = !is_tui_only), :344-358 (is_tui_only: the 10 TUI-only names), :362-403 (supports_tui)
- app/src/search/slash_command_menu/static_commands/commands.rs:696-831 (all_commands registry and per-command feature gates)
- app/src/terminal/input/slash_commands/data_source/mod.rs:317-319, :331, :341-350 (surface filter; is_tui_surface)
- crates/warp_tui/src/terminal_session_view.rs:4359-4371 (execute_tui_slash_command with the supports_tui gate), :4376-4845 (the per-kind arms), :4846-4882 (GUI-only catch-all debug_assert)
- app/src/terminal/input/slash_commands/mod.rs:1207-1223 (GUI's reciprocal TUI-only guard), :1224-1231 (no-handler catch-all that /api-keys falls into)
- commands.rs:313-324 and mod.rs:208-212 (/api-keys is fork-native and marked both-surfaces); commands.rs:895-914 (upstream calls it TUI-only; this fork deliberately does not)
- commands.rs:21-28, :1105-1143 (/add-mcp GUI-only: "has no TUI implementation; offering it there would dead-end")
- commands.rs:117-137 (/statusline and /reset-statusline TUI-only, with the GUI-guard note)
- commands.rs:718-800 (feature gates: LocalDockerSandbox, Changelog, FullSourceCodeEmbedding, PRComments* — the four that are off in a default build)
- DECLINED.md:86 (/logout deliberately not registered — "BYOP has no account to log out of")
- crates/warp_tui/src/terminal_session_view.rs:4653-4671 (/compact, /plan, /init round-trip as a prompt), :4790-4835 (/orchestrate on the Other arm)

Statusline
- app/src/settings/ai.rs:2017-2031 (toml_path "agents.statusline", type TuiStatuslineConfig)
- app/src/settings/ai.rs:599-693 (TuiStatuslineItem catalogue, ALL, labels; CreditUsage dropped for BYOP; AutoQueue re-semanticised to "a /queue'd follow-up is pending"; GitBranchStatus supersedes GitBranch; GitHubPullRequest via local gh)
- app/src/settings/ai.rs:705-745 (defaults: order = all 13; enabled = auto_approve, model, working_directory, git_branch, git_diff_status; normalized() drops unknown/duplicate and appends missing)
- app/src/settings/ai_tests.rs:47-61 (asserts those defaults)
- crates/warp_tui/src/terminal_session_view/statusline.rs:1-8 (fork-owned resolution order/separators), :33-66 (60s datetime repaint), :68-110 (segment list incl. Vim, ShellMode, AutoApproveIndicator, ContextWindowUsage, AgentTodoList, GitHubPullRequest), :112-125 (two-tier separators)
- crates/warp_tui/src/usage.rs:1-14, :20-25 (context-window percentage replaces credits/cost; not clickable, no persisted display mode)
- crates/warp_tui/src/terminal_session_view.rs:4887-4942 (open_statusline_config and save), :4944-4948 (reset_statusline)
- TOML value names: settings persist through SettingsValue::to_file_value (crates/settings/src/lib.rs:543), whose derive snake-cases variant names with convert_case 0.7 (crates/settings_value_derive/src/lib.rs:363-395; Cargo.toml:12). convert_case 0.7 Boundary::defaults() includes LOWER_DIGIT and DIGIT_UPPER, so Time12Hour -> time_12_hour, Time24Hour -> time_24_hour, GitHubPullRequest -> git_hub_pull_request. (Note: plain serde snake_case would give time12_hour; serde is not the path that writes settings.toml.)

Theme
- app/src/settings/tui_theme.rs:39-48 (TuiTheme auto|light|dark, Auto default), :59-69 (resolves to the two built-in light/dark themes), :88-95 (inferred_color_scheme for the "currently auto: dark" report), :97-113 (toml_path "appearance.theme")
- app/src/settings/theme.rs:14-47 (GUI: appearance.themes.theme / system_theme / selected_system_themes — a different key and a different value space)
- crates/warp_tui/src/session.rs:306-319 (TUI overrides Appearance at mount, deliberately without changing GUI theme selection)
- crates/warp_tui/src/terminal_background.rs:7-11, :76-82, :104-113, :198-199 (Auto probes the host background via OSC 11 and re-probes on focus; an explicit choice stops probing)
- crates/warp_tui/src/tui_builder.rs:53-90 (fixed scheme-aware chrome palette), :143-380 (ANSI colours read from the resolved theme)
- commands.rs:139-149 (/theme is TUI-only; the GUI has its own chooser); crates/warp_tui/src/terminal_session_view.rs:4843-4845 (dispatch), :5033+ (toggle_theme persists it)

Settings file location
- app/src/settings/mod.rs:643-654 (user_preferences.json vs settings.toml)
- app/src/settings/init.rs:88-90 (the three TUI groups registered), :308-321 (file watched, hot-reloaded), :539-570 (settings.toml used when FeatureFlag::SettingsFile is on; settings_file is in app/Cargo.toml default)
- crates/warp_core/src/paths.rs:113-157 (config_local_dir per platform)
- crates/settings/src/macros.rs:273-295 (toml_path is the literal dotted key)
- ed6d3597c "port(tui): surface settings load failures as footer error hints"

Autoupdate
- app/src/settings/tui_autoupdate.rs:8-21 (general.autoupdate_enabled, bool, default true, DESKTOP, TUI-only)
- crates/warp_tui/src/autoupdate.rs:1-27 (managed versions/current layout; opt-out setting + WARP_TUI_DISABLE_AUTOUPDATE; cargo-run builds unaffected), :49 (DISABLE_ENV_VAR), :71 (CHECK_INTERVAL = 10 min), :277-308 (eligibility read once at startup; disabled reasons)
- crates/warp_tui/src/bin/oss.rs:46 (autoupdate_config: None)
- crates/warp_core/src/channel/config.rs:30 and crates/warp_core/src/channel/state.rs:187-195 (server_root_url returns the DISABLED_HTTP_SENTINEL http://192.0.2.0:9)
- DECLINED.md:217 ("TUI autoupdate — DECIDED 2026-08-17: not shipped… inert by construction, not by a togglable setting"; and the warning not to conflate it with the GUI's FeatureFlag::Autoupdate)

Host terminal disconnect
- crates/warpui_core/src/runtime/mod.rs:138-172 (is_terminal_disconnect: BrokenPipe, ConnectionAborted/Reset, NotConnected, UnexpectedEof, unix EIO/ENXIO, windows 233)
- crates/warpui_core/src/runtime/mod.rs:175-207 (fail_tui_driver: latch once, cancel the repaint timer, log-and-exit-zero on disconnect, report+non-zero otherwise)
- crates/warpui_core/src/runtime/mod.rs:1070-1076 (failed latch stops further draws)
- crates/warp_tui/src/session.rs:389-406 (handle_tui_driver_startup_error: TerminalDisconnected before the first frame exits cleanly with no termination result)

Not-available table
- DECLINED.md:224 (tui_cli_shell_command / tui_resume_shell_command: no server tokens; load_conversation_by_server_token hardcoded to None)
- crates/warp_tui/src/session.rs:249-262 (the --resume hint is currently unreachable for exactly that reason)
- DECLINED.md:86 (/logout); commands.rs:523-526 (/status drops org/email)
- DECLINED.md:215 (provider-cost baselines; footer answers context, not cost); crates/warp_tui/src/usage.rs:1-14
- DECLINED.md:122, :139-146 (voice input UI incl. the TUI composer and statusline item)
- DECLINED.md:90 (MCP gallery + team-shared templates)
- DECLINED.md:223 (orchestration config-picker layer is cloud), :229 (agent-invoked RunAgents declined; /orchestrate is the user-invoked local route)
- DECLINED.md:217 (autoupdate); DECLINED.md:174 (TUI/GUI share one app id — why one config and one keychain)
- commands.rs:208-228 (/index gate), app/src/lib.rs:3422-3425 (FullSourceCodeEmbedding is unstable-only)
- app/src/search/slash_command_menu/static_commands/mod.rs:119-216 (kinds with no registered command: CloudAgent, Logout, CreateEnvironment, Host, Harness, Environment, MoveToCloud, ContinueLocally, RemoteControl, …)

Rough edges reported to the coordinator
- /api-keys in the GUI: supports_gui() true (mod.rs:329-331 + absence from is_tui_only at :344-358) but no arm in app/src/terminal/input/slash_commands/mod.rs:509-1215, so it lands on the no-handler catch-all at :1224-1231.
- Duplicate "toggle auto-approve" row: crates/warp_tui/src/terminal_session_view/state.rs:586-592 and :598-604.
-->
