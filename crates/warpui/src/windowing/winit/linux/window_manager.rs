use std::os::unix::fs::MetadataExt;
use std::{fs, path};

/// Attempt to find a running process that we believe is the window compositor.
///
/// The name comes from `/proc/$pid/comm`, and so it will be truncated to the first 15 chars of the
/// actual process name.
/// https://superuser.com/questions/567648/ps-comm-format-always-cuts-the-process-name
pub(crate) fn look_for_wayland_compositor() -> Option<String> {
    // Look for a recognized set of names among the user's running processes. This is a
    // heuristic and may have false positives, like processes that name-clash with these
    // compositors.
    //
    // This runs on the main thread while the first window is created, so it reads
    // `/proc/<pid>/comm` directly. It used to spawn `ps -u | grep`, which blocked startup
    // for 20-250 ms (two fork/execs plus a full process listing) on every launch.
    find_compositor_in_proc(path::Path::new("/proc"), nix::unistd::geteuid().as_raw())
}

/// Scans `proc_root` (normally `/proc`) for a process owned by `uid` whose name contains one of
/// [`WAYLAND_TILING_WM`], case-insensitively. Returns the matched part of the name as written
/// (what `grep -o -i` printed), checking processes in ascending pid order like `ps` did.
fn find_compositor_in_proc(proc_root: &path::Path, uid: u32) -> Option<String> {
    let mut pids: Vec<(u32, path::PathBuf)> = fs::read_dir(proc_root)
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
            Some((pid, entry.path()))
        })
        .collect();
    pids.sort_unstable_by_key(|(pid, _)| *pid);

    pids.into_iter().find_map(|(_, dir)| {
        if process_euid(&dir)? != uid {
            return None;
        }
        let comm = fs::read_to_string(dir.join("comm")).ok()?;
        match_wayland_tiling_wm(comm.trim_end())
    })
}

/// The effective uid of the process whose `/proc/<pid>` directory is `dir`, from the `Uid:` line
/// of its `status` file (what `ps -u` matches). The directory's owner is wrong for a
/// non-dumpable process (e.g. a compositor with file capabilities), which `/proc` makes root-owned.
/// Falls back to the directory owner when `status` is missing or has no parsable `Uid:` line.
fn process_euid(dir: &path::Path) -> Option<u32> {
    let from_status = fs::read_to_string(dir.join("status"))
        .ok()
        .and_then(|status| {
            let line = status.lines().find_map(|l| l.strip_prefix("Uid:"))?;
            line.split_whitespace().nth(1)?.parse::<u32>().ok()
        });
    from_status.or_else(|| fs::metadata(dir).ok().map(|m| m.uid()))
}

/// The earliest case-insensitive occurrence of a [`WAYLAND_TILING_WM`] name in `comm`, as written.
fn match_wayland_tiling_wm(comm: &str) -> Option<String> {
    let lowered = comm.to_ascii_lowercase();
    WAYLAND_TILING_WM
        .iter()
        .filter_map(|wm_name| lowered.find(wm_name).map(|at| (at, wm_name.len())))
        .min_by_key(|(at, _)| *at)
        .and_then(|(at, len)| comm.get(at..at + len))
        .map(str::to_owned)
}

/// Hand-picked tiling wayland compositors. These are the two most starred on GitHub.
const WAYLAND_TILING_WM: &[&str] = &["hyprland", "sway"];

pub(crate) fn is_tiling_window_manager(name: &str) -> bool {
    // List of X11 tiling window managers copied from Chromium repo:
    // https://source.chromium.org/chromium/chromium/src/+/6fa59a48:ui/base/x/x11_util.cc;l=374
    const X11_TILING_WM: &[&str] = &["i3", "ion3", "notion", "ratpoison", "stumpwm"];
    // Dynamic window managers can be configured to function as either tiling or stacking. It is
    // impractical for us to introspect how these are configured, so for now we copy Chrome's
    // approach to assume they are used as tiling.
    const X11_DYNAMIC_WM: &[&str] = &["awesome", "qtile", "xmonad", "wmii"];

    let normalized = name.trim().to_lowercase();
    X11_TILING_WM.contains(&normalized.as_str())
        || X11_DYNAMIC_WM.contains(&normalized.as_str())
        || WAYLAND_TILING_WM.contains(&normalized.as_str())
}

#[cfg(test)]
#[path = "window_manager_tests.rs"]
mod tests;
