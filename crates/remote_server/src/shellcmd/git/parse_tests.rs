//! Golden tests for the pure git builders and parsers.

use std::path::PathBuf;

use super::*;

fn nul(parts: &[&str]) -> String {
    let mut out = parts.join("\0");
    out.push('\0');
    out
}

#[test]
fn porcelain_v2_changed_entries_and_staged_columns() {
    let out = nul(&[
        "1 .M N... 100644 100644 100644 aaaa bbbb src/a b.rs",
        "1 M. N... 100644 100644 100644 aaaa bbbb staged.rs",
        "1 MM N... 100644 100644 100644 aaaa bbbb both.rs",
        "1 A. N... 000000 100644 100644 0000 bbbb new.rs",
        "1 .D N... 100644 100644 000000 aaaa aaaa gone.rs",
    ]);
    let entries = parse_status_entries(&out).unwrap();
    let got: Vec<_> = entries
        .iter()
        .map(|e| (e.path.to_str().unwrap(), e.status.clone(), e.staged))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "src/a b.rs",
                GitFileStatus::Modified,
                FileStagedState::Unstaged
            ),
            (
                "staged.rs",
                GitFileStatus::Modified,
                FileStagedState::Staged
            ),
            (
                "both.rs",
                GitFileStatus::Modified,
                FileStagedState::PartiallyStaged
            ),
            ("new.rs", GitFileStatus::New, FileStagedState::Staged),
            ("gone.rs", GitFileStatus::Deleted, FileStagedState::Unstaged),
        ]
    );
}

#[test]
fn porcelain_v2_rename_copy_unmerged_untracked_ignored() {
    let out = nul(&[
        "# branch.oid abc",
        "2 R. N... 100644 100644 100644 aaaa bbbb R100 new name.rs",
        "old name.rs",
        "2 C. N... 100644 100644 100644 aaaa bbbb C75 copy.rs",
        "orig.rs",
        "u UU N... 100644 100644 100644 100644 h1 h2 h3 conflict file.rs",
        "? untracked dir/x.txt",
        "! ignored.log",
        "1 M. N... 100644 100644 100644 aaaa bbbb after.rs",
    ]);
    let entries = parse_status_entries(&out).unwrap();
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0].path, PathBuf::from("new name.rs"));
    assert_eq!(
        entries[0].status,
        GitFileStatus::Renamed {
            old_path: "old name.rs".into()
        }
    );
    assert_eq!(entries[0].staged, FileStagedState::Staged);
    assert_eq!(
        entries[1].status,
        GitFileStatus::Copied {
            old_path: "orig.rs".into()
        }
    );
    assert_eq!(entries[2].path, PathBuf::from("conflict file.rs"));
    assert_eq!(entries[2].status, GitFileStatus::Conflicted);
    assert_eq!(entries[2].staged, FileStagedState::Unstaged);
    assert_eq!(entries[3].path, PathBuf::from("untracked dir/x.txt"));
    assert_eq!(entries[3].status, GitFileStatus::Untracked);
    // The rename's old-path token must not be read as its own entry.
    assert_eq!(entries[4].path, PathBuf::from("after.rs"));
}

#[test]
fn porcelain_v2_empty_and_malformed_inputs() {
    assert!(parse_status_entries("").unwrap().is_empty());
    assert!(
        parse_status_entries(&nul(&["1 .M short"]))
            .unwrap()
            .is_empty()
    );
    assert!(parse_status_entries(&nul(&["?"])).unwrap().is_empty());
    let bare = parse_status(&nul(&["? a.txt"])).unwrap();
    assert_eq!(
        bare,
        vec![(PathBuf::from("a.txt"), GitFileStatus::Untracked)]
    );
}

#[test]
fn status_code_mapping_defaults_to_modified() {
    for (code, want) in [
        (".M", GitFileStatus::Modified),
        ("M.", GitFileStatus::Modified),
        (".A", GitFileStatus::New),
        ("AM", GitFileStatus::New),
        ("AD", GitFileStatus::Deleted),
        ("T.", GitFileStatus::Modified),
    ] {
        assert_eq!(GitFileStatus::try_from(code).unwrap(), want, "{code}");
    }
    assert_eq!(
        FileStagedState::from_status_code("x"),
        FileStagedState::Unstaged
    );
}

#[test]
fn branch_headers_tracking() {
    let out = nul(&[
        "# branch.oid 0123abcd",
        "# branch.head feature/x",
        "# branch.upstream origin/feature/x",
        "# branch.ab +3 -12",
        "1 .M N... 100644 100644 100644 aaaa bbbb a.rs",
    ]);
    let h = parse_branch_headers(&out);
    assert_eq!(h.oid.as_deref(), Some("0123abcd"));
    assert_eq!(h.head.as_deref(), Some("feature/x"));
    assert_eq!(h.upstream.as_deref(), Some("origin/feature/x"));
    assert_eq!(h.ahead_behind, Some((3, 12)));
    assert!(!h.is_unborn() && !h.is_detached());
}

#[test]
fn branch_headers_detached_head() {
    let h = parse_branch_headers("# branch.oid 0123abcd\n# branch.head (detached)\n");
    assert!(h.is_detached());
    assert_eq!(h.head, None);
    assert_eq!(h.upstream, None);
    assert_eq!(h.ahead_behind, None);
}

#[test]
fn branch_headers_unborn_branch() {
    let h = parse_branch_headers("# branch.oid (initial)\n# branch.head main\n");
    assert!(h.is_unborn());
    assert_eq!(h.head.as_deref(), Some("main"));
}

#[test]
fn branch_headers_gone_upstream_has_no_ab_line() {
    let h = parse_branch_headers(&nul(&[
        "# branch.oid abc",
        "# branch.head main",
        "# branch.upstream origin/main",
    ]));
    assert_eq!(h.upstream.as_deref(), Some("origin/main"));
    assert_eq!(h.ahead_behind, None);
}

#[test]
fn branch_headers_garbage_ab_is_ignored() {
    assert_eq!(parse_branch_headers("# branch.ab nope").ahead_behind, None);
    assert_eq!(parse_branch_headers("# branch.ab +1").ahead_behind, None);
}

#[test]
fn shortstat_variants() {
    assert_eq!(
        parse_shortstat(" 1 file changed, 2 insertions(+), 17 deletions(-)\n"),
        Some((1, 2, 17))
    );
    assert_eq!(
        parse_shortstat(" 3 files changed, 5 insertions(+)"),
        Some((3, 5, 0))
    );
    assert_eq!(
        parse_shortstat(" 1 file changed, 1 deletion(-)"),
        Some((1, 0, 1))
    );
    assert_eq!(parse_shortstat("  \n"), None);
}

#[test]
fn tracking_counts_and_upstream_ref() {
    assert_eq!(parse_tracking_counts("2\t5\n"), Some((2, 5, 0)));
    assert_eq!(parse_tracking_counts("0 0 4"), Some((0, 0, 4)));
    assert_eq!(parse_tracking_counts("x 1"), None);
    assert_eq!(parse_tracking_counts("1"), None);
    assert_eq!(
        parse_upstream_ref("origin/main\n"),
        Some("origin/main".into())
    );
    assert_eq!(parse_upstream_ref("  \n"), None);
}

#[test]
fn numstat_z_sums_and_skips_rename_paths() {
    // 3 added/1 removed in a.rs; binary; rename with 2 added/0 removed.
    let out = "3\t1\ta.rs\0-\t-\tbin.png\02\t0\t\0old.rs\0new.rs\0";
    assert_eq!(sum_numstat_z(out), (5, 1));
    assert_eq!(sum_numstat_z(""), (0, 0));
}

#[test]
fn numstat_text_forms() {
    let out = "3\t1\ta.rs\n-\t-\tbin.png\n\n1\t0\tdir/with space.rs\n";
    let map = parse_numstat_map(out);
    assert_eq!(map.len(), 3);
    assert!(map[&PathBuf::from("bin.png")].is_binary_file);
    assert_eq!(map[&PathBuf::from("a.rs")].lines_added, 3);
    let entries = parse_numstat_entries(out);
    assert_eq!(entries[1].path, "bin.png");
    assert_eq!((entries[1].additions, entries[1].deletions), (0, 0));
    assert_eq!(entries[2].path, "dir/with space.rs");
    assert_eq!(parse_numstat_entries_splitn("1\t2\ta\tb\n")[0].path, "a\tb");
}

#[test]
fn name_status_z_with_rename_and_copy() {
    let out = "M\0a.rs\0A\0b.rs\0D\0c.rs\0R100\0old.rs\0new.rs\0C80\0copy.rs\0orig.rs\0";
    let parsed = parse_name_status(out).unwrap();
    assert_eq!(parsed[0], (PathBuf::from("a.rs"), GitFileStatus::Modified));
    assert_eq!(parsed[1], (PathBuf::from("b.rs"), GitFileStatus::New));
    assert_eq!(parsed[2], (PathBuf::from("c.rs"), GitFileStatus::Deleted));
    assert_eq!(
        parsed[3],
        (
            PathBuf::from("new.rs"),
            GitFileStatus::Renamed {
                old_path: "old.rs".into()
            }
        )
    );
    assert_eq!(
        parsed[4],
        (
            PathBuf::from("copy.rs"),
            GitFileStatus::Copied {
                old_path: "orig.rs".into()
            }
        )
    );
    assert!(parse_name_status("").unwrap().is_empty());
}

#[test]
fn commit_log_groups_numstat_under_commit() {
    let out = "COMMIT:abc123\tfirst\n1\t2\ta.rs\n3\t0\tb.rs\n\nCOMMIT:def456\tsecond\tstill subject\n-\t-\tbin\n";
    let commits = parse_commit_log(out).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].hash, "abc123");
    assert_eq!(
        (
            commits[0].files_changed,
            commits[0].additions,
            commits[0].deletions
        ),
        (2, 4, 2)
    );
    assert_eq!(commits[1].subject, "second\tstill subject");
    assert_eq!(commits[1].files_changed, 1);
    assert!(parse_commit_log("").unwrap().is_empty());
}

#[test]
fn branch_list_dedupes_and_marks_main() {
    let out = "main\nfeature\norigin/main\nHEAD\norigin/HEAD\n(HEAD detached at x)\nfeature\n\n";
    assert_eq!(
        parse_branch_list(out, "origin/main"),
        vec![
            ("main".to_string(), true),
            ("feature".to_string(), false),
            ("origin/main".to_string(), true),
        ]
    );
}

#[test]
fn sha_shape_and_truncation_on_char_boundary() {
    assert!(looks_like_commit_sha("abcdef1"));
    assert!(!looks_like_commit_sha("abcdef"));
    assert!(!looks_like_commit_sha("abcdefg"));
    assert_eq!(truncate_on_char_boundary("héllo", 2), "h");
    assert_eq!(truncate_on_char_boundary("héllo", 3), "hé");
    assert_eq!(truncate_on_char_boundary("abc", 10), "abc");
}

#[test]
fn pr_title_and_create_args() {
    assert_eq!(sanitize_pr_title("  one line \nsecond"), "one line");
    assert_eq!(
        sanitize_pr_title(&"x".repeat(300)).len(),
        MAX_PR_TITLE_BYTES
    );
    assert_eq!(
        gh_pr_create_args("main", None, None),
        ["pr", "create", "--base", "main", "--fill"]
    );
    assert_eq!(
        gh_pr_create_args("main", Some("T\nx"), Some("B")),
        [
            "pr", "create", "--base", "main", "--title", "T", "--body", "B"
        ]
    );
    assert_eq!(
        gh_pr_create_args("main", Some("T"), None),
        ["pr", "create", "--base", "main", "--fill"]
    );
    assert_eq!(
        parse_pr_number_from_url("https://github.com/o/r/pull/123"),
        Some(123)
    );
    assert_eq!(parse_pr_number_from_url("https://github.com/o/r"), None);
}

#[test]
fn gh_and_git_error_classifiers() {
    assert!(is_no_pr_for_branch_error(
        "no pull requests found for branch \"x\""
    ));
    assert!(is_pr_lookup_not_applicable_error("Not a GitHub repository"));
    assert!(!is_pr_lookup_not_applicable_error("HTTP 502"));
    assert!(is_repository_lookup_not_applicable_error(
        "could not determine base repo"
    ));
    assert!(is_gh_auth_error(
        "To get started with GitHub CLI, please run: gh auth login"
    ));
    assert!(is_gh_missing_error(
        "Failed to execute gh command: No such file or directory"
    ));
    assert!(is_unresolvable_head_error(
        "fatal: could not resolve 'HEAD'"
    ));
    assert!(!is_unresolvable_head_error("fatal: pathspec"));
}

#[test]
fn argv_builders() {
    assert_eq!(
        apply_cached_args(false, false, "/p/x.patch"),
        ["apply", "--cached", "/p/x.patch"]
    );
    assert_eq!(
        apply_cached_args(true, true, "-"),
        ["apply", "--cached", "--reverse", "--recount", "-"]
    );
    let paths = vec!["-x".to_string(), "dir/a b".to_string()];
    assert_eq!(
        stage_paths_args(&paths, false),
        ["add", "--", "-x", "dir/a b"]
    );
    assert_eq!(
        stage_paths_args(&paths, true),
        ["restore", "--staged", "--", "-x", "dir/a b"]
    );
    assert_eq!(
        rm_cached_args(&paths),
        ["rm", "--cached", "--", "-x", "dir/a b"]
    );
    assert_eq!(stash_message("App", &paths[..1]), "App: stash -x");
    assert_eq!(stash_message("App", &paths), "App: stash 2 files");
    assert_eq!(
        stash_args("m", &paths[..1]),
        ["stash", "push", "-u", "-m", "m", "--", "-x"]
    );
    assert_eq!(
        STATUS_FULL_ARGS.join(" "),
        "--no-optional-locks status --untracked-files=all --branch --porcelain=2 -z"
    );
    assert_eq!(
        STATUS_CHIP_ARGS.join(" "),
        "--no-optional-locks status --untracked-files=normal --porcelain=2 -z"
    );
}

#[test]
fn operation_probe_script_and_parse() {
    let script = operation_probe_script("/r/it's/.git");
    assert!(script.starts_with(
        "for f in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD rebase-merge rebase-apply index.lock;"
    ));
    assert!(script.contains("'/r/it'\\''s/.git'/\"$f\""));
    assert!(parse_operation_probe("index.lock\n"));
    assert!(!parse_operation_probe(""));
    assert!(!parse_operation_probe("something-else\n"));
}
