//! Codex CLI harness driver.
//!
//! Drives the user's locally installed `codex` binary the same way
//! `claude_code.rs` drives `claude` and `gemini.rs` drives `gemini`: prepare
//! `~/.codex` config, launch the TUI in the agent's terminal, watch the CLI
//! session, and send `/exit` when the run is done. Nothing here talks to a
//! hosted backend.
//!
//! Ported from the pinned oracle (`02b53fcd8`, Warp `2026.07.29.09.05` stable —
//! see `ORACLE.md`), `app/src/ai/agent_sdk/driver/harness/codex.rs`, for #323.
//!
//! ## What was cut, and why
//!
//! The oracle's Codex harness touches `server_api::harness_support` in exactly one
//! place: saving the conversation. `save_conversation` there does two uploads —
//! `super::upload_current_block_snapshot` and a local `upload_transcript` that reads
//! the rollout JSONL, wraps it in a `CodexTranscriptEnvelope`, asks the server for a
//! signed upload target (`HarnessSupportClient::get_transcript_upload_target`) and
//! PUTs the bytes (`harness_support::upload_to_target`). This fork dropped the hosted
//! backend entirely (`AGENTS.md` §5.10) — `HarnessSupportClient`, `upload_to_target`
//! and `ServerApi` do not exist here — so that seam is cut and `save_conversation`
//! keeps only the local half: it still resolves the on-disk rollout path (which is
//! what proves the session id was captured), then logs instead of uploading. This is
//! the same treatment `claude_code.rs` and `gemini.rs` already give their saves.
//!
//! Restoring upload later needs, in order: a transcript-sink trait to stand in for
//! `HarnessSupportClient`, a `create_external_conversation` equivalent so `start`
//! mints a server-side conversation id instead of a local [`AIConversationId`], and
//! `super::upload_current_block_snapshot` for the block half. The envelope-building
//! half is already here and tested — see `codex_transcript.rs`.
//!
//! Three further oracle trait methods are absent because this fork's
//! `ThirdPartyHarness` has no such methods, not because Codex lacks the behaviour:
//! - `fetch_resume_payload` — cloud resume; needs `ResumePayload` / the resume
//!   plumbing in `build_runner`, none of which exists here (see `claude_transcript.rs`,
//!   which cut the matching Claude wrappers for the same reason).
//! - `auth_check_command` (`codex login status`) — tracked with the rest of the
//!   `auth_check_command` surface in #289; adding it here alone would be an
//!   uncallable method.
//! - `requires_verified_platform_plugin` (`FeatureFlag::CodexPlugin`) — same: no
//!   caller in this fork's driver. Codex plugin installation still happens, via
//!   `plugin_manager_for(CLIAgent::Codex)` in `AgentDriver::setup_harness`.
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tempfile::NamedTempFile;
use uuid::Uuid;
use warp_cli::agent::Harness;
use warp_managed_secrets::ManagedSecretValue;
use warpui::{ModelHandle, ModelSpawner, SingletonEntity};

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent_events::AgentEventStreamClient;
use crate::ai::ambient_agents::task::HarnessModelConfig;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::mcp::{JSONMCPServer, JSONTransportType};
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::terminal::model::block::BlockId;
use crate::terminal::CLIAgent;

use super::super::terminal::{CommandHandle, TerminalDriver};
use super::super::{AgentDriver, AgentDriverError};
use super::codex_transcript::{codex_sessions_root, find_session_file};
use super::json_utils::read_json_file_or_default;
use super::{write_temp_file, HarnessRunner, SavePoint, ThirdPartyHarness};

pub(crate) struct CodexHarness;

/// Slash command Codex's TUI recognises as a graceful shutdown.
const CODEX_EXIT_COMMAND: &str = "/exit";
/// Allow the Zap-installed Codex plugin hooks to run in vetted driver sessions
/// without requiring an unattended `/hooks` review step.
const CODEX_BYPASS_HOOK_TRUST_FLAG: &str = "--dangerously-bypass-hook-trust";

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl ThirdPartyHarness for CodexHarness {
    fn harness(&self) -> Harness {
        Harness::Codex
    }

    fn cli_agent(&self) -> CLIAgent {
        CLIAgent::Codex
    }

    fn install_docs_url(&self) -> Option<&'static str> {
        Some("https://developers.openai.com/codex/cli")
    }

    /// Ported verbatim from the pin — these are literal Codex CLI output substrings,
    /// not cloud-specific.
    fn runtime_error_patterns(&self) -> &'static [&'static str] {
        &[
            // Quota / billing.
            "Quota exceeded. Check your plan and billing details.",
            "You've hit your usage limit",
            // Upstream HTTP failures Codex surfaces verbatim. The 401 form
            // matches invalid-API-key and wrong-endpoint variants.
            "unexpected status 401",
            "Incorrect API key provided",
            "invalid API key",
            // Region/endpoint block (Anthropic-style global vs US-only
            // routing surfaced through Codex's upstream client).
            "Access blocked by Cloudflare",
            // OAuth refresh failures — all five Codex variants share this
            // substring (see upstream session/token messages).
            "could not be refreshed",
            // Generically check for invalid request errors.
            // Keep this last so more specific patterns can be matched first.
            "\"type\": \"invalid_request_error\"",
        ]
    }

    /// Seed `~/.codex` (or `$CODEX_HOME`) before launching the CLI.
    ///
    /// The oracle threads resolved secret env vars, resolved MCP servers and the
    /// selected harness model down from the run request. This fork's
    /// `ThirdPartyHarness::prepare_environment_config` now carries the MCP servers
    /// and the model config too; only the resolved secret env vars are still absent
    /// from the trait, so they are rebuilt here from the same `build_secret_env_vars`
    /// the driver uses for the terminal session, preserving the oracle's precedence
    /// rules exactly (a worker-injected process env var beats a managed secret).
    fn prepare_environment_config(
        &self,
        working_dir: &Path,
        system_prompt: Option<&str>,
        secrets: &HashMap<String, ManagedSecretValue>,
        resolved_mcp_servers: &HashMap<String, JSONMCPServer>,
        third_party_harness_model_config: Option<&HarnessModelConfig>,
    ) -> Result<(), AgentDriverError> {
        let resolved_env_vars = super::super::build_secret_env_vars(secrets);
        prepare_codex_environment_config(
            working_dir,
            system_prompt,
            &resolved_env_vars,
            secrets,
            resolved_mcp_servers,
            third_party_harness_model_config,
        )
        .map_err(|error| AgentDriverError::HarnessConfigSetupFailed {
            harness: self.cli_agent().command_prefix().to_owned(),
            error,
        })
    }

    fn build_runner(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        resumption_prompt: Option<&str>,
        working_dir: &Path,
        _task_id: Option<AmbientAgentTaskId>,
        _agent_event_stream_client: Arc<dyn AgentEventStreamClient>,
        terminal_driver: ModelHandle<TerminalDriver>,
        _resolved_mcp_servers: &HashMap<String, JSONMCPServer>,
    ) -> Result<Box<dyn HarnessRunner>, AgentDriverError> {
        // Mirror Claude harness behavior: prepend the resumption preamble to the
        // user-turn prompt so codex treats it as immediate intent.
        // Order: resumption_prompt → prompt. (The oracle also splices a `context`
        // segment between the two; this fork's `build_runner` has no `context`
        // parameter, matching `claude_code.rs`.)
        let owned_prompt = match resumption_prompt {
            Some(preamble) if !preamble.is_empty() => format!("{preamble}\n\n{prompt}"),
            _ => prompt.to_string(),
        };
        Ok(Box::new(CodexHarnessRunner::new(
            self.cli_agent().command_prefix(),
            &owned_prompt,
            system_prompt,
            working_dir,
            terminal_driver,
        )?))
    }
}

/// Build the shell command that launches the Codex TUI.
///
/// `--dangerously-bypass-approvals-and-sandbox` disables both the sandbox and approval
/// prompts so the agent can run autonomously.
/// `--dangerously-bypass-hook-trust` allows the orchestration plugin hooks installed by
/// Zap to run without a manual hook review in unattended driver sessions.
/// `Some(session_id)` indicates that we want to resume that prior session. Unlike claude,
/// codex does not support assigning a session_id to a new conversation.
fn codex_command(cli_name: &str, session_id: Option<&Uuid>, prompt_path: &str) -> String {
    match session_id {
        Some(session_id) => format!(
            "{cli_name} resume --dangerously-bypass-approvals-and-sandbox {CODEX_BYPASS_HOOK_TRUST_FLAG} {session_id} \
             \"$(cat '{prompt_path}')\""
        ),
        None => {
            format!(
                "{cli_name} --dangerously-bypass-approvals-and-sandbox {CODEX_BYPASS_HOOK_TRUST_FLAG} \"$(cat '{prompt_path}')\""
            )
        }
    }
}

enum CodexRunnerState {
    Preexec,
    Running {
        conversation_id: AIConversationId,
        block_id: BlockId,
    },
}

struct CodexHarnessRunner {
    command: String,
    // The oracle also keeps a `cli_name` here, but only to serve
    // `HarnessRunner::harness_name`, which this fork's trait does not declare.
    /// Held so the temp file is cleaned up when the runner is dropped.
    _temp_prompt_file: NamedTempFile,
    terminal_driver: ModelHandle<TerminalDriver>,
    state: Mutex<CodexRunnerState>,
    /// Codex session UUID. Populated lazily by [`HarnessRunner::handle_session_update`]
    /// once the codex hooks emit `SessionStart`. Set once (using `OnceLock`).
    session_id: OnceLock<Uuid>,
    /// Path to the codex session rollout JSONL file. Populated by the first
    /// successful [`find_session_file`] walk so that subsequent saves skip the YYYY/MM/DD
    /// directory walk and read the JSONL file directly.
    transcript_path: OnceLock<PathBuf>,
}

impl CodexHarnessRunner {
    fn new(
        cli_command: &str,
        prompt: &str,
        _system_prompt: Option<&str>,
        _working_dir: &Path,
        terminal_driver: ModelHandle<TerminalDriver>,
    ) -> Result<Self, AgentDriverError> {
        let temp_file = write_temp_file("oz_prompt_", prompt)?;
        let prompt_path = temp_file.path().display().to_string();

        // The oracle seeds `session_id` / `transcript_path` here from a cloud
        // `CodexResumeInfo`. Without a resume payload both start empty and are
        // filled in by `handle_session_update` / `resolve_transcript_path`.
        let command = codex_command(cli_command, None, &prompt_path);

        Ok(Self {
            command,
            _temp_prompt_file: temp_file,
            terminal_driver,
            state: Mutex::new(CodexRunnerState::Preexec),
            session_id: OnceLock::new(),
            transcript_path: OnceLock::new(),
        })
    }

    /// Return the filepath for the session transcript, walking the codex sessions tree to find it on the
    /// first save call.
    async fn resolve_transcript_path(&self) -> Option<PathBuf> {
        if let Some(cached) = self.transcript_path.get() {
            return Some(cached.clone());
        }
        let session_id = self.session_id.get().copied()?;
        let resolved = tokio::task::spawn_blocking(move || -> Option<PathBuf> {
            let root = codex_sessions_root().ok()?;
            find_session_file(&root, session_id)
        })
        .await
        .ok()
        .flatten()?;
        let _ = self.transcript_path.set(resolved.clone());
        Some(resolved)
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl HarnessRunner for CodexHarnessRunner {
    async fn start(
        &self,
        foreground: &ModelSpawner<AgentDriver>,
    ) -> Result<CommandHandle, AgentDriverError> {
        let conversation_id = AIConversationId::new();
        log::info!("Created local Codex conversation {conversation_id}");

        let command = self.command.clone();
        let terminal_driver = self.terminal_driver.clone();
        let command_handle = foreground
            .spawn(move |_, ctx| {
                terminal_driver.update(ctx, |driver, ctx| driver.execute_command(&command, ctx))
            })
            .await??
            .await?;

        // Only store conversation info once the CLI command has started.
        *self.state.lock() = CodexRunnerState::Running {
            conversation_id,
            block_id: command_handle.block_id().clone(),
        };

        Ok(command_handle)
    }

    async fn exit(&self, foreground: &ModelSpawner<AgentDriver>) -> Result<()> {
        log::info!("Sending /exit to Codex CLI");
        let terminal_driver = self.terminal_driver.clone();
        foreground
            .spawn(move |_, ctx| {
                terminal_driver.update(ctx, |driver, ctx| {
                    driver.send_text_to_cli(CODEX_EXIT_COMMAND.to_string(), ctx);
                });
            })
            .await
            .map_err(|_| anyhow::anyhow!("Agent driver dropped while sending /exit"))
    }

    /// Capture the codex session ID from the `SessionStart` event picked up by the `CLIAgentSessionsModel`.
    ///
    /// Relies on codex hooks being set up to emit this event correctly.
    async fn handle_session_update(&self, foreground: &ModelSpawner<AgentDriver>) -> Result<()> {
        if self.session_id.get().is_some() {
            return Ok(());
        }
        let terminal_driver = self.terminal_driver.clone();
        let session_id_str = foreground
            .spawn(move |_, ctx| {
                let terminal_view_id = terminal_driver.as_ref(ctx).terminal_view().id();
                CLIAgentSessionsModel::handle(ctx)
                    .as_ref(ctx)
                    .session(terminal_view_id)
                    .and_then(|s| s.session_context.session_id.clone())
            })
            .await
            .ok()
            .flatten();
        let Some(session_id_str) = session_id_str else {
            return Ok(());
        };
        match Uuid::parse_str(&session_id_str) {
            Ok(uuid) => {
                log::info!("Captured codex session id {uuid}");
                let _ = self.session_id.set(uuid);
            }
            Err(e) => log::warn!("Failed to parse codex session id '{session_id_str}': {e}"),
        }
        Ok(())
    }

    async fn save_conversation(
        &self,
        save_point: SavePoint,
        foreground: &ModelSpawner<AgentDriver>,
    ) -> Result<()> {
        if matches!(save_point, SavePoint::Periodic)
            && !super::has_running_cli_agent(&self.terminal_driver, foreground).await
        {
            log::debug!("Will not save conversation, Codex not in progress");
            return Ok(());
        }

        let (conversation_id, block_id) = match &*self.state.lock() {
            CodexRunnerState::Preexec => {
                log::warn!("save_conversation called before start");
                return Ok(());
            }
            CodexRunnerState::Running {
                conversation_id,
                block_id,
            } => (*conversation_id, block_id.clone()),
        };

        // Still resolve the rollout path: it is the local half of the oracle's save,
        // and finding it is what proves the session id was captured from the hooks.
        let session_id = self.session_id.get().copied();
        let rollout_path = self.resolve_transcript_path().await;
        match (session_id, &rollout_path) {
            (None, _) if matches!(save_point, SavePoint::Final) => {
                log::warn!("Codex session id still unknown at final save")
            }
            (Some(session_id), None) if matches!(save_point, SavePoint::Final) => {
                log::warn!("No codex rollout file found at final save for session {session_id}")
            }
            (Some(_), Some(path)) => log::debug!("Codex rollout file at {}", path.display()),
            _ => log::debug!("Codex rollout file not available yet"),
        }

        let _ = (foreground, conversation_id, block_id, rollout_path);
        log::debug!("Skipping Codex transcript and block snapshot export in Zap");
        Ok(())
    }
}

const CODEX_CONFIG_DIR: &str = ".codex";
const CODEX_HOME_ENV: &str = "CODEX_HOME";
const CODEX_AGENTS_OVERRIDE_FILE_NAME: &str = "AGENTS.override.md";
const CODEX_AUTH_FILE_NAME: &str = "auth.json";
const CODEX_CONFIG_TOML_FILE_NAME: &str = "config.toml";
const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
const CODEX_AUTH_MODE_API_KEY: &str = "apikey";
/// Lowercase string Codex's `TrustLevel` enum serializes to (codex
/// `protocol/src/config_types.rs::TrustLevel`).
const CODEX_TRUST_LEVEL_TRUSTED: &str = "trusted";
/// Top-level config key codex reads to override the built-in `openai` provider's base URL
/// (codex `core/src/config/mod.rs`).
const CODEX_OPENAI_BASE_URL_KEY: &str = "openai_base_url";
const CODEX_CHECK_FOR_UPDATE_ON_STARTUP_KEY: &str = "check_for_update_on_startup";
const CODEX_MODEL_KEY: &str = "model";
const CODEX_MODEL_REASONING_EFFORT_KEY: &str = "model_reasoning_effort";
/// Target model for the `[notice.model_migrations]` table that suppresses Codex's
/// "choose a newer model" upgrade prompt at session launch. We stamp this for any
/// pinned model id (even when it already matches the target) so an unattended
/// run never blocks on the prompt.
const CODEX_MODEL_MIGRATIONS_TARGET: &str = "gpt-5.4";

fn prepare_codex_environment_config(
    working_dir: &Path,
    system_prompt: Option<&str>,
    resolved_env_vars: &HashMap<OsString, OsString>,
    resolved_secrets: &HashMap<String, ManagedSecretValue>,
    resolved_mcp_servers: &HashMap<String, JSONMCPServer>,
    third_party_harness_model_config: Option<&HarnessModelConfig>,
) -> Result<()> {
    let codex_dir = codex_config_dir()?;

    if let Some(prompt) = system_prompt {
        write_codex_agents_override(&codex_dir, prompt)?;
    }

    match resolve_openai_api_key(resolved_env_vars) {
        Some(api_key) => prepare_codex_auth(&codex_dir.join(CODEX_AUTH_FILE_NAME), &api_key)?,
        None => log::info!("No OPENAI_API_KEY available; skipping Codex auth.json seed"),
    }

    // Resolve the base URL directly from the typed OpenAI secret. This avoids
    // leaking base_url into the child process environment and ensures we only
    // apply it when the typed secret is the active API key source.
    let openai_base_url = resolve_openai_base_url_from_secret(resolved_secrets, resolved_env_vars);

    prepare_codex_config_toml(
        &codex_dir.join(CODEX_CONFIG_TOML_FILE_NAME),
        working_dir,
        resolved_mcp_servers,
        third_party_harness_model_config,
        openai_base_url.as_deref(),
    )?;
    Ok(())
}

fn codex_config_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var(CODEX_HOME_ENV)
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    dirs::home_dir()
        .map(|home| home.join(CODEX_CONFIG_DIR))
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))
}

fn write_codex_agents_override(codex_dir: &Path, system_prompt: &str) -> Result<()> {
    fs::create_dir_all(codex_dir).with_context(|| {
        format!(
            "Failed to create Codex config dir at {}",
            codex_dir.display()
        )
    })?;

    let prompt_path = codex_dir.join(CODEX_AGENTS_OVERRIDE_FILE_NAME);
    fs::write(&prompt_path, system_prompt).with_context(|| {
        format!(
            "Failed to write Codex system prompt to {}",
            prompt_path.display()
        )
    })
}

/// Mirrors the subset of Codex's `AuthDotJson` (codex `login/src/auth/storage.rs`) that we
/// need to seed. Unknown fields (`tokens`, `last_refresh`, `agent_identity`, ...) are
/// preserved via `extra` so we don't clobber an existing login.
#[derive(Default, Deserialize, Serialize, Debug)]
struct CodexAuthDotJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_mode: Option<String>,
    #[serde(
        rename = "OPENAI_API_KEY",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    openai_api_key: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

fn prepare_codex_auth(auth_path: &Path, api_key: &str) -> Result<()> {
    let mut auth: CodexAuthDotJson = read_json_file_or_default(auth_path)?;
    auth.openai_api_key = Some(api_key.to_owned());
    if auth.auth_mode.is_none() {
        auth.auth_mode = Some(CODEX_AUTH_MODE_API_KEY.to_owned());
    }
    write_codex_auth_json(auth_path, &auth)
}

/// Write Codex's `auth.json` with restrictive (0o600) permissions, mirroring how
/// codex sets up this file itself.
fn write_codex_auth_json(path: &Path, auth: &CodexAuthDotJson) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(auth).context("Failed to serialize Codex auth.json")?;

    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("Failed to open {} for writing", path.display()))?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
        file.write_all(&bytes)
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    #[cfg(not(unix))]
    fs::write(path, &bytes).with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Returns the OpenAI API key for Codex auth.
///
/// Checks the process env first (not in the resolved map since
/// `build_secret_env_vars` skips env vars already present in the process env),
/// then falls back to the resolved secret env vars map.
fn resolve_openai_api_key(resolved_env_vars: &HashMap<OsString, OsString>) -> Option<String> {
    // Process env wins.
    if let Ok(value) = std::env::var(OPENAI_API_KEY_ENV) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    // Otherwise use the resolved value from the secrets map.
    resolved_env_vars
        .get(OsStr::new(OPENAI_API_KEY_ENV))
        .and_then(|v| v.to_str())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Returns the OpenAI base URL from the typed secret, if applicable.
///
/// The base URL is only used when the typed `OpenaiApiKey` secret is the active
/// source of `OPENAI_API_KEY`. If the process env already provides the API key,
/// the typed-secret base URL is not applied (whoever set the env var controls
/// both the key and the endpoint).
fn resolve_openai_base_url_from_secret(
    secrets: &HashMap<String, ManagedSecretValue>,
    resolved_env_vars: &HashMap<OsString, OsString>,
) -> Option<String> {
    // If an API key was already injected into the process env, the typed secret
    // lost precedence — do not apply its base URL.
    if std::env::var(OPENAI_API_KEY_ENV)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        return None;
    }

    // Only apply when the resolved env vars actually contain OPENAI_API_KEY
    // from the typed secret (i.e. the secret was not skipped).
    resolved_env_vars.get(OsStr::new(OPENAI_API_KEY_ENV))?;

    secrets.values().find_map(|secret| match secret {
        ManagedSecretValue::OpenaiApiKey { base_url, .. } => base_url
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        _ => None,
    })
}

/// Edit `~/.codex/config.toml` via `toml_edit` to seed the harness defaults
/// while preserving anything that might already exist there. We handle:
/// - project trust: for a working dir and all of its git repo subdirectories,
///   set the projects to `trusted`.
/// - base URL: when `openai_base_url` is provided (from the secret's `base_url`
///   field), write it to config.toml. When absent, skip the key entirely so
///   Codex uses the provider's default global endpoint.
/// - update checks: disable Codex's startup update prompt for unattended runs.
/// - model override: when a non-default harness model config is
///   supplied, write the top-level `model` key so Codex pins the chosen model
///   for new sessions.
fn prepare_codex_config_toml(
    config_toml_path: &Path,
    working_dir: &Path,
    resolved_mcp_servers: &HashMap<String, JSONMCPServer>,
    third_party_harness_model_config: Option<&HarnessModelConfig>,
    openai_base_url: Option<&str>,
) -> Result<()> {
    let existing = match fs::read_to_string(config_toml_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(anyhow::Error::from(e).context(format!(
                "Failed to read Codex config.toml at {}",
                config_toml_path.display()
            )));
        }
    };
    let mut doc: toml_edit::DocumentMut = existing.parse().with_context(|| {
        format!(
            "Failed to parse Codex config.toml at {}",
            config_toml_path.display()
        )
    })?;

    // Only write openai_base_url when the secret specifies one.
    if let Some(url) = openai_base_url {
        set_codex_openai_base_url(&mut doc, url);
    }
    set_codex_check_for_update_on_startup(&mut doc, false);
    set_codex_model(&mut doc, third_party_harness_model_config);
    set_codex_model_reasoning_effort(&mut doc, third_party_harness_model_config);

    let canonical = working_dir.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize Codex working dir at {}",
            working_dir.display()
        )
    })?;
    let project_key = canonical.to_string_lossy().into_owned();
    set_codex_project_trust_level(&mut doc, &project_key, CODEX_TRUST_LEVEL_TRUSTED);

    // Codex's trust check is not recursive (see openai/codex#19426) -- when the
    // working dir is a workspace holding several checkouts, we usually have git
    // repo children that we also want to trust.
    for child_repo in find_child_git_repos(&canonical) {
        let key = child_repo.to_string_lossy().into_owned();
        set_codex_project_trust_level(&mut doc, &key, CODEX_TRUST_LEVEL_TRUSTED);
    }

    write_codex_mcp_servers(&mut doc, resolved_mcp_servers);

    if let Some(parent) = config_toml_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create Codex config dir at {}", parent.display())
        })?;
    }
    fs::write(config_toml_path, doc.to_string()).with_context(|| {
        format!(
            "Failed to write Codex config.toml at {}",
            config_toml_path.display()
        )
    })
}

/// Set the top-level `openai_base_url` key, overwriting any existing value.
fn set_codex_openai_base_url(doc: &mut toml_edit::DocumentMut, base_url: &str) {
    doc[CODEX_OPENAI_BASE_URL_KEY] = toml_edit::value(base_url);
}

fn set_codex_check_for_update_on_startup(doc: &mut toml_edit::DocumentMut, enabled: bool) {
    doc[CODEX_CHECK_FOR_UPDATE_ON_STARTUP_KEY] = toml_edit::value(enabled);
}

fn set_codex_model_reasoning_effort(
    doc: &mut toml_edit::DocumentMut,
    third_party_harness_model_config: Option<&HarnessModelConfig>,
) {
    let Some(reasoning_level) = third_party_harness_model_config
        .and_then(|config| config.reasoning_level.as_deref())
        .filter(|level| !level.is_empty())
    else {
        doc.remove(CODEX_MODEL_REASONING_EFFORT_KEY);
        return;
    };
    doc[CODEX_MODEL_REASONING_EFFORT_KEY] = toml_edit::value(reasoning_level);
}

fn set_codex_model(
    doc: &mut toml_edit::DocumentMut,
    third_party_harness_model_config: Option<&HarnessModelConfig>,
) {
    let Some(model_id) = third_party_harness_model_config
        .map(|config| config.model_id.as_str())
        .filter(|id| !id.is_empty() && *id != "default")
    else {
        // No model specified or "default" selected — remove any pre-existing
        // key so Codex uses its own default.
        doc.remove(CODEX_MODEL_KEY);
        return;
    };
    doc[CODEX_MODEL_KEY] = toml_edit::value(model_id);

    // Codex's TUI prompts the user to upgrade older models on session launch even when
    // a `model` key has been pinned. Stamping a migration entry keyed on the chosen
    // model id suppresses that prompt for the unattended run. We do this
    // unconditionally rather than enumerating a list of "old" models on the client:
    // mapping the migration target to itself (e.g. `gpt-5.4 = "gpt-5.4"`) is a no-op
    // for Codex, and keeping the client free of model-version knowledge means we
    // don't have to ship a client update every time a model is aged out.
    set_codex_model_migration(doc, model_id, CODEX_MODEL_MIGRATIONS_TARGET);
}

fn set_codex_model_migration(
    doc: &mut toml_edit::DocumentMut,
    from_model_id: &str,
    to_model_id: &str,
) {
    if !doc.contains_table("notice") {
        let mut notice_tbl = toml_edit::Table::new();
        notice_tbl.set_implicit(true);
        doc.insert("notice", toml_edit::Item::Table(notice_tbl));
    }
    let migrations_tbl = doc["notice"]
        .as_table_mut()
        .expect("notice table inserted above")
        .entry("model_migrations")
        .or_insert_with(toml_edit::table)
        .as_table_mut()
        .expect("model_migrations entry is a table");
    migrations_tbl.set_implicit(false);
    migrations_tbl[from_model_id] = toml_edit::value(to_model_id);
}

/// Return immediate subdirectories of `dir` that contain a `.git`.
fn find_child_git_repos(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.is_dir() && path.join(".git").exists()).then_some(path)
        })
        .collect()
}

/// Insert/update `[projects."<project_key>"] trust_level = <trust_level>`.
///
/// Codex itself always writes `projects` as an explicit table, so we don't
/// handle the inline-table form here.
fn set_codex_project_trust_level(
    doc: &mut toml_edit::DocumentMut,
    project_key: &str,
    trust_level: &str,
) {
    if !doc.contains_table("projects") {
        let mut projects_tbl = toml_edit::Table::new();
        projects_tbl.set_implicit(true);
        doc.insert("projects", toml_edit::Item::Table(projects_tbl));
    }
    let proj_tbl = doc["projects"]
        .as_table_mut()
        .expect("projects table inserted above")
        .entry(project_key)
        .or_insert_with(toml_edit::table)
        .as_table_mut()
        .expect("project entry is a table");
    proj_tbl.set_implicit(false);
    proj_tbl["trust_level"] = toml_edit::value(trust_level);
}

/// Write resolved MCP servers into `[mcp_servers.<name>]` sections in the Codex config.
fn write_codex_mcp_servers(
    doc: &mut toml_edit::DocumentMut,
    servers: &HashMap<String, JSONMCPServer>,
) {
    if servers.is_empty() {
        return;
    }
    if !doc.contains_table("mcp_servers") {
        let mut tbl = toml_edit::Table::new();
        tbl.set_implicit(true);
        doc.insert("mcp_servers", toml_edit::Item::Table(tbl));
    }
    let mcp_tbl = doc["mcp_servers"]
        .as_table_mut()
        .expect("mcp_servers table inserted above");

    for (name, server) in servers {
        let entry = mcp_tbl
            .entry(name)
            .or_insert_with(toml_edit::table)
            .as_table_mut()
            .expect("mcp_servers entry is a table");
        entry.set_implicit(false);

        match &server.transport_type {
            JSONTransportType::CLIServer {
                command,
                args,
                env,
                working_directory,
            } => {
                entry["command"] = toml_edit::value(command.as_str());
                if !args.is_empty() {
                    let mut arr = toml_edit::Array::new();
                    for arg in args {
                        arr.push(arg.as_str());
                    }
                    entry["args"] = toml_edit::value(arr);
                }
                if !env.is_empty() {
                    let mut env_tbl = toml_edit::InlineTable::new();
                    for (k, v) in env {
                        env_tbl.insert(k, v.as_str().into());
                    }
                    entry["env"] = toml_edit::value(env_tbl);
                }
                if let Some(cwd) = working_directory {
                    entry["cwd"] = toml_edit::value(cwd.as_str());
                }
            }
            JSONTransportType::SSEServer { url, headers } => {
                entry["url"] = toml_edit::value(url.as_str());
                if !headers.is_empty() {
                    let mut hdrs_tbl = toml_edit::InlineTable::new();
                    for (k, v) in headers {
                        hdrs_tbl.insert(k, v.as_str().into());
                    }
                    entry["http_headers"] = toml_edit::value(hdrs_tbl);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
