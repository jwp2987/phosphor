use std::borrow::Cow;

use itertools::Itertools;
use warp_util::path::ShellDialect;
use warpui::clipboard::ClipboardContent;

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
