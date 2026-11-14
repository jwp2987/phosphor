use super::*;

#[test]
fn enabled_support_uses_remote_server_for_legacy_ssh_when_feature_is_enabled() {
    assert!(SshRemoteServerSupport::Enabled.should_use_remote_server(true, true,));
}

#[test]
fn disabled_support_skips_remote_server_for_legacy_ssh() {
    assert!(!SshRemoteServerSupport::Disabled.should_use_remote_server(true, true,));
}

#[test]
fn enabled_support_skips_remote_server_when_feature_is_disabled() {
    assert!(!SshRemoteServerSupport::Enabled.should_use_remote_server(false, true,));
}

#[test]
fn enabled_support_skips_remote_server_for_non_legacy_ssh_session() {
    assert!(!SshRemoteServerSupport::Enabled.should_use_remote_server(true, false,));
}

/// #2155: the first `InitShell` of a helper-capable ssh session starts the helper setup; the
/// `InitShell` of the shell that replaced it (`exec zsh -l` in the remote profile, the session
/// still pending) does not start it a second time, and a session without the helper never does.
#[test]
fn a_reannounced_ssh_session_does_not_start_the_helper_setup_twice() {
    assert!(ssh_init_shell_goes_to_helper_setup(true, false));
    assert!(!ssh_init_shell_goes_to_helper_setup(true, true));
    assert!(!ssh_init_shell_goes_to_helper_setup(false, false));
    assert!(!ssh_init_shell_goes_to_helper_setup(false, true));
}
