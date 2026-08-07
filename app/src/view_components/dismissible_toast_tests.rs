use std::time::Duration;

use warp_core::ui::appearance::Appearance;
use warpui::platform::WindowStyle;
use warpui::{App, ViewHandle};

use super::{DismissibleToast, DismissibleToastStack, ToastFlavor};

fn stack_handle(app: &mut App) -> ViewHandle<DismissibleToastStack<()>> {
    app.add_singleton_model(|_| Appearance::mock());
    let (_, stack) = app.add_window(WindowStyle::NotStealFocus, |_| {
        DismissibleToastStack::new(Duration::from_secs(30))
    });
    stack
}

fn toast(text: impl Into<String>) -> DismissibleToast<()> {
    DismissibleToast::new(text.into(), ToastFlavor::Default)
}

// NOTE: the pin's `add_ephemeral_toasts_caps_at_three_newest`,
// `add_persistent_toasts_caps_at_three_newest`, `evicted_ephemeral_toast_aborts_timer`,
// `object_id_dedup_then_cap` (cap-at-three-newest eviction), and
// `toggle_message_expanded_flips_per_toast_state`, `expand_state_is_per_toast`,
// `truncation_predicate_is_correct`,
// `newline_heavy_message_is_truncated_to_collapsed_lines`,
// `expand_toggle_accepts_enter_and_space_without_modifiers` (message
// truncation/expand-collapse) are NOT ported here: the fork's
// `DismissibleToastStack`/`DismissibleToast` have neither a newest-three cap
// nor a truncate/expand feature. See the tracking issue filed alongside this
// port for the feature gap.
#[test]
fn manual_dismiss_and_clear_paths_unchanged() {
    App::test((), |mut app| async move {
        let stack = stack_handle(&mut app);
        let uuid = stack.update(&mut app, |stack, ctx| {
            stack.add_persistent_toast(toast("dismiss me"), ctx);
            stack.toasts[0].uuid
        });
        stack.update(&mut app, |stack, ctx| {
            stack.dismiss_toast_by_uuid(&uuid, ctx);
            assert!(stack.toasts.is_empty());
            stack.add_persistent_toast(toast("clear me"), ctx);
            stack.clear_toasts(ctx);
            assert!(stack.toasts.is_empty());
        });
    });
}
