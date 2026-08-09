use std::path::PathBuf;

use warpui::ModelContext;

use super::model::{ProjectContextModel, ProjectRule};

/// No-op stand-in for non-`local_fs` builds. File-based global rules require
/// filesystem watchers that don't exist on WASM, so callers see an empty
/// view here. Ported from the pinned oracle's `dummy_global_rules.rs`
/// (`02b53fcd8`), adapted to this fork's local-only `ProjectRule::path`
/// (`PathBuf` rather than `LocalOrRemotePath`) — see `global_rules.rs`'s
/// module doc comment for why.
#[derive(Debug, Default)]
pub(crate) struct GlobalRules;

impl GlobalRules {
    pub(crate) fn index(&mut self, _ctx: &mut ModelContext<ProjectContextModel>) {}
    pub(crate) fn active_rules(&self) -> impl Iterator<Item = ProjectRule> + '_ {
        std::iter::empty()
    }

    pub(crate) fn paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        std::iter::empty()
    }
}
