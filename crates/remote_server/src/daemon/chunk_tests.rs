use std::fs;
use std::path::Path;

use super::*;

fn write(root: &Path, rel: &str, body: impl AsRef<[u8]>) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "alpha\nbeta\n");
    write(dir.path(), "src/b.rs", "fn b() {}\n");
    write(dir.path(), "src/c.rs", "fn c() {}\n");
    dir
}

fn paths(s: &Scan) -> Vec<&str> {
    s.manifest.iter().map(|m| m.path.as_str()).collect()
}

fn hashes_of<'a>(s: &'a Scan, path: &str) -> Vec<&'a str> {
    s.chunks
        .iter()
        .filter(|c| c.path == path)
        .map(|c| c.content_hash.as_str())
        .collect()
}

#[test]
fn the_same_tree_prints_identical_output() {
    let dir = repo();
    let a = run_index_scan(dir.path(), None).unwrap();
    let b = run_index_scan(dir.path(), None).unwrap();
    assert_eq!(a, b);
    assert!(a.contains("\"root_hash\""));
}

#[test]
fn file_and_fragment_hashes_are_sha256_of_the_bytes() {
    let dir = repo();
    let s = scan(dir.path()).unwrap();
    // sha256("alpha\nbeta\n") and sha256("alpha\nbeta") (the fragment stops before the newline).
    assert_eq!(s.manifest[0].path, "a.txt");
    assert_eq!(s.manifest[0].file_hash, sha256_hex(b"alpha\nbeta\n"));
    assert_eq!(s.chunks[0].content_hash, sha256_hex(b"alpha\nbeta"));
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!((s.chunks[0].start_line, s.chunks[0].end_line), (0, 1));
    assert_eq!((s.chunks[0].byte_start, s.chunks[0].byte_end), (0, 10));
}

#[test]
fn changing_one_file_changes_only_its_hashes() {
    let dir = repo();
    let before = scan(dir.path()).unwrap();
    write(dir.path(), "src/b.rs", "fn b() { 1 }\n");
    let after = scan(dir.path()).unwrap();
    assert_ne!(before.root_hash, after.root_hash);
    assert_eq!(paths(&before), paths(&after));
    for (x, y) in before.manifest.iter().zip(&after.manifest) {
        assert_eq!(
            x.file_hash == y.file_hash,
            x.path != "src/b.rs",
            "{}",
            x.path
        );
    }
    assert_eq!(hashes_of(&before, "a.txt"), hashes_of(&after, "a.txt"));
    assert_eq!(
        hashes_of(&before, "src/c.rs"),
        hashes_of(&after, "src/c.rs")
    );
    assert_ne!(
        hashes_of(&before, "src/b.rs"),
        hashes_of(&after, "src/b.rs")
    );

    let d = diff(&before, &after);
    assert!(!d.unchanged);
    assert_eq!(d.changed_files, vec!["src/b.rs"]);
    assert!(d.added_files.is_empty() && d.removed_files.is_empty());
    assert_eq!(d.new_content_hashes, hashes_of(&after, "src/b.rs"));
}

#[test]
fn an_unchanged_tree_diffs_to_nothing() {
    let dir = repo();
    let s = scan(dir.path()).unwrap();
    let d = diff(&s, &scan(dir.path()).unwrap());
    assert!(d.unchanged);
    assert!(d.new_content_hashes.is_empty() && d.changed_files.is_empty());
}

#[test]
fn known_file_round_trips_through_the_command() {
    let dir = repo();
    let out = tempfile::tempdir().unwrap();
    let known = out.path().join("known.json");
    fs::write(&known, run_index_scan(dir.path(), None).unwrap()).unwrap();
    let same = run_index_scan(dir.path(), Some(&known)).unwrap();
    assert!(same.contains("\"unchanged\": true"), "{same}");
    write(dir.path(), "src/c.rs", "fn c() { 2 }\n");
    let changed = run_index_scan(dir.path(), Some(&known)).unwrap();
    assert!(changed.contains("\"unchanged\": false") && changed.contains("src/c.rs"));
    assert!(run_index_scan(dir.path(), Some(&out.path().join("nope.json"))).is_err());
}

#[test]
fn gitignore_is_respected_without_a_git_repo() {
    let dir = repo();
    write(dir.path(), ".gitignore", "ignored.txt\nbuild/\n");
    write(dir.path(), "ignored.txt", "secret\n");
    write(dir.path(), "build/out.txt", "artifact\n");
    write(dir.path(), ".cursorignore", "cursor.txt\n");
    write(dir.path(), "cursor.txt", "x\n");
    write(dir.path(), ".git/config", "[core]\n");
    let s = scan(dir.path()).unwrap();
    let p = paths(&s);
    assert!(p.contains(&".gitignore") && p.contains(&"a.txt"));
    for hidden in ["ignored.txt", "build/out.txt", "cursor.txt", ".git/config"] {
        assert!(!p.contains(&hidden), "{hidden} should be skipped: {p:?}");
    }
}

#[test]
fn binary_oversize_and_empty_files_are_skipped() {
    let dir = repo();
    write(dir.path(), "bin.dat", [0x7f, b'E', b'L', b'F', 0, 1, 2, 3]);
    write(dir.path(), "latin1.txt", [b'c', b'a', b'f', 0xe9]);
    write(dir.path(), "empty.txt", "");
    write(dir.path(), "big.txt", vec![b'a'; MAX_FILE_BYTES as usize]);
    write(
        dir.path(),
        "justunder.txt",
        vec![b'a'; MAX_FILE_BYTES as usize - 1],
    );
    let s = scan(dir.path()).unwrap();
    let p = paths(&s);
    for skipped in ["bin.dat", "latin1.txt", "empty.txt", "big.txt"] {
        assert!(!p.contains(&skipped), "{skipped} should be skipped: {p:?}");
    }
    assert!(p.contains(&"justunder.txt"));
}

#[test]
fn a_rename_keeps_the_content_hashes_and_moves_the_path() {
    let dir = repo();
    let before = scan(dir.path()).unwrap();
    fs::rename(
        dir.path().join("src/b.rs"),
        dir.path().join("src/renamed.rs"),
    )
    .unwrap();
    let after = scan(dir.path()).unwrap();
    assert_eq!(
        hashes_of(&before, "src/b.rs"),
        hashes_of(&after, "src/renamed.rs")
    );
    let d = diff(&before, &after);
    assert_eq!(d.added_files, vec!["src/renamed.rs"]);
    assert_eq!(d.removed_files, vec!["src/b.rs"]);
    assert!(d.changed_files.is_empty());
    // Nothing to embed: the vector for that fragment text is already known.
    assert!(d.new_content_hashes.is_empty());
    assert_ne!(before.root_hash, after.root_hash);
}

#[test]
fn a_deleted_file_is_reported_and_leaves_no_new_hashes() {
    let dir = repo();
    let before = scan(dir.path()).unwrap();
    fs::remove_file(dir.path().join("a.txt")).unwrap();
    let after = scan(dir.path()).unwrap();
    let d = diff(&before, &after);
    assert_eq!(d.removed_files, vec!["a.txt"]);
    assert!(d.new_content_hashes.is_empty() && d.added_files.is_empty());
    assert!(hashes_of(&after, "a.txt").is_empty());
}

#[test]
fn chunk_boundaries_equal_the_shared_chunker_on_a_long_file() {
    // 450 short lines: three fragments, lines 0-199, 200-399, 400-449 (the app's naive chunker).
    let dir = tempfile::tempdir().unwrap();
    let body: String = (0..450).map(|n| format!("l{n}\n")).collect();
    write(dir.path(), "long.txt", &body);
    let s = scan(dir.path()).unwrap();
    let lines: Vec<(usize, usize)> = s
        .chunks
        .iter()
        .map(|c| (c.start_line, c.end_line))
        .collect();
    assert_eq!(lines, vec![(0, 199), (200, 399), (400, 449)]);
    for c in &s.chunks {
        assert_eq!(
            c.content_hash,
            sha256_hex(body[c.byte_start..c.byte_end].as_bytes())
        );
    }
}

#[test]
fn a_missing_repo_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(scan(&dir.path().join("nope")).is_err());
}
