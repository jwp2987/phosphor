# 2. Terminal basics

Phosphor is a block-based terminal. Instead of one continuous stream of
characters, everything you run is grouped into a **block** — the command, its
output, its exit status and its metadata, kept together as one selectable,
copyable, searchable unit. Around that sit the pieces you interact with all
day: the input editor at the bottom (or top), panes and tabs, find, mouse
selection, and the renderer. This chapter covers all of them, with the real
setting names, defaults and key bindings taken from the source.

Two things to know before you start:

- **Settings file.** Everything with a TOML path below can be set by editing
  `settings.toml`. It lives at `~/.phosphor/settings.toml` on macOS,
  `~/.config/phosphor/settings.toml` on Linux/FreeBSD, and
  `%LOCALAPPDATA%\phosphor\Phosphor\config\settings.toml` on Windows. It is
  hot-reloaded, and invalid values are reported rather than silently ignored.
- **Key bindings file.** Custom key bindings go in `keybindings.yaml` in the
  *same* directory. Unlike `settings.toml`, it is read **only at startup** —
  restart Phosphor after editing it by hand. The GUI editor is at
  **Settings → Keybindings**.

---

## Blocks

### What a block is

A block is one command and everything that came with it. Running `ls` produces
a block containing the command line you typed, the output, the exit code, the
working directory and a timestamp. The next command starts a new block. This
grouping is what makes "copy just the output", "search inside this command",
"scroll to the top of this command" and "select these three commands" possible
at all.

Block grouping depends on **shell integration** (Phosphor's shell hooks, which
tell it where a prompt begins and a command ends). Phosphor installs these into
your shell automatically for local sessions. Over SSH, the same integration is
injected by the tmux wrapper — see **Settings → Phosphorize**. When integration
is not present, output cannot be split into blocks reliably and the terminal
behaves closer to a conventional emulator.

### Selecting blocks

| What you want | How |
|---|---|
| Select the previous / next block | `cmd-↑` / `cmd-↓` (macOS), `ctrl-↑` / `ctrl-↓` (Linux, Windows) |
| Extend the selection up / down | `shift-↑` / `shift-↓` |
| Select every block | `cmd-A` on macOS. **No default binding on Linux/Windows** — deliberately, so that `ctrl-A` stays available to the shell |
| Clear all blocks | `cmd-K` / `ctrl-shift-K` |
| Open the block context menu | `ctrl-M` (macOS only) |

By default, clicking a block moves focus out of the input. Set
`general.preserve_input_focus_on_block_selection = true` to keep typing focus in
the input while you select blocks.

### What you can do with a selected block

| Action | macOS | Linux / Windows |
|---|---|---|
| Copy command **and** output | `cmd-C` | `ctrl-shift-C` |
| Copy output only | `cmd-alt-shift-C` | `ctrl-alt-shift-C` |
| Copy command only | `cmd-shift-C` | *(unbound — would collide with copy)* |
| Find within the selected block | `cmd-F` | `ctrl-shift-F` |
| Bookmark / un-bookmark | `cmd-B` | `ctrl-shift-B` |
| Jump to the closest bookmark up / down | `alt-↑` / `alt-↓` | `alt-↑` / `alt-↓` |
| Scroll to top of the block | `cmd-shift-↑` | `ctrl-shift-↑` |
| Scroll to bottom of the block | `cmd-shift-↓` | `ctrl-shift-↓` |
| Re-enter the command in the input | *(unbound)* `terminal:reinput_commands` |
| Re-enter the command with `sudo` | *(unbound)* `terminal:reinput_commands_with_sudo` |

### Filtering a block's output

`terminal:toggle_block_filter_on_selected_or_last_block` (`shift-alt-F`, macOS
only by default) opens a filter bar scoped to one block. It has regex,
case-sensitivity and invert-match toggles, and hides non-matching lines rather
than just highlighting them — the terminal equivalent of piping through `grep`
after the fact.

> **There is no "collapse block" feature.** Warp users look for one; Phosphor
> has no collapse/expand action, no setting and no binding for it. Output
> filtering (above) and `general.snackbar_enabled` (the sticky command header)
> are the closest things.
>
> There is also **no delete-a-single-block** and **no re-run** action. `cmd-K` /
> `ctrl-shift-K` clears the whole list; to run a command again, use "reinput"
> from the block menu, which puts the command back in the input for you to edit
> and submit.

### The special blocks (`BlockVisibilitySettings`)

Some blocks are not your commands. Three of them are hidden by default:

| Block | What it is | TOML path | Default |
|---|---|---|---|
| Bootstrap / initialization block | The shell-integration bootstrap Phosphor runs when a session starts. Useful when debugging a shell that will not phosphorize | `appearance.blocks.should_show_bootstrap_block` | `false` |
| In-band command blocks | Internal commands Phosphor runs inside your session (completions generators and similar) | `appearance.blocks.should_show_in_band_command_blocks` | `false` |
| SSH connection block | The status block for an SSH connection being set up | `appearance.blocks.should_show_ssh_block` | `false` |

All three settings take effect in a normal build — the terminal subscribes to
them unconditionally. What *is* restricted is where you can flip them from the
UI:

- The **Blocks** menu (macOS only — Phosphor has no menu bar on Linux or
  Windows) carries "Show in-band command blocks" and "Show phosphorized SSH
  blocks". "Show initialization block" is behind a feature flag that is dark in
  every build this repository produces.
- **Settings** shows toggles for the bootstrap and in-band blocks only in a
  debug build, on the `Local`/`Dev` channels, or on Windows.
- On every platform, editing `settings.toml` works.

### `SameLinePromptBlockSettings`

`SameLinePromptBlockSettings` is not a preference and you cannot set it. It is a
one-bit record (`NotShown` / `Shown` / `DoNotShow`) of whether Phosphor has
already shown you the "same line prompt" onboarding block, so that the block is
shown at most once. It is private, has no TOML path, and does not appear in
`settings.toml`.

**In Phosphor it is inert.** The setting is registered but nothing in the tree
reads or writes it — the onboarding block it was tracking was never ported. It
will stay `NotShown` forever. It is documented here only because you may see the
name and wonder.

"Same line prompt" itself — drawing the prompt and the input on one line instead
of stacked — is a real prompt option, but its checkbox is currently unrendered
dead code (tracked as issue #555); the backing state machine works, so it can
only be reached by editing configuration today. It is mutually exclusive with
using your shell's `PS1`.

The related setting you *can* change freely is `terminal.input.honor_ps1`
(default `false`): turn it on to render your shell's own `PS1` prompt instead of
Phosphor's.

### Other block appearance settings

| Setting | What it does | TOML path | Default |
|---|---|---|---|
| Block dividers | Draws a rule between blocks | `appearance.blocks.show_block_dividers` | `true` |
| Jump-to-bottom button | Shows a button in long output to jump to the end | `appearance.blocks.show_jump_to_bottom_of_block_button` | `true` |
| Sticky command header | Pins the command line of the block you are scrolled into | `general.snackbar_enabled` | `true` |
| Spacing | `normal` or `compact` block spacing (**View → Compact Mode**) | `appearance.spacing` | `normal` |
| Scrollback limit | Maximum rows kept in the terminal grid | `terminal.maximum_grid_size` | `50000` |
| AI zero-state block | The AI prompt block shown in a fresh session | `terminal.show_terminal_zero_state_block` | `true` |

---

## The input editor

### Where the input sits (`InputModeSettings`)

`appearance.input.input_mode` controls the layout of the block list relative to
the input:

| Value | Behaviour |
|---|---|
| `pinned_to_bottom` **(default)** | Newest blocks at the bottom, input at the bottom. Conventional. |
| `pinned_to_top` | Newest blocks at the top; the block list is inverted. |
| `waterfall` | The input starts at the top of the pane and is pushed down as commands accumulate. |

> A source comment claims new users are defaulted to `waterfall` by the settings
> initializer. That is stale: nothing in this tree reads the
> `DefaultWaterfallMode` flag, so the effective default really is
> `pinned_to_bottom` for everyone.

### Which input box (`InputSettings.input_box_type_setting`)

`terminal.input.input_box_type_setting` picks between `classic` (terminal-first)
and `universal` (AI-first). **The default is `classic`.** Phosphor contains code
that would flip new users to `universal`, but it sits behind an onboarding check
that is permanently false in this fork, so it never runs.

### Editing

The input is a real multi-line editor, not a single-line readline.

- `enter` runs the command. `shift-enter` and `ctrl-J` insert a newline instead.
- Multiple cursors: `ctrl-shift-↑` / `ctrl-shift-↓` add a cursor above/below,
  `ctrl-G` adds a cursor at the next occurrence of the selection.
- `cmd-Z` / `ctrl-Z` undo, `cmd-shift-Z` / `ctrl-shift-Z` redo.
- `ctrl-C` clears the input when there is nothing running; it cancels the
  running process when there is.
- `ctrl-L` clears the screen.

### History and suggestions

- **`↑` recalls the previous command.** There is no "history next" binding —
  `↓` is ordinary cursor movement.
- `ctrl-R` opens **Command Search**, which searches history, workflows,
  notebooks and environment variables together. (`input:search_command_history`,
  which opens it pre-filtered to history only, ships unbound.)
- **Autosuggestions** (the greyed-out completion of what you are typing) are on
  by default (`terminal.input.autosuggestions.enabled`). Accept the whole
  suggestion with `→`, `End`, or `ctrl-E`; accept one word with `alt-→` /
  `ctrl-→` or `meta-F`. There is no default key bound to the dedicated
  `editor_view:insert_autosuggestion` action — the **Settings → Features → Tab
  key behaviour** dropdown is what rebinds `tab` between "open completions menu"
  (the default) and "accept autosuggestion", and it does so by writing a custom
  binding into `keybindings.yaml`.
- `tab` opens the completions menu by default. Set
  `terminal.input.completions_open_while_typing = true` to have it open as you
  type instead.
- `meta-.` inserts the last word of the previous command.

### Vim mode

Vim keybindings in the editor are a real setting, off by default:

| Setting | TOML path | Default |
|---|---|---|
| Vim keybindings | `text_editing.vim_mode_enabled` | `false` |
| Vim unnamed register uses the system clipboard | `text_editing.vim_unnamed_system_clipboard` | `false` |
| Show the Vim status bar | `text_editing.vim_status_bar` | `true` |

It is a genuine modal implementation, not a token one: normal/insert/visual/
replace modes, counts, operators (`d`, `c`, `y`, `>`, `<`), text objects
(`iw`, `a"`, `i(` …), `f`/`t` character search, registers and dot-repeat. It
applies to the terminal input editor, the AI composer and the code editor.

Two gotchas:

- `ctrl-[` exits insert mode; `ctrl-R` is redo in normal mode (Command Search is
  suppressed there so vim wins).
- **`ctrl-D` / `ctrl-U` half-page scroll only work in the code editor.** The
  vim engine defines the actions, but the terminal input editor does not
  implement them, so they do nothing there. Use `pageup` / `pagedown` to scroll
  terminal output.

`VimBannerSettings` is not the toggle — it only records whether you dismissed
the "did you mean to use vim keybindings?" banner.

### Emacs bindings

**Emacs-style editing keys are always on. There is no setting to enable or
disable them**, and `EmacsBindingsSettings` is not one: it is a Linux-only
record of whether you dismissed a banner. That banner appears the first time you
press `ctrl-E` on Linux and asks "did you intend `ctrl-A`/`ctrl-E` to move the
cursor?", offering *Yes, use Emacs-style bindings* / *No, keep IDE bindings*.

The always-available emacs keys in the input editor:

| Key | Action | Key | Action |
|---|---|---|---|
| `ctrl-A` | Start of line *(macOS only — on Linux/Windows `ctrl-A` is Select All)* | `ctrl-E` | End of line *(macOS only)* |
| `ctrl-B` / `ctrl-F` | Character left / right | `ctrl-P` / `ctrl-N` | Line up / down |
| `meta-B` / `meta-F` | Word left / right | `meta-A` / `meta-E` | Paragraph start / end |
| `meta-shift-<` / `meta-shift->` | Buffer start / end | `meta-.` | Insert last word of previous command |
| `ctrl-H` | Backspace | `ctrl-D` | Delete forward |
| `ctrl-W` | Cut word left | `alt-D` | Cut word right |
| `ctrl-K` | Cut to end of line | `ctrl-U` | Clear line and copy it |
| `ctrl-Y` | Yank | `ctrl-J` | Insert newline |
| `shift-ctrl-B`/`F`/`P`/`N`/`A`/`E` | Selecting variants *(macOS)* | `shift-meta-B` / `shift-meta-F` | Select word left / right |

On Linux and Windows the `ctrl-A` / `ctrl-E` line-start/line-end bindings are
deliberately **not** registered, because `ctrl-A` is Select All there and both
are control characters the shell needs. `Home` and `End` do the job instead.

### The `#` AI command search trigger

Typing `#` as the **first character** of an empty terminal input opens **AI
command search** — you describe what you want in English and Phosphor proposes a
shell command. This is gated by:

| Setting | TOML path | Default |
|---|---|---|
| `#` opens AI command search | `terminal.input.enable_ai_command_search_hash_trigger` | `true` |

It is also gated on the master AI switch (`agents.warp_agent.is_any_ai_enabled`,
default on). It is **not** gated on you having configured a provider: on a fresh
bring-your-own-provider install with no API key, `#` still opens the panel. The
"translate into a shell command" row simply rewrites your input to `# <query>`
without calling anything, and handing the query to the agent gets you the model
picker's "No custom provider configured — add one in Settings → AI" placeholder.
The toggle lives at **Settings → AI**, in the input section.

**How to type a literal `#`.** This trips people up, because `#` at the start of
a line is also a shell comment. Any of these work:

1. **Type `#`, then press `Escape` immediately.** The search closes and the `#`
   is *kept* in your input; carry on typing the rest of the line. This is the
   intended escape hatch — the code preserves the `#` when the panel is
   dismissed with the `#` filter chip still present and no query typed, on the
   reasoning that "the user probably wanted `#` without command search".

   Two ways to get this wrong, both of which **delete** the `#`: typing a query
   and then dismissing, and pressing `Backspace` first (that removes the filter
   chip, which makes the panel look empty rather than `#`-filtered). Press
   `Escape` first, not `Backspace`.
2. **Paste it.** The trigger only fires for characters you type; a paste never
   opens the search.
3. **Do not put it first.** The check is `buffer starts with "#"`. A leading
   space, or a `#` anywhere but position zero, is inert.
4. **Turn it off**: set
   `terminal.input.enable_ai_command_search_hash_trigger = false`, or use the
   toggle at **Settings → AI**. AI command search is still reachable from its
   key binding, ``ctrl-` ``.

The trigger only fires on the *transition* from "buffer does not start with `#`"
to "buffer does" — so once you have escaped out of it, continuing to type never
reopens it. Note that the panel opens on the very first `#` keystroke, with no
debounce, and takes keyboard focus; what you type next goes to the search bar,
not to the terminal input.

The `/` (slash commands) and `@` (context menu) triggers work the same way and
have their own toggles: `terminal.input.enable_slash_commands_in_terminal` and
`terminal.input.at_context_menu_in_terminal_mode`, both default `true`.

---

## Mouse

Right-click behaviour changed recently and is now configurable. The setting is:

| Setting | TOML path | Values | Default |
|---|---|---|---|
| Right-click behaviour | `terminal.input.right_click_behavior` | `context_menu`, `paste` | `context_menu` |

Set it from **Settings → Features**, under the mouse options.

### The right-click matrix

| Surface | `right_click_behavior = context_menu` (default) | `= paste` |
|---|---|---|
| Block list (normal screen) | Opens a context menu — for the block, the selected text, or the empty area, depending on where you clicked | Pastes the system clipboard. `shift`+right-click opens the menu |
| Block list, over an **active, long-running** command that has taken over the mouse | The click is forwarded to that program as a mouse report | Same — forwarding wins over pasting |
| Alternate screen (vim, htop, …) **with mouse tracking on** | Forwarded to the program | Forwarded to the program |
| Alternate screen without mouse tracking | Phosphor's alt-screen context menu | Pastes. `shift`+right-click opens the menu |
| Input editor / prompt | Input context menu | Pastes. `shift`+right-click opens the menu |
| Everywhere else in the app (tab bar, agent panels, notebooks, settings) | Context menu | **Still a context menu** — `paste` mode applies only to terminal surfaces |

**`shift` is the universal escape hatch.** It forces Phosphor's own handling in
both directions: `shift`+right-click always gives you the context menu when
right-click is set to paste, and `shift`+right-click (or `shift`+drag) also
overrides a full-screen program's mouse capture so you can select text locally.

Two platform notes:

- **On macOS, `ctrl`+left-click is converted to a right-click by the OS layer.**
  If you set right-click to paste, `ctrl`+click will paste. `ctrl` is not
  visible to Phosphor at right-click time at all, so there is no ctrl-based
  escape — use `shift`.
- A touch long-press is also delivered as a right-click.

### Middle-click paste

| Setting | TOML path | Default |
|---|---|---|
| Middle-click pastes | `terminal.input.middle_click_paste_enabled` | `true` *(macOS and Windows only)* |
| Use the Linux primary selection | `system.linux_selection_clipboard` | `true` *(Linux only)* |

- **On Linux/FreeBSD**, middle-click pastes the **primary selection**, as X11
  and Wayland users expect. `middle_click_paste_enabled` is *not consulted at
  all* on Linux — to turn middle-click paste off there you must set
  `system.linux_selection_clipboard = false`, which also stops selections being
  copied to the primary selection.
- **On macOS and Windows**, middle-click pastes the ordinary system clipboard,
  and `middle_click_paste_enabled` is the switch.
- If the pointer is over a detected link, middle-click **opens the link**
  instead of pasting.
- Middle-click is never forwarded to a full-screen program — Phosphor always
  handles it.

### Clicking and dragging

| Gesture | Effect |
|---|---|
| Click | Place the cursor (in the input) / start a selection (in the grid) |
| Double-click | Select a word |
| Triple-click | Select the line |
| Drag | Select a range |
| `shift`+click | Extend the selection |
| `cmd`+click *(macOS)* / `ctrl`+click *(Linux, Windows)* | Open the link or file path under the pointer |
| `cmd-alt`+drag *(macOS)* / `ctrl-alt`+drag | Rectangular (block) selection |
| `cmd`+click *(macOS)* / `alt`+click | Add a cursor, in the input editor |
| `ctrl`+scroll | Zoom, not scroll — excluded from terminal scrolling everywhere |

Hovering a link shows a tooltip rather than navigating, unless you use the
modifier click; turn the tooltip off with `general.link_tooltip = false`.
OSC 8 hyperlinks, bare URLs and file paths (with `file:line:col`) are all
recognised.

Double-click word boundaries are governed by "smart select":

| Setting | TOML path | Default |
|---|---|---|
| Smart select (URLs, emails, paths, identifiers) | `terminal.smart_select.enabled` | `true` |
| Word characters when smart select is off | `terminal.smart_select.word_char_allowlist` | `-.~/\` |

### What full-screen programs receive

| Setting | TOML path | Default |
|---|---|---|
| Forward mouse events to full-screen apps | `terminal.mouse_reporting_enabled` | `true` |
| Forward scroll events | `terminal.scroll_reporting_enabled` | `true` |
| Forward focus/blur events | `terminal.focus_reporting_enabled` | `true` |

All three have menu items under **View**. Scroll reporting is greyed out when
mouse reporting is off. Note that scroll forwarding does **not** honour the
`shift` bypass — only clicks and drags do.

---

## Selection and copying

| Setting | What it does | TOML path | Default |
|---|---|---|---|
| Copy on select | Selecting text copies it to the clipboard immediately | `terminal.copy_on_select` | `true` |
| Linux primary selection | Selections are also written to the X/Wayland primary selection | `system.linux_selection_clipboard` | `true` *(Linux only)* |
| Middle-click paste | See above | `terminal.input.middle_click_paste_enabled` | `true` *(macOS/Windows)* |
| Right-click behaviour | See above | `terminal.input.right_click_behavior` | `context_menu` |

Copy-on-select fires at the end of a drag selection, when you extend a selection
with the keyboard, and in the alternate screen as well as the block list. It is
also a checkbox in the **Edit** menu.

**One asymmetry worth knowing on Linux:** the write to the primary selection is
*not* gated on `copy_on_select`. Turning `terminal.copy_on_select` off stops
selections going to the system clipboard but they will still populate the
primary selection unless you also turn off `system.linux_selection_clipboard`.

Explicit copy:

| Action | macOS | Linux / Windows |
|---|---|---|
| Copy the current selection | `cmd-C` | `ctrl-shift-C` |
| Paste | `cmd-V` | `ctrl-shift-V` (and plain `ctrl-V` in the input editor) |
| Cut (input editor) | `cmd-X` | `ctrl-X` |

A program running in the terminal can also reach the clipboard through OSC 52,
which is **denied by default**:

| Setting | Values | TOML path | Default |
|---|---|---|---|
| OSC 52 clipboard access | `deny`, `write_only`, `read_write` | `terminal.osc52_clipboard_access` | `deny` |

---

## Find and search

### Find in the terminal

`cmd-F` (macOS) / `ctrl-shift-F` opens the find bar over the current terminal.
It searches the whole block list by default. The bar has three toggles:

- **Find in block** — restricts matches to the selected block(s). If nothing is
  selected when you turn it on, the most recent block is selected for you.
  Opening find while a block is already selected (the "find within selected
  block" action) starts in this state.
- **Match case**
- **Regex**

Navigate matches with `Enter` / `shift-Enter`, or `cmd-G` / `F3` (next) and
`cmd-shift-G` / `shift-F3` (previous). `Escape` closes the bar. The case and
regex toggles reset to off each time the bar is opened; the query is remembered.
Search direction follows the block order, so in the default bottom-pinned layout
"next" moves upward through history.

There is **no whole-word toggle** — only case, regex and find-in-block.

`experimental.async_find_enabled` exists as an opt-in for a non-blocking find
implementation on large outputs, but in a normal Phosphor build the underlying
feature is compiled on and force-enabled, so the setting has no observable
effect.

### Command search and history

- `ctrl-R` — **Command Search**: fuzzy search across your command history.
  Disabled in vim normal mode so that vim's `ctrl-R` redo works.
- ``ctrl-` `` — **AI command search**: describe a command in English. Same
  surface the `#` trigger opens. Requires a configured AI provider.
- `↑` — previous command from history.

### Palettes and global search

| Surface | macOS | Linux / Windows |
|---|---|---|
| Command palette (actions, settings, workflows) | `cmd-P` | `ctrl-shift-P` |
| Files palette | `cmd-O` | `ctrl-shift-O` |
| Navigation palette | `cmd-shift-P` | *(unbound)* |
| Global search panel | `cmd-shift-F` | `alt-shift-F` |
| Global search in the left panel | `ctrl-3` | `alt-3` |
| Workflows | `cmd-S` | `ctrl-shift-S` |

**Global search searches files, not blocks.** It is a ripgrep-backed search over
your project roots (and, on an SSH session, over the remote host), shown in the
left panel. It has regex and case-sensitivity toggles, both off by default. It
is on by default; `code.editor.show_global_search` (default `true`) turns it off.

There is no "search across all blocks" surface other than the find bar.

---

## Scrolling

| Setting | What it does | TOML path | Default |
|---|---|---|---|
| Mouse scroll speed | Multiplier applied to **non-precise** (wheel) scroll deltas only; trackpads are untouched | `general.mouse_scroll_multiplier` | `3.0` |
| Scrollback size | Maximum grid rows retained | `terminal.maximum_grid_size` | `50000` |

| Action | Binding |
|---|---|
| Page up / down | `pageup` / `pagedown` |
| Scroll one line up / down | *(no default binding; `terminal:scroll_up_one_line` / `..._down_one_line` are remappable)* |
| Scroll to top / bottom of the selected block | `cmd-shift-↑` / `cmd-shift-↓` (macOS), `ctrl-shift-↑` / `ctrl-shift-↓` |

Notes:

- The scroll multiplier is applied once, at event ingest, so it affects every
  scrollable surface — and it is applied *before* the decision to forward the
  event, so a boosted delta is what a full-screen program sees too.
- In the alternate screen there is no scrollback of your own. Scrolling is
  translated into arrow-key sequences sent to the program, which is what makes
  the wheel work in `less` and `man`.
- **There is no vim half-page scroll for terminal output.** `ctrl-D` / `ctrl-U`
  scroll by half a page only inside the code editor.
- `ctrl`+scroll is zoom, and is excluded from all terminal scroll handling.

---

## Panes and tabs

### Panes

| Action | macOS | Linux / Windows |
|---|---|---|
| Split right | `cmd-D` | `ctrl-shift-D` |
| Split down | `cmd-shift-D` | `ctrl-shift-E` |
| Split left / up | *(unbound)* `pane_group:add_left` / `pane_group:add_up` |
| Close the current pane | `cmd-W` | `ctrl-shift-W` |
| Move focus left/right/up/down | `cmd-alt-←/→/↑/↓` | `ctrl-alt-←/→/↑/↓` |
| Next / previous pane | `cmd-]` / `cmd-[` | `ctrl-shift-}` / `ctrl-shift-{` |
| Resize (move the divider) | `cmd-ctrl-←/→/↑/↓` | *(unbound by default)* |
| Maximise / restore the pane | `cmd-shift-enter` | `ctrl-alt-M` |
| Rename the pane | *(unbound)* `workspace:rename_active_pane` |

| Setting | TOML path | Default |
|---|---|---|
| Dim inactive panes | `appearance.panes.should_dim_inactive_panes` | `false` |
| Focus a pane on hover (focus follows mouse) | `appearance.panes.focus_pane_on_hover` | `false` |

### Tabs

| Action | macOS | Linux / Windows |
|---|---|---|
| New tab | `cmd-T` | `ctrl-shift-T` |
| Close tab | *(unbound)* `workspace:close_active_tab` |
| Close other tabs / tabs to the right | *(unbound)* |
| Reopen the last closed tab | `cmd-shift-T` | `ctrl-alt-T` |
| Next / previous tab | `shift-cmd-}` / `shift-cmd-{` | `ctrl-pagedown` / `ctrl-pageup` |
| Cycle sessions | `ctrl-tab` / `ctrl-shift-tab` | same |
| Go to tab 1–8 | `cmd-1` … `cmd-8` | `ctrl-1` … `ctrl-8` |
| Go to the last tab | `cmd-9` | `ctrl-9` |
| Move the tab left / right | `shift-ctrl-←` / `shift-ctrl-→` | `shift-ctrl-pageup` / `shift-ctrl-pagedown` |
| Rename the tab | *(unbound)* `workspace:rename_active_tab` |
| New window | `cmd-N` | `ctrl-shift-N` |
| Close window | `cmd-shift-W` | *(unbound)* |

`ctrl-tab` is configurable: `keys.ctrl_tab_behavior_setting` takes
`activate_prev_next_tab` (default), `cycle_most_recent_session` or
`cycle_most_recent_tab`.

| Setting | TOML path | Default |
|---|---|---|
| Where new tabs are inserted | `general.new_tab_placement` | `after_current_tab` |
| Close-button position on tabs (the dropdown appears only when the close-button-on-left feature is on, which it is by default) | `appearance.tabs.tab_close_button_position` | `right` |
| When the tab bar and window decorations are shown (`always_show`, `hide_fullscreen`, `on_hover`) | `appearance.tabs.workspace_decoration_visibility` | `hide_fullscreen` |
| Restore the previous session at launch | `general.restore_session` | `true` |
| Confirm before closing a session | `general.should_confirm_close_session` | `true` |
| Undo-close enabled | `general.undo_close.enabled` | `true` |
| Undo-close grace period | `general.undo_close.grace_period` | 60 seconds |

Session restore brings back windows, tabs, panes, tab groups, custom tab titles
and colours. **On Wayland, window positions are not restored** — the protocol
does not let an application place its own windows.

### Tab groups

**Tab groups exist and are on by default.** They are gated by a feature flag
(`GroupedTabs`) that is compiled in as a default feature, plus a user setting:

| Setting | TOML path | Default |
|---|---|---|
| Tab groups | `appearance.tabs.enable_tab_groups` | `true` |

Tab groups live in the **vertical tabs panel**, so turn vertical tabs on
(`appearance.vertical_tabs.enabled`, or `cmd-B` / `ctrl-shift-B`) to use them.
Multi-select tabs with `shift`-click (range) and `cmd`-click (individual), then
group them from the tab context menu; click a group header to collapse it,
double-click to rename it.

Actions — all unbound by default, and all remappable:
`workspace:new_tab_group`, `workspace:new_tab_group_from_active_or_selected_tabs`,
`workspace:remove_active_or_selected_tabs_from_group`. Pinning is a separate,
also-default-on feature: `workspace:pin_active_tab`,
`workspace:unpin_active_tab`, `workspace:pin_active_tab_group`,
`workspace:unpin_active_tab_group`.

Vertical tabs are also available (`appearance.vertical_tabs.enabled`, default
`false`; toggle with `cmd-B` / `ctrl-shift-B`).

---

## Fonts and rendering

### Fonts (`FontSettings`)

| Setting | TOML path | Default |
|---|---|---|
| Terminal font | `appearance.text.font_name` | `Hack` |
| Fallback font | `appearance.text.fallback_font_name` | *(empty — system fallback)* |
| Terminal font size | `appearance.text.font_size` | `13.0` |
| Terminal font weight | `appearance.text.font_weight` | `normal` |
| Line height ratio | `appearance.text.line_height_ratio` | `1.2` |
| UI font | `appearance.text.ui_font_name` | *(empty — system UI font)* |
| UI font size | `appearance.text.ui_font_size` | `12.0` |
| AI content font | `appearance.text.ai_font_name` | `Hack` |
| Match the AI font to the terminal font | `appearance.text.match_ai_font` | `false` |
| Notebook font size | `appearance.text.notebook_font_size` | `14.0` |
| Match notebook size to terminal size | `appearance.text.match_notebook_to_monospace_font_size` | `true` |
| Minimum-contrast enforcement | `appearance.text.enforce_minimum_contrast` | `only_named_colors` |
| Thin strokes *(macOS)* | `appearance.text.use_thin_strokes` | `on_high_dpi_displays` |
| Markdown heading scales H1–H6 | `appearance.text.markdown_heading_h1_scale` … `_h6_scale` | `2.0`, `1.5`, `1.17`, `1.0`, `0.83`, `0.75` |

> Phosphor contains a rule that would bump the default font size to 16 px on
> Windows for new users, but it lives inside the same never-taken onboarding
> branch as the `universal` input default. The real default on every platform is
> `13.0`.

Zoom: `cmd-=` / `cmd--` / `cmd-0` (macOS), `ctrl-=` / `ctrl--` / `ctrl-0`
elsewhere.

### Ligatures

| Setting | TOML path | Default |
|---|---|---|
| Render font ligatures | `appearance.text.ligature_rendering_enabled` | `false` |

Ligature rendering is off by default even though the underlying capability is
compiled in. Turn it on under **Settings → Appearance** if your font has
programming ligatures (`->`, `!=`, `=>`) that you want rendered.

### Cursor

| Setting | Values | TOML path | Default |
|---|---|---|---|
| Cursor style | `bar`, `block`, `underline` | `appearance.cursor.cursor_display_type` | `bar` |
| Cursor blink | `enabled`, `disabled` | `appearance.cursor.cursor_blink` | `enabled` |

Cursor style is not configurable while vim mode is on — vim drives it.

### GPU (`GPUSettings`)

| Setting | What it does | TOML path | Default |
|---|---|---|---|
| Prefer the low-power GPU | Uses the integrated GPU rather than the discrete one | `system.prefer_low_power_gpu` | `true` on Linux/FreeBSD, `false` elsewhere |
| Preferred graphics backend | Windows only; `vulkan` or the platform default | `system.preferred_graphics_backend` | unset |

`system.force_x11` is also available on Linux if the Wayland backend gives you
trouble. It defaults to `true` under WSL and `false` otherwise.

### Full-screen application padding

| Setting | TOML path | Default |
|---|---|---|
| Padding around full-screen apps | `appearance.full_screen_apps.alt_screen_padding` | uniform `0` |

Set it to `match_blocklist` to use the same padding as the block list.

---

## Reference: every setting in this chapter

| TOML path | What it does | Default |
|---|---|---|
| `appearance.blocks.should_show_bootstrap_block` | Show the shell-integration bootstrap block *(debug builds only)* | `false` |
| `appearance.blocks.should_show_in_band_command_blocks` | Show Phosphor's internal in-band commands *(debug builds only)* | `false` |
| `appearance.blocks.should_show_ssh_block` | Show the SSH connection block | `false` |
| `appearance.blocks.show_block_dividers` | Draw dividers between blocks | `true` |
| `appearance.blocks.show_jump_to_bottom_of_block_button` | Jump-to-bottom button in long output | `true` |
| `appearance.cursor.cursor_blink` | Cursor blink | `enabled` |
| `appearance.cursor.cursor_display_type` | Cursor shape | `bar` |
| `appearance.full_screen_apps.alt_screen_padding` | Padding around full-screen apps | `0` uniform |
| `appearance.input.input_mode` | Input position / block ordering | `pinned_to_bottom` |
| `appearance.panes.focus_pane_on_hover` | Focus follows mouse between panes | `false` |
| `appearance.panes.should_dim_inactive_panes` | Dim inactive panes | `false` |
| `appearance.spacing` | Block spacing | `normal` |
| `appearance.tabs.enable_tab_groups` | Allow named, collapsible tab groups | `true` |
| `appearance.tabs.tab_close_button_position` | Close button side | `right` |
| `appearance.tabs.workspace_decoration_visibility` | When the tab bar is shown | `hide_fullscreen` |
| `appearance.text.ai_font_name` | Font for AI content | `Hack` |
| `appearance.text.enforce_minimum_contrast` | Contrast enforcement | `only_named_colors` |
| `appearance.text.fallback_font_name` | Fallback font | *(empty)* |
| `appearance.text.font_name` | Terminal font | `Hack` |
| `appearance.text.font_size` | Terminal font size | `13.0` |
| `appearance.text.font_weight` | Terminal font weight | `normal` |
| `appearance.text.ligature_rendering_enabled` | Render ligatures | `false` |
| `appearance.text.line_height_ratio` | Line height | `1.2` |
| `appearance.text.markdown_heading_h1_scale` … `h6` | Markdown heading sizes | `2.0`/`1.5`/`1.17`/`1.0`/`0.83`/`0.75` |
| `appearance.text.match_ai_font` | AI font follows terminal font | `false` |
| `appearance.text.match_notebook_to_monospace_font_size` | Notebook size follows terminal size | `true` |
| `appearance.text.notebook_font_size` | Notebook font size | `14.0` |
| `appearance.text.ui_font_name` | UI font | *(empty)* |
| `appearance.text.ui_font_size` | UI font size | `12.0` |
| `appearance.text.use_thin_strokes` | Thin glyph strokes *(macOS)* | `on_high_dpi_displays` |
| `appearance.vertical_tabs.enabled` | Vertical tab panel | `false` |
| `experimental.async_find_enabled` | Opt-in async find *(already force-enabled in this build)* | `false` |
| `general.link_tooltip` | Tooltip on link hover | `true` |
| `general.mouse_scroll_multiplier` | Wheel scroll speed | `3.0` |
| `general.new_tab_placement` | Where new tabs go | `after_current_tab` |
| `general.preserve_input_focus_on_block_selection` | Keep input focus when selecting blocks | `false` |
| `general.restore_session` | Restore the previous session at launch | `true` |
| `general.should_confirm_close_session` | Confirm before closing a session | `true` |
| `general.snackbar_enabled` | Sticky command header | `true` |
| `general.undo_close.enabled` | Undo close tab/pane | `true` |
| `general.undo_close.grace_period` | Undo window | `60s` |
| `keys.ctrl_tab_behavior_setting` | What `ctrl-tab` does | `activate_prev_next_tab` |
| `system.force_x11` | Force the X11 backend on Linux | `true` under WSL, `false` otherwise |
| `system.linux_selection_clipboard` | Use the primary selection *(Linux)* | `true` |
| `system.prefer_low_power_gpu` | Use the integrated GPU | `true` on Linux/FreeBSD |
| `system.preferred_graphics_backend` | Windows graphics backend | unset |
| `terminal.copy_on_select` | Copy on selection | `true` |
| `terminal.focus_reporting_enabled` | Forward focus events to full-screen apps | `true` |
| `terminal.input.alias_expansion_enabled` | Expand shell aliases in the input | `false` |
| `terminal.input.at_context_menu_in_terminal_mode` | `@` context menu | `true` |
| `terminal.input.autosuggestions.enabled` | Autosuggestions | `true` |
| `terminal.input.autosuggestions.keybinding_hint` | Show the accept-key hint | `true` |
| `terminal.input.autosuggestions.show_ignore_button` | Show the ignore button | `false` |
| `terminal.input.classic_completions_mode` | Traditional completions | `false` |
| `terminal.input.command_corrections` | Suggest fixes for mistyped commands | `true` |
| `terminal.input.completions_open_while_typing` | Open completions as you type | `false` |
| `terminal.input.enable_ai_command_search_hash_trigger` | `#` opens AI command search | `true` |
| `terminal.input.enable_slash_commands_in_terminal` | `/` slash commands | `true` |
| `terminal.input.error_underlining_enabled` | Underline command errors | `true` |
| `terminal.input.extra_meta_keys` | Which Alt keys act as Meta | both off |
| `terminal.input.honor_ps1` | Use your shell's `PS1` | `false` |
| `terminal.input.input_box_type_setting` | `classic` or `universal` input | `classic` |
| `terminal.input.middle_click_paste_enabled` | Middle-click paste *(macOS/Windows)* | `true` |
| `terminal.input.right_click_behavior` | `context_menu` or `paste` | `context_menu` |
| `terminal.input.show_hint_text` | Placeholder hint in the input | `true` |
| `terminal.input.syntax_highlighting` | Highlight the command line | `true` |
| `terminal.maximum_grid_size` | Scrollback rows | `50000` |
| `terminal.mouse_reporting_enabled` | Forward mouse events to full-screen apps | `true` |
| `terminal.osc52_clipboard_access` | OSC 52 clipboard access | `deny` |
| `terminal.scroll_reporting_enabled` | Forward scroll to full-screen apps | `true` |
| `terminal.show_terminal_zero_state_block` | AI zero-state block in new sessions | `true` |
| `terminal.smart_select.enabled` | Smart double-click selection | `true` |
| `terminal.smart_select.word_char_allowlist` | Word characters when smart select is off | `-.~/\` |
| `terminal.use_audible_bell` | Audible bell | `false` |
| `text_editing.autocomplete_symbols` | Auto-close brackets and quotes | `true` |
| `text_editing.code_editor_line_number_mode` | `absolute` or `relative` | `absolute` |
| `text_editing.vim_mode_enabled` | Vim keybindings | `false` |
| `text_editing.vim_status_bar` | Vim status bar | `true` |
| `text_editing.vim_unnamed_system_clipboard` | Vim unnamed register = system clipboard | `false` |

---

## Not available in Phosphor

Phosphor is a de-Warped fork with no Warp account and no Warp backend. If you
are coming from Warp, these terminal features are gone on purpose. See
`DECLINED.md` for the full register.

| What you will look for | Why it is not here |
|---|---|
| **Share block / block permalinks** | The share-block modal and the `terminal:open_share_block_modal` binding were removed with the rest of the cloud layer; a permalink needs Warp's servers to host it. The block context menu's "Share block…" / "Share session…" entries are gone, as is the **Settings → Shared Blocks** page. A `CreateBlockPermalink` action still exists as a leftover enum variant but is registered to nothing and dispatches nowhere. |
| **Shared sessions / session sharing** | Cloud-hosted. There is no server to host a shared session or resolve who may join it. |
| **Screen and session recording** | Declined outright (`DECLINED.md`, #367 and #350) — not because it was cloud, but because Phosphor is not shipping it. There is no capture code in the tree. |
| **Warp Drive as a cloud store** | The Library panel is local. Cloud Drive objects, team folders and team workflows do not exist; `warp.dev/drive/...` links resolve to nothing. |
| **Teams, workspaces, org policy** | `has_teams()` is permanently `false`. No team tabs, team workflows, or organisation-level command denylists. |
| **Voice input** | Recording works but transcription was cloud (Wispr) and is disabled; the `/voice` command is not registered. |
| **Warp account / login / sign-in avatar in the tab bar** | There is no account. The avatar-in-tab-bar flag is force-disabled. |
| **Telemetry** | The channel is physically removed; the toggle never renders. |
| **The "Oz updates" zero-state feed and Warp-branded feature-intro popovers** | Branded content this fork does not carry. |

Two things that *are* still here despite looking cloudy: crash reporting (it is
functional, and writes a local backtrace to the log — nothing is uploaded), and
the SSH tmux wrapper, which Phosphor keeps permanently even though upstream
deprecated it.

---

## Where to change things

| | |
|---|---|
| Settings UI | `cmd-,` / `ctrl-,` |
| Settings file | `~/.phosphor/settings.toml` (macOS), `~/.config/phosphor/settings.toml` (Linux), `%LOCALAPPDATA%\phosphor\Phosphor\config\settings.toml` (Windows) — hot reloaded |
| Key bindings UI | **Settings → Keybindings** (`cmd-ctrl-K` on macOS) |
| Key bindings file | `keybindings.yaml` in the same directory — **read only at startup** |
| Keyboard cheat sheet | `cmd-/` (macOS only) |
| Mouse settings | **Settings → Features** |
| Fonts, blocks, tabs, panes | **Settings → Appearance** |
| The `#` trigger toggle | **Settings → AI** |
