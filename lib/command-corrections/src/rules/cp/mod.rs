use crate::rules::cp::{cp_create_destination::CpCreateDestination, cp_directory::CpDirectory};
use crate::rules::Rule;

use super::CommandGroup;

mod cp_create_destination;
mod cp_directory;

pub(crate) fn command_group() -> CommandGroup {
    CommandGroup::new(
        &["cp"],
        vec![CpDirectory.to_arc(), CpCreateDestination.to_arc()],
    )
}
