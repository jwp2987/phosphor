use std::sync::Arc;

use crate::{
    context_chips::context_chip::GeneratorContext,
    terminal::model::{
        block::BlockMetadata,
        session::{
            command_executor::testing::TestCommandExecutor, BootstrapSessionType, Session,
            SessionInfo,
        },
    },
};

#[test]
fn test_working_directory() {
    let session = Session::test();
    // SessionInfo forces the home directory in tests.
    let home_dir = session.home_dir().expect("Home dir is set in tests");

    let block_in_cwd = BlockMetadata::new(Some(session.id()), Some(format!("{home_dir}/projects")));

    assert_eq!(
        super::working_directory(&GeneratorContext {
            active_block_metadata: &block_in_cwd,
            active_session: Some(&session),
            current_environment: &Default::default(),
        })
        .as_ref()
        .and_then(|v| v.as_text()),
        Some("~/projects")
    );

    let block_outside_cwd = BlockMetadata::new(Some(session.id()), Some("/etc".to_string()));

    assert_eq!(
        super::working_directory(&GeneratorContext {
            active_block_metadata: &block_outside_cwd,
            active_session: Some(&session),
            current_environment: &Default::default(),
        })
        .as_ref()
        .and_then(|v| v.as_text()),
        Some("/etc")
    );
}

#[test]
fn test_remote_sessions() {
    let local_session = Session::test();
    let remote_session = Session::new(
        SessionInfo::new_for_test()
            .with_session_type(BootstrapSessionType::WarpifiedRemote)
            .with_hostname("remote-host".to_string())
            .with_user("remote-user".to_string()),
        Arc::new(TestCommandExecutor {}),
    );

    let local_ctx = GeneratorContext {
        active_block_metadata: &BlockMetadata::new(Some(local_session.id()), None),
        active_session: Some(&local_session),
        current_environment: &Default::default(),
    };

    let remote_ctx = GeneratorContext {
        active_block_metadata: &BlockMetadata::new(Some(remote_session.id()), None),
        active_session: Some(&remote_session),
        current_environment: &Default::default(),
    };

    // The Username and Hostname chips are always present.
    assert_eq!(
        super::username(&local_ctx)
            .as_ref()
            .and_then(|v| v.as_text()),
        Some("local:user")
    );
    assert_eq!(
        super::username(&remote_ctx)
            .as_ref()
            .and_then(|v| v.as_text()),
        Some("remote-user")
    );
    assert_eq!(
        super::hostname(&local_ctx)
            .as_ref()
            .and_then(|v| v.as_text()),
        Some("local:host")
    );
    assert_eq!(
        super::hostname(&remote_ctx)
            .as_ref()
            .and_then(|v| v.as_text()),
        Some("remote-host")
    );

    // The SSH chip is only shown for remote sessions.
    assert_eq!(super::ssh_session(&local_ctx), None);
    assert_eq!(
        super::ssh_session(&remote_ctx)
            .as_ref()
            .and_then(|v| v.as_text()),
        Some("remote-user@remote-host")
    );
}

#[test]
fn test_node_version() {
    use crate::context_chips::context_chip::Environment;
    use crate::terminal::model::block::BlockMetadata;
    use crate::terminal::model::session::Session;

    let session = Session::test();
    let block_metadata = BlockMetadata::new(Some(session.id()), None);

    // Test with no node version
    let environment_no_node = Environment::default();
    let ctx_no_node = GeneratorContext {
        active_block_metadata: &block_metadata,
        active_session: Some(&session),
        current_environment: &environment_no_node,
    };
    assert_eq!(super::node_version(&ctx_no_node), None);

    // Test with node version - create environment with node version
    let environment_with_node = Environment::new(
        None,                        // virtual_env
        None,                        // conda_env
        Some("v18.0.0".to_string()), // node_version
    );
    let ctx_with_node = GeneratorContext {
        active_block_metadata: &block_metadata,
        active_session: Some(&session),
        current_environment: &environment_with_node,
    };
    assert_eq!(
        super::node_version(&ctx_with_node)
            .as_ref()
            .and_then(|v| v.as_text()),
        Some("v18.0.0")
    );
}

/// Chip commands are run for Phosphor by the session's command executors, which take POSIX
/// syntax for a nushell session; a chip with no nushell entry falls back to its bash command,
/// and one that really lacks bash support is still unsupported.
#[test]
fn test_shell_specific_chip_commands_fall_back_to_bash_for_nu() {
    use crate::context_chips::context_chip::ShellCommand;
    use crate::terminal::shell::ShellType;

    let command = ShellCommand::shell_specific([
        (ShellType::Bash, "bash-version".to_string()),
        (ShellType::Fish, "fish-version".to_string()),
    ]);
    assert_eq!(command.for_shell(ShellType::Nu), Some("bash-version"));
    assert_eq!(command.for_shell(ShellType::Fish), Some("fish-version"));
    assert_eq!(command.for_shell(ShellType::PowerShell), None);

    // An explicit nushell entry wins.
    let command = ShellCommand::shell_specific([
        (ShellType::Bash, "bash-version".to_string()),
        (ShellType::Nu, "nu-version".to_string()),
    ]);
    assert_eq!(command.for_shell(ShellType::Nu), Some("nu-version"));

    let git = super::shell_git_branch();
    assert!(git.command().for_shell(ShellType::Nu).is_some());
}

#[test]
fn direct_powershell_invocation_recognises_the_per_prompt_chip_commands() {
    use crate::terminal::shell::ShellType;

    // The chips' own PowerShell text maps to the program they wrap (#1230).
    let line_changes = super::shell_git_line_changes();
    let line_changes_command = line_changes
        .command()
        .for_shell(ShellType::PowerShell)
        .expect("the git line-changes chip supports PowerShell");
    let direct = super::direct_powershell_invocation(line_changes_command)
        .expect("the git diff-stat chip command is a single program");
    assert_eq!(direct.program, "git");
    assert_eq!(
        direct.args,
        [
            "-c",
            "diff.autoRefreshIndex=false",
            "diff",
            "--shortstat",
            "HEAD"
        ]
    );
    assert_eq!(direct.env, [("GIT_OPTIONAL_LOCKS", "0")]);

    let kubernetes = super::kubernetes_current_context();
    let kubernetes_command = kubernetes
        .command()
        .for_shell(ShellType::PowerShell)
        .expect("the kubernetes chip supports PowerShell");
    let direct = super::direct_powershell_invocation(kubernetes_command)
        .expect("the kubectl chip command is a single program");
    assert_eq!(direct.program, "kubectl");
    assert_eq!(direct.args, ["config", "current-context"]);
    assert!(direct.env.is_empty());
}

#[test]
fn the_powershell_diff_stat_chip_text_is_unchanged() {
    use crate::terminal::shell::ShellType;

    // The direct-run recogniser compares against this text, so it must not drift from what
    // `safe_git_powershell` always produced for the chip.
    let generator = super::shell_git_line_changes();
    let command = generator
        .command()
        .for_shell(ShellType::PowerShell)
        .expect("PowerShell supported");
    assert_eq!(
        command,
        super::safe_git_powershell("git -c diff.autoRefreshIndex=false diff --shortstat HEAD")
    );
}

#[test]
fn direct_powershell_invocation_leaves_every_other_command_to_the_shell() {
    for command in [
        "git status",
        "kubectl config current-context; rm -rf x",
        "kubectl config get-contexts",
        // Anything that only looks like the chip command: wrapper missing, extra statements.
        "git -c diff.autoRefreshIndex=false diff --shortstat HEAD",
        "",
    ] {
        assert!(
            super::direct_powershell_invocation(command).is_none(),
            "{command:?} must still run in PowerShell"
        );
    }
    // The other git chips have logic around the git calls and stay in PowerShell.
    let other = super::shell_other_git_branches();
    let text = other
        .command()
        .for_shell(crate::terminal::shell::ShellType::PowerShell)
        .unwrap();
    assert!(super::direct_powershell_invocation(text).is_none());
}
