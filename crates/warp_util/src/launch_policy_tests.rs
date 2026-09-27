use super::*;
use std::path::Path;

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
    assert_eq!(
        reveal_directory_for(Path::new("/tmp/Evil.app/Contents/MacOS/run.command")),
        Some(Path::new("/tmp/Evil.app/Contents/MacOS"))
    );
    assert_eq!(
        reveal_directory_for(Path::new("/tmp/Evil.app")),
        Some(Path::new("/tmp"))
    );
    assert_eq!(
        reveal_directory_for(Path::new("/tmp/Outer.app/Inner.app")),
        Some(Path::new("/tmp"))
    );
    assert_eq!(reveal_directory_for(Path::new("Evil.app")), None);
}
