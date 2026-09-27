use warp_util::path::EscapeChar;

use super::{ExecutedCommands, executed_commands};

fn analysis(source: &str) -> ExecutedCommands {
    executed_commands(source, EscapeChar::Backslash)
}

fn contains_command(result: &ExecutedCommands, expected: &[&str]) -> bool {
    result.commands.iter().any(|command| {
        command
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
    })
}

/// `source` is fully resolved and one of the commands it executes is exactly `expected`.
#[track_caller]
fn assert_runs(source: &str, expected: &[&str]) {
    assert_runs_with(source, EscapeChar::Backslash, expected);
}

#[track_caller]
fn assert_runs_with(source: &str, escape_char: EscapeChar, expected: &[&str]) {
    let result = executed_commands(source, escape_char);
    assert!(
        contains_command(&result, expected),
        "{source:?} should be found to run {expected:?}; found {:?}",
        result.commands
    );
    assert!(
        result.is_fully_resolved(),
        "{source:?} should be fully resolved; unresolved: {:?}",
        result.unresolved
    );
}

#[track_caller]
fn assert_unresolved(source: &str) {
    let result = analysis(source);
    assert!(
        !result.is_fully_resolved(),
        "{source:?} has a command word that cannot be known statically and must be reported \
         unresolved; found {:?}",
        result.commands
    );
}

const RM: &[&str] = &["rm", "-rf", "~"];

#[test]
fn test_redirections_do_not_hide_the_command_word() {
    for source in [
        ">/dev/null rm -rf ~",
        "> /dev/null rm -rf ~",
        "</dev/null rm -rf ~",
        "2>/dev/null rm -rf ~",
        "2>&1 rm -rf ~",
        "&>/dev/null rm -rf ~",
        ">>log rm -rf ~",
        "{fd}>/dev/null rm -rf ~",
        "rm>/dev/null -rf ~",
        "rm>/dev/null -rf>/dev/null ~",
        "rm 2>/dev/null -rf ~",
        "rm -rf ~ >/dev/null 2>&1",
        "rm<<<x -rf ~",
        "X=1 >/dev/null rm -rf ~",
    ] {
        assert_runs(source, RM);
    }
}

#[test]
fn test_brace_expansion_forms_the_command() {
    assert_runs("{rm,-rf,~}", RM);
    assert_runs("{rm,-rf,~} ", RM);
    assert_runs("{r,}m -rf ~", &["rm", "m", "-rf", "~"]);
    assert_runs("echo ok; {rm,-rf,~}", RM);
    assert_runs("{rm,-r{f,}} ~", &["rm", "-rf", "-r", "~"]);
    // Sequences expand too, and quoting suppresses expansion.
    assert_runs("echo {a..c}", &["echo", "a", "b", "c"]);
    assert_runs("echo {3..1}", &["echo", "3", "2", "1"]);
    assert_runs("echo '{rm,-rf}'", &["echo", "{rm,-rf}"]);
    assert_runs("echo \\{rm,-rf}", &["echo", "{rm,-rf}"]);
    // A lone `{}` / `{x}` is literal, as in bash.
    assert_runs("echo {} {x}", &["echo", "{}", "{x}"]);
}

#[test]
fn test_compound_commands_expose_their_bodies() {
    for source in [
        "if true; then rm -rf ~; fi",
        "if true\nthen\n  rm -rf ~\nfi",
        "if false; then :; else rm -rf ~; fi",
        "if false; then :; elif rm -rf ~; then :; fi",
        "if rm -rf ~; then :; fi",
        "while true; do rm -rf ~; done",
        "until false; do rm -rf ~; done",
        "for i in 1 2; do rm -rf ~; done",
        "for i do rm -rf ~; done",
        "for ((i = 0; i < 1; i++)); do rm -rf ~; done",
        "select x in a b; do rm -rf ~; done",
        "case x in x) rm -rf ~;; esac",
        "case x in\n  (x|y) rm -rf ~ ;;\n  *) echo other ;;\nesac",
        "case x in y) echo;; x) rm -rf ~; esac",
        "{ rm -rf ~; }",
        "( rm -rf ~ )",
        "(rm -rf ~)",
        "((1)) && rm -rf ~",
        // dash parses `((` as nested subshells and fish as command substitution.
        "((rm -rf ~))",
        "echo $((rm -rf ~))",
        "! rm -rf ~",
        "time rm -rf ~",
        "time -p rm -rf ~",
        "[[ -d ~ ]] && rm -rf ~",
        "[[ a < b && -d ~ ]] && rm -rf ~",
        "f() { rm -rf ~; }; f",
        "f () { rm -rf ~; }",
        "function f { rm -rf ~; }",
        "function f() { rm -rf ~; }",
        "coproc rm -rf ~",
    ] {
        assert_runs(source, RM);
    }
}

#[test]
fn test_command_lists_are_all_checked() {
    for source in [
        "true; rm -rf ~",
        "true && rm -rf ~",
        "false || rm -rf ~",
        "echo | rm -rf ~",
        "echo |& rm -rf ~",
        "sleep 1 & rm -rf ~",
        "true\nrm -rf ~",
        "echo a # comment\nrm -rf ~",
    ] {
        assert_runs(source, RM);
    }
}

#[test]
fn test_substitutions_are_analysed_recursively() {
    for source in [
        "echo $(rm -rf ~)",
        "echo \"$(rm -rf ~)\"",
        "echo `rm -rf ~`",
        "echo \"`rm -rf ~`\"",
        "cat <(rm -rf ~)",
        "tee >(rm -rf ~)",
        "echo $(( $(rm -rf ~) + 1 ))",
        "echo ${x:-$(rm -rf ~)}",
        "echo $(echo $(rm -rf ~))",
        "X=$(rm -rf ~) true",
        "FOO=(a $(rm -rf ~)) true",
        "cat <<EOF\n$(rm -rf ~)\nEOF",
        "cat <<-EOF\n\t`rm -rf ~`\n\tEOF",
    ] {
        assert_runs(source, RM);
    }

    // A quoted here-document delimiter means the body is data, not commands.
    let result = analysis("cat <<'EOF' > out.txt\n$(rm -rf ~)\nit's if\nEOF\necho done");
    assert!(!contains_command(&result, RM), "{:?}", result.commands);
    assert!(contains_command(&result, &["echo", "done"]));
    assert!(result.is_fully_resolved(), "{:?}", result.unresolved);
}

#[test]
fn test_precommand_wrappers_are_peeled() {
    for source in [
        "env rm -rf ~",
        "env -i FOO=1 rm -rf ~",
        "env -u HOME -- rm -rf ~",
        "env - rm -rf ~",
        "/usr/bin/env rm -rf ~",
        "command rm -rf ~",
        "command -p rm -rf ~",
        "builtin rm -rf ~",
        "exec rm -rf ~",
        "exec -a name rm -rf ~",
        "nice rm -rf ~",
        "nice -n 5 rm -rf ~",
        "nice -5 rm -rf ~",
        "nohup rm -rf ~",
        "sudo rm -rf ~",
        "sudo -u root -- rm -rf ~",
        "sudo -E -H rm -rf ~",
        "doas rm -rf ~",
        "timeout 5 rm -rf ~",
        "timeout -s KILL 5s rm -rf ~",
        "stdbuf -oL rm -rf ~",
        "setsid rm -rf ~",
        "ionice -c 3 rm -rf ~",
        "taskset -c 0 rm -rf ~",
        "flock /tmp/lock rm -rf ~",
        "sudo env nice -n 1 nohup rm -rf ~",
        "xargs rm -rf ~",
        "xargs -0 rm -rf ~",
        r"find . -exec rm -rf ~ \;",
        "find . -name x -execdir rm -rf ~ +",
        "eval rm -rf ~",
        "eval 'rm -rf ~'",
        "sh -c 'rm -rf ~'",
        "bash -lc \"rm -rf ~\"",
        "bash -o pipefail -c 'rm -rf ~'",
        "zsh -c 'true; rm -rf ~'",
        "env -S 'rm -rf ~'",
        "su -c 'rm -rf ~' root",
        "watch -n 1 rm -rf ~",
        "alias x='rm -rf ~'",
        "trap 'rm -rf ~' EXIT",
    ] {
        assert_runs(source, RM);
    }
    assert_runs("xargs -0 -I{} rm {}", &["rm", "{}"]);

    // `command -v` only describes a command; it does not run it.
    let result = analysis("command -v rm");
    assert!(!contains_command(&result, &["rm"]), "{:?}", result.commands);
}

#[test]
fn test_quoting_is_removed() {
    for source in [
        "'r'm -rf ~",
        "\"rm\" -rf ~",
        "r\"m\" -rf ~",
        "\\rm -rf ~",
        "r\\m -rf ~",
        "r\\\nm -rf ~",
        "$'rm' -rf ~",
        "$\"rm\" -rf ~",
        "$'\\x72m' -rf ~",
        "$'\\162m' -rf ~",
        "rm '-rf' \"~\"",
        "X=1 rm -rf ~",
        "FOO=a=b rm -rf ~",
        "FOO=(a b) rm -rf ~",
        "FOO+=x rm -rf ~",
    ] {
        assert_runs(source, RM);
    }
}

#[test]
fn test_policy_spellings_include_the_bare_program_name() {
    let result = analysis("/bin/rm -rf ~ && ./rm -rf ~");
    let spellings = result.policy_spellings();
    assert!(spellings.contains(&"/bin/rm -rf ~".to_string()));
    assert!(spellings.contains(&"rm -rf ~".to_string()), "{spellings:?}");

    // A *quoted* expansion confined to the directory still names a static program.
    let result = analysis("\"$HOME\"/bin/rm -rf ~");
    assert!(result.is_fully_resolved(), "{:?}", result.unresolved);
    assert!(
        result.policy_spellings().contains(&"rm -rf ~".to_string()),
        "{:?}",
        result.policy_spellings()
    );
}

#[test]
fn test_run_time_command_words_are_unresolved() {
    for source in [
        "$CMD -rf ~",
        "${CMD} -rf ~",
        "\"$CMD\" -rf ~",
        "$(which rm) -rf ~",
        "`which rm` -rf ~",
        "R=rm; $R -rf ~",
        // Unquoted expansions are word-split, so even the directory part can supply the name.
        "$HOME/bin/rm -rf ~",
        "/bin/r? -rf ~",
        "/bin/r[m] -rf ~",
        "rm-$V -rf ~",
        "sudo $CMD -rf ~",
        "env FOO=1 $CMD",
        "xargs $CMD",
        "eval \"$CMD\"",
        "sh -c \"$CMD\"",
        "!rm",
        "echo !!",
        "^foo^rm",
    ] {
        assert_unresolved(source);
    }
}

#[test]
fn test_unparseable_input_is_unresolved() {
    for source in [
        "echo 'unterminated",
        "echo \"unterminated",
        "echo $(rm -rf ~",
        "echo `rm -rf ~",
        "echo ${x",
        "if true; then rm -rf ~",
        "while true; do rm -rf ~",
        "case x in x) rm -rf ~;;",
        "{ rm -rf ~",
        "( rm -rf ~",
        "rm -rf ~ )",
        "fi",
        "echo >",
        "echo {1..100000}",
    ] {
        assert_unresolved(source);
    }
}

#[test]
fn test_ordinary_commands_are_resolved() {
    for (source, expected) in [
        ("git status", &["git", "status"][..]),
        ("ls -la | grep foo", &["grep", "foo"][..]),
        ("cargo build 2>&1 | tail -20", &["tail", "-20"][..]),
        (
            "echo \"hello world\" > out.txt",
            &["echo", "hello world"][..],
        ),
        ("for f in *.rs; do echo \"$f\"; done", &["echo", "$f"][..]),
        ("[ -f x ] && echo yes", &["[", "-f", "x", "]"][..]),
        (
            "git commit -m \"fix: a (b)!\"",
            &["git", "commit", "-m", "fix: a (b)!"][..],
        ),
        ("awk '{print $1}' f", &["awk", "{print $1}", "f"][..]),
        ("test -n \"$HOME\" && echo ok", &["echo", "ok"][..]),
        ("echo $((1 + 2))", &["echo", "$((1 + 2))"][..]),
        (
            "\"$HOME/.cargo/bin/cargo\" build",
            &["$HOME/.cargo/bin/cargo", "build"][..],
        ),
        ("cd /tmp && ls", &["ls"][..]),
        (
            "python3 -c \"print(1)\"",
            &["python3", "-c", "print(1)"][..],
        ),
        (
            "find . -name '*.rs' -type f",
            &["find", ".", "-name", "*.rs", "-type", "f"][..],
        ),
        ("command -v curl", &["command", "-v", "curl"][..]),
        (
            "git log --format='%h %s' -n 5",
            &["git", "log", "--format=%h %s", "-n", "5"][..],
        ),
        ("X=1 Y=2 make test", &["make", "test"][..]),
        ("git diff -- src/ | head -n 50", &["head", "-n", "50"][..]),
    ] {
        assert_runs(source, expected);
    }
}

#[test]
fn test_powershell_is_approximated_without_losing_commands() {
    assert_runs_with("Remove-Item foo; rm -rf ~", EscapeChar::Backtick, RM);
    assert_runs_with("r`m -rf ~", EscapeChar::Backtick, RM);
    assert_runs_with("$x = rm -rf ~", EscapeChar::Backtick, RM);
    assert_runs_with(
        "Get-ChildItem -Path C:\\tmp",
        EscapeChar::Backtick,
        &["Get-ChildItem", "-Path", "C:\\tmp"],
    );
}

#[test]
fn test_adversarial_input_is_bounded() {
    // Deep nesting, brace bombs and oversized wrappers are reported unresolved instead of
    // recursing or enumerating without bound.
    let deep_parens = format!("{}rm -rf ~{}", "( ".repeat(10_000), " )".repeat(10_000));
    assert_unresolved(&deep_parens);
    // Unspaced, this is bash arithmetic; it only has to terminate.
    let deep_arithmetic = format!("{}rm -rf ~{}", "(".repeat(10_000), ")".repeat(10_000));
    let _ = analysis(&deep_arithmetic);
    let deep_substitution = format!("{}rm{}", "$(".repeat(10_000), ")".repeat(10_000));
    assert_unresolved(&deep_substitution);
    assert_unresolved(&"{a,b}".repeat(10_000));
    assert_unresolved("echo {a,b}{c,d}{e,f}{g,h}{i,j}{k,l}{m,n}{o,p}{q,r}");
    assert_unresolved(&format!("sudo --unknown {}", "x ".repeat(100)));
    let deep_eval = format!("{}rm -rf ~", "eval ".repeat(100));
    let result = analysis(&deep_eval);
    assert!(!result.is_fully_resolved() || contains_command(&result, RM));
}
