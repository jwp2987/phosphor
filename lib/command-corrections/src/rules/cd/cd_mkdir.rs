use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata, Shell};

use super::{matches_cd_doesnt_exist, parse_command_and_directory};

/// This rule corrects a cd command when the directory doesn't exist
pub struct CdMkdir;
impl Rule for CdMkdir {
    default_rule_id!(CdMkdir);

    fn matches(&self, command: &Command, session_metadata: &SessionMetadata) -> bool {
        matches_cd_doesnt_exist(command, session_metadata)
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let (_, dirname) = parse_command_and_directory(command)?;

        let new_command = if session_metadata.shell == Shell::PowerShell {
            vec!["New-Item", "-ItemType", "Directory", "-Path", dirname]
        } else {
            vec!["mkdir", "-p", dirname]
        };

        Some(vec![
            session_metadata.and(new_command, command.input_parts())?
        ])
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        rules::cd::get_pwsh_error_command_for_platform,
        test_utils::{
            corrections_given_input_output, corrections_given_metadata,
            test_input_output_powershell,
        },
        Command, ExitCode, SessionMetadata, SessionType,
    };

    #[test]
    fn test_cd_mkdir_for_bash_and_zsh() {
        assert_eq!(
            corrections_given_input_output("cd app", "cd: no such file or directory: app"),
            vec!["mkdir -p app && cd app"]
        )
    }

    #[test]
    fn test_cd_mkdir_for_fish() {
        assert_eq!(
            corrections_given_input_output("cd app", "cd: The directory 'app' does not exist"),
            vec!["mkdir -p app && cd app"]
        )
    }

    #[test]
    fn test_cd_mkdir_with_remote_session() {
        let command = Command::new("cd app", "cd: no such file or directory: app", ExitCode(1));
        let mut session = SessionMetadata::default();
        session.set_session_type(SessionType::Remote);
        assert!(corrections_given_metadata(command, &session).is_empty())
    }

    #[test]
    fn test_cd_mkdir_powershell() {
        use crate::{PlatformType::*, Shell::*};
        let mut session = SessionMetadata::default();
        session.set_shell(PowerShell, Some(PowerShell.test_version()));

        for platform_type in [Posix, Windows] {
            session.set_platform_type(platform_type.clone());
            for command_name in ["cd", "sl", "chdir", "Set-Location"] {
                let input = format!("{command_name} app");

                let output = format!(
                    "{}: Cannot find path 'app' because it does not exist.",
                    get_pwsh_error_command_for_platform(command_name, platform_type.clone())
                );

                let command = Command::new(&input, &output, ExitCode(1));
                let expected_cmd =
                    format!("New-Item -ItemType Directory -Path app && {command_name} app");
                let expected = vec![expected_cmd.as_str()];
                let actual = corrections_given_metadata(command, &session);

                assert_eq!(
                    actual,
                    expected,
                    "expected pwsh on {platform_type} to correct '{input}' to {expected:?}, but got {actual:?}",
                );
            }
        }
    }

    #[test]
    fn test_cd_mkdir_old_powershell() {
        test_input_output_powershell(
            "cd app",
            "cd: no such file or directory: app",
            ("New-Item -ItemType Directory -Path app", "cd app"),
        )
    }
}
