//! Helpers for resolving per-agent "global" skill specs against skills already on disk.
//!
//! Ported from the pinned oracle's `ai/skills/global_skills.rs` (`02b53fcd8`) — with one
//! deliberate cut. The pin's file has two functions:
//!
//! - `resolve_skill_repos` — parses raw spec strings and returns the unique set of GitHub
//!   repos (`crate::ai::cloud_environments::GithubRepo`) that need to be cloned for them.
//!   **Not ported.** `GithubRepo` is re-exported from `cloud_object_models`, and its module
//!   (`app/src/ai/cloud_environments/mod.rs`) imports `crate::cloud_object`,
//!   `crate::server::sync_queue`, and `crate::workspaces::user_workspaces::UserWorkspaces` —
//!   none of which exist in this fork (cloud backend dropped). More importantly, its *only*
//!   pinned caller — `AgentDriver::resolve_global_skills`
//!   (`app/src/ai/agent_sdk/driver.rs:1885-1901` at the pin) — is itself gated on
//!   `FeatureFlag::OzPlatformSkills` and reads the raw specs from
//!   `AuthStateProvider::as_ref(ctx).get().global_skills()`, i.e. a Warp **Team/workspace
//!   policy** value delivered over the cloud auth channel. So despite `GithubRepo` looking
//!   like a plain data struct, "global skills" as Warp implements it is an org-policy
//!   delivery mechanism, not a per-user local feature — the same shape as the already-
//!   declined cloud-teams/org-policy class (`DECLINED.md`, `UserWorkspaces::current_team()`,
//!   issue #445). Porting it would both re-grow the cloud surface (hard rule: no new
//!   `crate::cloud_object`/`crate::server::...` imports) and have no non-cloud way to obtain
//!   the spec list it needs. See the PR description for the full trace; this correction
//!   should be read alongside issue #487's premise that this file is "user-level skills
//!   outside a workspace" — it is not, for the specs it fetches over the network.
//!
//! - `filter_skills_by_spec` — pure, local, no cloud dependency: given skills already found
//!   on disk under a repo path, select the ones a caller's `SkillSpec`s actually asked for.
//!   **Ported below**, matching the pin's `LocalOrRemotePath`-based signature now that
//!   `ai::skills::ParsedSkill::path` carries that type (issue #299/#205 migration).
//!
//! Like in the pin, this function currently has **no production caller**: its only pinned
//! call site (`AgentDriver::load_global_skills`) is fed by the now-dropped
//! `resolve_skill_repos`/`resolve_global_skills` above. Wiring a non-cloud caller (e.g. a
//! local, non-Team-policy source of global skill specs) would be new feature work, not a
//! file port, and is left to a follow-up.

use std::collections::{HashMap, HashSet};

use ai::skills::{provider_rank, ParsedSkill};
use warp_cli::skill::SkillSpec;
use warp_util::local_or_remote_path::LocalOrRemotePath;

/// Returns the subset of skills that were explicitly requested by the given skill specs.
///
/// For simple skill names, this mirrors cached skill resolution by checking parsed skill names
/// in provider precedence order. For full-path specs, it matches the exact path relative to the
/// repo root.
pub fn filter_skills_by_spec(
    repo_path: &LocalOrRemotePath,
    skills: Vec<ParsedSkill>,
    specs: &[SkillSpec],
) -> Vec<ParsedSkill> {
    if specs.is_empty() || skills.is_empty() {
        return Vec::new();
    }

    let skills_by_path = skills
        .iter()
        .map(|skill| (skill.path.clone(), skill))
        .collect::<HashMap<_, _>>();
    let mut selected_paths = Vec::new();
    let mut seen_paths = HashSet::new();

    for spec in specs {
        if let Some(path) = matching_skill_path(repo_path, &skills_by_path, spec) {
            if seen_paths.insert(path.clone()) {
                selected_paths.push(path);
            }
        }
    }

    let selected_paths = selected_paths.into_iter().collect::<HashSet<_>>();
    skills
        .into_iter()
        .filter(|skill| selected_paths.contains(&skill.path))
        .collect()
}

fn matching_skill_path(
    repo_path: &LocalOrRemotePath,
    skills_by_path: &HashMap<LocalOrRemotePath, &ParsedSkill>,
    spec: &SkillSpec,
) -> Option<LocalOrRemotePath> {
    if spec.is_full_path() {
        let path = repo_path.join(&spec.skill_identifier);
        return skills_by_path.contains_key(&path).then_some(path);
    }
    matching_simple_skill_path(repo_path, skills_by_path, &spec.skill_identifier)
}

fn matching_simple_skill_path(
    repo_path: &LocalOrRemotePath,
    skills_by_path: &HashMap<LocalOrRemotePath, &ParsedSkill>,
    skill_name: &str,
) -> Option<LocalOrRemotePath> {
    let mut matches = skills_by_path
        .values()
        .copied()
        .filter(|skill| skill.path.starts_with(repo_path) && skill.name == skill_name)
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| {
        provider_rank(left.provider)
            .cmp(&provider_rank(right.provider))
            .then_with(|| left.path.display_path().cmp(&right.path.display_path()))
    });
    matches.into_iter().map(|skill| skill.path.clone()).next()
}

#[cfg(test)]
#[path = "global_skills_tests.rs"]
mod tests;
