use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata};

pub struct SshAdd;
impl Rule for SshAdd {
    default_rule_id!(SshAdd);

    fn matches(&self, command: &Command, _session_metadata: &SessionMetadata) -> bool {
        command.lowercase_output().contains("ssh:")
            && command.lowercase_output().contains("permission denied")
    }

    fn generate_command_corrections<'a>(
        &self,
        _command: &'a Command,
        _session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        Some(vec!["ssh-add -K /path/to/private/key".into()])
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::corrections_given_input_output;

    #[test]
    fn test_ssh_add() {
        assert_eq!(
            corrections_given_input_output(
                "cargo build",
                "Unable to update ssh://git@github.com: Permission denied (publickey)."
            ),
            vec!["ssh-add -K /path/to/private/key"]
        )
    }
}
