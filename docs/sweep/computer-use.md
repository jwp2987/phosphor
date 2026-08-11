# Sweep verdicts — `crates/computer_use/**`

Oracle: `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`.

**This area was never touched by the six-area sweep** recorded in
`docs/SWEEP-SUMMARY.md` — `crates/computer_use` is not `app/src/ai`,
`settings_view`/`workspace`/`pane_group`/`search`, `app/src/terminal`,
`warp_cli`/`onboarding`, `crates/ai`/`build_cache`, or `warp_tui`. Confirmed
empty going in: `grep -c computer_use docs/sweep-verdict-ledger.tsv` returns
hits only for *other* areas' evidence text (e.g. `app/src/ai/blocklist/action_model/recording_controller_tests.rs`,
already adjudicated in `docs/sweep/app-ai.md`); zero rows have `pin_file`
under `crates/computer_use/`.

**`crates/computer_use` is not cloud.** It runs entirely against the user's
own local desktop (screenshots, mouse/keyboard synthesis, window
enumeration). `DECLINED.md`'s "Not declined — common false positives"
section names it explicitly, alongside `remote_server` and Grok OAuth, as a
symbol that keeps getting mislabelled as cloud by name alone.

## Method

1. Listed every file the pin ships under `crates/computer_use/` (`git
   ls-tree -r --name-only 02b53fcd8 -- crates/computer_use`) and diffed
   against the fork's current tree.
2. For every pin-only test file, counted `#[test]` + `#[tokio::test]`
   attributes directly against the pin blob (`git show 02b53fcd8:<path>`) and
   extracted each test's function name, rather than trusting `SCOPE-REST.md`'s
   older aggregate counts.
3. Diffed the pin's `lib.rs` against the fork's `lib.rs` in full (not just the
   test files) to check for partial/dead wiring the test-file diff alone
   would miss — a symbol left half-ported, referenced from neither side.
4. Cross-checked every named symbol (`PointerSession`, `PointerSink`,
   `RecordingConfig`, `Recorder`, `overlay`) against the whole fork tree with
   `grep -rn`, not just the crate, to rule out a rename.

## Scope correction since `SCOPE-REST.md` — 69 → 64

`SCOPE-REST.md` (written before `DECLINED.md`'s #350 decision and before
`#349` closed) counted **69** absent tests in `crates/computer_use`, split 58
recording (C) + 11 non-cloud (D): `pointer_session_tests.rs` (6) +
`mac/activation_tests.rs` (5).

**`mac/activation_tests.rs` is no longer absent.** `DECLINED.md`'s #350 row
records "#349 is NOT covered... it was closed on 2026-08-10." Verified
directly: `crates/computer_use/src/mac/activation.rs`,
`mac/post.rs`, `mac/window.rs`, `linux/x11/seat.rs` and
`linux/x11/windows.rs` all now exist in the fork, and
`mac/activation_tests.rs` carries the pin's exact 5 test names
(`end_sessions_for_owner_clears_only_that_owners_entries`,
`tap_callback_filters_focus_by_activation_window`,
`tap_callback_addressing_matrix_passes_non_matching_identity`,
`tap_callback_passes_non_focus_target_tap_and_suppression_off`,
`tap_callback_concurrent_sessions_isolate_by_activation_window`).
`docs/PIN-IDENTITY-MANIFEST-files.tsv` independently confirms both
`mac/activation.rs` and `mac/activation_tests.rs` as `IDENTICAL` to the pin
blob. **0 absent, was 5.** This closes the gap SCOPE-REST called D
(non-cloud feature gap) — it is fully ported, not a live row here.

That leaves exactly **64** absent tests, all six of them the pin's session-
recording subsystem for computer use (video capture of a computer-use
session, for upload as a cloud artifact) — matching the task brief's count.

## Bucket counts (64 / 64 adjudicated — 100%)

| bucket | tests | files |
|---|---:|---|
| **DECLINED** (#350) | **64** | `overlay_tests.rs` (41), `pointer_session_tests.rs` (6), `linux/recording_tests.rs` (6), `mac/recording_tests.rs` (4), `recording_tests.rs` (4), `recording_metadata_tests.rs` (3) |
| PORTABLE | 0 | — |
| CLOUD | 0 | — |
| COVERED-ELSEWHERE | 0 | — |
| DIVERGENT | 0 | — |
| MISSING-SUBSYSTEM | 0 | — |

**Nothing was ported.** All 64 are already covered by `DECLINED.md`'s
"computer_use session recording" row (#350, decided 2026-08-08): *"the whole
subsystem is video capture of computer-use sessions: `mac/recording.rs`,
`linux/recording.rs`, `recording_metadata.rs`, and `overlay::build_overlay_ass`
which burns click/drag annotations into the video... Recording is the only
declined part of computer use."*

## Per-file verdicts

### `crates/computer_use/src/overlay_tests.rs` — 41 absent → DECLINED (#350)

pin 41 · fork 0 · source `crates/computer_use/src/overlay.rs` (does not exist
in fork)

`overlay.rs`'s own doc comment (quoted in `SCOPE-REST.md`): *"Action overlay
model and .ass subtitle generation for burned-in recording annotations."* Its
only pin consumers are `app/src/ai/blocklist/action_model/recording_controller.rs`
(app-side, already adjudicated DECLINED in `docs/sweep/app-ai.md`'s
`recording_controller_tests.rs` row) and `use_computer.rs`. The fork's
`crates/ai::UseComputerExecutor` (`app/src/ai/blocklist/action_model/execute/use_computer.rs`)
builds `computer_use::Options { screenshot_params, background_enabled }` —
two fields, no `pointer_sink`, no recorder — confirming the overlay burn-in
has no producer to feed it. `grep -rn "overlay\|Overlay" crates/computer_use/src/`
returns zero hits outside a single unrelated macOS "Screen Recording
permission" comment in `mac/window.rs` (the OS permission needed to read
window titles, unrelated to this feature). Covered by #350, quoted above.

### `crates/computer_use/src/pointer_session_tests.rs` — 6 absent → DECLINED (#350)

pin 6 · fork 0 · source: `PointerSession`/`PointerSink`, defined inline in
`crates/computer_use/src/lib.rs` (pin lines 639–742; absent from the fork's
`lib.rs` entirely)

This is the row most likely to be mistaken for portable — a pure
press/move/release state machine reads as local input bookkeeping with no
recording dependency (`SCOPE-REST.md` originally bucketed it **D**, before
#350 was decided). Independently re-verified the pin source rather than
trusting the decline at face value: `PointerSink`'s own doc comment reads
*"Collects resolved pointer events **during a recording** so the finalize
pass can burn in click/drag annotations. Only the Linux x11 actor populates
it,"* and `PointerSession`'s doc comment: *"**Recording-scoped** pointer
session state, shared between the recording controller and each
`UseComputer` call's `PointerSink`... Owned by the active recording, which
hands an `Arc` clone to each call's sink... The finalize pass classifies one
flattened recording-level pointer stream (see `overlay::build_overlay_ass`)."*
`Options.pointer_sink: Option<PointerSink>` is documented `None` on every
non-recording path. There is no pin call site that constructs a
`PointerSession` outside the recording controller. `DECLINED.md`'s framing —
*"sound independent but exist solely to stitch pointer events across the
discrete `UseComputer` calls that make up one recording... not separable"* —
holds up against the source, not just the decline text. Covered by #350.

### `crates/computer_use/src/linux/recording_tests.rs` — 6 absent → DECLINED (#350)

pin 6 (3 `#[test]` + 3 `#[tokio::test]`) · fork 0 · source
`crates/computer_use/src/linux/recording.rs` (does not exist in fork)

Tests: `records_window_target_via_native_x11grab_after_raise`,
`visibility_samples_stay_inside_window`, `records_full_display_for_screen_target`,
`linux_capture_command_captures_at_1x_without_setpts`,
`build_cut_only_filtergraph_constructs_trim_setpts_concat`,
`smart_cut_retains_only_selected_frames_in_order`. All exercise the Linux
`ffmpeg`/x11grab `Recorder` implementation. Covered by #350 (quoted above:
"`linux/recording.rs`" named explicitly).

### `crates/computer_use/src/mac/recording_tests.rs` — 4 absent → DECLINED (#350)

pin 4 · fork 0 · source `crates/computer_use/src/mac/recording.rs` (does not
exist in fork)

Tests: `applies_setpts_filter_when_playback_speed_exceeds_one`,
`omits_setpts_filter_when_playback_speed_is_real_time`,
`limits_duration_as_an_input_option_before_i`,
`ignores_window_target_until_window_scoped_recording_lands`. The macOS
`avfoundation` `Recorder` implementation. Covered by #350 (`mac/recording.rs`
named explicitly).

### `crates/computer_use/src/recording_tests.rs` — 4 absent → DECLINED (#350)

pin 4 (3 `#[test]` + 1 `#[tokio::test]`) · fork 0 · source: `RecordingHandle`
/ `RecordingExitKind` / `Recorder` trait, defined inline in `lib.rs` (pin
lines 52–482; entirely absent from the fork's `lib.rs`)

Tests: `observes_synthetic_recording_exit`,
`removes_unclaimed_output_when_handle_is_dropped`,
`removes_unclaimed_output_when_handle_is_dropped_macos`,
`start_reports_unsupported_when_ffmpeg_absent`. The platform-neutral
recording handle / exit-polling / cleanup-on-drop machinery shared by both
platform recorders. Covered by #350 (`recording_tests.rs` named explicitly).

### `crates/computer_use/src/recording_metadata_tests.rs` — 3 absent → DECLINED (#350)

pin 3 (2 `#[test]` + 1 `#[tokio::test]`) · fork 0 · source
`crates/computer_use/src/recording_metadata.rs` (does not exist in fork)

Tests: `parses_ffmpeg_container_duration`, `rejects_missing_or_invalid_duration`,
`probes_duration_after_timestamp_rescaling`. Probes a finalized recording's
media-timeline duration via `ffprobe`; consumed only by
`finalized_video_duration`, itself only called from the (absent)
`recording_controller.rs`. Covered by #350 (`recording_metadata.rs` named
explicitly).

## Full `lib.rs` diff — confirms zero collateral damage

Diffed the pin's `crates/computer_use/src/lib.rs` against the fork's in full
(not just test-file presence), specifically to rule out the failure mode
`CLAUDE.md` and `ORACLE.md` warn about — a MIXED file where declining one
facet silently drags down an unrelated one. **Every line of that diff is
part of the recording/overlay/pointer-session subsystem**: the `mock`,
`overlay`, `recording_metadata` module declarations; `RecordingExitKind`,
`RecordingError`, `RecordingConfig`, `RecordingHandle`, `RecordingOutput`,
`RecordingCompletionStatus`; the `Recorder` trait and `create_recorder`/
`post_process_recording`/`finalized_video_duration` functions; `PointerSink`/
`PointerSession`/`PointerSessionState`; `Action::is_no_op` (whose sole pin
consumer, `should_decorate_recorded_use_computer` in
`app/src/ai/blocklist/block/view_impl/output.rs`, was itself recording-gated
— see below); and the two `#[cfg(test)] mod recording_tests;` /
`mod pointer_session_tests;` declarations. Nothing outside that subsystem
changed. `grep -rniE "recording|pointersession|pointersink|overlay"
crates/computer_use/src/` finds zero live references anywhere in the fork's
crate — no half-wired remnant, no renamed survivor.

## Deliverable answers

**(1) Counts.** 64/64 DECLINED. 0 PORTABLE, 0 CLOUD, 0 COVERED-ELSEWHERE, 0
DIVERGENT, 0 MISSING-SUBSYSTEM.

**(2) Ported vs. left.** Nothing ported — all 64 are already correctly
covered by the existing `DECLINED.md` #350 decision, verified independently
against pin source rather than taken on faith (the `PointerSession` row in
particular, since it is the one that reads as separable and isn't). Nothing
was left unadjudicated.

**(3) Fork code defect found: none, in this area.** The one candidate that
looked like a regression at first — `Action::is_no_op()` dropped from
`lib.rs` — traced clean. Its only pin call site was
`should_decorate_recorded_use_computer` (recording-gated), and the fork's
`output.rs` replaces that mechanism outright with
`should_decorate_blind_use_computer_screenshot` (see next item), documented
inline as the deliberate substitute and correctly citing `DECLINED.md` #350.
No dangling reference, no silently-changed behavior.

**(4) Sighted computer-use rework — no interaction with this area's 64
tests, confirmed rather than assumed.** The recent "screenshots delivered to
the model" work (`ContentPart::Binary` on the following user message, gated
on `AttachmentCaps::images`, `screenshot.delivery` status,
`app/src/ai/agent_providers/tools/computer.rs`'s "Screenshots reach the
model, but not through the tool result" section) is orthogonal to session
*recording* — one is about whether the model sees a screenshot, the other is
about capturing a video of the session for upload as a cloud artifact. The
fork's own code says so explicitly:
`app/src/ai/blocklist/block/view_impl/output.rs`'s
`should_decorate_blind_use_computer_screenshot` doc comment names the pin's
recording-footer mechanism it replaces and cites `DECLINED.md` #350 by name.
None of the 64 tests here reference screenshot delivery, `AttachmentCaps`, or
`ContentPart`; the sighted work neither unblocks nor conflicts with any of
them. (Outside my area, but worth flagging since I read the file anyway: the
app-side "blind screenshot footer" is a genuinely new, well-reasoned
mechanism worth a second pair of eyes if a sibling agent is covering
`app/src/ai/blocklist/block/**`.)

**(5) Ranked list of what I am least sure compiles.** Nothing — zero lines
of code were changed in this area. All 64 tests are DECLINED, not ported, so
there is nothing from this task at risk of a build break. The only files
touched are this new doc.

## Proposed `DECLINED.md` enrichment (not applied — another agent owns that file)

The existing #350 row already fully covers this area; nothing here
contradicts or needs to reverse it. One precision gap: the row states an
explicit count only for `pointer_session_tests.rs` ("(6)"), leaving the other
five file names uncounted. Suggest appending counts for symmetry, using the
numbers verified in this sweep — `overlay_tests.rs` (41),
`recording_metadata_tests.rs` (3), `recording_tests.rs` (4),
`mac/recording_tests.rs` (4), `linux/recording_tests.rs` (6) — and, since
`#349` closing is now independently confirmed here too (`mac/activation_tests.rs`
is `IDENTICAL` to the pin per `docs/PIN-IDENTITY-MANIFEST-files.tsv`), no
change needed to that sentence — it already says "closed on 2026-08-10."
