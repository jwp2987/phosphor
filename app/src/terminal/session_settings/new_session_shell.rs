use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use warp_util::path::ShellFamily;
use warpui::platform::OperatingSystem;

#[derive(
    Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[schemars(
    description = "Shell to use when opening new sessions.",
    rename_all = "snake_case"
)]
pub enum NewSessionShell {
    #[default]
    #[schemars(description = "Use the operating system's default shell.")]
    SystemDefault,
    #[schemars(description = "A shell executable path.")]
    Executable(String),
    #[schemars(description = "An MSYS2 shell environment.")]
    MSYS2(String),
    #[schemars(description = "A Windows Subsystem for Linux distribution.")]
    WSL(String),
    #[schemars(description = "A custom shell command.")]
    Custom(String),
}

/// The structured settings-file form of [`NewSessionShell`]. Its derived
/// `SettingsValue` fixes the file spelling of each variant (snake_case keys,
/// e.g. `{ executable = "<path>" }`), and [`NewSessionShell`]'s own impl adds
/// the bare-string shorthand on top of it (#1553).
#[derive(Serialize, Deserialize, settings_value::SettingsValue)]
#[serde(rename_all = "snake_case")]
enum NewSessionShellFile {
    SystemDefault,
    Executable(String),
    MSYS2(String),
    WSL(String),
    Custom(String),
}

impl From<NewSessionShellFile> for NewSessionShell {
    fn from(f: NewSessionShellFile) -> Self {
        match f {
            NewSessionShellFile::SystemDefault => Self::SystemDefault,
            NewSessionShellFile::Executable(s) => Self::Executable(s),
            NewSessionShellFile::MSYS2(s) => Self::MSYS2(s),
            NewSessionShellFile::WSL(s) => Self::WSL(s),
            NewSessionShellFile::Custom(s) => Self::Custom(s),
        }
    }
}

impl From<&NewSessionShell> for NewSessionShellFile {
    fn from(f: &NewSessionShell) -> Self {
        match f {
            NewSessionShell::SystemDefault => Self::SystemDefault,
            NewSessionShell::Executable(s) => Self::Executable(s.clone()),
            NewSessionShell::MSYS2(s) => Self::MSYS2(s.clone()),
            NewSessionShell::WSL(s) => Self::WSL(s.clone()),
            NewSessionShell::Custom(s) => Self::Custom(s.clone()),
        }
    }
}

impl settings_value::SettingsValue for NewSessionShell {
    fn to_file_value(&self) -> serde_json::Value {
        NewSessionShellFile::from(self).to_file_value()
    }

    /// Accepts the structured form (`{ executable = "<path>" }`, `"system_default"`, ...)
    /// and, like `startup_shell_override`, a bare string that is an executable
    /// path or command (`"C:/Program Files/Git/bin/bash.exe"`). Anything else
    /// (empty string, number, unknown table) is rejected so the settings loader
    /// reports the value as invalid.
    fn from_file_value(value: &serde_json::Value) -> Option<Self> {
        if let Some(f) = NewSessionShellFile::from_file_value(value) {
            return Some(f.into());
        }
        match value {
            serde_json::Value::String(s) if !s.trim().is_empty() => {
                Some(Self::Executable(s.clone()))
            }
            _ => None,
        }
    }
}

impl NewSessionShell {
    pub fn shell_family(&self) -> ShellFamily {
        let shell = match self {
            NewSessionShell::SystemDefault => return OperatingSystem::get().default_shell_family(),
            NewSessionShell::WSL(_) => return ShellFamily::Posix,
            NewSessionShell::Executable(shell) => shell,
            NewSessionShell::MSYS2(shell) => shell,
            NewSessionShell::Custom(shell) => shell,
        };

        let path = PathBuf::from(shell);
        if let Some(file_stem) = path
            .file_stem()
            .and_then(|s| s.to_str().map(|s| s.to_lowercase()))
        {
            if file_stem.contains("powershell") || file_stem.contains("pwsh") {
                return ShellFamily::PowerShell;
            }
        }
        ShellFamily::Posix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use settings_value::SettingsValue;

    #[test]
    fn bare_string_is_an_executable() {
        assert_eq!(
            NewSessionShell::from_file_value(&json!("C:/Program Files/Git/bin/bash.exe")),
            Some(NewSessionShell::Executable(
                "C:/Program Files/Git/bin/bash.exe".into()
            ))
        );
    }

    #[test]
    fn structured_forms_still_parse() {
        assert_eq!(
            NewSessionShell::from_file_value(&json!({"executable": "/bin/zsh"})),
            Some(NewSessionShell::Executable("/bin/zsh".into()))
        );
        assert_eq!(
            NewSessionShell::from_file_value(&json!("system_default")),
            Some(NewSessionShell::SystemDefault)
        );
        assert_eq!(
            NewSessionShell::from_file_value(&json!({"wsl": "Ubuntu"})),
            Some(NewSessionShell::WSL("Ubuntu".into()))
        );
    }

    #[test]
    fn round_trips_in_the_structured_form() {
        for shell in [
            NewSessionShell::SystemDefault,
            NewSessionShell::Executable("/bin/fish".into()),
            NewSessionShell::MSYS2("msys".into()),
            NewSessionShell::WSL("Ubuntu".into()),
            NewSessionShell::Custom("nu -l".into()),
        ] {
            assert_eq!(
                NewSessionShell::from_file_value(&shell.to_file_value()),
                Some(shell)
            );
        }
        assert_eq!(
            NewSessionShell::Executable("/bin/fish".into()).to_file_value(),
            json!({"executable": "/bin/fish"})
        );
    }

    #[test]
    fn invalid_values_are_rejected_not_defaulted() {
        // `None` is what makes the settings loader report "Invalid value for ...".
        for bad in [
            json!(""),
            json!("  "),
            json!(5),
            json!({"nope": "x"}),
            json!([1]),
        ] {
            assert_eq!(NewSessionShell::from_file_value(&bad), None, "{bad}");
        }
        // Inside the real setting type (Option<..>): null is "unset", junk is an error.
        assert_eq!(
            <Option<NewSessionShell>>::from_file_value(&json!(null)),
            Some(None)
        );
        assert_eq!(
            <Option<NewSessionShell>>::from_file_value(&json!({"nope": "x"})),
            None
        );
    }
}
