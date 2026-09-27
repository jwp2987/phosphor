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
            "python3 script.py --flag",
            &["python3", "script.py", "--flag"][..],
        ),
        (
            "python3 -m pytest -q",
            &["python3", "-m", "pytest", "-q"][..],
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

/// One of the commands `source` executes is exactly `expected`, whether or not the line is
/// also reported unresolved.
#[track_caller]
fn assert_finds(source: &str, escape_char: EscapeChar, expected: &[&str]) {
    let result = executed_commands(source, escape_char);
    assert!(
        contains_command(&result, expected),
        "{source:?} should be found to run {expected:?}; found {:?}",
        result.commands
    );
}

/// `spelling` is among the policy spellings of `source`.
#[track_caller]
fn assert_spelling(source: &str, escape_char: EscapeChar, spelling: &str) {
    let spellings = executed_commands(source, escape_char).policy_spellings();
    assert!(
        spellings.iter().any(|candidate| candidate == spelling),
        "{source:?} should be matched as {spelling:?}; spellings {spellings:?}"
    );
}

#[track_caller]
fn assert_unresolved_with(source: &str, escape_char: EscapeChar) {
    let result = executed_commands(source, escape_char);
    assert!(
        !result.is_fully_resolved(),
        "{source:?} must be reported unresolved; found {:?}",
        result.commands
    );
}

#[test]
fn test_same_line_aliases_and_functions_are_followed() {
    assert_runs("alias r=rm; r -rf ~", RM);
    assert_runs("alias r='rm -rf'\nr ~", RM);
    assert_runs("alias x='echo ok; rm -rf ~'; x", RM);
    assert_runs("f(){ rm -rf ~; }; f", RM);
    assert_runs("function f { rm -rf ~; }; f", RM);
    // A self-referencing alias terminates and stays resolved.
    assert_runs("alias ls='ls -la'; ls", &["ls", "-la"]);
}

#[test]
fn test_parameter_expansion_in_the_command_word_is_unresolved() {
    for source in [
        "${x:-rm} -rf ~",
        "${!x} -rf ~",
        "${x/y/rm} -rf ~",
        "\"${x:-rm}\" -rf ~",
    ] {
        assert_unresolved(source);
    }
}

#[test]
fn test_comments_continuations_and_heredoc_variants() {
    // `#` inside a word is not a comment.
    assert_runs("echo a#b; rm -rf ~", RM);
    // A comment is also read as commands, because zsh without `interactive_comments` runs
    // it -- additively, so the line stays resolved.
    assert_runs("echo hi # ; rm -rf ~", RM);
    assert_runs("echo hi #rm -rf ~", RM);
    // Line continuations, inside and between words.
    assert_runs("rm \\\n-rf ~", RM);
    assert_runs("r\\\nm -rf ~", RM);
    // Here-documents: unquoted delimiters expand substitutions, quoted ones do not.
    assert_runs("cat << EOF\n$(rm -rf ~)\nEOF", RM);
    assert_runs("cat <<EOF1 <<EOF2\nx\nEOF1\n`rm -rf ~`\nEOF2", RM);
    for source in [
        "cat <<-\"EOF\"\n\t$(rm -rf ~)\n\tEOF",
        "cat <<\\EOF\n$(rm -rf ~)\nEOF",
        "cat <<'E O F'\n$(rm -rf ~)\nE O F",
    ] {
        let result = analysis(source);
        assert!(
            !contains_command(&result, RM),
            "{source:?}: {:?}",
            result.commands
        );
        assert!(
            result.is_fully_resolved(),
            "{source:?}: {:?}",
            result.unresolved
        );
    }
}

#[test]
fn test_precommand_chains() {
    for source in [
        "coproc rm -rf ~",
        "coproc NAME { rm -rf ~; }",
        "exec rm -rf ~",
        "command -p rm -rf ~",
        "builtin exec rm -rf ~",
        "time nice -n 1 rm -rf ~",
        "! time rm -rf ~",
        "time ! rm -rf ~",
        "nice nohup env X=1 rm -rf ~",
        "env -S 'rm -rf ~'",
        "sudo -- env -- rm -rf ~",
    ] {
        assert_runs(source, RM);
    }
    assert_runs("xargs -I{} rm -rf {}", &["rm", "-rf", "{}"]);
}

#[test]
fn test_input_driven_commands_are_unresolved() {
    for source in [
        // xargs appends its input: with nothing (or a bare wrapper) to run, the input *is*
        // the command.
        "xargs env",
        "xargs sudo",
        "xargs xargs",
        "xargs sh -c",
        "xargs -I{} sh -c '{}'",
        "xargs -I{} {} -rf ~",
        "xargs -I CMD CMD -rf ~",
        // find substitutes each path for `{}`.
        r"find . -exec {} \;",
        // GNU parallel builds commands from its input.
        "parallel rm ::: a b",
        "parallel ::: 'rm -rf ~'",
    ] {
        assert_unresolved(source);
    }
    // The template is still offered as a candidate.
    assert_finds(
        "parallel rm -rf ::: ~",
        EscapeChar::Backslash,
        &["rm", "-rf"],
    );
}

#[test]
fn test_git_config_and_environment_commands_are_followed() {
    for source in [
        "git -c core.pager='rm -rf ~' log",
        "git -c alias.x='!rm -rf ~' x",
        "git -c credential.helper='!rm -rf ~' fetch",
        "git -c diff.bin.textconv='rm -rf ~' diff",
        "git -c core.sshCommand='rm -rf ~' fetch",
        "git config core.pager 'rm -rf ~'; git log",
        "git rebase -i --exec 'rm -rf ~' HEAD~3",
        "git rebase --exec='rm -rf ~' main",
        "git bisect run rm -rf ~",
        "git submodule foreach 'rm -rf ~'",
        "git submodule foreach --recursive rm -rf ~",
        "git filter-branch --tree-filter 'rm -rf ~' HEAD",
        "git difftool -x 'rm -rf ~'",
        "GIT_EXTERNAL_DIFF='rm -rf ~' git diff",
        "GIT_PAGER='rm -rf ~' git log",
        "PAGER='rm -rf ~'; git log",
        "export PAGER='rm -rf ~'; git log",
        "env GIT_SSH_COMMAND='rm -rf ~' git fetch",
        "EDITOR='rm -rf ~' git commit",
        "PROMPT_COMMAND='rm -rf ~'",
        "PS1='$(rm -rf ~)'",
    ] {
        assert_runs(source, RM);
    }
    for source in [
        "git -c include.path=/tmp/x log",
        "git -c core.hooksPath=/tmp/hooks commit -m x",
        "git --config-env=core.pager=PAGER_CMD log",
        "git -c core.pager=\"$X\" log",
        "LD_PRELOAD=/tmp/x.so ls",
        "BASH_ENV=/tmp/x bash script.sh",
        "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.pager GIT_CONFIG_VALUE_0=x git log",
        "export NODE_OPTIONS='--require /tmp/x.js'; node app.js",
    ] {
        assert_unresolved(source);
    }
    // Ordinary config and variables stay resolved.
    assert_runs(
        "git -c color.ui=always log -n 5",
        &["git", "-c", "color.ui=always", "log", "-n", "5"],
    );
    assert_runs(
        "git -c user.name=x commit -m y",
        &["git", "-c", "user.name=x", "commit", "-m", "y"],
    );
    assert_runs("EDITOR=vim git commit", &["vim"]);
}

#[test]
fn test_zsh_and_fish_forms() {
    // zsh `=cmd` expands to the command's path.
    assert_spelling("=rm -rf ~", EscapeChar::Backslash, "rm -rf ~");
    // zsh `;|` case terminator.
    assert_runs("case x in x) echo;| y) rm -rf ~;; esac", RM);
    for source in [
        "- rm -rf ~",
        "repeat 3 rm -rf ~",
        "nocorrect rm -rf ~",
        "noglob rm -rf ~",
    ] {
        assert_runs(source, RM);
    }
    // fish: `and`/`or`/`not` are peeled; blocks and `( … )` substitution are found, and the
    // POSIX reading reports them unresolved rather than hiding them.
    for source in ["true; and rm -rf ~", "false; or rm -rf ~", "not rm -rf ~"] {
        assert_runs(source, RM);
    }
    for source in [
        "echo (rm -rf ~)",
        "begin; rm -rf ~; end",
        "if true; rm -rf ~; end",
        "for x in a; rm -rf ~; end",
        "while true; rm -rf ~; end",
    ] {
        assert_finds(source, EscapeChar::Backslash, RM);
    }
}

#[test]
fn test_powershell_commands_are_found_or_fail_closed() {
    let ps = EscapeChar::Backtick;
    // Found and resolved.
    assert_runs_with("& 'rm' -rf ~", ps, RM);
    assert_runs_with("& rm -rf ~", ps, RM);
    assert_runs_with("iex 'rm -rf ~'", ps, RM);
    assert_runs_with("Invoke-Expression 'rm -rf ~'", ps, RM);
    assert_runs_with(
        "Start-Process rm -ArgumentList x",
        ps,
        &["rm", "-ArgumentList", "x"],
    );
    assert_runs_with("Start-Process -FilePath rm", ps, &["rm", "-FilePath"]);
    // Aliases and case-insensitive names are matched under every name.
    assert_spelling("ri -Recurse ~", ps, "rm -Recurse ~");
    assert_spelling("del x", ps, "rm x");
    assert_spelling("Remove-Item x", ps, "rm x");
    assert_spelling("RM -rf ~", ps, "rm -rf ~");
    assert_spelling("& 'C:\\tools\\rm.exe' -rf ~", ps, "rm -rf ~");
    // Constructs the grammar here cannot follow are unresolved, never hidden.
    for source in [
        "{ rm -rf ~ }.Invoke()",
        "Get-ChildItem | ForEach-Object { Remove-Item $_ }",
        "Invoke-Command -ScriptBlock { rm -rf ~ }",
        "@(rm -rf ~)",
        "[IO.File]::Delete('x')",
        "$f.Delete()",
        "Remove-Item(\"x\")",
        "Set-Alias x rm; x -rf ~",
        "& $cmd -rf ~",
        "& (Get-Command rm) -rf ~",
        ". $script",
        "pwsh -EncodedCommand abc",
        "powershell -enc abc",
        "pwsh -c '{ rm -rf ~ }.Invoke()'",
        "@'\nrm -rf ~\n'@",
        "<# c #> Get-Date",
    ] {
        assert_unresolved_with(source, ps);
    }
    // Ordinary PowerShell stays resolved.
    for (source, expected) in [
        (
            "Get-ChildItem -Path C:\\tmp | Select-Object Name",
            &["Select-Object", "Name"][..],
        ),
        ("$x = Get-Date", &["Get-Date"][..]),
        ("git status; npm install", &["npm", "install"][..]),
        ("Write-Output (Get-Date)", &["Get-Date"][..]),
        (
            "Test-Path x -PathType Leaf",
            &["Test-Path", "x", "-PathType", "Leaf"][..],
        ),
        ("$env:PATH", &["$env:PATH"][..]),
    ] {
        assert_runs_with(source, ps, expected);
    }
}

#[test]
fn test_unicode_and_unusual_whitespace() {
    assert_unresolved("r\u{200b}m -rf ~");
    assert_unresolved("r\u{43c} -rf ~");
    assert_unresolved("\u{202e}mr -rf ~");
    // Non-ASCII arguments are fine.
    assert_runs("echo h\u{e9}llo", &["echo", "h\u{e9}llo"]);
    // Tabs and carriage returns separate words (bash would keep `\r` in the word, which can
    // only make this over-report).
    assert_runs("rm\t-rf\t~", RM);
    assert_runs("rm -rf ~\r", RM);
}

#[test]
fn test_caps_fail_closed() {
    assert_unresolved(&"echo a; ".repeat(10_000));
    assert_unresolved(&"a;".repeat(5_000));
    assert_unresolved(&format!("echo {}", "{a,b}".repeat(40)));
    assert_unresolved(&format!("{}rm{}", "$(".repeat(100), ")".repeat(100)));
    assert_unresolved(&format!("{}rm -rf ~", "eval ".repeat(40)));
}

#[test]
fn test_case_insensitive_names_are_matched() {
    let posix = EscapeChar::Backslash;
    assert_spelling("RM -rf ~", posix, "rm -rf ~");
    assert_spelling("/BIN/RM -rf ~", posix, "rm -rf ~");
    assert_spelling("Rm.EXE -rf ~", posix, "rm -rf ~");
    assert_runs("SUDO rm -rf ~", RM);
    assert_runs("Env rm -rf ~", RM);
}

#[test]
fn test_code_given_to_interpreters_is_unresolved() {
    for source in [
        "python3 -c 'import os'",
        "python -Bc 'x'",
        "python3 - <<EOF\nprint(1)\nEOF",
        "echo x | python3",
        "perl -e 'system(\"rm -rf ~\")'",
        "perl -ne 'print'",
        "ruby -e 'x'",
        "node -e 'x'",
        "node --eval=x",
        "node -p 1",
        "deno eval 'x'",
        "php -r 'x'",
        "lua -e x",
        "osascript -e x",
        "awk 'BEGIN{system(\"rm -rf ~\")}'",
        "awk '{print | \"sh\"}' f",
        "sed 's/x/rm -rf ~/e' f",
        "sed '1e rm -rf ~' f",
        "sed -e 'e rm -rf ~'",
        "sed '/x/e rm -rf ~' f",
        "sed 's/a;b/c/e' f",
        "bash",
        "sh -s",
        "bash -c",
        "echo 'rm -rf ~' | sh",
        "vim -c '!rm -rf ~'",
        "gdb -ex 'shell rm -rf ~'",
        "fc -s",
        "hash -p /bin/rm ls; ls -rf ~",
    ] {
        assert_unresolved(source);
    }
    for (source, expected) in [
        ("python3 --version", &["python3", "--version"][..]),
        ("node server.js", &["node", "server.js"][..]),
        ("perl script.pl", &["perl", "script.pl"][..]),
        (
            "awk -F: '{print $1}' /etc/passwd",
            &["awk", "-F:", "{print $1}", "/etc/passwd"][..],
        ),
        ("sed -n '1,5p' f", &["sed", "-n", "1,5p", "f"][..]),
        ("sed -i 's/a/b/g' f", &["sed", "-i", "s/a/b/g", "f"][..]),
        ("sed 's/e/E/' f", &["sed", "s/e/E/", "f"][..]),
        ("sed '/^$/d' f", &["sed", "/^$/d", "f"][..]),
        (
            "sed 's/x/y/w /tmp/new' f",
            &["sed", "s/x/y/w /tmp/new", "f"][..],
        ),
        ("bash script.sh", &["bash", "script.sh"][..]),
        ("bash --version", &["bash", "--version"][..]),
    ] {
        assert_runs(source, expected);
    }
}
