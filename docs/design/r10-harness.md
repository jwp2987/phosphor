# r10 harness: wrapped-line copy (#1976) and vertical tabs (#1990)

Both issues come from `docs/design/user-annoyance-review.md` (items 8 and 20). Both are TEST gaps: no
defect was found by reading. Nothing in the app changes on this branch.

## #1976: copy of soft-wrapped lines

### What the code does (read at merge/w0 41340e2fd)

- A soft wrap is the `WRAPLINE` flag on the last cell of a grid row. The copy text of a row gets a
  newline only when the selection reaches the last column AND the row's last cell has no `WRAPLINE`:
  `app/src/terminal/model/grid/grid_handler.rs:1272` (`line_to_string`), and the off-thread twin
  `app/src/terminal/model/grid/text_snapshot.rs:82` (`SnapshotRow::render_into`).
- Every copy path ends there: a mouse selection (`app/src/terminal/model/blocks/selection.rs:1110`,
  `:1114`, `:1199`), "copy block" and "copy output" (`app/src/terminal/view.rs:27095` `copy_outputs`
  -> `copy_blocks` -> `Block::bounds_to_string`).
- A triple click selects the logical line: `line_search_left` / `line_search_right` walk across
  `row_wraps` (`grid_handler.rs:2743`, `:2754`).
- A line exactly as wide as the pane does not wrap: `carriage_return` clears `input_needs_wrap`
  (`ansi_handler.rs:587`), so its last cell carries no `WRAPLINE` and its newline is kept.

So by reading, the suspected defect ("copy inserts hard line breaks at soft wraps") is not there.
What was missing is proof: `grid_handler_test.rs` had no test of the wrap rule on the copy path (the
snapshot twin was only compared against the inline path, not against expected text), and no appci
case copies wrapped output.

### Chosen

1. Unit tests (`grid_handler_test.rs`, `*_1976`): a wrapped line copies as one line through both the
   inline and the snapshot path, for a whole-line selection and for drags inside and across the wrap
   points; a line of exactly the grid width keeps its newline, one cell more wraps and stays one
   line; the copied text is the same after a resize to a narrower and a wider grid.
2. Two appci cases in `cases.PROVING` (`appci/cases_wrap_vtabs.py`):
   - `ui_copy_wrapped_output` (keyboard only, so it runs on Linux, macOS and the Windows Git Bash
     pane): `cat` a file whose lines are a 2.5-width line, a short line, a line of exactly the pane
     width, one of width + 1 and a last line; select the block with the block-up key and copy it
     (copy, then copy-output). The clipboard must hold the file's lines exactly. The pane width is
     the shell's own `stty size`.
   - `ui_copy_wrapped_drag` (pointer: X11, and Wayland with the virtual pointer): drag from a word on
     the first visual row of the long line to a word on the second, and triple-click a word. The
     drag must give the text between the two words with no line break; the triple click the whole
     logical line.

### Rejected

- One combined case: a platform without a pointer tool would have to SKIP the keyboard half too, or
  a skip would hide checks that already ran.
- Locating the text by OCR only: the words go through the app's find hook first
  (`cases_ui.find_token`), OCR is the fallback, as in `ui_mouse_select_copy`.
- A fixed column count: the pane width differs per platform, profile and font. The case reads it.

### Risk

Test-only. The unit tests assert reflow results that were not run here (no cargo on this machine):
the resize test uses `starts_with` on the whole grid text so that blank rows below the text do not
matter. The Windows pane goes through ConPTY, which re-emits wrapped output itself; a red result
there needs a TEST-vs-APP verdict before it is called a defect.

## #1990: vertical tabs (the owner's layout)

### What exists

`ui_settings_live` flips `appearance.vertical_tabs.enabled` and checks that the screen changes and
typing stays exact. Nothing opens several tabs with the panel on, switches between them, or clicks a
row. The owner's seed (`~/.config/phosphor-ci/owner-settings.toml`) may already enable the panel,
but the harness never says so and no case depends on it.

Code facts used:
- the setting is `appearance.vertical_tabs.enabled` (`app/src/workspace/tab_settings.rs:549`);
- a tab key that reaches the workspace logs `hop=tab_activate index= count= active= window=`
  (`app/src/workspace/view.rs:4834`);
- a click on a panel row dispatches `WorkspaceAction::FocusPane` (`vertical_tabs.rs:600`), not
  `ActivateTab`, so a click is proven by where typing lands, not by that hop;
- a row shows the pane's command and its working directory in every `primary_info` setting
  (`vertical_tabs.rs:4636`); the working directory is clipped at its START, so a directory whose
  last component is a short token stays readable.

### Chosen

Two appci cases in `cases.PROVING`, both modes:
- `ui_vertical_tabs_many`: vertical tabs on (from the profile, or switched on live and restored), 8
  tabs (`APPCI_VT_TABS`), one exact command in each, every tab reached by its number key in a
  shuffled order plus next / previous / last-tab keys, each proven by the tab's own `pwd`; the tab
  count from `hop=tab_activate`; exact typing with all tabs open and after they are closed.
- `ui_vertical_tabs_panel_click`: 4 tabs, each in a directory named by an OCR-safe token and with a
  last command holding the token; with tab 1 shown, the tokens of the other tabs are on screen only
  in the panel. They must be stacked top to bottom in tab order, and a click on a row must put the
  next typed command in that tab's shell.

### Rejected

- Forcing `primary_info` / `view_mode` for a deterministic row: the point of #1990 is the owner's
  layout, so the panel settings stay as the profile has them. A row that cannot be read is
  NO-REFERENCE with the reason, not a pass and not a fail.
- The find hook for the rows: a hidden tab's token was drawn in its own pane while that tab was
  shown, and the hook logs a position once, so a logged position cannot be told from a stale one.
  The rows are read by OCR of the screen with tab 1 shown.
- Windows: the cases rewrite the settings file while the app runs, as `ui_settings_live` does, and
  that path is not ported (`winmap.NOT_WINDOWS`). macOS has no pointer tool, so the click case skips
  there; the keyboard case runs.

### Risk

Test-only. The case leaves extra tabs open if the close key fails; it reads the tab count from the
log before every close so it never closes the last tab (which would close the window), and it fails
if more than one tab is left where a close key exists.

## Not app bugs

Neither issue has evidence of an app defect. If the next run shows a red result in one of the four
cases, that is the first evidence either way and needs a TEST-vs-APP verdict from the log.
