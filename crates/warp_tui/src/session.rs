//! The headless `warp-tui` front-end's session bootstrap.
//!
//! [`run`] boots the real headless Warp app via [`warp::run_tui`]. Once shared
//! initialization is done, the mount built here starts the TUI driver and
//! defers creating the first terminal session until login.

use std::io::{self, IsTerminal as _, Read as _};

use ai::LLMProvider;
use ai::api_keys::ApiKeyManager;
use crate::report_error::report_error;
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use clap::error::ErrorKind;
use inquire::{InquireError, Password, PasswordDisplayMode};
use warp::settings::TuiThemeSettings;
use warp::tui_export::{Appearance, ServerConversationToken};
use warp::{TuiLoginEvent, TuiLoginModel, TuiLoginPhase};
use warpui::SingletonEntity as _;
use warpui_core::platform::{TerminationMode, WindowStyle};
use warpui_core::runtime::spawn_tui_driver;
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

// Crossterm 0.29 drops associated text in all-key mode, which breaks AltGr and dead-key input.
const REPORT_MODIFIER_KEY_LIFECYCLE: bool = false;

#[derive(Debug, Parser)]
#[command(name = "warp", version = CLI_VERSION)]
struct TuiArgs {
    /// Resume an Oz/Warp conversation by server token.
    #[arg(long)]
    resume: Option<String>,

    /// API key for non-interactive authentication.
    #[arg(long, env = "WARP_API_KEY")]
    api_key: Option<String>,

    /// Securely store a model-provider API key for Warp Agent CLI.
    #[arg(
        long,
        value_name = LLMProvider::API_KEY_PROVIDER_VALUE_NAME,
        value_parser = LLMProvider::from_api_key_slug,
        conflicts_with_all = ["resume", "clear_provider_api_key"]
    )]
    set_provider_api_key: Option<LLMProvider>,

    /// Remove a securely stored model-provider API key from Warp Agent CLI.
    #[arg(
        long,
        value_name = LLMProvider::API_KEY_PROVIDER_VALUE_NAME,
        value_parser = LLMProvider::from_api_key_slug,
        conflicts_with_all = ["resume", "set_provider_api_key"]
    )]
    clear_provider_api_key: Option<LLMProvider>,
}

enum ProviderApiKeyCommand {
    Set {
        provider: LLMProvider,
        api_key: String,
    },
    Clear {
        provider: LLMProvider,
    },
}

/// Reads a provider API key from a masked TTY prompt or, when stdin is piped,
/// from stdin. Empty input and interactive cancellation return `Ok(None)`.
/// Never echoes or logs the returned value; callers must not print it either.
fn read_provider_api_key() -> Result<Option<String>> {
    if io::stdin().is_terminal() {
        return match Password::new("Provider API key:")
            .with_display_mode(PasswordDisplayMode::Masked)
            .without_confirmation()
            .prompt()
        {
            Ok(value) if value.trim().is_empty() => Ok(None),
            Ok(value) => Ok(Some(value)),
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(None),
            Err(error) => Err(error.into()),
        };
    }

    let mut value = String::new();
    io::stdin().read_to_string(&mut value)?;
    let value = value.trim().to_owned();
    Ok((!value.is_empty()).then_some(value))
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
    // `--set-provider-api-key` / `--clear-provider-api-key`: a one-shot,
    // non-interactive credential command, not a TUI launch (clap's
    // `conflicts_with_all` above already rejects combining either with
    // `--resume`). `Xai` is parsed like the other three providers but has no
    // pasted-key path in this fork -- see `DECLINED.md`'s xAI / Grok
    // subscription OAuth entry (#319) -- so it is rejected here with
    // guidance toward the store that *does* serve it (the arbitrary-provider
    // "Agent providers" BYOP surface, reachable from Settings or the TUI's
    // own `/api-keys` menu).
    let provider_api_key_command = if let Some(provider) = args.set_provider_api_key {
        if !provider.supports_pasted_api_key() {
            return Err(anyhow!(
                "{} credentials aren't supported by --set-provider-api-key in this build; \
                 add an xAI key as a custom agent provider instead (Settings > AI > Agent \
                 providers, or the TUI's /api-keys menu)",
                provider.display_name()
            ));
        }
        let Some(api_key) = read_provider_api_key()? else {
            return Err(anyhow!("No provider API key was supplied"));
        };
        Some(ProviderApiKeyCommand::Set { provider, api_key })
    } else {
        match args.clear_provider_api_key {
            Some(LLMProvider::Xai) => {
                return Err(anyhow!(
                    "xAI credentials aren't managed by --clear-provider-api-key in this \
                     build; remove them via Settings > AI > Agent providers or the TUI's \
                     /api-keys menu instead"
                ));
            }
            Some(provider) => Some(ProviderApiKeyCommand::Clear { provider }),
            None => None,
        }
    };
    if let Some(command) = provider_api_key_command {
        // Reuses the same shared bootstrap as a normal launch (`init` below)
        // instead of a separate one-shot entry point: `mount` already runs
        // after `ApiKeyManager` is registered, and returning without adding a
        // window (mirroring the `spawn_tui_driver` error path in `init`) is
        // enough to persist the key and exit without ever drawing a TUI.
        return warp::run_tui(
            None,
            Box::new(move |ctx| {
                let (provider, key, success_verb) = match command {
                    ProviderApiKeyCommand::Set { provider, api_key } => {
                        (provider, Some(api_key), "saved")
                    }
                    ProviderApiKeyCommand::Clear { provider } => (provider, None, "cleared"),
                };
                ApiKeyManager::handle(ctx).update(ctx, |manager, ctx| match provider {
                    LLMProvider::OpenAI => manager.set_openai_key(key, ctx),
                    LLMProvider::Anthropic => manager.set_anthropic_key(key, ctx),
                    LLMProvider::Google => manager.set_google_key(key, ctx),
                    // Unreachable: `Xai` is rejected above and
                    // `from_api_key_slug` never produces `Unknown`.
                    LLMProvider::Xai | LLMProvider::Unknown => {}
                });
                // The setters above write the shared keyring synchronously, so
                // this process is done persisting by the time it bumps the
                // revision file. Already-running Zap processes -- GUI or TUI --
                // watch that file and re-read the keyring, instead of serving
                // the keys they read at their own startup until restarted.
                // A failure here means only that other processes keep stale
                // keys until they restart; the key itself is already saved, so
                // it must not turn a successful save into an error exit.
                //
                // The setters now stamp the revision themselves (the write
                // choke point in `ApiKeyManager::write_keys_to_secure_storage`,
                // so that the GUI's key editors notify too). This second stamp
                // is kept anyway: it is the only path that reports a failed
                // stamp to the user, on a CLI whose whole output is one line.
                // Two stamps in a row cost one extra file write and are
                // coalesced by the watcher's debounce.
                if let Err(error) = warp::tui_export::notify_tui_api_keys_changed() {
                    log::warn!(
                        "API key was saved, but signalling running Phosphor processes to \
                         reload it failed: {error:#}"
                    );
                }
                println!("{} API key {success_verb}", provider.display_name());
                ctx.terminate_app(TerminationMode::ForceTerminate, None);
            }),
        );
    }
    let resume_token = args.resume.map(parse_resume_token).transpose()?;
    let exit_summary = TuiExitSummaryHandle::default();
    let exit_summary_for_app = exit_summary.clone();
    let result = warp::run_tui(
        args.api_key,
        Box::new(move |ctx| init(resume_token, exit_summary_for_app, ctx)),
    );
    if result.is_ok()
        && let Some(token) = exit_summary.token()
    {
        let token = token.as_str();
        println!("To continue this conversation, run:");
        println!("warp --resume {token}");
    }
    result
}

/// Creates the login-gated root and starts the headless draw and input driver.
fn init(
    resume_token: Option<ServerConversationToken>,
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
    match spawn_tui_driver(
        ctx,
        window_id,
        root.clone(),
        Some(probe),
        REPORT_MODIFIER_KEY_LIFECYCLE,
    ) {
        Ok(driver) => {
            let sessions =
                ctx.add_singleton_model(|_| TuiSessions::new(driver, exit_summary, resume_token));
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
        Err(error) => {
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
