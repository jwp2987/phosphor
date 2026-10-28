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

#[test]
fn cwd_independent_commands_are_not_sensitive() {
    for command in [
        "ssh host",
        "ssh -p 2222 -J jump me@host",
        "ssh -i ~/.ssh/id_work host",
        "ssh -i /home/me/.ssh/id host",
        "ssh -i%d/.ssh/id host",
        "ssh -F ~/.ssh/alt_config host",
        "ssh -o IdentityFile=~/.ssh/id host",
        "ssh -o 'IdentityFile /abs/key' host",
        "ssh -oControlPath=none host",
        "ssh -o IdentityAgent=SSH_AUTH_SOCK host",
        "ssh -o ServerAliveInterval=30 host",
        "ssh -o ProxyJump=jump host",
        "ssh -4Ap2222 host",
        "ssh -L 8080:localhost:80 host",
        "ssh -i 'C:\\Users\\me\\.ssh\\id' host",
    ] {
        assert!(!is_cwd_sensitive(command), "{command}");
    }
}

#[test]
fn cwd_dependent_commands_are_sensitive() {
    for command in [
        "ssh -i ./key host",
        "ssh -i key.pem host",
        "ssh -i*.pem host",
        "ssh -i 'keys/*' host",
        "ssh h?st",
        "ssh -F config host",
        "ssh -E ssh.log host",
        "ssh -Ai./key host",
        "ssh -o IdentityFile=./key host",
        "ssh -o 'identityfile key' host",
        "ssh -oUserKnownHostsFile=known host",
        "ssh -o 'ProxyCommand=/usr/bin/nc %h %p' host",
        "ssh -o LocalCommand=/bin/true host",
        "ssh -i",
    ] {
        assert!(is_cwd_sensitive(command), "{command}");
    }
}

fn session(id: u64) -> SessionId {
    SessionId::from(id)
}

#[test]
fn follow_up_cds_once_on_the_same_host_and_session() {
    let mut follow_up = SplitFollowUp::default();
    follow_up.arm(Some(pending("/srv/app")), true);

    assert!(follow_up.on_ssh_session_bootstrapped(session(7), "me@box", ShellType::Bash));
    assert!(follow_up.has_ready_cd());
    // Only for the session it was prepared for, while the input is untouched.
    assert_eq!(
        follow_up.take_cd_for_bootstrap_precmd(Some(session(7)), true),
        Some("cd '/srv/app'".to_string())
    );
    // One-shot: nothing on any later bootstrap.
    assert!(!follow_up.has_ready_cd());
    assert_eq!(
        follow_up.take_cd_for_bootstrap_precmd(Some(session(7)), true),
        None
    );
    assert!(!follow_up.on_ssh_session_bootstrapped(session(8), "me@box", ShellType::Bash));
    assert_eq!(
        follow_up.take_cd_for_bootstrap_precmd(Some(session(8)), true),
        None
    );
}

#[test]
fn follow_up_skips_other_hosts_sessions_and_typed_input() {
    let mut follow_up = SplitFollowUp::default();
    follow_up.arm(Some(pending("/srv")), false);
    assert!(!follow_up.on_ssh_session_bootstrapped(session(1), "me@other", ShellType::Bash));
    assert!(!follow_up.has_ready_cd());

    follow_up.arm(Some(pending("/srv")), false);
    follow_up.on_ssh_session_bootstrapped(session(1), "me@box", ShellType::Bash);
    assert_eq!(
        follow_up.take_cd_for_bootstrap_precmd(Some(session(2)), true),
        None,
        "a different active session never gets the cd"
    );

    follow_up.arm(Some(pending("/srv")), false);
    follow_up.on_ssh_session_bootstrapped(session(1), "me@box", ShellType::Bash);
    assert_eq!(
        follow_up.take_cd_for_bootstrap_precmd(Some(session(1)), false),
        None,
        "typed input is never overwritten"
    );
    // ...and the dropped cd does not come back.
    assert_eq!(
        follow_up.take_cd_for_bootstrap_precmd(Some(session(1)), true),
        None
    );
}

#[test]
fn follow_up_never_outlives_a_failed_replay() {
    let mut follow_up = SplitFollowUp::default();
    follow_up.arm(Some(pending("/srv/app")), true);
    // The replayed ssh block completes (auth failed, Ctrl-C, exit) before any ssh bootstrap.
    follow_up.on_user_block_completed();
    // A later, unrelated ssh to the same host in this pane inherits nothing.
    assert!(!follow_up.on_ssh_session_bootstrapped(session(3), "me@box", ShellType::Bash));
    assert!(!follow_up.has_ready_cd());
    assert_eq!(
        follow_up.take_cd_for_bootstrap_precmd(Some(session(3)), true),
        None
    );
}
