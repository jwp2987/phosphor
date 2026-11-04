use crate::rules::pip::pip_unknown_command::PipUnknownCommand;
use crate::rules::Rule;

use super::CommandGroup;

mod pip_unknown_command;

pub(crate) fn command_group() -> CommandGroup {
    CommandGroup::new(&["pip", "pip2", "pip3"], vec![PipUnknownCommand.to_arc()])
}
