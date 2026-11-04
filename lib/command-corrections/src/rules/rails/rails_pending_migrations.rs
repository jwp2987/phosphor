use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref RE: Regex = Regex::new(r"(?i)To resolve this issue, run:((.|\n)*)").unwrap();
}

/// Suggests a command to run rails migrations
pub struct RailsPendingMigrations;
impl Rule for RailsPendingMigrations {
    default_rule_id!(RailsPendingMigrations);

    fn matches(&self, command: &Command, _session_metadata: &SessionMetadata) -> bool {
        RE.is_match(command.lowercase_output())
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let migration_cmd = RE
            .captures(command.output)
            .and_then(|captures| captures.get(1))
            .map(|regex_match| regex_match.as_str().trim())?;

        Some(vec![session_metadata.and(migration_cmd, command.input)?])
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{corrections_given_input_output, test_input_output_powershell};

    #[test]
    fn test_rails_pending_migrations() {
        assert_eq!(
            corrections_given_input_output(
                "rails s",
                "Migrations are pending. To resolve this issue, run:
                
                     rails db:migrate RAILS_ENV=development
                "
            ),
            vec!["rails db:migrate RAILS_ENV=development && rails s"]
        )
    }

    #[test]
    fn test_rails_pending_migrations_powershell() {
        test_input_output_powershell(
            "rails s",
            "Migrations are pending. To resolve this issue, run:
            
                 rails db:migrate RAILS_ENV=development
            ",
            ("rails db:migrate RAILS_ENV=development", "rails s"),
        )
    }
}
