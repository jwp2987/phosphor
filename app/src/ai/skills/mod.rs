use std::path::{Path, PathBuf};

use ai::skills::SkillPathOrigin;
use warp_util::local_or_remote_path::LocalOrRemotePath;

mod telemetry;
pub use telemetry::{SkillOpenOrigin, SkillTelemetryEvent};

cfg_if::cfg_if! {
    if #[cfg(not(feature = "local_fs"))] {
        mod dummy_skill_manager;
        pub use dummy_skill_manager::SkillManager;
    }
}

pub use ai::skills::SkillReference;

/// Events emitted by [`SkillManager`] (both the real, `feature = "local_fs"`
/// implementation in `skill_manager.rs` and the no-op [`dummy_skill_manager`]).
///
/// Ported from the pin's `ai/skills/mod.rs::SkillManagerEvent` (`02b53fcd8`), moved here
/// (out of `skill_manager.rs`/`dummy_skill_manager.rs`, which each used to define their
/// own copy) so both implementations share one definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub enum SkillManagerEvent {
    InventoryChanged,
}

/// One duplicate copy of a same-name skill, as surfaced by
/// [`SkillManager::list_skill_inventory`] (fork-original — the pin has no equivalent
/// inventory feature; see `skill_manager.rs`'s module doc comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInventoryDuplicate {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub content: String,
    pub provider: ai::skills::SkillProvider,
    pub scope: ai::skills::SkillScope,
}

/// One skill name's worth of inventory: the default (highest-priority) copy plus every
/// duplicate found across provider directories. Fork-original, consumed by
/// `app/src/skill_manager/panel.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInventoryItem {
    pub name: String,
    pub default_skill: SkillInventoryDuplicate,
    pub duplicates: Vec<SkillInventoryDuplicate>,
}

impl SkillInventoryItem {
    pub fn has_duplicates(&self) -> bool {
        self.duplicates.len() > 1
    }
}

/// Error returned by [`SkillManager::active_skill_by_reference_with_origin`] when a
/// referenced skill cannot currently be invoked.
///
/// Ported from the pin's `ai/skills/mod.rs::ActiveSkillLookupError` (`02b53fcd8`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActiveSkillLookupError {
    #[error("Bundled skills are not available on this remote session")]
    BundledSkillsUnavailable,
    #[error("Skill not found: {reference}")]
    NotFound { reference: SkillReference },
}

impl ActiveSkillLookupError {
    pub(crate) fn for_reference(reference: &SkillReference, path_origin: &SkillPathOrigin) -> Self {
        if matches!(path_origin, SkillPathOrigin::Unavailable)
            && matches!(reference, SkillReference::BundledSkillId(_))
        {
            Self::BundledSkillsUnavailable
        } else {
            Self::NotFound {
                reference: reference.clone(),
            }
        }
    }
}

mod listed_skill;
pub use listed_skill::SkillDescriptor;

mod skill_utils;
pub use skill_utils::{
    icon_override_for_skill_name, list_skills, render_skill_button, skill_path_from_file_path,
    skill_path_from_location,
};

/// Query type accepted by [`SkillManager::skill_by_path`] /
/// [`SkillManager::reference_for_skill_path`], letting callers pass a bare local `&Path`
/// (the common case) or an explicit [`LocalOrRemotePath`] (for remote-aware call sites)
/// without two parallel method families.
///
/// Ported from the pin's `ai/skills/mod.rs::SkillPathQuery` (`02b53fcd8`).
pub trait SkillPathQuery {
    fn to_skill_location(&self) -> LocalOrRemotePath;
}

impl SkillPathQuery for LocalOrRemotePath {
    fn to_skill_location(&self) -> LocalOrRemotePath {
        self.clone()
    }
}

impl SkillPathQuery for Path {
    fn to_skill_location(&self) -> LocalOrRemotePath {
        LocalOrRemotePath::Local(self.to_path_buf())
    }
}

impl SkillPathQuery for PathBuf {
    fn to_skill_location(&self) -> LocalOrRemotePath {
        LocalOrRemotePath::Local(self.clone())
    }
}

#[cfg(not(target_family = "wasm"))]
mod resolve_skill_spec;
#[cfg(not(target_family = "wasm"))]
pub use resolve_skill_spec::{
    clone_repo_for_skill, resolve_skill_spec, ResolveSkillError, ResolvedSkill,
};

#[cfg(not(target_family = "wasm"))]
mod global_skills;
#[cfg(not(target_family = "wasm"))]
pub use global_skills::filter_skills_by_spec;

cfg_if::cfg_if! {
    if #[cfg(feature = "local_fs")] {
        mod bundled;
        // `BundledSkill` is the per-host catalog; `BundledSkills` multiplexes the local
        // one against per-connected-host catalogs keyed by `HostId`. Both are needed
        // outside this module: the #353 daemon producer serializes a `BundledSkill`,
        // and `remote_agent_context.rs` inserts/removes remote catalogs on `BundledSkills`.
        pub use bundled::{BundledSkill, BundledSkills};
        mod skill_manager;
        pub use skill_manager::{
            extract_skill_parent_directory, BundledSkillActivation, SkillManager,
        };
        #[allow(unused_imports)]
        pub use skill_manager::SkillWatcher;
    }
}

// The daemon-side snapshot producer (#353) needs the local bundled-skill catalog but not
// the rest of `SkillManager`'s state, so it is gated on `local_fs` alone (matching
// `bundled`) rather than piggybacking on the `SkillManager` cfg_if above.
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
mod remote;
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
pub(crate) use remote::bundled_skill_snapshot_protos;
