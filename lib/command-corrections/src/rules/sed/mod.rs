use crate::rules::sed::sed_unterminated_s::SedUnterminatedS;
use crate::rules::Rule;

use super::CommandGroup;

mod sed_unterminated_s;

pub(crate) fn command_group() -> CommandGroup {
    CommandGroup::new(&["sed"], vec![SedUnterminatedS.to_arc()])
}
