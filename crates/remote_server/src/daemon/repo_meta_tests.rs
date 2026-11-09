use super::*;

fn names(children: &[TreeChild]) -> Vec<(&str, bool, bool)> {
    children
        .iter()
        .map(|c| (c.name.as_str(), c.is_dir, c.ignored))
        .collect()
}

#[test]
fn the_repo_root_is_the_nearest_ancestor_with_a_dot_git() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    assert_eq!(find_repo_root(&root.join("a/b")), None);
    std::fs::create_dir(root.join("a/.git")).unwrap();
    assert_eq!(find_repo_root(&root.join("a/b")), Some(root.join("a")));
    // A worktree or submodule has a `.git` FILE.
    std::fs::write(root.join("a/b/.git"), "gitdir: x").unwrap();
    assert_eq!(find_repo_root(&root.join("a/b")), Some(root.join("a/b")));
}

#[test]
fn listing_applies_gitignore_and_flags_dot_git_only_inside_a_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::create_dir(root.join("build")).unwrap();
    std::fs::write(root.join(".gitignore"), "build/\n*.tmp\n").unwrap();
    std::fs::write(root.join("keep.rs"), "").unwrap();
    std::fs::write(root.join("junk.tmp"), "").unwrap();

    let in_git = list_dir(&root, true, false).unwrap();
    assert_eq!(
        names(&in_git),
        [
            (".git", true, true),
            ("build", true, true),
            (".gitignore", false, false),
            ("junk.tmp", false, true),
            ("keep.rs", false, false),
        ]
    );
    let outside = list_dir(&root, false, false).unwrap();
    assert!(outside.iter().all(|c| !c.ignored));
    // Inside an ignored directory everything is ignored, whatever the rules say.
    let inside_ignored = list_dir(&root, true, true).unwrap();
    assert!(inside_ignored.iter().all(|c| c.ignored));
}

#[cfg(unix)]
#[test]
fn symlinked_directories_are_omitted_and_symlinked_files_are_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir(root.join("real")).unwrap();
    std::fs::write(root.join("f"), "").unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("dirlink")).unwrap();
    std::os::unix::fs::symlink(root.join("f"), root.join("filelink")).unwrap();
    std::os::unix::fs::symlink(root.join("nowhere"), root.join("broken")).unwrap();
    let got = list_dir(&root, false, false).unwrap();
    assert_eq!(
        names(&got),
        [
            ("real", true, false),
            ("broken", false, false),
            ("f", false, false),
            ("filelink", false, false),
        ]
    );
}

#[test]
fn a_git_walk_is_breadth_first_and_skips_ignored_directories_a_plain_walk_is_one_level() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::write(root.join(".gitignore"), "out/\n").unwrap();
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::create_dir_all(root.join("out/deep")).unwrap();
    std::fs::write(root.join("a/b/f"), "").unwrap();

    let git = walk(&root, true);
    let keys: Vec<_> = git.keys().cloned().collect();
    assert_eq!(
        keys,
        [
            lossy(&root),
            lossy(&root.join("a")),
            lossy(&root.join("a/b"))
        ]
    );
    let plain = walk(&root, false);
    assert_eq!(plain.keys().cloned().collect::<Vec<_>>(), [lossy(&root)]);
}
