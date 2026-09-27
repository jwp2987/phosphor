use std::path::PathBuf;

use super::*;
use crate::terminal::shell::ShellType;

#[test]
fn build_find_command_single_quotes_patterns_and_path() {
    let patterns = vec![
        "$(touch /tmp/zap-poc)*.rs".to_string(),
        "owner's*.rs".to_string(),
    ];

    let command = build_find_command(&patterns, "/tmp/repo path", ShellType::Bash);

    assert_eq!(
        command,
        r#"find '/tmp/repo path' -type f -name '$(touch /tmp/zap-poc)*.rs' -o -name 'owner'"'"'s*.rs'"#
    );
}

#[test]
fn build_git_ls_files_command_single_quotes_joined_patterns() {
    let pattern = "$(touch /tmp/zap-poc)*.rs";
    let patterns = vec![pattern.to_string()];
    let target_path = PathBuf::from(std::path::MAIN_SEPARATOR_STR)
        .join("tmp")
        .join("repo");

    let command = build_git_ls_files_command(
        &patterns,
        target_path.to_str().unwrap(),
        None,
        ShellType::Bash,
    );

    let expected = format!(
        "git ls-files -c -o --exclude-standard -- '{}' '{}'",
        target_path.join(pattern).display(),
        target_path.join("*").join(pattern).display(),
    );
    assert_eq!(command, expected);
}

#[test]
fn build_powershell_get_childitem_command_single_quotes_patterns_and_path() {
    let patterns = vec![
        r#"$(New-Item C:\pwn)*.rs"#.to_string(),
        "owner's*.rs".to_string(),
    ];

    let command = build_powershell_get_childitem_command(&patterns, r#"C:\repo path"#);

    assert_eq!(
        command,
        r#"Get-ChildItem -File -Recurse -Include '$(New-Item C:\pwn)*.rs','owner''s*.rs' -Path 'C:\repo path' | ForEach-Object { $_.FullName }"#
    );
}

fn glob_matches(count: usize) -> Vec<FileGlobV2Match> {
    (0..count)
        .map(|i| FileGlobV2Match {
            file_path: format!("/repo/src/file{i}.rs"),
        })
        .collect()
}

/// This is the fix the entry asks for: a model's `limit`, threaded down as
/// `AIAgentActionType::FileGlobV2::result_limit`, must actually shorten the match list the
/// executor hands back -- not just be accepted and clamped in the schema.
#[test]
fn apply_result_limit_truncates_a_success_result_to_the_limit() {
    let result = FileGlobV2Result::Success {
        matched_files: glob_matches(5),
        warnings: None,
    };

    let FileGlobV2Result::Success { matched_files, .. } = apply_result_limit(result, Some(2))
    else {
        panic!("expected Success");
    };
    assert_eq!(matched_files.len(), 2);
    assert_eq!(matched_files[0].file_path, "/repo/src/file0.rs");
    assert_eq!(matched_files[1].file_path, "/repo/src/file1.rs");
}

/// A limit at or above the actual match count is a no-op -- truncation never pads or
/// otherwise changes a list that already satisfies it.
#[test]
fn apply_result_limit_is_a_no_op_when_the_limit_is_not_exceeded() {
    let result = FileGlobV2Result::Success {
        matched_files: glob_matches(3),
        warnings: None,
    };

    let FileGlobV2Result::Success { matched_files, .. } = apply_result_limit(result, Some(10))
    else {
        panic!("expected Success");
    };
    assert_eq!(matched_files.len(), 3);
}

/// `None` -- a plain `FileGlob` v1 request, or any action built before `result_limit`
/// existed -- must apply no truncation at all, leaving `glob_result_to_json`'s own cap as
/// the only backstop, exactly as before this limit was honoured here.
#[test]
fn apply_result_limit_with_no_limit_leaves_the_result_unchanged() {
    let result = FileGlobV2Result::Success {
        matched_files: glob_matches(3),
        warnings: None,
    };

    let FileGlobV2Result::Success { matched_files, .. } = apply_result_limit(result, None) else {
        panic!("expected Success");
    };
    assert_eq!(matched_files.len(), 3);
}

/// A limit is meaningless for an error or a cancellation; both must pass through untouched.
#[test]
fn apply_result_limit_passes_non_success_results_through_unchanged() {
    assert!(matches!(
        apply_result_limit(FileGlobV2Result::Error("boom".to_owned()), Some(1)),
        FileGlobV2Result::Error(message) if message == "boom"
    ));
    assert!(matches!(
        apply_result_limit(FileGlobV2Result::Cancelled, Some(1)),
        FileGlobV2Result::Cancelled
    ));
}
