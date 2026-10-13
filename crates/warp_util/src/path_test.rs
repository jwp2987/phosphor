use super::*;

#[test]
fn test_user_friendly_path_with_home() {
    let home = "/Users/blue";
    assert_eq!(
        user_friendly_path("/Users/blue", Some(home)),
        "~".to_string(),
    );
    assert_eq!(
        user_friendly_path("/Users/blue/warp", Some(home)),
        "~/warp".to_string(),
    );
    assert_eq!(
        user_friendly_path("/Users/admin/warp", Some(home)),
        "/Users/admin/warp".to_string(),
    );
}

#[test]
fn test_to_relative_path() {
    use super::to_relative_path;
    use std::path::Path;

    // Basic relative path conversion
    #[cfg(not(windows))]
    {
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/projects/app/src/main.rs"),
                Path::new("/Users/john/projects")
            ),
            Some("app/src/main.rs".to_string())
        );

        // Same directory
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/projects"),
                Path::new("/Users/john/projects")
            ),
            Some(".".to_string())
        );

        // Parent directory
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john"),
                Path::new("/Users/john/projects")
            ),
            Some("..".to_string())
        );

        // Nested parent
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users"),
                Path::new("/Users/john/projects")
            ),
            Some("../..".to_string())
        );

        // Cross-branch paths
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/documents/file.txt"),
                Path::new("/Users/john/projects")
            ),
            Some("../documents/file.txt".to_string())
        );

        // Root to subdirectory
        assert_eq!(
            to_relative_path(false, Path::new("/var/log/system.log"), Path::new("/")),
            Some("var/log/system.log".to_string())
        );

        // Handles paths that would have leading slashes correctly
        assert_eq!(
            to_relative_path(false, Path::new("/home/user/file.txt"), Path::new("/home")),
            Some("user/file.txt".to_string())
        );

        // Test with current directory references that should be cleaned
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/projects/./app/src/main.rs"),
                Path::new("/Users/john/projects")
            ),
            Some("app/src/main.rs".to_string()),
        );
    }

    #[cfg(windows)]
    {
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/projects/app/src/main.rs"),
                Path::new("/Users/john/projects")
            ),
            Some("app\\src\\main.rs".to_string())
        );

        // Same directory
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/projects"),
                Path::new("/Users/john/projects")
            ),
            Some(".".to_string())
        );

        // Parent directory
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john"),
                Path::new("/Users/john/projects")
            ),
            Some("..".to_string())
        );

        // Nested parent
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users"),
                Path::new("/Users/john/projects")
            ),
            Some("..\\..".to_string())
        );

        // Cross-branch paths
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/documents/file.txt"),
                Path::new("/Users/john/projects")
            ),
            Some("..\\documents\\file.txt".to_string())
        );

        // Root to subdirectory
        assert_eq!(
            to_relative_path(false, Path::new("/var/log/system.log"), Path::new("/")),
            Some("var\\log\\system.log".to_string())
        );

        // Handles paths that would have leading slashes correctly
        assert_eq!(
            to_relative_path(false, Path::new("/home/user/file.txt"), Path::new("/home")),
            Some("user\\file.txt".to_string())
        );

        // Test with current directory references that should be cleaned
        assert_eq!(
            to_relative_path(
                false,
                Path::new("/Users/john/projects/./app/src/main.rs"),
                Path::new("/Users/john/projects")
            ),
            Some("app\\src\\main.rs".to_string())
        );

        // Windows paths - different drives should return None
        assert_eq!(
            to_relative_path(
                /* is_wsl */ false,
                Path::new("D:\\projects\\app"),
                Path::new("C:\\workspace")
            ),
            None,
        );

        // Windows paths - same drive
        assert_eq!(
            to_relative_path(
                /* is_wsl */ false,
                Path::new("C:\\projects\\app\\src\\main.rs"),
                Path::new("C:\\projects")
            ),
            Some("app\\src\\main.rs".to_string())
        );

        // Windows paths - same drive -- WSL is disabled for now
        assert_eq!(
            to_relative_path(
                /* is_wsl */ true,
                Path::new("C:\\projects\\app\\src\\main.rs"),
                Path::new("C:\\projects")
            ),
            None
        );
    }
}

#[test]
fn test_normalize_relative_path_for_glob() {
    use std::path::Path;

    assert_eq!(
        normalize_relative_path_for_glob(Path::new("app/src/main.rs")),
        "app/src/main.rs"
    );
    assert_eq!(
        normalize_relative_path_for_glob(Path::new("./app/src/main.rs")),
        "app/src/main.rs"
    );
    assert_eq!(
        normalize_relative_path_for_glob(Path::new("../app/src/main.rs")),
        "app/src/main.rs"
    );
    assert_eq!(normalize_relative_path_for_glob(Path::new("..")), "");
    assert_eq!(normalize_relative_path_for_glob(Path::new("")), "");
}

#[test]
fn test_posix_escape() {
    let shell_family = ShellFamily::Posix;
    assert_eq!(
        shell_family.escape("~/test_dir/library% 1$2"),
        "\\~/test_dir/library%\\ 1\\$2"
    );
    assert_eq!(shell_family.escape("あい"), "あい");
    // #867: this used to be `abc\ \<LF>\ \<TAB>`. A backslash-newline is a line continuation,
    // so the newline was dropped (wrong file), and a raw control character in the buffer is
    // acted on by the PTY before quoting is parsed. Control characters now force ANSI-C quoting.
    assert_eq!(shell_family.escape("abc \n \t"), r"$'abc \x0A \x09'");
    assert_eq!(shell_family.escape(""), "''");
    assert_eq!(
        shell_family.escape("foo '\"' bar"),
        "foo\\ \\'\\\"\\'\\ bar"
    );
}

#[test]
fn test_powershell_escape() {
    let shell_family = ShellFamily::PowerShell;
    assert_eq!(
        shell_family.escape("~/test_dir/library% 1$2"),
        "~/test_dir/library%` 1`$2"
    );
    assert_eq!(shell_family.escape("あい"), "あい");
    // #867: was "abc` `<LF>` `<TAB>", raw control characters in the buffer.
    assert_eq!(
        shell_family.escape("abc \n \t"),
        r#""abc $([char]0x000A) $([char]0x0009)""#
    );
    assert_eq!(shell_family.escape(""), "''");
    assert_eq!(shell_family.escape("foo '\"' bar"), "foo` `'`\"`'` bar");
}

/// File names that defeated the pre-#867 escaping, each with the expected bash/zsh, fish and
/// PowerShell spellings. The bash/zsh and fish forms were checked in real shells: passing the
/// word to `printf '%s'` reproduces the original bytes exactly (bash 5.3, zsh 5.9, fish 4.2).
const HOSTILE_NAMES: &[(&str, &str, &str, &str)] = &[
    // (name, bash/zsh, fish, PowerShell)
    (
        "rc\rexit",
        r"$'rc\x0Dexit'",
        r"rc\x0Dexit",
        r#""rc$([char]0x000D)exit""#,
    ),
    ("a\nb", r"$'a\x0Ab'", r"a\x0Ab", r#""a$([char]0x000A)b""#),
    (
        "t\tab",
        r"$'t\x09ab'",
        r"t\x09ab",
        r#""t$([char]0x0009)ab""#,
    ),
    (
        "esc\x1b[31mRED",
        r"$'esc\x1B[31mRED'",
        r"esc\x1B\[31mRED",
        r#""esc$([char]0x001B)[31mRED""#,
    ),
    ("x\x01y", r"$'x\x01y'", r"x\x01y", r#""x$([char]0x0001)y""#),
    (
        "del\x7fz",
        r"$'del\x7Fz'",
        r"del\x7Fz",
        r#""del$([char]0x007F)z""#,
    ),
    (
        "trail\n",
        r"$'trail\x0A'",
        r"trail\x0A",
        r#""trail$([char]0x000A)""#,
    ),
    ("\n", r"$'\x0A'", r"\x0A", r#""$([char]0x000A)""#),
    // Quotes, backslashes and expansions combined with a control character: only `\` and `'`
    // are special inside `$'...'` (`'` is written `\x27`, and `!` `\x21` so bash 3.2's history
    // scanner never sees either); in PowerShell's double quotes, `` ` ``, `"` and `$` are.
    (
        "it's \"q\"\r$(x)`y` \\ z",
        r#"$'it\x27s "q"\x0D$(x)`y` \\ z'"#,
        r#"it\'s\ \"q\"\x0D\$\(x\)\`y\`\ \\\ z"#,
        r#""it's `"q`"$([char]0x000D)`$(x)``y`` \ z""#,
    ),
    (
        "hi!\r!!",
        r"$'hi\x21\x0D\x21\x21'",
        r"hi\!\x0D\!\!",
        r#""hi!$([char]0x000D)!!""#,
    ),
    // A literal backslash-x in the name must not turn into a byte escape.
    (
        "bs\\x41\r",
        r"$'bs\\x41\x0D'",
        r"bs\\x41\x0D",
        r#""bs\x41$([char]0x000D)""#,
    ),
    // C1 controls: bash/zsh get the UTF-8 bytes, which is locale independent.
    (
        "c1\u{85}x",
        r"$'c1\xC2\x85x'",
        r"c1\u0085x",
        r#""c1$([char]0x0085)x""#,
    ),
    (
        "ls\u{2028}sep",
        r"$'ls\xE2\x80\xA8sep'",
        r"ls\u2028sep",
        r#""ls$([char]0x2028)sep""#,
    ),
    // A tilde stays literal; only `shell_escape` keeps a leading `~/` expandable.
    (
        "~lit\r",
        r"$'~lit\x0D'",
        r"\~lit\x0D",
        r#""~lit$([char]0x000D)""#,
    ),
];

#[test]
fn test_escape_control_characters_per_dialect() {
    for &(name, posix, fish, powershell) in HOSTILE_NAMES {
        assert_eq!(ShellDialect::Posix.escape(name), posix, "{name:?}");
        assert_eq!(ShellDialect::Fish.escape(name), fish, "{name:?}");
        assert_eq!(
            ShellDialect::PowerShell.escape(name),
            powershell,
            "{name:?}"
        );
        // The family-level API, used where the exact shell is unknown, gives the bash/zsh
        // form for POSIX shells. fish rejects `$'` at parse time, so it fails closed.
        assert_eq!(ShellFamily::Posix.escape(name), posix, "{name:?}");
        assert_eq!(ShellFamily::PowerShell.escape(name), powershell, "{name:?}");
        for dialect in [
            ShellDialect::Posix,
            ShellDialect::Fish,
            ShellDialect::PowerShell,
        ] {
            let escaped = dialect.escape(name);
            assert!(
                !escaped.chars().any(needs_control_char_quoting),
                "{dialect:?} left a control character in {escaped:?}"
            );
        }
    }
}

#[test]
fn test_shell_escape_keeps_home_prefix_with_control_characters() {
    assert_eq!(
        ShellDialect::Posix.shell_escape("~/rc\rexit"),
        r"~/$'rc\x0Dexit'"
    );
    assert_eq!(
        ShellDialect::Fish.shell_escape("~/rc\rexit"),
        r"~/rc\x0Dexit"
    );
    assert_eq!(
        ShellDialect::Posix.shell_escape("$HOME/a b\n"),
        r"$HOME/$'a b\x0A'"
    );
    assert_eq!(
        ShellDialect::PowerShell.shell_escape("~/a\rb"),
        r#"~/"a$([char]0x000D)b""#
    );
    assert_eq!(
        ShellDialect::PowerShell.shell_escape("~\\a\rb"),
        r#"~\"a$([char]0x000D)b""#
    );
    // The quoted word starts after the separator: `~$'/rc\x0Dexit'` would not expand `~` in
    // bash or zsh, because the `/` after it is quoted.
    assert_eq!(
        ShellDialect::Posix.shell_escape("~/d\r/f'!"),
        r"~/$'d\x0D/f\x27\x21'"
    );
    // A POSIX backslash separator stays inside the quotes; outside it would escape the `$`.
    assert_eq!(ShellDialect::Posix.shell_escape("~\\a\rb"), r"~$'\\a\x0Db'");
    // Without a control character, nothing changes.
    assert_eq!(ShellDialect::Posix.shell_escape("~/a b"), r"~/a\ b");
    assert_eq!(ShellDialect::Posix.shell_escape("~/"), "~/");
}

#[test]
fn test_ordinary_names_escape_unchanged_in_every_dialect() {
    let names = [
        "",
        "plain.txt",
        "あい",
        "my file (1).txt",
        "~/test_dir/library% 1$2",
        "foo '\"' bar",
        "a\\b",
        "$'look-alike'",
        "semi;colon|pipe&amp>gt<lt",
        "emoji 😀 rtl \u{202E}x",
    ];
    for name in names {
        for dialect in [ShellDialect::Posix, ShellDialect::Fish] {
            assert_eq!(
                dialect.escape(name),
                ShellFamily::Posix.escape_metacharacters(name),
                "{dialect:?} {name:?}"
            );
            assert_eq!(
                dialect.shell_escape(name),
                ShellFamily::Posix.shell_escape(name)
            );
        }
        assert_eq!(
            ShellDialect::PowerShell.escape(name),
            ShellFamily::PowerShell.escape_metacharacters(name),
            "{name:?}"
        );
    }
    // Still borrowed when nothing needs escaping.
    assert!(matches!(
        ShellDialect::Fish.escape("plain.txt"),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn test_needs_control_char_quoting() {
    for c in [
        '\0', '\r', '\n', '\t', '\x1b', '\x7f', '\u{80}', '\u{9f}', '\u{2028}', '\u{2029}',
    ] {
        assert!(needs_control_char_quoting(c), "{c:?}");
    }
    for c in [' ', 'a', '~', '\u{a0}', 'é', '\u{202e}', '😀'] {
        assert!(!needs_control_char_quoting(c), "{c:?}");
    }
}

#[test]
fn test_posix_unescape() {
    let shell_family = ShellFamily::Posix;
    // Escaped spaces
    assert_eq!(shell_family.unescape("my\\ file.txt"), "my file.txt");
    // Multiple escaped characters
    assert_eq!(
        shell_family.unescape("path/to/my\\ file\\ \\(1\\).txt"),
        "path/to/my file (1).txt"
    );
    // No escaping needed — returns borrowed
    assert!(matches!(
        shell_family.unescape("simple.txt"),
        std::borrow::Cow::Borrowed(_)
    ));
    // Trailing backslash kept as-is
    assert_eq!(shell_family.unescape("trailing\\"), "trailing\\");
    // Roundtrip: unescape(escape(x)) == x
    let original = "hello world $HOME 'quotes'";
    assert_eq!(
        shell_family.unescape(&shell_family.escape(original)),
        original
    );
}

#[test]
fn test_powershell_unescape() {
    let shell_family = ShellFamily::PowerShell;
    // Escaped spaces
    assert_eq!(shell_family.unescape("my` file.txt"), "my file.txt");
    // Multiple escaped characters
    assert_eq!(shell_family.unescape("path` `$var"), "path $var");
    // No escaping needed — returns borrowed
    assert!(matches!(
        shell_family.unescape("simple.txt"),
        std::borrow::Cow::Borrowed(_)
    ));
    // Roundtrip: unescape(escape(x)) == x
    let original = "hello world $HOME";
    assert_eq!(
        shell_family.unescape(&shell_family.escape(original)),
        original
    );
}

#[test]
fn test_clean_path() {
    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml:10:5"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 10,
                column_num: Some(5)
            }),
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml:30:5abc"),
        CleanPathResult {
            path: "Cargo.toml:30:5abc".into(),
            line_and_column_num: None
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml[30,5]"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 30,
                column_num: Some(5)
            })
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml(3,1)"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 3,
                column_num: Some(1)
            })
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml\", line 100, in"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 100,
                column_num: None,
            })
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml\", line 5, column 20"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 5,
                column_num: Some(20),
            })
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml#L100"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 100,
                column_num: None
            })
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml#L100:4"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 100,
                column_num: Some(4)
            })
        }
    );

    // Line range format :start-end (should link to start line, ignore end line)
    assert_eq!(
        CleanPathResult::with_line_and_column_number("Cargo.toml:10-50"),
        CleanPathResult {
            path: "Cargo.toml".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 10,
                column_num: None
            })
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("/path/to/file.rs:1-1000"),
        CleanPathResult {
            path: "/path/to/file.rs".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 1,
                column_num: None
            })
        }
    );

    assert_eq!(
        CleanPathResult::with_line_and_column_number("src/main.rs:100-100"),
        CleanPathResult {
            path: "src/main.rs".into(),
            line_and_column_num: Some(LineAndColumnArg {
                line_num: 100,
                column_num: None
            })
        }
    );
}

/// The single predicate every WSL UNC host check in this module goes through, so its casing and
/// host-spelling table is asserted here rather than repeated at each call site.
#[test]
fn test_is_wsl_unc_host() {
    for host in [
        "wsl$",
        "WSL$",
        "Wsl$",
        "wsl.localhost",
        "WSL.localhost",
        "Wsl.Localhost",
    ] {
        assert!(
            is_wsl_unc_host(host),
            "{host} should be recognized as a WSL UNC host"
        );
    }

    for host in [
        "",
        "wsl",
        "wsl$$",
        "wsl.localhost.example.com",
        "localhost",
        "server",
    ] {
        assert!(
            !is_wsl_unc_host(host),
            "{host} should not be recognized as a WSL UNC host"
        );
    }
}

/// Covers the prefix-kind dispatch — only UNC and verbatim UNC prefixes carry a host to check.
#[test]
fn test_is_wsl_unc_prefix() {
    fn is_wsl_unc_path_prefix(path: &str) -> bool {
        let typed_path = TypedPath::from(path);
        match typed_path.components().next() {
            Some(TypedComponent::Windows(WindowsComponent::Prefix(prefix))) => {
                is_wsl_unc_prefix(&prefix)
            }
            _ => panic!("{path} should be inferred as a Windows path with a prefix"),
        }
    }

    for path in [
        "//wsl$/Ubuntu/home",
        "//WSL.localhost/Ubuntu/home",
        r"\\wsl$\Ubuntu\home",
        r"\\?\UNC\WSL.localhost\Ubuntu\home",
    ] {
        assert!(
            is_wsl_unc_path_prefix(path),
            "{path} should be recognized as a WSL UNC prefix"
        );
    }

    for path in [
        r"\\server\share\file",
        r"\\?\UNC\server\share",
        r"\\.\COM42",
        r"\\?\pictures",
        r"C:\Users",
        r"\\?\C:\Users",
    ] {
        assert!(
            !is_wsl_unc_path_prefix(path),
            "{path} should not be recognized as a WSL UNC prefix"
        );
    }
}

/// WSL is exposed over UNC but served locally, so no spelling or casing of its host may be
/// classified as a network resource — that would make `is_path_valid` reject valid WSL file links.
#[test]
#[cfg(windows)]
fn test_is_network_resource_wsl_hosts_are_local() {
    for path in [
        r"\\WSL$\Ubuntu\home\user\file.rs",
        r"\\wsl$\Ubuntu\home\user\file.rs",
        r"\\Wsl$\Ubuntu\home\user\file.rs",
        r"\\wsl.localhost\Ubuntu\home\user\file.rs",
        r"\\WSL.localhost\Ubuntu\home\user\file.rs",
        r"\\Wsl.Localhost\Ubuntu\home\user\file.rs",
        r"\\?\UNC\wsl$\Ubuntu\home\user\file.rs",
        r"\\?\UNC\WSL$\Ubuntu\home\user\file.rs",
        r"\\?\UNC\wsl.localhost\Ubuntu\home\user\file.rs",
        r"\\?\UNC\WSL.localhost\Ubuntu\home\user\file.rs",
    ] {
        assert!(
            !is_network_resource(Path::new(path)),
            "{path} should not be treated as a network resource"
        );
    }
}

#[test]
#[cfg(windows)]
fn test_is_network_resource_non_wsl_paths() {
    for path in [
        r"\\server\share\file",
        r"\\?\UNC\server\share\file",
        r"\\wsl\Ubuntu\home",
    ] {
        assert!(
            is_network_resource(Path::new(path)),
            "{path} should be treated as a network resource"
        );
    }

    for path in [r"C:\Users\x", r"\\?\C:\Users\x", r"relative\path"] {
        assert!(
            !is_network_resource(Path::new(path)),
            "{path} should not be treated as a network resource"
        );
    }
}

#[test]
#[cfg(windows)]
fn test_msys2_exe_to_root() {
    assert_eq!(
        msys2_exe_to_root(WindowsPath::new(r"D:\Program Files\Git\usr\bin\git.exe")),
        WindowsPathBuf::from(r"D:\Program Files\Git")
    );
    assert_eq!(
        msys2_exe_to_root(WindowsPath::new(r"C:\git.exe")),
        WindowsPathBuf::from(r"C:\Program Files\Git")
    );
    assert_eq!(
        msys2_exe_to_root(WindowsPath::new(r"C:\foo\bar\baz\git.exe")),
        WindowsPathBuf::from(r"C:\Program Files\Git")
    );
    assert_eq!(
        msys2_exe_to_root(WindowsPath::new(r"C:\msys64\usr\bin\fish.exe")),
        WindowsPathBuf::from(r"C:\msys64")
    );
}

/// These tests all fail when running on UNIX.
#[test]
#[cfg(windows)]
fn test_convert_git_bash_to_windows_native_path() {
    use std::sync::LazyLock;
    static GIT_BASH_ROOT: LazyLock<WindowsPathBuf> =
        LazyLock::new(|| WindowsPathBuf::from(r"C:\Program Files\Git"));

    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/c/foo/bar").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"C:\foo\bar")
    );
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/d/special folder").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"D:\special folder")
    );
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/z").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"Z:\")
    );
    // non-ascii isn't actually a valid drive name
    assert!(matches!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/😊/invalid").to_path(),
            &GIT_BASH_ROOT
        ),
        Err(MSYS2PathConversionError::NotInDrive)
    ));
    assert!(matches!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/aa/invalid").to_path(),
            &GIT_BASH_ROOT
        ),
        Err(MSYS2PathConversionError::NotInDrive)
    ));
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("//wsl$/Ubuntu/home").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"\\wsl$\Ubuntu\home")
    );
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("//WSL.localhost/Ubuntu/home").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"\\WSL.localhost\Ubuntu\home")
    );
    // These paths might get auto-inferred by typed-path to be Windows paths, even if they look
    // like UNIX with forward slashes. Every casing of both WSL UNC hosts must be accepted.
    for path in [
        "//wsl$/Ubuntu/home",
        "//WSL$/Ubuntu/home",
        "//Wsl$/Ubuntu/home",
        "//wsl.localhost/Ubuntu/home",
        "//WSL.localhost/Ubuntu/home",
        "//Wsl.Localhost/Ubuntu/home",
    ] {
        assert_eq!(
            convert_msys2_to_windows_native_path(&TypedPath::from(path), &GIT_BASH_ROOT)
                .unwrap_or_else(|err| panic!("{path} should be convertible, got {err:?}")),
            PathBuf::from(path.replace('/', "\\"))
        );
    }
    // A Windows-encoded UNC path to an actual remote host is still rejected.
    assert!(matches!(
        convert_msys2_to_windows_native_path(&TypedPath::from("//server/share"), &GIT_BASH_ROOT),
        Err(MSYS2PathConversionError::NonUnixPath)
    ));
    // Windows drive letters are case-insensitive, so an uppercase MSYS2 drive prefix resolves the
    // same as a lowercase one.
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/C/foo/bar").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"C:\foo\bar")
    );
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("~/.bash_history").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"~\.bash_history")
    );
    // Relative paths cannot be converted.
    assert!(matches!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("some/relative/path").to_path(),
            &GIT_BASH_ROOT
        ),
        Err(MSYS2PathConversionError::PathNotAbsolute)
    ));
    assert!(matches!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_windows(r"C:\Users").to_path(),
            &GIT_BASH_ROOT
        ),
        Err(MSYS2PathConversionError::NonUnixPath)
    ));
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"C:\Program Files\Git")
    );
    assert_eq!(
        convert_msys2_to_windows_native_path(
            &TypedPathBuf::from_unix("/usr/bin").to_path(),
            &GIT_BASH_ROOT
        )
        .unwrap(),
        PathBuf::from(r"C:\Program Files\Git\usr\bin")
    );
}

/// These tests all fail when running on UNIX.
#[test]
#[cfg(windows)]
fn test_convert_wsl_to_windows_host_path() {
    assert_eq!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("/mnt/c/foo/bar").to_path(),
            "Ubuntu"
        )
        .unwrap(),
        PathBuf::from(r"C:\foo\bar")
    );
    assert_eq!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("/mnt/e/special dir").to_path(),
            "Ubuntu"
        )
        .unwrap(),
        PathBuf::from(r"E:\special dir")
    );
    assert_eq!(
        convert_wsl_to_windows_host_path(&TypedPathBuf::from_unix("/mnt/z").to_path(), "Ubuntu")
            .unwrap(),
        PathBuf::from(r"Z:\")
    );
    // Windows drive letters are case-insensitive, so an uppercase mount resolves the same as a
    // lowercase one.
    assert_eq!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("/mnt/C/foo").to_path(),
            "Ubuntu"
        )
        .unwrap(),
        PathBuf::from(r"C:\foo")
    );
    assert_eq!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("/home/andy").to_path(),
            "Ubuntu"
        )
        .unwrap(),
        PathBuf::from(r"\\WSL$\Ubuntu\home\andy")
    );
    // Paths inside the distribution are returned as the UNC path they were built from, with no
    // symlink resolution. Resolving them would mean a `read_link` over the 9p transport for every
    // conversion, which is what froze the UI while WSL tabs were open (upstream #12492). Only the
    // `/mnt/<drive>` arm above touches the filesystem. `/bin` is a symlink to `/usr/bin` on every
    // usr-merged distribution, so on a host that really has a distro installed this asserts the
    // link is *not* followed.
    assert_eq!(
        convert_wsl_to_windows_host_path(&TypedPathBuf::from_unix("/bin").to_path(), "Ubuntu")
            .unwrap(),
        PathBuf::from(r"\\WSL$\Ubuntu\bin")
    );
    assert!(matches!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("some/relative/path").to_path(),
            "Ubuntu"
        ),
        Err(WSLPathConversionError::PathNotAbsolute)
    ));
    assert!(matches!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("~/.bash_history").to_path(),
            "Ubuntu"
        ),
        Err(WSLPathConversionError::PathNotAbsolute)
    ));
    // Two letters isn't actually a valid drive.
    assert_eq!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("/mnt/aa/invalid_drive").to_path(),
            "Ubuntu"
        )
        .unwrap(),
        PathBuf::from(r"\\WSL$\Ubuntu\mnt\aa\invalid_drive")
    );
    assert_eq!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_unix("/mnt/😊/invalid_drive").to_path(),
            "Ubuntu"
        )
        .unwrap(),
        PathBuf::from(r"\\WSL$\Ubuntu\mnt\😊\invalid_drive")
    );
    assert!(matches!(
        convert_wsl_to_windows_host_path(
            &TypedPathBuf::from_windows(r"C:\Users").to_path(),
            "Ubuntu"
        ),
        Err(WSLPathConversionError::NonUnixPath)
    ));
}

/// Asserts that `input` parses into the given distribution and Linux path.
fn assert_parses_wsl_unc(input: &str, distro: &str, linux_path: &str) {
    assert_eq!(
        parse_wsl_unc_path(Path::new(input)),
        Some(WslUncPath {
            distro: distro.to_string(),
            linux_path: linux_path.to_string(),
        }),
        "input: {input:?}"
    );
}

#[test]
fn test_parse_wsl_unc_path() {
    assert_parses_wsl_unc(r"\\wsl$\Ubuntu\home\user", "Ubuntu", "/home/user");
    assert_parses_wsl_unc(r"\\wsl.localhost\Ubuntu\home", "Ubuntu", "/home");
    assert_parses_wsl_unc(r"\\?\UNC\wsl$\Ubuntu\home\user", "Ubuntu", "/home/user");
    assert_parses_wsl_unc(
        r"\\?\UNC\wsl.localhost\Ubuntu\srv\repo",
        "Ubuntu",
        "/srv/repo",
    );
    assert_parses_wsl_unc("//wsl$/Ubuntu/home/user", "Ubuntu", "/home/user");
    // The host is matched case-insensitively.
    assert_parses_wsl_unc(r"\\WSL$\Ubuntu\src", "Ubuntu", "/src");
    assert_parses_wsl_unc(r"\\Wsl.LocalHost\Ubuntu\src", "Ubuntu", "/src");
    // Trailing separators are dropped.
    assert_parses_wsl_unc(r"\\wsl$\Ubuntu\home\user\", "Ubuntu", "/home/user");
    assert_parses_wsl_unc("//wsl$/Ubuntu/home/user/", "Ubuntu", "/home/user");
    // Distribution names may contain dots and dashes.
    assert_parses_wsl_unc(
        r"\\wsl.localhost\Ubuntu-24.04\home\krag",
        "Ubuntu-24.04",
        "/home/krag",
    );
    // The distribution root maps to `/`.
    assert_parses_wsl_unc(r"\\wsl$\Ubuntu", "Ubuntu", "/");
    assert_parses_wsl_unc(r"\\wsl$\Ubuntu\", "Ubuntu", "/");
    assert_parses_wsl_unc("//wsl$/archlinux", "archlinux", "/");
}

#[test]
fn test_parse_wsl_unc_path_rejects_other_paths() {
    // Non-WSL UNC share.
    assert_eq!(parse_wsl_unc_path(Path::new(r"\\server\share\dir")), None);
    // Drive-letter path.
    assert_eq!(parse_wsl_unc_path(Path::new(r"C:\Users\foo")), None);
    // Relative paths.
    assert_eq!(parse_wsl_unc_path(Path::new(r"foo\bar")), None);
    assert_eq!(parse_wsl_unc_path(Path::new("foo/bar")), None);
    // WSL host without a distribution name.
    assert_eq!(parse_wsl_unc_path(Path::new(r"\\wsl$")), None);
    assert_eq!(parse_wsl_unc_path(Path::new(r"\\wsl$\")), None);
}

#[test]
fn test_convert_windows_path_to_wsl() {
    assert_eq!(
        convert_windows_path_to_wsl(r"C:\Users\aloke\file.txt"),
        "/mnt/c/Users/aloke/file.txt"
    );
    assert_eq!(
        convert_windows_path_to_wsl(r"D:\Pictures\Screenshots\Screenshot 2025-05-14 155816.png"),
        "/mnt/d/Pictures/Screenshots/Screenshot 2025-05-14 155816.png"
    );
    // Drive letter only
    assert_eq!(convert_windows_path_to_wsl(r"C:\"), "/mnt/c");
    assert_eq!(convert_windows_path_to_wsl("C:"), "/mnt/c");
    // Uppercase drive letter should be lowercased
    assert_eq!(convert_windows_path_to_wsl(r"E:\foo"), "/mnt/e/foo");
    // Non-drive path (e.g. UNC) gets backslashes replaced
    assert_eq!(
        convert_windows_path_to_wsl(r"\\server\share\file"),
        "//server/share/file"
    );
}

#[test]
fn test_convert_windows_path_to_msys2() {
    assert_eq!(
        convert_windows_path_to_msys2(r"C:\Users\aloke\file.txt"),
        "/c/Users/aloke/file.txt"
    );
    assert_eq!(
        convert_windows_path_to_msys2(r"D:\Pictures\Screenshots\Screenshot 2025-05-14 155816.png"),
        "/d/Pictures/Screenshots/Screenshot 2025-05-14 155816.png"
    );
    // Drive letter only
    assert_eq!(convert_windows_path_to_msys2(r"C:\"), "/c");
    assert_eq!(convert_windows_path_to_msys2("C:"), "/c");
    // Uppercase drive letter should be lowercased
    assert_eq!(convert_windows_path_to_msys2(r"E:\foo"), "/e/foo");
    // Non-drive path (e.g. UNC) gets backslashes replaced
    assert_eq!(
        convert_windows_path_to_msys2(r"\\server\share\file"),
        "//server/share/file"
    );
}

#[test]
fn test_file_uri_drive_path_to_windows() {
    assert_eq!(
        file_uri_drive_path_to_windows("/E:/CLAUDE-BASE"),
        r"E:\CLAUDE-BASE"
    );
    assert_eq!(
        file_uri_drive_path_to_windows("/C:/Users/foo/bar"),
        r"C:\Users\foo\bar"
    );
    assert_eq!(file_uri_drive_path_to_windows("/E:/"), r"E:\");
    assert_eq!(file_uri_drive_path_to_windows("/E:"), "E:");
    assert_eq!(
        file_uri_drive_path_to_windows("/Users/foo/bar"),
        "/Users/foo/bar"
    );
    assert_eq!(file_uri_drive_path_to_windows("/E/notdrive"), "/E/notdrive");
    assert_eq!(file_uri_drive_path_to_windows("/E:extra"), "/E:extra");
    assert_eq!(
        file_uri_drive_path_to_windows(r"E:\CLAUDE-BASE"),
        r"E:\CLAUDE-BASE"
    );
}

#[test]
fn test_canonicalize_git_bash_path() {
    assert_eq!(
        canonicalize_git_bash_path(
            Path::new("C:")
                .join("Program Files")
                .join("Git")
                .join("bin")
                .join("bash.exe")
        ),
        Path::new("C:")
            .join("Program Files")
            .join("Git")
            .join("usr")
            .join("bin")
            .join("bash.exe")
    );
    assert_eq!(
        canonicalize_git_bash_path(
            Path::new("C:")
                .join("Windows")
                .join("system32")
                .join("bash.exe")
        ),
        Path::new("C:")
            .join("Windows")
            .join("system32")
            .join("bash.exe")
    );
}

// ── group_roots_by_common_ancestor tests ─────────────────────────────

mod group_roots_by_common_ancestor_tests {
    use crate::path::group_roots_by_common_ancestor;
    use std::path::PathBuf;

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn empty_input_produces_empty_grouping() {
        let grouping = group_roots_by_common_ancestor::<PathBuf>(&[]);
        assert!(grouping.roots.is_empty());
        assert!(grouping.absorbed_by_root.is_empty());
    }

    #[test]
    fn single_path_survives_with_no_absorbed() {
        let grouping = group_roots_by_common_ancestor(&[pb("/a")]);
        assert_eq!(grouping.roots, vec![pb("/a")]);
        assert!(grouping.absorbed_by_root.is_empty());
    }

    #[test]
    fn unrelated_siblings_both_survive() {
        let grouping = group_roots_by_common_ancestor(&[pb("/a"), pb("/b")]);
        assert_eq!(grouping.roots, vec![pb("/a"), pb("/b")]);
        assert!(grouping.absorbed_by_root.is_empty());
    }

    #[test]
    fn descendant_absorbed_into_ancestor() {
        // Ancestor listed first.
        let grouping = group_roots_by_common_ancestor(&[pb("/a"), pb("/a/b")]);
        assert_eq!(grouping.roots, vec![pb("/a")]);
        assert_eq!(grouping.absorbed_by_root.len(), 1);
        assert_eq!(grouping.absorbed_by_root[&pb("/a")], vec![pb("/a/b")]);
    }

    #[test]
    fn descendant_first_still_absorbed() {
        // Descendant listed first, ancestor second; survivor is still the
        // ancestor and its input order is preserved.
        let grouping = group_roots_by_common_ancestor(&[pb("/a/b"), pb("/a")]);
        assert_eq!(grouping.roots, vec![pb("/a")]);
        assert_eq!(grouping.absorbed_by_root[&pb("/a")], vec![pb("/a/b")]);
    }

    #[test]
    fn three_deep_chain_collapses_to_root() {
        // Descendant order in input is preserved in the absorbed list.
        let grouping = group_roots_by_common_ancestor(&[pb("/a/b/c"), pb("/a/b"), pb("/a")]);
        assert_eq!(grouping.roots, vec![pb("/a")]);
        assert_eq!(
            grouping.absorbed_by_root[&pb("/a")],
            vec![pb("/a/b/c"), pb("/a/b")]
        );
    }

    #[test]
    fn mixed_groups_absorb_independently() {
        let grouping =
            group_roots_by_common_ancestor(&[pb("/a"), pb("/x"), pb("/a/b"), pb("/x/y")]);
        assert_eq!(grouping.roots, vec![pb("/a"), pb("/x")]);
        assert_eq!(grouping.absorbed_by_root[&pb("/a")], vec![pb("/a/b")]);
        assert_eq!(grouping.absorbed_by_root[&pb("/x")], vec![pb("/x/y")]);
    }

    #[test]
    fn same_prefix_different_component_name_both_survive() {
        // /foo/a is NOT an ancestor of /foo/abc (component-aware match).
        let grouping = group_roots_by_common_ancestor(&[pb("/foo/a"), pb("/foo/abc")]);
        assert_eq!(grouping.roots, vec![pb("/foo/a"), pb("/foo/abc")]);
        assert!(grouping.absorbed_by_root.is_empty());
    }

    #[test]
    fn duplicate_inputs_collapse_to_single_survivor() {
        let grouping = group_roots_by_common_ancestor(&[pb("/a"), pb("/a")]);
        assert_eq!(grouping.roots, vec![pb("/a")]);
        assert!(grouping.absorbed_by_root.is_empty());
    }

    #[test]
    fn surviving_root_order_matches_input_order() {
        // Insert a descendant between two surviving ancestors; the survivors
        // should appear in their original input order even though processing
        // sorted by component count.
        let grouping = group_roots_by_common_ancestor(&[pb("/b"), pb("/a/x"), pb("/a"), pb("/c")]);
        assert_eq!(grouping.roots, vec![pb("/b"), pb("/a"), pb("/c")]);
        assert_eq!(grouping.absorbed_by_root[&pb("/a")], vec![pb("/a/x")]);
    }

    #[test]
    fn descendant_absorbed_by_closest_ancestor_not_furthest() {
        // Both /a and /a/b are surviving ancestors of /a/b/c... wait, /a/b is
        // itself absorbed into /a. So /a/b/c should be absorbed into /a as
        // well (the only surviving ancestor).
        let grouping = group_roots_by_common_ancestor(&[pb("/a"), pb("/a/b"), pb("/a/b/c")]);
        assert_eq!(grouping.roots, vec![pb("/a")]);
        assert_eq!(
            grouping.absorbed_by_root[&pb("/a")],
            vec![pb("/a/b"), pb("/a/b/c")]
        );
    }
}

/// TODO(CORE-3626): write an equivalent test with Windows paths.
#[cfg(not(windows))]
#[test]
fn test_resolve_command() {
    use std::path::Path;

    use crate::path::resolve_executable;

    assert_eq!(
        &resolve_executable("/bin/sh").unwrap(),
        Path::new("/bin/sh")
    );
    assert_eq!(
        &resolve_executable("env").unwrap(),
        Path::new("/usr/bin/env")
    );
    // This path exists in the Zap repo, so it should resolve. The `../../`
    // is because Rust unit tests run from the root of the crate (`warp_util`
    // in this case).
    assert_eq!(
        &resolve_executable("../../script/run").unwrap(),
        Path::new("../../script/run")
    );
    // `pwd` should always exist (it's also a shell builtin), but we won't
    // assume a specific location.
    assert!(resolve_executable("pwd").is_some());
    assert!(resolve_executable("unlikely-command").is_none());
    assert!(resolve_executable("nonexistent/relative/path").is_none());
    // src/main.rs does exist, but is not executable.
    assert!(resolve_executable("src/main.rs").is_none());
    // Note the trailing space.
    assert!(resolve_executable("zsh ").is_none());
}

#[cfg(windows)]
struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

#[cfg(windows)]
impl EnvVarGuard {
    fn set(key: &'static str, value: impl Into<std::ffi::OsString>) -> Self {
        let original = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value.into());
        }
        Self { key, original }
    }
}

#[cfg(windows)]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(original) = &self.original {
            unsafe {
                std::env::set_var(self.key, original);
            }
        } else {
            unsafe {
                std::env::remove_var(self.key);
            }
        }
    }
}

#[cfg(windows)]
#[test]
#[serial_test::serial]
fn test_resolve_command_uses_pathext() {
    use crate::path::resolve_executable_in_path;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let command_path = temp_dir.path().join("codex.cmd");
    std::fs::write(&command_path, "@echo off\r\n").unwrap();

    let _pathext = EnvVarGuard::set("PATHEXT", ".CMD;.EXE");
    let resolved = resolve_executable_in_path("codex", temp_dir.path().as_os_str()).unwrap();
    assert_eq!(
        resolved.as_ref().to_string_lossy().to_ascii_lowercase(),
        command_path.to_string_lossy().to_ascii_lowercase()
    );
}
