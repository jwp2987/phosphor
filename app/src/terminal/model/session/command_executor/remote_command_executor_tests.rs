use std::collections::HashMap;
use std::time::Duration;

use futures::FutureExt as _;

use super::{
    InflightLimit, ShellType, build_exec_script, command_timeout_from_env, max_inflight_from_env,
    shell_mode_timeout_from_env,
};

/// #1240: the in-flight limit is a counting semaphore whose waiters leave the queue with their
/// futures, so a superseded generator never holds a slot it will not use.
#[test]
fn inflight_limit_admits_up_to_the_limit_and_frees_a_slot_when_a_permit_drops() {
    let limit = InflightLimit::new(Some(2));
    let first = limit.acquire().now_or_never().flatten();
    let second = limit.acquire().now_or_never().flatten();
    assert!(first.is_some() && second.is_some());
    assert_eq!(limit.available(), Some(0));

    // The third request waits...
    let mut third = Box::pin(limit.acquire());
    assert!(third.as_mut().now_or_never().is_none());

    // ...and a waiter that gives up (its caller was superseded) leaves no trace.
    drop(third);
    assert_eq!(limit.available(), Some(0));

    // A finished request frees its slot for the next one.
    drop(first);
    assert!(limit.acquire().now_or_never().flatten().is_some());
}

#[test]
fn an_unlimited_executor_never_waits() {
    let limit = InflightLimit::new(None);
    assert_eq!(limit.available(), None);
    for _ in 0..64 {
        // No permit to hold, and never pending.
        assert!(limit.acquire().now_or_never().is_some());
    }
}

#[test]
fn max_inflight_env_parsing() {
    assert_eq!(max_inflight_from_env(None), Some(4));
    assert_eq!(max_inflight_from_env(Some("")), Some(4));
    assert_eq!(max_inflight_from_env(Some("lots")), Some(4));
    assert_eq!(max_inflight_from_env(Some(" 2 ")), Some(2));
    assert_eq!(max_inflight_from_env(Some("0")), None);
}

#[test]
fn command_timeout_env_parsing() {
    assert_eq!(
        command_timeout_from_env(None),
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        command_timeout_from_env(Some("x")),
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        command_timeout_from_env(Some("15")),
        Some(Duration::from_secs(15))
    );
    assert_eq!(command_timeout_from_env(Some("0")), None);
}

/// Parity design R1: the shell-mode channel's output cap marks truncation instead of failing.
#[test]
fn channel_output_is_cut_at_the_cap_and_marked() {
    assert_eq!(
        super::cap_stdout(b"abcdef".to_vec(), 4),
        (b"abcd".to_vec(), true)
    );
    assert_eq!(
        super::cap_stdout(b"abcd".to_vec(), 4),
        (b"abcd".to_vec(), false)
    );
    // 0 = no cap.
    assert_eq!(
        super::cap_stdout(b"abcdef".to_vec(), 0),
        (b"abcdef".to_vec(), false)
    );
}

/// The generator executor and the shell-mode channel of one master share ONE in-flight limit
/// (#1240): two limits of four would be eight mux clients on a master that allows ten sessions.
#[test]
fn one_master_has_one_inflight_limit() {
    let socket = std::path::Path::new("/nonexistent/cm-limit-test-socket");
    let a = super::inflight_limit_for_socket(socket);
    let b = super::inflight_limit_for_socket(socket);
    let _held = a.acquire().now_or_never().flatten();
    assert_eq!(b.available(), a.available());
    assert_eq!(a.available(), Some(3));
    let other = super::inflight_limit_for_socket(std::path::Path::new("/nonexistent/other"));
    assert_eq!(other.available(), Some(4));
}

#[test]
fn login_shell_types_map_to_command_builder_kinds() {
    use crate::terminal::shell::ShellType;
    use remote_server::shellcmd::ShellKind;
    let kind = super::super::shell_kind_for_remote;
    assert_eq!(kind(ShellType::Bash), ShellKind::Posix);
    assert_eq!(kind(ShellType::Zsh), ShellKind::Posix);
    assert_eq!(kind(ShellType::Fish), ShellKind::Fish);
    assert_eq!(kind(ShellType::Nu), ShellKind::Nu);
    assert_eq!(kind(ShellType::PowerShell), ShellKind::Pwsh);
}

#[test]
fn shell_mode_timeout_matches_the_helper_budget() {
    assert_eq!(
        shell_mode_timeout_from_env(None),
        Some(Duration::from_secs(120))
    );
    assert_eq!(
        shell_mode_timeout_from_env(Some("x")),
        Some(Duration::from_secs(120))
    );
    assert_eq!(
        shell_mode_timeout_from_env(Some("15")),
        Some(Duration::from_secs(15))
    );
    assert_eq!(shell_mode_timeout_from_env(Some("0")), None);
}

/// #2180: for a nushell session the script is POSIX (`&&`, `export`), because it is run by `sh`,
/// not by nu; no nu syntax (`$env.`) appears.
#[test]
fn nu_session_script_is_posix() {
    let mut env = HashMap::new();
    env.insert("A".to_string(), "it's".to_string());
    let script = build_exec_script("git ls-files", ShellType::Nu, Some("/w/it's"), Some(env));
    assert!(script.starts_with("export "), "{script}");
    assert!(!script.contains("$env."), "{script}");
    assert!(
        script.contains("cd '/w/it'\"'\"'s' && git ls-files"),
        "{script}"
    );
}

#[test]
fn other_shells_keep_their_own_script_text() {
    let script = build_exec_script("ls", ShellType::Bash, Some("/w"), None);
    assert_eq!(script, "cd '/w' && ls");
    let script = build_exec_script("ls", ShellType::Fish, None, None);
    assert_eq!(script, "ls");
}
