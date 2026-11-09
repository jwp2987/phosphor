use std::path::Path;

use super::*;
use crate::shellcmd::git::test_runner::{git, git_available, repo_with};

fn watched(root: &Path) -> Watched {
    Watched {
        root: root.to_path_buf(),
        users: 1,
        git_dirs: vec![root.join(".git")],
        held: BTreeSet::new(),
        unwatched: false,
    }
}

#[test]
fn only_head_moves_the_index_refs_and_the_head_log_matter_inside_the_git_dir() {
    for rel in [
        "HEAD",
        "index",
        "packed-refs",
        "logs/HEAD",
        "refs/heads/main",
        "refs/remotes/origin/feature/x",
    ] {
        assert_eq!(git_internal(Path::new(rel)), Some(Change::All), "{rel}");
    }
    for rel in [
        "index.lock",
        "refs/heads/main.lock",
        "objects/ab/cdef",
        "COMMIT_EDITMSG",
        "FETCH_HEAD",
        "logs/refs/heads/main",
    ] {
        assert_eq!(git_internal(Path::new(rel)), None, "{rel}");
    }
}

#[test]
fn a_worktree_path_is_a_file_change_unless_it_is_a_directory_or_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "").unwrap();
    let w = watched(root);
    assert_eq!(
        w.classify_path(&root.join("src/a.rs")),
        Some(Change::Files(vec!["src/a.rs".into()]))
    );
    // A directory (new or renamed): the watch set must be re-planned.
    assert_eq!(w.classify_path(&root.join("src")), Some(Change::All));
    assert_eq!(w.classify_path(&root.join(".gitignore")), Some(Change::All));
    // The root itself, a nested repository's .git and anything outside are not ours.
    assert_eq!(w.classify_path(root), None);
    assert_eq!(w.classify_path(&root.join("sub/.git/index")), None);
    assert_eq!(w.classify_path(Path::new("/elsewhere/x")), None);
    // The repo's own git dir.
    assert_eq!(w.classify_path(&root.join(".git/index")), Some(Change::All));
    assert_eq!(w.classify_path(&root.join(".git/index.lock")), None);
}

#[test]
fn many_files_in_one_batch_become_a_whole_recompute() {
    let dir = tempfile::tempdir().unwrap();
    let mut repos = RepoWatches::default();
    repos.repos.insert("r".into(), watched(dir.path()));
    let few: Vec<_> = (0..3).map(|i| dir.path().join(format!("f{i}"))).collect();
    let got = repos.classify(&few, FsKind::Modify);
    assert_eq!(got.len(), 1);
    assert!(matches!(&got[0].1, Change::Files(f) if f.len() == 3));
    let many: Vec<_> = (0..=MAX_FILE_DELTAS)
        .map(|i| dir.path().join(format!("f{i}")))
        .collect();
    assert_eq!(repos.classify(&many, FsKind::Modify)[0].1, Change::All);
    assert_eq!(repos.classify(&[], FsKind::Rescan)[0].1, Change::All);
    assert!(
        repos
            .classify(&[Path::new("/x/y").to_path_buf()], FsKind::Modify)
            .is_empty()
    );
}

#[test]
fn an_unwatched_repo_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut repos = RepoWatches::default();
    let mut w = watched(dir.path());
    w.unwatched = true;
    repos.repos.insert("r".into(), w);
    assert!(
        repos
            .classify(&[dir.path().join("f")], FsKind::Modify)
            .is_empty()
    );
    assert!(repos.classify(&[], FsKind::Rescan).is_empty());
    assert!(repos.is_unwatched("r"));
}

#[test]
fn the_plan_covers_the_root_git_dir_refs_logs_and_every_tracked_directory() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("top.txt", "t"), ("a/b/deep.txt", "d"), ("a/x.txt", "x")]);
    git(dir.path(), &["branch", "feature/one"]);
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let plan = plan_watches(&root);
    assert!(!plan.truncated);
    assert_eq!(plan.git_dirs, vec![root.join(".git")]);
    for expected in [
        root.clone(),
        root.join(".git"),
        root.join(".git/logs"),
        root.join(".git/refs"),
        root.join(".git/refs/heads"),
        root.join(".git/refs/heads/feature"),
        root.join("a"),
        root.join("a/b"),
    ] {
        assert!(
            plan.dirs.contains(&expected),
            "{expected:?} in {:?}",
            plan.dirs
        );
    }
}
