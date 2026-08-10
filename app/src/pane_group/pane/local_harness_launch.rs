use std::{collections::HashMap, ffi::OsString, path::PathBuf};

use shell_words::quote as shell_quote;
use uuid::Uuid;
use warp_cli::agent::Harness;
use warp_managed_secrets::ManagedSecretValue;

use crate::ai::{
    agent_sdk::{
        driver::AgentDriverError, harness_model_env_vars, task_env_vars, validate_cli_installed,
        ClaudeHarness, ThirdPartyHarness,
    },
    ambient_agents::{
        task::{HarnessConfig, HarnessModelConfig},
        AgentConfigSnapshot, AmbientAgentTaskId,
    },
};
use crate::terminal::cli_agent_sessions::plugin_manager::plugin_manager_for;
use crate::terminal::shell::ShellType;

#[derive(Clone)]
pub(super) struct PreparedLocalHarnessLaunch {
    pub command: String,
    pub env_vars: HashMap<OsString, OsString>,
    pub run_id: String,
    pub task_id: AmbientAgentTaskId,
}

pub(super) fn normalize_local_child_harness(harness_type: &str) -> Option<Harness> {
    Harness::parse_local_child_harness(harness_type)
}

/// The harness `/orchestrate` launches children with. Fixed rather than a
/// command-line argument: the maintainer asked for this feature usable
/// before its options are settled, so `/orchestrate` deliberately has no
/// flags at all (see [`split_orchestrate_tasks`] and
/// [`compose_child_agent_prompt`]). Claude is the only harness with a real
/// local-child implementation today -- OpenCode parses but has no caller
/// exercising it yet, and Codex/Gemini/Oz are rejected by
/// `normalize_local_child_harness`.
pub(super) const ORCHESTRATE_DEFAULT_HARNESS: &str = "claude";

/// Splits a raw `/orchestrate` argument into one task per child agent.
///
/// `/orchestrate` spawns one local child per `;`-separated segment, e.g.
/// `/orchestrate write tests; update the docs` spawns two children. This is
/// deliberately the command's *entire* argument syntax -- no flags for
/// harness choice, child count, or anything else. Empty segments (a leading,
/// trailing, or doubled `;`) are dropped so `/orchestrate task;` behaves the
/// same as `/orchestrate task`.
pub(super) fn split_orchestrate_tasks(argument: &str) -> Vec<String> {
    argument
        .split(';')
        .map(str::trim)
        .filter(|task| !task.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Composes a local child agent's prompt from one `/orchestrate` task.
///
/// Deliberately a trim-only pass-through: the child receives exactly the
/// task text the user wrote for it, nothing more.
///
/// This is the prompt-composition decision for #325's local arm. Structural
/// context -- the working directory, shell, and the `OZ_PARENT_RUN_ID`
/// linkage back to the parent conversation -- is inherited automatically via
/// [`prepare_local_harness_child_launch`]'s other parameters, which the
/// caller derives from the parent pane. The parent conversation's
/// *transcript* is deliberately NOT summarized or injected into the prompt:
/// there is no cheap non-cloud way to condense an arbitrary transcript, and
/// building one (length limits, what to include, how much) is exactly the
/// kind of configuration surface the maintainer asked to defer. Each task
/// segment must therefore be a self-contained instruction; the user, not an
/// automatic summary, decides what a child needs to know.
pub(super) fn compose_child_agent_prompt(task: &str) -> String {
    task.trim().to_string()
}

pub(super) fn validate_local_harness_shell(shell_type: Option<ShellType>) -> Result<(), String> {
    match shell_type {
        Some(ShellType::Bash) | Some(ShellType::Zsh) | Some(ShellType::Fish) => Ok(()),
        Some(ShellType::PowerShell) => Err(
            "Local child harnesses currently require bash, zsh, or fish; PowerShell is not supported."
                .to_string(),
        ),
        None => Err(
            "Local child harnesses currently require a detected bash, zsh, or fish session."
                .to_string(),
        ),
    }
}

/// Instructions prepended to a local Claude child's prompt so it knows how to coordinate with
/// the lead agent via the Oz CLI messaging environment (`OZ_CLI`/`OZ_RUN_ID`/`OZ_PARENT_RUN_ID`,
/// set as env vars by `task_env_vars` below).
///
/// The pin's version of this prompt (`app/src/pane_group/pane/local_harness_launch.rs:85-113`,
/// `02b53fcd8`) points children at `oz run message *`, a client for Warp's
/// server-side hosted-CLI-task mailbox -- cloud, and physically removed from
/// this fork's `CliCommand` (`crates/warp_cli/src/lib_tests.rs`'s
/// `run_command_is_removed`). This fork's local child agents instead use `oz
/// agent message send`/`list`, a plain on-disk mailbox
/// (`crates/warp_cli/src/agent_mailbox.rs`) keyed by `OZ_RUN_ID`; see that
/// module's doc comment for why `oz run` could not be ported and this is a
/// new command under the existing `agent` surface instead.
const LOCAL_CLAUDE_CHILD_ORCHESTRATION_INSTRUCTIONS: &str = r#"You are a local Claude Code child agent launched by a lead agent in Zap.

Coordinate with the lead agent through the Oz CLI messaging environment:
- Your run id is in OZ_RUN_ID.
- The lead agent id is in OZ_PARENT_RUN_ID.
- The Oz CLI command is in OZ_CLI.

If OZ_CLI, OZ_RUN_ID, or OZ_PARENT_RUN_ID is missing, report that blocker in your final response.
Do not use Claude Code Agent or SendMessage tools to contact the lead agent; use the Oz CLI commands below.
Do not ask to inspect help before messaging. The command shapes below are complete.

Send a message to the lead agent at start, when blocked, and when complete:
"$OZ_CLI" agent message send --sender-run-id "$OZ_RUN_ID" --to "$OZ_PARENT_RUN_ID" --subject "<subject>" --body "<body>"
All four send arguments are required: --sender-run-id "$OZ_RUN_ID", --to "$OZ_PARENT_RUN_ID", --subject, and --body.
Do not pass "$OZ_PARENT_RUN_ID" as a positional argument to send.

After sending a message, and before ending or standing by, check recent inbox messages:
"$OZ_CLI" agent message list "$OZ_RUN_ID" --limit 25

Each listed message includes its full subject and body, so no separate read step is needed. If recent messages from "$OZ_PARENT_RUN_ID" are present and you have not handled them, use the latest one as task context.
"#;

pub(super) fn local_claude_child_prompt(task_prompt: &str) -> String {
    format!("{LOCAL_CLAUDE_CHILD_ORCHESTRATION_INSTRUCTIONS}\nTask:\n{task_prompt}")
}

pub(super) fn build_local_claude_child_command(prompt: &str) -> String {
    let session_id = Uuid::new_v4();
    let quoted_prompt = shell_quote(prompt);
    // Local child harness panes are launched off-screen. We intentionally skip
    // Claude's own permission prompts here so the child can start unattended
    // instead of hanging on an approval UI the user cannot see in that hidden
    // pane.
    format!("claude --session-id {session_id} --dangerously-skip-permissions {quoted_prompt}")
}

pub(super) fn build_local_opencode_child_command(prompt: &str) -> String {
    let quoted_prompt = shell_quote(prompt);
    format!("opencode --prompt {quoted_prompt}")
}

pub(super) fn build_local_codex_child_command(prompt: &str) -> String {
    let quoted_prompt = shell_quote(prompt);
    format!("codex --dangerously-bypass-approvals-and-sandbox {quoted_prompt}")
}

fn local_child_task_config(harness: Harness) -> Option<AgentConfigSnapshot> {
    match harness {
        Harness::Oz | Harness::OpenCode | Harness::Gemini | Harness::Codex | Harness::Unknown => {
            None
        }
        Harness::Claude => Some(AgentConfigSnapshot {
            harness: Some(HarnessConfig::from_harness_type(harness)),
            ..Default::default()
        }),
    }
}

pub(super) async fn prepare_local_harness_child_launch(
    prompt: String,
    harness_type: String,
    model_id: Option<String>,
    parent_run_id: Option<String>,
    shell_type: Option<ShellType>,
    startup_directory: Option<PathBuf>,
) -> Result<PreparedLocalHarnessLaunch, String> {
    // Ported from the pin (`local_harness_launch.rs:180-186`, `02b53fcd8`) for #323's
    // ANTHROPIC_MODEL merge sub-item.
    let harness_model_config = model_id
        .filter(|id| !id.is_empty())
        .map(|model_id| HarnessModelConfig {
            model_id,
            reasoning_level: None,
        });
    let Some(harness) = normalize_local_child_harness(&harness_type) else {
        let harness_name = harness_type.trim();
        return Err(if harness_name.is_empty() {
            "Local child harness type is missing.".to_string()
        } else {
            format!("Unsupported local child harness '{harness_name}'.")
        });
    };
    validate_local_harness_shell(shell_type)?;
    let command = match harness {
        Harness::Oz => unreachable!("normalize_local_child_harness filters out Oz"),
        Harness::Unknown => unreachable!("normalize_local_child_harness filters out Unknown"),
        Harness::Claude => {
            let working_dir = startup_directory
                .or_else(|| std::env::current_dir().ok())
                .ok_or_else(|| {
                    "Could not resolve a working directory for the local Claude child.".to_string()
                })?;
            let claude_harness = ClaudeHarness;
            claude_harness
                .validate()
                .map_err(|error: AgentDriverError| error.to_string())?;
            // Local child harness panes inherit the user's existing local Claude
            // auth/session state. We still prepare Claude's config files here,
            // but there are no Zap-managed secrets to materialize into the
            // hidden child pane.
            let managed_secrets: HashMap<String, ManagedSecretValue> = HashMap::new();
            claude_harness
                .prepare_environment_config(&working_dir, None, &managed_secrets)
                .map_err(|error: AgentDriverError| error.to_string())?;
            if let Some(manager) = plugin_manager_for(claude_harness.cli_agent()) {
                if let Err(error) = manager.install().await {
                    log::warn!("Claude plugin installation failed for child harness: {error}");
                }
            }

            build_local_claude_child_command(&local_claude_child_prompt(&prompt))
        }
        Harness::OpenCode => {
            validate_cli_installed("opencode", Some("https://opencode.ai/docs"))
                .map_err(|error: AgentDriverError| error.to_string())?;
            build_local_opencode_child_command(&prompt)
        }
        // `Harness::parse_local_child_harness` now recognizes "codex" (issue
        // #411's pinned-parity requirement); #323 completes the launch.
        Harness::Codex => {
            // Local Codex child panes must rely on the user's existing local
            // auth/session state, same as Claude above. Unlike Claude, there is
            // no per-child environment-config prep to run here -- the fork has
            // no `CodexHarness`/`ThirdPartyHarness` impl (only `validate()` +
            // `prepare_environment_config()` on `ClaudeHarness`/`GeminiHarness`),
            // and the pin's own Codex branch deliberately skips that shared prep
            // too ("it can seed OPENAI_API_KEY into ~/.codex/auth.json and
            // rewrite ~/.codex/config.toml for the whole machine"). A plain
            // CLI-presence check, matching the OpenCode arm above, is what's
            // actually needed before claiming success.
            validate_cli_installed("codex", Some("https://developers.openai.com/codex/cli"))
                .map_err(|error: AgentDriverError| error.to_string())?;
            build_local_codex_child_command(&prompt)
        }
        Harness::Gemini => unreachable!("normalize_local_child_harness filters out Gemini"),
    };

    // Zap (localization, Phase 3b-4): launching a local harness child task no
    // longer goes through the cloud `create_agent_task` mutation; a UUID v4
    // is generated locally as the task_id instead.
    // The `local_child_task_config(harness)` argument is no longer used.
    let _ = local_child_task_config(harness);
    let task_id = AmbientAgentTaskId::new_local();

    let mut env_vars = task_env_vars(Some(&task_id), parent_run_id.as_deref(), harness);
    // Propagate the selected model to Claude Code via ANTHROPIC_MODEL. Codex local
    // children never receive a model override -- `harness_model_env_vars` only acts
    // on `Harness::Claude`.
    env_vars.extend(harness_model_env_vars(
        harness,
        harness_model_config.as_ref(),
    ));

    Ok(PreparedLocalHarnessLaunch {
        command,
        env_vars,
        run_id: task_id.to_string(),
        task_id,
    })
}

#[cfg(test)]
#[path = "local_harness_launch_tests.rs"]
mod tests;
