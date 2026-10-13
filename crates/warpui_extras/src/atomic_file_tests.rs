//! #859: settings.toml and user_preferences.json were rewritten in place, so a
//! full disk left them truncated mid-line.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;

use super::{is_temp_file_for, should_write_in_place, sweep_stale_temp_files, write, write_with};

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

    /// A writable file in a directory the user cannot create files in: the
    /// temp file cannot be created (EACCES), so the write falls back to an
    /// in-place write rather than failing. (Skipped as root.)
    #[test]
    fn a_writable_file_in_a_read_only_directory_is_written_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        let path = locked.join("settings.toml");
        fs::write(&path, "old\n").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let running_as_root = fs::File::create(locked.join("probe")).is_ok();

        let result = write(&path, "new\n");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if running_as_root {
            return;
        }

        result.unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
        assert_eq!(entries(&locked), vec!["settings.toml"]);
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

#[test]
fn falls_back_in_place_only_for_cannot_rename_errors_on_a_writable_target() {
    use io::ErrorKind::*;

    for kind in [PermissionDenied, ResourceBusy, CrossesDevices] {
        assert!(
            should_write_in_place(&io::Error::from(kind), true),
            "{kind:?}"
        );
        assert!(
            !should_write_in_place(&io::Error::from(kind), false),
            "{kind:?} on a missing or unwritable target must not fall back"
        );
    }
    for kind in [
        StorageFull,
        QuotaExceeded,
        Other,
        Interrupted,
        NotFound,
        IsADirectory,
    ] {
        assert!(
            !should_write_in_place(&io::Error::from(kind), true),
            "{kind:?} must never fall back to a truncating write"
        );
    }
    #[cfg(unix)]
    {
        // The raw errnos the kinds above stand for, as the kernel reports them.
        const EPERM: i32 = 1;
        const EIO: i32 = 5;
        const EACCES: i32 = 13;
        const EBUSY: i32 = 16;
        const EXDEV: i32 = 18;
        const ENOSPC: i32 = 28;
        for errno in [EPERM, EACCES, EBUSY, EXDEV] {
            assert!(
                should_write_in_place(&io::Error::from_raw_os_error(errno), true),
                "{errno}"
            );
        }
        for errno in [EIO, ENOSPC] {
            assert!(
                !should_write_in_place(&io::Error::from_raw_os_error(errno), true),
                "{errno}"
            );
        }
    }
}

#[test]
fn temp_file_names_are_matched_exactly() {
    assert!(is_temp_file_for(
        ".settings.toml.tmp-123-0",
        "settings.toml"
    ));
    assert!(!is_temp_file_for("settings.toml", "settings.toml"));
    assert!(!is_temp_file_for(".settings.toml", "settings.toml"));
    assert!(!is_temp_file_for(".settings.toml.bak", "settings.toml"));
    assert!(!is_temp_file_for(
        ".keybindings.yaml.tmp-1-0",
        "settings.toml"
    ));
    assert!(!is_temp_file_for(
        ".settings.tomlx.tmp-1-0",
        "settings.toml"
    ));
}

#[test]
fn sweep_removes_only_stale_temp_files_of_the_target() {
    use std::time::{Duration, SystemTime};

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("settings.toml");
    fs::write(&target, "x").unwrap();
    let two_hours_ago = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
    let age = |name: &str, modified: SystemTime| {
        let path = dir.path().join(name);
        let file = fs::File::create(&path).unwrap();
        file.set_modified(modified).unwrap();
    };
    age(".settings.toml.tmp-1-0", two_hours_ago); // stale: removed
    age(".settings.toml.tmp-2-0", SystemTime::now()); // in flight: kept
    age(".other.toml.tmp-1-0", two_hours_ago); // another file's: kept
    age("notes.txt", two_hours_ago); // unrelated: kept

    sweep_stale_temp_files(dir.path(), &target);

    assert_eq!(
        entries(dir.path()),
        vec![
            ".other.toml.tmp-1-0",
            ".settings.toml.tmp-2-0",
            "notes.txt",
            "settings.toml"
        ]
    );
}
