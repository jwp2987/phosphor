//! The gates that decide whether a repository gets indexed, and when.
//!
//! Restored verbatim from the pin (`02b53fcd8:app/src/ai/codebase_auto_indexing.rs`).
//! The only fork difference is behind
//! `UserWorkspaces::is_codebase_context_enabled`, which no longer consults an
//! organization-level override — see that method.
//!
//! # These predicates are also the fork's consent mechanism
//!
//! The pin carried a separate "Index Codebase?" speedbump banner
//! (`42effe840:app/src/ai/blocklist/codebase_index_speedbump_banner.rs`). It
//! was not ported, and that is a decision, not an omission — `DECLINED.md`
//! records it. The short version: at the pin the banner's consent half was
//! unreachable (its only insertion site, `/index`, indexed first and then
//! showed the banner already switched to its "Indexing codebase" progress
//! state), so it asked nothing. Here consent is expressed by
//! `CodeSettings::codebase_context_enabled`, which defaults to `false` and is
//! read by `codebase_indexing_enabled` below.
//!
//! That makes this file the load-bearing gate rather than a convenience check,
//! which is why `consent_gate_tests` exists at the bottom.

use std::collections::HashSet;
use std::hash::Hash;

use warp_core::features::FeatureFlag;
use warpui::{AppContext, SingletonEntity};

use crate::settings::CodeSettings;
use crate::workspaces::user_workspaces::UserWorkspaces;

#[derive(Clone, Copy, Debug)]
pub(crate) enum CodebaseAutoIndexingSurface {
    Local,
    Remote,
}

impl CodebaseAutoIndexingSurface {
    fn required_feature_enabled(self) -> bool {
        match self {
            Self::Local => true,
            Self::Remote => FeatureFlag::RemoteCodebaseIndexing.is_enabled(),
        }
    }
}

pub(crate) fn should_use_codebase_indexing(
    surface: CodebaseAutoIndexingSurface,
    ctx: &AppContext,
) -> bool {
    codebase_indexing_enabled(
        surface,
        UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx),
    )
}

pub(crate) fn should_auto_index_codebase(
    surface: CodebaseAutoIndexingSurface,
    ctx: &AppContext,
) -> bool {
    codebase_auto_indexing_enabled(
        surface,
        UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx),
        *CodeSettings::as_ref(ctx).auto_indexing_enabled,
    )
}

fn codebase_indexing_enabled(
    surface: CodebaseAutoIndexingSurface,
    codebase_context_enabled: bool,
) -> bool {
    FeatureFlag::FullSourceCodeEmbedding.is_enabled()
        && surface.required_feature_enabled()
        && codebase_context_enabled
}

pub(crate) fn codebase_auto_indexing_enabled(
    surface: CodebaseAutoIndexingSurface,
    codebase_context_enabled: bool,
    auto_indexing_enabled: bool,
) -> bool {
    codebase_indexing_enabled(surface, codebase_context_enabled) && auto_indexing_enabled
}

pub(crate) fn auto_index_candidate_roots<Root>(
    roots: impl IntoIterator<Item = Root>,
    mut should_request_index: impl FnMut(&Root) -> bool,
) -> Vec<Root>
where
    Root: Clone + Eq + Hash,
{
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for root in roots {
        if seen.insert(root.clone()) && should_request_index(&root) {
            candidates.push(root);
        }
    }
    candidates
}

/// The consent half of the gate, asserted separately from the ported tests.
///
/// `codebase_auto_indexing_enabled` is what the ported suite covers; this
/// covers `codebase_indexing_enabled`, which is the predicate behind
/// `should_use_codebase_indexing` and therefore the one that decides whether
/// the user's BYOP embedding API key is transmitted to a remote daemon
/// (`crate::ai::codebase_embeddings::remote_client_preferences`).
///
/// It exists because `FeatureFlag::RemoteCodebaseIndexing` is in
/// `app/Cargo.toml`'s `default` feature set — `is_enabled()` is a constant
/// `true` in a shipped build — so `surface.required_feature_enabled()` carries
/// no consent on its own. Only `codebase_context_enabled` does, and it defaults
/// to `false` (`app/src/settings/code.rs`, `code.indexing.agent_mode_codebase_context`).
#[cfg(test)]
mod consent_gate_tests {
    use super::*;

    #[test]
    fn remote_indexing_is_off_when_the_user_has_not_enabled_codebase_context() {
        let _embedding = FeatureFlag::FullSourceCodeEmbedding.override_enabled(true);
        let _remote = FeatureFlag::RemoteCodebaseIndexing.override_enabled(true);

        assert!(
            !codebase_indexing_enabled(CodebaseAutoIndexingSurface::Remote, false),
            "both feature flags on must not be enough: `RemoteCodebaseIndexing` is a \
             default-on cargo feature and expresses no user consent"
        );
        assert!(
            !codebase_indexing_enabled(CodebaseAutoIndexingSurface::Local, false),
            "the local surface takes consent from the same setting"
        );
        assert!(
            codebase_indexing_enabled(CodebaseAutoIndexingSurface::Remote, true),
            "with consent given the gate must open, or the negative case above is vacuous"
        );
    }

    #[test]
    fn remote_indexing_is_off_without_the_embedding_feature_even_with_consent() {
        let _embedding = FeatureFlag::FullSourceCodeEmbedding.override_enabled(false);
        let _remote = FeatureFlag::RemoteCodebaseIndexing.override_enabled(true);

        assert!(!codebase_indexing_enabled(
            CodebaseAutoIndexingSurface::Remote,
            true
        ));
    }
}

#[cfg(test)]
#[path = "codebase_auto_indexing_tests.rs"]
mod tests;
