use crate::rules::touch::missing_touch::MissingTouch;
use crate::rules::Rule;

use super::CommandGroup;

mod missing_touch;

pub(crate) fn command_group() -> CommandGroup {
    CommandGroup::new(&["touch"], vec![MissingTouch.to_arc()])
}
