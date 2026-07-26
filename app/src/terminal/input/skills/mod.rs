mod data_source;
mod view;

pub use data_source::{
    AcceptSkill, SelectableSkill, SkillSelectorDataSource, UpdatedAvailableSkills,
    query_selectable_skills,
};
pub use view::{InlineSkillSelectorEvent, InlineSkillSelectorView};

/// Shown when a local skill is invoked against a remote machine. Ported from
/// warp/master `terminal/input/skills/core.rs`.
pub const LOCAL_SKILLS_REMOTE_EXECUTION_ERROR_MESSAGE: &str = "Local skills cannot run on a remote machine. Try forking the conversation locally and running the skill.";
