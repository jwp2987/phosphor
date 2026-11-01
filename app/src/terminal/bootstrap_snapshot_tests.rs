//! Snapshots of the setup lines the app types into a subshell, per shell, unmarked and with
//! setup-line markers (`ESC ] 7771 ; <token> ; <index> BEL`). The lines are what the shells run;
//! a change to any of them is meant to show up here as a reviewed diff
//! (`cargo insta review`), not as an edited golden string in a test.
//!
//! The script being chunked is a small fixed one with a quote, a backslash and a non-ASCII
//! character, so the per-shell quoting is visible. The real bootstrap scripts are covered by
//! `bootstrap_test.rs`; this pins the framing around them.

use super::*;

macro_rules! snap {
    ($name:literal, $value:expr) => {
        insta::with_settings!({
            prepend_module_to_snapshot => false,
            snapshot_path => "snapshots",
        }, {
            insta::assert_snapshot!($name, $value);
        });
    };
}

const SCRIPT: &str = "echo 'q' \\ \"x\" é";
const TOKEN: u64 = 4242;

fn render(lines: &[String]) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| format!("[{index}] {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn both(unmarked: &[String], marked: &[String]) -> String {
    format!(
        "unmarked\n{}\n\nmarked (token {TOKEN})\n{}",
        render(unmarked),
        render(marked)
    )
}

fn posix(shell_type: ShellType) -> String {
    both(
        &chunked_eval_lines_with_markers(shell_type, SCRIPT, None),
        &chunked_eval_lines_with_markers(shell_type, SCRIPT, Some(TOKEN)),
    )
}

#[test]
fn bootstrap_setup_lines_bash() {
    snap!("bootstrap_setup_lines_bash", posix(ShellType::Bash));
}

#[test]
fn bootstrap_setup_lines_zsh() {
    snap!("bootstrap_setup_lines_zsh", posix(ShellType::Zsh));
}

#[test]
fn bootstrap_setup_lines_fish() {
    snap!("bootstrap_setup_lines_fish", posix(ShellType::Fish));
}

#[test]
fn bootstrap_setup_lines_nu() {
    let id = SessionId::from(TOKEN);
    snap!(
        "bootstrap_setup_lines_nu",
        both(
            &nu_subshell_lines_with_markers(SCRIPT, id, false),
            &nu_subshell_lines_with_markers(SCRIPT, id, true),
        )
    );
}

/// PowerShell's first two lines (the third is the whole script, covered elsewhere).
#[test]
fn bootstrap_setup_lines_pwsh() {
    let (history, policy) = powershell_marked_setup_lines(TOKEN);
    snap!(
        "bootstrap_setup_lines_pwsh",
        both(
            &[
                POWERSHELL_HISTORY_HANDLER_LINE.to_owned(),
                POWERSHELL_EXECUTION_POLICY_LINE.to_owned(),
            ],
            &[history, policy],
        )
    );
}
