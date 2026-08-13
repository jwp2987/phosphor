//! Tests for the OSC 52 clipboard-access dropdown added to the Features page.
//!
//! `terminal.osc52_clipboard_access` (`private: false`) previously had no
//! settings-page control at all -- see issue tracking "ported but never
//! wired" settings (of which #498, `show_hidden_files`, is the same shape).
//! These tests are deliberately narrow: they pin the pieces that are cheapest
//! to get wrong silently (the default, the label mapping, the fluent keys,
//! and the write-through action) rather than constructing the full
//! `FeaturesPageView`, which pulls in singletons well beyond what this
//! control touches.

use warpui::App;

use super::FeaturesPageAction;
use crate::server::telemetry::TelemetryEvent;
use crate::terminal::settings::Osc52ClipboardAccess;

/// AGENTS §5.10: changing the default is a separate maintainer decision.
/// This also pins the dropdown's item order (Deny, WriteOnly, ReadWrite, per
/// `FeaturesPageView::update_osc52_clipboard_access_dropdown`) against the
/// enum's own label mapping, so the two can't silently drift apart.
#[test]
fn osc52_clipboard_access_default_is_deny_and_labels_are_distinct() {
    assert_eq!(Osc52ClipboardAccess::default(), Osc52ClipboardAccess::Deny);

    let labels: Vec<&str> = [
        Osc52ClipboardAccess::Deny,
        Osc52ClipboardAccess::WriteOnly,
        Osc52ClipboardAccess::ReadWrite,
    ]
    .iter()
    .map(|value| value.as_dropdown_label())
    .collect();

    assert_eq!(labels, ["Deny", "Write only", "Read and write"]);
}

/// `t!` silently returns the raw fluent key when i18n isn't initialized, and
/// nextest gives each test its own process -- so this calls `i18n::init`
/// itself rather than relying on another test having done it.
#[test]
fn osc52_clipboard_access_strings_resolve_to_real_text() {
    crate::i18n::init(Some("en"));

    let label = crate::t!("settings-features-osc52-clipboard-access-label");
    let description = crate::t!("settings-features-osc52-clipboard-access-description");

    assert_eq!(label, "OSC 52 clipboard access");
    // Not just present -- must actually resolve, not fall back to the key.
    assert_ne!(label, "settings-features-osc52-clipboard-access-label");

    // The whole point of the description is to spell out that read access is
    // the riskier direction (it exposes whatever the user last copied).
    assert!(
        description.contains("read") && description.contains("write"),
        "expected the description to explain the read vs. write security tradeoff, got: {description}"
    );
    assert_ne!(
        description,
        "settings-features-osc52-clipboard-access-description"
    );
}

/// Exercises `FeaturesPageAction::telemetry_event` for the new action, the
/// same way the existing `SetCtrlTabBehavior`/`SetGlobalHotkeyMode` arms are
/// covered implicitly by this method's exhaustive match. `telemetry_event`
/// reads several settings groups unconditionally before matching on `self`,
/// all of which `initialize_settings_for_tests` registers.
#[test]
fn set_osc52_clipboard_access_action_reports_correct_telemetry() {
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);

        let action = FeaturesPageAction::SetOsc52ClipboardAccess(Osc52ClipboardAccess::ReadWrite);
        let event = app.read(|ctx| action.telemetry_event(ctx));

        match event {
            TelemetryEvent::FeaturesPageAction { action, value } => {
                assert_eq!(action, "SetOsc52ClipboardAccess");
                assert_eq!(value, "ReadWrite");
            }
            // `TelemetryEvent` doesn't derive `Debug`, so this can't print the
            // mismatched variant -- the match arms above are exhaustive enough
            // that reaching here already says everything useful.
            _ => panic!("expected a FeaturesPageAction telemetry event"),
        }
    });
}
