use futures::executor::block_on;

use super::*;

fn leftover_temp_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".phosphor-") && n.ends_with(".tmp"))
        .collect()
}

#[test]
fn replaces_existing_content_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "old old old old").unwrap();
    block_on(write_atomic(&path, b"new")).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert!(leftover_temp_files(dir.path()).is_empty());
}

#[test]
fn creates_a_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh.txt");
    block_on(write_atomic(&path, b"hello")).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
}

/// The write is a rename of a new file, not a truncate of the old one: the old inode is
/// untouched, so a reader holding it open (or a crash before the rename) sees the old content.
#[cfg(unix)]
#[test]
fn replaces_by_rename_not_by_truncating_in_place() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "old").unwrap();
    let old_inode = std::fs::metadata(&path).unwrap().ino();
    let held_open = std::fs::File::open(&path).unwrap();
    block_on(write_atomic(&path, b"new")).unwrap();
    assert_ne!(std::fs::metadata(&path).unwrap().ino(), old_inode);
    let mut old_view = String::new();
    std::io::Read::read_to_string(&mut &held_open, &mut old_view).unwrap();
    assert_eq!(
        old_view, "old",
        "the replaced inode must not have been truncated"
    );
}

#[cfg(unix)]
#[test]
fn preserves_the_permission_bits() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.sh");
    std::fs::write(&path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o750)).unwrap();
    block_on(write_atomic(&path, b"#!/bin/sh\necho hi\n")).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o750);
}

/// Editing through a symlink edits the file it points to; the link survives.
#[cfg(unix)]
#[test]
fn writes_through_a_symlink_and_keeps_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.txt");
    let link = dir.path().join("link.txt");
    std::fs::write(&real, "old").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    block_on(write_atomic(&link, b"new")).unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&real).unwrap(), "new");
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "new");
}

#[cfg(unix)]
#[test]
fn writes_through_a_dangling_symlink_by_creating_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("not-yet.txt");
    let link = dir.path().join("link.txt");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    block_on(write_atomic(&link, b"made")).unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&real).unwrap(), "made");
}

/// A rename would detach the other names of a hard-linked file, so those are written in place.
#[cfg(unix)]
#[test]
fn keeps_hard_links_connected() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "old").unwrap();
    std::fs::hard_link(&a, &b).unwrap();
    block_on(write_atomic(&a, b"new")).unwrap();
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "new");
}

/// A failure before the rename leaves the original untouched and no temp file behind.
#[cfg(unix)]
#[test]
fn a_failed_write_leaves_the_original_intact() {
    let dir = tempfile::tempdir().unwrap();
    // A directory at the target path: not a regular file, so the OS refuses the write.
    let target = dir.path().join("dir");
    std::fs::create_dir(&target).unwrap();
    assert!(block_on(write_atomic(&target, b"x")).is_err());
    assert!(target.is_dir());
    assert!(leftover_temp_files(dir.path()).is_empty());
}

#[test]
fn temp_files_are_siblings_with_distinct_names() {
    let target = Path::new("/some/dir/file.rs");
    let a = temp_path_for(target);
    let b = temp_path_for(target);
    assert_ne!(a, b);
    assert_eq!(a.parent(), target.parent());
}
