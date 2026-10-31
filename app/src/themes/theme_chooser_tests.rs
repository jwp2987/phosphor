use super::*;

/// #1151 F5: arrowing through themes only records the latest preview; exactly one theme (the
/// last) is handed to the persist step, once. Before the fix each Up/Down persisted immediately.
#[test]
fn arrow_previews_collapse_into_a_single_persist() {
    let mut pending = PendingThemePersist::default();
    assert_eq!(pending.take(), None, "nothing pending before any arrow");

    pending.note_preview(ThemeKind::Dark);
    pending.note_preview(ThemeKind::Light);
    pending.note_preview(ThemeKind::Dark);

    assert_eq!(pending.take(), Some(ThemeKind::Dark));
    assert_eq!(pending.take(), None, "persisting happens only once");
}

/// An explicit selection (click) persists immediately and supersedes any pending arrow preview,
/// so close doesn't persist a stale theme on top of it.
#[test]
fn explicit_persist_clears_the_pending_preview() {
    let mut pending = PendingThemePersist::default();
    pending.note_preview(ThemeKind::Light);
    pending.clear();
    assert_eq!(pending.take(), None);
}
