# Round 16: keys (#1988 Ctrl-R and Alt-R, #2079 readline Ctrl-A / Ctrl-E / Ctrl-Y, #2080 shifted scroll keys at a prompt)

Branch `fix/r16-keys2`, from `integ/r14` fae4cecfd. Owner decisions of 2026-11-11. Nothing was
compiled and no app case was run when this was written: the code is worked, unproven. Not
touched, by instruction: `terminal/input.rs`, `terminal/input/enter_queue.rs`,
`terminal/view/give_up_to_pty.rs`.

All four behaviours differ from the upstream pin on purpose; `DECLINED.md` has the IMPROVED rows
("Round 16: keys").

## 1. Ctrl-R: the user's own binding wins over command search (#1988)

**Decision.** "Ctrl-R: the user's own binding wins over command search."

**Cause.** A key press walks the responder chain from the focused view up
(`warpui_core/src/core/app.rs` `dispatch_keystroke`); at each view the matcher returns the first
binding whose chord and context match (`keymap/matcher.rs` `push_keystroke`). Command search is
an editable binding of the input (`terminal/input.rs`, `workspace:show_command_search`, context
`Input & !VoltronActive & !VimNormalMode`, default `ctrl-r`). With the prompt focused the chain is
editor, input, terminal, pane group, workspace. So a user line such as
`"workspace:new_tab": ctrl-r` in `keybindings.yaml` (`keyboard.rs` `load_custom_keybindings`
sets it as a custom trigger) never fired at the prompt: the input answered first. Only an action
of the editor itself could take the key. The appci case `keybinding_rebind` documented this as
"not played: an owner question". On macOS the key does not even reach the keymap: the menu item
"Command Search" carries `ctrl-r` as its key equivalent (`app_menus.rs` `custom_action_updater`).

**Fix.** One mark on the binding, one rule in the matcher.

- `EditableBinding::yielding_to_user_bindings()` (and `Matcher::mark_yielding_to_user_bindings`
  by name, used here because the registration is in `input.rs`). The lens reports it only while
  the binding has no custom trigger.
- `Keymap::user_binding_outranks_default(first key, contexts of the whole chain)`: true when a
  yielding binding has that key as its default AND an enabled binding with a custom trigger
  beginning with that key applies in one of the chain's contexts. Both sides come from the
  existing first-keystroke index, so a key nobody marked costs one map lookup.
- `dispatch_keystroke` computes it once per key press and passes it to
  `push_keystroke_yielding`, which skips a yielding binding as the first key of a chord. The walk
  then reaches the view of the user's binding. The key is matched once per view exactly as
  before: nothing is held, reordered or dropped.
- macOS: `bindings::menu_keystroke_for_binding` gives the menu item no key equivalent while its
  binding yields and `AppContext::user_binding_claims_keystroke` is true for the focused view.
  The key then reaches the keymap, where the user's keystroke binding matches (command search is
  a custom-action binding there and is never matched by keystroke).
- `launch()` marks the names in `BINDINGS_YIELDING_TO_USER_BINDINGS` (command search only) right
  after the user file is loaded.

Where the user's binding does not apply (its view is not in the chain), command search keeps
Ctrl-R. Once the user gives command search itself a key, it is a user binding and stands aside
for nothing. Command search stays on the command palette, the macOS menu, Alt-R (history) and
any key it is moved to.

Kill switch `PHOSPHOR_USER_BINDING_WINS=0` (nothing is marked).

**Rejected.** (a) The rule for every built-in default: it would change which binding wins for
every chord a user has reused in a parent view (today the deeper default wins everywhere); the
decision names Ctrl-R, and the general rule is an owner question. (b) A context-free rule
("command search is off whenever any user binding has ctrl-r"): Ctrl-R would do nothing in the
input when the user's binding belongs to a view that is not in front. (c) Reordering the
responder chain: every key would pay for it.

**Risk.** Low off macOS (unit-tested in `matcher_test.rs`; one extra index lookup per key). The
macOS half is read, not run: it relies on the menu asking the updater before it takes a key
equivalent (the existing `menuNeedsUpdate` path that already swaps the key when the user rebinds
command search). If a menu keeps a stale key equivalent the result is the old behaviour, not a
lost key.

## 2. Alt-R opens the app's history search (#1988 option B)

`input:search_command_history` (command search filtered to history) shipped unbound. Its custom
action `CustomAction::HistorySearch` now maps to `alt-r` on Linux / Windows and `meta-r` on macOS
(`bindings::default_history_search_keystroke`, one arm in `custom_tag_to_keystroke`). The binding
lives in the input, so a program that has the terminal still receives `ESC r`.

- Linux / Windows: custom triggers are converted to keystrokes at registration, so the arm is
  the whole change. An Alt key set to act as Meta arrives as `meta-r`; a fixed binding of the
  same action covers it (`terminal/view/init.rs`), as `meta-.` does for Alt-. (#1989).
- macOS: custom actions keep `Trigger::Custom` and fire through their menu item. The history
  search has no menu item, so nothing would ever fire it. `lib.rs` now registers a second
  conversion on macOS that maps ONLY menu-less actions (`mac_menuless_custom_tag_to_keystroke`,
  today the history search). Option-R with Option as Meta; an Option key that types characters
  keeps typing them.

Kill switch `PHOSPHOR_ALT_R_HISTORY_SEARCH=0` (ships unbound, the fixed fallback is not
registered).

**Rejected.** Editing `input.rs` to give the binding `.with_key_binding`: the file is off limits
and the custom action already is the per-platform hook. Option A (lending the prompt to the
shell's own search) is declined with this decision.

**Risk.** Low. `alt-r` / `meta-r` were free (round 14's test pinned it). The macOS conversion is
read, not run.

## 3. Readline Ctrl-A / Ctrl-E / Ctrl-Y off macOS (#2079)

**Decision.** "Readline Ctrl-A/E/Y in the input off macOS, select-all moves to Ctrl-Shift-A."

**Before** (`editor/view/mod.rs`, Linux / Windows): `editor_view:select_all` is the `SelectAll`
custom action (`ctrl-a`); `editor_view:move_to_line_start` / `_end` have macOS defaults only;
Linux registers a fixed `ctrl-e` that only raises the "emacs bindings" banner;
`editor_view:delete_all_left` defaults to `ctrl-y` and, being editable, shadows the fixed `ctrl-y`
yank.

**Fix** (`bindings::readline_input_keys_enabled()`: Linux / Windows, on unless the switch):

- `move_to_line_start` / `move_to_line_end` default to `ctrl-a` / `ctrl-e` on every platform;
- `editor_view:select_all` defaults to `ctrl-shift-A` (`bindings::with_select_all_default`);
- `editor_view:delete_all_left` has no default key in the text editor, so the fixed `ctrl-y` yank
  answers (it pastes the editor's own kill buffer, what Ctrl-K / Ctrl-U / Ctrl-W cut);
- the banner's `ctrl-e` binding is not registered, and the banner is not raised by select-all
  (`terminal/view.rs`, the `EmacsBindingUsed` handler): its offer is the default now.

Scope: `EditorView` is the input AND the app's single-line fields (find, palette filter,
settings search); all get the readline keys, exactly what the app's own "Yes, use Emacs-style
bindings" button has always set. The code editor and the notebook editor are different views:
their Ctrl-A / Ctrl-E / Ctrl-Y are not changed, and both keep a FIXED `cmdorctrl-a` select-all.
They register a binding of the same name `editor_view:select_all`; it takes the same
`ctrl-shift-A` default so the name has one default in the settings page (Ctrl-A still selects
all there through the fixed binding).

Kill switch `PHOSPHOR_READLINE_INPUT_KEYS=0`: the old defaults, the banner included.

**Known costs, for the owner.**

- `ctrl-shift-A` is also the Linux / Windows default of "toggle the conversation list"
  (`workspace/mod.rs`, a workspace binding). The focused text editor answers first, so with a
  text field focused that chord selects all. The panel stays on Alt-1 and its button; the
  agent view's shortcut hint still names Ctrl-Shift-A. Which of the two moves is an owner
  question.
- `editor_view:delete_all_left` and `editor_view:move_to_line_start` are shared names with the
  code / notebook editors, whose defaults (ctrl-y; none) differ off macOS now. The settings page
  shows one row per name and description.
- Test and harness code that cleared a text field with Ctrl-A + Backspace is changed to End +
  Backspaces (appci palette and find cases, the Windows latency scripts), which means the same on
  every build.

**Rejected.** Scoping the three keys to the terminal input by a context flag: one binding name
would need two defaults inside one view type, and the fields next to the input would keep
"Ctrl-A selects all" while the input does not.

## 4. Shifted scroll keys at a prompt, editable (#2080)

**Decision.** "Shifted scroll keys also at a prompt, and editable."

**Fix.**

- The four bindings are editable (`terminal:scrollback_page_up`, `_page_down`, `_top`,
  `_bottom`; `scrollback_keys::scrollback_key_bindings`), same action, same handler, same
  `hop=scrollback_key` line.
- Predicate: `Terminal & !IMEOpen & !AltScreen & (!LongRunningCommand | !EditorFocused)`. Round
  15's reach (a running command with the terminal focused) plus every state without a running
  command. A running command with the input editor focused (the rich input of a CLI agent) is
  still excluded. The alternate screen keeps the keys in every case.
- At a prompt the input editor is focused and is asked first. Shift+PageUp / Shift+PageDown have
  no binding in it and arrive at the terminal view. Shift+Home / Shift+End are the editor's fixed
  "select to line start / end". They are left to the terminal view only while the terminal input
  is EMPTY (`shift_home_end_selects_predicate`: a new context flag `EditorView_BufferEmpty`, and
  the existing `TERMINAL_INPUT_PAGE_KEYS_HANDLED_BY_INPUT` that marks the terminal input's
  editor). Nothing is taken from the editor: there is nothing to select in an empty input.

Kill switches: `PHOSPHOR_SCROLLBACK_KEYS_AT_PROMPT=0` (round 15's predicate, the editor keeps
both keys in every state), `PHOSPHOR_SCROLLBACK_KEYS=0` (nothing registered).

**Rejected.** Taking Shift+Home / Shift+End from an input that holds text: selecting to the line
start by keyboard would be gone on Linux and Windows. If the owner wants the keys to scroll
there too, it is one predicate.

**Risk.** Low. `EditorView_BufferEmpty` is one more context flag; a pending multi-key chord is
cleared when a view's context changes between its keys, which this flag can now cause when a
chord's first key also empties or fills the buffer (no default chord does).

## 5. Tests

- `warpui_core` `keymap/matcher_test.rs`: the yield rule across views and inside one view, the
  control without the mark, marking by name, a two-key user chord.
- `util/bindings_tests.rs`: Alt-R default and its only claimants, the macOS menu-less map, the
  command search mark, the readline defaults per platform, the scroll bindings editable; the
  kill switches.
- `terminal/view/scrollback_keys_tests.rs`, `editor/view/mod_test.rs`: both predicates.
- appci: `keybinding_rebind` arm C and Alt-R steps; `meta_and_word_keys` (no `app_default`);
  `scroll_keys_foreground` prompt half. `docs/testing.md` "Round 16: keys".
- Not covered: macOS (the menu key equivalent, Option-R); the conversation-list chord; a user
  rebinding one of the four scroll bindings.
