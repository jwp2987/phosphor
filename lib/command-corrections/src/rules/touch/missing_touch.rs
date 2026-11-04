use std::path::Path;

use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref RE: Regex = Regex::new("(?i)touch: (.+):").unwrap();
}

/// Suggests to create a directory/directories before touch'ing file
pub struct MissingTouch;
impl Rule for MissingTouch {
    default_rule_id!(MissingTouch);

    fn matches(&self, command: &Command, _session_metadata: &SessionMetadata) -> bool {
        command
            .lowercase_output()
            .contains("no such file or directory")
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let path_str = RE
            .captures(command.output)
            .and_then(|captures| captures.get(1))
            .map(|regex_match| regex_match.as_str())?;
        let directory_path = Path::new(path_str).parent()?.to_str()?;

        let new_command = vec!["mkdir", "-p", directory_path];
        Some(vec![
            session_metadata.and(new_command, command.input_parts())?
        ])
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{corrections_given_input_output, test_input_output_powershell};

    #[test]
    fn test_touch() {
        assert_eq!(
            corrections_given_input_output(
                "touch random/foo",
                "touch: random/foo: No such file or directory"
            ),
            vec!["mkdir -p random && touch random/foo"]
        )
    }

    #[test]
    fn test_touch_powershell() {
        test_input_output_powershell(
            "touch random/foo",
            "touch: random/foo: No such file or directory",
            ("mkdir -p random", "touch random/foo"),
        )
    }
}
