use std::collections::HashMap;

use crate::Builder;
use regex::Regex;
use settings::Setting as _;
use warp::{
    integration_testing::{
        step::new_step_with_default_assertions,
        subshell::{
            accept_tmux_install, assert_subshell_banner_is_showing,
            assert_subshell_is_bootstrapped, enter_ssh_command, enter_ssh_command_without_prompt,
            enter_ssh_password, run_exit_command, trigger_subshell_bootstrap,
            util::{
                FIXTURE_MOTD, legacy_ssh_wrapper_takes_ssh, machine_motd_marker, remote_hostname,
                remote_user, ssh_test_fixture_available, stop_ssh_fixtures,
            },
            wait_for_password_prompt,
        },
        terminal::{
            assert_active_block_output_for_single_terminal_in_tab,
            assert_long_running_block_executing_for_single_terminal_in_tab,
            execute_command_for_single_terminal_in_tab,
            util::{ExactLine, ExpectedExitStatus, current_shell_starter_and_version, nonce},
            validate_block_output, wait_until_bootstrapped_single_pane_for_tab,
        },
        view_getters::{single_terminal_view, single_terminal_view_for_tab},
    },
    terminal::{
        model::bootstrap::BootstrapStage,
        session_settings::{StartupShell, StartupShellOverride},
        shell::ShellType,
        warpify::settings::{SshExtensionInstallMode, SshExtensionInstallModeSetting},
    },
};
use warpui::{
    async_assert, async_assert_eq,
    integration::{AssertionCallback, AssertionOutcome, TestStep, needs, skip_because},
};

use super::new_builder;

/// What the scenarios that drive the legacy (ControlMaster) ssh wrapper need (#2128): that a
/// typed `ssh` runs that wrapper. Upstream's gate was `!FeatureFlag::SSHTmuxWrapper.is_enabled()`,
/// where the flag chose the tmux wrapper over the legacy one. In this fork the flag is on in
/// every default build (it also gates tmux control-mode handling) while the tmux ssh-wrapper
/// option is gone (`DECLINED.md`, removed 2026-11-08) and the legacy wrapper is what runs, so
/// under upstream's gate these scenarios skipped in every run. The gate now asks the fact
/// that decides it (`legacy_ssh_wrapper_takes_ssh`), which is true in every build of this fork.
const LEGACY_SSH_WRAPPER: &str = "a typed ssh to run the legacy (ControlMaster) ssh wrapper";

/// What the tmux-wrapper scenarios need. Nothing offers it: the option was removed on
/// 2026-11-08 (`DECLINED.md`), and the scenarios are not in the run list.
const TMUX_SSH_WRAPPER: &str = "the tmux ssh-wrapper option, which this fork removed \
    (DECLINED.md, 2026-11-08): this scenario cannot run";

/// A scenario that overrides the startup shell needs that shell on this machine: without it the
/// app falls back to the system default and the scenario would not test what its name says.
const LOCAL_ZSH: &str = "zsh installed (the startup shell this scenario sets)";

fn is_on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// PowerShell has no ssh wrapper (CORE-2333).
const POSIX_LOCAL_SHELL: &str = "a POSIX local shell (PowerShell has no ssh wrapper)";

/// The ssh scenarios answer the "install the remote helper?" question in advance (#2122): with
/// the default (`always_ask`) the app shows the question and holds the remote bootstrap until a
/// person answers, which no scenario here does, so the remote shell never bootstraps. No helper
/// is the primary remote design, and the real-app harness sets the same value for the same
/// reason (docs/testing.md, `ssh_extension_install`).
fn without_helper_question() -> HashMap<String, String> {
    HashMap::from([(
        SshExtensionInstallModeSetting::storage_key().to_owned(),
        serde_json::to_string(&SshExtensionInstallMode::NeverInstall)
            .expect("Can serialize setting as JSON"),
    )])
}

/// The prompt regex of fish on the local sshd fixture: `user@host ~>` with the short host name
/// (fish's `prompt_hostname`) and the fixture's remote home shown as `~`.
fn fish_prompt_regex() -> String {
    let hostname = remote_hostname();
    let short_host = hostname.split('.').next().unwrap_or_default();
    format!(
        r"\n{}@{} ~>$",
        regex::escape(&remote_user()),
        regex::escape(short_host)
    )
}

/// Verifies that the active block is part of a remote session.
fn assert_active_block_is_remote(user: String, host: String) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let terminal_view = single_terminal_view_for_tab(app, window_id, 0);
        terminal_view.read(app, |view, ctx| {
            let model = view.model.lock();
            let active_block = model.block_list().active_block();

            let Some(session_id) = active_block.session_id() else {
                return AssertionOutcome::PreconditionFailed(
                    "Active block returned None from shell_host()".into(),
                );
            };
            let Some(session) = view.sessions(ctx).get(session_id) else {
                return AssertionOutcome::PreconditionFailed(
                    "Active block should be part of a known session".into(),
                );
            };

            match async_assert!(
                !session.is_local(),
                "Active block should be part of a remote session"
            ) {
                AssertionOutcome::Success => {}
                failure => return failure,
            };

            let Some(shell_host) = active_block.shell_host() else {
                return AssertionOutcome::PreconditionFailed(
                    "Active block returned None from shell_host()".into(),
                );
            };

            match async_assert_eq!(
                shell_host.user,
                user.clone(),
                "Remote session did not have the expected user"
            ) {
                AssertionOutcome::Success => {}
                failure => return failure,
            };

            async_assert_eq!(
                shell_host.hostname,
                host.clone(),
                "Remote session did not have the expected host"
            )
        })
    })
}

/// Assertion that the MotD message is shown. How we expect it to be shown
/// depends on whether or not the remote shell can be bootstrapped.
///
/// WHICH MotD (#2128): the fixture's "remote" is the machine the scenario runs on, and a session
/// through the ssh wrapper carries a remote command, so the fixture's own banner
/// ([`FIXTURE_MOTD`], printed by its `ForceCommand` script only for a session without a
/// command) is not what appears. The wrapper's remote command (sh, ash) or the remote
/// bootstrap (bash, zsh) prints the machine's own MotD file, as sshd would have. The assertion
/// used to look for the text `Welcome to Ubuntu` whatever the machine: that held only where the
/// machine's MotD happens to start with those words. It now looks for the start of the first
/// plain line of this machine's MotD (on a stock Ubuntu that is the same line), or the
/// fixture's banner. On a machine with no MotD file nothing is printed and there is nothing to
/// look for: the assertion says so in the log and holds (before, it could not pass there).
fn assert_motd_shown(bootstrapped: bool) -> AssertionCallback {
    let machine_motd = machine_motd_marker();
    if machine_motd.is_none() {
        log::warn!(
            "ssh scenario: this machine has no MotD file with a plain line; the MotD \
             assertion has nothing to look for and holds"
        );
    }
    Box::new(move |app, window_id| {
        let Some(machine_motd) = machine_motd.as_deref() else {
            return AssertionOutcome::Success;
        };
        let terminal_view = single_terminal_view(app, window_id);
        terminal_view.read(app, |view, _| {
            let model = view.model.lock();

            let motd_output = if bootstrapped {
                let motd_block = model
                    .block_list()
                    .blocks()
                    .iter()
                    .rev()
                    .find(|block| block.bootstrap_stage() == BootstrapStage::ScriptExecution)
                    .expect("MotD block should exist");
                // Because of how we move blocks through their stages, output from
                // RC files and the MotD goes into the command grid.
                motd_block.command_to_string()
            } else {
                // For non-bootstrapped blocks, the MotD is part of the SSH
                // session output in the active block.
                model.block_list().active_block().output_to_string()
            };

            async_assert!(
                motd_output.contains(machine_motd) || motd_output.contains(FIXTURE_MOTD),
                "Expected output to hold this machine's MotD ({machine_motd:?}) or the \
                 fixture's banner, but was:\n{motd_output:?}"
            )
        })
    })
}

/// Verifies that the current session is using a login shell.
fn verify_login_shell(shell: &str) -> TestStep {
    let command = match shell {
        "zsh" => "[[ -o login ]]",
        "fish" => "status --is-login",
        // Other shells: started with `-l` since #2126 and read /etc/profile themselves (a shell
        // that refuses `-l` is not a login shell; the remote command sources /etc/profile then).
        _ => "test \"$WARP_PROFILE_LOADED\" = true",
    };

    match shell {
        "bash" | "zsh" => execute_command_for_single_terminal_in_tab(
            0,
            command.into(),
            ExpectedExitStatus::Success,
            (),
        )
        .add_assertion(assert_motd_shown(true /* bootstrapped */)),
        _ => {
            // For non-bootstrapped shells, run the command directly and verify
            // the exit status.
            let nonce = nonce();
            let expected_output = ExactLine::from(format!("{nonce}: 0"));

            TestStep::new("Verify login shell")
                .with_typed_characters(&[&format!("{command}; echo \"{nonce}\": $?")])
                .with_keystrokes(&["enter"])
                .add_assertion(assert_active_block_output_for_single_terminal_in_tab(
                    expected_output,
                    0,
                ))
                .add_assertion(assert_motd_shown(false /* bootstrapped */))
        }
    }
}

/// A macro to generate a test function to validate that we are able to
/// bootstrap a given remote shell when using ssh.
macro_rules! generate_can_bootstrap_legacy_ssh_test_for_shell {
    ($fn_name:ident, $shell:literal) => {
        /// Ensure we can successfully ssh into a $shell remote shell and bootstrap it
        /// successfully.
        pub fn $fn_name() -> Builder {
            new_builder()
                .with_cleanup(|_| stop_ssh_fixtures())
                .with_user_defaults(without_helper_question())
                // TODO(CORE-2333) PowerShell has no SSH wrapper.
                .set_should_run_test(|| {
                    if !legacy_ssh_wrapper_takes_ssh() {
                        return skip_because(LEGACY_SSH_WRAPPER);
                    }
                    let (starter, _) = current_shell_starter_and_version();
                    // Last: this starts the local sshd (see `ssh_test_fixture_available`).
                    needs(
                        starter.shell_type() != ShellType::PowerShell,
                        POSIX_LOCAL_SHELL,
                    ) && ssh_test_fixture_available($shell)
                })
                .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
                .with_step(enter_ssh_command($shell))
                .with_step(wait_for_password_prompt(0 /*tab_idx*/, $shell))
                .with_step(
                    enter_ssh_password().set_post_step_pause(std::time::Duration::from_millis(250)),
                )
                .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
                .with_step(
                    new_step_with_default_assertions(
                        "Assert active block is part of a remote session",
                    )
                    .add_assertion(assert_active_block_is_remote(
                        remote_user(),
                        remote_hostname(),
                    )),
                )
                .with_step(verify_login_shell($shell))
        }
    };
}

/// A macro to generate a test function to validate that we are able to
/// bootstrap a given remote shell when using ssh.
macro_rules! generate_can_bootstrap_tmux_ssh_test_for_shell {
    ($fn_name:ident, $shell:literal, $install_tmux:literal) => {
        /// Ensure we can successfully ssh into a $shell remote shell and bootstrap it
        /// successfully.
        pub fn $fn_name() -> Builder {
            fn warpify(builder: Builder) -> Builder {
                builder
                    .with_step(enter_ssh_command($shell))
                    .with_step(wait_for_password_prompt(0 /*tab_idx*/, $shell))
                    .with_step(
                        enter_ssh_password()
                            .set_post_step_pause(std::time::Duration::from_millis(250)),
                    )
                    .with_step(assert_subshell_banner_is_showing())
                    .with_step(trigger_subshell_bootstrap())
            }

            fn assert_warpification(builder: Builder) -> Builder {
                builder
                    .with_step(assert_subshell_is_bootstrapped(0, 0))
                    .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
                    .with_step(
                        new_step_with_default_assertions(
                            "Assert active block is part of a remote session",
                        )
                        .add_assertion(assert_active_block_is_remote(
                            remote_user(),
                            remote_hostname(),
                        )),
                    )
                    .with_step(verify_login_shell($shell))
            }

            let builder = new_builder()
                .with_cleanup(|_| stop_ssh_fixtures())
                // TODO(CORE-2333) PowerShell has no SSH wrapper.
                .set_should_run_test(|| {
                    if legacy_ssh_wrapper_takes_ssh() {
                        return skip_because(TMUX_SSH_WRAPPER);
                    }
                    let (starter, _) = current_shell_starter_and_version();
                    // Last: this starts the local sshd (see `ssh_test_fixture_available`).
                    needs(
                        starter.shell_type() != ShellType::PowerShell,
                        POSIX_LOCAL_SHELL,
                    ) && ssh_test_fixture_available($shell)
                })
                .with_step(wait_until_bootstrapped_single_pane_for_tab(0));
            // Install Tmux
            let builder = warpify(builder).with_step(
                accept_tmux_install().set_post_step_pause(std::time::Duration::from_secs(3)),
            );
            // Quit SSH Session once we validate warpificaiton works with Tmux Install
            let builder = assert_warpification(builder).with_step(run_exit_command());

            // Validate we can Warpify when Tmux is already installed
            assert_warpification(warpify(builder))
        }
    };
}

/// A macro to generate a test function to validate that we are able to
/// successfully start (but not bootstrap) an ssh connection with the given
/// remote shell.  Verifies that the ssh connection worked by asserting that
/// there is still a long-running block after entering the password, and that
/// attempting to run `exit` returns us to the bootstrapped local shell.
macro_rules! generate_long_running_block_ssh_test_for_shell {
    ($fn_name:ident, $shell:literal, prompt_regex: $prompt_regex:expr) => {
        /// Ensure we can successfully ssh into a $shell remote shell and bootstrap it
        /// successfully.
        pub fn $fn_name() -> Builder {
            new_builder()
                .with_cleanup(|_| stop_ssh_fixtures())
                .with_user_defaults(without_helper_question())
                // TODO(CORE-2333) PowerShell has no SSH wrapper.
                .set_should_run_test(|| {
                    let (starter, _) = current_shell_starter_and_version();
                    // Last: this starts the local sshd (see `ssh_test_fixture_available`).
                    needs(
                        starter.shell_type() != ShellType::PowerShell,
                        POSIX_LOCAL_SHELL,
                    ) && ssh_test_fixture_available($shell)
                })
                .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
                .with_step(enter_ssh_command($shell))
                .with_step(wait_for_password_prompt(0 /*tab_idx*/, $shell))
                .with_step(enter_ssh_password())
                .with_step(
                    TestStep::new("Assert prompt is awaiting input")
                        .add_assertion(
                            assert_long_running_block_executing_for_single_terminal_in_tab(true, 0),
                        )
                        .add_assertion(move |app, window_id| {
                            let regex = Regex::new(&String::from($prompt_regex))
                                .expect("regex should not fail to compile");
                            validate_block_output(&regex, 0, 0, window_id, app)
                        }),
                )
                .with_step(verify_login_shell($shell))
                .with_step(TestStep::new("Exit ssh session").with_typed_characters(&["exit\n"]))
                .with_step(new_step_with_default_assertions(
                    "Assert ssh session has completed",
                ))
        }
    };
}

// Generate test methods to validate expected ssh behavior for a variety of
// remote shells.
generate_can_bootstrap_legacy_ssh_test_for_shell!(test_legacy_ssh_into_bash, "bash");
generate_can_bootstrap_legacy_ssh_test_for_shell!(test_legacy_ssh_into_zsh, "zsh");
generate_can_bootstrap_tmux_ssh_test_for_shell!(test_tmux_ssh_into_bash, "bash", false);
generate_can_bootstrap_tmux_ssh_test_for_shell!(test_tmux_ssh_into_zsh, "zsh", false);
generate_long_running_block_ssh_test_for_shell!(test_ssh_into_fish, "fish", prompt_regex: fish_prompt_regex());
generate_long_running_block_ssh_test_for_shell!(test_ssh_into_sh, "sh", prompt_regex: r"\n\$ $");
generate_long_running_block_ssh_test_for_shell!(test_ssh_into_ash, "ash", prompt_regex: r"\n\$ $");

/// Tests a regression with the startup shell setting and SSH proxies.
/// See WAR-6337 for details - if `$SHELL` is not set to a valid executable file
/// path, SSH fails to execute proxy commands. (The scenario now connects to the
/// local sshd fixture, which has no ProxyCommand; it still guards the override
/// path of the ssh wrapper.)
///
/// #2133: the override is the LEGACY key with a shell name, and until that issue the app
/// ignored it for the first tab (the shell scan had not finished), so this scenario only ever
/// drove the system default shell: round 14 started `/bin/bash` every time. It now says that
/// it needs zsh, and its second step proves the pane's shell IS zsh before any ssh is typed,
/// so it can no longer pass (or fail) on a shell it did not ask for.
pub fn test_ssh_with_shell_override() -> Builder {
    new_builder()
        .with_cleanup(|_| stop_ssh_fixtures())
        // TODO(CORE-2333) PowerShell has no SSH wrapper.
        .set_should_run_test(|| {
            let (starter, _) = current_shell_starter_and_version();
            // Last: this starts the local sshd (see `ssh_test_fixture_available`).
            needs(
                starter.shell_type() != ShellType::PowerShell,
                POSIX_LOCAL_SHELL,
            ) && needs(is_on_path("zsh"), LOCAL_ZSH)
                && ssh_test_fixture_available("bash")
        })
        .with_user_defaults(HashMap::from([(
            StartupShellOverride::storage_key().to_owned(),
            serde_json::to_string(&StartupShell::Zsh).expect("Can serialize setting as JSON"),
        )]))
        .with_user_defaults(without_helper_question())
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        // The override was applied: the local shell is zsh (`ZSH_VERSION` is set by zsh only).
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            "test -n \"$ZSH_VERSION\"".into(),
            ExpectedExitStatus::Success,
            (),
        ))
        .with_step(enter_ssh_command("bash"))
        .with_step(wait_for_password_prompt(0, "bash"))
        .with_step(enter_ssh_password())
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(
            new_step_with_default_assertions("Assert active block is part of a remote session")
                .add_assertion(assert_active_block_is_remote(
                    remote_user(),
                    remote_hostname(),
                )),
        )
        .with_step(verify_login_shell("bash"))
}

/// The remote bash shell bootstraps over ssh: the `Bootstrapped` hook of the REMOTE shell arrives
/// (the same wait the legacy scenarios use), the session is remote for the right user and host,
/// and the next command really runs on the remote side (`PHOSPHOR_TEST_SSHD=remote` is exported by
/// the fixture's login script and nowhere else). Runs the legacy ssh wrapper (the one this fork
/// ships, #2128); the connect uses the key without a passphrase.
pub fn test_ssh_remote_bash_bootstrapped_hook() -> Builder {
    new_builder()
        .with_cleanup(|_| stop_ssh_fixtures())
        .with_user_defaults(without_helper_question())
        .set_should_run_test(|| {
            let (starter, _) = current_shell_starter_and_version();
            // Last: this starts the local sshd (see `ssh_test_fixture_available`).
            needs(
                starter.shell_type() != ShellType::PowerShell,
                POSIX_LOCAL_SHELL,
            ) && needs(legacy_ssh_wrapper_takes_ssh(), LEGACY_SSH_WRAPPER)
                && ssh_test_fixture_available("bash")
        })
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(enter_ssh_command_without_prompt("bash"))
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(
            new_step_with_default_assertions("Assert active block is part of a remote session")
                .add_assertion(assert_active_block_is_remote(
                    remote_user(),
                    remote_hostname(),
                )),
        )
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            "echo \"$PHOSPHOR_TEST_SSHD\"".into(),
            ExpectedExitStatus::Success,
            ExactLine::from("remote"),
        ))
}

/// Counts, over every block, how often the typeahead's output appeared: it must be there exactly
/// once with the REMOTE marker and never with the LOCAL one. The typed text itself contains
/// neither token (it spells the marker as a `${PHOSPHOR_TEST_SSHD:-local}` expansion), so only
/// the output of a command that ran counts.
fn assert_typeahead_ran_once_remotely(nonce: String) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let terminal_view = single_terminal_view(app, window_id);
        terminal_view.read(app, |view, _| {
            let model = view.model.lock();
            let remote_token = format!("TA-{nonce}-remote");
            let local_token = format!("TA-{nonce}-local");
            let mut remote = 0usize;
            let mut local = 0usize;
            for block in model.block_list().blocks().iter() {
                let text = format!(
                    "{}\n{}",
                    block.command_to_string(),
                    block.output_to_string()
                );
                remote += text.matches(remote_token.as_str()).count();
                local += text.matches(local_token.as_str()).count();
            }
            if local > 0 {
                return AssertionOutcome::failure(format!(
                    "Typeahead typed during the ssh connect ran LOCALLY ({local} time(s))"
                ));
            }
            async_assert_eq!(
                remote,
                1,
                "Typeahead typed during the ssh connect should run exactly once on the remote shell"
            )
        })
    })
}

/// A command typed while ssh is still connecting and the remote shell is still bootstrapping is
/// delivered to the REMOTE shell, exactly once, and never runs in the local shell (the worst
/// wrong-destination bug: a command meant for the remote host running locally).
pub fn test_ssh_typeahead_during_connect_runs_remotely_once() -> Builder {
    let nonce = nonce();
    let typed = format!("echo TA-{nonce}-${{PHOSPHOR_TEST_SSHD:-local}}");
    let sentinel = format!("echo done-{nonce}");
    new_builder()
        .with_cleanup(|_| stop_ssh_fixtures())
        .with_user_defaults(without_helper_question())
        .set_should_run_test(|| {
            let (starter, _) = current_shell_starter_and_version();
            // Last: this starts the local sshd (see `ssh_test_fixture_available`).
            needs(
                starter.shell_type() != ShellType::PowerShell,
                POSIX_LOCAL_SHELL,
            ) && needs(legacy_ssh_wrapper_takes_ssh(), LEGACY_SSH_WRAPPER)
                && ssh_test_fixture_available("bash")
        })
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(enter_ssh_command_without_prompt("bash"))
        // No pause between the two steps: this is typed during the connect / remote bootstrap.
        .with_step(
            TestStep::new("Type a command during the ssh connect")
                .with_typed_characters(&[&typed])
                .with_keystrokes(&["enter"]),
        )
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(execute_command_for_single_terminal_in_tab(
            0,
            sentinel.clone(),
            ExpectedExitStatus::Success,
            ExactLine::from(format!("done-{nonce}")),
        ))
        .with_step(
            new_step_with_default_assertions("Typeahead ran once, on the remote shell")
                .add_assertion(assert_typeahead_ran_once_remotely(nonce.clone())),
        )
}
