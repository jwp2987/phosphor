use lazy_static::lazy_static;
use regex::Regex;

use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata, Shell};

lazy_static! {
    static ref RE: Regex = Regex::new(r"(?i)(?<command>cd|chdir|sl|set-location)\.\.").unwrap();
}

/// This rule corrects a cd command that looks like "cd.."
/// Note: this is a "generic" rule because `cd..` doesn't apply to `cd`
pub struct CdParent;
impl Rule for CdParent {
    default_rule_id!(CdParent);

    fn matches(&self, command: &Command, session_metadata: &SessionMetadata) -> bool {
        match session_metadata.shell {
            Shell::PowerShell => RE.is_match(command.input),
            _ => command.input == "cd..",
        }
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let command = match session_metadata.shell {
            Shell::PowerShell => RE.captures(command.input)?.name("command")?.as_str(),
            _ => "cd",
        };
        Some(vec![vec![command, ".."].into()])
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        test_utils::{corrections_given_input_output, corrections_given_metadata},
        Command, ExitCode, SessionMetadata, Shell,
    };

    #[test]
    fn test_cd_parent() {
        assert_eq!(
            corrections_given_input_output("cd..", "zsh: command not found: cd.."),
            vec!["cd .."]
        )
    }

    #[test]
    fn test_cd_parent_powershell() {
        let mut session = SessionMetadata::new();
        session.set_shell(Shell::PowerShell, Some(Shell::PowerShell.test_version()));

        for cmd in ["cd", "chdir", "sl", "Set-Location"] {
            let input = format!("{cmd}..");
            let output = format!("{cmd}..: The term '{cmd}..' is not recognized as a name of a cmdlet, function, script file, or executable program.");
            let command = Command::new(&input, &output, ExitCode(127));
            let expected = format!("{cmd} ..");
            assert_eq!(
                corrections_given_metadata(command, &session),
                vec![expected]
            )
        }
    }
}
