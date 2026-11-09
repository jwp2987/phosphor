use super::*;

fn child(name: &str, is_dir: bool, ignored: bool) -> TreeChild {
    TreeChild {
        name: name.into(),
        is_dir,
        ignored,
    }
}

#[test]
fn children_sort_directories_first_then_by_name() {
    let mut v = vec![
        child("b", false, false),
        child("z", true, false),
        child("a", false, false),
        child("c", true, false),
    ];
    sort_children(&mut v);
    let names: Vec<_> = v.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["c", "z", "a", "b"]);
}

#[test]
fn paths_join_with_the_hosts_separator() {
    assert_eq!(join_path("/repo", "a"), "/repo/a");
    assert_eq!(join_path("/", "a"), "/a");
    assert_eq!(join_path("C:\\repo", "a"), "C:\\repo\\a");
    assert_eq!(parent_of("/a/b"), Some("/a"));
    assert_eq!(parent_of("/a"), Some("/"));
    assert_eq!(parent_of("/"), None);
    assert!(is_within("/repo", "/repo/x/y"));
    assert!(is_within("/repo", "/repo"));
    assert!(!is_within("/repo", "/repository"));
}

#[test]
fn a_tree_from_paths_lists_every_ancestor_loaded_and_ignored_dirs_unloaded() {
    let files = vec![
        "a.txt".to_owned(),
        "src/lib.rs".to_owned(),
        "src/deep/x.rs".to_owned(),
    ];
    let ignored = vec!["target/".to_owned(), "debug.log".to_owned()];
    let dirs = tree_from_paths("/repo", &files, &ignored);
    assert_eq!(
        dirs["/repo"],
        vec![
            child("src", true, false),
            child("target", true, true),
            child("a.txt", false, false),
            child("debug.log", false, true),
        ]
    );
    assert_eq!(
        dirs["/repo/src"],
        vec![child("deep", true, false), child("lib.rs", false, false)]
    );
    assert!(!dirs.contains_key("/repo/target"));

    let entries = snapshot_entries("/repo", &dirs);
    // Breadth-first: root, then src, then src/deep.
    let parents: Vec<_> = entries
        .iter()
        .map(|e| e.parent_path_to_replace.as_str())
        .collect();
    assert_eq!(parents, ["/repo", "/repo/src", "/repo/src/deep"]);
    let root_nodes = &entries[0].subtree_metadata;
    let proto::repo_node_metadata::Node::Directory(src) = root_nodes[0].node.clone().unwrap()
    else {
        panic!("src is a directory");
    };
    assert!(src.loaded && !src.ignored);
    let proto::repo_node_metadata::Node::Directory(target) = root_nodes[1].node.clone().unwrap()
    else {
        panic!("target is a directory");
    };
    assert!(!target.loaded && target.ignored);
    let proto::repo_node_metadata::Node::File(f) = root_nodes[2].node.clone().unwrap() else {
        panic!("a.txt is a file");
    };
    assert_eq!(f.extension.as_deref(), Some("txt"));
}

#[test]
fn a_directory_diff_removes_vanished_and_retyped_entries_and_adds_new_ones() {
    let old = vec![
        child("gone", false, false),
        child("same", false, false),
        child("flip", true, false),
        child("ign", false, false),
    ];
    let new = vec![
        child("same", false, false),
        child("flip", false, false),
        child("new", true, false),
        child("ign", false, true),
    ];
    let (removed, added) = diff_dir("/r", &old, &new);
    assert_eq!(removed, ["/r/gone", "/r/flip"]);
    let added_paths: Vec<String> = added
        .iter()
        .map(|n| match n.node.clone().unwrap() {
            proto::repo_node_metadata::Node::Directory(d) => d.path,
            proto::repo_node_metadata::Node::File(f) => f.path,
        })
        .collect();
    assert_eq!(added_paths, ["/r/flip", "/r/new", "/r/ign"]);
}

#[test]
fn list_records_parse_types_and_drop_symlinked_dirs_and_specials() {
    let raw = b"dd src\0ff a.txt\0ld linkdir\0lf linkfile\0pf fifo\0ff with space.txt\0dd .git\0";
    let got = parse_list_records(raw);
    let names: Vec<_> = got.iter().map(|c| (c.name.as_str(), c.is_dir)).collect();
    assert_eq!(
        names,
        [
            ("src", true),
            ("a.txt", false),
            ("linkfile", false),
            ("with space.txt", false),
            (".git", true)
        ]
    );
}

#[test]
fn finish_children_marks_git_and_ignored_names_only_inside_a_repo() {
    let listing = vec![
        child("b", false, false),
        child(".git", true, false),
        child("a", true, false),
        child("x.log", false, false),
    ];
    let ignored: BTreeSet<String> = ["x.log".to_owned()].into();
    let in_git = finish_children(listing.clone(), &ignored, true);
    assert_eq!(
        in_git,
        vec![
            child(".git", true, true),
            child("a", true, false),
            child("b", false, false),
            child("x.log", false, true)
        ]
    );
    let outside = finish_children(listing, &ignored, false);
    assert!(outside.iter().all(|c| !c.ignored));
}

#[test]
fn path_lists_accept_nul_or_newline_and_drop_a_cut_final_record() {
    assert_eq!(parse_path_list(b"a\0b/c\0", false), ["a", "b/c"]);
    assert_eq!(parse_path_list(b"a\0b/c", true), ["a"]);
    assert_eq!(parse_path_list(b"a\r\nb\r\n", false), ["a", "b"]);
    assert!(parse_path_list(b"", false).is_empty());
}

#[test]
fn builders_quote_the_path_for_each_shell() {
    let posix = list_dir_command(ShellKind::Posix, "/a b/it's");
    assert!(posix.contains("cd -- '/a b/it'\\''s'"), "{posix}");
    assert!(posix.contains("-printf '%y%Y %f\\0'"), "{posix}");
    assert!(posix.contains("ls -1Ap"), "{posix}");
    let pwsh = list_dir_command(ShellKind::Pwsh, "C:\\a b\\it's");
    assert!(pwsh.contains("-LiteralPath 'C:\\a b\\it''s'"), "{pwsh}");
    assert!(canonical_dir_command(ShellKind::Posix, "/x").contains("pwd -P"));
    assert!(repo_files_command(ShellKind::Posix, "/r").contains("head -c 4194304"));
    assert!(ignored_children_command(ShellKind::Posix, "/r").contains("check-ignore -z --stdin"));
    assert!(repo_ignored_command(ShellKind::Pwsh, "C:\\r").contains("--directory"));
}

#[test]
fn a_navigation_answer_needs_a_directory_and_optionally_a_git_root() {
    let nonce = "abc123";
    let ok = format!(
        "@@PHOSPHOR-{nonce} BEGIN canon\n/work/p\n@@PHOSPHOR-{nonce} END canon exit=0\n@@PHOSPHOR-{nonce} BEGIN git\n/work\n@@PHOSPHOR-{nonce} END git exit=0\n"
    );
    let frame = Frame::parse(ok.as_bytes(), nonce).unwrap();
    assert_eq!(
        parse_nav_target(&frame).unwrap(),
        NavTarget {
            dir: "/work/p".into(),
            git_root: Some("/work".into())
        }
    );
    let no_git = format!(
        "@@PHOSPHOR-{nonce} BEGIN canon\n/work/p\n@@PHOSPHOR-{nonce} END canon exit=0\n@@PHOSPHOR-{nonce} BEGIN git\n@@PHOSPHOR-{nonce} END git exit=128\n"
    );
    let frame = Frame::parse(no_git.as_bytes(), nonce).unwrap();
    assert_eq!(parse_nav_target(&frame).unwrap().git_root, None);
    let bad = format!("@@PHOSPHOR-{nonce} BEGIN canon\n@@PHOSPHOR-{nonce} END canon exit=2\n");
    let frame = Frame::parse(bad.as_bytes(), nonce).unwrap();
    assert!(parse_nav_target(&frame).is_err());
}

#[test]
fn a_directory_is_ignored_when_its_parents_listing_says_so() {
    let mut dirs = BTreeMap::new();
    dirs.insert(
        "/r".to_owned(),
        vec![child("target", true, true), child("src", true, false)],
    );
    assert!(dir_is_ignored(&dirs, "/r/target"));
    assert!(!dir_is_ignored(&dirs, "/r/src"));
    assert!(!dir_is_ignored(&dirs, "/r"));
    assert!(!dir_is_ignored(&dirs, "/r/unknown"));
}

#[test]
fn a_refresh_diffs_shared_directories_and_drops_removed_subtrees() {
    let mut old = tree_from_paths(
        "/r",
        &["a.txt".to_owned(), "gone/x".to_owned(), "keep/y".to_owned()],
        &["build/".to_owned()],
    );
    // A lazily loaded ignored directory the fresh git listing knows nothing about.
    old.insert("/r/build".to_owned(), vec![child("o", false, true)]);
    let new = tree_from_paths(
        "/r",
        &[
            "a.txt".to_owned(),
            "keep/y".to_owned(),
            "keep/z".to_owned(),
            "fresh/q".to_owned(),
        ],
        &["build/".to_owned()],
    );
    let (removed, updates) = apply_refresh(&mut old, &new);
    assert_eq!(removed, ["/r/gone"]);
    assert!(
        !old.contains_key("/r/gone"),
        "the removed subtree is forgotten"
    );
    assert!(
        old.contains_key("/r/build"),
        "an unlisted cached directory is left alone"
    );
    let parents: Vec<_> = updates
        .iter()
        .map(|u| u.parent_path_to_replace.as_str())
        .collect();
    assert_eq!(parents, ["/r", "/r/keep"]);
    // Refreshing again with the same listing says nothing.
    let (removed, updates) = apply_refresh(&mut old, &new);
    assert!(removed.is_empty() && updates.is_empty());
}
