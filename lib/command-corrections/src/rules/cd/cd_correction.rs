use std::path::Path;

use super::{matches_cd_doesnt_exist, parse_command_and_directory};
use crate::rules::util::correct_path_at_every_level;
use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata};

/// This rule corrects a cd command when the directory doesn't exist
pub struct CdCorrection;
impl Rule for CdCorrection {
    default_rule_id!(CdCorrection);

    fn matches(&self, command: &Command, session_metadata: &SessionMetadata) -> bool {
        matches_cd_doesnt_exist(command, session_metadata)
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        _session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let (command_name, wrong_dirname) = parse_command_and_directory(command)?;

        let wrong_dirname = Path::new(wrong_dirname);

        let corrected_path =
            correct_path_at_every_level(wrong_dirname, command.working_dir?, |path| path.is_dir())?;

        Some(vec![vec![command_name.to_owned(), corrected_path].into()])
    }
}

#[cfg(test)]
mod tests {
    use crate::rules::cd::get_pwsh_error_command_for_platform;
    use crate::test_utils::with_temp_directories;
    use crate::SessionType;
    use crate::{test_utils::corrections_given_metadata, Command, ExitCode, SessionMetadata};

    const SAMPLE_DIR_PATHS: &[&str] = &[
        "apples/bananas/oranges/mangos",
        "apples/dir2/dir3/dir4",
        "acrobat",
    ];

    #[test]
    fn test_cd_correction_relative() {
        with_temp_directories(SAMPLE_DIR_PATHS, |tempdir| {
            let command = Command::new(
                "cd aples/banannas/oranges/mans",
                "cd: no such file or directory: aples",
                ExitCode(1),
            )
            .set_working_dir(tempdir.path().to_str().unwrap());

            assert!(
                corrections_given_metadata(command, &SessionMetadata::default())
                    .contains(&"cd apples/bananas/oranges/mangos".to_owned())
            )
        });
    }

    #[test]
    fn test_cd_correction_with_remote_session() {
        with_temp_directories(SAMPLE_DIR_PATHS, |tempdir| {
            let command = Command::new(
                "cd aples/banannas/oranges/mans",
                "cd: no such file or directory: aples",
                ExitCode(1),
            )
            .set_working_dir(tempdir.path().to_str().unwrap());

            let mut session = SessionMetadata::default();
            session.set_session_type(SessionType::Remote);
            assert!(corrections_given_metadata(command, &session).is_empty())
        });
    }

    #[test]
    fn test_cd_correction_powershell() {
        use crate::{PlatformType::*, Shell};
        with_temp_directories(SAMPLE_DIR_PATHS, |tempdir| {
            let mut session = SessionMetadata::default();
            session.set_shell(Shell::PowerShell, Some(Shell::PowerShell.test_version()));

            for platform_type in [Posix, Windows] {
                session.set_platform_type(platform_type.clone());

                for command_name in ["cd", "chdir", "sl", "Set-Location"] {
                    let input = format!("{command_name} aples/banannas/oranges/mans");
                    let output = format!(
                        "{}: Cannot find path 'app' because it does not exist.",
                        get_pwsh_error_command_for_platform(command_name, platform_type.clone())
                    );

                    let command = Command::new(&input, &output, ExitCode(1))
                        .set_working_dir(tempdir.path().to_str().unwrap());
                    let expected = format!("{command_name} apples/bananas/oranges/mangos");
                    let actual = corrections_given_metadata(command, &session);
                    assert!(actual.contains(&expected.to_owned()), "expected pwsh on {platform_type} correction for '{input}' to contain '{expected}', but got '{actual:?}'")
                }
            }
        });
    }
}
