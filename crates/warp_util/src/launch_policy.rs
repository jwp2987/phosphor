//! Which local paths may be handed to the operating system's default handler (#681).
//!
//! "Open with the default application" is how a click on a path becomes a *launch*: on macOS
//! `open Foo.app` starts the app and `open x.pkg` starts Installer; on Windows `.exe`, `.msi`,
//! `.lnk`, `.js` and `.vbs` run; on Linux `xdg-open` of a trusted `.desktop` file starts its
//! `Exec=` line. The paths reaching that call are frequently *named by someone else* -- a
//! model's answer in an AI block, an agent-written AI document, a markdown/notebook link, a
//! build tool's output or an OSC 8 hyperlink printed over SSH -- so a single click would run
//! whatever that party put at that path.
//!
//! Files an agent or a tool writes locally carry no Mark-of-the-Web (Windows) and no
//! `com.apple.quarantine` attribute (macOS), so the platforms' own last-line checks --
//! Gatekeeper's first-launch prompt, Office Protected View, SmartScreen -- do not engage. The
//! only safe place to stop the launch is before it happens.
//!
//! The policy is deliberately about *what the OS would do*, not about what the file contains:
//! a path is [`is_launchable_path`] if handing it to the default handler can execute code,
//! install software, mount an image, follow a shortcut, or open a document format whose
//! handler runs embedded macros. Such a path is **revealed** in the file manager instead (or,
//! when it is text such as a `.command` or `.py` file, opened in the in-app code editor, which
//! reads rather than runs it); the user can still launch it from the file manager
//! deliberately. Ordinary documents, images, media and archives are unaffected.
//!
//! The policy is applied at every layer so that no surface can bypass it:
//! `openable_file_type::resolve_file_target` (the app's routing decision), the workspace's
//! `open_file_with_target` sink, `AppContext::open_file_path` in `warpui_core` (which every
//! "open this path with the system" call in the process goes through), and, for `file:` URLs,
//! the terminal's URL handler plus the app's `set_before_open_url` callback.

use std::path::Path;

/// Extensions that launch, install, mount or follow a shortcut on at least one desktop
/// platform, or whose default handler runs embedded macros. Lower-case, without the dot.
///
/// The list is applied on every platform, not just the one that defines the format: Wine
/// registers `.exe`/`.msi`/`.lnk` on Linux, the python.org installer registers Python Launcher
/// (which *runs* the script) for `.py` on macOS, and a model can name any of these anywhere.
/// The cost of an entry that is inert on the current platform is one extra click in the file
/// manager.
const LAUNCHABLE_EXTENSIONS: &[&str] = &[
    // --- macOS: application bundles, installers, disk images, system add-ons ---
    "app",
    "pkg",
    "mpkg",
    "dmg",
    "prefpane",
    "saver",
    "kext",
    "mobileconfig",
    "workflow",
    "action",
    "shortcut",
    // --- macOS: scripts and shortcut files the default handler executes or follows ---
    "command",
    "tool",
    "terminal",
    "scpt",
    "scptd",
    "applescript",
    "webloc",
    "inetloc",
    "fileloc",
    "afploc",
    // --- Windows: executables ---
    "exe",
    "com",
    "scr",
    "pif",
    "cpl",
    "msc",
    "gadget",
    "hta",
    "application",
    "appref-ms",
    "xbap",
    // --- Windows: installers and packages ---
    "msi",
    "msp",
    "mst",
    "msix",
    "msixbundle",
    "appx",
    "appxbundle",
    "appinstaller",
    // --- Windows: scripts, shortcuts and shell-integration files ---
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "wsf",
    "wsh",
    "wsc",
    "sct",
    "reg",
    "inf",
    "scf",
    "lnk",
    "url",
    "website",
    "library-ms",
    "search-ms",
    "searchconnector-ms",
    "settingcontent-ms",
    "theme",
    "themepack",
    "diagcab",
    "chm",
    // --- Linux: launchers, self-extracting installers and packages ---
    "desktop",
    "appimage",
    "run",
    "deb",
    "rpm",
    "snap",
    "flatpak",
    "flatpakref",
    "flatpakrepo",
    // --- Cross-platform: interpreters with a registered "run" handler ---
    "jar",
    "jnlp",
    "sh",
    "bash",
    "zsh",
    "ksh",
    "csh",
    "tcsh",
    "fish",
    "py",
    "pyw",
    "pyz",
    "pyzw",
    "pyc",
    "pyo",
    // --- Cross-platform: disk images (opening mounts them; Windows then offers AutoPlay) ---
    "iso",
    "img",
    "vhd",
    "vhdx",
    // --- Office documents: revealed, never opened (#681) ---
    // Every Office and OpenDocument format is here, not only the macro-enabled ones: a locally
    // written file has no Mark-of-the-Web, so Protected View -- Office's defence against exactly
    // these documents -- does not engage, and the cost of revealing is one double-click.
    // Macro-enabled OOXML, templates and add-ins run VBA.
    "docm",
    "dotm",
    "xlsm",
    "xltm",
    "xlsb",
    "xlam",
    "xla",
    "xll",
    "pptm",
    "potm",
    "ppsm",
    "ppam",
    "sldm",
    // Legacy binary formats carry VBA and OLE objects.
    "doc",
    "dot",
    "xls",
    "xlt",
    "ppt",
    "pot",
    "pps",
    "rtf",
    "pub",
    // Macro-free OOXML still reaches remote templates, OLE and DDE (Follina, CVE-2022-30190,
    // was a `.docx`), and `.ppsx` opens straight into a slideshow.
    "docx",
    "dotx",
    "xlsx",
    "xltx",
    "pptx",
    "potx",
    "ppsx",
    // OpenDocument files can embed Basic macros.
    "odt",
    "ott",
    "ods",
    "ots",
    "odp",
    "otp",
    "odg",
];

/// Extensions that are launchable only on Windows, where Windows Script Host is the registered
/// handler and *runs* them. Everywhere else they are ordinary source files that open in an
/// editor, and revealing them would be a regression for no gain.
#[cfg(windows)]
const WINDOWS_ONLY_LAUNCHABLE_EXTENSIONS: &[&str] = &["js", "jse"];

/// The extension the OS would act on, lower-cased.
///
/// Trailing dots and spaces are stripped first: Win32 path normalisation drops them, so
/// `evil.exe.` and `evil.exe ` both open `evil.exe`, while `Path::extension` reports an empty or
/// space-suffixed extension for them.
fn effective_extension(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let name = name.trim_end_matches(['.', ' ']);
    let (stem, ext) = name.rsplit_once('.')?;
    // A leading-dot name (`.bashrc`) has no extension, matching `Path::extension`.
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Whether `ext` (lower-case, no dot) is in the launchable set for this platform.
pub fn is_launchable_extension(ext: &str) -> bool {
    let ext = ext.to_ascii_lowercase();
    if LAUNCHABLE_EXTENSIONS.contains(&ext.as_str()) {
        return true;
    }
    #[cfg(windows)]
    if WINDOWS_ONLY_LAUNCHABLE_EXTENSIONS.contains(&ext.as_str()) {
        return true;
    }
    false
}

/// Whether handing `path` to the OS default handler could launch, install, mount or run it.
///
/// Checks, in order:
/// 1. the extension against [`LAUNCHABLE_EXTENSIONS`] -- this also covers macOS bundles, which
///    are directories (`Foo.app/`), and does not require the path to exist;
/// 2. on Unix, a regular file with the owner's execute bit that either has no extension or
///    starts with `#!` or an executable-format magic number. macOS `open` runs such a file in
///    Terminal; Linux file managers offer to run it. An executable bit on a file with a
///    document extension (a `.pdf` on a FAT-formatted drive, where every file is 0777) does
///    not count on its own: the handler is chosen by extension and does not execute it.
///
/// Directories other than bundles are never launchable: opening one shows it in the file
/// manager, which is exactly what revealing would do.
pub fn is_launchable_path(path: &Path) -> bool {
    if effective_extension(path).is_some_and(|ext| is_launchable_extension(&ext)) {
        return true;
    }
    is_executable_file(path)
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.permissions().mode() & 0o100 == 0 {
        return false;
    }
    if effective_extension(path).is_none() {
        return true;
    }
    starts_with_executable_magic(path)
}

#[cfg(not(unix))]
fn is_executable_file(_path: &Path) -> bool {
    // Windows decides executability by extension alone, which step 1 already covered.
    false
}

/// Whether `path` starts with `#!`, or an ELF, Mach-O (thin or fat) or PE header.
///
/// Reads at most four bytes: the path can be named by terminal output or a model, so the file
/// is attacker-controlled in size.
#[cfg(unix)]
fn starts_with_executable_magic(path: &Path) -> bool {
    use std::io::Read;

    let mut prefix = [0u8; 4];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut filled = 0;
    while filled < prefix.len() {
        match file.read(&mut prefix[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        }
    }
    let prefix = &prefix[..filled];
    const MAGICS: &[&[u8]] = &[
        b"#!",
        b"\x7fELF",
        &[0xfe, 0xed, 0xfa, 0xce],
        &[0xfe, 0xed, 0xfa, 0xcf],
        &[0xce, 0xfa, 0xed, 0xfe],
        &[0xcf, 0xfa, 0xed, 0xfe],
        &[0xca, 0xfe, 0xba, 0xbe],
        b"MZ",
    ];
    MAGICS.iter().any(|magic| prefix.starts_with(magic))
}

/// The directory to show instead of launching `path`: its nearest ancestor that is not itself
/// launchable (`Evil.app/Contents/x.command` must not resolve to `Evil.app`, which would launch
/// the bundle). `None` if no such ancestor exists.
pub fn reveal_directory_for(path: &Path) -> Option<&Path> {
    path.ancestors()
        .skip(1)
        .find(|ancestor| !ancestor.as_os_str().is_empty() && !is_launchable_path(ancestor))
}

#[cfg(test)]
#[path = "launch_policy_tests.rs"]
mod tests;
