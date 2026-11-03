use serde::{Deserialize, Deserializer, Serialize};

/// A user setting for the shell to start new terminal sessions with.
///
/// Users choose between their login shell, the default versions of zsh/bash/fish
/// (if installed, the first matching executable on their `$PATH`), and a
/// custom path or command.
#[derive(Debug, Clone, Default, PartialEq, Eq, schemars::JsonSchema)]
#[schemars(
    with = "Option<String>",
    description = "Shell to start terminal sessions with. Use null for the system default, or one of \"bash\", \"zsh\", \"fish\", \"nu\", \"pwsh\", or a custom shell command/path."
)]
pub enum StartupShell {
    #[default]
    Default,
    Bash,
    Fish,
    Zsh,
    PowerShell,
    Nu,
    Custom(String),
}

impl StartupShell {
    /// Returns the command for this startup shell.
    pub fn shell_command(&self) -> Option<&str> {
        match self {
            StartupShell::Default => None,
            StartupShell::Bash => Some("bash"),
            StartupShell::Fish => Some("fish"),
            StartupShell::Zsh => Some("zsh"),
            StartupShell::PowerShell => Some("pwsh"),
            StartupShell::Nu => Some("nu"),
            StartupShell::Custom(shell) => Some(shell),
        }
    }
}

impl Serialize for StartupShell {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.shell_command().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StartupShell {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Option<String> = Option::deserialize(deserializer)?;
        Ok(value.into())
    }
}

impl settings_value::SettingsValue for StartupShell {
    /// A bare string (or null) as always, plus the structured form that
    /// `new_session_shell_override` uses (`{ executable = "<path>" }`,
    /// `{ custom = "<cmd>" }`), so both settings accept the same spellings
    /// (#1553). Anything else is rejected and reported as an invalid value.
    fn from_file_value(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Null | serde_json::Value::String(_) => {
                serde_json::from_value(value.clone()).ok()
            }
            serde_json::Value::Object(_) => {
                match <super::NewSessionShell as settings_value::SettingsValue>::from_file_value(
                    value,
                )? {
                    super::NewSessionShell::Executable(s) | super::NewSessionShell::Custom(s)
                        if !s.trim().is_empty() =>
                    {
                        Some(Some(s).into())
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl From<Option<String>> for StartupShell {
    fn from(value: Option<String>) -> Self {
        match value {
            None => StartupShell::Default,
            Some(shell) if shell == "bash" => StartupShell::Bash,
            Some(shell) if shell == "zsh" => StartupShell::Zsh,
            Some(shell) if shell == "fish" => StartupShell::Fish,
            Some(shell) if shell == "pwsh" => StartupShell::PowerShell,
            Some(shell) if shell == "nu" => StartupShell::Nu,
            Some(shell) => StartupShell::Custom(shell),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use settings_value::SettingsValue;

    #[test]
    fn bare_string_and_structured_forms_agree() {
        let want = StartupShell::Custom("C:/Git/bin/bash.exe".into());
        assert_eq!(
            StartupShell::from_file_value(&json!("C:/Git/bin/bash.exe")),
            Some(want.clone())
        );
        assert_eq!(
            StartupShell::from_file_value(&json!({"executable": "C:/Git/bin/bash.exe"})),
            Some(want)
        );
        assert_eq!(
            StartupShell::from_file_value(&json!("zsh")),
            Some(StartupShell::Zsh)
        );
        assert_eq!(
            StartupShell::from_file_value(&json!(null)),
            Some(StartupShell::Default)
        );
    }

    #[test]
    fn invalid_values_are_rejected() {
        for bad in [
            json!(7),
            json!({"wsl": "Ubuntu"}),
            json!({"executable": ""}),
            json!([]),
        ] {
            assert_eq!(StartupShell::from_file_value(&bad), None, "{bad}");
        }
    }
}
