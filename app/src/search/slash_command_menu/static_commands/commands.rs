use std::{collections::HashMap, sync::LazyLock};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp_core::features::FeatureFlag;

use crate::search::slash_command_menu::{StaticCommand, static_commands::Argument};
use crate::t_static;

use super::Availability;

pub static AGENT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/agent",
    description: t_static!("slash-cmd-agent-desc"),
    icon_path: "bundled/svg/oz.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_execute_on_selection()),
});

pub static ADD_MCP: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/add-mcp",
    description: t_static!("slash-cmd-add-mcp-desc"),
    icon_path: "bundled/svg/dataflow.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static PR_COMMENTS: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/pr-comments",
    description: t_static!("slash-cmd-pr-comments-desc"),
    icon_path: "bundled/svg/github.svg",
    availability: Availability::REPOSITORY.union(Availability::AI_ENABLED),
    auto_enter_ai_mode: true,
    argument: None,
});

pub static CREATE_DOCKER_SANDBOX: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/docker-sandbox",
    description: t_static!("slash-cmd-docker-sandbox-desc"),
    icon_path: "bundled/svg/docker.svg",
    availability: Availability::LOCAL.union(Availability::AI_ENABLED),
    auto_enter_ai_mode: false,
    argument: None,
});

pub static CREATE_NEW_PROJECT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/create-new-project",
    description: t_static!("slash-cmd-create-new-project-desc"),
    icon_path: "bundled/svg/plus.svg",
    availability: Availability::LOCAL | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(
        Argument::required().with_hint_text(t_static!("slash-cmd-create-new-project-hint")),
    ),
});

pub static EDIT_SKILL: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-skill",
    description: t_static!("slash-cmd-open-skill-desc"),
    icon_path: "bundled/svg/file-code-02.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static INVOKE_SKILL: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/skills",
    description: t_static!("slash-cmd-skills-desc"),
    icon_path: "bundled/svg/stars-01.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static ADD_PROMPT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/add-prompt",
    description: t_static!("slash-cmd-add-prompt-desc"),
    icon_path: if FeatureFlag::AgentView.is_enabled() {
        "bundled/svg/prompt.svg"
    } else {
        "bundled/svg/agentmode.svg"
    },
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static ADD_RULE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/add-rule",
    description: t_static!("slash-cmd-add-rule-desc"),
    icon_path: "bundled/svg/book-open.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static EDIT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-file",
    description: t_static!("slash-cmd-open-file-desc"),
    icon_path: "bundled/svg/file-code-02.svg",
    availability: Availability::LOCAL,
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_hint_text(t_static!("slash-cmd-open-file-hint"))),
});

pub static RENAME_TAB: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/rename-tab",
    description: t_static!("slash-cmd-rename-tab-desc"),
    icon_path: "bundled/svg/pencil-line.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: Some(Argument::required().with_hint_text(t_static!("slash-cmd-rename-tab-hint"))),
});

// TUI-only: configures which items appear in the bottom statusline and their order. Not
// executable in the GUI (see `execute_slash_command`'s explicit guard for this command).
pub static STATUSLINE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/statusline",
    description: t_static!("slash-cmd-statusline-desc"),
    icon_path: "bundled/svg/sliders-04.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: sets the TUI color theme (`auto`/`light`/`dark`, backed by `TuiTheme`). Not
/// executable in the GUI (see `execute_slash_command`'s explicit guard); the GUI has its own
/// theme chooser (`workspace:show_theme_chooser`).
pub static THEME: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/theme",
    description: t_static!("slash-cmd-theme-desc"),
    icon_path: "bundled/svg/settings.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: Some(Argument::required().with_hint_text("<auto|light|dark>")),
});

pub static FORK: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/fork",
    description: t_static!("slash-cmd-fork-desc"),
    icon_path: "bundled/svg/arrow-split.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text(t_static!("slash-cmd-fork-hint"))),
});

/// Spawns one or more local child agents for the current conversation.
///
/// User-invoked only: this executes directly (see the `/orchestrate` arm of
/// `execute_slash_command` on the GUI, and the `SlashCommandKind::Other`
/// name-guarded arm in `TuiTerminalSessionView::execute_tui_slash_command`
/// on the TUI) rather than being submitted as a prompt for the model to act
/// on. It deliberately does NOT use `SlashCommandKind::Orchestrate` -- that
/// kind is the pin's agent-invoked semantics
/// (`slash_command_is_submitted_as_prompt` routes it to the model, which
/// would need `AIAgentActionType::RunAgents` to act on it). That path is
/// deferred; `kind()` has no `"/orchestrate"` arm so this command falls
/// through to `SlashCommandKind::Other`, same as any other fork-native
/// command with no upstream counterpart.
///
/// TUI and GUI both spawn through the same `local_harness_launch` machinery
/// -- the GUI via `pane_group::pane::terminal_pane::spawn_local_child_agents`
/// (real `PaneGroup` hidden panes), the TUI via
/// `crate::pane_group::TuiPaneGroup::spawn_local_child_agents`
/// (`crates/warp_tui/src/pane_group.rs`, hidden `TuiSessions` sessions
/// reached through the `pub` seam in `pane_group::pane::mod`) -- so the two
/// surfaces stay at parity (AGENTS §5.9).
///
/// No wasm: local child processes have no wasm equivalent (see
/// `pane_group::pane::local_harness_launch`, `#[cfg(not(target_family = "wasm"))]`).
pub static ORCHESTRATE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/orchestrate",
    description: t_static!("slash-cmd-orchestrate-desc"),
    icon_path: "bundled/svg/create-team.svg",
    availability: Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::LOCAL
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: Some(Argument::required().with_hint_text(t_static!("slash-cmd-orchestrate-hint"))),
});

pub static OPEN_CODE_REVIEW: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-code-review",
    description: t_static!("slash-cmd-open-code-review-desc"),
    icon_path: "bundled/svg/diff.svg",
    availability: Availability::REPOSITORY,
    auto_enter_ai_mode: false,
    argument: None,
});

pub const INIT_NAME: &str = "/init";

pub static INIT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: INIT_NAME,
    description: t_static!("slash-cmd-init-desc"),
    icon_path: "bundled/svg/warp-2.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional()),
});

pub static OPEN_PROJECT_RULES: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-project-rules",
    description: t_static!("slash-cmd-open-project-rules-desc"),
    icon_path: "bundled/svg/file-code-02.svg",
    availability: Availability::REPOSITORY.union(Availability::AI_ENABLED),
    auto_enter_ai_mode: false,
    argument: None,
});

pub static OPEN_MCP_SERVERS: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-mcp-servers",
    description: t_static!("slash-cmd-open-mcp-servers-desc"),
    icon_path: "bundled/svg/dataflow.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static OPEN_SETTINGS_FILE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-settings-file",
    description: t_static!("slash-cmd-open-settings-file-desc"),
    icon_path: "bundled/svg/file-code-02.svg",
    availability: Availability::LOCAL,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static CHANGELOG: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/changelog",
    description: t_static!("slash-cmd-changelog-desc"),
    icon_path: "bundled/svg/book-open.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static OPEN_REPO: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-repo",
    description: t_static!("slash-cmd-open-repo-desc"),
    icon_path: "bundled/svg/folder.svg",
    availability: Availability::LOCAL.union(Availability::AI_ENABLED),
    auto_enter_ai_mode: false,
    argument: None,
});

pub static OPEN_RULES: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/open-rules",
    description: t_static!("slash-cmd-open-rules-desc"),
    icon_path: "bundled/svg/book-open.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static NEW: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/new",
    description: t_static!("slash-cmd-new-desc"),
    icon_path: "bundled/svg/new-conversation.svg",
    availability: Availability::NO_LRC_CONTROL | Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_execute_on_selection()),
});

pub static MODEL: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/model",
    description: t_static!("slash-cmd-model-desc"),
    icon_path: "bundled/svg/oz.svg",
    availability: Availability::AGENT_VIEW | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: None,
});

/// Fork-native: opens the BYOP provider API-key manager (list configured providers, see which
/// have a key connected, add/update/clear one) without leaving the terminal or TUI. This fork's
/// entire identity is BYOP, so unlike upstream Warp's cloud-gated `/add-api-key` this is always
/// available whenever AI is enabled, with no fixed provider list and no billing/credit concepts.
pub static API_KEYS: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/api-keys",
    description: t_static!("slash-cmd-api-keys-desc"),
    icon_path: "bundled/svg/key.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static PROFILE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/profile",
    description: t_static!("slash-cmd-profile-desc"),
    icon_path: "bundled/svg/psychology.svg",
    availability: Availability::AGENT_VIEW | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: None,
});

pub const PLAN_NAME: &str = "/plan";

pub static PLAN: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: PLAN_NAME,
    description: t_static!("slash-cmd-plan-desc"),
    icon_path: "bundled/svg/file-06.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text(t_static!("slash-cmd-plan-hint"))),
});

/// If `query` starts with the given command `name` followed by a space,
/// returns the remainder of the query. Otherwise returns `None`.
pub fn strip_command_prefix(query: &str, name: &str) -> Option<String> {
    query
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(' '))
        .map(|rest| rest.to_string())
}

pub static COMPACT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/compact",
    description: t_static!("slash-cmd-compact-desc"),
    icon_path: "bundled/svg/collapse_content.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text(t_static!("slash-cmd-compact-hint"))),
});

pub static COMPACT_AND: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/compact-and",
    description: t_static!("slash-cmd-compact-and-desc"),
    icon_path: "bundled/svg/collapse_content.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text(t_static!("slash-cmd-compact-and-hint"))),
});

pub static QUEUE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/queue",
    description: t_static!("slash-cmd-queue-desc"),
    icon_path: "bundled/svg/clock-plus.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::required().with_hint_text(t_static!("slash-cmd-queue-hint"))),
});

pub static FORK_AND_COMPACT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/fork-and-compact",
    description: t_static!("slash-cmd-fork-and-compact-desc"),
    icon_path: "bundled/svg/fork_and_compact.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::NO_LRC_CONTROL
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(
        Argument::optional().with_hint_text(t_static!("slash-cmd-fork-and-compact-hint")),
    ),
});

pub static FORK_FROM: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/fork-from",
    description: t_static!("slash-cmd-fork-from-desc"),
    icon_path: "bundled/svg/arrow-split.svg",
    availability: Availability::AGENT_VIEW
        .union(Availability::NO_LRC_CONTROL)
        .union(Availability::AI_ENABLED),
    auto_enter_ai_mode: true,
    argument: None,
});

pub static CONVERSATIONS: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/conversations",
    description: t_static!("slash-cmd-conversations-desc"),
    icon_path: "bundled/svg/conversation.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static PROMPTS: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/prompts",
    description: t_static!("slash-cmd-prompts-desc"),
    icon_path: "bundled/svg/prompt.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static REWIND: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/rewind",
    description: t_static!("slash-cmd-rewind-desc"),
    icon_path: "bundled/svg/clock-rewind.svg",
    availability: Availability::AGENT_VIEW.union(Availability::AI_ENABLED),
    auto_enter_ai_mode: true,
    argument: None,
});

pub static EXPORT_TO_CLIPBOARD: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/export-to-clipboard",
    description: t_static!("slash-cmd-export-to-clipboard-desc"),
    icon_path: "bundled/svg/copy.svg",
    availability: Availability::AGENT_VIEW.union(Availability::AI_ENABLED),
    auto_enter_ai_mode: true,
    argument: None,
});

/// Copies an identifier for the current conversation so the user can attach it to a
/// Phosphor issue. See `ServerConversationToken::debugging_payload` for what lands on the
/// clipboard -- a deeplink on dogfood channels, a plain id blob everywhere else.
pub static COPY_DEBUGGING_ID: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/copy-debugging-id",
    description: t_static!("slash-cmd-copy-debugging-id-desc"),
    icon_path: "bundled/svg/copy.svg",
    availability: Availability::ACTIVE_CONVERSATION,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static EXPORT_TO_FILE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/export-to-file",
    description: t_static!("slash-cmd-export-to-file-desc"),
    icon_path: "bundled/svg/download-01.svg",
    availability: Availability::AGENT_VIEW | Availability::AI_ENABLED,
    auto_enter_ai_mode: true,
    argument: Some(Argument::optional().with_hint_text(t_static!("slash-cmd-export-to-file-hint"))),
});

/// Toggles the shared `text_editing.vim_mode_enabled` setting. Primarily useful on the
/// ratatui TUI surface (see `crates/warp_tui`'s `supports_tui` gate below), where there is
/// no Settings UI; on the GUI the same effect is available from Settings > Text Editing.
pub static VIM_MODE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/vim-mode",
    description: t_static!("slash-cmd-vim-mode-desc"),
    icon_path: "bundled/svg/keyboard.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: toggles auto-approve for the selected conversation. Not executable in the GUI
/// (see `execute_slash_command`'s explicit guard), which drives the same state from the
/// agent panel's own control.
pub static AUTO_APPROVE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/auto-approve",
    description: t_static!("slash-cmd-auto-approve-desc"),
    icon_path: "bundled/svg/check-circle-broken.svg",
    availability: Availability::AGENT_VIEW
        | Availability::ACTIVE_CONVERSATION
        | Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: toggles `ai.ai_autodetection_enabled_internal`, which decides whether a typed
/// line is auto-classified as a prompt or a shell command. Not executable in the GUI (see
/// `execute_slash_command`'s explicit guard), where the same setting lives in Settings > AI.
pub static NATURAL_LANGUAGE_DETECTION: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/natural-language-detection",
    description: t_static!("slash-cmd-natural-language-detection-desc"),
    icon_path: "bundled/svg/sparkle.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: opens the MCP server manager. Not executable in the GUI (see
/// `execute_slash_command`'s explicit guard), which has its own `/add-mcp` and
/// `/open-mcp-servers` entry points instead.
pub static MCP: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/mcp",
    description: t_static!("slash-cmd-mcp-desc"),
    icon_path: "bundled/svg/dataflow.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: opens the session-status overlay. Not executable in the GUI (see
/// `execute_slash_command`'s explicit guard), which has its own status surfaces. Unlike the
/// oracle's `/status`, this drops the `org`/`email` account fields -- this fork is BYOP with
/// no cloud account or sign-in, so there is nothing truthful to show there.
pub static STATUS: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/status",
    description: t_static!("slash-cmd-status-desc"),
    icon_path: "bundled/svg/info.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: quits the TUI process. Not executable in the GUI (see `execute_slash_command`'s
/// explicit guard), which has its own window-close affordances.
pub static EXIT: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/exit",
    description: t_static!("slash-cmd-exit-desc"),
    icon_path: "bundled/svg/log-out-01.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: bundles the app's logs into a zip archive and reveals it in the file manager.
/// Not executable in the GUI (see `execute_slash_command`'s explicit guard).
pub static VIEW_LOGS: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/view-logs",
    description: t_static!("slash-cmd-view-logs-desc"),
    icon_path: "bundled/svg/download-01.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: None,
});

/// TUI-only: alias for `/agent`/`/new` (see `SlashCommandKind::Clear`'s doc comment). Not
/// executable in the GUI (see `execute_slash_command`'s explicit guard); the GUI has no
/// equivalent alias command upstream either.
pub static CLEAR: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/clear",
    description: t_static!("slash-cmd-clear-desc"),
    icon_path: "bundled/svg/refresh-ccw-01.svg",
    availability: Availability::NO_LRC_CONTROL | Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: Some(Argument::optional().with_execute_on_selection()),
});

static SET_TAB_COLOR_HINT: LazyLock<String> = LazyLock::new(|| {
    let mut hint = String::from("<");
    for color in crate::ui_components::color_dot::TAB_COLOR_OPTIONS {
        hint.push_str(&color.to_string().to_ascii_lowercase());
        hint.push('|');
    }
    hint.push_str("none>");
    hint
});

/// GUI-only: sets the current tab's color. Not executable in the TUI (there is no concept of
/// a tab there); see `TuiTerminalSessionView::execute_tui_slash_command`'s GUI-only catch-all.
pub static SET_TAB_COLOR: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/set-tab-color",
    description: t_static!("slash-cmd-set-tab-color-desc"),
    icon_path: "bundled/svg/ellipse.svg",
    availability: Availability::ALWAYS,
    auto_enter_ai_mode: false,
    argument: Some(Argument::required().with_hint_text(SET_TAB_COLOR_HINT.as_str())),
});

/// Reports how much of the active model's context window the current conversation occupies.
///
/// Sanctioned BYOP divergence from Warp (AGENTS §5.10): Warp's `/usage` opens its hosted
/// billing-and-usage pane, which reports plan credits and quota against Warp's servers. This
/// fork has no hosted subscription to report on — the user pays their own provider — so
/// `/usage` reports the budget a BYOP conversation genuinely spends against, the context
/// window. It is a new presentation of `AIConversation::context_window_usage`, the same
/// number both footers already show; see `ai::usage_cost` for the full rationale.
pub static USAGE: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/usage",
    description: t_static!("slash-cmd-usage-desc"),
    icon_path: "bundled/svg/bar-chart-04.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

/// Reports what the current conversation's tokens cost at the user's configured provider
/// rates.
///
/// Sanctioned BYOP divergence from Warp (AGENTS §5.10): Warp's `/cost` toggles a usage footer
/// whose money figure is computed server-side by Warp against its own price list. A BYOP
/// provider returns token counts and never a dollar figure, so this fork multiplies those
/// counts by the rates the user configured for the provider/model
/// (`AgentProviderModel::token_price`, defaulting to `AgentProvider::token_price`). When no
/// rate is configured it reports the token counts and says so, rather than inventing one; see
/// `ai::usage_cost`.
pub static COST: LazyLock<StaticCommand> = LazyLock::new(|| StaticCommand {
    name: "/cost",
    description: t_static!("slash-cmd-cost-desc"),
    icon_path: "bundled/svg/coins-stacked-02.svg",
    availability: Availability::AI_ENABLED,
    auto_enter_ai_mode: false,
    argument: None,
});

pub static COMMAND_REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

/// A unique identifier for a static slash command.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct SlashCommandId(Uuid);

impl SlashCommandId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SlashCommandId {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Registry {
    commands: HashMap<SlashCommandId, StaticCommand>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        let mut commands = HashMap::new();
        for command in all_commands().into_iter() {
            debug_assert!(
                !command
                    .availability
                    .contains(Availability::TERMINAL_VIEW | Availability::AGENT_VIEW),
                "command `{}` sets both TERMINAL_VIEW and AGENT_VIEW, which is unsatisfiable",
                command.name,
            );
            commands.insert(SlashCommandId::new(), command);
        }
        Self { commands }
    }

    pub fn all_commands_by_id(&self) -> impl Iterator<Item = (SlashCommandId, &StaticCommand)> {
        self.commands.iter().map(|(id, cmd)| (*id, cmd))
    }

    pub fn all_commands(&self) -> impl Iterator<Item = &StaticCommand> {
        self.commands.values()
    }

    pub fn get_command(&self, id: &SlashCommandId) -> Option<&StaticCommand> {
        self.commands.get(id)
    }

    pub fn get_command_with_name(&self, name: &str) -> Option<&StaticCommand> {
        self.commands.values().find(|command| command.name == name)
    }

    #[cfg(test)]
    pub fn get_command_id_with_name(&self, name: &str) -> Option<&SlashCommandId> {
        self.commands
            .iter()
            .find(|(_, command)| command.name == name)
            .map(|(id, _)| id)
    }
}

fn all_commands() -> Vec<StaticCommand> {
    let mut commands = vec![
        ADD_MCP.clone(),
        ADD_PROMPT.clone(),
        ADD_RULE.clone(),
        INIT.clone(),
        OPEN_PROJECT_RULES.clone(),
        OPEN_MCP_SERVERS.clone(),
        OPEN_RULES.clone(),
        AGENT.clone(),
        NEW.clone(),
        PLAN.clone(),
        RENAME_TAB.clone(),
        STATUSLINE.clone(),
        CONVERSATIONS.clone(),
        EXPORT_TO_CLIPBOARD.clone(),
        COPY_DEBUGGING_ID.clone(),
        MODEL.clone(),
        API_KEYS.clone(),
    ];

    if FeatureFlag::LocalDockerSandbox.is_enabled() {
        commands.push(CREATE_DOCKER_SANDBOX.clone());
    }

    if FeatureFlag::Changelog.is_enabled() {
        commands.push(CHANGELOG.clone());
    }

    if FeatureFlag::AgentView.is_enabled() {
        commands.push(PROMPTS.clone());
    }

    commands.push(OPEN_CODE_REVIEW.clone());

    if FeatureFlag::CreateProjectFlow.is_enabled() {
        commands.push(CREATE_NEW_PROJECT.clone());
    }

    if FeatureFlag::SummarizationConversationCommand.is_enabled() {
        commands.push(COMPACT.clone());
        commands.push(COMPACT_AND.clone());
    }

    if FeatureFlag::QueueSlashCommand.is_enabled() {
        commands.push(QUEUE.clone());
    }

    if !cfg!(target_family = "wasm") {
        commands.extend([FORK.clone(), FORK_AND_COMPACT.clone()]);

        if FeatureFlag::ForkFromCommand.is_enabled() {
            commands.push(FORK_FROM.clone());
        }

        // No feature flag (AGENTS §5.4): the maintainer asked for this usable
        // before its options are fixed, and it has no half-built state to
        // hide -- it either spawns a local child agent or shows an error.
        commands.push(ORCHESTRATE.clone());
    }

    if !cfg!(target_family = "wasm") {
        commands.extend([EDIT.clone(), EXPORT_TO_FILE.clone()]);
    }

    if FeatureFlag::ListSkills.is_enabled() && !cfg!(target_family = "wasm") {
        commands.push(EDIT_SKILL.clone());
        commands.push(INVOKE_SKILL.clone());
    }

    if FeatureFlag::PRCommentsSlashCommand.is_enabled()
        && !FeatureFlag::PRCommentsSkill.is_enabled()
    {
        commands.push(PR_COMMENTS.clone());
    }

    if FeatureFlag::InlineProfileSelector.is_enabled() {
        commands.push(PROFILE.clone());
    }

    if FeatureFlag::RevertToCheckpoints.is_enabled() && FeatureFlag::RewindSlashCommand.is_enabled()
    {
        commands.push(REWIND.clone());
    }

    if FeatureFlag::InlineRepoMenu.is_enabled() && !cfg!(target_family = "wasm") {
        commands.push(OPEN_REPO.clone());
    }

    if FeatureFlag::SettingsFile.is_enabled() && cfg!(feature = "local_fs") {
        commands.push(OPEN_SETTINGS_FILE.clone());
    }

    commands.push(VIM_MODE.clone());
    // TUI-only toggles with no feature flag (AGENTS §5.4): both flip a setting the app
    // already owns and already renders, and both have live TUI dispatch handlers — they
    // were simply never registered, so the rows were unreachable (see #147/#338).
    commands.push(AUTO_APPROVE.clone());
    commands.push(NATURAL_LANGUAGE_DETECTION.clone());
    // No feature flag (AGENTS §5.4): both are read-only reports over data the app already
    // holds, with no rollout risk and no half-built state to hide, matching the other
    // fork-native always-on commands (`/vim-mode`, `/api-keys`).
    commands.push(USAGE.clone());
    commands.push(COST.clone());
    // TUI-only, no feature flag (AGENTS §5.4): each already has a live TUI dispatch handler
    // in `TuiTerminalSessionView::execute_tui_slash_command` -- only the registry entry was
    // missing, so the rows were unreachable (see #147/#338).
    commands.push(EXIT.clone());
    commands.push(MCP.clone());
    commands.push(STATUS.clone());
    commands.push(VIEW_LOGS.clone());
    commands.push(CLEAR.clone());
    // TUI-only, no feature flag (AGENTS §5.4): already has a live TUI dispatch handler
    // (`TuiTerminalSessionView::toggle_theme`) backed by the already-shipped `TuiTheme`
    // setting; only the registry entry and dispatch arm were missing (see #147).
    commands.push(THEME.clone());
    // GUI-only, no feature flag: reuses the already-shipped, already-tested
    // `WorkspaceAction::SetActiveTabColor` (see `app/src/workspace/view_test.rs`); only the
    // slash-command registration and dispatch arm were missing (see #147).
    commands.push(SET_TAB_COLOR.clone());

    commands
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn command_names_are_unique() {
        let names = COMMAND_REGISTRY.all_commands().map(|command| command.name);
        let mut seen = HashSet::new();
        for name in names {
            assert!(seen.insert(name), "duplicate slash command name: {name}");
        }
    }

    /// Ported from Warp's `command_names_and_kinds_are_unique_per_surface` (name uniqueness is
    /// already covered by `command_names_are_unique` above; this covers the `kind()` half). Zap
    /// derives `kind()` from the command name rather than storing it, and multiple Zap-native
    /// commands (e.g. `/pr-comments`) intentionally share `SlashCommandKind::Other` -- see the
    /// doc comment on `SlashCommandKind::Other` -- so those are excluded from the uniqueness check.
    #[test]
    fn command_kinds_are_unique_excluding_other() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        let mut kinds = HashSet::new();
        for command in COMMAND_REGISTRY.all_commands() {
            let kind = command.kind();
            if kind == SlashCommandKind::Other {
                continue;
            }
            assert!(
                kinds.insert(kind),
                "duplicate slash command kind for {}: {kind:?}",
                command.name
            );
        }
    }

    /// Adapted from the pin's `copy_debugging_id_command_has_correct_registry_metadata`
    /// (upstream b4070d6a9). The pin asserts on a stored `kind` field and a
    /// `SlashCommandSurfaces::GuiAndTui` value; this fork derives both from the command name,
    /// so the equivalent assertions are `kind()`, `supports_gui()` and `supports_tui()`.
    #[test]
    fn copy_debugging_id_command_is_registered_for_gui_and_tui() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        let command = COMMAND_REGISTRY
            .get_command_with_name(COPY_DEBUGGING_ID.name)
            .expect("expected /copy-debugging-id to be registered");

        assert_eq!(command.name, "/copy-debugging-id");
        assert_eq!(command.kind(), SlashCommandKind::CopyDebuggingId);
        assert!(command.supports_gui());
        assert!(command.supports_tui());
        assert!(!command.auto_enter_ai_mode);
        assert_eq!(command.availability, Availability::ACTIVE_CONVERSATION);
        assert!(command.argument.is_none());
        // Available once there is an active conversation, hidden before that.
        assert!(command.is_active(Availability::ACTIVE_CONVERSATION));
        assert!(!command.is_active(Availability::ALWAYS));
    }

    /// Ported from Warp's `api_keys_command_is_tui_only_and_has_no_arguments`. Unlike Warp,
    /// `/api-keys` is Zap-native (see the doc comment on `API_KEYS`) and is not restricted to
    /// the TUI surface, so this only checks what still applies: registration, metadata, and TUI
    /// support.
    #[test]
    fn api_keys_command_has_no_arguments_and_supports_tui() {
        crate::i18n::init(Some("en"));

        let command = COMMAND_REGISTRY
            .get_command_with_name(API_KEYS.name)
            .expect("expected /api-keys to be registered");
        assert_eq!(command, &*API_KEYS);
        assert_eq!(command.availability, Availability::AI_ENABLED);
        assert!(command.argument.is_none());
        assert_eq!(
            command.description,
            "Add, view, or clear a provider's API key"
        );
        assert!(command.supports_tui());
    }

    /// Ported from Warp OSS `commands_tests.rs::version_command_is_not_registered`.
    #[test]
    fn version_command_is_not_registered() {
        assert!(COMMAND_REGISTRY.get_command_with_name("/version").is_none());
    }

    /// Fork-authored (AGENTS §5.10): `/orchestrate` is a fork-native, user-invoked command
    /// with no pin equivalent to port a test from -- the pin's `/orchestrate` submits as a
    /// prompt for the model (`SlashCommandKind::Orchestrate`); this one executes directly
    /// (see `commands::ORCHESTRATE`'s doc comment for why they deliberately diverge).
    #[test]
    fn orchestrate_command_is_registered_and_available_on_both_surfaces() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        let command = COMMAND_REGISTRY
            .get_command_with_name(ORCHESTRATE.name)
            .expect("expected /orchestrate to be registered");
        // No `kind()` arm maps to it: this must stay `Other`, not
        // `SlashCommandKind::Orchestrate` -- see the module doc comment on
        // `ORCHESTRATE` for why reusing that kind would smuggle in the deferred
        // agent-invoked path. The TUI dispatches it via a name guard on the
        // `Other` arm of `execute_tui_slash_command` (AGENTS §5.9 parity) --
        // see `crates/warp_tui/src/pane_group.rs`'s `TuiPaneGroup` for the TUI's
        // local-child-spawning counterpart to the GUI's `PaneGroup`.
        assert_eq!(command.kind(), SlashCommandKind::Other);
        assert!(
            command.supports_tui(),
            "/orchestrate must stay at TUI/GUI parity (AGENTS §5.9): TuiPaneGroup closed the \
             PaneGroup hidden-pane gap this command used to be GUI-only for"
        );
        assert!(command.supports_gui());
        assert!(!command.auto_enter_ai_mode);
        assert_eq!(
            command.availability,
            Availability::ACTIVE_CONVERSATION
                | Availability::NO_LRC_CONTROL
                | Availability::LOCAL
                | Availability::AI_ENABLED
        );
        let argument = command
            .argument
            .as_ref()
            .expect("expected /orchestrate to declare an argument");
        assert!(!argument.is_optional);
        assert!(
            !argument.should_execute_on_selection,
            "selecting /orchestrate from the menu should insert text, not execute with no task"
        );
        assert!(argument.hint_text.is_some());
    }

    #[test]
    fn statusline_command_is_registered_and_tui_only() {
        let command = COMMAND_REGISTRY
            .get_command_with_name(STATUSLINE.name)
            .expect("expected /statusline to be registered");
        assert_eq!(
            command.kind(),
            crate::search::slash_command_menu::static_commands::SlashCommandKind::Statusline
        );
        assert_eq!(command.availability, Availability::ALWAYS);
        assert!(!command.auto_enter_ai_mode);
        assert!(command.argument.is_none());
        assert!(command.supports_tui());
    }

    #[test]
    fn status_command_is_registered_and_tui_only() {
        let command = COMMAND_REGISTRY
            .get_command_with_name(STATUS.name)
            .expect("expected /status to be registered");
        assert_eq!(
            command.kind(),
            crate::search::slash_command_menu::static_commands::SlashCommandKind::Status
        );
        assert_eq!(command.availability, Availability::ALWAYS);
        assert!(!command.auto_enter_ai_mode);
        assert!(command.argument.is_none());
        assert!(command.supports_tui());
        assert!(command.is_tui_only());
    }

    /// Fork-authored (AGENTS §5.10): `/usage` and `/cost` mean something different here than
    /// in Warp — see their doc comments — so upstream has no test to port for this.
    #[test]
    fn usage_and_cost_commands_are_registered_and_tui_capable() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        for (command, expected_kind) in [
            (&*USAGE, SlashCommandKind::Usage),
            (&*COST, SlashCommandKind::Cost),
        ] {
            let registered = COMMAND_REGISTRY
                .get_command_with_name(command.name)
                .unwrap_or_else(|| panic!("expected {} to be registered", command.name));
            assert_eq!(registered.kind(), expected_kind);
            // Both are read-only reports over local BYOP data, so AGENTS §5.9 requires them
            // in the TUI as well as the GUI.
            assert!(
                registered.supports_tui(),
                "{} must be executable in the TUI",
                command.name
            );
            // No argument: selecting from the menu executes immediately rather than inserting
            // text for the user to complete.
            assert!(registered.argument.is_none());
            assert!(!registered.auto_enter_ai_mode);
            // Availability stops at AI_ENABLED on purpose: both commands answer usefully with
            // no conversation open, and their handlers say so in words.
            assert_eq!(registered.availability, Availability::AI_ENABLED);
        }
    }

    /// Ported from the pinned oracle's `commands_tests.rs::view_logs_command_is_registered_only_for_tui_mode`
    /// (`02b53fcd8`) and its `exit_command`/`mcp_command` counterparts (folded into one test:
    /// unlike the oracle, the fork has one registry rather than a GUI/TUI-filtered one, so
    /// "registered only for TUI mode" is expressed via `supports_tui()` here — see #338).
    #[test]
    fn exit_mcp_and_view_logs_commands_are_registered_and_tui_only() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        for (command, expected_kind) in [
            (&*EXIT, SlashCommandKind::Exit),
            (&*MCP, SlashCommandKind::Mcp),
            (&*VIEW_LOGS, SlashCommandKind::ViewLogs),
        ] {
            let registered = COMMAND_REGISTRY
                .get_command_with_name(command.name)
                .unwrap_or_else(|| panic!("expected {} to be registered", command.name));
            assert_eq!(registered.kind(), expected_kind);
            assert!(
                registered.supports_tui(),
                "{} must be executable in the TUI",
                command.name
            );
            assert!(registered.argument.is_none());
            assert!(!registered.auto_enter_ai_mode);
        }
    }

    /// Ported from the pinned oracle's `commands_tests.rs::auto_approve_command_is_local_agent_action_without_arguments`.
    /// The oracle also asserts `NOT_CLOUD_AGENT`/`CLOUD_AGENT` bits the fork's `Availability`
    /// does not carry (the fork has no cloud-agent mode at all, so every command is implicitly
    /// non-cloud) -- omitted here for the same reason the fork's other ported commands omit it.
    #[test]
    fn auto_approve_command_is_local_agent_action_without_arguments() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        let command = COMMAND_REGISTRY
            .get_command_with_name(AUTO_APPROVE.name)
            .expect("expected /auto-approve to be registered");
        assert_eq!(command.kind(), SlashCommandKind::AutoApprove);
        assert!(command.supports_tui());
        assert!(!command.auto_enter_ai_mode);
        assert_eq!(
            command.availability,
            Availability::AGENT_VIEW | Availability::ACTIVE_CONVERSATION | Availability::AI_ENABLED
        );
        assert!(command.argument.is_none());
    }

    /// Ported from the pinned oracle's
    /// `commands_tests.rs::natural_language_detection_command_is_ai_enabled_and_executes_immediately`.
    #[test]
    fn natural_language_detection_command_is_ai_enabled_and_executes_immediately() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        let command = COMMAND_REGISTRY
            .get_command_with_name(NATURAL_LANGUAGE_DETECTION.name)
            .expect("expected /natural-language-detection to be registered");
        assert_eq!(command.kind(), SlashCommandKind::NaturalLanguageDetection);
        assert!(command.supports_tui());
        assert_eq!(command.availability, Availability::AI_ENABLED);
        assert!(!command.auto_enter_ai_mode);
        assert!(command.argument.is_none());
    }

    /// Ported from the pinned oracle's `commands_tests.rs::clear_command_has_correct_registry_metadata`
    /// and `clear_command_is_active_only_outside_cloud_mode` (the latter's `NOT_CLOUD_AGENT` bit
    /// omitted for the same reason as `auto_approve_command_is_local_agent_action_without_arguments`).
    #[test]
    fn clear_command_has_correct_registry_metadata() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        let command = COMMAND_REGISTRY
            .get_command_with_name(CLEAR.name)
            .expect("expected /clear to be registered");
        assert_eq!(command.kind(), SlashCommandKind::Clear);
        assert!(command.supports_tui());
        assert!(!command.auto_enter_ai_mode);
        assert_eq!(
            command.availability,
            Availability::NO_LRC_CONTROL | Availability::AI_ENABLED
        );
        let argument = command
            .argument
            .as_ref()
            .expect("expected /clear to declare an argument");
        assert!(argument.is_optional);
        assert!(argument.should_execute_on_selection);
        assert!(argument.hint_text.is_none());

        let local_context = Availability::NO_LRC_CONTROL | Availability::AI_ENABLED;
        assert!(command.is_active(local_context));
    }

    /// Ported from the pinned oracle's `commands_tests.rs::theme_command_is_registered_only_for_tui_mode`.
    /// The oracle asserts the GUI-mode registry never yields `SlashCommandKind::Theme`; the
    /// fork has one registry rather than a GUI/TUI-filtered one, so "TUI only" is expressed
    /// via `supports_tui()`/`is_tui_only()` here, matching the other ported TUI-only commands
    /// (see `exit_mcp_and_view_logs_commands_are_registered_and_tui_only`).
    #[test]
    fn theme_command_has_correct_registry_metadata() {
        use crate::search::slash_command_menu::static_commands::SlashCommandKind;

        let command = COMMAND_REGISTRY
            .get_command_with_name(THEME.name)
            .expect("expected /theme to be registered");
        assert_eq!(command.kind(), SlashCommandKind::Theme);
        assert!(command.supports_tui());
        assert!(
            !command.supports_gui(),
            "/theme is TUI-only, matching the oracle"
        );
        assert!(!command.auto_enter_ai_mode);
        assert_eq!(command.availability, Availability::ALWAYS);
        let argument = command
            .argument
            .as_ref()
            .expect("expected /theme to require an argument");
        assert!(!argument.is_optional);
        assert!(!argument.should_execute_on_selection);
        assert_eq!(argument.hint_text, Some("<auto|light|dark>"));
    }

    /// Ported from the pinned oracle's `commands_tests.rs::set_tab_color_command_requires_argument`.
    #[test]
    fn set_tab_color_command_requires_argument() {
        let command = COMMAND_REGISTRY
            .get_command_with_name(SET_TAB_COLOR.name)
            .expect("expected /set-tab-color to be registered");
        assert!(
            !command.supports_tui(),
            "/set-tab-color has no TUI concept of a tab and must stay GUI-only"
        );
        let argument = command
            .argument
            .as_ref()
            .expect("expected /set-tab-color to require an argument");

        assert!(!argument.is_optional);
        assert!(!argument.should_execute_on_selection);

        let hint = argument
            .hint_text
            .expect("/set-tab-color hint text is set dynamically");
        for color in crate::ui_components::color_dot::TAB_COLOR_OPTIONS {
            let lower = color.to_string().to_ascii_lowercase();
            assert!(hint.contains(&lower), "hint should mention `{lower}`");
        }
        assert!(hint.contains("none"), "hint should mention `none`");
    }

    #[test]
    fn rename_tab_command_requires_argument() {
        // hint_text goes through i18n; initialize the loader to get the real English copy
        crate::i18n::init(Some("en"));
        let command = COMMAND_REGISTRY
            .get_command_with_name(RENAME_TAB.name)
            .expect("expected /rename-tab to be registered");
        let argument = command
            .argument
            .as_ref()
            .expect("expected /rename-tab to require an argument");

        assert!(!argument.is_optional);
        assert!(!argument.should_execute_on_selection);
        assert_eq!(argument.hint_text, Some("<tab name>"));
    }

    #[test]
    fn strip_command_prefix_no_match() {
        let result = strip_command_prefix("just a normal query", "/plan");
        assert_eq!(result, None);
    }

    #[test]
    fn strip_command_prefix_empty() {
        let result = strip_command_prefix("", "/plan");
        assert_eq!(result, None);
    }

    #[test]
    fn strip_command_prefix_no_trailing_space() {
        // "/plan" alone (no trailing space) should NOT be stripped
        let result = strip_command_prefix("/plan", "/plan");
        assert_eq!(result, None);
    }

    #[test]
    fn strip_command_prefix_trailing_space_only() {
        // "/plan " with nothing after should strip to empty string
        let result = strip_command_prefix("/plan ", "/plan");
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn strip_command_prefix_substring_not_matched() {
        // "/planning" should not match "/plan"
        let result = strip_command_prefix("/planning something", "/plan");
        assert_eq!(result, None);
    }

    #[test]
    fn strip_command_prefix_matches_orchestrate() {
        // `strip_command_prefix` is a plain string helper; exercise it against
        // `/orchestrate`'s argument text same as any other command with an argument.
        let result = strip_command_prefix("/orchestrate deploy services", "/orchestrate");
        assert_eq!(result, Some("deploy services".to_string()));
    }
}
