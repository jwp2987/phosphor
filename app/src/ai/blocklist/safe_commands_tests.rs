use super::*;

fn defaults() -> Vec<String> {
    default_safe_commands()
}

fn ok(command: &str) -> bool {
    matches_allowlist(command, &defaults())
}

#[test]
fn the_defaults_hold_no_git_subcommand() {
    assert!(DEFAULT_SAFE_COMMANDS.iter().all(|e| !e.starts_with("git")));
    assert!(!ok("git status"));
    // A user may add one deliberately; the subcommand rules then apply.
    let entries = vec!["git status".to_owned()];
    assert!(matches_allowlist("git status -sb", &entries));
}

#[test]
fn plain_read_only_commands_match() {
    for command in [
        "ls",
        "ls -la",
        "ls -la src",
        "cat Cargo.toml",
        "grep -rn foo src",
        "rg foo",
        "df -h",
        "du -sh target",
        "pwd",
        "  ls   -l  ",
    ] {
        assert!(ok(command), "{command:?} should match");
    }
}

#[test]
fn anything_a_shell_could_do_besides_run_one_program_is_refused() {
    for command in [
        "ls | cat",
        "ls > out.txt",
        "ls >> out.txt",
        "cat < /etc/hosts",
        "ls; rm -rf x",
        "ls && rm x",
        "ls || rm x",
        "ls & rm x",
        "ls $(rm x)",
        "ls `rm x`",
        "ls $HOME",
        "ls (x)",
        "(ls)",
        "{ ls; }",
        "ls *",
        "ls ?",
        "ls [a]",
        "ls ~",
        "ls !",
        "ls # comment",
        "ls \"x\"",
        "ls 'x'",
        "ls a\\ b",
        "ls\nrm x",
        "ls\rrm x",
        "ls\trm",
        "FOO=1 ls",
        "ls --color=auto",
        "ls \u{202e}",
        "",
        "   ",
    ] {
        assert!(!ok(command), "{command:?} must be refused");
    }
}

#[test]
fn the_program_name_is_exact() {
    for command in [
        "./ls",
        "/bin/ls",
        "lsx",
        "l",
        "LS",
        "ls2",
        "-ls",
        ".ls",
        "git",
        "git branch foo",
        "git -c core.pager=x status",
        "git push",
        "gitstatus",
    ] {
        assert!(!ok(command), "{command:?} must be refused");
    }
}

#[test]
fn flags_that_write_run_something_or_never_end_are_refused() {
    let entries: Vec<String> = [
        "tail", "rg", "date", "sort", "file", "tree", "git diff", "git log", "git show",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for command in [
        "tail -f log",
        "tail -F log",
        "tail -nf log",
        "tail --follow log",
        "rg --pre foo x",
        "rg -z x",
        "date -s now",
        "date now",
        "sort -o out in",
        "sort --output out in",
        "file -C",
        "tree -o out",
        "git diff --output out",
        "git diff --ext-diff",
        "git log --output out",
        "git show --open-files-in-pager",
        "git diff -O",
    ] {
        assert!(
            !matches_allowlist(command, &entries),
            "{command:?} must be refused"
        );
    }
    assert!(matches_allowlist("tail -n 20 log", &entries));
    assert!(matches_allowlist("date -u", &entries));
    assert!(matches_allowlist("sort in", &entries));
}

#[test]
fn programs_that_run_or_write_are_never_vouched_for_even_if_listed() {
    let entries: Vec<String> = [
        "find",
        "env",
        "xargs",
        "sudo",
        "sh",
        "bash",
        "sed",
        "tee",
        "cp",
        "rm",
        "curl",
        "git",
        "git branch",
        "git push",
        "less",
        "vim",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for command in [
        "find . -delete",
        "find .",
        "env ls",
        "xargs rm",
        "sudo ls",
        "sh x",
        "bash x",
        "sed -i s x",
        "tee x",
        "cp a b",
        "rm x",
        "curl http://x",
        "git branch foo",
        "git push",
        "git",
        "less x",
        "vim x",
    ] {
        assert!(
            !matches_allowlist(command, &entries),
            "{command:?} must be refused"
        );
    }
}

#[test]
fn an_empty_list_matches_nothing_and_a_malformed_entry_matches_nothing() {
    assert!(!matches_allowlist("ls", &[]));
    for entry in [
        "", "  ", "ls | cat", "/bin/ls", "-l", ".x", "a b c d", "ls;", "$x", "ls*",
    ] {
        assert!(!is_valid_entry(entry), "{entry:?}");
        assert!(!matches_allowlist("ls", &[entry.to_owned()]), "{entry:?}");
    }
}

#[test]
fn a_user_entry_with_a_subcommand_needs_the_subcommand_first() {
    let entries = vec!["git status".to_owned()];
    assert!(matches_allowlist("git status", &entries));
    assert!(!matches_allowlist("git log", &entries));
    assert!(!matches_allowlist("git", &entries));
    assert!(!matches_allowlist("git status; git push", &entries));
}

#[test]
fn every_default_entry_is_valid_and_matches_itself() {
    for entry in DEFAULT_SAFE_COMMANDS {
        assert!(is_valid_entry(entry), "{entry:?}");
        assert!(ok(entry), "{entry:?} should match itself");
    }
}
