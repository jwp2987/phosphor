# Round 17: misc (r17-misc): #2028, #2030, #2055, #2091, #2096

Branch `fix/r17-misc`, from `integ/r14` `04e5193a2`. Nothing here was compiled or run with cargo;
Rust is `rustfmt --check` only. The shell and Python pieces were run directly on the laptop
(bash 5.3.9, zsh, fish, strace present). Wording: "worked, unproven" until a run shows it.

## 1. #2028: the markdown viewer's first frame has no width

Cause (read, not run):

- The viewer (`FileNotebookView::new`, `app/src/notebooks/file/mod.rs:281`) builds its editor with
  `NotebooksEditorModel::new`, which is the eager (not lazy) layout mode.
- `RenderState::new` starts with a viewport of `Pixels::zero()` by `Pixels::zero()`
  (`crates/editor/src/render/model/mod.rs:2143`).
- In eager mode a `LayoutAction::BufferEdit` is laid out when it arrives
  (`mod.rs:3028`), with `layout_context()`'s width = `self.viewport.width()` (`mod.rs:3106`): 0
  before the element was ever laid out. So the whole file is shaped one glyph per line, and block
  items (the image of the issue) get a content width of 0 minus their padding (the `-20`).
- The element computes the real width in its `layout` and hands it to the model only in
  `after_layout` (`element/mod.rs:1073`, by design: no mutable model in layout). The model then
  emits `RenderEvent::NeedsResize` (`mod.rs:2707`), which the notebook model debounces by 150 ms
  (`app/src/notebooks/editor/model.rs:83`, `:508`) before `rebuild_layout`.
- So the first frame paints the width-0 layout and stays for at least 150 ms plus one full layout.

Fix: a per-model opt-in, "hold layout until the first size". While a model that opted in has never
been given a width above zero, `BufferEdit` and `LayoutTemporaryBlock` actions are queued in the
existing `pending_edits` queue (the lazy mode's queue) and NOT laid out. The first
`set_viewport_size` with a width above zero stores the size and then lays the queue out, in order,
at that width, emits `LayoutUpdated` and notifies; it does not emit `NeedsResize` (nothing was laid
out at the old width, so there is nothing to redo, and no 150 ms wait). After that the model
behaves exactly as before (one-shot). The buffer version of a held edit is published
(`next_render_version`) only when it is laid out, so a pending scroll fraction is not applied
against the empty document.

What the user sees: frame 1 is the empty document (the same picture as a viewer whose file has not
loaded), the next frame is the file laid out at the pane width. A zero-width pane (hidden) keeps
holding until it has a width.

Opt-in: `FileNotebookView` only. Kill switch: `PHOSPHOR_MD_FIRST_LAYOUT_HOLD=0` (the viewer does
not opt in; old behaviour). One log line when the hold is released:
`hop`-free, plain `log::info!("rich text: first size ... released N held layout edit(s)")`, which the
appci case reads.

Rejected:

- Passing the element's width into the lazy flush inside `layout` (a `Cell` hint): every reader of
  `viewport.width()` in that frame (the viewport iterator's `content_width`, images, tables) would
  still see 0; making them all read the hint touches the read paths of every editor.
- Making the first `NeedsResize` skip the debounce only: the width-0 frame is still painted once.
- Giving the model the pane size at construction: the view is built before any layout; the pane
  group does not know the size then either.
- Opting in every `NotebooksEditorModel` (AI documents, comments, the code pane's rendered
  markdown): models that are read without ever being shown would never lay out. Not proven safe by
  reading, so not done. The code pane's rendered remote markdown (`app/src/code/view.rs:624`) very
  likely has the same first frame; it is left as is and named in the report.

Risk: a viewer whose element is never laid out keeps its edits queued (memory: the deltas, which
share the buffer's blocks). `test_file_notebook_mermaid_blocks_default_to_rendered` reads the laid
out content after `layout_complete()`; with the hold the content exists only after the first
element layout, so the test now also waits (bounded) for the hold to be released. Its assertions
are unchanged.

## 2. #2030: remote bash prompt hooks

Measured before changing anything (laptop, bash 5.3.9, the REAL assembled bootstrap in a pty,
`script/test_hook_forks` with the shell started as a REMOTE one, `WARP_IS_LOCAL_SHELL_SESSION`
unset): 0 execs and 0 forks per prompt in a plain directory and in a git repository, in both prompt
modes. So there is no fork or subshell left to remove on the per-command path of a modern bash;
the issue's premise (forks) does not hold there. Enter-to-Precmd median of `true`: bash 5.5 to
7.7 ms, zsh 1.9 to 2.5 ms.

Committed lab numbers (r13a, `lab_enter_start.*.150.nohelper`, medians of 30): command start bash
85.7 ms / zsh 81.5 ms; next prompt bash 107.1 ms / zsh 82.0 ms. So bash is about 4 ms behind zsh
before the command and about 25 ms behind at the next prompt: the cost is on the precmd side.

Where the time goes (xtrace with `$EPOCHREALTIME` in `PS4`, one `true` cycle, 1280 traced
commands): 813 of them are the byte loop of `__warp_hex_encode_to` (`bash_body.sh:1217`): 437 for
the CommandFinished message (about 145 bytes, encoded whole on every prompt,
`bash_body.sh:727`), 313 for the Precmd head (`:992`), the rest for the Preexec command and the
tails. Three shell commands per byte. Only the exit code, the block counter and `seq` change
between prompts in those texts.

Fix: encode the fixed text of CommandFinished and of the Precmd head once per session id (as the
Preexec message already does since #1283) and join it with the hex of the numbers, which for a
string of decimal digits is `3` + the digit per digit (`__warp_hex_digits_to`; anything else goes
through the general encoder). Hex is per byte, so the joined hex is exactly the hex of the whole
message: the bytes on the wire do not change. Kill switch `PHOSPHOR_BASH_HOOK_HEX_CACHE=0` (read at
every prompt, in the shell that runs the hooks; the old whole-message encoding).

Rejected: a faster general encoder (a lookup table still loops per byte); `${d//?/3&}` (needs bash
5.2 and `patsub_replacement`, which a user can switch off); touching zsh or fish (no gap measured).
Not found by reading and therefore not changed: what makes the lab's bash 5.2 another ~4 ms slower
than zsh BEFORE the command (bash 5.2 takes `__bp_history1_viafile`, a file write and read per
command, no fork after the first `mktemp`); the next lab run's `enter_start_app_ms` says whether
that remains.

Risk: low (same bytes, pinned by a new equivalence check with the switch on and off). The cache is
keyed on `WARP_SESSION_ID` like the Preexec one.

## 3. #2091: the lost-device oracle of `gpu_real_render`

Hook: `PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE=<path>` (read once, when the event loop is built; unset in
every normal run, where the cost is one `Option` test per redraw). When set, each redraw tries to
remove that file; the redraw that succeeds reports `renderer::Error::DeviceLost` instead of
drawing, once per file. From there the app's own path runs unchanged: `DrawRetryState` decides
(recreate, count toward the exit), `Recreating the renderer` is logged, `drop_renderer` really drops
the device and `recreate_renderer` builds a new one. The path must end in
`phosphor-test-lose-gpu-device` or the variable is ignored (the app deletes that file).

Rejected: `wgpu::Device::destroy()` (later calls on a destroyed device raise validation errors
through the uncaptured-error handler, which is not the lost-device path); setting the renderer's
`device_lost` flag (it is read only when a buffer is created or grows, so a frame may not see it).

Case: a third arm in `gpu_real_render` (Linux): relaunch with the variable, draw the pattern,
screenshot, create the file, type one key (the frame that hits the loss), then judge: the hook
line and `Recreating the renderer` in the log, a new `Surface configured:` line, `hop=frame_drawn`
lines after it, the app alive, and the same screen within the picture tolerance.

Found by reading, NOT changed (new issue filed, see the report): a counted failure that rebuilds
the renderer has `retry_after: None`, so nothing asks for another redraw; the frame that hit the
loss is drawn only at the next event. The case measures it (frames drawn before the next input)
and reports it as a note, not a verdict.

## 4. #2096: the blink test's "must not notify"

Cause: as the issue says; a notify is a queued `Effect::ViewNotification` and is not in the window
invalidation before the flush. Fix: `AppContext::pending_view_notifications_for_test(window, view)`
counts the queued notifications; the test reads it before and after the tick inside the same
update and requires zero new ones. A control test makes the same observation of a real
`ctx.notify()` (one counted, and `needs_layout` still false), so the first test can fail. No app
code changes.

## 5. #2055: the six unwired harnesses

`script/precheck`: the three quick ones (`test_hijack_bootstrap`, `test_bash_preexec_large_command`,
`test_zsh_hex_encoder`) join the "shell tests" step (also `--fast`); the three slow ones
(`test_fish_bootstrap`, `test_hook_forks`, `test_line_editor_markers`, about 3.5 minutes together)
run in a new step after the `--fast` exit, so only the full precheck pays for them. `guards` job:
all six, each behind a check of what it needs (bash / zsh / fish / strace), printing
`SKIPPED <name>: no <tool> on this runner` when it is missing; the job's timeout goes from 10 to
15 minutes. A harness whose shebang interpreter is missing (`test_zsh_hex_encoder` is a zsh
script, `test_fish_bootstrap` exits when fish is missing) is skipped visibly in precheck too
instead of failing.

Cost, for the owner to weigh (not decided here): every pipeline's `guards` job, which every other
job waits for, takes up to about 3.5 minutes longer when fish, zsh and strace are on the box. What
the box has was not checked from here.
