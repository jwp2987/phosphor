/// Truncate text from the end with ellipsis if it exceeds max_length.
/// Properly handles UTF-8 character boundaries to avoid panics.
pub fn truncate_from_end(text: &str, max_length: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_length {
        text.to_string()
    } else {
        let chars_to_take = max_length.saturating_sub(1);
        let truncated: String = text.chars().take(chars_to_take).collect();
        format!("{truncated}…")
    }
}

/// Truncate text from the beginning with ellipsis if it exceeds max_length.
/// Properly handles UTF-8 character boundaries to avoid panics.
pub fn truncate_from_beginning(text: &str, max_length: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_length {
        text.to_string()
    } else {
        let chars_to_take = max_length.saturating_sub(1);
        let truncated: String = text.chars().skip(char_count - chars_to_take).collect();
        format!("…{truncated}")
    }
}

/// Returns the longest prefix of `text` that is at most `max_bytes` long and ends on a
/// char boundary. `O(1)` in the length of `text`: it never walks the whole string.
pub fn prefix_on_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Display text for a one-line row that may be fed an arbitrarily long string, such as a
/// shell history entry (#868): `text` itself when it is at most `max_bytes` long,
/// otherwise its first `max_bytes` (on a char boundary) followed by an ellipsis.
///
/// A single-line row clips its text anyway, but laying the text out still costs time in
/// its full length, so a 50 MB history line stalled every frame its row was visible. Use
/// this only for display; whatever the row accepts or executes must stay the full text.
pub fn bounded_display_text(text: &str, max_bytes: usize) -> String {
    let shown = prefix_on_char_boundary(text, max_bytes);
    if shown.len() == text.len() {
        text.to_owned()
    } else {
        format!("{shown}…")
    }
}

#[cfg(test)]
#[path = "truncation_tests.rs"]
mod tests;
