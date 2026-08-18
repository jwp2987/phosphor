use super::*;

// Sentinel value mirrored from `build_host_shell_command` / `build_docker_sandbox_command`,
// which set HISTFILESIZE/HISTSIZE to an unusually large literal so that Zap can detect (via the
// WARP_INITIAL_* env vars) whether the shell's startup files clobbered the history size.
const BASH_HISTORY_SIZE_SENTINEL: &str = "57265949261";

fn shell_starter(shell_type: ShellType, shell_path: &str) -> DirectShellStarter {
    DirectShellStarter::new_for_test(shell_type, PathBuf::from(shell_path), Vec::new())
}

fn env_value(command: &Command, key: &str) -> Option<Option<String>> {
    command
        .get_envs()
        .find(|(env_key, _)| *env_key == std::ffi::OsStr::new(key))
        .map(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()))
}

#[test]
fn host_bash_command_sets_history_size_sentinels() {
    let command = build_host_shell_command(
        shell_starter(ShellType::Bash, "/bin/bash"),
        None,
        HashMap::new(),
        None,
        false,
        false,
        false,
        false,
        false,
    );

    assert_eq!(
        env_value(&command, "HISTFILESIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
    assert_eq!(
        env_value(&command, "HISTSIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
    assert_eq!(
        env_value(&command, "WARP_INITIAL_HISTFILESIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
    assert_eq!(
        env_value(&command, "WARP_INITIAL_HISTSIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
}

#[test]
fn host_non_bash_command_does_not_set_history_size_sentinels() {
    let command = build_host_shell_command(
        shell_starter(ShellType::Zsh, "/bin/zsh"),
        None,
        HashMap::new(),
        None,
        false,
        false,
        false,
        false,
        false,
    );

    assert_eq!(env_value(&command, "HISTFILESIZE"), None);
    assert_eq!(env_value(&command, "HISTSIZE"), None);
    assert_eq!(env_value(&command, "WARP_INITIAL_HISTFILESIZE"), None);
    assert_eq!(env_value(&command, "WARP_INITIAL_HISTSIZE"), None);
}

#[test]
fn docker_sandbox_command_sets_history_size_sentinels() {
    let docker_starter =
        DockerSandboxShellStarter::new(shell_starter(ShellType::Bash, "sbx"), None);
    let command = build_docker_sandbox_command(
        &docker_starter,
        None,
        HashMap::new(),
        false,
        false,
        false,
        false,
        false,
    );

    assert_eq!(
        env_value(&command, "HISTFILESIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
    assert_eq!(
        env_value(&command, "HISTSIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
    assert_eq!(
        env_value(&command, "WARP_INITIAL_HISTFILESIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
    assert_eq!(
        env_value(&command, "WARP_INITIAL_HISTSIZE"),
        Some(Some(BASH_HISTORY_SIZE_SENTINEL.to_owned()))
    );
}

// `WARP_SSH_REUSE_CONTROL_MASTER` is the only channel by which the SSH
// wrapper in the bootstrap scripts learns that it may attach to a user-owned
// ControlMaster. Assert it is always exported (so an unset value can never be
// mistaken for "reuse enabled") and that it tracks the flag on both spawn
// paths. Ported alongside Warp `0d24d2cf` (#12465).
#[test]
fn host_command_exports_ssh_reuse_control_master_flag() {
    for (reuse, expected) in [(false, "0"), (true, "1")] {
        let command = build_host_shell_command(
            shell_starter(ShellType::Bash, "/bin/bash"),
            None,
            HashMap::new(),
            None,
            true,
            reuse,
            false,
            false,
            false,
        );

        assert_eq!(
            env_value(&command, "WARP_SSH_REUSE_CONTROL_MASTER"),
            Some(Some(expected.to_owned())),
            "host shell command should export WARP_SSH_REUSE_CONTROL_MASTER={expected} when reuse_ssh_control_master={reuse}"
        );
    }
}

#[test]
fn docker_sandbox_command_exports_ssh_reuse_control_master_flag() {
    for (reuse, expected) in [(false, "0"), (true, "1")] {
        let docker_starter =
            DockerSandboxShellStarter::new(shell_starter(ShellType::Bash, "sbx"), None);
        let command = build_docker_sandbox_command(
            &docker_starter,
            None,
            HashMap::new(),
            true,
            reuse,
            false,
            false,
            false,
        );

        assert_eq!(
            env_value(&command, "WARP_SSH_REUSE_CONTROL_MASTER"),
            Some(Some(expected.to_owned())),
            "docker sandbox command should export WARP_SSH_REUSE_CONTROL_MASTER={expected} when reuse_ssh_control_master={reuse}"
        );
    }
}

// `WARP_PROMPT_NODE_VERSION_ENABLED` is the only channel by which the bootstrap
// scripts learn that the Node.js Version chip is off and the per-prompt
// `node --version` spawn can be skipped. The bootstrap treats any value other
// than "0" as enabled, so assert the var is always exported and tracks the flag
// on both spawn paths.
#[test]
fn host_command_exports_node_version_chip_flag() {
    for (enabled, expected) in [(false, "0"), (true, "1")] {
        let command = build_host_shell_command(
            shell_starter(ShellType::Bash, "/bin/bash"),
            None,
            HashMap::new(),
            None,
            false,
            false,
            false,
            false,
            enabled,
        );

        assert_eq!(
            env_value(&command, "WARP_PROMPT_NODE_VERSION_ENABLED"),
            Some(Some(expected.to_owned())),
            "host shell command should export WARP_PROMPT_NODE_VERSION_ENABLED={expected} when node_version_chip_enabled={enabled}"
        );
    }
}

#[test]
fn docker_sandbox_command_exports_node_version_chip_flag() {
    for (enabled, expected) in [(false, "0"), (true, "1")] {
        let docker_starter =
            DockerSandboxShellStarter::new(shell_starter(ShellType::Bash, "sbx"), None);
        let command = build_docker_sandbox_command(
            &docker_starter,
            None,
            HashMap::new(),
            false,
            false,
            false,
            false,
            enabled,
        );

        assert_eq!(
            env_value(&command, "WARP_PROMPT_NODE_VERSION_ENABLED"),
            Some(Some(expected.to_owned())),
            "docker sandbox command should export WARP_PROMPT_NODE_VERSION_ENABLED={expected} when node_version_chip_enabled={enabled}"
        );
    }
}
