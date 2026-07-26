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
//! `<available_skills><skill><name>`. `from_args` loads the name into the proto's
//! `SkillReference::SkillPath` slot (without changing the proto); the `read_skill` executor,
//! on a cache miss, first reverse-looks-up the real absolute SKILL.md path by name before
//! reading it from disk. This fallback also tolerates the model passing an absolute path
//! directly, or the old bundled-form syntax `@warp-skill:<id>`.
//!
//! ## Usage guidance (written into the description)
//!
//! The model may proactively call this in scenarios such as:
//! - The user mentions a skill name / filename / path
//! - The task matches a skill's description (e.g. "do a PR review" triggers the `review` skill)

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use warp_multi_agent_api as api;

use super::OpenAiTool;

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
                "description": "Skill name (must exactly match the <available_skills><skill><name> field in the system prompt)."
            }
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

fn from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    use api::message::tool_call::read_skill::SkillReference;
    let parsed: Args = serde_json::from_str(args)?;
    // Reuse the proto's `SkillPath` slot to carry the name (avoids a proto schema change);
    // on cache miss, the executor side reverse-looks-up the real SKILL.md path by name.
    Ok(api::message::tool_call::Tool::ReadSkill(
        api::message::tool_call::ReadSkill {
            skill_reference: Some(SkillReference::SkillPath(parsed.name)),
            name: String::new(),
        },
    ))
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
