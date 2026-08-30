// `parses_provider_api_key_setup_flag` through `version_flag_prints_cli_version`
// are ported from the pinned oracle's `crates/warp_tui/src/session_tests.rs`
// (`02b53fcd8`, release `2026.07.29.09.05` stable — see `ORACLE.md`)
// unchanged: all six are pure `TuiArgs`/`clap` parsing assertions, so no
// shape adaptation was needed even though what happens after parsing differs
// -- upstream persists via `persist_provider_key`, and this fork refuses both
// flags outright (`reject_provider_api_key_flags`, #629). See issues #392 /
// #225.
use std::io;

use ai::LLMProvider;
use clap::Parser;
use warp::tui_export::register_tui_session_view_test_singletons;
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, SingletonEntity};
use warpui_core::App;
use warpui_core::runtime::TuiDriverStartupError;

use super::{
    TuiArgs, create_terminal_session_after_login, handle_tui_driver_startup_error,
    parse_resume_token, reject_provider_api_key_flags,
};
use crate::root_view::RootTuiView;
use crate::session_registry::TuiSessions;
use crate::test_fixtures::{add_test_semantic_selection, add_test_terminal_session};

#[test]
fn parses_provider_api_key_setup_flag() {
    let args = TuiArgs::try_parse_from(["warp", "--set-provider-api-key", "anthropic"])
        .expect("provider API-key setup arguments should parse");

    assert_eq!(args.set_provider_api_key, Some(LLMProvider::Anthropic));
}

#[test]
fn parses_provider_api_key_clear_flag() {
    let args = TuiArgs::try_parse_from(["warp", "--clear-provider-api-key", "google"])
        .expect("provider API-key clear arguments should parse");

    assert_eq!(args.clear_provider_api_key, Some(LLMProvider::Google));
}

/// Fork-authored (AGENTS §5.10), regression for #629. Both key flags used to
/// write `ai::api_keys::ApiKeyManager` (secure-storage `AiApiKeys`) and print
/// "<provider> API key saved", over a store that cannot affect anything the
/// user can reach -- not because nothing reads it (six live call sites do, via
/// `is_using_api_key_for_provider`), but because every one of those reads is
/// gated on `LLMProvider::{OpenAI, Anthropic, Google}` while every model this
/// fork constructs carries `LLMProvider::Unknown`, so they all return `false`
/// whatever is stored. The key is never sent either: `RequestParams` is not
/// `Serialize` and nothing reads `params.api_keys`. See
/// `session::reject_provider_api_key_flags` for the full derivation. The flags
/// are now refused for every provider, and the refusal must name the surface
/// that does serve the agent.
///
/// Restoring the `ApiKeyManager` write path (or dropping the
/// `reject_provider_api_key_flags` call from `run`) fails this: the parsed args
/// would no longer produce an error. Trimming the message down to a bare
/// "not supported" fails it too -- a refusal that does not say where to go
/// leaves the user exactly where #629 found them.
#[test]
fn provider_api_key_flags_are_refused_for_every_provider() {
    for (flag, slug, provider) in [
        ("--set-provider-api-key", "anthropic", "Anthropic"),
        ("--set-provider-api-key", "openai", "OpenAI"),
        ("--set-provider-api-key", "google", "Google"),
        ("--set-provider-api-key", "grok", "xAI"),
        ("--clear-provider-api-key", "anthropic", "Anthropic"),
        ("--clear-provider-api-key", "openai", "OpenAI"),
        ("--clear-provider-api-key", "google", "Google"),
        ("--clear-provider-api-key", "grok", "xAI"),
    ] {
        let args = TuiArgs::try_parse_from(["warp", flag, slug])
            .expect("the flags still parse; they are refused after parsing, not by clap");
        let error = reject_provider_api_key_flags(&args)
            .expect_err("no provider has a store these flags can usefully write");
        let message = error.to_string();

        assert!(
            message.contains(flag),
            "the refusal must name the flag the user typed: {message}"
        );
        assert!(
            message.contains(provider),
            "the refusal must name the provider the user asked for: {message}"
        );
        assert!(
            message.contains("Settings > AI > Agent providers"),
            "the refusal must point at the GUI surface that stores usable keys: {message}"
        );
        assert!(
            message.contains("/api-keys"),
            "the refusal must point at the TUI surface that stores usable keys: {message}"
        );
    }
}

/// The refusal is scoped to the two key flags: an ordinary launch must still
/// reach the TUI. Without this, widening the guard to any provider-shaped
/// argument would go unnoticed.
#[test]
fn a_launch_without_the_key_flags_is_not_refused() {
    let args = TuiArgs::try_parse_from(["warp", "--auto-approve"])
        .expect("a plain launch should parse");

    assert!(reject_provider_api_key_flags(&args).is_ok());
}

#[test]
fn rejects_unknown_provider_api_key_setup_value() {
    let error = TuiArgs::try_parse_from(["warp", "--set-provider-api-key", "other"])
        .expect_err("unknown providers should be rejected");

    assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn provider_api_key_flags_are_mutually_exclusive() {
    let error = TuiArgs::try_parse_from([
        "warp",
        "--set-provider-api-key",
        "anthropic",
        "--clear-provider-api-key",
        "anthropic",
    ])
    .expect_err("setting and clearing a provider API key should conflict");

    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn provider_api_key_help_lists_supported_providers() {
    let error = TuiArgs::try_parse_from(["warp", "--help"])
        .expect_err("--help should short-circuit clap parsing");

    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    let help = error.to_string();
    for flag in ["--set-provider-api-key", "--clear-provider-api-key"] {
        let expected = format!("{flag} <{}>", LLMProvider::API_KEY_PROVIDER_VALUE_NAME);
        assert!(help.contains(&expected));
    }
    assert!(help.contains("--auto-approve"));
}

#[test]
fn version_flag_prints_cli_version() {
    let error = TuiArgs::try_parse_from(["warp", "--version"])
        .expect_err("--version should short-circuit clap parsing");

    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
    // `run()` prints only CLI_VERSION (no binary-name precursor). Clap's
    // DisplayVersion payload still contains the configured version string.
    assert!(
        error.to_string().contains(super::CLI_VERSION),
        "--version should be backed by the configured CLI version"
    );
}

#[test]
fn parses_resume_server_token() {
    let token = uuid::Uuid::new_v4().to_string();
    let args = TuiArgs::try_parse_from([
        "warp",
        "--resume",
        token.as_str(),
        "--auto-approve",
        "--api-key",
        "test-api-key",
    ])
    .expect("TUI launch arguments should parse together");

    assert_eq!(args.resume.as_deref(), Some(token.as_str()));
    assert!(args.auto_approve);
    assert_eq!(args.api_key.as_deref(), Some("test-api-key"));
    assert_eq!(
        parse_resume_token(token.clone())
            .expect("UUID token should validate")
            .as_str(),
        token
    );
}

/// Startup bootstrap must not spawn a second terminal when one already exists. Upstream calls the
/// bootstrap `ensure_terminal_session`; this fork names it `create_terminal_session_after_login`,
/// but the guard (`!sessions.is_empty()` → bail) and therefore the behaviour under test is the same.
#[test]
fn terminal_bootstrap_is_idempotent_after_background_terminal_exists() {
    App::test((), |mut app| async move {
        register_tui_session_view_test_singletons(&mut app);
        app.update(add_test_semantic_selection);
        app.update(crate::autoupdate::TuiAutoupdater::register);
        let (window_id, root) = app.update(|ctx| {
            ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| RootTuiView::new(),
            )
        });
        let sessions = app.add_singleton_model(|_| TuiSessions::new_for_test());
        let (surface, manager) = add_test_terminal_session(&mut app, window_id);
        app.update(|ctx| {
            TuiSessions::register_session(&sessions, surface, manager, true, ctx);
            create_terminal_session_after_login(&sessions, &root, ctx);
            create_terminal_session_after_login(&sessions, &root, ctx);
        });

        app.read(|ctx| assert_eq!(TuiSessions::as_ref(ctx).len(), 1));
    });
}

/// Ported from the pin's `terminal_disconnect_during_driver_startup_exits_without_error`
/// (upstream 4111d08f9), unchanged.
///
/// The assertion is that no termination result is recorded: a host terminal
/// that went away before the first frame must not turn into a non-zero exit
/// status. Reverting `handle_tui_driver_startup_error` to the single arm it
/// replaced -- which always passed `Some(Err(error))` -- fails this.
#[test]
fn terminal_disconnect_during_driver_startup_exits_without_error() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            handle_tui_driver_startup_error(
                TuiDriverStartupError::TerminalDisconnected(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "terminal disconnected",
                )),
                ctx,
            );
        });

        // `termination_result` alone cannot distinguish "exited quietly" from "never
        // exited": the test platform delegate's `terminate_app` is a no-op and this branch
        // deliberately passes `None`. Assert the termination itself, or deleting the
        // `terminate_app` call leaves this green.
        assert_eq!(
            app.termination_requests(),
            1,
            "a startup disconnect must still terminate the app"
        );
        assert!(
            app.termination_result().is_none(),
            "and must do so without a non-zero exit status"
        );
    });
}

#[test]
fn rejects_malformed_resume_server_token() {
    let error = parse_resume_token("not-a-token".to_owned())
        .expect_err("non-UUID token should be rejected");

    assert!(
        error
            .to_string()
            .contains("invalid server conversation token")
    );
}

#[test]
fn accepts_startup_without_resume() {
    let args = TuiArgs::try_parse_from(["warp"]).expect("empty arguments should parse");

    assert_eq!(args.resume, None);
    assert!(!args.auto_approve);
    assert_eq!(args.api_key, None);
    assert_eq!(args.set_provider_api_key, None);
    assert_eq!(args.clear_provider_api_key, None);
}
