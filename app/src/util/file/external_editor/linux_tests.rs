use warp_util::path::LineAndColumnArg;

use super::{DesktopExecError, EditorMetadata};
use std::path::PathBuf;

#[cfg(test)]
fn with_files(tag: &str, contents: &str, cb: impl FnOnce(PathBuf, PathBuf) -> anyhow::Result<()>) {
    use crate::test_util::{Stub, VirtualFS};

    VirtualFS::test(tag, |dirs, mut sandbox| {
        sandbox.with_files(vec![
            Stub::FileWithContent("bar.desktop", contents),
            Stub::EmptyFile("foo.txt"),
        ]);

        let desktop_file_path = dirs.tests().join("bar.desktop");
        let content_file_path = dirs.tests().join("foo.txt");

        match cb(desktop_file_path, content_file_path) {
            Ok(_) => {}
            Err(err) => panic!("{err:?}"),
        };
    })
}

#[test]
fn test_missing_exec_command_errors() {
    with_files(
        "test_missing_exec_command_errors",
        "",
        |desktop, _content| {
            let result = EditorMetadata::try_new(desktop);

            assert!(matches!(result, Err(DesktopExecError::NoExec)));
            Ok(())
        },
    )
}

#[test]
fn test_exec_ending_on_percent_fails() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=echo "hello world" %
    "#;
    with_files(
        "test_exec_ending_on_percent_fails",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let result = metadata.build_default_command(&content);
            assert!(matches!(result, Err(DesktopExecError::MalformedFieldCode)));
            Ok(())
        },
    )
}

#[test]
fn test_basic_exec_no_field_codes() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=echo "hello world"
    "#;
    with_files(
        "test_basic_exec_no_field_codes",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let result = metadata.build_default_command(&content);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.get_program(), "echo");
            assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["hello world"]);
            Ok(())
        },
    )
}

#[test]
fn test_file_path_substitution() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=cat %f
    "#;
    with_files("test_file_path_substitution", data, |desktop, content| {
        let metadata = EditorMetadata::try_new(desktop)?;
        let file_name = content.display().to_string();
        let result = metadata.build_default_command(&content);

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().get_args().collect::<Vec<_>>(),
            [file_name.as_str()]
        );
        Ok(())
    });

    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=cat %F
    "#;
    with_files("test_file_path_substitution", data, |desktop, content| {
        let metadata = EditorMetadata::try_new(desktop)?;
        let file_name = content.display().to_string();
        let result = metadata.build_default_command(&content);

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().get_args().collect::<Vec<_>>(),
            [file_name.as_str()]
        );
        Ok(())
    });
}

#[test]
fn test_file_url_substitution() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=open %u
    "#;
    with_files("test_file_url_substitution", data, |desktop, content| {
        let metadata = EditorMetadata::try_new(desktop)?;
        let file_name = content.display().to_string();
        let expected_file_uri = format!("file://{file_name}");
        let result = metadata.build_default_command(&content);

        assert!(result.is_ok());

        assert_eq!(
            result.unwrap().get_args().collect::<Vec<_>>(),
            [expected_file_uri.as_str()]
        );
        Ok(())
    });

    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=open %U
    "#;
    with_files("test_file_url_substitution", data, |desktop, content| {
        let metadata = EditorMetadata::try_new(desktop)?;
        let file_name = content.display().to_string();
        let expected_file_uri = format!("file://{file_name}");
        let result = metadata.build_default_command(&content);

        assert!(result.is_ok());

        assert_eq!(
            result.unwrap().get_args().collect::<Vec<_>>(),
            [expected_file_uri.as_str()]
        );
        Ok(())
    });
}

#[test]
fn test_remaining_substitutions() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=echo %c && echo %i && echo %k && echo %%
    Name=Zap Test Application
    Icon=/foo/bar/icon.png
    "#;
    with_files("test_remaining_substitutions", data, |desktop, content| {
        let desktop_file_path = desktop.display().to_string();
        let metadata = EditorMetadata::try_new(desktop)?;
        let result = metadata.build_default_command(&content);

        assert!(result.is_ok());

        // When building the command based on argv, each token is a separate argument.
        // %c → "Zap Test Application" (a single argument, spaces preserved)
        // %i → "--icon" and "/foo/bar/icon.png" (two separate arguments)
        // %k → the desktop file path
        // %% → "%"
        let cmd = result.unwrap();
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args[0], "Zap Test Application");
        assert_eq!(args[1], "&&");
        assert_eq!(args[2], "echo");
        assert_eq!(args[3], "--icon");
        assert_eq!(args[4], "/foo/bar/icon.png");
        assert_eq!(args[5], "&&");
        assert_eq!(args[6], "echo");
        assert_eq!(args[7], desktop_file_path.as_str());
        assert_eq!(args[8], "&&");
        assert_eq!(args[9], "echo");
        assert_eq!(args[10], "%");
        Ok(())
    });
}

#[test]
fn test_jetbrains_command_no_line_numbers() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/snap/bin/phpstorm %f
    "#;

    with_files(
        "test_jetbrains_command_no_line_numbers",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_jetbrains_command(&content, None);

            assert!(result.is_ok());

            assert_eq!(
                result.unwrap().get_args().collect::<Vec<_>>(),
                [file_path.as_str()]
            );
            Ok(())
        },
    );
}

#[test]
fn test_jetbrains_command_line_numbers() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/snap/bin/phpstorm %f
    "#;

    with_files(
        "test_jetbrains_command_line_numbers",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_jetbrains_command(
                &content,
                Some(LineAndColumnArg {
                    line_num: 42,
                    column_num: None,
                }),
            );

            assert!(result.is_ok());

            assert_eq!(
                result.unwrap().get_args().collect::<Vec<_>>(),
                ["--line", "42", file_path.as_str()]
            );
            Ok(())
        },
    );
}

#[test]
fn test_jetbrains_command_line_and_col_numbers() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/snap/bin/phpstorm %f
    "#;
    with_files(
        "test_jetbrains_command_line_and_col_numbers",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_jetbrains_command(
                &content,
                Some(LineAndColumnArg {
                    line_num: 42,
                    column_num: Some(25),
                }),
            );

            assert!(result.is_ok());

            assert_eq!(
                result.unwrap().get_args().collect::<Vec<_>>(),
                ["--line", "42", "--column", "25", file_path.as_str()]
            );
            Ok(())
        },
    );
}

#[test]
fn test_sublime_command_no_line_numbers() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/snap/bin/subl %f
    "#;
    with_files(
        "test_sublime_command_no_line_numbers",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result: Result<command::blocking::Command, DesktopExecError> =
                metadata.build_sublime_command(&content, None);

            assert!(result.is_ok());

            assert_eq!(
                result.unwrap().get_args().collect::<Vec<_>>(),
                [file_path.as_str()]
            );
            Ok(())
        },
    );
}

#[test]
fn test_sublime_command_line_numbers() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/snap/bin/subl %f
    "#;
    with_files(
        "test_sublime_command_line_numbers",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_sublime_command(
                &content,
                Some(LineAndColumnArg {
                    line_num: 42,
                    column_num: None,
                }),
            );

            assert!(result.is_ok());

            assert_eq!(
                result.unwrap().get_args().collect::<Vec<_>>(),
                [format!("{file_path}:42").as_str()]
            );
            Ok(())
        },
    );
}

#[test]
fn test_sublime_command_line_and_col_numbers() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/snap/bin/subl %f
    "#;
    with_files(
        "test_sublime_command_line_numbers",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_sublime_command(
                &content,
                Some(LineAndColumnArg {
                    line_num: 42,
                    column_num: Some(25),
                }),
            );

            assert!(result.is_ok());

            assert_eq!(
                result.unwrap().get_args().collect::<Vec<_>>(),
                [format!("{file_path}:42:25").as_str()]
            );
            Ok(())
        },
    );
}

// ---------- Shell-metacharacter / quoting behavior of build_default_command ----------
//
// Ported from warp/master's linux_tests.rs. Upstream also exercises a hand-rolled
// `tokenize_exec` function directly; this fork instead delegates tokenization to the
// `shell_words` crate (see `EditorMetadata::build_command`), so the `tokenize_exec`-specific
// unit tests (and the exact `DesktopExecError::UnterminatedQuote` variant, which no longer
// exists here — unterminated quotes surface as `MalformedFieldCode` instead) are not portable
// as literal ports. The behavioral tests below go through the same public
// `try_new`/`build_default_command` surface as the tests above and still apply.

#[test]
fn test_file_path_with_shell_metacharacters_is_single_arg() {
    // Verify that shell metacharacters in file paths are treated as literal
    // characters, not interpreted by a shell.
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/usr/bin/editor %f
    "#;

    let malicious_path = PathBuf::from("/tmp/foo; rm -rf /");
    with_files(
        "test_file_path_with_shell_metacharacters",
        data,
        |desktop, _content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let result = metadata.build_default_command(&malicious_path);

            assert!(result.is_ok());
            let cmd = result.unwrap();
            // The program is the editor, not "sh".
            assert_eq!(cmd.get_program(), "/usr/bin/editor");
            // The malicious path is a single argument, not split by shell.
            assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["/tmp/foo; rm -rf /"]);
            Ok(())
        },
    );
}

#[test]
fn test_file_path_with_spaces_is_single_arg() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/usr/bin/editor %f
    "#;

    let path_with_spaces = PathBuf::from("/home/user/my documents/file.txt");
    with_files("test_file_path_with_spaces", data, |desktop, _content| {
        let metadata = EditorMetadata::try_new(desktop)?;
        let result = metadata.build_default_command(&path_with_spaces);

        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.get_program(), "/usr/bin/editor");
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            ["/home/user/my documents/file.txt"]
        );
        Ok(())
    });
}

#[test]
fn test_quoted_executable_path() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec="/opt/My App/editor" --flag %f
    "#;
    with_files("test_quoted_executable_path", data, |desktop, content| {
        let metadata = EditorMetadata::try_new(desktop)?;
        let file_path = content.display().to_string();
        let result = metadata.build_default_command(&content);

        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.get_program(), "/opt/My App/editor");
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            ["--flag", file_path.as_str()]
        );
        Ok(())
    });
}

#[test]
fn test_quoted_field_code_is_still_expanded() {
    // The spec says field codes must not be used inside a quoted argument and
    // the result is undefined. Our implementation expands them anyway since
    // quotes are stripped before field code processing.
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/usr/bin/editor "%f"
    "#;
    with_files(
        "test_quoted_field_code_is_still_expanded",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_default_command(&content);

            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.get_program(), "/usr/bin/editor");
            assert_eq!(cmd.get_args().collect::<Vec<_>>(), [file_path.as_str()]);
            Ok(())
        },
    );
}

#[test]
fn test_localized_name_with_spaces_is_single_arg() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/usr/bin/app --title %c %f
    Name=My Cool Application
    "#;
    with_files(
        "test_localized_name_with_spaces",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_default_command(&content);

            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.get_program(), "/usr/bin/app");
            // %c expands to a single arg even though the name contains spaces.
            assert_eq!(
                cmd.get_args().collect::<Vec<_>>(),
                ["--title", "My Cool Application", file_path.as_str()]
            );
            Ok(())
        },
    );
}

#[test]
fn test_shell_constructs_in_exec_are_literal() {
    // Subcommand syntax and backticks in the Exec string itself are not
    // interpreted because we execute directly, not via sh -c.
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/usr/bin/app $(whoami) `id` %f
    "#;
    with_files(
        "test_shell_constructs_in_exec_are_literal",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_default_command(&content);

            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.get_program(), "/usr/bin/app");
            assert_eq!(
                cmd.get_args().collect::<Vec<_>>(),
                ["$(whoami)", "`id`", file_path.as_str()]
            );
            Ok(())
        },
    );
}

// Regression check: upstream Warp drops the deprecated FreeDesktop field codes
// (%d %D %n %N %v %m) entirely per spec. This fork's `process_field_code` has no
// explicit arm for them, so they fall into the `other => parts.last_mut().push(other)`
// catch-all and are kept as literal single-character arguments instead of being
// dropped. If this test is red, that confirms the fork emits extra bogus argv
// entries ("d", "D", "n", "N", "v", "m") when launching an external editor whose
// .desktop Exec line still has these legacy codes.
#[test]
fn test_deprecated_field_codes_are_dropped() {
    let data = r#"
    [Desktop Entry]
    Version=1.0
    Type=Application
    Exec=/usr/bin/app %d %D %n %N %v %m %f
    "#;
    with_files(
        "test_deprecated_field_codes_are_dropped",
        data,
        |desktop, content| {
            let metadata = EditorMetadata::try_new(desktop)?;
            let file_path = content.display().to_string();
            let result = metadata.build_default_command(&content);

            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.get_program(), "/usr/bin/app");
            // All deprecated codes are silently dropped; only %f remains.
            assert_eq!(cmd.get_args().collect::<Vec<_>>(), [file_path.as_str()]);
            Ok(())
        },
    );
}
