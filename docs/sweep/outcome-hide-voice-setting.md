# Outcome: hide the voice-input setting (no `Transcriber` impl exists)

Task package: voice input is exposed in Settings but cannot work — `git grep
'impl Transcriber for'` returns nothing, confirmed before and after this
change. The backend was Wispr (cloud); `DECLINED.md`'s "Voice input" section
records it KEEP-DROPPED (maintainer, 2026-08-02). Follow the precedent already
in the tree for exactly this situation: `AppAnalyticsWidget` (telemetry) is
retained but gated on `ChannelState::is_telemetry_available()`, hard-coded
`false`, so it never renders — `app/src/settings_view/privacy_page.rs:1404-1416`.

Base: step-zero reset to `origin/main` at `206799556` ("feat(usage-suite): run
real-shell scenarios by DEFAULT - the race is sandbox-only"). Written,
unverified — `cargo`/`nextest`/`script/precheck` are off-limits per the task
brief; only `rustfmt --check` and the pure-shell `script/check_*` guards were
run.

## What was changed

**New availability predicate**, `app/src/voice/transcriber.rs`:

```rust
pub fn voice_transcription_available() -> bool {
    false
}
```

Placed next to `VoiceTranscriber` (the singleton `Transcriber` holder), with a
doc comment stating why it is `false` today (no `impl Transcriber for`
anywhere in the tree; the one implementation Warp ships,
`ServerVoiceTranscriber`, calls the cloud Wispr STT endpoint this fork does
not have) and exactly what would flip it to `true` (a local/BYOP
transcription engine — e.g. a bundled Whisper-class model — wired up as a real
`Transcriber` impl and injected in `app/src/lib.rs` in place of
`VoiceTranscriber::disabled()`).

**Widget gated on it**, `app/src/settings_view/ai_page.rs`, `VoiceWidget::should_render`:

```rust
fn should_render(&self, app: &AppContext) -> bool {
    voice_transcription_available()
        && cfg!(feature = "voice_input")
        && UserWorkspaces::as_ref(app).is_voice_enabled()
}
```

`voice_transcription_available()` is `&&`-ed in front of the two checks that
were already there, so it short-circuits everything else. `VoiceWidget`
renders both the `VoiceInputEnabled` toggle and, conditionally, the
`VoiceInputToggleKey` dropdown (`render_voice_section`, same file, lines
~6169-6237) — hiding the widget hides both controls, since neither can do
anything without a transcriber: the toggle enables a feature that can never
transcribe, and the toggle-key dropdown configures a shortcut for that same
dead feature.

**Test added**, `app/src/voice/transcriber_tests.rs` (new file, wired via
`#[cfg(test)] #[path = "transcriber_tests.rs"] mod tests;` at the bottom of
`transcriber.rs` — the codebase's established convention, e.g.
`app/src/tab.rs` → `tab_tests.rs`, `app/src/lib.rs` → `lib_tests.rs`; used that
instead of an inline `mod tests { ... }` block for consistency):

```rust
#[test]
fn voice_transcription_is_not_available_without_a_transcriber_impl() {
    assert!(!voice_transcription_available());
}
```

This is the one testable surface here — `should_render` itself needs a live
`AppContext`/`UserWorkspaces` singleton to call, which is exactly the kind of
harness-dependent test the task said not to add if it can't run standalone.

## What was deliberately left in place, and why

- **`app/src/voice/` and the `Transcriber` trait** (`transcriber.rs:31`) —
  untouched. `VoiceTranscriber` (the `None`/`Some(Arc<dyn Transcriber>)`
  singleton wrapper the task brief calls "`MaybeTranscriber`" — the actual
  type name in this tree is `VoiceTranscriber`, confirmed by grep; no type
  named `MaybeTranscriber` exists anywhere in the tree) is untouched.
  `VoiceTranscriber::disabled()` (constructed in `app/src/lib.rs:1880`) keeps
  working exactly as before.
- **`VoiceInputEnabled` and `VoiceInputToggleKey` settings themselves** —
  still registered, still declared via `implement_setting_for_enum!` /
  the `AISettings` group, still load and save through the normal settings
  pipeline. Only the *widget* that renders them was gated; the `Setting`
  trait impl that governs (de)serialization is a completely separate layer
  from `SettingsWidget::should_render`, so nothing about persistence changed.
- **The footer voice UI** (`CLIVoiceInputState::Transcribing` and friends,
  `app/src/ai/blocklist/agent_view/agent_input_footer/mod.rs:1434-1525`) —
  untouched. It already has its own gates
  (`UserWorkspaces::is_voice_enabled()`, `AISettings::is_voice_input_enabled()`)
  and already handles `VoiceTranscriber::disabled()` gracefully (`transcriber()`
  returns `None`, so `handle_cli_voice_session_result`'s `if let Some(transcriber)
  = ...` branch is simply never taken — recording works, transcription silently
  doesn't happen, exactly the pre-existing documented behavior in `DECLINED.md`).
  This was out of scope for "hide the setting" and changing it risked a
  regression the task didn't ask for.
- **`crates/voice_input`** (`VoiceInputLifecycle`, the real `cpal`
  audio-capture state machine) — untouched, never a candidate for removal.
  `DECLINED.md` explicitly warns this is a *different* type from anything
  voice-transcription-related and must not be flagged absent.

## Persisted-value check (`agents.voice.voice_input_toggle_key`)

Traced `implement_setting_for_enum!` (`crates/settings/src/macros.rs:518`):
it generates the `Setting`/`ToggleableSetting` impl (`storage_key`,
`toml_path`, `register`, load/save) independently of any UI concept — there is
no `should_render` or widget-visibility check anywhere in that macro or in the
settings-load path. `SettingsWidget::should_render` is purely a rendering-time
gate on a separate trait (`app/src/settings_view/settings_page.rs`), consulted
only when building the settings page's widget list. Since this change never
touches `VoiceInputToggleKey::register` or its `AISettings`-group
registration, a user with `agents.voice.voice_input_toggle_key` already in
their TOML keeps loading it exactly as before — no error, no reset to
default, no data loss. This is structurally identical to how
`IsTelemetryEnabled` behaves today (its widget has been hard-`false`-gated
since the telemetry channel was pulled, and the setting itself has never
needed special-casing on load).

## Gates run

- `rustfmt --check` on every touched/added file
  (`app/src/voice/transcriber.rs`, `app/src/voice/transcriber_tests.rs`): both
  clean, zero diff.
  `app/src/settings_view/ai_page.rs` reports pre-existing diffs — confirmed via
  `git stash` + re-run that the exact same diffs (same content, only line
  numbers shifted by the one inserted `use` line) exist on the file *before*
  this change too. This local `rustfmt 1.8.0` disagrees with the repo's
  committed formatting on this file already, independent of anything touched
  here (matches the documented pattern in `docs/sweep/outcome-small-gaps.md`
  and `HANDOFF.md`). The only new line this change adds to that diff is the
  new `use crate::voice::transcriber::voice_transcription_available;`
  import, which the same churn wants moved/reformatted along with every other
  pre-existing import in the block — not a defect introduced by this change.
  The `should_render` function body itself produces zero new diff lines.
- `script/check_settings_registry`: ok — "50 group(s) registered in both".
- `script/check_stub_coverage`: ok — "no test targets a gutted stub".
- `script/check_cloud_boundary`: ok — "270 allowlisted import sites"
  (unchanged; this change adds no new cloud imports).
- `script/check_declined_collisions`: ok.
- `script/check_sweep_ledger`: ok.
- **Not run** (build freeze per task instructions): `cargo`, `nextest`,
  `script/precheck`. Cannot claim this compiles or the added test actually
  passes — only that it is sourced from a grep-verified premise, mirrors an
  existing working pattern (`AppAnalyticsWidget`) line-for-line in structure,
  and passes every gate that can run without a compiler.

## Exact condition that should un-hide the setting

`voice_transcription_available()` in `app/src/voice/transcriber.rs` returns
`true` once — and only once — a real `Transcriber` implementation exists and
is injected as the app's `VoiceTranscriber` in `app/src/lib.rs` in place of
`VoiceTranscriber::disabled()` (e.g. a local/BYOP engine such as a bundled
Whisper-class model). No other change is needed: `VoiceWidget::should_render`
already reads that predicate, so the enable toggle and the toggle-key dropdown
reappear automatically the moment it flips.

## Unfinished

- Nothing left mid-flight in this change. The footer voice UI's silent
  "records but never transcribes" behavior is pre-existing (documented in
  `DECLINED.md`) and intentionally not touched here — fixing that is the same
  larger "wire up a local transcriber" project that would also flip
  `voice_transcription_available()`, not a follow-up to this task.
- Everything above is **written, unverified**: no `cargo check`/`test` was
  run. The change is two small, additive edits (one new `&&` clause, one new
  free function, one new test file) with no signature changes to anything
  else, so the blast radius for a compile break is small, but it has not been
  compiled in this session.
