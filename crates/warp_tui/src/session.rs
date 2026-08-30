//! The headless `warp-tui` front-end's session bootstrap.
//!
//! [`run`] boots the real headless Warp app via [`warp::run_tui`]. Once shared
//! initialization is done, the mount built here starts the TUI driver and
//! defers creating the first terminal session until login.

use ai::LLMProvider;
use crate::report_error::report_error;
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use clap::error::ErrorKind;
use warp::settings::{TuiThemeSettings, TuiZeroStateSettings, TuiZeroStateSettingsChangedEvent};
use warp::tui_export::{AIConversationAutoexecuteMode, Appearance, ServerConversationToken};
use warp::{TuiLoginEvent, TuiLoginModel, TuiLoginPhase};
use warp_core::settings::Setting as _;
use warpui::SingletonEntity as _;
use warpui_core::platform::{TerminationMode, WindowStyle};
use warpui_core::runtime::{TuiDriverStartupError, TuiFocusPolicy, spawn_tui_driver};
use warpui_core::{AddWindowOptions, AppContext, ModelHandle, ViewHandle};

use crate::orchestration_model::TuiOrchestrationModel;
use crate::pane_group::TuiPaneGroup;
use crate::resume::TuiExitSummaryHandle;
use crate::root_view::RootTuiView;
use crate::session_registry::{TuiSessions, TuiSessionsEvent};
use crate::telemetry::TuiStartupTelemetryEvent;
use crate::terminal_background::TuiHostTerminalBackground;
use crate::terminal_session_view::{TuiConversationRestoreOrigin, TuiConversationRestoreTarget};

/// Version string printed by `--version`. Release builds get `GIT_RELEASE_TAG`
/// (the same env var `ChannelState::app_version` reads at runtime); local
/// cargo builds fall back to a numeric placeholder.
const CLI_VERSION: &str = match option_env!("GIT_RELEASE_TAG") {
    Some(version) => version,
    None => "v0.0.0.0.0.0",
};

/// Name this binary is invoked by, used for clap's usage/help output and for
/// any instruction we print for the user to run. The cargo bin is
/// `zap-tui-oss` (a lineage internal, see README "Deliberately *not* renamed"),
/// but every release job copies it to `phosphor-tui` before packaging
/// (`.github/workflows/phosphor_release.yml`), so `phosphor-tui` is the name a
/// user actually types. The pin hardcodes `"warp"` here
/// (`42effe840:crates/warp_tui/src/session.rs:51`); that name ships no binary
/// in this fork, so it is deliberately not carried over.
const CLI_NAME: &str = "phosphor-tui";

// Crossterm 0.29 drops associated text in all-key mode, which breaks AltGr and dead-key input.
const REPORT_MODIFIER_KEY_LIFECYCLE: bool = false;

#[derive(Debug, Parser)]
#[command(name = CLI_NAME, version = CLI_VERSION)]
struct TuiArgs {
    /// Resume an Oz/Warp conversation by server token.
    #[arg(long)]
    resume: Option<String>,

    /// Enable auto-approve by default for new conversations.
    #[arg(long)]
    auto_approve: bool,

    /// API key for non-interactive authentication.
    // `hide_env_values` keeps `--help` from rendering the RESOLVED value of
    // WARP_API_KEY into the terminal, scrollback, and anything capturing help
    // output. Deliberately diverges from the pin, which leaves this unguarded
    // here while guarding the identical argument in warp_cli (9dcef6a88). See
    // DECLINED.md and #588.
    #[arg(long, env = "WARP_API_KEY", hide_env_values = true)]
    api_key: Option<String>,

    /// Accepted for compatibility and always refused: keys live in the
    /// "Agent providers" store (Settings > AI > Agent providers, or the
    /// TUI's /api-keys menu). See `reject_provider_api_key_flags`.
    #[arg(
        long,
        value_name = LLMProvider::API_KEY_PROVIDER_VALUE_NAME,
        value_parser = LLMProvider::from_api_key_slug,
        conflicts_with_all = ["resume", "clear_provider_api_key"]
    )]
    set_provider_api_key: Option<LLMProvider>,

    /// Accepted for compatibility and always refused: keys live in the
    /// "Agent providers" store (Settings > AI > Agent providers, or the
    /// TUI's /api-keys menu). See `reject_provider_api_key_flags`.
    #[arg(
        long,
        value_name = LLMProvider::API_KEY_PROVIDER_VALUE_NAME,
        value_parser = LLMProvider::from_api_key_slug,
        conflicts_with_all = ["resume", "set_provider_api_key"]
    )]
    clear_provider_api_key: Option<LLMProvider>,
}

/// Refuses `--set-provider-api-key` / `--clear-provider-api-key`, for every
/// provider. `Ok(())` only when neither flag was given.
///
/// They used to write `ai::api_keys::ApiKeyManager` (secure-storage key
/// `AiApiKeys`) and report success, over a store that cannot affect anything a
/// user of this fork can reach. These flags were its only writers.
///
/// Be precise about *why*, because the obvious short version is wrong and the
/// next reader will re-derive this. The store **is** read: `is_using_api_key_for_provider`
/// (`app/src/ai/llms.rs:26`) calls `ApiKeyManager::keys()`, from six live call
/// sites, one of them behavioural rather than cosmetic (clearing
/// `DisableReason::RequiresUpgrade` in `terminal/input/models/data_source.rs:245,355`).
/// It is dead *in effect*, not dead in code: every arm that could observe a
/// stored key is gated on `LLMProvider::{OpenAI, Anthropic, Google}` with
/// `_ => false`, and no model this fork constructs carries any of those --
/// every production `LLMInfo` site sets `provider: LLMProvider::Unknown`
/// (`ai/llms.rs:295,440,467,489,511` and `ai/agent_providers/mod.rs:219,248`).
/// So each of those six reads returns `false` whatever is stored. The three
/// variants appear nowhere else in production but two further *matches*
/// (`ai/llms.rs:97` provider icons, `data_source.rs:679` the BYOK hint), never
/// a construction.
///
/// The key is also never sent. Its one other consumer is `RequestParams::api_keys`
/// (`app/src/ai/agent/api.rs:161`), a `warp_multi_agent_api` field belonging to
/// the removed server path: `RequestParams` is `#[derive(Debug, Clone)]` with no
/// `Serialize`, and nothing anywhere reads that field. The agent's BYOP send
/// path resolves keys from `AgentProviderSecrets` instead
/// (`agent_providers::lookup_byop`), so the flags' whole effect was a success
/// message over a key the agent could not use. See #629.
///
/// Pointing them at `AgentProviderSecrets` is not available as a fix: that
/// store is keyed by the UUID of a provider entry the user defined (name,
/// `base_url`, `api_type`, model list -- `settings::ai::AgentProvider`), and
/// `agent_providers` defaults to empty. A fixed [`LLMProvider`] name has no
/// entry to map onto, and inventing one would mean inventing an endpoint and a
/// model list on the user's behalf.
///
/// This subsumes the narrower `Xai` refusal that used to live here
/// (`DECLINED.md`'s xAI / Grok subscription-OAuth entry, #319): every provider
/// now gets the same message, pointing at the store that does serve the agent.
///
/// A separate function rather than inline in [`run`] so it is assertable from
/// argv without booting an app -- the whole fix is this refusal and its
/// wording, and a message that stops naming the surface which actually serves
/// the agent puts the user back where they started.
fn reject_provider_api_key_flags(args: &TuiArgs) -> Result<()> {
    // `conflicts_with_all` already rejects giving both.
    let (flag, provider) = match (args.set_provider_api_key, args.clear_provider_api_key) {
        (Some(provider), _) => ("--set-provider-api-key", provider),
        (_, Some(provider)) => ("--clear-provider-api-key", provider),
        (None, None) => return Ok(()),
    };
    Err(anyhow!(
        "{flag} is not supported in this build: the agent reads its keys from the \
         arbitrary-provider \"Agent providers\" store, which is keyed by provider entries \
         you define yourself, so a fixed provider name has nothing to write to. Manage \
         the {} key from Settings > AI > Agent providers, or the TUI's /api-keys menu.",
        provider.display_name()
    ))
}

/// Validates and wraps a server conversation token from the command line.
fn parse_resume_token(token: String) -> Result<ServerConversationToken> {
    uuid::Uuid::parse_str(&token)
        .with_context(|| format!("invalid server conversation token: {token}"))?;
    Ok(ServerConversationToken::new(token))
}

/// Boots the headless Warp app and mounts the transcript-capable TUI session.
pub fn run() -> Result<()> {
    // Protect this managed version before any worker dispatch or resource
    // access. The guard stays alive until this process exits.
    let _version_lease = crate::autoupdate::VersionLease::acquire_for_current_process()?;
    // If this process was re-exec'd as a Warp worker (e.g. the terminal
    // server), dispatch that instead of starting another TUI — otherwise the
    // worker re-exec would recursively launch TUIs.
    if let Some(result) = warp::run_tui_worker_if_requested() {
        return result;
    }
    let args = match TuiArgs::try_parse() {
        Ok(args) => args,
        // Match the zero-state version line: bare tag/version, no binary name prefix.
        Err(error) if error.kind() == ErrorKind::DisplayVersion => {
            println!("{CLI_VERSION}");
            return Ok(());
        }
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            error.print()?;
            return Ok(());
        }
        Err(error) => return Err(anyhow::Error::new(error)),
    };
    // Neither key flag does anything this build can use; see
    // `reject_provider_api_key_flags` for why they are refused rather than
    // repointed (#629).
    reject_provider_api_key_flags(&args)?;
    let resume_token = args.resume.map(parse_resume_token).transpose()?;
    // `--auto-approve` only sets the mode new conversations *start* in. It does
    // not widen what auto-approve means: this fork's `can_ask_user_question`
    // still surfaces every `ask_user_question` regardless of the conversation's
    // autoexecute mode (DECLINED.md #373), so a question is never swallowed.
    let default_autoexecute_mode = if args.auto_approve {
        AIConversationAutoexecuteMode::RunToCompletion
    } else {
        AIConversationAutoexecuteMode::RespectUserSettings
    };
    let exit_summary = TuiExitSummaryHandle::default();
    let exit_summary_for_app = exit_summary.clone();
    let result = warp::run_tui(
        args.api_key,
        Box::new(move |ctx| {
            init(
                resume_token,
                default_autoexecute_mode,
                exit_summary_for_app,
                ctx,
            )
        }),
    );
    // Currently unreachable: the token comes from
    // `AIConversation::server_conversation_token`, which BYOP never populates
    // (see DECLINED.md, "tui_cli_shell_command / tui_resume_shell_command").
    // Kept because `--resume` is a live flag, so the hint is correct the moment
    // a token exists; the name it prints must stay the name users invoke.
    if result.is_ok()
        && let Some(token) = exit_summary.token()
    {
        let token = token.as_str();
        println!("To continue this conversation, run:");
        println!("{CLI_NAME} --resume {token}");
    }
    result
}

/// Creates the login-gated root and starts the headless draw and input driver.
fn init(
    resume_token: Option<ServerConversationToken>,
    default_autoexecute_mode: AIConversationAutoexecuteMode,
    exit_summary: TuiExitSummaryHandle,
    ctx: &mut AppContext,
) {
    warp_core::send_telemetry_from_app_ctx!(TuiStartupTelemetryEvent, ctx);
    // Register the TUI views' keybindings (and, in debug builds, the
    // cross-surface binding validators) before any input can be dispatched.
    crate::keybindings::init(ctx);

    // Kick off the background auto-updater (its polling loop only runs for
    // release builds installed via the managed versioned layout; see the
    // `autoupdate` module docs).
    crate::autoupdate::TuiAutoupdater::register(ctx);

    // Register the session-scoped file-edit revert registry so `/rewind` can
    // restore files edited during this session (see tui_revert_registry).
    crate::tui_revert_registry::TuiFileEditRevertRegistry::register(ctx);

    // Load the zero-state rotating-object animation's config (built-in mark
    // vs. a user-supplied `TuiZeroStateObject::AsciiFile`, rotation period,
    // extrusion depth) from settings, and keep it live-reloading on setting
    // changes. See zero_state_animation_config.rs and #384.
    crate::zero_state_animation::ZeroStateAnimationConfig::register(ctx);

    // Theme the transcript to match the host terminal, and register the live
    // focus-triggered re-probe so a later appearance change (e.g. switching
    // the host terminal's profile) is picked up mid-session. Keep this scoped
    // to the TUI process by overriding the already-initialized Appearance
    // theme at mount time, without changing normal GUI theme selection or
    // font settings.
    let selected_theme = TuiThemeSettings::as_ref(ctx).selected_theme();
    let (theme, probe) = TuiHostTerminalBackground::register(selected_theme, ctx);
    Appearance::handle(ctx).update(ctx, |appearance, ctx| {
        appearance.set_theme(theme, ctx);
    });

    let (window_id, root) = ctx.add_tui_window(
        AddWindowOptions {
            window_style: WindowStyle::NotStealFocus,
            ..Default::default()
        },
        |_| RootTuiView::new(),
    );
    let freeze_repaints_when_unfocused = *TuiZeroStateSettings::as_ref(ctx)
        .freeze_animation_when_unfocused
        .value();
    match spawn_tui_driver(
        ctx,
        window_id,
        root.clone(),
        TuiFocusPolicy::PresentedTree,
        Some(probe),
        REPORT_MODIFIER_KEY_LIFECYCLE,
        freeze_repaints_when_unfocused,
    ) {
        Ok(driver) => {
            let sessions =
                ctx.add_singleton_model(|_| {
                    TuiSessions::new(driver, exit_summary, resume_token, default_autoexecute_mode)
                });
            let sessions_for_zero_state_settings = sessions.clone();
            ctx.subscribe_to_model(
                &TuiZeroStateSettings::handle(ctx),
                move |settings, event, ctx| {
                    let TuiZeroStateSettingsChangedEvent::TuiZeroStateFreezeAnimationWhenUnfocusedSetting {
                        ..
                    } = event
                    else {
                        return;
                    };
                    let freeze = *settings.as_ref(ctx).freeze_animation_when_unfocused.value();
                    sessions_for_zero_state_settings.update(ctx, |sessions, _| {
                        sessions.set_freeze_repaints_when_unfocused(freeze);
                    });
                    ctx.invalidate_all_views();
                },
            );
            let orchestration = TuiOrchestrationModel::register(ctx);
            TuiSessions::wire_orchestration(&sessions, &orchestration, ctx);
            TuiPaneGroup::register(ctx);
            root.update(ctx, |_, ctx| {
                ctx.subscribe_to_model(&sessions, |_, _, event, ctx| match event {
                    TuiSessionsEvent::SessionRemoved(_) => ctx.notify(),
                    TuiSessionsEvent::FocusChanged(_) => ctx.notify(),
                });
            });
            let sessions_for_login = sessions.clone();
            let root_for_login = root.clone();
            let login_model = TuiLoginModel::handle(ctx);
            ctx.subscribe_to_model(&login_model, move |_, event, ctx| match event {
                TuiLoginEvent::LoggedIn => {
                    create_terminal_session_after_login(&sessions_for_login, &root_for_login, ctx)
                }
                TuiLoginEvent::LoggedOut => {
                    root_for_login.update(ctx, |root, ctx| root.show_auth(ctx));
                    sessions_for_login.update(ctx, |sessions, ctx| sessions.clear(ctx));
                }
            });
            if matches!(TuiLoginModel::as_ref(ctx).phase(), TuiLoginPhase::LoggedIn) {
                // Already authenticated at mount: create the first session now.
                create_terminal_session_after_login(&sessions, &root, ctx);
            }
        }
        Err(error) => handle_tui_driver_startup_error(error, ctx),
    }
}

/// Ends the process after the TUI driver failed to start.
///
/// A host terminal that disappeared before the first frame is not a fault of
/// this program: log it and exit cleanly, with no termination result, so the
/// exit status stays zero. Anything else is a real startup failure and is both
/// reported and surfaced as a non-zero exit.
fn handle_tui_driver_startup_error(error: TuiDriverStartupError, ctx: &mut AppContext) {
    match error {
        TuiDriverStartupError::TerminalDisconnected(error) => {
            log::error!("failed to start the TUI driver: {error}");
            ctx.terminate_app(TerminationMode::ForceTerminate, None);
        }
        TuiDriverStartupError::Unexpected(error) => {
            let error = anyhow::Error::new(error);
            report_error!(&error);
            ctx.terminate_app(TerminationMode::ForceTerminate, Some(Err(error)));
        }
    }
}

/// Creates the focused bootstrap session and restores the requested conversation.
fn create_terminal_session_after_login(
    sessions: &ModelHandle<TuiSessions>,
    root: &ViewHandle<RootTuiView>,
    ctx: &mut AppContext,
) {
    if sessions.read(ctx, |sessions, _| !sessions.is_empty()) {
        return;
    }

    let resume_token = sessions.update(ctx, |sessions, _| sessions.take_resume_token());
    let window_id = root.window_id(ctx);
    let (_, surface) = TuiSessions::create_local_terminal_session(
        sessions,
        window_id,
        true,
        std::env::current_dir().ok(),
        ctx,
    );
    surface.update(ctx, |view, ctx| {
        view.enable_cli_agent_osc_event_publishing(ctx);
    });
    if let Some(token) = resume_token {
        surface.update(ctx, |view, ctx| {
            view.restore_conversation(
                TuiConversationRestoreTarget::Server(token),
                TuiConversationRestoreOrigin::Startup,
                ctx,
            );
        });
    }
    root.update(ctx, |root, ctx| root.show_terminal(ctx));
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
