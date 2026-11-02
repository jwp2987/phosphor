use super::super::blocklist::block::secret_redaction::{
    find_secrets_in_text, SECRET_REDACTION_REPLACEMENT_CHARACTER,
};

/// Redact all detected secrets in-place within the given string.
pub(crate) fn redact_secrets(input: &mut String) {
    let mut secrets: Vec<_> = find_secrets_in_text(input)
        .into_iter()
        .map(|r| r.byte_range)
        .collect();
    // Replace from the end to preserve indices
    secrets.sort_by_key(|range| range.start);
    for range in secrets.into_iter().rev() {
        let replacement =
            SECRET_REDACTION_REPLACEMENT_CHARACTER.repeat(range.end.saturating_sub(range.start));
        input.replace_range(range.start..range.end, &replacement);
    }
}

#[cfg(test)]
#[path = "redaction_tests.rs"]
mod tests;
