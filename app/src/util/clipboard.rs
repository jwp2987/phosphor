use std::borrow::Cow;

use itertools::Itertools;
use warp_util::path::ShellDialect;
use warpui::clipboard::{
    BackgroundTextRead, ClipboardContent, ReadKind, StartedRead, read_in_background,
};
use warpui::{SingletonEntity as _, View, ViewContext};

use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

/// Returns a string representation of the ClipboardContent with any paths properly escaped if there is a known shell. If not, do not escape the paths.
pub fn clipboard_content_with_escaped_paths(
    mut content: ClipboardContent,
    shell_dialect: Option<ShellDialect>,
    replace_newlines_with_spaces: bool,
) -> String {
    if replace_newlines_with_spaces {
        content = ClipboardContent {
            plain_text: content.plain_text.replace("\n", " ").to_string(),
            ..content
        }
    }
    match content.paths {
        Some(paths) => paths
            .iter()
            .map(|path| match shell_dialect {
                Some(shell_dialect) => shell_dialect.escape(path),
                None => Cow::Borrowed(path.as_ref()),
            })
            .join(" "),
        None => content.plain_text,
    }
}

/// What to tell the user when an off-thread clipboard read (`warpui::clipboard::read_in_background`)
/// gave no content, or `None` when it did (#1380, #1382).
pub fn read_failure_message(read: &BackgroundTextRead) -> Option<String> {
    match read {
        BackgroundTextRead::Content(_) => None,
        BackgroundTextRead::TimedOut => {
            Some("Paste failed \u{2014} the clipboard did not answer".to_string())
        }
        BackgroundTextRead::TooLarge(bytes) => Some(format!(
            "Paste too large \u{2014} the clipboard holds {} MB (limit {} MB); nothing was pasted",
            bytes / (1024 * 1024),
            warpui::clipboard::MAX_PASTE_BYTES / (1024 * 1024)
        )),
    }
}

/// Shows an error toast for a failed clipboard read from a view's callback (no-op without a toast
/// stack, as in headless tests).
pub fn show_read_failure_toast<V: View>(message: String, ctx: &mut ViewContext<V>) {
    if !ctx.has_singleton_model::<ToastStack>() {
        log::warn!("{message}");
        return;
    }
    let window_id = ctx.window_id();
    ToastStack::handle(ctx).update(ctx, |toasts, ctx| {
        toasts.add_ephemeral_toast(DismissibleToast::error(message), window_id, ctx);
    });
}

/// Reads the clipboard off the UI thread for `view` and calls `on_read` with the result (#1380).
/// Where the platform cannot read off-thread the read happens here and `on_read` runs at once,
/// so behaviour (and ordering against later input) is then exactly the old synchronous one.
pub fn read_clipboard_for_view<V: View>(
    view: &mut V,
    ctx: &mut ViewContext<V>,
    kind: ReadKind,
    on_read: impl FnOnce(&mut V, BackgroundTextRead, &mut ViewContext<V>) + 'static,
) {
    match read_in_background(ctx.clipboard(), kind) {
        StartedRead::Ready(read) => on_read(view, read, ctx),
        StartedRead::Pending(pending) => {
            ctx.spawn(pending, on_read);
        }
    }
}

/// Announces text that a paste has just inserted, for screen readers (#1380). Called after the
/// insert lands, so no clipboard read is needed at action time.
pub fn announce_pasted_text<V: View>(text: &str, ctx: &mut ViewContext<V>) {
    ctx.emit_a11y_content(
        warpui::accessibility::AccessibilityContent::new_without_help(
            format!("Pasting: {text}"),
            warpui::accessibility::WarpA11yRole::UserAction,
        ),
    );
}
