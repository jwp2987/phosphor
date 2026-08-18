use super::*;

fn snapshot(buffer_text: &str, cursor_byte_offset: usize) -> TuiCompletionInputSnapshot {
    TuiCompletionInputSnapshot {
        buffer_text: buffer_text.to_owned(),
        cursor_byte_offset,
    }
}

/// Ported from the pin's `tab_is_consumed_by_an_existing_non_completion_menu`
/// (`crates/warp_tui/src/input/view_tests.rs`), relocated because the two
/// forks route Tab differently: the pin dispatches `TuiInputAction::Complete`
/// into `TuiInputView`, where `handle_inline_menu_action` consumes it and only
/// the un-consumed case emits `TuiInputViewEvent::RequestShellCompletion`. This
/// fork has neither the action nor the event -- Tab is a session-level binding
/// (`TRIGGER_COMPLETIONS_BINDING_NAME`) that calls `request_shell_completion`
/// directly -- so the same precedence lives here, and so does its test.
///
/// The assertion is the pin's: with a non-completion menu already open, Tab
/// raises **no** completion request and leaves that menu alone.
#[test]
fn tab_is_consumed_by_an_existing_non_completion_menu() {
    assert_eq!(
        shell_completion_tab_action(false, true),
        ShellCompletionTabAction::ConsumedByOpenInlineMenu,
    );
    // The completions popup keeps Tab even if the shared suggestions mode
    // reports it as the active inline menu.
    assert_eq!(
        shell_completion_tab_action(true, true),
        ShellCompletionTabAction::CycleCandidates,
    );
    assert_eq!(
        shell_completion_tab_action(true, false),
        ShellCompletionTabAction::CycleCandidates,
    );
    // With a clear composer Tab still starts a request -- the guard must not
    // disable completion outright.
    assert_eq!(
        shell_completion_tab_action(false, false),
        ShellCompletionTabAction::RequestCandidates,
    );
}

#[test]
fn completion_requests_reject_every_stale_snapshot_dimension() {
    let input = snapshot("git che", 7);
    let request = CompletionRequestSnapshot {
        input: input.clone(),
        session_id: SessionId::from(42),
        current_working_directory: "/repo".to_owned(),
        generation: 7,
    };
    assert!(completion_request_is_current(
        &request,
        7,
        Some(&input),
        Some(SessionId::from(42)),
        Some("/repo"),
        false,
    ));

    let changed_input = snapshot("git checkout", 12);
    for is_current in [
        completion_request_is_current(
            &request,
            8,
            Some(&input),
            Some(SessionId::from(42)),
            Some("/repo"),
            false,
        ),
        completion_request_is_current(
            &request,
            7,
            Some(&changed_input),
            Some(SessionId::from(42)),
            Some("/repo"),
            false,
        ),
        completion_request_is_current(
            &request,
            7,
            Some(&input),
            Some(SessionId::from(43)),
            Some("/repo"),
            false,
        ),
        completion_request_is_current(
            &request,
            7,
            Some(&input),
            Some(SessionId::from(42)),
            Some("/other"),
            false,
        ),
        completion_request_is_current(
            &request,
            7,
            Some(&input),
            Some(SessionId::from(42)),
            Some("/repo"),
            true,
        ),
    ] {
        assert!(!is_current);
    }
}
