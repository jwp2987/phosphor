use crate::rules::util::get_single_closest_match_with_cutoff;
use crate::rules::Rule;
use crate::{default_rule_id, Command, RuleCorrection, SessionMetadata};
use itertools::Itertools;

/// We choose a strict score cutoff because we don't want the history
/// rule to be spammy.
const MATCH_SCORE_CUTOFF: f32 = 0.9;

/// Returns the set of commands from the session_metadata's history
/// that succeeded and do not match the command itself.
/// If match_dir is true, only history items that were run in
/// the same working dir as the command itself are returned.
fn get_history_entries_excluding_command<'a>(
    command: &'_ Command,
    session_metadata: &'a SessionMetadata,
    match_dir: bool,
) -> Vec<&'a str> {
    session_metadata
        .history
        .iter()
        .copied()
        .filter(|history_item| history_item.command != command.input)
        .filter(|history_item| history_item.exit_code.is_success())
        .filter(|history_item| {
            if match_dir {
                command.working_dir == Some(history_item.pwd)
            } else {
                true
            }
        })
        .map(|c| c.command)
        .collect_vec()
}

/// Suggest command corrections based on the user's history.
/// History items that were run in the same working dir as the command
/// to correct itself are prioritized.
pub struct History;

impl Rule for History {
    default_rule_id!(History);

    fn matches(&self, _command: &Command, _session_metadata: &SessionMetadata) -> bool {
        // There's not much to test for here other than checking if the history has a similar command,
        // which we simply do in `generate_command_corrections` instead.
        true
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        let history_items_in_same_dir =
            get_history_entries_excluding_command(command, session_metadata, true);
        let history_items_in_any_dir =
            get_history_entries_excluding_command(command, session_metadata, false);

        let fix_in_same_dir = get_single_closest_match_with_cutoff(
            command.input,
            history_items_in_same_dir,
            MATCH_SCORE_CUTOFF,
        );
        let fix_in_any_dir = get_single_closest_match_with_cutoff(
            command.input,
            history_items_in_any_dir,
            MATCH_SCORE_CUTOFF,
        );
        Some(
            fix_in_same_dir
                .into_iter()
                .chain(fix_in_any_dir)
                .map(|fix| fix.into())
                .collect_vec(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{test_utils::corrections_given_metadata, Command, HistoryItem, SessionMetadata};
    use lazy_static::lazy_static;

    lazy_static! {
        static ref HISTORY: [HistoryItem<'static>; 2] = [
            HistoryItem::new("./super-script -f", 0, "/Users/suraj/foo"),
            HistoryItem::new("./superscripts -f", 0, "/Users/suraj/bar"),
        ];
    }

    #[test]
    fn test_history_correction_without_pwd() {
        let command = Command::new("./superscript -f", "no such file or directory", 127.into());
        let mut metadata = SessionMetadata::new();
        metadata.set_history(*HISTORY);

        assert_eq!(
            corrections_given_metadata(command, &metadata),
            vec!["./super-script -f"]
        );
    }

    #[test]
    fn test_history_correction_with_pwd() {
        let command = Command::new("./superscript -f", "no such file or directory", 127.into())
            .set_working_dir("/Users/suraj/bar");
        let mut metadata = SessionMetadata::new();
        metadata.set_history(*HISTORY);

        assert_eq!(
            corrections_given_metadata(command, &metadata),
            vec!["./superscripts -f", "./super-script -f"]
        );
    }
}
