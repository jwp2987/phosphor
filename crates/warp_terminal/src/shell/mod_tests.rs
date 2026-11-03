use super::*;

#[test]
fn test_parse_history_bash() {
    let history_lines = "cat ~/.bash_history
#1618089175
ls
#1618089176
pwd";
    assert_eq!(
        ShellType::Bash.parse_history(history_lines.as_bytes()),
        vec![
            "cat ~/.bash_history".to_string(),
            "ls".to_string(),
            "pwd".to_string()
        ]
    );
}

#[test]
fn test_strip_zsh_extended_prefix() {
    // Extended history format.
    assert_eq!(
        strip_zsh_extended_prefix(": 1699251735:0;an extended history command"),
        "an extended history command"
    );

    // Non-extended format (no prefix to strip).
    assert_eq!(
        strip_zsh_extended_prefix("cat ~/.zsh_history"),
        "cat ~/.zsh_history"
    );

    // Edge cases that should NOT be stripped.
    assert_eq!(
        strip_zsh_extended_prefix(": not_a_timestamp"),
        ": not_a_timestamp"
    );
    assert_eq!(strip_zsh_extended_prefix(": 123;"), ": 123;"); // Missing second number.
    assert_eq!(strip_zsh_extended_prefix(": :0;cmd"), ": :0;cmd"); // Empty timestamp.
    assert_eq!(strip_zsh_extended_prefix(": 123:;cmd"), ": 123:;cmd"); // Empty elapsed.
}

#[test]
fn test_parse_history_zsh() {
    let history_lines = "
cat ~/.zsh_history
a multi-line\\
command
: 1699251735:0;an extended history command
: 1699251735:0;a multi-line extended\\
history command
";
    assert_eq!(
        ShellType::Zsh.parse_history(history_lines.as_bytes()),
        vec![
            "cat ~/.zsh_history".to_string(),
            "a multi-line\ncommand".to_string(),
            "an extended history command".to_string(),
            "a multi-line extended\nhistory command".to_string(),
        ]
    );
}

#[test]
fn test_parse_history_zsh_continuation_line_looks_like_prefix() {
    // Regression test: a continuation line that happens to match the extended history
    // prefix pattern should NOT be stripped.
    let history_lines = ": 1699251735:0;echo '\\
: 9999:0;fake prefix'";
    assert_eq!(
        ShellType::Zsh.parse_history(history_lines.as_bytes()),
        vec!["echo '\n: 9999:0;fake prefix'".to_string()]
    );
}

#[test]
fn test_zsh_unmetafy() {
    let test_zsh_history = [
        227, 129, 131, 179, 227, 130, 131, 172, 227, 129, 175, 230, 131, 183, 165, 230, 131, 188,
        172, 232, 170, 131, 190, 227, 129, 167, 227, 129, 131, 185,
    ];
    let unmetafied_test = zsh_unmetafy(&test_zsh_history);
    assert_eq!("これは日本語です", &unmetafied_test);
}

#[test]
fn test_fish_unescape_history_yaml() {
    assert_eq!(fish_unescape_history_yaml("foo"), "foo");
    assert_eq!(fish_unescape_history_yaml("foo\\nbar"), "foo\nbar");
    assert_eq!(fish_unescape_history_yaml("foo\\\\"), "foo\\");
    assert_eq!(fish_unescape_history_yaml("foo\\"), "foo"); // trailing escape dropped
}

#[test]
fn test_from_name() {
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("bash"));
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("-bash"));
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("/bin/bash"));
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("/usr/bin/bash"));
    assert_eq!(Some(ShellType::Zsh), ShellType::from_name("/bin/zsh"));
    assert_eq!(None, ShellType::from_name("/bin/zsh/foo"));
    assert_eq!(None, ShellType::from_name("/bin/zsh/-bash"));
    assert_eq!(None, ShellType::from_name("rezsh"));
    assert_eq!(Some(ShellType::Fish), ShellType::from_name("fish"));
    assert_eq!(
        Some(ShellType::Fish),
        ShellType::from_name("/usr/local/bin/fish")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_name("pwsh.exe")
    );
    assert_eq!(None, ShellType::from_name("pwsh.bat"));
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_name("/usr/bin/env/powershell")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_name("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
    );
    assert_eq!(None, ShellType::from_name("psh"));
}

#[test]
fn test_from_markdown_language_spec() {
    // Standard shell languages
    assert_eq!(
        Some(ShellType::Bash),
        ShellType::from_markdown_language_spec("bash")
    );
    assert_eq!(
        Some(ShellType::Bash),
        ShellType::from_markdown_language_spec("shell")
    );
    assert_eq!(
        Some(ShellType::Bash),
        ShellType::from_markdown_language_spec("sh")
    );
    assert_eq!(
        Some(ShellType::Zsh),
        ShellType::from_markdown_language_spec("zsh")
    );
    assert_eq!(
        Some(ShellType::Fish),
        ShellType::from_markdown_language_spec("fish")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_markdown_language_spec("powershell")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_markdown_language_spec("pwsh")
    );

    // Non-shell languages and invalid inputs
    assert_eq!(None, ShellType::from_markdown_language_spec("python"));
    assert_eq!(None, ShellType::from_markdown_language_spec("rust"));
    assert_eq!(None, ShellType::from_markdown_language_spec(""));
    // Paths and executable names should not match (use from_name for those)
    assert_eq!(None, ShellType::from_markdown_language_spec("/bin/bash"));
    assert_eq!(None, ShellType::from_markdown_language_spec("-bash"));
}

#[test]
fn test_fish_parse_abbrs() {
    let raw_abbrs = "abbr -a -U -- gco 'git checkout'
abbr -a -g -- gq 'git commit'
abbr -a -U -- ehw 'echo \"Hello, world\"'
abbr -a -- ga 'git add' # imported from a universal variable, see `help abbr`";
    let abbrs = ShellType::Fish.abbreviations(raw_abbrs);

    assert_eq!(abbrs.len(), 4);
    assert_eq!(abbrs.get("gco").unwrap(), "git checkout");
    assert_eq!(abbrs.get("gq").unwrap(), "git commit");
    assert_eq!(abbrs.get("ehw").unwrap(), r#"echo "Hello, world""#);
    assert_eq!(abbrs.get("ga").unwrap(), "git add");
}

#[test]
fn test_fish_parse_aliases() {
    let raw_aliases = "alias g git
alias rmi 'rm -i'
alias ehw 'echo \"Hello, world\"'";
    let aliases = ShellType::Fish.aliases(raw_aliases);

    assert_eq!(aliases.len(), 3);
    assert_eq!(aliases.get("g").unwrap(), "git");
    assert_eq!(aliases.get("rmi").unwrap(), "rm -i");
    assert_eq!(aliases.get("ehw").unwrap(), r#"echo "Hello, world""#);
}

#[test]
fn test_should_add_command_to_history() {
    {
        // Test zsh's "histignorespace" option.
        let options = HashSet::from(["histignorespace".to_string()]);
        let shell = Shell::new(
            ShellType::Zsh,
            None,
            Some(options),
            Default::default(),
            None,
        );

        assert!(shell.should_add_command_to_history("asdf"));
        assert!(!shell.should_add_command_to_history(" asdf"));

        let shell = Shell::new(ShellType::Zsh, None, None, Default::default(), None);
        assert!(shell.should_add_command_to_history("asdf"));
        assert!(shell.should_add_command_to_history(" asdf"));
    }

    // Test our "!histcontrol_" faked option for bash.
    {
        for variant in [
            "!histcontrol_ignorespace",
            "!histcontrol_ignoreboth",
            "!histcontrol_testing:ignorespace",
            "!histcontrol_testing:ignoreboth:testing",
        ] {
            let options = HashSet::from([variant.to_string()]);
            let bash_shell = Shell::new(
                ShellType::Bash,
                None,
                Some(options.clone()),
                Default::default(),
                None,
            );

            assert!(bash_shell.should_add_command_to_history("asdf"));
            assert!(!bash_shell.should_add_command_to_history(" asdf"));

            // Make sure that option only takes effect when the shell is bash.
            let zsh_shell = Shell::new(
                ShellType::Zsh,
                None,
                Some(options),
                Default::default(),
                None,
            );
            assert!(zsh_shell.should_add_command_to_history(" asdf"));

            let bash_shell_no_options =
                Shell::new(ShellType::Bash, None, None, Default::default(), None);
            assert!(bash_shell_no_options.should_add_command_to_history("asdf"));
            assert!(bash_shell_no_options.should_add_command_to_history(" asdf"));
        }

        // Ensure we're not only looking for "!histcontrol".
        {
            let options = HashSet::from(["!histcontrol_testing".to_string()]);
            let bash_shell = Shell::new(
                ShellType::Bash,
                None,
                Some(options),
                Default::default(),
                None,
            );
            assert!(bash_shell.should_add_command_to_history(" asdf"));
        }
    }

    // Fish has no shell options that prevent a command from being written to history.
    {
        let fish_shell = Shell::new(ShellType::Fish, None, None, Default::default(), None);
        assert!(fish_shell.should_add_command_to_history("asdf"));
        assert!(fish_shell.should_add_command_to_history(" asdf"));
    }
}

/// Regression test for https://github.com/warpdotdev/warp/issues/10474.
///
/// `rc_file_paths` is rendered into a shell command that runs on the *target*
/// (e.g. an SSH remote, or a subshell during Auto-Warpify). The path separator
/// must therefore depend on the target OS, not the host that built Warp.
/// Previously this used `PathBuf::join`, which uses the host's separator and
/// produced strings like `~\.zshrc` on a Windows host targeting a Unix shell —
/// the remote zsh then errored with "no such user or named directory".
/// `rc_file_paths` now returns `TypedPathBuf`s encoded for the target OS, so the
/// separator is enforced by the encoding rather than the host.
#[test]
fn test_rc_file_paths_use_target_os_separator() {
    for os in [TargetOS::Linux, TargetOS::MacOS] {
        assert_eq!(
            ShellType::Zsh.rc_file_paths(os.clone()),
            vec![TypedPathBuf::from_unix("~/.zshrc")],
            "Zsh rc path on {os:?} should use forward slash regardless of host",
        );
        assert_eq!(
            ShellType::Bash.rc_file_paths(os.clone()),
            vec![TypedPathBuf::from_unix("~/.bashrc")],
        );
        assert_eq!(
            ShellType::Fish.rc_file_paths(os.clone()),
            vec![TypedPathBuf::from_unix("~/.config/fish/config.fish")],
        );
        // pwsh on Unix reads the XDG profile, not the Windows `Documents` one (#804).
        assert_eq!(
            ShellType::PowerShell.rc_file_paths(os),
            vec![TypedPathBuf::from_unix(
                "~/.config/powershell/Microsoft.PowerShell_profile.ps1"
            )],
        );
    }

    // On Windows the only Auto-Warpify-supported shell is PowerShell; Unix
    // shells deliberately return no rc paths. Both editions' profiles are
    // written (#804). The leading separator follows target OS; the literal
    // interior slashes are left as-is (PowerShell accepts forward slashes on
    // Windows).
    assert_eq!(
        ShellType::PowerShell.rc_file_paths(TargetOS::Windows),
        vec![
            TypedPathBuf::from_windows(
                "$HOME\\Documents/PowerShell/Microsoft.PowerShell_profile.ps1"
            ),
            TypedPathBuf::from_windows(
                "$HOME\\Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"
            ),
        ],
    );
    assert!(ShellType::Zsh.rc_file_paths(TargetOS::Windows).is_empty());
    assert!(ShellType::Bash.rc_file_paths(TargetOS::Windows).is_empty());
    assert!(ShellType::Fish.rc_file_paths(TargetOS::Windows).is_empty());
}

// The deferred function/builtin enumeration commands (#586). These are
// per-shell rather than a constant because only one shell needs them: Bash,
// Zsh and Fish already emit their complete function and builtin lists during
// bootstrap, so asking again would cost an in-band round trip and return
// nothing new.
#[test]
fn only_powershell_enumerates_functions_and_builtins_after_bootstrap() {
    for shell_type in [ShellType::Bash, ShellType::Zsh, ShellType::Fish] {
        assert_eq!(shell_type.shell_command_to_get_all_functions(), None);
        assert_eq!(shell_type.shell_command_to_get_all_builtins(), None);
    }

    let functions = ShellType::PowerShell
        .shell_command_to_get_all_functions()
        .expect("PowerShell's bootstrap cannot afford the full function list");
    let builtins = ShellType::PowerShell
        .shell_command_to_get_all_builtins()
        .expect("PowerShell's bootstrap cannot afford the full cmdlet list");

    assert!(functions.contains("Get-Command -CommandType Function"));
    assert!(builtins.contains("Get-Command -CommandType Cmdlet"));

    // The shell integration's own helpers are still declared `Warp-*` in
    // `pwsh.ps1`, so the filter has to keep matching that prefix; rebranding it
    // would leak `Warp-Precmd` and friends into the user's completions.
    assert!(functions.contains("$_.Name.StartsWith('Warp')"));
    assert!(!builtins.contains("StartsWith"));

    // Both write raw UTF-8 bytes to stdout rather than letting PowerShell's
    // formatter wrap or pad the object list.
    for command in [functions, builtins] {
        assert!(command.contains("[System.Text.UTF8Encoding]::new($false).GetBytes($text)"));
        assert!(command.contains("[Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)"));
    }
}

// `ebedb9fd`: executable discovery on a localized Windows install. PowerShell
// encodes its stdout with the machine's active code page, so on a GBK (or any
// other non-UTF-8) system the executable names come back as bytes
// `CommandOutput::to_string` — which is `String::from_utf8`, not a lossy decode
// — refuses outright. `executables_from_shell_command_output` then returns an
// empty Vec and every command in the input stays underlined as unknown for the
// rest of the session. The fix is at the emitter: have PowerShell write the
// joined names as explicit UTF-8 bytes, exactly as the deferred function and
// cmdlet enumerations already do.
#[test]
fn powershell_executable_enumeration_writes_utf8_not_the_active_code_page() {
    let command = ShellType::PowerShell.shell_command_to_get_executables();

    assert!(command.contains("Get-Command -CommandType Application"));
    assert!(command.contains("[System.Text.UTF8Encoding]::new($false).GetBytes($text)"));
    assert!(command.contains("[Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)"));

    // The bare `Get-Command … | Select-Object -ExpandProperty Name` pipeline is
    // the pre-fix form: its output goes through PowerShell's own encoder.
    let pre_fix_form = "Select-Object -ExpandProperty Name";
    assert!(!command.trim_end().ends_with(pre_fix_form));

    // Only PowerShell needs this; the Unix shells emit UTF-8 already.
    for shell_type in [ShellType::Bash, ShellType::Zsh, ShellType::Fish] {
        assert!(
            !shell_type
                .shell_command_to_get_executables()
                .contains("UTF8Encoding")
        );
    }
}

#[test]
fn test_nu_from_name_and_markdown_spec() {
    for name in [
        "nu",
        "-nu",
        "nushell",
        "/usr/bin/nu",
        "/opt/homebrew/bin/nu",
        "nu.exe",
        "/c/Program Files/nu/nu.exe",
    ] {
        assert_eq!(Some(ShellType::Nu), ShellType::from_name(name), "{name}");
    }
    // Names that merely contain "nu" are not nushell.
    for name in [
        "menu",
        "/usr/bin/gnu",
        "nul",
        "nu.sh",
        "nushell-config",
        "/bin/nu/foo",
    ] {
        assert_eq!(None, ShellType::from_name(name), "{name}");
    }
    assert_eq!(
        Some(ShellType::Nu),
        ShellType::from_markdown_language_spec("nu")
    );
    assert_eq!(
        Some(ShellType::Nu),
        ShellType::from_markdown_language_spec("nushell")
    );
    assert_eq!("nu", ShellType::Nu.name());
    // The name round-trips, which `StartupShell` and the InitShell hook rely on.
    assert_eq!(
        Some(ShellType::Nu),
        ShellType::from_name(ShellType::Nu.name())
    );
}

#[test]
fn test_nu_parse_history_decodes_multi_line_entries() {
    // reedline's plaintext history writes the newlines inside one entry as `<\n>`.
    let history = "ls\nif true {<\\n>    echo hi<\\n>}\n\n   \nls | where size > 1mb\n";
    assert_eq!(
        ShellType::Nu.parse_history(history.as_bytes()),
        vec![
            "ls".to_string(),
            "if true {\n    echo hi\n}".to_string(),
            "ls | where size > 1mb".to_string(),
        ]
    );
    assert_eq!(
        ShellType::Nu.history_files()[0],
        "~/.config/nushell/history.txt"
    );
}

#[test]
fn test_nu_parse_aliases_keeps_the_expansion_verbatim() {
    let aliases = ShellType::Nu.aliases(
        "ll -> ls -l\ngs -> git status --short\nq -> echo \"a b\" 'c'\nbroken\nempty -> \n",
    );
    assert_eq!(aliases.get("ll").map(String::as_str), Some("ls -l"));
    assert_eq!(
        aliases.get("gs").map(String::as_str),
        Some("git status --short")
    );
    // Quotes and backslashes are not shell-escaped by nushell and must survive.
    assert_eq!(
        aliases.get("q").map(String::as_str),
        Some("echo \"a b\" 'c'")
    );
    assert_eq!(aliases.len(), 3);
}

#[test]
fn test_nu_terminal_bytes_and_capabilities() {
    // Ctrl-P, bound to `edit: clear` by nu.nu.
    assert_eq!(
        ShellType::Nu.kill_buffer_bytes(),
        &[escape_sequences::C0::DLE][..]
    );
    assert_eq!(ShellType::Nu.execute_command_bytes(), b"\r");
    assert!(!ShellType::Nu.is_fully_supported_remotely());
    assert_eq!(ShellType::Nu.shell_command_to_get_all_functions(), None);
    assert_eq!(ShellType::Nu.shell_command_to_get_all_builtins(), None);

    let shell = Shell::new(ShellType::Nu, None, None, Default::default(), None);
    assert!(shell.supports_autocd());
    assert_eq!(
        shell.input_reporting_sequence(),
        Some([escape_sequences::C0::ESC, b'i'])
    );
}

#[test]
fn test_nu_should_add_command_to_history_follows_ignore_space_prefixed() {
    let ignoring = Shell::new(
        ShellType::Nu,
        None,
        Some(HashSet::from(["ignore_space_prefixed".to_string()])),
        Default::default(),
        None,
    );
    assert!(ignoring.should_add_command_to_history("ls"));
    assert!(!ignoring.should_add_command_to_history(" ls"));

    let recording = Shell::new(
        ShellType::Nu,
        None,
        Some(HashSet::new()),
        Default::default(),
        None,
    );
    assert!(recording.should_add_command_to_history(" ls"));
    let unknown = Shell::new(ShellType::Nu, None, None, Default::default(), None);
    assert!(unknown.should_add_command_to_history(" ls"));
}

#[test]
fn test_nu_commands_phosphor_runs_for_itself_are_bash() {
    assert_eq!(ShellType::Nu.executor_syntax(), ShellType::Bash);
    for shell_type in [
        ShellType::Bash,
        ShellType::Zsh,
        ShellType::Fish,
        ShellType::PowerShell,
    ] {
        assert_eq!(shell_type.executor_syntax(), shell_type);
    }
    // POSIX `sh` syntax only: no nushell constructs, and it must run under `sh` itself.
    let command = ShellType::Nu.shell_command_to_get_executables();
    assert!(command.starts_with("IFS=:; for d in $PATH"));
    assert!(!command.contains("$env"));
    let output = CommandOutput {
        status: CommandExitStatus::Success,
        stdout: b"ls\ngit\nnu\n".to_vec(),
        stderr: vec![],
        exit_code: None,
    };
    assert_eq!(
        ShellType::Nu.executables_from_shell_command_output(Ok(output), false),
        vec![SmolStr::from("ls"), "git".into(), "nu".into()]
    );
}

#[test]
fn test_nu_dialect_family_and_rc_file() {
    assert_eq!(
        warp_util::path::ShellDialect::from(ShellType::Nu),
        warp_util::path::ShellDialect::Nu
    );
    assert_eq!(
        warp_util::path::ShellFamily::from(ShellType::Nu),
        warp_util::path::ShellFamily::Posix
    );
    let rc = ShellType::Nu.rc_file_paths(TargetOS::Linux);
    assert_eq!(rc.len(), 1);
    assert_eq!(rc[0].to_str(), Some("~/.config/nushell/config.nu"));
    assert!(ShellType::Nu.rc_file_paths(TargetOS::Windows).is_empty());
}

/// The executables listing must work in a plain POSIX `sh` (it is run through `bash -c` / `sh -c`
/// for a nushell session) and list only executable regular files, once per directory entry.
#[cfg(unix)]
#[test]
fn test_nu_executables_command_runs_in_sh_and_lists_executables() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = std::env::temp_dir().join(format!("phosphor-nu-exes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin one")).unwrap();
    std::fs::create_dir_all(dir.join("bin2/subdir")).unwrap();
    for (path, mode) in [
        ("bin one/tool a", 0o755),
        ("bin one/plain", 0o644),
        ("bin2/tool-b", 0o755),
    ] {
        std::fs::write(dir.join(path), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(dir.join(path), std::fs::Permissions::from_mode(mode)).unwrap();
    }
    let path = format!(
        "{}:{}:{}",
        dir.join("bin one").display(),
        dir.join("bin2").display(),
        dir.join("missing").display()
    );
    // Absolute path: `PATH` is overridden below, and `Command` resolves the program against the
    // child's `PATH`, so a bare "sh" would fail with NotFound.
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(ShellType::Nu.shell_command_to_get_executables())
        .env("PATH", path)
        .output()
        .unwrap();
    let mut names: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    names.sort();
    assert_eq!(names, vec!["tool a".to_string(), "tool-b".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// #1203: PSReadLine's ConsoleHost_history.txt (pwsh 7 and Windows PowerShell 5.1) joins the
/// lines of a multi-line command with a trailing backtick; the bootstrap's own lines are not the
/// user's history.
#[test]
fn test_parse_history_powershell_joins_backtick_continuations_and_drops_bootstrap_lines() {
    let history_lines = "Get-ChildItem | Where-Object Length -gt 1kb\r\n\
if (-not $global:WARP_BOOTSTRAPPED) { $global:_warpPrevHistoryHandler = 1 } # apikey\r\n\
. 'C:\\Users\\me\\AppData\\Local\\Temp\\.tmpAb12Cd.ps1'\r\n\
. /home/me/.local/share/Phosphor/pwsh.ps1\r\n\
Get-Process |`\r\n  Sort-Object CPU -Descending |`\r\n  Select-Object -First 5\r\n\
\r\n\
git status`";
    assert_eq!(
        ShellType::PowerShell.parse_history(history_lines.as_bytes()),
        vec![
            "Get-ChildItem | Where-Object Length -gt 1kb".to_string(),
            "Get-Process |\n  Sort-Object CPU -Descending |\n  Select-Object -First 5".to_string(),
            "git status\n".to_string(),
        ]
    );
    // A user's own `. ./build.ps1` is history; only the bootstrap's spellings are dropped.
    assert_eq!(
        ShellType::PowerShell.parse_history(b". ./build.ps1\n. C:\\x\\.tmpAb12Cd.ps1\n"),
        vec![". ./build.ps1".to_string()]
    );
}
