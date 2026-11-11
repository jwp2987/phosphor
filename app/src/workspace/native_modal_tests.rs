use super::should_run_disable_callback;

/// #2092: the "don't show again" callback toggles `general.show_warning_before_quitting`. A
/// dialog that has to show whatever that setting says (unsaved editor edits) must never run it:
/// the checkbox state starts ticked when the setting is off, so running it would switch the
/// setting back on behind the user's back.
#[test]
fn a_dialog_without_the_checkbox_never_runs_the_disable_callback() {
    assert!(!should_run_disable_callback(false, true));
    assert!(!should_run_disable_callback(false, false));
}

#[test]
fn a_dialog_with_the_checkbox_runs_it_only_when_ticked() {
    assert!(should_run_disable_callback(true, true));
    assert!(!should_run_disable_callback(true, false));
}
