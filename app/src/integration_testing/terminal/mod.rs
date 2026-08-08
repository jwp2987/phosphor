mod assertion;
mod step;
pub mod util;

pub use assertion::*;
pub use step::*;

/// Re-exported so out-of-crate integration scenarios (`crates/integration`) can
/// name the visibility scope that block predicates like `Block::is_empty`,
/// `is_visible` and `height` take. Mirrors how `integration_testing::agent_mode`
/// re-exports `AgentViewState`.
pub use crate::terminal::model::block::TranscriptScope;
