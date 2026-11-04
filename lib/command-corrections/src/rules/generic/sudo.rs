/// Fixes error for commands that need to be run with "sudo".
/// Note: this rule is not in the `sudo` directory, because it
/// applies to rules that _don't_ start with `sudo`.
use crate::rules::Rule;
use crate::{default_rule_id, Command, PlatformType, RuleCorrection, SessionMetadata};

pub struct Sudo;

static PATTERNS: &[&str] = &[
    "permission denied",
    "eacces",
    "pkg: insufficient privileges",
    "you cannot perform this operation unless you are root",
    "non-root users cannot",
    "operation not permitted",
    "not super-user",
    "superuser privilege",
    "root privilege",
    "this command has to be run under the root user",
    "this operation requires root",
    "requested operation requires superuser privilege",
    "must be run as root",
    "must run as root",
    "must be superuser",
    "must be root",
    "need to be root",
    "need root",
    "needs to be run as root",
    "only root can",
    "you don't have access to the history db",
    "authentication is required",
    "edspermissionerror",
    "you don't have write permissions",
    "use `sudo`",
    "sudorequirederror",
    "error: insufficient privileges",
    "updatedb: can not open a temporary file",
];

static ANTI_PATTERNS: &[&str] = &["ssh:"];

impl Rule for Sudo {
    default_rule_id!(Sudo);

    fn matches(&self, command: &Command, session_metadata: &SessionMetadata) -> bool {
        // Sudo is not a thing on windows
        if session_metadata.platform_type == PlatformType::Windows {
            return false;
        }

        // If user already tried sudo, no point in suggesting it again.
        if let Some("sudo") = command.input_parts().first().map(String::as_str) {
            return false;
        }

        let lowercase_output = command.lowercase_output();
        PATTERNS
            .iter()
            .any(|pattern| lowercase_output.contains(pattern))
            && !ANTI_PATTERNS
                .iter()
                .any(|anti_pattern| lowercase_output.contains(anti_pattern))
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        _session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let new_command = [&["sudo".to_owned()], command.input_parts()].concat();
        Some(vec![new_command.into()])
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        test_utils::{corrections_given_input_output, corrections_given_metadata},
        Command, ExitCode, PlatformType, SessionMetadata,
    };

    #[test]
    fn test_single_command() {
        assert_eq!(
            corrections_given_input_output("rm file", "permission denied"),
            vec!["sudo rm file"]
        )
    }

    #[test]
    fn test_single_command_windows() {
        let mut session = SessionMetadata::new();
        session.set_platform_type(PlatformType::Windows);
        let command = Command::new("rm file", "permission denied", ExitCode(1));
        assert!(corrections_given_metadata(command, &session).is_empty())
    }
}
