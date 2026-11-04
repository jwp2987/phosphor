use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata, Shell};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref RE_SH: Regex = Regex::new(r"[\s]*\$[\s]*(\S.*)").unwrap();
    static ref RE_PWSH: Regex = Regex::new(r"(?i)\$: The term '\$' is not recognized as a name of a cmdlet, function, script file, or executable program").unwrap();
}

/*
Fixes error for commands that begin with the shell prompt '$'.
This happens when copy-pasta'ing code-blocks in documentation.
See more here: https://github.com/nvbn/thefuck/blob/5198b34f24ca4bc414a5bf1b0288ee86ea2529a8/thefuck/rules/remove_shell_prompt_literal.py#L1
*/
pub struct LeadingShellPrompt;
impl Rule for LeadingShellPrompt {
    default_rule_id!(LeadingShellPrompt);

    fn matches(&self, command: &Command, session_metadata: &SessionMetadata) -> bool {
        match session_metadata.shell {
            Shell::PowerShell => RE_PWSH.is_match(command.output),
            _ => {
                command.output.to_lowercase().contains("command not found")
                    && RE_SH.is_match(command.input)
            }
        }
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        _session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        Some(vec![command.input_parts()[1..].into()])
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        test_utils::{corrections_given_input_output, corrections_given_metadata},
        Command, ExitCode, SessionMetadata,
        Shell::PowerShell,
    };

    #[test]
    fn test_leading_shell_prompt() {
        assert_eq!(
            corrections_given_input_output("$ git status", "zsh: command not found: $"),
            vec!["git status"]
        );
    }

    #[test]
    fn test_leading_shell_prompt_powershell() {
        let mut session = SessionMetadata::new();
        session.set_shell(PowerShell, Some(PowerShell.test_version()));

        let command = Command::new(
            "$ git status",
            "$: The term '$' is not recognized as a name of a cmdlet, function, script file, or executable program.\nCheck the spelling of the name, or if a path was included, verify that the path is correct and try again.",
            ExitCode(1)
        );
        assert_eq!(
            corrections_given_metadata(command, &session),
            vec!["git status"]
        );
    }
}
