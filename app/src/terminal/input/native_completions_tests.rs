use super::*;
use warp_completer::meta::Span;

fn inputs() -> NativeCompletionsInputs {
    NativeCompletionsInputs {
        trigger: CompletionsTrigger::Keybinding,
        shell_supports_native: true,
        multi_line: false,
        is_ai_input: false,
        remote_without_helper: true,
        flag_or_forced: false,
        remote_native_enabled: true,
    }
}

/// #1801: an explicit Tab in a remote session without the helper asks the remote shell first.
/// Terminal mode and a shell command typed in agent mode are the same input here (a shell
/// command), so both get it.
#[test]
fn explicit_tab_in_a_remote_session_without_the_helper_prefers_the_shell() {
    assert_eq!(
        native_completions_use(inputs()),
        NativeCompletionsUse::Preferred
    );
    // The upstream flag on as well changes nothing: the shell is still asked first.
    assert_eq!(
        native_completions_use(NativeCompletionsInputs {
            flag_or_forced: true,
            ..inputs()
        }),
        NativeCompletionsUse::Preferred
    );
}

/// The completions-while-typing menu never puts a request on the session's PTY per keystroke
/// burst, and a slash-command menu is not a shell completion.
#[test]
fn only_an_explicit_tab_asks_the_remote_shell() {
    for trigger in [
        CompletionsTrigger::AsYouType,
        CompletionsTrigger::SlashCommandAutoOpen,
    ] {
        assert_eq!(
            native_completions_use(NativeCompletionsInputs {
                trigger,
                ..inputs()
            }),
            NativeCompletionsUse::None,
            "{trigger:?}"
        );
    }
}

/// Natural language for the agent (agent mode, AI input) is not a shell command line.
#[test]
fn agent_text_is_never_sent_to_the_shell() {
    assert_eq!(
        native_completions_use(NativeCompletionsInputs {
            is_ai_input: true,
            ..inputs()
        }),
        NativeCompletionsUse::None
    );
}

/// With the helper connected (or a local session), nothing changes: the completer answers,
/// and the shell is asked only through upstream's flag, as a fallback.
#[test]
fn sessions_with_the_helper_or_local_keep_the_completer() {
    let with_helper = NativeCompletionsInputs {
        remote_without_helper: false,
        ..inputs()
    };
    assert_eq!(
        native_completions_use(with_helper),
        NativeCompletionsUse::None
    );
    assert_eq!(
        native_completions_use(NativeCompletionsInputs {
            flag_or_forced: true,
            ..with_helper
        }),
        NativeCompletionsUse::Fallback
    );
}

#[test]
fn unsupported_shells_multi_line_text_and_the_kill_switch_keep_the_completer() {
    for case in [
        NativeCompletionsInputs {
            shell_supports_native: false,
            ..inputs()
        },
        NativeCompletionsInputs {
            multi_line: true,
            ..inputs()
        },
        NativeCompletionsInputs {
            remote_native_enabled: false,
            ..inputs()
        },
    ] {
        assert_eq!(
            native_completions_use(case),
            NativeCompletionsUse::None,
            "{case:?}"
        );
    }
}

#[test]
fn kill_switch_reads_zero_as_off() {
    assert!(remote_native_completions_enabled_from_env(None));
    assert!(remote_native_completions_enabled_from_env(Some("1")));
    assert!(remote_native_completions_enabled_from_env(Some("")));
    assert!(!remote_native_completions_enabled_from_env(Some("0")));
    assert!(!remote_native_completions_enabled_from_env(Some(" 0 ")));
}

/// The shell writes each candidate as the whole token it replaces, so the span is the token
/// before the cursor: after the last whitespace, or the whole text before the cursor.
#[test]
fn native_results_replace_the_token_before_the_cursor() {
    let results = native_suggestion_results(
        vec![ShellCompletion::new("projects/".to_owned())],
        "cd pro",
        6,
    );
    assert_eq!(results.replacement_span, Span::from((3, 6)));
    assert_eq!(results.suggestions.len(), 1);

    let results = native_suggestion_results(vec![], "gi", 2);
    assert_eq!(results.replacement_span, Span::from((0, 2)));

    // A trailing space: the token is empty, at the cursor.
    let results = native_suggestion_results(vec![], "ls ", 3);
    assert_eq!(results.replacement_span, Span::from((3, 3)));

    // Only the text before the cursor counts, and a multi-byte character before the token
    // does not shift it.
    let results = native_suggestion_results(vec![], "é\u{3000}ab cd", 7);
    assert_eq!(results.replacement_span, Span::from((5, 7)));
}
