use warpui::{assets::asset_cache::AssetSource, App};

use crate::{
    terminal::ssh::util::convert_script_to_one_line,
    test_util::settings::initialize_settings_for_tests, Assets,
};

use super::*;

fn initialize_app(app: &mut App) {
    initialize_settings_for_tests(app);
}

/// We write a couple extra bytes at the beginning of a script to clear the line, so we limit the
/// script to 1020 bytes here.
fn assert_script_is_short_enough_mac(script: &str, script_name: &str, convert_to_one_line: bool) {
    let script = if convert_to_one_line {
        convert_script_to_one_line(script)
    } else {
        script.to_string()
    };
    assert!(
        script.len() <= 1020,
        "{} script too long: {} bytes",
        script_name,
        script.len()
    );
}

fn get_script(asset_source: AssetSource, ctx: &AppContext) -> String {
    match AssetCache::as_ref(ctx).load_asset::<String>(asset_source) {
        AssetState::Loaded { data } => data.to_string(),
        _ => panic!("install tmux script should be available as a string"),
    }
}

#[cfg_attr(windows, ignore = "TODO(CORE-3626)")]
#[test]
/// See [assert_script_is_short_enough_mac] for more information.
fn test_mac_warpification_script_size() {
    App::test(Assets, |mut app| async move {
        initialize_app(&mut app);

        app.read(|ctx| {
            assert_script_is_short_enough_mac(
                // u64::MAX is the widest ID `generate_session_id` can produce, so this
                // measures the worst case for the macOS 1020-byte pty limit.
                &begin_warpify_ssh_session_command(SessionId::from(u64::MAX), ctx),
                "unknown_init_subshell.sh",
                false,
            );

            assert_script_is_short_enough_mac(
                &get_script(
                    bundled_asset!("ssh/bash_zsh/install_tmux_and_warpify_brew.sh"),
                    ctx,
                ),
                "install_tmux_and_warpify_brew.sh",
                false,
            );
            assert_script_is_short_enough_mac(
                &get_script(
                    bundled_asset!("ssh/fish/install_tmux_and_warpify_brew.sh"),
                    ctx,
                ),
                "fish/install_tmux_and_warpify_brew.sh",
                false,
            );

            // These three also carry a session ID now (#532), so they are measured at
            // `u64::MAX` for the same reason `unknown_init_subshell.sh` is above.
            assert_script_is_short_enough_mac(
                &warpify_ssh_session_command(
                    "Darwin",
                    ShellType::Zsh,
                    SessionId::from(u64::MAX),
                    ctx,
                )
                .expect("Should get Darwin zsh script"),
                "zsh warpify",
                true,
            );
            assert_script_is_short_enough_mac(
                &warpify_ssh_session_command(
                    "Darwin",
                    ShellType::Bash,
                    SessionId::from(u64::MAX),
                    ctx,
                )
                .expect("Should get Darwin bash script"),
                "bash warpify",
                true,
            );
            assert_script_is_short_enough_mac(
                &warpify_ssh_session_command(
                    "Darwin",
                    ShellType::Fish,
                    SessionId::from(u64::MAX),
                    ctx,
                )
                .expect("Should get Darwin fish script"),
                "fish warpify",
                true,
            )
        });
    });
}

/// Every warpify script must have its [`SESSION_ID_PLACEHOLDER`] substituted before it reaches
/// the pty. A leftover placeholder emits `"session_id": @@WARP_SESSION_ID@@`, which is not valid
/// JSON, so `SshTmuxInstaller` / `RemoteWarpificationIsUnavailable` would be dropped at
/// deserialization -- a failed warpification would hang with no error block, which is the exact
/// silent failure #532 exists to prevent.
#[cfg_attr(windows, ignore = "TODO(CORE-3626)")]
#[test]
fn test_warpify_scripts_substitute_session_id() {
    App::test(Assets, |mut app| async move {
        initialize_app(&mut app);

        app.read(|ctx| {
            for uname in ["Darwin", "Linux"] {
                for shell_type in [ShellType::Zsh, ShellType::Bash, ShellType::Fish] {
                    let name = shell_type.name();
                    let script =
                        warpify_ssh_session_command(uname, shell_type, SessionId::from(1234), ctx)
                            .expect("Should get a warpify script");
                    assert!(
                        !script.contains(SESSION_ID_PLACEHOLDER),
                        "{uname}/{name} warpify script still contains {SESSION_ID_PLACEHOLDER}"
                    );
                    assert!(
                        script.contains(r#"\"session_id\": 1234"#),
                        "{uname}/{name} warpify script does not quote the session ID"
                    );
                }
            }
        });
    });
}
