use typed_path::{TypedPath, TypedPathBuf, WindowsPath};
use warp_terminal::shell::ShellLaunchData;
use warp_util::path::{
    convert_msys2_to_windows_native_path, convert_wsl_to_windows_host_path, msys2_exe_to_root,
};
use warpui::platform::OperatingSystem;

fn use_unix_paths(shell: Option<&ShellLaunchData>) -> bool {
    OperatingSystem::get().is_linux()
        || OperatingSystem::get().is_mac()
        || shell.is_some_and(|shell| {
            matches!(
                shell,
                ShellLaunchData::WSL { .. } | ShellLaunchData::MSYS2 { .. }
            )
        })
}

pub fn join_paths(paths: &[&str], shell: Option<&ShellLaunchData>) -> String {
    let use_unix_paths = use_unix_paths(shell);

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
) -> TypedPathBuf {
    let expanded_path = shellexpand::tilde(file_path).into_owned();

    let use_unix_paths = use_unix_paths(shell);
    let (cwd, file_path) = if use_unix_paths {
        (
            TypedPathBuf::from_unix(current_working_directory),
            TypedPath::unix(&expanded_path),
        )
    } else {
        (
            TypedPathBuf::from_windows(current_working_directory),
            TypedPath::windows(&expanded_path),
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
    let Some(cwd) = current_working_directory else {
        return shellexpand::tilde(file_path).into_owned();
    };
    shell_native_absolute_path_internal(file_path, shell, cwd)
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
    let Some(cwd) = current_working_directory.as_ref() else {
        return shellexpand::tilde(file_path).into_owned();
    };
    let normalized_path = shell_native_absolute_path_internal(file_path, shell.as_ref(), cwd);

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

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
