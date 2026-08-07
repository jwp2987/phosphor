use super::*;
use clap::Parser;

use crate::agent::{AgentCommand, Harness};
// Zap Wave 7-2: the `environment` CLI was physically removed along with the cloud ambient agent subsystem.

// Port audit against the pinned oracle (02b53fcd8, ORACLE.md), issue #210: the pin's
// `lib_tests.rs` has 126 tests covering CLI subcommands that dispatch to Warp's cloud
// backend (server-hosted "hosted CLI task commands", per commit a9ab253cd "Localize AI
// agent execution paths"). Those subcommands were deliberately, physically removed from
// `CliCommand`/`AgentCommand` across several documented cloud-removal passes, so their
// tests have no local/BYOP target to port to. Verified per-command against the pin's
// `crates/warp_cli/src/{lib,agent,environment,memory_store,schedule,secret,integration,
// artifact,harness_support,task}.rs` before classifying as CLOUD (never solely by test
// name):
//
// - `CliCommand::Environment` (cloud dev-environment provisioning, AWS/GCP OIDC) -- Wave
//   7-2 (commit e94e7599c). Tests: environment_image_list_parses,
//   environment_create_accepts_description, environment_create_description_max_length,
//   environment_update_accepts_description, environment_update_accepts_remove_description.
// - `CliCommand::MemoryStore` / `Memory` (team-shared memory synced to the server, UID +
//   version identified) -- never ported (a9ab253cd). Tests: memory_store_list_parses,
//   memory_stores_alias_parses, memory_store_get_parses, memory_store_get_store_alias_parses,
//   memory_store_update_parses, memory_store_update_store_alias_parses, memory_list_parses,
//   memory_create_parses, memory_update_parses, memory_delete_parses, memory_versions_parses,
//   legacy_memory_store_memory_commands_are_rejected.
// - `CliCommand::Login` / `Logout` (cloud auth) -- Wave3-1 (commit 60d685b58, "物理删
//   app/src/auth/ ... cloud API key 管理 UI"). Tests: login_parses, logout_parses.
// - `CliCommand::Schedule` (cron-scheduled cloud agents) -- Wave 7-1 (commit b190cb499,
//   "整删 cron 调度子系统"). Tests: schedule_create_accepts_file,
//   schedule_create_accepts_mcp_json, schedule_create_accepts_team_scope,
//   schedule_create_accepts_personal_scope, schedule_create_rejects_multiple_scopes,
//   schedule_resume_alias_parses_as_unpause, schedule_update_accepts_file,
//   schedule_update_accepts_mcp_json_and_remove_mcp.
// - `CliCommand::Secret` (server-side secret store backing `agent run-cloud
//   --claude-auth-secret`/`--codex-auth-secret`, supports `--team` scope) -- never ported
//   (a9ab253cd removed `agent_sdk/secret.rs`, 776 lines). Tests:
//   secret_create_codex_api_key_parses_minimal,
//   secret_create_codex_api_key_accepts_base_url_and_value_file,
//   secret_create_codex_api_key_requires_name.
// - `CliCommand::Integration` (Slack-triggered cloud agent runs) -- never ported
//   (ed50466ba). Tests: integration_create_accepts_file, integration_create_accepts_model,
//   integration_create_accepts_mcp_json, integration_update_accepts_file,
//   integration_update_accepts_model, integration_update_accepts_mcp_json_and_remove_mcp.
// - `CliCommand::Artifact` (cloud snapshot/artifact storage keyed by cloud run-id /
//   conversation-id) -- "snapshot upload support" removal (a9ab253cd, ed50466ba). Tests:
//   artifact_upload_accepts_run_id, artifact_upload_accepts_run_id_and_description,
//   artifact_upload_accepts_conversation_id_and_description,
//   artifact_upload_accepts_missing_association_target_for_env_fallback,
//   artifact_upload_rejects_both_association_targets, artifact_download_parses_artifact_id_and_out,
//   artifact_get_parses_artifact_uid, artifact_help_hides_upload_but_keeps_download_visible.
// - `CliCommand::HarnessSupport` (`--run-id` status callbacks a hosted harness reports back
//   to Oz's cloud backend for a cloud-dispatched run) -- never ported (a9ab253cd removed
//   `agent_sdk/harness_support.rs`). Tests: finish_task_accepts_status_success,
//   finish_task_accepts_status_failure, finish_task_rejects_invalid_status,
//   finish_task_rejects_missing_status, report_shutdown_clean_parses,
//   report_shutdown_abnormal_parses, report_external_reference_required_args_parse,
//   report_external_reference_optional_title_parses,
//   report_external_reference_missing_url_fails,
//   report_external_reference_missing_reference_type_fails.
// - `AgentCommand::RunCloud` (dispatch to Warp's hosted MAA infra) plus the
//   `--task-id`/`--conversation`/snapshot-upload flags on `AgentCommand::Run` that only make
//   sense on the server-side "hosted" prompt path -- never ported. Tests:
//   agent_run_cloud_accepts_file_short_flag, agent_run_cloud_accepts_model,
//   agent_run_cloud_accepts_agent_flag, agent_run_cloud_accepts_mcp,
//   agent_run_cloud_accepts_run_ambient_alias, agent_run_cloud_accepts_snapshot_flags,
//   agent_run_cloud_accepts_computer_use_flag, agent_run_cloud_accepts_no_computer_use_flag,
//   agent_run_cloud_rejects_both_computer_use_flags,
//   agent_run_cloud_defaults_to_no_computer_use_override, agent_run_cloud_accepts_harness_flag,
//   agent_run_cloud_defaults_harness_to_oz, agent_run_cloud_accepts_claude_auth_secret_with_harness,
//   agent_run_cloud_claude_auth_secret_without_harness_parses, run_cloud_help_lists_harness_and_auth_secret_flags,
//   run_cloud_accepts_claude_auth_secret, run_cloud_accepts_codex_auth_secret,
//   run_cloud_rejects_claude_auth_secret_without_claude_harness,
//   run_cloud_rejects_codex_auth_secret_without_codex_harness, agent_run_accepts_task_id_only,
//   agent_run_accepts_skill_and_task_id, agent_run_accepts_task_id_with_conversation_for_worker_followups,
//   agent_run_accepts_snapshot_flags, agent_run_accepts_skip_initial_turn_with_task_id_and_idle_on_complete,
//   agent_run_rejects_skip_initial_turn_without_idle_on_complete,
//   agent_run_rejects_skip_initial_turn_without_task_id, agent_run_rejects_file_and_task_id,
//   agent_run_rejects_prompt_and_task_id, agent_run_rejects_saved_prompt_and_task_id,
//   agent_run_rejects_without_prompt_or_task_id (fork's equivalent is
//   `agent_run_rejects_without_prompt_or_skill`, already ported above).
// - `AgentCommand::{Get,Create,Update,Delete}` (CRUD on named agents stored server-side --
//   UID-identified, secrets/skills/cloud-environment attached server-side) -- never ported.
//   Tests: agent_create_accepts_prompt, agent_update_accepts_prompt_replacement,
//   agent_update_accepts_remove_prompt, agent_update_leaves_prompt_unset_when_neither_flag_passed,
//   agent_update_rejects_conflicting_remove_flags, agent_update_rejects_prompt_and_remove_prompt,
//   agent_update_rejects_remove_all_secret_deltas.
// - `CliCommand::Run` / `task.rs` (hosted CLI task commands + cross-run mailbox) -- "hosted
//   CLI task commands" removal (a9ab253cd). Tests: run_message_send_parses,
//   run_message_list_parses_filters, run_message_list_rejects_non_positive_limit,
//   run_message_watch_parses, run_message_read_parses, run_message_mark_delivered_parses,
//   run_message_delivered_alias_parses, raw_command_keeps_message_visible_before_runtime_help_customization
//   (fork's coverage of the *removal* is `run_command_is_removed`, already present above).
// - `hidden_server_overrides_parse_from_env` -- pins env-var overrides
//   (`WARP_SERVER_ROOT_URL`/`WARP_WS_SERVER_URL`/`WARP_SESSION_SHARING_SERVER_URL`) for
//   Warp's cloud GraphQL/session-sharing backends; those accessors and env constants don't
//   exist here since the fork has no cloud backend to point at.
//
// `harness_parse_local_child_harness_accepts_codex` / `harness_parse_orchestration_harness_accepts_codex`
// are FEATURE GAP, not cloud debt: Warp's `Harness::Codex` variant and
// `config_name`/`from_config_name` were never ported to `warp_cli::agent` (issue #183).
// Reported there, not invented here.
//
// `api_key_before_subcommand_parses` / `debug_before_subcommand_parses` /
// `multiple_global_flags_before_subcommand_parse` are ported below, adapted to target
// `whoami` instead of the removed `login` -- see the comment at their definition.

/// Ported from warp/master `identifies_worker_subcommands`.
#[test]
fn identifies_worker_subcommands() {
    assert!(is_worker_invocation("minidump-server"));
    #[cfg(unix)]
    assert!(is_worker_invocation(&terminal_server_subcommand()));
    #[cfg(feature = "plugin_host")]
    assert!(is_worker_invocation("--plugin-host"));
    assert!(!is_worker_invocation("--prompt"));
}

#[test]
fn agent_run_accepts_model() {
    let args = Args::try_parse_from([
        "warp", "agent", "run", "--prompt", "hello", "--model", "gpt-4o",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(run_args.model.model.as_deref(), Some("gpt-4o"));
}

#[test]
fn agent_run_accepts_hidden_bedrock_inference_role_flag() {
    let args = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--bedrock-inference-role",
        "arn:aws:iam::123456789012:role/test",
        // Updated to also pass --bedrock-role-region: restoring the pin's `requires`
        // linkage between these two flags (see the comment below) makes the region
        // mandatory alongside the role, matching the pin's own version of this test.
        "--bedrock-role-region",
        "us-east-1",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(
        run_args.bedrock_inference_role.as_deref(),
        Some("arn:aws:iam::123456789012:role/test")
    );
    assert_eq!(run_args.bedrock_role_region.as_deref(), Some("us-east-1"));
}

// Ported from the pin. The fork's `bedrock_inference_role` flag was missing the
// `requires = "bedrock_role_region"` clap constraint (and the `bedrock_role_region` field
// entirely), so a user could pass `--bedrock-inference-role` alone and have it silently
// accepted at parse time. Restored both fields' `requires` linkage in
// `crates/warp_cli/src/agent.rs` to match the pin. See the filed issue for the deeper
// gap this uncovered: the OIDC credential-minting path itself still requires an "ambient
// task ID" that only a cloud-dispatched run provides (`app/src/ai/aws_credentials.rs`),
// so today this flag combination cannot succeed locally even when well-formed -- only the
// CLI-level validation is restored here.
#[test]
fn agent_run_rejects_bedrock_inference_role_without_region() {
    let err = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--bedrock-inference-role",
        "arn:aws:iam::123456789012:role/test",
    ])
    .expect_err("--bedrock-inference-role must require --bedrock-role-region");
    assert!(
        err.to_string().contains("--bedrock-role-region"),
        "expected error to reference --bedrock-role-region, got: {err}"
    );
}

#[test]
fn agent_run_rejects_bedrock_role_region_without_role() {
    let err = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--bedrock-role-region",
        "us-east-1",
    ])
    .expect_err("--bedrock-role-region must require --bedrock-inference-role");
    assert!(
        err.to_string().contains("--bedrock-inference-role"),
        "expected error to reference --bedrock-inference-role, got: {err}"
    );
}

#[test]
fn model_list_parses() {
    let args = Args::try_parse_from(["warp", "model", "list"]).unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp model list` command");
    };
    let CliCommand::Model(model_cmd) = boxed_cmd.as_ref() else {
        panic!("Expected `warp model` command");
    };

    assert!(matches!(model_cmd, crate::model::ModelCommand::List));
}

#[test]
fn agent_run_accepts_file() {
    let args = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--file",
        "config.yaml",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(
        run_args.config_file.file.as_ref().and_then(|p| p.to_str()),
        Some("config.yaml")
    );
}

#[test]
fn agent_run_accepts_idle_on_complete_flag() {
    let args = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--idle-on-complete",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(
        run_args.idle_on_complete,
        Some(humantime::Duration::from(std::time::Duration::from_secs(
            45 * 60
        )))
    );
}

#[test]
fn agent_run_accepts_idle_on_complete_duration() {
    let args = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--idle-on-complete",
        "10m",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(
        run_args.idle_on_complete,
        Some(humantime::Duration::from(std::time::Duration::from_secs(
            10 * 60
        )))
    );
}

#[test]
fn agent_run_rejects_without_prompt_or_skill() {
    let result = Args::try_parse_from(["warp", "agent", "run", "--model", "gpt-4o"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(err_str.contains("prompt_group") || err_str.contains("required"));
}

#[test]
fn agent_run_accepts_prompt_only() {
    let args = Args::try_parse_from(["warp", "agent", "run", "--prompt", "hello"]).unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(run_args.prompt_arg.prompt.as_deref(), Some("hello"));
    assert!(run_args.prompt_arg.saved_prompt.is_none());
    assert!(run_args.skill.is_none());
}

#[test]
fn agent_run_accepts_saved_prompt_only() {
    let args = Args::try_parse_from(["warp", "agent", "run", "--saved-prompt", "sp-123"]).unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert!(run_args.prompt_arg.prompt.is_none());
    assert_eq!(run_args.prompt_arg.saved_prompt.as_deref(), Some("sp-123"));
    assert!(run_args.skill.is_none());
}

#[test]
fn agent_run_accepts_skill_only() {
    let args = Args::try_parse_from(["warp", "agent", "run", "--skill", "my-skill"]).unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert!(run_args.prompt_arg.prompt.is_none());
    assert!(run_args.skill.is_some());
}

#[test]
fn agent_run_accepts_prompt_and_skill() {
    let args = Args::try_parse_from([
        "warp", "agent", "run", "--prompt", "do stuff", "--skill", "my-skill",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(run_args.prompt_arg.prompt.as_deref(), Some("do stuff"));
    assert!(run_args.skill.is_some());
}

#[test]
fn agent_run_accepts_saved_prompt_and_skill() {
    let args = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--saved-prompt",
        "sp-1",
        "--skill",
        "my-skill",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert_eq!(run_args.prompt_arg.saved_prompt.as_deref(), Some("sp-1"));
    assert!(run_args.skill.is_some());
}

#[test]
fn agent_run_rejects_prompt_and_saved_prompt() {
    let result = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--saved-prompt",
        "sp-1",
    ]);
    assert!(result.is_err());
}

#[test]
fn run_command_is_removed() {
    let result = Args::try_parse_from(["warp", "run", "message"]);
    assert!(result.is_err());
}

// Zap Wave 7-2: environment_image_list_parses / environment_create_accepts_description /
// environment_create_description_max_length / environment_update_accepts_description /
// environment_update_accepts_remove_description were physically removed along
// with the cloud ambient agent subsystem.

#[test]
fn agent_run_accepts_computer_use_flag() {
    let args = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--computer-use",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert!(run_args.computer_use.computer_use);
    assert!(!run_args.computer_use.no_computer_use);
    assert_eq!(run_args.computer_use.computer_use_override(), Some(true));
}

#[test]
fn agent_run_accepts_no_computer_use_flag() {
    let args = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--no-computer-use",
    ])
    .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert!(!run_args.computer_use.computer_use);
    assert!(run_args.computer_use.no_computer_use);
    assert_eq!(run_args.computer_use.computer_use_override(), Some(false));
}

#[test]
fn agent_run_rejects_both_computer_use_flags() {
    let result = Args::try_parse_from([
        "warp",
        "agent",
        "run",
        "--prompt",
        "hello",
        "--computer-use",
        "--no-computer-use",
    ]);

    assert!(result.is_err());
}

#[test]
fn agent_run_defaults_to_no_computer_use_override() {
    let args = Args::try_parse_from(["warp", "agent", "run", "--prompt", "hello"]).unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp agent run` command");
    };
    let CliCommand::Agent(AgentCommand::Run(run_args)) = boxed_cmd.as_ref() else {
        panic!("Expected `warp agent run` command");
    };

    assert!(!run_args.computer_use.computer_use);
    assert!(!run_args.computer_use.no_computer_use);
    assert_eq!(run_args.computer_use.computer_use_override(), None);
}
#[test]
fn harness_parse_orchestration_harness_accepts_aliases() {
    assert_eq!(
        Harness::parse_orchestration_harness("claude-code"),
        Some(Harness::Claude)
    );
    assert_eq!(
        Harness::parse_orchestration_harness("open_code"),
        Some(Harness::OpenCode)
    );
}

#[test]
fn harness_parse_local_child_harness_rejects_oz() {
    assert_eq!(Harness::parse_local_child_harness("oz"), None);
    assert_eq!(
        Harness::parse_local_child_harness("opencode"),
        Some(Harness::OpenCode)
    );
}

// FEATURE GAP (issue #183), not test debt: the pin also has
// `harness_parse_orchestration_harness_accepts_codex` and
// `harness_parse_local_child_harness_accepts_codex`, asserting
// `Harness::parse_orchestration_harness("codex") == Some(Harness::Codex)` and the local
// equivalent. `Harness::Codex` doesn't exist in `crate::agent::Harness` here, so these
// can't be ported without inventing the feature. Reported at #183, not implemented here.

// Ported from the pin's `api_key_before_subcommand_parses` / `debug_before_subcommand_parses`
// / `multiple_global_flags_before_subcommand_parse`. These pin the CLI's
// `args_conflicts_with_subcommands` global-flag-ordering behavior and are independent of
// which subcommand follows -- the pin happens to use `login` as its example subcommand.
// `login`/`logout` are cloud auth and were physically removed (see the audit comment at
// the top of this file), so adapted to target `whoami` instead: it still exists, takes no
// positional/required args, and exercises the identical parsing path.
#[test]
fn api_key_before_subcommand_parses() {
    // Regression test: `warp --api-key KEY <subcommand>` should work.
    // Previously the top-level [URLS] positional would swallow the subcommand
    // when --api-key preceded it.
    let args = Args::try_parse_from(["warp", "--api-key", "test-key", "whoami"]).unwrap();

    assert_eq!(args.api_key(), Some(&"test-key".to_string()));
    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp whoami` command");
    };
    assert!(matches!(boxed_cmd.as_ref(), CliCommand::Whoami));
}

#[test]
fn debug_before_subcommand_parses() {
    // Regression test: `warp --debug <subcommand>` should work.
    // Global flags like --debug must not prevent subcommand detection.
    let args = Args::try_parse_from(["warp", "--debug", "whoami"]).unwrap();

    assert!(args.debug());
    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp whoami` command");
    };
    assert!(matches!(boxed_cmd.as_ref(), CliCommand::Whoami));
}

#[test]
fn multiple_global_flags_before_subcommand_parse() {
    // Both --api-key and --debug before the subcommand should work.
    let args =
        Args::try_parse_from(["warp", "--api-key", "test-key", "--debug", "whoami"]).unwrap();

    assert_eq!(args.api_key(), Some(&"test-key".to_string()));
    assert!(args.debug());
    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp whoami` command");
    };
    assert!(matches!(boxed_cmd.as_ref(), CliCommand::Whoami));
}
