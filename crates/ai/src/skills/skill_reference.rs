use serde::{Deserialize, Serialize};
use std::fmt;
use warp_util::local_or_remote_path::LocalOrRemotePath;

/// An unique reference to a skill.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum SkillReference {
    /// A skill identified by the path to its SKILL.md file.
    Path(LocalOrRemotePath),
    /// A bundled skill distributed with Zap.
    BundledSkillId(String),
}

impl SkillReference {
    /// A user-facing label for the skill. Unlike [`Display`](fmt::Display), which renders the
    /// canonical `@warp-skill:<id>` reference form for bundled skills, this omits the internal
    /// `@warp-skill:` prefix so bundled-skill copy reads the same way as path-based skill copy.
    pub fn display_label(&self) -> String {
        match self {
            SkillReference::Path(path) => path.display_path(),
            SkillReference::BundledSkillId(id) => id.clone(),
        }
    }
}

impl fmt::Display for SkillReference {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SkillReference::Path(path) => path.display_path().fmt(f),
            SkillReference::BundledSkillId(id) => write!(f, "@warp-skill:{id}"),
        }
    }
}

impl From<SkillReference> for warp_multi_agent_api::skill_descriptor::SkillReference {
    fn from(reference: SkillReference) -> Self {
        match reference {
            SkillReference::Path(path) => {
                warp_multi_agent_api::skill_descriptor::SkillReference::Path(path.display_path())
            }
            SkillReference::BundledSkillId(id) => {
                warp_multi_agent_api::skill_descriptor::SkillReference::BundledSkillId(id)
            }
        }
    }
}
