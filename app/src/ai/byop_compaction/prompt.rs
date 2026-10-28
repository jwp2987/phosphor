//! Summary prompt — copied byte-for-byte from opencode
//! `packages/opencode/src/session/compaction.ts:40-75, 121-132`.
//!
//! Don't "optimize" the template text — this is a mutual-portability contract with
//! opencode, and any change needs to be synced both ways.

/// Corresponds directly to `compaction.ts:40-75 SUMMARY_TEMPLATE`.
pub const SUMMARY_TEMPLATE: &str = "Output exactly the Markdown structure shown inside <template> and keep the section order unchanged. Do not include the <template> tags in your response.\n<template>\n## Goal\n- [single-sentence task summary]\n\n## Constraints & Preferences\n- [user constraints, preferences, specs, or \"(none)\"]\n\n## Progress\n### Done\n- [completed work or \"(none)\"]\n\n### In Progress\n- [current work or \"(none)\"]\n\n### Blocked\n- [blockers or \"(none)\"]\n\n## Key Decisions\n- [decision and why, or \"(none)\"]\n\n## Next Steps\n- [ordered next actions or \"(none)\"]\n\n## Critical Context\n- [important technical facts, errors, open questions, or \"(none)\"]\n\n## Relevant Files\n- [file or directory path: why it matters, or \"(none)\"]\n</template>\n\nRules:\n- Keep every section, even when empty.\n- Use terse bullets, not prose paragraphs.\n- Preserve exact file paths, commands, error strings, and identifiers when known.\n- Do not mention the summary process or that context was compacted.";

/// The `## ` section headings of [`SUMMARY_TEMPLATE`] ("## Goal", "## Progress", ...), read
/// from the template itself so they cannot drift from what the summarizer is asked for.
pub fn summary_section_headings() -> impl Iterator<Item = &'static str> {
    SUMMARY_TEMPLATE
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("## "))
}

/// The section names of the template headings, lowercase ("goal", "progress", ...).
fn summary_section_names() -> Vec<String> {
    summary_section_headings()
        .map(|heading| heading.trim_start_matches('#').trim().to_lowercase())
        .collect()
}

/// The section name a line declares, if it is a heading: `#` to `####` followed by a space,
/// or a line that is entirely bold (`**Goal**`). Lowercased, with optional numbering
/// ("1. Goal", "2) Progress"), surrounding bold inside a `#` heading, and a trailing colon
/// removed. Real models vary all of these; the section name itself must match whole.
fn heading_line_name(line: &str) -> Option<String> {
    let line = line.trim();
    let hashes = line.chars().take_while(|c| *c == '#').count();
    let rest = if (1..=4).contains(&hashes) && line[hashes..].starts_with(char::is_whitespace) {
        line[hashes..].trim()
    } else if hashes == 0 && line.len() > 4 && line.starts_with("**") && line.ends_with("**") {
        &line[2..line.len() - 2]
    } else {
        return None;
    };
    let mut name = rest.trim().trim_matches('*').trim();
    // Optional numbering: "1. Goal" / "1) Goal".
    let digits = name.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let after = &name[digits..];
        if let Some(after) = after.strip_prefix('.').or_else(|| after.strip_prefix(')')) {
            name = after.trim();
        }
    }
    let name = name.trim_end_matches(':').trim().trim_matches('*').trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

/// The template sections `text` declares as headings, in order of appearance.
fn declared_sections(text: &str) -> Vec<String> {
    let names = summary_section_names();
    text.lines()
        .filter_map(heading_line_name)
        .filter(|name| names.contains(name))
        .collect()
}

/// Whether `text` is a COMPLETE summary: it declares the template's last section ("Relevant
/// Files") as a heading. A summarize stream that also produced a tool call commits only such
/// a summary: a half-written one ("## Goal ... ## Progress ..." and then a call to gather
/// facts) would hide the head behind a partial summary.
pub fn is_complete_summary(text: &str) -> bool {
    let Some(last) = summary_section_names().pop() else {
        return false;
    };
    declared_sections(text).contains(&last)
}

/// Builds the final user prompt — matches `compaction.ts:121-132 buildPrompt`.
///
/// `previous_summary = Some(...)` → takes the "update" branch, anchoring the
/// existing summary as `<previous-summary>`; `None` → takes the "fresh" branch.
/// `context` comes from a plugin hook (the local implementation currently leaves it
/// as an empty vec).
pub fn build_prompt(previous_summary: Option<&str>, context: &[String]) -> String {
    let anchor = match previous_summary {
        Some(prev) => format!(
            "Update the anchored summary below using the conversation history above.\n\
             Preserve still-true details, remove stale details, and merge in the new facts.\n\
             <previous-summary>\n{prev}\n</previous-summary>"
        ),
        None => "Create a new anchored summary from the conversation history above.".to_string(),
    };
    let mut parts: Vec<String> = Vec::with_capacity(2 + context.len());
    parts.push(anchor);
    parts.push(SUMMARY_TEMPLATE.to_string());
    parts.extend(context.iter().cloned());
    parts.join("\n\n")
}

/// Synthesizes a user "Continue..." synthetic message on the `replay=false` +
/// `auto=true` path — matches `compaction.ts:533-537` byte-for-byte.
///
/// When `overflow=true`, an extra "previous request exceeded ... attachments were too large" explanation is prepended.
pub fn build_continue_message(overflow: bool) -> String {
    let prefix = if overflow {
        "The previous request exceeded the provider's size limit due to large media attachments. \
         The conversation was compacted and media files were removed from context. \
         If the user was asking about attached images or files, explain that the attachments were too large to process and suggest they try again with smaller or fewer files.\n\n"
    } else {
        ""
    };
    format!(
        "{prefix}Continue if you have next steps, or stop and ask for clarification if you are unsure how to proceed."
    )
}
