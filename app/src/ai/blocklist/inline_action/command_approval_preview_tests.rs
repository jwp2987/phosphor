//! Tests for the approval-card command preview (issue #847).

use super::{
    COLLAPSED_PREVIEW_MAX_CHARS, LONG_RUN_CHARS, PreviewMode, approval_notes,
    approval_requires_full_view, command_approval_preview, command_shape_summary, one_line_title,
};

/// The display text with zero-width break opportunities removed.
fn visible(text: &str) -> String {
    text.chars().filter(|c| *c != '\u{200B}').collect()
}

fn issue_847_command(token_len: usize) -> String {
    format!("echo {}; touch ~/hidden-payload.txt", "A".repeat(token_len))
}

#[test]
fn short_command_is_shown_verbatim_and_not_truncated() {
    let command = "ls -la && echo done | tee out.txt";
    let preview = command_approval_preview(command, PreviewMode::Collapsed);
    assert_eq!(preview.text, command);
    assert!(!preview.is_truncated());
    assert!(!approval_requires_full_view(command, false));
}

#[test]
fn long_token_keeps_separator_and_tail_visible() {
    let command = issue_847_command(100_000);
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);

    assert!(preview.text.starts_with("echo AAAA"));
    assert!(
        preview.text.ends_with("; touch ~/hidden-payload.txt"),
        "separator and payload must stay visible, got tail {:?}",
        &preview.text[preview.text.len().saturating_sub(60)..]
    );
    assert!(preview.text.contains(';'));
    assert!(preview.text.chars().count() < 400);
}

#[test]
fn long_token_has_marker_with_exact_hidden_count() {
    let command = issue_847_command(100_000);
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);

    // 100,000 'A's minus the 48 head and 24 tail chars kept visible.
    assert_eq!(preview.hidden_chars, 100_000 - 48 - 24);
    assert!(
        preview.text.contains("…[99,928 more chars]…"),
        "{}",
        preview.text
    );
    assert!(preview.is_truncated());
}

#[test]
fn separator_is_highlighted_as_operator() {
    let command = issue_847_command(100_000);
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    let semicolon_index = preview
        .text
        .chars()
        .position(|c| c == ';')
        .expect("separator present");
    assert!(preview.operator_char_indices.contains(&semicolon_index));
}

#[test]
fn every_operator_survives_elision() {
    let long = "x".repeat(5_000);
    let command = format!(
        "a{long};b{long}&&c{long}||d{long}|e{long}&f{long}>g{long}<h{long}$(i{long})`j{long}`'k{long}'\"l{long}\"\\m{long}\nn{long}"
    );
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    let operators: String = command
        .chars()
        .filter(|c| ";&|<>()$`'\"\\\n".contains(*c))
        .collect();
    let shown: String = preview
        .text
        .chars()
        .filter(|c| ";&|<>()$`'\"\\\n".contains(*c))
        .collect();
    assert_eq!(shown, operators);
}

#[test]
fn full_view_contains_the_exact_command() {
    let command = issue_847_command(100_000);
    let preview = command_approval_preview(&command, PreviewMode::Full);
    assert_eq!(visible(&preview.text), command);
    assert!(!preview.is_truncated());
}

#[test]
fn full_view_never_hands_the_renderer_a_word_over_its_limit() {
    // The renderer drops any single shaped word over 16,384 chars. The full view must break
    // every run well below that.
    let command = issue_847_command(100_000);
    let preview = command_approval_preview(&command, PreviewMode::Full);
    let longest_word = preview
        .text
        .split(|c: char| c.is_whitespace() || c == '\u{200B}')
        .map(|w| w.chars().count())
        .max()
        .unwrap_or(0);
    assert!(
        longest_word <= 1_024,
        "longest unbroken word is {longest_word}"
    );
}

#[test]
fn three_thousand_char_token_is_elided_not_silently_clipped() {
    let command = issue_847_command(3_000);
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.is_truncated());
    assert!(preview.text.ends_with("; touch ~/hidden-payload.txt"));
}

#[test]
fn run_at_the_threshold_is_not_elided() {
    let command = format!("echo {}", "A".repeat(LONG_RUN_CHARS));
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert_eq!(preview.text, command);
    assert!(!preview.is_truncated());
}

#[test]
fn long_whitespace_stretch_is_elided_and_tail_kept() {
    let command = format!("true{}; rm -rf ~/x", " ".repeat(20_000));
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.is_truncated());
    assert!(preview.text.contains("whitespace chars"));
    assert!(preview.text.ends_with("; rm -rf ~/x"));
}

#[test]
fn many_medium_tokens_cut_middle_with_count_and_separator_note() {
    // 2,000 short statements: no single run is long, but the whole is.
    let command = (0..2_000)
        .map(|i| format!("echo {i:05}"))
        .collect::<Vec<_>>()
        .join("; ")
        + "; touch ~/payload";
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);

    assert!(preview.is_truncated());
    assert!(preview.text.chars().count() <= COLLAPSED_PREVIEW_MAX_CHARS + 100);
    assert!(preview.text.ends_with("; touch ~/payload"));
    assert!(preview.text.contains("hidden here, including"));
    assert!(preview.text.contains("command separators"));
    // Shown plus hidden accounts for every command char.
    let shown_command_chars = preview.text.chars().count() - preview.marker_char_indices.len();
    assert_eq!(
        shown_command_chars + preview.hidden_chars,
        command.chars().count()
    );
}

#[test]
fn control_and_invisible_characters_are_escaped_visibly() {
    let command = "echo safe\r\u{1b}[2K\u{202E}txt.exe\u{200B}; rm x";
    let preview = command_approval_preview(command, PreviewMode::Collapsed);
    assert!(preview.text.contains("<U+000D>"));
    assert!(preview.text.contains("<U+001B>"));
    assert!(preview.text.contains("<U+202E>"));
    assert!(preview.text.contains("<U+200B>"));
    assert!(!preview.text.contains('\r'));
    assert!(!preview.text.contains('\u{1b}'));
    assert!(!preview.text.contains('\u{202E}'));
    assert!(!preview.text.contains('\u{200B}'));
    assert!(!preview.marker_char_indices.is_empty());
    // Escaping is display-only, not truncation.
    assert!(!preview.is_truncated());
}

#[test]
fn newlines_are_kept_as_line_breaks() {
    let command = format!("echo {}\nrm -rf ~/x", "A".repeat(10_000));
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.text.ends_with("\nrm -rf ~/x"));
}

#[test]
fn approve_is_gated_until_full_command_shown() {
    let command = issue_847_command(100_000);
    assert!(approval_requires_full_view(&command, false));
    assert!(!approval_requires_full_view(&command, true));
    assert!(!approval_requires_full_view("echo hi; ls", false));
}

#[test]
fn shape_summary_counts_statements_outside_quotes() {
    assert_eq!(command_shape_summary("ls").commands, 1);
    assert_eq!(
        command_shape_summary("a; b && c || d | e & f\ng").commands,
        7
    );
    assert_eq!(command_shape_summary("echo 'a; b' \"c && d\"").commands, 1);
    assert_eq!(command_shape_summary("cmd 2>&1 &>/dev/null").commands, 1);
    assert_eq!(command_shape_summary("echo a\\; b").commands, 1);
    assert_eq!(command_shape_summary("ls;").commands, 1);
    assert_eq!(command_shape_summary(&issue_847_command(100)).commands, 2);
}

#[test]
fn shape_summary_flags_command_substitution() {
    assert!(command_shape_summary("echo $(whoami)").has_command_substitution);
    assert!(command_shape_summary("echo `whoami`").has_command_substitution);
    assert!(command_shape_summary("echo \"$(whoami)\"").has_command_substitution);
    assert!(!command_shape_summary("echo '$(whoami)'").has_command_substitution);
    assert!(!command_shape_summary("echo $HOME").has_command_substitution);
}

#[test]
fn notes_explain_hidden_text_and_multiple_commands() {
    let command = issue_847_command(100_000);
    let collapsed = command_approval_preview(&command, PreviewMode::Collapsed);
    let notes = approval_notes(&command, &collapsed, false);
    assert!(notes[0].contains("99,928 chars of this command are hidden"));
    assert!(notes[0].contains("Show the full command to enable Run"));
    assert!(notes.iter().any(|n| n == "This runs 2 commands."));

    let notes = approval_notes(&command, &collapsed, true);
    assert!(notes[0].starts_with("Showing the full command"));
}

#[test]
fn notes_are_empty_for_a_plain_command() {
    let collapsed = command_approval_preview("ls -la", PreviewMode::Collapsed);
    assert!(approval_notes("ls -la", &collapsed, false).is_empty());
}

#[test]
fn one_line_title_elides_long_runs_and_marks_more_lines() {
    let title = one_line_title(&issue_847_command(100_000));
    assert!(title.ends_with("; touch ~/hidden-payload.txt"));
    assert!(title.contains("more chars]"));

    assert_eq!(one_line_title("cargo build\n--release"), "cargo build…");
    assert_eq!(one_line_title("git status\n   \t  "), "git status");
}
