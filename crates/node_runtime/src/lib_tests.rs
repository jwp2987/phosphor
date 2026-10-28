//! Tests for archive extraction hardening (refs #1076).

use std::io::{Cursor, Write};

use futures::executor::block_on;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::*;

/// Builds an in-memory zip: a normal file, a `..` escape, rooted names (Unix
/// absolute, backslash-rooted, UNC, drive-qualified), a symlink pointing
/// outside the destination, and a setuid regular file.
fn hostile_zip() -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();

    writer.start_file("pkg/ok.txt", options).unwrap();
    writer.write_all(b"ok").unwrap();

    writer.start_file("../escape.txt", options).unwrap();
    writer.write_all(b"escape").unwrap();

    for rooted in [
        "/abs.txt",
        "\\backslash-abs.txt",
        "\\\\server\\share\\unc.txt",
        "C:\\drive.txt",
        "C:drive-relative.txt",
    ] {
        writer.start_file(rooted, options).unwrap();
        writer.write_all(b"rooted").unwrap();
    }

    writer
        .add_symlink("pkg/link", "../../outside", options)
        .unwrap();

    // S_IFREG | setuid | 0o755, in the high 16 bits as Unix zips store it.
    let setuid = SimpleFileOptions::default().external_attributes(0o104755 << 16);
    writer.start_file("pkg/suid", setuid).unwrap();
    writer.write_all(b"#!/bin/sh\n").unwrap();

    writer.finish().unwrap().into_inner()
}

#[test]
fn extract_zip_confines_entries_and_skips_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("dest");
    std::fs::create_dir_all(&dest).unwrap();

    block_on(extract_zip(&hostile_zip(), &dest, None::<fn(&str) -> bool>)).unwrap();

    assert_eq!(std::fs::read(dest.join("pkg/ok.txt")).unwrap(), b"ok");
    assert!(!root.path().join("escape.txt").exists());
    assert!(!dest.join("escape.txt").exists());
    // Rooted names are not extracted at all -- not even re-rooted inside dest,
    // which is what zip 9's enclosed_name alone would do.
    assert!(!dest.join("abs.txt").exists());
    assert!(!root.path().join("outside").exists());
    // The symlink entry is neither created as a link nor written as a file.
    // Unix only: zips written on Windows record System::Dos, whose attributes
    // cannot mark an entry as a symlink, so there it is an ordinary file.
    #[cfg(unix)]
    assert!(std::fs::symlink_metadata(dest.join("pkg/link")).is_err());

    // Nothing else was written anywhere under root.
    #[allow(unused_mut)]
    let mut expected = vec![dest.join("pkg/ok.txt"), dest.join("pkg/suid")];
    #[cfg(not(unix))]
    expected.push(dest.join("pkg/link"));
    expected.sort();
    assert_eq!(files_under(root.path()), expected);
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if std::fs::symlink_metadata(&path).unwrap().is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn absolute_zip_entry_names_are_detected() {
    for rooted in [
        "/x",
        "\\x",
        "\\\\server\\share\\x",
        "//server/share/x",
        "C:\\x",
        "c:/x",
        "C:x",
    ] {
        assert!(is_absolute_zip_entry_name(rooted), "{rooted:?}");
    }
    for relative in ["x", "pkg/x", "./x", "../x", "ab:c"] {
        assert!(!is_absolute_zip_entry_name(relative), "{relative:?}");
    }
}

#[cfg(unix)]
#[test]
fn extract_zip_never_applies_setuid_bits() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    block_on(extract_zip(
        &hostile_zip(),
        root.path(),
        None::<fn(&str) -> bool>,
    ))
    .unwrap();

    let mode = std::fs::metadata(root.path().join("pkg/suid"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o7000, 0, "setuid/setgid/sticky leaked: {mode:o}");
}

#[cfg(unix)]
#[test]
fn extract_zip_filter_ignores_symlink_named_like_binary() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink("bin/tool", "/usr/bin/tool", SimpleFileOptions::default())
        .unwrap();
    let data = writer.finish().unwrap().into_inner();

    let root = tempfile::tempdir().unwrap();
    let result = block_on(extract_zip(
        &data,
        root.path(),
        Some(|name: &str| name.ends_with("tool")),
    ));
    assert!(result.is_err(), "a symlink must not satisfy the filter");
    assert!(std::fs::symlink_metadata(root.path().join("bin/tool")).is_err());
}
