use super::*;

#[test]
fn head_changing_commands() {
    assert!(command_may_change_head("git checkout main"));
    assert!(command_may_change_head("cd x && git -C y pull"));
    assert!(command_may_change_head("git -c a=b rebase -i"));
    assert!(!command_may_change_head("git status"));
    assert!(!command_may_change_head(""));
}

#[test]
fn cwd_changing_commands() {
    assert!(command_may_change_cwd("cd /tmp"));
    assert!(command_may_change_cwd("ls; pushd x"));
    assert!(command_may_change_cwd("(cd a && make)"));
    assert!(!command_may_change_cwd("echo cdrom"));
}

#[test]
fn git_state_commands() {
    assert!(command_may_change_git_state("git commit -m x"));
    assert!(command_may_change_git_state("cd a && git pull"));
    assert!(command_may_change_git_state("git.exe checkout b"));
    assert!(command_may_change_git_state("gh pr checkout 3"));
    assert!(!command_may_change_git_state("cargo build"));
    assert!(!command_may_change_git_state("echo digit"));
    assert!(!command_may_change_git_state(""));
}
