use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref RE: Regex = Regex::new(
        "(?i)unexpected argument '([^']*)' found\\s*tip: a similar argument exists: '([^']*)'"
    )
    .unwrap();
}

/*
Corrects a misspelled `cargo *` argument based on the `similar argument exists` recommendations from cargo.
*/
pub struct CargoOption;
impl Rule for CargoOption {
    default_rule_id!(CargoOption);

    fn matches(&self, command: &Command, _session_metadata: &SessionMetadata) -> bool {
        RE.is_match(command.output)
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        _session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let captures = RE.captures(command.output)?;
        let error = captures.get(1)?.as_str();
        let fix = captures.get(2)?.as_str();

        // Replace first occurrence of the incorrect argument with cargo's suggestion.
        let mut replacement = command.input_parts().to_vec();
        for part in replacement.iter_mut() {
            if part == error {
                *part = fix.to_owned();
                return Some(vec![replacement.into()]);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::corrections_given_input_output;

    #[test]
    fn test_cargo_incorrect_subcommand_option() {
        assert_eq!(
            corrections_given_input_output(
                "cargo check --all-feats",
                "error: unexpected argument '--all-feats' found

  tip: a similar argument exists: '--all-features'

Usage: cargo check --all-features

For more information, try '--help'."
            ),
            vec!["cargo check --all-features"]
        )
    }

    #[test]
    fn test_cargo_incorrect_option() {
        assert_eq!(
            corrections_given_input_output(
                "cargo --colro always",
                "error: unexpected argument '--colro' found

  tip: a similar argument exists: '--color'

Usage: cargo [+toolchain] [OPTIONS] [COMMAND]
       cargo [+toolchain] [OPTIONS] -Zscript <MANIFEST_RS> [ARGS]...

For more information, try '--help'."
            ),
            vec!["cargo --color always"]
        )
    }
}
