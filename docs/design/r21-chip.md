# Round 21: the prompt suggestion is a chip on the editor's first line (#2138)

Owner decision 2026-11-11 on #2138: a prompt suggestion that arrives late must not move the output under the pointer, and
not by keeping a permanent empty band above the prompt (that was `fix/r21-banner` 766cb55ba, not merged). The suggestion is
shown as a chip inside a row the input already has, so the input's height is the same with and without it.
Nothing here was compiled or run.

## Cause (unchanged from `r21-banner.md`)

`Input::render_input_banner` (`app/src/terminal/input.rs:18410`) returns a box of two text lines only while a suggestion
shows; each input layout adds it to its column, so the input grows, `TerminalView::render` gives the block list less room and
every visible row moves by the box's height (78 device px in the round 15 macOS run). The pin has the same layout.

## The input layouts and the rows each already has

`Input::render` (`input.rs:19164`) picks one of five. Rows are listed top to bottom for an input at the bottom.

| layout | when | rows (file:line) |
|---|---|---|
| terminal | AgentView on, no agent view, no PS1 | [banner] `terminal.rs:51`; prompt chips `:60`; **editor** `:69`; message bar, else an 8 px margin `:77-91` |
| agent | AgentView on, agent view active | [banner] `agent.rs:177`; [attachment chips] `:189`; **editor** `:204`; footer view `:211` |
| classic | PS1 honoured (or AgentView off and Classic) | [banner] `classic.rs:116`; prompt top padding and prompt row `:123` (empty when the prompt is drawn inside the editor, `:64`); [attachments] `:127`; **editor** `:140`; message bar, else a 4 px margin `:142-157` |
| universal | AgentView off, universal input | [banner] `universal.rs:64`; prompt chips `:75`; [attachments] `:79`; **editor** `:94`; button bar view `:101` |
| CLI agent | a CLI agent's input is open | never showed the banner (`cli_agent.rs` has no `render_input_banner` call); unchanged |

Rows in brackets exist only sometimes. What the four banner layouts share:

- A prompt chips row: terminal and universal only. The agent view has none; classic has the shell's own prompt, often inside
  the editor.
- A toolbar or status row: agent (footer) and universal (button bar) only, and both are child views with their own controls
  from edge to edge. Terminal and classic have the message bar only while there is a message; otherwise 8 or 4 px.
- **The editor row** (`Input::render_input_box`, `input.rs:18508`): every layout, both modes, one function.

So the row is the editor row. The chip sits at the right end of the editor's first line.

## Layout of the chip

`render_input_box` returns `SavePosition(EventHandler(Container(editor)))`. With a visible suggestion and room for it, the
container is put in a `Stack` and the chip is added with `Stack::add_positioned_child`, anchored top right to top right, moved
left by the editor's right padding.

**The height is the same by construction.** `Stack::layout` (`crates/warpui_core/src/elements/stack/mod.rs:182-192`) takes
its size from the children that are not positioned; a positioned child is laid out afterwards and adds nothing. No layout's
column gets a banner child any more (`render_input_banner` returns `None` in chip mode). The chip appearing, shrinking, being
hidden or going therefore changes no element's size in the terminal, agent, classic and universal layouts, for an input at
the bottom, at the top or in waterfall mode. The view test reads the rectangles to prove it.

The chip itself: the banner's button (`render_button` in `view/inline_banner/prompt_suggestions.rs`) in a compact form.
Height = one editor line (`EditorView::line_height`), so it stays inside the first line; icon, label, and the accept
keystroke when there is room for it (32 cells or more).

### Room and truncation

The first line also holds what the user types, so the chip gets only the part of the line that is free
(`input/suggestion_chip.rs`, `chip_max_width`, pure):

- `used` = prefix + the first buffer line + the ghost text of an active autosuggestion, in display columns
  (`unicode-width`; a tab counts 8), times the editor's cell width (`EditorView::em_width`);
- `free` = editor width - right inset - `used` - a 2 cell gap;
- the chip is at most `min(free, half the editor width)`; under 16 cells it is **not drawn** (too short to read).

The editor width is not known at render time; it is taken as pane width minus twice the terminal padding minus 16 px, which
is at or under the real width in all four layouts (the universal input's 6 px margins and border included), so the estimate
errs towards a smaller chip.

A long suggestion: the label is a single-line text in a `Shrinkable` inside a box of that maximum width, as in the banner
today, so it is cut at the box; the icon and the keystroke keep their size. Typing makes the chip narrower, then hides it;
deleting brings it back. Hidden for lack of room is only "not drawn": the suggestion stays pending, the accept key still
takes it and Escape still drops it, exactly like a banner today.

Classic with the prompt inside the editor (PS1): the shell's prompt is on the same line. The prefix is the prompt's last
line (`HeaderGrid::lprompt_last_line_width_cols`), the chip is moved down to the prompt's last row, and the right inset is
the right prompt's width plus the gap, from `Block::rprompt_render_offset`. A same-line prompt that is not the PS1 grid
(context chips on the editor's line: only with AgentView off) has no width the input can read: that one configuration keeps
the growing banner.

### Click, keyboard, dismissal

Nothing about the actions changes; only where the control is drawn.

- Click: the chip's `Hoverable::on_click` dispatches `TerminalAction::ResolvePromptSuggestion(Accept { Button })`, the
  banner's own action. It is dispatched from the input's tree; the terminal view above it handles it (as it does
  `TerminalAction::MiddleClickOnInput` from the same tree).
- Keyboard: `terminal:accept_prompt_suggestions` (`view/init.rs:365`; Alt+Shift+Enter on Linux and Windows, Ctrl+Enter on
  macOS) is bound on the context flag `HAS_PENDING_PROMPT_SUGGESTION`, which `set_prompt_suggestions_banner_state` sets. Not
  touched.
- Dismissal: Escape, Enter, entering or leaving the agent view, a new agent block (`TerminalView::clear_prompt_suggestions`).
  Not touched.
- `should_hide` (a coding suggestion waiting for its diff): not drawn, as before.
- A disabled suggestion (no connection, out of requests): the dimmed chip with its tooltip, as the banner's button. The
  banner's separate alert on the right is not shown in the chip (the prompt and the footer show it already).

A click is still taken at once. The chip is drawn inside the input, where no output was a frame earlier, and nothing moves
when it comes, so the click of #2138 cannot land on it.

## Kill switch

`PHOSPHOR_PROMPT_SUGGESTION_CHIP=0` (also `off`, `false`, `no`; read once): `render_input_banner` returns the banner and
`render_input_box` adds no chip, which is today's growing input.

## Log

`input_order hop=prompt_chip shown=<0|1> hidden=<0|1> place=<chip|banner> drawn=<0|1> width=<px>` from
`set_prompt_suggestions_banner_state`. `drawn` and `width` are what the next render will use (0 when there is no room).
No text of the suggestion.

## Rejected

- Keeping the banner's space (766cb55ba): two empty lines in every pane; the owner said no.
- A chip in the prompt chips row or the footer: neither exists in all four layouts, and two of them are child views that
  would each need their own chip.
- Shrinking the editor to make room: the buffer would wrap at another column when the chip comes, and a wrapped buffer is
  taller.
- A chip drawn by the editor element as a right-side section (where the right prompt goes): exact widths, but it changes the
  editor's layout code for every editor in the app.
- Anchored scroll or an overlay over the output: the chip would be drawn where the click was aimed.

## Cost and risk

- Layout only: no key path, no timer, no model request changes.
- The chip can be absent while a suggestion is pending (a long line, a narrow pane). The accept key then takes a suggestion
  that is not on screen. Same key, same action as with a banner; stated for the owner.
- The room is an estimate from columns, not from the editor's own layout. It errs towards hiding. A wide glyph the width
  table counts short could let the chip touch the last typed character.
- The placeholder text of an empty editor is not counted; the chip covers its right half at most.
- While a suggestion is pending the agent input renders on every edit again (the chip's width is one of its display facts,
  #1962), and `render_input_box` reads the buffer's first line once per render. With no suggestion pending nothing is added.
- "Execute this plan" (the static suggestion, not behind the setting) uses the same state, so it is a chip too and grows
  nothing.

## Tests

- `input/suggestion_chip_tests.rs`: the switch's parser, the placement table, `chip_max_width` (empty line, typing shrinks
  it, the half-width cap, under the minimum, a line longer than the editor, prefix and right inset), display columns.
- `terminal/view/suggestion_chip_view_tests.rs`: the block list's and the input's rectangles before the suggestion, with
  it and after it goes are equal; with it, the chip has a rectangle inside the input's; after, it has none.
- appci (`cases_chip.py`, PROVING / PROVING_FAKE): `suggestion_chip_keeps_output` (both modes, the pass's model; no
  suggestion within the wait is UNVERIFIABLE) and `agent_suggestion_chip_keeps_output` (scripted model, late answer). The
  lowest output row's drawn y from the find hook before and with the chip; the chip's text read from the screen by OCR
  (UNVERIFIABLE when it cannot be read); then the accept key, judged from the app's own log.

## For the owner, on a build

Listed in the report: a screenshot of the chip in each layout and mode, a long suggestion, a narrow pane, typing towards the
chip, and the PS1 prompt with a right prompt.
