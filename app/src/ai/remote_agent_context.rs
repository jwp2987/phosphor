//! Client-side reconciliation of `RemoteAgentContextSnapshot`s (#438 dependent feature 1,
//! #353 producer, #487 SSH-arm reversal) into [`SkillManager`]'s per-host remote catalogs.
//!
//! Ported from the pin's `ai/remote_agent_context.rs` (`02b53fcd8`), **scoped down**: the
//! pin's `RemoteAgentContextState` also carries `global_rules` and reconciles them into
//! `ProjectContextModel` via `set_remote_global_rules`/`remove_remote_global_rules`. Those
//! methods don't exist on this fork's `ProjectContextModel` (`crates/ai/src/project_context/
//! model.rs`), which is entirely local-`PathBuf`-based with no per-host storage — adding
//! them is a separate, comparably-sized feature (remote-aware rule discovery threaded
//! through `pending_context`'s system-prompt assembly), not attempted here. The daemon
//! (`app/src/remote_server/server_model.rs`) still populates `snapshot.global_rules`
//! (#353's producer serializes it unconditionally), so a remote host's AGENTS.md/WARP.md
//! files already cross the wire — they are just not consumed client-side yet. Tracked as a
//! known gap; do not assume `global_rules` do anything today.
use ai::skills::{
    get_provider_for_path, parse_skill_content_at_location, ParsedSkill, SkillProvider,
    SkillScope,
};
use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use remote_server::proto::{remote_skill_proto, RemoteAgentContextSnapshot, RemoteSkillProto};
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
                me.reconcile_snapshot(host_id.clone(), snapshot.clone(), ctx);
                return;
            }
            if let RemoteServerManagerEvent::HostDisconnected { host_id } = event {
                me.remove_host_context(host_id, ctx);
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
        } = parse_snapshot(&host_id, snapshot);
        SkillManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.replace_remote_agent_context(
                host_id.clone(),
                bundled_skills,
                home_skills.map(|home| (home.home_dir, home.skills)),
                ctx,
            );
        });
    }

    fn remove_host_context(&mut self, host_id: &HostId, ctx: &mut ModelContext<Self>) {
        SkillManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.remove_remote_agent_context(host_id, ctx);
        });
    }
}

fn parse_snapshot(host_id: &HostId, snapshot: RemoteAgentContextSnapshot) -> RemoteAgentContextState {
    let bundled_skills = FeatureFlag::BundledSkills
        .is_enabled()
        .then(|| bundled_skill_from_protos(host_id, &snapshot.skills));
    let Some(home_dir) = remote_path(host_id, &snapshot.home_dir) else {
        safe_warn!(
            safe: ("Ignoring remote home context with an invalid home directory"),
            full: ("Ignoring remote home context with an invalid home directory for {host_id}")
        );
        return RemoteAgentContextState {
            bundled_skills,
            home_skills: None,
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
