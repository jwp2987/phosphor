use super::*;
use crate::terminal::ssh::util::InteractiveSshCommand;

const REMOTE: SessionType = SessionType::WarpifiedRemote { host_id: None };
const LOCAL: SessionType = SessionType::Local;

fn info(command: &str, parsed: bool) -> SubshellInitializationInfo {
    SubshellInitializationInfo {
        spawning_command: command.to_string(),
        was_triggered_by_rc_file_snippet: false,
        env_var_collection_name: None,
        ssh_connection_info: parsed.then(InteractiveSshCommand::default),
    }
}

fn inherited(
    session_type: &SessionType,
    subshell_info: Option<&SubshellInitializationInfo>,
    legacy: Option<&str>,
    spawning: Option<&SessionType>,
) -> Option<String> {
    inherited_ssh_command(
        session_type,
        resolve_ssh_spawning_command(subshell_info, legacy),
        spawning,
    )
}

#[test]
fn wrapper_session_plain_ssh_is_inherited() {
    assert_eq!(
        inherited(&REMOTE, None, Some("ssh host"), Some(&LOCAL)),
        Some("ssh host".to_string())
    );
}

#[test]
fn user_options_are_kept_verbatim() {
    for command in [
        "ssh -J jump -p 2222 host",
        "ssh -i ~/.ssh/id_work -l deploy host",
        "ssh -o 'ProxyCommand=nc -X 5 -x proxy:1080 %h %p' host",
        "ssh -o \"ServerAliveInterval 30\" user@host.example",
        "ssh -4 -A -L 8080:localhost:80 host",
    ] {
        assert_eq!(
            inherited(&REMOTE, None, Some(command), Some(&LOCAL)),
            Some(command.to_string()),
            "{command}"
        );
    }
    // Surrounding whitespace (the block's command text) is not part of the command.
    assert_eq!(
        inherited(&REMOTE, None, Some("  ssh -p 2222 host \t"), Some(&LOCAL)),
        Some("ssh -p 2222 host".to_string())
    );
}

#[test]
fn tmux_warpified_session_is_inherited() {
    assert_eq!(
        inherited(&REMOTE, Some(&info("ssh host", true)), None, Some(&LOCAL)),
        Some("ssh host".to_string())
    );
}

#[test]
fn local_session_is_not_inherited() {
    assert_eq!(
        inherited(&LOCAL, None, Some("ssh host"), Some(&LOCAL)),
        None
    );
    assert_eq!(
        inherited(&LOCAL, Some(&info("ssh host", true)), None, Some(&LOCAL)),
        None
    );
}

#[test]
fn nested_or_unknown_spawning_session_is_not_inherited() {
    assert_eq!(
        inherited(&REMOTE, None, Some("ssh host"), Some(&REMOTE)),
        None
    );
    assert_eq!(inherited(&REMOTE, None, Some("ssh host"), None), None);
}

#[test]
fn missing_or_unparsed_command_is_not_inherited() {
    assert_eq!(inherited(&REMOTE, None, None, Some(&LOCAL)), None);
    assert_eq!(
        inherited(&REMOTE, Some(&info("ssh host", false)), None, Some(&LOCAL)),
        None
    );
}

#[test]
fn subshell_info_takes_precedence_over_wrapper_command() {
    assert_eq!(
        resolve_ssh_spawning_command(Some(&info("ssh a", false)), Some("ssh b")),
        None
    );
}

#[test]
fn wrappers_and_ssh_like_commands_are_not_inherited() {
    for command in [
        "sshpass -p x ssh host",
        "command ssh host",
        "FOO=1 ssh host",
        "gcloud compute ssh vm",
        "s",
    ] {
        assert_eq!(
            inherited(&REMOTE, None, Some(command), Some(&LOCAL)),
            None,
            "{command}"
        );
        // The tmux-warpified path records any parsed ssh-like command; it must still not be
        // replayed.
        assert_eq!(
            inherited(&REMOTE, Some(&info(command, true)), None, Some(&LOCAL)),
            None,
            "{command}"
        );
    }
}

#[test]
fn non_interactive_ssh_is_not_inherited() {
    for command in [
        "ssh host ls",
        "ssh -T host",
        "ssh -N host",
        "ssh",
        "ssh -p 22",
    ] {
        assert_eq!(
            inherited(&REMOTE, None, Some(command), Some(&LOCAL)),
            None,
            "{command}"
        );
    }
}

#[test]
fn shell_syntax_is_never_replayed() {
    for command in [
        "ssh host; rm -rf ~",
        "ssh host && echo done",
        "ssh host | tee log",
        "ssh host > out",
        "ssh $(cat target)",
        "ssh `cat target`",
        "ssh $HOST",
        "ssh -i ${KEY} host",
        "ssh host!",
        "(ssh host)",
        "ssh host\nrm -rf ~",
        "ssh ho\rst",
        "ssh ho\u{1b}st",
    ] {
        assert_eq!(replayable_ssh_command(command), None, "{command:?}");
        assert_eq!(
            inherited(&REMOTE, Some(&info(command, true)), None, Some(&LOCAL)),
            None,
            "{command:?}"
        );
    }
}

fn pending(path: &str) -> PendingRemoteCd {
    PendingRemoteCd {
        remote_host: "me@box".to_string(),
        path: path.to_string(),
    }
}

#[test]
fn remote_cd_matches_host() {
    assert_eq!(
        remote_cd_command(&pending("/srv/app"), "me@box", ShellType::Bash, true),
        Some("cd '/srv/app'".to_string())
    );
}

#[test]
fn remote_cd_quotes_remote_supplied_path() {
    assert_eq!(
        remote_cd_command(&pending("/srv/it's here"), "me@box", ShellType::Bash, true),
        Some(r#"cd '/srv/it'"'"'s here'"#.to_string())
    );
    assert_eq!(
        remote_cd_command(&pending("/tmp/$(reboot);x"), "me@box", ShellType::Zsh, true),
        Some("cd '/tmp/$(reboot);x'".to_string())
    );
    // A control character is never typed raw into the pty (it could submit the line).
    let cmd = remote_cd_command(&pending("/tmp/a\nreboot"), "me@box", ShellType::Bash, true)
        .expect("same host, empty input");
    assert!(!cmd.contains('\n'), "{cmd:?}");
}

#[test]
fn remote_cd_skips_host_mismatch_dirty_input_and_empty_path() {
    assert_eq!(
        remote_cd_command(&pending("/srv"), "me@other", ShellType::Bash, true),
        None
    );
    assert_eq!(
        remote_cd_command(&pending("/srv"), "me@box", ShellType::Bash, false),
        None
    );
    assert_eq!(
        remote_cd_command(&pending(""), "me@box", ShellType::Bash, true),
        None
    );
}
