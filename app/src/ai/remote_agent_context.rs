//! Client-side reconciliation of `RemoteAgentContextSnapshot`s (#438 dependent feature 1,
//! #353 producer, #487 SSH-arm reversal, #575 global rules) into [`SkillManager`]'s
//! per-host remote skill catalogs and [`ProjectContextModel`]'s per-host remote global
//! rules.
//!
//! Ported from the pin's `ai/remote_agent_context.rs` (`02b53fcd8`). Global-rule
//! reconciliation follows the pin's shape (`set_remote_global_rules`/
//! `remove_remote_global_rules` on snapshot/disconnect) but stores raw
//! `ProjectRule { path: PathBuf, .. }` entries rather than the pin's
//! `LocalOrRemotePath`-typed ones: this fork's `ProjectContextModel::path_to_rules` has
//! no per-host dimension at all (`ProjectRule::path` has no `LocalOrRemotePath` variant),
//! so `remote_global_rules` is per-host scaffolding only — see `set_remote_global_rules`'s
//! doc comment in `crates/ai/src/project_context/model.rs`. Nothing in this fork yet
//! layers a host's stored entries into a rule lookup for a path on that host (there is no
//! remote-path-aware `pending_context` equivalent to layer them into), so the stored
//! entries are inert until such a consumer exists — the daemon-to-client wire transfer and
//! per-host storage are complete, but end-to-end "a remote host's AGENTS.md affects an
//! agent query" is not.
use ai::project_context::model::{ProjectContextModel, ProjectRule};
use ai::skills::{
    get_provider_for_path, parse_skill_content_at_location, ParsedSkill, SkillProvider,
    SkillScope,
};
use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use remote_server::proto::{remote_skill_proto, RemoteAgentContextSnapshot, RemoteSkillProto};
use std::path::PathBuf;
use warp_core::features::FeatureFlag;
use warp_core::safe_warn;
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warp_util::standardized_path::StandardizedPath;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::mcp::McpIntegration;
use super::skills::{BundledSkill, BundledSkillActivation, SkillManager};

/// Home skills parsed from a remote agent context snapshot.
struct HomeSkills {
    home_dir: LocalOrRemotePath,
    skills: Vec<ParsedSkill>,
}

/// Valid application state parsed from a remote agent context snapshot.
struct RemoteAgentContextState {
    bundled_skills: Option<BundledSkill>,
    home_skills: Option<HomeSkills>,
    /// This host's file-based global rules (e.g. `~/.agents/AGENTS.md`), as
    /// published by the daemon's `ProjectContextModel::global_rules()`. #575.
    global_rules: Vec<ProjectRule>,
}

/// Singleton that subscribes to [`RemoteServerManager`] and feeds accepted snapshots into
/// [`SkillManager`]. Has no state and no events of its own — it exists purely to own the
/// subscription for its lifetime (dropping it would unsubscribe).
pub(crate) struct RemoteAgentContext;

impl RemoteAgentContext {
    pub(crate) fn new(ctx: &mut ModelContext<Self>) -> Self {
        let remote_server_manager = RemoteServerManager::handle(ctx);
        ctx.subscribe_to_model(&remote_server_manager, |me, event, ctx| {
            if let RemoteServerManagerEvent::RemoteAgentContextSnapshot { host_id, snapshot } =
                event
            {
                // `RemoteServerManagerEvent` carries `warp_core::HostId`, but the
                // skills/context types use `warp_util::host_id::HostId`. These are
                // distinct types in this fork (they are the same type in the pin), so
                // bridge with the helper built for exactly this mismatch.
                me.reconcile_snapshot(
                    crate::code::buffer_location::core_host_id_to_util(host_id),
                    snapshot.clone(),
                    ctx,
                );
                return;
            }
            if let RemoteServerManagerEvent::HostDisconnected { host_id } = event {
                me.remove_host_context(
                    &crate::code::buffer_location::core_host_id_to_util(host_id),
                    ctx,
                );
            }
        });
        Self
    }

    fn reconcile_snapshot(
        &mut self,
        host_id: HostId,
        snapshot: RemoteAgentContextSnapshot,
        ctx: &mut ModelContext<Self>,
    ) {
        let RemoteAgentContextState {
            bundled_skills,
            home_skills,
            global_rules,
        } = parse_snapshot(&host_id, snapshot);
        SkillManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.replace_remote_agent_context(
                host_id.clone(),
                bundled_skills,
                home_skills.map(|home| (home.home_dir, home.skills)),
                ctx,
            );
        });
        ProjectContextModel::handle(ctx).update(ctx, |model, _ctx| {
            model.set_remote_global_rules(host_id, global_rules);
        });
    }

    fn remove_host_context(&mut self, host_id: &HostId, ctx: &mut ModelContext<Self>) {
        SkillManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.remove_remote_agent_context(host_id, ctx);
        });
        ProjectContextModel::handle(ctx).update(ctx, |model, _ctx| {
            model.remove_remote_global_rules(host_id);
        });
    }
}

fn parse_snapshot(host_id: &HostId, snapshot: RemoteAgentContextSnapshot) -> RemoteAgentContextState {
    let bundled_skills = FeatureFlag::BundledSkills
        .is_enabled()
        .then(|| bundled_skill_from_protos(host_id, &snapshot.skills));
    // Global rules are plain (path, content) pairs — no host-scoped parsing to fail on,
    // unlike skill/home-dir paths below, so this is collected unconditionally before the
    // early return for an invalid home directory.
    let global_rules = snapshot
        .global_rules
        .iter()
        .map(|rule| ProjectRule {
            path: PathBuf::from(&rule.path),
            content: rule.content.clone(),
        })
        .collect();
    let Some(home_dir) = remote_path(host_id, &snapshot.home_dir) else {
        safe_warn!(
            safe: ("Ignoring remote home context with an invalid home directory"),
            full: ("Ignoring remote home context with an invalid home directory for {host_id}")
        );
        return RemoteAgentContextState {
            bundled_skills,
            home_skills: None,
            global_rules,
        };
    };
    let skills = snapshot
        .skills
        .iter()
        .filter(|proto| matches!(proto.source, Some(remote_skill_proto::Source::Home(_))))
        .filter_map(|proto| {
            parse_remote_skill(
                host_id,
                proto,
                SkillScope::Home,
                Some(&home_dir),
                get_provider_for_path,
            )
        })
        .collect();
    RemoteAgentContextState {
        bundled_skills,
        home_skills: Some(HomeSkills { home_dir, skills }),
        global_rules,
    }
}

fn parse_remote_skill(
    host_id: &HostId,
    proto: &RemoteSkillProto,
    scope: SkillScope,
    required_root: Option<&LocalOrRemotePath>,
    provider_for_path: impl FnOnce(&LocalOrRemotePath) -> Option<SkillProvider>,
) -> Option<ParsedSkill> {
    let Some(path) = remote_path(host_id, &proto.path) else {
        safe_warn!(
            safe: ("Skipping remote skill with an invalid path"),
            full: ("Skipping remote skill with an invalid path: {}", proto.path)
        );
        return None;
    };
    if required_root.is_some_and(|root| !path.starts_with(root)) {
        return None;
    }
    let provider = provider_for_path(&path)?;
    match parse_skill_content_at_location(path, &proto.content, provider, scope) {
        Ok(skill) => Some(skill),
        Err(err) => {
            safe_warn!(
                safe: ("Skipping remote skill that failed to parse"),
                full: ("Skipping remote skill at {} that failed to parse: {err:#}", proto.path)
            );
            None
        }
    }
}

fn bundled_skill_from_protos(host_id: &HostId, skills: &[RemoteSkillProto]) -> BundledSkill {
    let definitions = skills.iter().filter_map(|proto| {
        let remote_skill_proto::Source::Bundled(metadata) = proto.source.as_ref()? else {
            return None;
        };
        let skill = parse_remote_skill(
            host_id,
            proto,
            SkillScope::Bundled,
            None,
            |_| Some(SkillProvider::Zap),
        )?;
        let activation = match metadata.requires_mcp.as_deref() {
            None => BundledSkillActivation::Always,
            Some(wire_id) => match mcp_integration_from_wire_id(wire_id) {
                Some(integration) => BundledSkillActivation::RequiresMcp(integration),
                None => {
                    safe_warn!(
                        safe: ("Skipping bundled skill with an unknown MCP integration"),
                        full: ("Skipping bundled skill {} with an unknown MCP integration: {wire_id}", metadata.id)
                    );
                    return None;
                }
            },
        };
        Some((metadata.id.clone(), skill, activation))
    });
    BundledSkill::from_definitions(definitions)
}

fn mcp_integration_from_wire_id(wire_id: &str) -> Option<McpIntegration> {
    match wire_id {
        "figma" => Some(McpIntegration::Figma),
        _ => None,
    }
}

fn remote_path(host_id: &HostId, path: &str) -> Option<LocalOrRemotePath> {
    StandardizedPath::try_new(path)
        .ok()
        .map(|path| LocalOrRemotePath::Remote(RemotePath::new(host_id.clone(), path)))
}

impl Entity for RemoteAgentContext {
    type Event = ();
}

impl SingletonEntity for RemoteAgentContext {}

#[cfg(test)]
#[path = "remote_agent_context_tests.rs"]
mod tests;
