//! `read_skill`: reads one of Zap's Skill markdown templates.
//!
//! A Skill is a reusable workflow predefined by the user/project (a `SKILL.md` file +
//! optional metadata). Once the model reads a skill, it can advance the task following the
//! steps the user expects. warp maintains its own `SkillManager` that indexes all available
//! skills, referenceable either by name (the frontmatter `name` field) or by absolute path or
//! bundled id.
//!
//! ## Input contract
//!
//! The BYOP path exposes a `name` field, whose value comes from the system prompt's
//! `<available_skills><skill><name>`. `from_args` keeps the model's name in the proto's `name`
//! field, and [`resolve_reference`] binds it to an exact reference from **this turn's** skill
//! list before any client action is created. Only skills listed for the session's execution
//! host resolve, so a same-named SKILL.md from some other indexed repository (or host) can
//! never stand in for a listed one — `read_skill` runs without approval, so the listed set is
//! the permission (#1068). Same-named listed skills are told apart by the full `skill_path`
//! from the list; a bundled skill can also be named as `@warp-skill:<id>`.
//!
//! ## Usage guidance (written into the description)
//!
//! The model may proactively call this in scenarios such as:
//! - The user mentions a skill name / filename / path
//! - The task matches a skill's description (e.g. "do a PR review" triggers the `review` skill)

use ai::skills::{SkillPathOrigin, SkillReference};
use anyhow::Result;
use api::message::tool_call::read_skill::SkillReference as ApiSkillReference;
use serde::Deserialize;
use serde_json::{json, Value};
use agent_proto as api;
use warp_util::local_or_remote_path::LocalOrRemotePath;

use super::OpenAiTool;
use crate::ai::skills::SkillDescriptor;

/// Why a `read_skill` name did not bind to a listed skill. Surfaced to the model as a
/// structured tool result (see `chat_stream`'s parse-failure branch), never executed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkillResolutionError {
    #[error(
        "This skill is unavailable in the current session. Choose a skill from the \
         <available_skills> list in the system prompt."
    )]
    NotAvailable,
    #[error(
        "Multiple listed skills have this name. Call read_skill again with the exact \
         skill_path from the <available_skills> list."
    )]
    Ambiguous,
}

impl SkillResolutionError {
    /// The machine-readable code carried in the error payload's `reason` field.
    pub fn code(&self) -> &'static str {
        match self {
            SkillResolutionError::NotAvailable => "skill_unavailable",
            SkillResolutionError::Ambiguous => "ambiguous_skill_name",
        }
    }
}

#[derive(Debug, Deserialize)]
struct Args {
    name: String,
}

fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Skill name (must exactly match the <available_skills><skill><name> field in the system prompt); when several listed skills share a name, pass that skill's full <skill_path> instead."
            }
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

fn from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: Args = serde_json::from_str(args)?;
    // The reference is left unset here: only `resolve_reference`, which knows this turn's
    // skill list, may fill it. A ReadSkill without a reference never converts into an action
    // (`convert_read_skill` rejects it), so a name that skipped resolution reads nothing.
    Ok(api::message::tool_call::Tool::ReadSkill(
        api::message::tool_call::ReadSkill {
            skill_reference: None,
            name: parsed.name,
        },
    ))
}

/// Whether a listed skill lives on the execution host selected by `path_origin`.
///
/// The turn's list is already built for the session's host, so this is a second guard, not
/// the primary one: it keeps a remote-host skill from binding in a local session (or for a
/// different host) and vice versa. A bundled id carries no host — the executor resolves it
/// against `path_origin`'s own catalog. Path skills bind only for a live, known host: when
/// the host identity is unknown (`Unavailable`) nothing path-based can be read, because the
/// conversion that follows cannot place the path anywhere.
pub fn is_listed_for_origin(skill: &SkillDescriptor, path_origin: &SkillPathOrigin) -> bool {
    match (&skill.reference, path_origin) {
        (SkillReference::BundledSkillId(_), _) => true,
        (SkillReference::Path(LocalOrRemotePath::Local(_)), SkillPathOrigin::Local) => true,
        // `Remote`: a client-local skill is not on this host. `Unavailable` (a session whose
        // host identity is unknown, e.g. legacy SSH): `location_for_path` cannot place any
        // path there (`PathOriginUnavailable`), so a bound local path would only fail later
        // in `convert_read_skill`. `RestoredDisplayOnly` is for rendering history, never for
        // execution.
        (SkillReference::Path(LocalOrRemotePath::Local(_)), _) => false,
        (
            SkillReference::Path(LocalOrRemotePath::Remote(path)),
            SkillPathOrigin::Remote { host_id },
        ) => path.host_id == *host_id,
        (SkillReference::Path(LocalOrRemotePath::Remote(_)), _) => false,
    }
}

/// The `skill_path` a listed skill is shown with — in `<available_skills>` and in a rejected
/// call's `available_skills` — and that [`resolve_reference`] accepts in place of its name:
/// the SKILL.md path for a file skill, `@warp-skill:<id>` for a bundled one. Showing it for
/// bundled skills too is what lets the model pick one that shares its name with a file skill.
pub fn listed_skill_path(reference: &SkillReference) -> String {
    match reference {
        SkillReference::Path(path) => path.display_path(),
        SkillReference::BundledSkillId(id) => format!("@warp-skill:{id}"),
    }
}

/// Binds the model's `read_skill` name to an exact reference from the skills listed for this
/// turn (`skills`, the same `AIAgentContext::Skills` snapshot the system prompt rendered),
/// filling `read_skill.skill_reference`.
///
/// A name matches a listed skill by its frontmatter `name`, by its full listed `skill_path`,
/// or (bundled skills) by `@warp-skill:<id>`. Nothing outside the list is ever consulted —
/// there is deliberately no fallback to the `SkillManager`'s global name index (#1068).
pub fn resolve_reference(
    read_skill: &mut api::message::tool_call::ReadSkill,
    skills: &[SkillDescriptor],
    path_origin: &SkillPathOrigin,
) -> Result<(), SkillResolutionError> {
    let requested = read_skill.name.trim();
    let mut candidates = skills.iter().filter(|skill| {
        is_listed_for_origin(skill, path_origin)
            && (skill.name == requested || listed_skill_path(&skill.reference) == requested)
    });
    let selected = candidates
        .next()
        .ok_or(SkillResolutionError::NotAvailable)?;
    if candidates.any(|candidate| candidate.reference != selected.reference) {
        return Err(SkillResolutionError::Ambiguous);
    }
    read_skill.skill_reference = Some(match &selected.reference {
        SkillReference::Path(path) => ApiSkillReference::SkillPath(path.display_path()),
        SkillReference::BundledSkillId(id) => ApiSkillReference::BundledSkillId(id.clone()),
    });
    Ok(())
}

fn result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::message::tool_call_result::Result as R;
    use api::read_skill_result::Result as SR;
    let r = match result {
        R::ReadSkill(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(SR::Success(s)) => {
            // FileContent { file_path, content, line_range } is a plain single message,
            // not a oneof, so there's no inner content to unwrap.
            let (path, content) = s
                .content
                .as_ref()
                .map(|c| (c.file_path.clone(), c.content.clone()))
                .unwrap_or_default();
            json!({ "status": "ok", "path": path, "content": content })
        }
        Some(SR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static READ_SKILL: OpenAiTool = OpenAiTool {
    name: "read_skill",
    description: include_str!("../prompts/tool_descriptions/read_skill.md"),
    parameters,
    from_args,
    result_to_json,
};

#[cfg(test)]
#[path = "skill_tests.rs"]
mod tests;
