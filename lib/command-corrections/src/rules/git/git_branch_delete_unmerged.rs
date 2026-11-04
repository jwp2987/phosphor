/*
 * Suggests force-deleting a branch with `-D` after deleting a branch with `-d` fails.
 * Git errors if trying to delete a branch that isn't fully merged into its upstream, or in
 * HEAD (i.e. the current branch/commit) if it has no upstream.
 */

use lazy_static::lazy_static;
use regex::Regex;

use crate::{default_rule_id, rules::Rule, Command, RuleCorrection, SessionMetadata};

pub struct GitBranchDeleteUnmerged;

lazy_static! {
    static ref RE: Regex = Regex::new("'(git branch -D [^']*)'").expect("Regex must compile");
}

impl Rule for GitBranchDeleteUnmerged {
    default_rule_id!(GitBranchDeleteUnmerged);

    fn matches(&self, command: &Command, _session_metadata: &SessionMetadata) -> bool {
        let mut parts = command.input_parts().iter();
        // Look for the `branch` subcommand, followed by the `-d` flag.
        parts.any(|part| part == "branch")
            && parts.any(|part| part == "-d")
            && RE.is_match(command.output)
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        _session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        // Git already suggests the command to run, so use that.
        let suggested_force_delete = RE
            .captures(command.output)
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str())?;

        // If this rule has false positives, we could also check that the branch name exists.

        Some(vec![suggested_force_delete.into()])
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::corrections_given_input_output;

    #[test]
    fn test_git_delete_unmerged() {
        assert_eq!(
            corrections_given_input_output(
                "git branch -d mybranch",
                "error: the branch 'mybranch' is not fully merged.
                If you are sure you want to delete it, run 'git branch -D mybranch'."
            ),
            vec!["git branch -D mybranch"]
        );
    }

    #[test]
    fn test_git_delete_other_error() {
        assert_eq!(
            corrections_given_input_output(
                "git branch -d nobranch",
                "error: branch 'nobranch' not found."
            ),
            Vec::<&'static str>::new()
        );
    }
}
