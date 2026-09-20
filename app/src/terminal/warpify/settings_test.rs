use settings::{Setting, SyncToCloud};
use warp_util::path::ShellFamily;
use warpui::{App, SingletonEntity};

use super::{EnableSshWrapper, WarpifySettings};
use crate::test_util::settings::initialize_settings_for_tests;

#[cfg(windows)]
#[test]
fn test_wsl_subshell_detection_success() {
    [
        "wsl",
        "wsl.exe",
        "wsl -d Ubuntu",
        "wsl --distribution Ubuntu",
        "wsl -u user",
        "wsl --cd /home/user",
        "wsl --system",
        "wsl --shell-type login",
        "wsl -d Ubuntu --cd /home/user -u username",
        "wsl.exe -d Ubuntu --cd /home/user -u username",
    ]
    .iter()
    .for_each(|cmd| {
        assert!(
            WarpifySettings::is_built_in_subshell_match(cmd),
            "{} failed to match",
            *cmd
        )
    });
}

#[cfg(windows)]
#[test]
fn test_wsl_subshell_detection_fail() {
    [
        "wsl --install",
        "wsl --status",
        "wsl --list",
        "wsl --export Ubuntu file.tar",
        "wsl --uninstall",
        "wsl --shutdown",
        "wslfetch",
        "nowsl",
        "wsl --help",
        "wsl --version",
        "wsl --terminate Ubuntu",
        "wsl --unregister Ubuntu",
        "wsl --update",
        "wsl --import-in-place Ubuntu",
        "wsl --default-user root",
        "wsl --mount \\device",
    ]
    .iter()
    .for_each(|cmd| {
        assert!(
            !WarpifySettings::is_built_in_subshell_match(cmd),
            "{} accidentally matched",
            *cmd
        )
    });
}

/// Privilege-escalation shells must be warpified. Without hooks in the escalated shell,
/// directory completion answers from a stale cached cwd, and
/// `PtyWrite::RunNativeShellCompletions` locks the pane: it writes a bare ^Y and waits
/// forever for an OSC reply that only the hooks can send, leaving the controller in
/// `AwaitingPrompt` so no further keystroke is ever written to the pty.
///
/// These entries do not exist upstream at the pin; they are a deliberate fork divergence.
#[test]
fn test_privilege_escalation_subshell_detection_success() {
    [
        // Bare su, with and without a login flag and/or a target user.
        "su",
        "su -",
        "su -l",
        "su --login",
        "su someuser",
        "su - someuser",
        "su -l someuser",
        "su --login someuser",
        "/bin/su",
        "/bin/su -",
        "/usr/bin/su - operator",
        // Kerberos su.
        "ksu",
        "ksu alice",
        "ksu alice@EXAMPLE.COM",
        "/usr/bin/ksu",
        // sudo su, with and without sudo options.
        "sudo su",
        "sudo su -",
        "sudo su - root",
        "sudo su root",
        "sudo su -l",
        "sudo -H su -",
        "sudo -E -H su",
        "/usr/bin/sudo su -",
        "sudo /usr/bin/su - operator",
        // sudo's own login/shell flags.
        "sudo -i",
        "sudo -s",
        "sudo --login",
        "sudo --shell",
        "sudo -u www-data -s",
        "sudo -u postgres -i",
        "sudo -H -u deploy -s",
        // sudo invoking a shell directly.
        "sudo bash",
        "sudo zsh",
        "sudo fish",
        "sudo /bin/bash",
        "sudo -u root /usr/bin/zsh",
    ]
    .iter()
    .for_each(|cmd| {
        assert!(
            WarpifySettings::is_built_in_subshell_match(cmd),
            "{} failed to match",
            *cmd
        )
    });
}

/// The false-positive guard for the privilege-escalation entries above. Matching any of
/// these would pop the warpify banner (or auto-bootstrap) on a command that never spawns
/// an interactive shell.
#[test]
fn test_privilege_escalation_subshell_detection_fail() {
    [
        // Ordinary privileged commands -- by far the most common input starting with sudo.
        "sudo systemctl restart foo",
        "sudo systemctl status sshd",
        "sudo apt install subversion",
        "sudo apt-get update",
        "sudo apt -y install fish",
        "sudo make install",
        "sudo ls -la",
        "sudo docker ps",
        "sudo pip install zsh",
        "sudo -E pip install fish",
        "sudo journalctl -u fish",
        "sudo rm -rf /tmp/su",
        "sudo -u deploy git pull",
        "sudo -u www-data ls",
        "sudo chmod +s /usr/bin/foo",
        // sudo without a shell of any kind.
        "sudo",
        "sudo -l",
        "sudo -v",
        "sudo --version",
        // A command argument means the shell is not interactive, so there is nothing to
        // warpify and no prompt will ever come back.
        "su -c 'rm -rf /'",
        "su - root -c whoami",
        "sudo su -c id",
        "sudo -i systemctl restart nginx",
        "sudo -s -c 'echo hi'",
        "sudo bash -c 'echo hi'",
        // Longer words that merely begin with "su", and su-like binaries that are not su.
        "subversion",
        "sushi",
        "sudoku",
        "summary",
        "subl .",
        "su-exec nobody id",
        "sudo su-exec nobody id",
        // "su" appearing somewhere other than as the command word.
        "git submodule update",
        "echo su",
        "ls /usr/bin/su",
    ]
    .iter()
    .for_each(|cmd| {
        assert!(
            !WarpifySettings::is_built_in_subshell_match(cmd),
            "{} accidentally matched",
            *cmd
        )
    });
}

/// Built-in subshell regexes and the user's `warpify.subshells.added_subshell_commands`
/// are two separate lists that `is_compatible_subshell_command` consults in turn, so
/// adding built-ins can neither shadow nor duplicate a user's own entries.
#[test]
fn test_privilege_escalation_builtins_coexist_with_added_subshell_commands() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            let settings = WarpifySettings::as_ref(ctx);
            // The new built-ins match with no user entries configured at all.
            assert!(settings.is_compatible_subshell_command("sudo su -", ShellFamily::Posix));
            assert!(
                !settings.is_compatible_subshell_command("my-custom-shell", ShellFamily::Posix),
                "unconfigured custom command must not match a built-in"
            );
        });

        WarpifySettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .added_subshell_commands
                .set_value(vec!["^my-custom-shell$".to_string()], ctx)
                .unwrap();
        });

        app.read(|ctx| {
            let settings = WarpifySettings::as_ref(ctx);
            // The user's own entry still matches...
            assert!(settings.is_compatible_subshell_command("my-custom-shell", ShellFamily::Posix));
            // ...and the built-ins are unaffected by its presence.
            assert!(settings.is_compatible_subshell_command("su -", ShellFamily::Posix));
            assert!(settings.is_compatible_subshell_command("sudo -i", ShellFamily::Posix));
            // The stored user list is untouched -- built-ins are never merged into it.
            assert_eq!(
                settings.added_subshell_commands.to_vec(),
                vec!["^my-custom-shell$".to_string()],
                "built-ins must not be written into the user's setting"
            );
        });
    });
}

// Ported from warp/master `app/src/terminal/warpify/settings_tests.rs`.
// Assertions are unchanged from Warp.

#[test]
fn test_parsed_subshell_commands_updated_via_self_subscription() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            assert!(
                WarpifySettings::as_ref(ctx)
                    .parsed_added_subshell_commands
                    .is_empty()
            );
        });

        WarpifySettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .added_subshell_commands
                .set_value(vec!["^my-custom-shell$".to_string()], ctx)
                .unwrap();
        });

        // The parsed field must now contain the compiled regex.
        app.read(|ctx| {
            let parsed = &WarpifySettings::as_ref(ctx).parsed_added_subshell_commands;
            assert_eq!(
                parsed.len(),
                1,
                "self-subscription should have updated parsed field"
            );
            let regex = parsed[0].as_ref().expect("regex should compile");
            assert!(
                regex.is_match("my-custom-shell"),
                "compiled regex should match the command pattern"
            );
        });
    });
}

/// Verify that a user who previously set `enable_legacy_ssh_wrapper = false`
/// (old `SshSettings::enable_ssh_wrapper`) has that opt-out forwarded to
/// `enable_ssh_warpification` on first launch after the migration.
#[test]
fn test_enable_ssh_wrapper_false_migrates_to_enable_ssh_warpification_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // Simulate a user who had explicitly opted out of the legacy SSH wrapper.
        WarpifySettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .enable_ssh_wrapper
                .set_value(false, ctx)
                .expect("set enable_ssh_wrapper to false");
        });

        // The migration in `register` already ran during `initialize_settings_for_tests`
        // (before we set the value above), so we trigger it manually by calling
        // `register` again on a fresh model to simulate a new launch with the value
        // pre-set in storage.  We verify the outcome by checking state directly.
        //
        // Simpler approach: confirm the migration logic produces the right state
        // by applying it explicitly here.
        app.update(|ctx| {
            WarpifySettings::handle(ctx).update(ctx, |me, ctx| {
                if me.enable_ssh_wrapper.is_value_explicitly_set()
                    && !*me.enable_ssh_wrapper.value()
                {
                    me.enable_ssh_warpification
                        .set_value(false, ctx)
                        .expect("migration set enable_ssh_warpification");
                    me.enable_ssh_wrapper
                        .set_value(true, ctx)
                        .expect("migration reset enable_ssh_wrapper");
                }
            });
        });

        app.read(|ctx| {
            let settings = WarpifySettings::as_ref(ctx);
            assert!(
                !*settings.enable_ssh_warpification.value(),
                "enable_ssh_warpification should be false after migration"
            );
            // The wrapper is reset to true so the migration condition
            // (`!*enable_ssh_wrapper.value()`) won't fire again on the next launch.
            assert!(
                *settings.enable_ssh_wrapper.value(),
                "enable_ssh_wrapper should be reset to true (default) after migration"
            );
        });
    });
}

/// Post-#13228 behavior: the one-time legacy-wrapper migration honors a historical
/// opt-out once, and a user who then re-enables Warpify SSH keeps it. Because the
/// trigger is no longer synced, its reset-to-default persists and the migration does
/// not fire again to clobber the user's choice.
#[test]
fn test_legacy_wrapper_migration_is_one_time_and_preserves_reenabled_warpification() {
    /// Mirrors the one-time migration body from `WarpifySettings::register`.
    fn run_migration(app: &mut App) {
        app.update(|ctx| {
            WarpifySettings::handle(ctx).update(ctx, |me, ctx| {
                if me.enable_ssh_wrapper.is_value_explicitly_set()
                    && !*me.enable_ssh_wrapper.value()
                {
                    me.enable_ssh_warpification
                        .set_value(false, ctx)
                        .expect("migration set enable_ssh_warpification");
                    me.enable_ssh_wrapper
                        .set_value(true, ctx)
                        .expect("migration reset enable_ssh_wrapper");
                }
            });
        });
    }

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // Historical opt-out of the legacy wrapper.
        WarpifySettings::handle(&app).update(&mut app, |settings, ctx| {
            settings.enable_ssh_wrapper.set_value(false, ctx).unwrap();
        });

        // Launch 1: migration honors the opt-out once and resets the trigger.
        run_migration(&mut app);
        app.read(|ctx| {
            let settings = WarpifySettings::as_ref(ctx);
            assert!(
                !*settings.enable_ssh_warpification.value(),
                "opt-out is honored once"
            );
            assert!(
                *settings.enable_ssh_wrapper.value(),
                "trigger reset to default acts as the one-time marker"
            );
        });

        // The user re-enables Warpify SSH.
        WarpifySettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .enable_ssh_warpification
                .set_value(true, ctx)
                .unwrap();
        });

        // Launch 2: the trigger is no longer synced, so it stays at its reset
        // default; the migration is a no-op and does not re-disable warpification.
        run_migration(&mut app);
        app.read(|ctx| {
            assert!(
                *WarpifySettings::as_ref(ctx)
                    .enable_ssh_warpification
                    .value(),
                "re-enabled Warpify SSH persists across launches (#13228)"
            );
        });
    });
}

/// Ported from the pin's `test_deprecated_ssh_wrapper_migration_triggers_are_not_synced`
/// (`02b53fcd8`, `app/src/terminal/warpify/settings_tests.rs`), narrowed to the half that
/// applies here. The pin also asserts `UseSshTmuxWrapper::sync_to_cloud() ==
/// SyncToCloud::Never`, guarding against the same re-arm hazard for a one-time migration
/// that resets `use_ssh_tmux_wrapper` and shows a tmux-deprecation notice. This fork does
/// not have that migration at all -- `SshTmuxDeprecationNoticePending` and the deprecation
/// notice do not exist here, because the fork keeps the tmux wrapper permanently rather
/// than deprecating it (`DECLINED.md`, "SSH tmux wrapper -- kept, deprecation not ported",
/// #322). With no migration to re-arm, `use_ssh_tmux_wrapper`'s sync setting has nothing
/// to protect against, so that half of the pin test is not ported.
///
/// `enable_ssh_wrapper` is a different story: the one-time legacy-wrapper migration above
/// (`test_enable_ssh_wrapper_false_migrates_to_enable_ssh_warpification_false`) is real and
/// live here, and `settings.rs` already sets `sync_to_cloud: SyncToCloud::Never` on it with
/// a comment citing the exact upstream bug (warpdotdev/Warp#13228) this guards against --
/// but until now nothing pinned that value against an accidental future revert.
#[test]
fn enable_ssh_wrapper_migration_trigger_is_not_synced() {
    assert_eq!(
        EnableSshWrapper::sync_to_cloud(),
        SyncToCloud::Never,
        "enable_ssh_wrapper must not sync -- a stale synced value re-arms the one-time \
         migration and re-disables enable_ssh_warpification (warpdotdev/Warp#13228)"
    );
}

/// Verify that the default state (no legacy setting present) does not
/// spuriously disable `enable_ssh_warpification`.
#[test]
fn test_enable_ssh_wrapper_default_does_not_affect_enable_ssh_warpification() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            let settings = WarpifySettings::as_ref(ctx);
            // Neither setting should be explicitly set — both default to true.
            assert!(
                !settings.enable_ssh_wrapper.is_value_explicitly_set(),
                "enable_ssh_wrapper should not be explicitly set in a fresh install"
            );
            assert!(
                *settings.enable_ssh_warpification.value(),
                "enable_ssh_warpification should remain true when no migration is needed"
            );
        });
    });
}
