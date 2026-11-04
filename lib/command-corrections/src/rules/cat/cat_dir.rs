use crate::rules::Rule;
use crate::{default_rule_id, Command, PlatformType, RuleCorrection, SessionMetadata, Shell};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref RE_SH_POSIX: Regex = Regex::new("(?i)cat: (?<directory>.+): is a directory").unwrap();
    static ref RE_PWSH_POSIX: Regex =
        Regex::new("(?i)(gc|type|get-content): Unable to get content because it is a directory: '(?<directory>.+)'. Please use '(?<command>.+)' instead.").unwrap();
    static ref RE_PWSH_WINDOWS: Regex =
        Regex::new("(?i)(cat|gc|type|get-content): Access to the path '(?<directory>.+)' is denied").unwrap();
}

/// Replaces cat with ls when command tries to cat a directory.
/// See more here: https://github.com/nvbn/thefuck/blob/5198b34f24ca4bc414a5bf1b0288ee86ea2529a8/thefuck/rules/cat_dir.py
pub struct CatDir;

fn get_patterns(shell: &Shell, platform: &PlatformType) -> Vec<&'static Regex> {
    match (shell, platform) {
        (Shell::PowerShell, PlatformType::Windows) => vec![&RE_PWSH_WINDOWS],
        (Shell::PowerShell, PlatformType::Posix) => {
            vec![&RE_SH_POSIX, &RE_PWSH_POSIX]
        }
        (_, _) => vec![&RE_SH_POSIX],
    }
}

impl Rule for CatDir {
    default_rule_id!(CatDir);

    fn matches(&self, command: &Command, session_metadata: &SessionMetadata) -> bool {
        get_patterns(&session_metadata.shell, &session_metadata.platform_type)
            .iter()
            .any(|pattern| pattern.is_match(command.output))
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let captures = get_patterns(&session_metadata.shell, &session_metadata.platform_type)
            .iter()
            .filter_map(|pattern| pattern.captures(command.output))
            .next()?;

        let dirname = captures.name("directory").map(|m| m.as_str())?;
        let command = captures
            .name("command")
            .map(|m| m.as_str())
            .unwrap_or_else(|| match &session_metadata.platform_type {
                PlatformType::Windows => "Get-ChildItem",
                PlatformType::Posix => "ls",
            });

        // We can discard all other options since they won't be applicable to `ls`
        let new_command = vec![command.to_owned(), dirname.to_owned()];
        Some(vec![new_command.into()])
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{corrections_given_input_output, corrections_given_metadata};

    #[test]
    fn test_cat_dir() {
        assert_eq!(
            corrections_given_input_output("cat -b src", "cat: src: Is a directory"),
            vec!["ls src"]
        )
    }

    #[test]
    fn test_cat_dir_with_spaces() {
        assert_eq!(
            corrections_given_input_output("cat foo\\ bar", "cat: foo bar: Is a directory"),
            vec![r#"ls 'foo bar'"#]
        )
    }

    #[test]
    fn test_cat_dir_with_powershell_on_posix() {
        use crate::{Command, ExitCode, SessionMetadata, Shell};

        for command_str in ["get-content", "gc", "type"] {
            let input = format!("{command_str} foo");
            let output = format!("{command_str}: Unable to get content because it is a directory: 'foo'. Please use 'Get-ChildItem' instead.");
            let mut metadata = SessionMetadata::default();

            metadata.set_shell(Shell::Bash, Some(Shell::Bash.test_version()));
            let command = Command::new(&input, &output, ExitCode(1));
            let results = corrections_given_metadata(command, &metadata);
            assert!(
                results.is_empty(),
                "Expected bash command '{command_str}' to have no corrections, but got {results:?}"
            );

            metadata.set_shell(Shell::PowerShell, Some(Shell::PowerShell.test_version()));
            let command = Command::new(&input, &output, ExitCode(1));
            let results = corrections_given_metadata(command, &metadata);
            let expected = "Get-ChildItem foo";
            assert_eq!(results, vec![expected], "Expected pwsh command '{command_str}' to correct to '{expected}', but got {results:?}");
        }
    }

    #[test]
    fn test_cat_dir_with_powershell_on_windows() {
        use crate::{Command, ExitCode, PlatformType, SessionMetadata, Shell};

        for command_str in ["cat", "get-content", "gc", "type"] {
            let input = format!("{command_str} foo");
            let output = format!("{command_str}: Access to the path 'foo' is denied");
            let mut metadata = SessionMetadata::default();

            metadata.set_shell(Shell::Bash, Some(Shell::Bash.test_version()));
            metadata.set_platform_type(PlatformType::Windows);
            let command = Command::new(&input, &output, ExitCode(1));
            let results = corrections_given_metadata(command, &metadata);
            assert!(
                results.is_empty(),
                "Expected bash command '{command_str}' to have no corrections, but got {results:?}"
            );

            metadata.set_shell(Shell::PowerShell, Some(Shell::PowerShell.test_version()));
            metadata.set_platform_type(PlatformType::Windows);
            let command = Command::new(&input, &output, ExitCode(1));
            let results = corrections_given_metadata(command, &metadata);
            let expected = "Get-ChildItem foo";
            assert_eq!(results, vec![expected], "Expected pwsh command '{command_str}' to correct to '{expected}', but got {results:?}");
        }
    }
}
