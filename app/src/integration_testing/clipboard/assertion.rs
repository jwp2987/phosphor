use warpui::{async_assert_eq, integration::AssertionCallback};

pub fn assert_clipboard_contains_string(string: String) -> AssertionCallback {
    Box::new(move |app, _window_id| {
        let clipboard = app.update(|ctx| ctx.clipboard().read());
        let content = match clipboard.paths {
            Some(paths) => paths.join(" "),
            None => clipboard.plain_text,
        };

        // Show both sides. A bare `async_assert_eq!` renders as "(left = right)",
        // which for a multi-line markdown round-trip says nothing at all -- not
        // even whether the clipboard was empty.
        async_assert_eq!(
            content,
            string,
            "clipboard mismatch\n--- expected ---\n{string}\n--- actual ---\n{content}\n--- end ---"
        )
    })
}
