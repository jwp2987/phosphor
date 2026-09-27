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

use std::path::{Component, Path, PathBuf};

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
    "ftploc",
    "mailloc",
    "vncloc",
    // --- macOS: disk images besides .dmg (opening one mounts it) ---
    "sparseimage",
    "sparsebundle",
    "cdr",
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
    "rdp",
    "mht",
    "mhtml",
    "vsto",
    "vsix",
    "wll",
    // OneNote notebooks embed attachments that run on double-click.
    "one",
    "onepkg",
    // Access databases run VBA and macros on open.
    "mdb",
    "accdb",
    "accde",
    "ade",
    "adp",
    "mde",
    "mda",
    "mam",
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
    // Excel data-connection and legacy formats that run DDE/web queries on open.
    "iqy",
    "slk",
    "dqy",
    "xlw",
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

/// Every extension the OS might act on for `path`, lower-cased and without the dot. Empty when
/// the name has none.
///
/// * The name is read lossily, never with `to_str()`: a non-UTF-8 name (`x\xff.deb` on Linux, an
///   unpaired surrogate on Windows) must still yield its extension, or the policy fails open.
/// * Trailing dots and spaces are stripped: Win32 path normalisation drops them, so `evil.exe.`
///   and `evil.exe ` both open `evil.exe`.
/// * An NTFS alternate-data-stream suffix is considered too: `evil.exe::$DATA` and
///   `evil.exe:stream` name `evil.exe`, so the part before the first `:` contributes its
///   extension alongside the whole name's. On Unix `:` is an ordinary character and the extra
///   candidate only ever makes the check stricter.
pub fn candidate_extensions(path: &Path) -> Vec<String> {
    let Some(name) = path.file_name() else {
        return Vec::new();
    };
    let name = name.to_string_lossy();
    let mut forms = vec![name.as_ref()];
    if let Some((before_stream, _)) = name.split_once(':') {
        forms.push(before_stream);
    }
    let mut extensions = Vec::new();
    for form in forms {
        let form = form.trim_end_matches(['.', ' ']);
        if let Some((stem, ext)) = form.rsplit_once('.') {
            // A leading-dot name (`.bashrc`) has no extension, matching `Path::extension`.
            if !stem.is_empty() && !ext.is_empty() {
                extensions.push(ext.to_ascii_lowercase());
            }
        }
    }
    extensions
}

/// Whether `ext` (no dot, any case) is in the launchable set for this platform.
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

/// The path the platform opener will actually act on, computed the way it will compute it.
///
/// * A leading `~` is expanded to the home directory: macOS's opener calls
///   `stringByExpandingTildeInPath`, so a literal `~/x.app` means `$HOME/x.app` to it.
/// * A relative path is made absolute against the current directory. The result no longer
///   starts with `~`, so an unexpandable `~user/...` cannot be re-interpreted downstream.
/// * Symlinks and `..` are resolved (`fs::canonicalize`); for a path that does not exist the
///   longest existing prefix is canonicalised and the rest is normalised lexically. macOS's
///   opener calls `standardizedURL`, which folds `Evil.app/Contents/..` into `Evil.app`.
///
/// [`is_launchable_path`] checks this path as well as the one it was given, and callers pass
/// *this* path -- the one that was checked -- to the opener.
pub fn canonical_path_for_open(path: &Path) -> PathBuf {
    let expanded = expand_leading_tilde(path);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        match std::env::current_dir() {
            Ok(dir) => dir.join(&expanded),
            Err(_) => expanded,
        }
    };
    if let Ok(canonical) = dunce::canonicalize(&absolute) {
        return canonical;
    }
    let normal = normalize_lexically(&absolute);
    let mut existing = normal.as_path();
    let mut missing = Vec::new();
    loop {
        if let Ok(mut canonical) = dunce::canonicalize(existing) {
            for name in missing.iter().rev() {
                canonical.push(name);
            }
            return canonical;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_owned());
                existing = parent;
            }
            _ => return normal,
        }
    }
}

fn expand_leading_tilde(path: &Path) -> PathBuf {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == "~" => match dirs::home_dir() {
            Some(home) => home.join(components.as_path()),
            None => path.to_path_buf(),
        },
        _ => path.to_path_buf(),
    }
}

/// `.` dropped and `..` folded into its parent, without touching the filesystem.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let last_is_normal =
                    matches!(normal.components().next_back(), Some(Component::Normal(_)));
                let last_is_root = matches!(
                    normal.components().next_back(),
                    Some(Component::RootDir | Component::Prefix(_))
                );
                if last_is_normal {
                    normal.pop();
                } else if !last_is_root {
                    // A relative path climbing above its start keeps the `..`; `/..` is `/`.
                    normal.push(component);
                }
            }
            other => normal.push(other),
        }
    }
    normal
}

/// Whether handing `path` to the OS default handler could launch, install, mount or run it.
///
/// The path is checked as given, lexically normalised, and as [`canonical_path_for_open`]
/// resolves it: a harmless name can be a symlink to a bundle (`docs/guide.pdf -> ../Evil.app`),
/// and `Evil.app/Contents/..` has no file name at all. Each form is launchable if:
/// 1. any of its [`candidate_extensions`] is launchable -- this also covers macOS bundles, which
///    are directories (`Foo.app/`), and does not require the path to exist; or
/// 2. on Unix, it is a regular file with the owner's execute bit that either has no extension or
///    starts with `#!` or an executable-format magic number. macOS `open` runs such a file in
///    Terminal; Linux file managers offer to run it. An executable bit on a file with a
///    document extension (a `.pdf` on a FAT-formatted drive, where every file is 0777) does
///    not count on its own: the handler is chosen by extension and does not execute it.
///
/// Directories other than bundles are never launchable: opening one shows it in the file
/// manager, which is exactly what revealing would do.
///
/// This touches the filesystem (stat, canonicalise, read four bytes). Call it when the user
/// clicks, never on hover: a hung network mount would freeze the UI.
pub fn is_launchable_path(path: &Path) -> bool {
    resolve_for_open(path).launchable
}

/// The result of checking a path before handing it to the OS opener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOpen {
    /// The path to hand to the opener (or to reveal): [`canonical_path_for_open`] of the input.
    pub path: PathBuf,
    /// Whether any form of the input is [`is_launchable_path`].
    pub launchable: bool,
}

/// Check `path` once and return the exact path that was checked, for the opener to use.
pub fn resolve_for_open(path: &Path) -> ResolvedOpen {
    let canonical = canonical_path_for_open(path);
    let launchable = is_launchable_literal(path)
        || is_launchable_literal(&normalize_lexically(path))
        || is_launchable_literal(&canonical);
    ResolvedOpen {
        path: canonical,
        launchable,
    }
}

/// The policy applied to one spelling of a path, with no resolution.
fn is_launchable_literal(path: &Path) -> bool {
    candidate_extensions(path)
        .iter()
        .any(|ext| is_launchable_extension(ext))
        || is_executable_file(path)
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
    if candidate_extensions(path).is_empty() {
        return true;
    }
    starts_with_executable_magic(path)
}

#[cfg(not(unix))]
fn is_executable_file(_path: &Path) -> bool {
    // Windows decides executability by extension alone, which the extension check covered.
    false
}

/// Whether `path` starts with `#!`, or an ELF, Mach-O (thin or fat) or PE header.
///
/// Reads at most four bytes: the path can be named by terminal output or a model, so the file
/// is attacker-controlled in size. Opened with `O_NONBLOCK` and re-checked with `fstat` on the
/// handle: a file swapped for a FIFO between the caller's `stat` and this `open` would otherwise
/// block the UI thread until something writes to the pipe.
#[cfg(unix)]
fn starts_with_executable_magic(path: &Path) -> bool {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;

    let Ok(mut file) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    else {
        return false;
    };
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return false;
    }
    let mut prefix = [0u8; 4];
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

/// The directory to show instead of launching `path`: the nearest ancestor of its
/// [`canonical_path_for_open`] that is not itself launchable (`Evil.app/Contents/x.command` must
/// not resolve to `Evil.app`, which would launch the bundle). Working from the canonical path
/// means the result is never a symlink to a bundle either. `None` if no such ancestor exists.
pub fn reveal_directory_for(path: &Path) -> Option<PathBuf> {
    let canonical = canonical_path_for_open(path);
    canonical
        .ancestors()
        .skip(1)
        .find(|ancestor| !ancestor.as_os_str().is_empty() && !is_launchable_literal(ancestor))
        .map(Path::to_path_buf)
}

#[cfg(test)]
#[path = "launch_policy_tests.rs"]
mod tests;
