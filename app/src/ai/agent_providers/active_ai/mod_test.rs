use super::nld_predict::allowed_for_session;

/// #1982: a query prediction sends the input buffer to the model provider at every typing pause.
/// In an ssh session, and for a pane whose session is not known, it is requested only when the
/// user turned `natural_language_autosuggestions_in_ssh_enabled` on.
#[test]
fn a_query_prediction_is_not_requested_in_an_ssh_session_unless_allowed() {
    let in_ssh = Some(true);
    let local = Some(false);
    let unknown = None;

    assert!(allowed_for_session(local, false));
    assert!(!allowed_for_session(in_ssh, false));
    assert!(
        !allowed_for_session(unknown, false),
        "not knowing the session must not send the buffer"
    );

    for session in [local, in_ssh, unknown] {
        assert!(allowed_for_session(session, true));
    }
}
