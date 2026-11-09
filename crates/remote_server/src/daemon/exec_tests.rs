use std::collections::HashMap;

use crate::proto::{RunCommandRequest, run_command_response, server_message};

use super::*;

/// Tests use bash when present and skip otherwise.
fn bash() -> Option<SessionShell> {
    resolve_in_path("bash").map(|path| SessionShell {
        kind: ShellKind::Bash,
        path: Some(path),
    })
}

fn request(command: &str) -> RunCommandRequest {
    RunCommandRequest {
        command: command.to_string(),
        working_directory: None,
        environment_variables: HashMap::new(),
        session_id: 1,
    }
}

fn success(message: server_message::Message) -> RunCommandSuccess {
    let server_message::Message::RunCommandResponse(resp) = message else {
        panic!("wrong response type");
    };
    let Some(run_command_response::Result::Success(ok)) = resp.result else {
        panic!("expected success: {resp:?}");
    };
    ok
}

#[test]
fn shell_names_match_the_apps_rules() {
    assert_eq!(ShellKind::from_name("/usr/bin/zsh"), Some(ShellKind::Zsh));
    assert_eq!(ShellKind::from_name("-bash"), Some(ShellKind::Bash));
    assert_eq!(ShellKind::from_name("fish"), Some(ShellKind::Fish));
    assert_eq!(ShellKind::from_name("/opt/nu"), Some(ShellKind::Nu));
    assert_eq!(ShellKind::from_name("pwsh"), Some(ShellKind::PowerShell));
    assert_eq!(ShellKind::from_name("tcsh"), None);
}

#[test]
fn each_shell_skips_its_user_configuration() {
    for (kind, flag) in [
        (ShellKind::Zsh, "-f"),
        (ShellKind::Bash, "--norc"),
        (ShellKind::Fish, "--no-config"),
    ] {
        let cmd = build_command(&SessionShell { kind, path: None }, "echo hi");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![flag.to_string(), "-c".to_string(), "echo hi".to_string()]
        );
    }
}

#[test]
fn runs_a_command_and_reports_output_and_exit_code() {
    let Some(shell) = bash() else { return };
    let ok = success(run_command(
        &shell,
        request("printf out; printf err >&2; exit 3"),
        &Cancel::default(),
    ));
    assert_eq!(ok.stdout, b"out");
    assert_eq!(ok.stderr, b"err");
    assert_eq!(ok.exit_code, Some(3));
}

#[test]
fn honours_working_directory_and_environment() {
    let Some(shell) = bash() else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut req = request("printf '%s %s' \"$(pwd -P)\" \"$FOO\"");
    req.working_directory = Some(dir.path().to_string_lossy().into_owned());
    req.environment_variables.insert("FOO".into(), "bar".into());
    let ok = success(run_command(&shell, req, &Cancel::default()));
    let expected = format!("{} bar", dir.path().canonicalize().unwrap().display());
    assert_eq!(String::from_utf8_lossy(&ok.stdout), expected);
}

#[test]
fn a_shell_that_cannot_start_is_an_execution_failure() {
    let shell = SessionShell {
        kind: ShellKind::Bash,
        path: Some(PathBuf::from("/definitely/not/a/shell")),
    };
    let server_message::Message::RunCommandResponse(resp) =
        run_command(&shell, request("true"), &Cancel::default())
    else {
        panic!("wrong response type");
    };
    let Some(run_command_response::Result::Error(e)) = resp.result else {
        panic!("expected error");
    };
    assert_eq!(e.code, RunCommandErrorCode::ExecutionFailed as i32);
}

#[test]
fn oversized_output_is_truncated_under_the_wire_cap() {
    let mut out = vec![b'a'; MAX_MESSAGE_SIZE];
    let mut err = vec![b'b'; 4096];
    truncate_to_wire_limit(&mut out, &mut err);
    assert!(out.len() + err.len() <= MAX_MESSAGE_SIZE - 1024);
    let mut small_out = vec![1; 10];
    let mut small_err = vec![2; 10];
    truncate_to_wire_limit(&mut small_out, &mut small_err);
    assert_eq!((small_out.len(), small_err.len()), (10, 10));
}

#[test]
fn aborting_kills_a_running_command() {
    let Some(shell) = bash() else { return };
    let cancel = std::sync::Arc::new(Cancel::default());
    let worker = {
        let cancel = cancel.clone();
        std::thread::spawn(move || run_command(&shell, request("sleep 30"), &cancel))
    };
    // Wait until the child is recorded, then cancel; the worker must return long before 30 s.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let started = std::time::Instant::now();
    cancel.cancel();
    let _ = worker.join().unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}
