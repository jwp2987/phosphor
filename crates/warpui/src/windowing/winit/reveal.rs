//! "Show containing folder" for Linux and Windows: opens the file manager with the file
//! *selected* when possible, falling back to opening the containing folder.
//!
//! Every filesystem stat and the D-Bus / process work runs on a short-lived background
//! thread so the UI thread never blocks on a slow mount or an absent file manager.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What the file manager should be asked to do for a given path.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum RevealAction {
    /// Open the file manager with this existing, non-directory path selected.
    Select(PathBuf),
    /// Just open this directory.
    Open(PathBuf),
    /// Nothing sensible to show.
    Nothing,
}

/// Decides the action from stat results. `is_dir` / `exists` describe `path`;
/// `parent` is its parent directory (if any) and whether that is a directory.
pub(super) fn decide(
    path: &Path,
    is_dir: bool,
    exists: bool,
    parent_is_dir: Option<(&Path, bool)>,
) -> RevealAction {
    if is_dir {
        return RevealAction::Open(path.to_path_buf());
    }
    match parent_is_dir {
        Some((_, true)) if exists => RevealAction::Select(path.to_path_buf()),
        Some((parent, true)) => RevealAction::Open(parent.to_path_buf()),
        _ => RevealAction::Nothing,
    }
}

/// Stats `path` (blocking; call off the main thread) and decides the action.
pub(super) fn plan(path: &Path) -> RevealAction {
    let path = expand_tilde(path, home_dir().as_deref());
    let is_dir = path.is_dir();
    let exists = is_dir || path.exists();
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let parent_state = parent.map(|p| (p, p.is_dir()));
    decide(&path, is_dir, exists, parent_state)
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    std::env::var_os(var).map(PathBuf::from)
}

/// Expands a leading `~` component using `home`.
pub(super) fn expand_tilde(path: &Path, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~"), home) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.to_path_buf(),
    }
}

/// The `file://` URI for `org.freedesktop.FileManager1.ShowItems` (percent-encoded).
pub(super) fn file_manager_uri(path: &Path) -> Option<String> {
    url::Url::from_file_path(path).ok().map(String::from)
}

/// The single raw argument for `explorer.exe`: `/select,"<path>"` with backslash separators.
pub(super) fn explorer_select_arg(path: &Path) -> OsString {
    let mut arg = OsString::from("/select,\"");
    let lossy = path.to_string_lossy().replace('/', "\\");
    arg.push(lossy);
    arg.push("\"");
    arg
}

/// Runs `action`, using `show_items` for selection and `open_dir` for plain opens.
/// If `show_items` fails, falls back to opening the parent folder.
pub(super) fn execute(
    action: RevealAction,
    show_items: impl FnOnce(&Path) -> Result<(), String>,
    mut open_dir: impl FnMut(&Path),
) {
    match action {
        RevealAction::Select(path) => {
            if let Err(err) = show_items(&path) {
                log::info!("Unable to select {path:?} in the file manager ({err}); opening folder");
                if let Some(parent) = path.parent() {
                    open_dir(parent);
                }
            }
        }
        RevealAction::Open(dir) => open_dir(&dir),
        RevealAction::Nothing => {
            log::info!("Neither file nor parent was a valid directory, not opening file");
        }
    }
}

/// Reveals `path` in the platform file manager without blocking the caller.
pub(super) fn reveal_in_background(path: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("reveal-in-file-manager".into())
        .spawn(move || execute(plan(&path), show_items, open_dir));
    if let Err(err) = spawned {
        log::warn!("Unable to start reveal thread: {err}");
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn show_items(path: &Path) -> Result<(), String> {
    use std::time::Duration;

    let uri = file_manager_uri(path).ok_or_else(|| "path is not absolute".to_string())?;
    let call = async move {
        let connection = zbus::Connection::session()
            .await
            .map_err(|e| e.to_string())?;
        connection
            .call_method(
                Some("org.freedesktop.FileManager1"),
                "/org/freedesktop/FileManager1",
                Some("org.freedesktop.FileManager1"),
                "ShowItems",
                &(vec![uri], ""),
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    let timeout = async {
        futures_timer::Delay::new(Duration::from_secs(3)).await;
        Err("timed out".to_string())
    };
    futures_lite::future::block_on(futures_lite::future::or(call, timeout))
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn open_dir(dir: &Path) {
    if let Ok(child) = command::blocking::Command::new("xdg-open").arg(dir).spawn() {
        command::blocking::reap_in_background(child);
    }
}

#[cfg(windows)]
fn show_items(path: &Path) -> Result<(), String> {
    use command::windows::CommandExt as _;
    let mut cmd = command::blocking::Command::new("explorer.exe");
    cmd.raw_arg(explorer_select_arg(path));
    // explorer.exe returns a nonzero exit code even on success, so spawn and don't wait.
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn open_dir(dir: &Path) {
    if let Err(e) = open::that_detached(dir) {
        log::warn!("Unable to open path {e:?}");
    }
}

#[cfg(test)]
#[path = "reveal_tests.rs"]
mod tests;
