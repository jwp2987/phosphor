use super::*;

// Ported from Warp's `environment_tests.rs` (`wsl_env_allowlist_*`), minus the
// `WARP_SSH_REUSE_CONTROL_MASTER` / `WARP_PROMPT_NODE_VERSION_ENABLED` entries
// — see the doc comment on `wsl_env_allowlist` for why those two aren't
// forwarded yet.

#[test]
fn wsl_env_allowlist_includes_client_version_without_notifications_flag() {
    let _guard = FeatureFlag::HOANotifications.override_enabled(false);

    let wslenv = wsl_env_allowlist(false).to_string_lossy().into_owned();

    assert_eq!(
        wslenv.split(':').collect::<Vec<_>>(),
        vec![
            format!("{HONOR_PS1_NAME}/u"),
            format!("{USE_SSH_WRAPPER_NAME}/u"),
            format!("{SHELL_DEBUG_MODE_NAME}/u"),
            format!("{TERM_PROGRAM_NAME}/u"),
            format!("{IS_LOCAL_SESSION_NAME}/u"),
            format!("{SSH_SOCKET_DIR}/u"),
            format!("{WARP_CLIENT_VERSION_ENV}/u"),
            format!("{TERMINAL_SESSION_UUID_ENV}/u"),
            format!("{FOCUS_URL_ENV}/u"),
        ],
    );
}

#[test]
fn wsl_env_allowlist_includes_cli_agent_protocol_when_notifications_flag_is_enabled() {
    let _guard = FeatureFlag::HOANotifications.override_enabled(true);

    let wslenv = wsl_env_allowlist(true).to_string_lossy().into_owned();

    assert_eq!(
        wslenv.split(':').collect::<Vec<_>>(),
        vec![
            format!("{HONOR_PS1_NAME}/u"),
            format!("{USE_SSH_WRAPPER_NAME}/u"),
            format!("{SHELL_DEBUG_MODE_NAME}/u"),
            format!("{TERM_PROGRAM_NAME}/u"),
            format!("{IS_LOCAL_SESSION_NAME}/u"),
            format!("{SSH_SOCKET_DIR}/u"),
            format!("{WARP_CLIENT_VERSION_ENV}/u"),
            format!("{TERMINAL_SESSION_UUID_ENV}/u"),
            format!("{FOCUS_URL_ENV}/u"),
            format!("{WARP_CLI_AGENT_PROTOCOL_VERSION_ENV}/u"),
            format!("{INITIAL_WORKING_DIR_NAME}/pu"),
        ],
    );
}
