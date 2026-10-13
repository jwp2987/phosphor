//! #859: settings.toml and user_preferences.json were rewritten in place, so a
//! full disk left them truncated mid-line.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;

use super::{write, write_with};

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn creates_a_new_file_and_missing_parents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("settings.toml");

    write(&path, "font_size = 13.0\n").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "font_size = 13.0\n");
    assert_eq!(entries(path.parent().unwrap()), vec!["settings.toml"]);
}

#[test]
fn replaces_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    fs::write(&path, "a much longer original line that must not survive\n").unwrap();

    write(&path, "short\n").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "short\n");
    assert_eq!(
        entries(dir.path()),
        vec!["settings.toml"],
        "no temp file left"
    );
}

/// The disk-full case: the write fails part-way through. The original must
/// be intact, the error returned, and no temporary file left behind.
#[test]
fn failure_part_way_leaves_the_original_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let original = "[appearance.text]\nfont_size = 13.0\nprefer_low_power_gpu = true\n";
    fs::write(&path, original).unwrap();

    let err = write_with(&path, |file| {
        file.write_all(b"[appearance.text]\nfont_size = 13.0\nprefer_low_power_gpu =")?;
        Err(io::Error::other("No space left on device"))
    })
    .unwrap_err();

    assert_eq!(err.to_string(), "No space left on device");
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(
        entries(dir.path()),
        vec!["settings.toml"],
        "no temp file left"
    );
}

#[test]
fn failure_on_a_new_file_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");

    write_with(&path, |_| Err(io::Error::other("boom"))).unwrap_err();

    assert!(entries(dir.path()).is_empty());
}

#[test]
fn a_directory_in_place_of_the_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    fs::create_dir(&path).unwrap();

    write(&path, "x").unwrap_err();

    assert!(path.is_dir(), "the directory must be left alone");
    assert_eq!(
        entries(dir.path()),
        vec!["settings.toml"],
        "no temp file left"
    );
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    use super::{entries, write};

    #[test]
    fn preserves_permission_bits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("user_preferences.json");
        fs::write(&path, "{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        write(&path, "{\"a\":1}").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    /// Dotfile managers keep config files as symlinks into a repo; the write
    /// must go to the repo file and leave the link in place.
    #[test]
    fn writes_through_a_symlink_and_keeps_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("dotfiles");
        fs::create_dir(&repo).unwrap();
        let real = repo.join("settings.toml");
        fs::write(&real, "old\n").unwrap();
        let config = dir.path().join("config");
        fs::create_dir(&config).unwrap();
        let link = config.join("settings.toml");
        symlink("../dotfiles/settings.toml", &link).unwrap();

        write(&link, "new\n").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "new\n");
        assert_eq!(entries(&repo), vec!["settings.toml"], "no temp file left");
        assert_eq!(entries(&config), vec!["settings.toml"]);
    }

    #[test]
    fn a_dangling_symlink_creates_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.toml");
        let link = dir.path().join("settings.toml");
        symlink(&real, &link).unwrap();

        write(&link, "x\n").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "x\n");
    }

    #[test]
    fn a_symlink_loop_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        symlink(&b, &a).unwrap();
        symlink(&a, &b).unwrap();

        write(&a, "x").unwrap_err();
    }

    /// A read-only file is refused, as `std::fs::write` refused it, even
    /// though the directory would allow a rename over it. (Skipped as root,
    /// where permission bits do not stop writes.)
    #[test]
    fn a_read_only_file_is_refused_and_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "keep\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        if fs::OpenOptions::new().write(true).open(&path).is_ok() {
            return; // running as root
        }

        let err = write(&path, "replaced\n").unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep\n");
        assert_eq!(entries(dir.path()), vec!["settings.toml"]);
    }
}
