use std::fs;
use std::path::PathBuf;

use super::*;
use crate::terminal::shell::ShellType;
use tempfile::TempDir;

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

/// Conversation-pane fallback (`docs/design/moth-parliament.md` step 2): with no
/// session, file_glob must still find files under the target directory and
/// respect the pattern, reporting matches in the same shape the shell path uses
/// -- `FileGlobV2Result::Success` with each match's absolute path. Fails if the
/// pattern stops being applied (e.g. every file is returned regardless of
/// extension) or if this starts requiring a session again.
#[test]
fn file_glob_filesystem_sync_respects_pattern() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("keep.rs"), "").expect("write fixture file");
    fs::write(dir.path().join("skip.txt"), "").expect("write fixture file");

    let result = file_glob_filesystem_sync(
        &["*.rs".to_string()],
        dir.path().to_str().expect("tempdir path is utf8"),
    )
    .expect("file_glob_filesystem_sync should succeed against a real directory");

    assert_eq!(
        result,
        FileGlobV2Result::Success {
            matched_files: vec![FileGlobV2Match {
                file_path: dir.path().join("keep.rs").to_string_lossy().into_owned(),
            }],
            warnings: None,
        }
    );
}

/// The tool's own documented contract (`tools/search.rs`'s `glob_parameters`)
/// gives `"**/*.rs"` as an example pattern, so the filesystem fallback must
/// recurse into subdirectories, not just match the top-level directory. Fails
/// if the walk stops recursing or if `**` stops crossing directory boundaries.
#[test]
fn file_glob_filesystem_sync_recursive_pattern_matches_nested_file() {
    let dir = TempDir::new().expect("tempdir");
    let nested = dir.path().join("src").join("nested");
    fs::create_dir_all(&nested).expect("mkdir -p");
    fs::write(nested.join("deep.rs"), "").expect("write fixture file");

    let result = file_glob_filesystem_sync(
        &["**/*.rs".to_string()],
        dir.path().to_str().expect("tempdir path is utf8"),
    )
    .expect("file_glob_filesystem_sync should succeed against a real directory");

    assert_eq!(
        result,
        FileGlobV2Result::Success {
            matched_files: vec![FileGlobV2Match {
                file_path: nested.join("deep.rs").to_string_lossy().into_owned(),
            }],
            warnings: None,
        }
    );
}

/// The shell path reports "no matches" as `FileGlobV2Result::Success` with an
/// empty file list, never as an `Error` (`run_find_command`'s success-with-
/// empty-stdout branch). The filesystem fallback must match that -- a
/// different representation here would look like it works while quietly
/// changing what the agent concludes about the search. Fails if a clean "no
/// matches" run starts returning `FileGlobV2Result::Error` instead.
#[test]
fn file_glob_filesystem_sync_reports_no_matches_as_success_with_empty_list() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("file.txt"), "").expect("write fixture file");

    let result = file_glob_filesystem_sync(
        &["*.rs".to_string()],
        dir.path().to_str().expect("tempdir path is utf8"),
    )
    .expect("file_glob_filesystem_sync should succeed even when nothing matches");

    assert_eq!(
        result,
        FileGlobV2Result::Success {
            matched_files: vec![],
            warnings: None,
        }
    );
}

/// Wiring test for the `Some(session) = session else { .. }` branch in
/// `run_file_glob`: with no session, the top-level entry point must reach the
/// filesystem path rather than erroring out immediately. Fails if that branch
/// goes back to returning `anyhow::anyhow!("No session provided to file_glob")`
/// unconditionally.
#[tokio::test]
async fn run_file_glob_dispatches_to_filesystem_when_there_is_no_session() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("a.rs"), "").expect("write fixture file");

    let result = run_file_glob(
        vec!["*.rs".to_string()],
        dir.path().to_string_lossy().into_owned(),
        None,
        None,
    )
    .await
    .expect("run_file_glob should succeed with no session");

    let FileGlobV2Result::Success { matched_files, .. } = result else {
        panic!("expected FileGlobV2Result::Success, got {result:?}");
    };
    assert_eq!(matched_files.len(), 1);
}
