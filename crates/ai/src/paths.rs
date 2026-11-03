use typed_path::{TypedPath, TypedPathBuf, WindowsPath};
use warp_terminal::shell::ShellLaunchData;
use warp_util::path::{
    convert_msys2_to_windows_native_path, convert_wsl_to_windows_host_path, msys2_exe_to_root,
};
use warpui::platform::OperatingSystem;

/// Which host a session's paths belong to. `Local` keeps the local-OS rules (and is also the
/// answer for a remote whose OS is unknown); a known remote uses its own OS's rules whatever
/// the local OS is (#1278).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PathHost {
    #[default]
    Local,
    RemoteUnix,
    RemoteWindows,
}

impl PathHost {
    /// From a session's reported OS category (`"Linux"`, `"MacOS"`, `"Windows"`, ...). A remote
    /// session with no / unrecognised category is `Local`: the previous behaviour.
    pub fn for_remote(os_category: Option<&str>) -> Self {
        match os_category.map(str::to_ascii_lowercase).as_deref() {
            Some("windows") => Self::RemoteWindows,
            Some("linux" | "macos" | "mac" | "darwin") => Self::RemoteUnix,
            _ => Self::Local,
        }
    }

    fn is_remote(self) -> bool {
        self != Self::Local
    }
}

/// Whether paths for this session use `/` separators and unix rules.
///
/// `remote_unix` is true for a remote (ssh / warpified-remote) session: those hosts are
/// Linux or macOS, so their paths are unix paths whatever OS the local app runs on (#1278).
/// Without it, a Windows host driving a Linux remote joined and normalised agent paths with
/// `\` and drive-letter rules.
fn use_unix_paths(shell: Option<&ShellLaunchData>, host: PathHost) -> bool {
    use_unix_paths_on(
        OperatingSystem::get().is_linux() || OperatingSystem::get().is_mac(),
        shell,
        host,
    )
}

fn use_unix_paths_on(
    local_os_is_unix: bool,
    shell: Option<&ShellLaunchData>,
    host: PathHost,
) -> bool {
    match host {
        PathHost::RemoteUnix => return true,
        PathHost::RemoteWindows => return false,
        PathHost::Local => {}
    }
    local_os_is_unix
        || shell.is_some_and(|shell| {
            matches!(
                shell,
                ShellLaunchData::WSL { .. } | ShellLaunchData::MSYS2 { .. }
            )
        })
}

pub fn join_paths(paths: &[&str], shell: Option<&ShellLaunchData>) -> String {
    join_paths_for(paths, shell, PathHost::Local)
}

/// [`join_paths`] for a session that may be remote; see [`use_unix_paths`].
pub fn join_paths_for(paths: &[&str], shell: Option<&ShellLaunchData>, host: PathHost) -> String {
    join_paths_with_style(paths, use_unix_paths(shell, host))
}

fn join_paths_with_style(paths: &[&str], use_unix_paths: bool) -> String {
    let base_path = if use_unix_paths {
        TypedPathBuf::unix()
    } else {
        TypedPathBuf::windows()
    };
    paths
        .iter()
        .fold(base_path, |acc, path| acc.join(path))
        .to_string_lossy()
        .into_owned()
}

fn shell_native_absolute_path_internal(
    file_path: &str,
    shell: Option<&ShellLaunchData>,
    current_working_directory: &str,
    host: PathHost,
) -> TypedPathBuf {
    let expanded_path = if host.is_remote() {
        // `~` was expanded against the remote home by the caller; the local home is wrong.
        file_path.to_owned()
    } else {
        shellexpand::tilde(file_path).into_owned()
    };

    let use_unix_paths = use_unix_paths(shell, host);
    normalize_with_style(&expanded_path, current_working_directory, use_unix_paths)
}

fn normalize_with_style(
    expanded_path: &str,
    current_working_directory: &str,
    use_unix_paths: bool,
) -> TypedPathBuf {
    let (cwd, file_path) = if use_unix_paths {
        (
            TypedPathBuf::from_unix(current_working_directory),
            TypedPath::unix(expanded_path),
        )
    } else {
        (
            TypedPathBuf::from_windows(current_working_directory),
            TypedPath::windows(expanded_path),
        )
    };
    cwd.join(file_path).normalize()
}

/// Returns the absolute path of the path in the shell's native format.
///
/// On Unix systems, this will always be Unix encoded paths. On Windows, this
/// will be a Windows encoded path unless the user is using WSL or Git Bash, in
/// which case Unix encoded paths will be used.
pub fn shell_native_absolute_path(
    file_path: &str,
    shell: Option<&ShellLaunchData>,
    current_working_directory: Option<&String>,
) -> String {
    shell_native_absolute_path_for(file_path, shell, current_working_directory, PathHost::Local)
}

/// [`shell_native_absolute_path`] for a session that may be remote: a remote session's paths
/// use unix rules whatever the local OS is (#1278).
pub fn shell_native_absolute_path_for(
    file_path: &str,
    shell: Option<&ShellLaunchData>,
    current_working_directory: Option<&String>,
    host: PathHost,
) -> String {
    let Some(cwd) = current_working_directory else {
        return if host.is_remote() {
            file_path.to_owned()
        } else {
            shellexpand::tilde(file_path).into_owned()
        };
    };
    shell_native_absolute_path_internal(file_path, shell, cwd, host)
        .to_string_lossy()
        .into_owned()
}

/// Returns the absolute path of the path in the host's native format.
///
/// This should be used over [`shell_native_absolute_path`] when we need an
/// absolute path in the format of the user's OS, regardless of what shell
/// they're using. e.g. A Windows encoded path when the user is using WSL.
pub fn host_native_absolute_path(
    file_path: &str,
    shell: &Option<ShellLaunchData>,
    current_working_directory: &Option<String>,
) -> String {
    host_native_absolute_path_for(file_path, shell, current_working_directory, PathHost::Local)
}

/// [`host_native_absolute_path`] for a session that may be remote. A remote session's paths are
/// unix paths on the remote host, so they are joined and normalised with unix rules and are
/// never converted through the LOCAL WSL / MSYS2 mapping (#1278).
pub fn host_native_absolute_path_for(
    file_path: &str,
    shell: &Option<ShellLaunchData>,
    current_working_directory: &Option<String>,
    host: PathHost,
) -> String {
    let Some(cwd) = current_working_directory.as_ref() else {
        return if host.is_remote() {
            file_path.to_owned()
        } else {
            shellexpand::tilde(file_path).into_owned()
        };
    };
    let normalized_path = shell_native_absolute_path_internal(file_path, shell.as_ref(), cwd, host);
    if host.is_remote() {
        return normalized_path.to_string_lossy().into_owned();
    }

    match shell {
        Some(ShellLaunchData::WSL { distro }) => {
            match convert_wsl_to_windows_host_path(&normalized_path.to_path(), distro) {
                Ok(path) => path.to_string_lossy().into_owned(),
                Err(err) => {
                    log::error!(
                        "Could not convert WSL to Windows host path {normalized_path:?}: {err:#}"
                    );
                    normalized_path.to_string_lossy().into_owned()
                }
            }
        }
        Some(ShellLaunchData::MSYS2 {
            executable_path, ..
        }) => {
            match convert_msys2_to_windows_native_path(
                &normalized_path.to_path(),
                &msys2_exe_to_root(WindowsPath::new(
                    executable_path.as_os_str().as_encoded_bytes(),
                )),
            ) {
                Ok(path) => path.to_string_lossy().into_owned(),
                Err(err) => {
                    log::error!(
                        "Could not convert MSYS2 to Windows host path {normalized_path:?}: {err:#}"
                    );
                    normalized_path.to_string_lossy().into_owned()
                }
            }
        }
        _ => normalized_path.to_string_lossy().into_owned(),
    }
}

/// Expands a leading `~` or `~/` in `file_path` against `home`, the home directory of the host
/// the shell runs on.
///
/// [`shell_native_absolute_path`] and [`host_native_absolute_path`] expand `~` against the
/// LOCAL user's home, which is right for a local shell and wrong for a remote session: `~/x`
/// on a remote host is not `/home/<local user>/x` (#1278). Callers on a remote session expand
/// with the session's own home first. Paths without a leading `~`, other users' homes
/// (`~bob`), and an unknown `home` are returned unchanged.
pub fn expand_tilde_against(file_path: &str, home: Option<&str>) -> String {
    let Some(home) = home.filter(|home| !home.is_empty()) else {
        return file_path.to_owned();
    };
    if file_path == "~" {
        return home.to_owned();
    }
    match file_path.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", home.trim_end_matches('/')),
        None => file_path.to_owned(),
    }
}

/// Whether `file_path` starts with a `~` that [`expand_tilde_against`] would expand (`~` or
/// `~/...`). A remote session with no known home must not let such a path fall through to the
/// local expansion; callers fail closed on it (#1288).
pub fn path_needs_home(file_path: &str) -> bool {
    file_path == "~" || file_path.starts_with("~/")
}

/// The path-joining rules a remote session needs, decided independently of the local OS so
/// they can be tested on any host.
#[cfg(test)]
pub(crate) fn remote_style_for_test(local_os_is_unix: bool, host: PathHost) -> bool {
    use_unix_paths_on(local_os_is_unix, None, host)
}

#[cfg(test)]
pub(crate) fn join_for_test(paths: &[&str], unix: bool) -> String {
    join_paths_with_style(paths, unix)
}

#[cfg(test)]
pub(crate) fn normalize_for_test(path: &str, cwd: &str, unix: bool) -> String {
    normalize_with_style(path, cwd, unix)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
