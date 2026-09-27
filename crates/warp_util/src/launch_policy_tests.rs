use super::*;
use std::path::{Path, PathBuf};

/// The policy table (#681). Every entry here is a path that must be revealed, never handed to
/// the OS default handler. None of these paths exist: the extension alone decides.
#[test]
fn launchable_extensions_are_revealed() {
    for path in [
        // macOS bundles, installers, disk images, add-ons
        "/tmp/Evil.app",
        "/tmp/Evil.APP",
        "/tmp/setup.pkg",
        "/tmp/setup.mpkg",
        "/tmp/image.dmg",
        "/tmp/pane.prefPane",
        "/tmp/screen.saver",
        "/tmp/driver.kext",
        "/tmp/profile.mobileconfig",
        "/tmp/automator.workflow",
        "/tmp/run.shortcut",
        // macOS scripts and shortcut files
        "/tmp/run.command",
        "/tmp/run.tool",
        "/tmp/settings.terminal",
        "/tmp/script.scpt",
        "/tmp/script.scptd",
        "/tmp/script.applescript",
        "/tmp/link.webloc",
        "/tmp/link.inetloc",
        "/tmp/link.fileloc",
        // Windows executables and installers
        "C:/x/setup.exe",
        "/tmp/setup.EXE",
        "/tmp/x.com",
        "/tmp/x.scr",
        "/tmp/x.pif",
        "/tmp/x.cpl",
        "/tmp/x.msc",
        "/tmp/x.hta",
        "/tmp/x.appref-ms",
        "/tmp/setup.msi",
        "/tmp/patch.msp",
        "/tmp/app.msix",
        "/tmp/app.appx",
        "/tmp/app.appinstaller",
        // Windows scripts and shortcuts
        "/tmp/x.bat",
        "/tmp/x.cmd",
        "/tmp/x.ps1",
        "/tmp/x.vbs",
        "/tmp/x.vbe",
        "/tmp/x.wsf",
        "/tmp/x.reg",
        "/tmp/x.inf",
        "/tmp/x.scf",
        "/tmp/x.lnk",
        "/tmp/x.url",
        "/tmp/x.library-ms",
        "/tmp/x.settingcontent-ms",
        "/tmp/x.theme",
        "/tmp/x.chm",
        // Linux
        "/tmp/app.desktop",
        "/tmp/App.AppImage",
        "/tmp/installer.run",
        "/tmp/pkg.deb",
        "/tmp/pkg.rpm",
        "/tmp/pkg.snap",
        "/tmp/pkg.flatpak",
        "/tmp/pkg.flatpakref",
        // Cross-platform interpreters and disk images
        "/tmp/app.jar",
        "/tmp/app.jnlp",
        "/tmp/x.sh",
        "/tmp/x.bash",
        "/tmp/x.zsh",
        "/tmp/x.fish",
        "/tmp/x.py",
        "/tmp/x.pyw",
        "/tmp/x.pyc",
        "/tmp/disk.iso",
        "/tmp/disk.img",
        "/tmp/disk.vhdx",
        // Office: macro-enabled, legacy, OOXML, OpenDocument
        "/tmp/report.docm",
        "/tmp/sheet.xlsm",
        "/tmp/deck.pptm",
        "/tmp/addin.xlam",
        "/tmp/report.doc",
        "/tmp/sheet.xls",
        "/tmp/deck.ppt",
        "/tmp/report.rtf",
        "/tmp/report.docx",
        "/tmp/sheet.xlsx",
        "/tmp/deck.pptx",
        "/tmp/show.ppsx",
        "/tmp/doc.odt",
        "/tmp/sheet.ods",
        // Win32 strips trailing dots and spaces, so these open the `.exe`.
        "/tmp/evil.exe.",
        "/tmp/evil.exe ",
        "/tmp/evil.exe. .",
        // A launchable extension behind a harmless-looking one.
        "/tmp/invoice.pdf.exe",
    ] {
        assert!(
            is_launchable_path(Path::new(path)),
            "{path:?} must be revealed, not opened"
        );
    }
}

/// The other half of the table: ordinary documents, images, media, archives and code keep
/// opening exactly as before.
#[test]
fn ordinary_files_are_not_launchable() {
    for path in [
        "/tmp/photo.png",
        "/tmp/photo.jpg",
        "/tmp/anim.gif",
        "/tmp/pic.webp",
        "/tmp/drawing.svg",
        "/tmp/paper.pdf",
        "/tmp/movie.mp4",
        "/tmp/song.mp3",
        "/tmp/archive.zip",
        "/tmp/archive.tar.gz",
        "/tmp/notes.txt",
        "/tmp/data.csv",
        "/tmp/README.md",
        "/tmp/main.rs",
        "/tmp/config.json",
        "/tmp/page.html",
        "/tmp/.bashrc",
        "/tmp/Makefile",
        // Extension-only lookalikes.
        "/tmp/app",
        "/tmp/exe",
        "/tmp/x.exec",
        "/tmp/x.apps",
        "/tmp/x.pdf.app.txt",
    ] {
        assert!(
            !is_launchable_path(Path::new(path)),
            "{path:?} must keep its normal open behaviour"
        );
    }
}

/// `.js` runs under Windows Script Host on Windows and is an ordinary source file elsewhere.
#[test]
fn javascript_is_launchable_only_on_windows() {
    assert_eq!(
        is_launchable_path(Path::new("/tmp/payload.js")),
        cfg!(windows)
    );
    assert_eq!(
        is_launchable_path(Path::new("/tmp/payload.JSE")),
        cfg!(windows)
    );
}

#[test]
fn launchable_extension_is_case_insensitive() {
    assert!(is_launchable_extension("APP"));
    assert!(is_launchable_extension("Msi"));
    assert!(!is_launchable_extension("png"));
}

/// macOS bundles are directories. The extension decides even for a real directory, while an
/// ordinary directory (which has the execute bit set on Unix) is never launchable.
#[test]
fn bundle_directories_are_launchable_plain_directories_are_not() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Evil.app");
    std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    assert!(is_launchable_path(&bundle));

    let plain = dir.path().join("src");
    std::fs::create_dir(&plain).unwrap();
    assert!(!is_launchable_path(&plain));
}

#[cfg(unix)]
fn write_with_mode(path: &Path, contents: &[u8], mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, contents).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// An extensionless file with the owner's execute bit: macOS `open` runs it in Terminal.
#[test]
#[cfg(unix)]
fn extensionless_executable_is_launchable() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("payload");
    write_with_mode(&exe, b"echo pwned\n", 0o755);
    assert!(is_launchable_path(&exe));

    let not_exe = dir.path().join("notes");
    write_with_mode(&not_exe, b"echo pwned\n", 0o644);
    assert!(!is_launchable_path(&not_exe));

    // Only the owner's bit counts, matching `is_runnable_shell_script`.
    let group_only = dir.path().join("group_only");
    write_with_mode(&group_only, b"echo pwned\n", 0o070);
    assert!(!is_launchable_path(&group_only));
}

/// An executable bit on a document is common (FAT/NTFS mounts are 0777) and harmless: the
/// handler is picked by extension. It only counts when the content is itself executable.
#[test]
#[cfg(unix)]
fn executable_bit_on_a_document_counts_only_with_executable_content() {
    let dir = tempfile::tempdir().unwrap();

    let pdf = dir.path().join("paper.pdf");
    write_with_mode(&pdf, b"%PDF-1.7\n", 0o777);
    assert!(!is_launchable_path(&pdf));

    let disguised_script = dir.path().join("notes.txt");
    write_with_mode(&disguised_script, b"#!/bin/sh\necho pwned\n", 0o755);
    assert!(is_launchable_path(&disguised_script));

    let disguised_elf = dir.path().join("photo.png");
    write_with_mode(&disguised_elf, b"\x7fELF\x02\x01\x01", 0o755);
    assert!(is_launchable_path(&disguised_elf));

    let disguised_macho = dir.path().join("paper.pdf2");
    write_with_mode(&disguised_macho, &[0xcf, 0xfa, 0xed, 0xfe, 0x07], 0o755);
    assert!(is_launchable_path(&disguised_macho));

    // Same content without the execute bit: the handler would not run it.
    let inert_elf = dir.path().join("inert.png");
    write_with_mode(&inert_elf, b"\x7fELF\x02\x01\x01", 0o644);
    assert!(!is_launchable_path(&inert_elf));
}

#[test]
#[cfg(unix)]
fn executable_magic_reads_short_files() {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in [("empty.txt", &b""[..]), ("one.txt", &b"#"[..])] {
        let path = dir.path().join(name);
        write_with_mode(&path, contents, 0o755);
        assert!(!is_launchable_path(&path), "{name}");
    }
}

/// Revealing must never land on a launchable ancestor: opening `Evil.app` *is* launching it.
#[test]
fn reveal_directory_skips_launchable_ancestors() {
    let dir = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(dir.path()).unwrap();
    assert_eq!(
        reveal_directory_for(&base.join("Evil.app/Contents/MacOS/run.command")),
        Some(base.join("Evil.app/Contents/MacOS"))
    );
    assert_eq!(
        reveal_directory_for(&base.join("Evil.app")),
        Some(base.clone())
    );
    assert_eq!(
        reveal_directory_for(&base.join("Outer.app/Inner.app")),
        Some(base.clone())
    );
}

/// #681 review: a harmless name that is a symlink to a bundle. `metadata` follows the link and
/// sees a directory; the opener resolves the link and launches the bundle.
#[test]
#[cfg(unix)]
fn symlink_with_a_harmless_name_to_a_bundle_is_launchable() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Evil.app");
    std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    std::fs::create_dir(dir.path().join("docs")).unwrap();

    let file_named_link = dir.path().join("docs/guide.pdf");
    std::os::unix::fs::symlink("../Evil.app", &file_named_link).unwrap();
    assert!(is_launchable_path(&file_named_link));

    let dir_named_link = dir.path().join("docs2");
    std::os::unix::fs::symlink(&bundle, &dir_named_link).unwrap();
    assert!(is_launchable_path(&dir_named_link));

    // The opener is handed the resolved path, which is exactly what was checked.
    let resolved = resolve_for_open(&file_named_link);
    assert!(resolved.launchable);
    assert_eq!(resolved.path, dunce::canonicalize(&bundle).unwrap());

    // The reveal folder is never the symlinked bundle.
    let inside = dir_named_link.join("Contents");
    assert_eq!(
        reveal_directory_for(&inside.join("run.command")),
        Some(dunce::canonicalize(bundle.join("Contents")).unwrap())
    );
    assert_eq!(
        reveal_directory_for(&dir_named_link),
        Some(dunce::canonicalize(dir.path()).unwrap())
    );

    // A symlink to an ordinary file stays ordinary.
    let pdf = dir.path().join("real.pdf");
    std::fs::write(&pdf, b"%PDF-1.7\n").unwrap();
    let pdf_link = dir.path().join("docs/paper.pdf");
    std::os::unix::fs::symlink(&pdf, &pdf_link).unwrap();
    assert!(!is_launchable_path(&pdf_link));
}

/// #681 review: `Evil.app/Contents/..` has no file name, and `metadata` sees a directory; the
/// macOS opener standardises it to `Evil.app` and launches it.
#[test]
fn dot_dot_suffix_into_a_bundle_is_launchable() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Evil.app");
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    let sneaky = bundle.join("Contents").join("..");
    assert!(sneaky.file_name().is_none());
    assert!(is_launchable_path(&sneaky));
    assert_eq!(
        resolve_for_open(&sneaky).path,
        dunce::canonicalize(&bundle).unwrap()
    );

    // Also when nothing exists: lexical normalisation alone must catch it.
    assert!(is_launchable_path(Path::new(
        "/nonexistent-681/Evil.app/Contents/.."
    )));
    assert!(is_launchable_path(Path::new(
        "/nonexistent-681/Evil.app/./Contents/../"
    )));
    assert!(!is_launchable_path(Path::new(
        "/nonexistent-681/Evil.app/.."
    )));
}

#[test]
fn normalize_lexically_folds_parent_components() {
    assert_eq!(
        normalize_lexically(Path::new("/a/b/../c/./d")),
        PathBuf::from("/a/c/d")
    );
    assert_eq!(normalize_lexically(Path::new("/..")), PathBuf::from("/"));
    assert_eq!(
        normalize_lexically(Path::new("../../a")),
        PathBuf::from("../../a")
    );
}

/// #681 review: macOS's opener expands a leading `~`, so the check must too.
#[test]
fn leading_tilde_is_expanded_like_the_opener() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let resolved = canonical_path_for_open(Path::new("~/nonexistent-681/Evil.app"));
    assert!(
        resolved.ends_with("nonexistent-681/Evil.app"),
        "{resolved:?}"
    );
    assert!(
        resolved.starts_with(dunce::canonicalize(&home).unwrap_or(home)),
        "{resolved:?}"
    );
    // Relative paths come back absolute, so nothing downstream re-interprets a `~`.
    assert!(canonical_path_for_open(Path::new("~someone/x.txt")).is_absolute());
}

/// #681 review: a non-UTF-8 name must still yield its extension; `to_str()` returned `None` and
/// the policy failed open.
#[test]
#[cfg(unix)]
fn non_utf8_names_keep_their_extension() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let name = Path::new(OsStr::from_bytes(b"/tmp/x\xff.deb"));
    assert_eq!(candidate_extensions(name), vec!["deb".to_owned()]);
    assert!(is_launchable_path(name));
    let app = Path::new(OsStr::from_bytes(b"/tmp/\xfe\xffEvil.APP"));
    assert!(is_launchable_path(app));
}

#[test]
#[cfg(windows)]
fn unpaired_surrogate_names_keep_their_extension() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    // "C:\x<unpaired surrogate>.exe"
    let mut wide: Vec<u16> = "C:\\x".encode_utf16().collect();
    wide.push(0xD800);
    wide.extend(".exe".encode_utf16());
    let path = std::path::PathBuf::from(OsString::from_wide(&wide));
    assert!(is_launchable_path(&path));
}

/// #681 review: NTFS alternate data streams name the file before the `:`.
#[test]
fn alternate_data_stream_suffix_is_seen_through() {
    for path in [
        "/tmp/evil.exe::$DATA",
        "/tmp/evil.exe:stream",
        "/tmp/evil.EXE:stream:$DATA",
        "/tmp/notes.txt:hidden.exe",
    ] {
        assert!(is_launchable_path(Path::new(path)), "{path}");
    }
    assert!(!is_launchable_path(Path::new("/tmp/notes.txt:hidden")));
}

/// #681 review: table additions.
#[test]
fn review_table_additions_are_launchable() {
    for ext in [
        "one",
        "onepkg",
        "rdp",
        "mdb",
        "accdb",
        "accde",
        "ade",
        "adp",
        "mde",
        "mda",
        "mam",
        "iqy",
        "slk",
        "dqy",
        "xlw",
        "mht",
        "mhtml",
        "vsto",
        "vsix",
        "wll",
        "sparseimage",
        "sparsebundle",
        "cdr",
        "ftploc",
        "mailloc",
        "vncloc",
    ] {
        assert!(is_launchable_extension(ext), "{ext}");
        assert!(
            is_launchable_path(Path::new(&format!("/tmp/x.{ext}"))),
            "{ext}"
        );
    }
}

/// #681 review: a FIFO in place of the file must not block the magic-number read. Before the
/// fix, `File::open` on a FIFO with no writer blocked forever.
#[test]
#[cfg(unix)]
fn fifo_does_not_block_the_magic_read() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("photo.png");
    let c_path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is a valid NUL-terminated path for the duration of the call.
    let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o755) };
    assert_eq!(rc, 0, "mkfifo failed");

    // Called directly: this is the "swapped between stat and open" case.
    assert!(!starts_with_executable_magic(&fifo));
    // And through the public entry point.
    assert!(!is_launchable_path(&fifo));
}
