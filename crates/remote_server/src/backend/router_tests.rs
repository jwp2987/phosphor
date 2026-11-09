use super::*;
use crate::capability::{self, ServerCapabilities};
use crate::proto::InitializeResponse;

fn host() -> HostId {
    HostId::new("h1".to_string())
}

fn probe(client_id: usize, connected: bool, caps: &[&str]) -> HelperProbe {
    HelperProbe {
        client_id,
        connected,
        capabilities: ServerCapabilities::from_initialize_response(&InitializeResponse {
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
            ..Default::default()
        }),
    }
}

#[test]
fn no_helper_means_shell() {
    assert_eq!(
        decide(None, Family::FileContext, false, false),
        BackendKind::Shell
    );
    assert_eq!(
        FeatureRouter::new().choose(&host(), Family::GitWrite, None),
        BackendKind::Shell
    );
}

#[test]
fn an_empty_capability_list_is_a_legacy_daemon_and_serves_everything() {
    let p = probe(1, true, &[]);
    for f in Family::ALL {
        assert_eq!(
            decide(Some(&p), *f, false, false),
            BackendKind::Helper,
            "{f:?}"
        );
    }
}

#[test]
fn a_partial_list_serves_only_what_it_lists() {
    let p = probe(1, true, &[capability::FS_BASIC, capability::GIT_READ]);
    assert_eq!(
        decide(Some(&p), Family::FsBasic, false, false),
        BackendKind::Helper
    );
    assert_eq!(
        decide(Some(&p), Family::GitRead, false, false),
        BackendKind::Helper
    );
    assert_eq!(
        decide(Some(&p), Family::FileContext, false, false),
        BackendKind::Shell
    );
    assert_eq!(
        decide(Some(&p), Family::GitWrite, false, false),
        BackendKind::Shell
    );
}

#[test]
fn a_lost_connection_means_shell() {
    let p = probe(1, false, &[]);
    assert_eq!(
        decide(Some(&p), Family::FsBasic, false, false),
        BackendKind::Shell
    );
}

#[test]
fn unsupported_from_the_helper_moves_that_family_to_the_shell_for_the_connection() {
    let router = FeatureRouter::new();
    let p = probe(7, true, &[]);
    assert_eq!(
        router.choose(&host(), Family::DiffState, Some(&p)),
        BackendKind::Helper
    );
    router.note_fallback(&host(), 7, Family::DiffState);
    assert_eq!(
        router.choose(&host(), Family::DiffState, Some(&p)),
        BackendKind::Shell
    );
    // Other families keep using the helper.
    assert_eq!(
        router.choose(&host(), Family::GitRead, Some(&p)),
        BackendKind::Helper
    );
    // Another host is unaffected.
    let other = HostId::new("h2".to_string());
    assert_eq!(
        router.choose(&other, Family::DiffState, Some(&p)),
        BackendKind::Helper
    );
}

#[test]
fn a_reconnect_starts_with_a_clean_slate() {
    let router = FeatureRouter::new();
    router.note_fallback(&host(), 7, Family::DiffState);
    let reconnected = probe(8, true, &[]);
    assert_eq!(
        router.choose(&host(), Family::DiffState, Some(&reconnected)),
        BackendKind::Helper
    );
}

#[test]
fn the_always_shell_override_beats_a_capable_helper() {
    let router = FeatureRouter::new();
    router.set_always_shell(AlwaysShell::only([Family::FileContext]));
    let p = probe(1, true, &[]);
    assert_eq!(
        router.choose(&host(), Family::FileContext, Some(&p)),
        BackendKind::Shell
    );
    assert_eq!(
        router.choose(&host(), Family::FsBasic, Some(&p)),
        BackendKind::Helper
    );
    router.set_always_shell(AlwaysShell::all());
    assert_eq!(
        router.choose(&host(), Family::FsBasic, Some(&p)),
        BackendKind::Shell
    );
    router.set_always_shell(AlwaysShell::none());
    assert_eq!(
        router.choose(&host(), Family::FileContext, Some(&p)),
        BackendKind::Helper
    );
}

#[test]
fn the_override_parses_names_all_and_garbage() {
    assert_eq!(AlwaysShell::parse(None), AlwaysShell::none());
    assert_eq!(AlwaysShell::parse(Some("  ")), AlwaysShell::none());
    assert_eq!(AlwaysShell::parse(Some("all")), AlwaysShell::all());
    assert_eq!(AlwaysShell::parse(Some("1")), AlwaysShell::all());
    let some = AlwaysShell::parse(Some("file_context, git_write ,nonsense"));
    assert!(some.contains(Family::FileContext) && some.contains(Family::GitWrite));
    assert!(!some.contains(Family::Buffers));
}

#[test]
fn out_of_band_channels_are_per_session() {
    use crate::backend::{ChannelKind, ExecError, ExecOutput, ExecRequest};
    use crate::shellcmd::ShellKind;
    use futures::future::BoxFuture;

    #[derive(Debug)]
    struct Stub;
    impl OutOfBandExecutor for Stub {
        fn channel_kind(&self) -> ChannelKind {
            ChannelKind::ControlMaster
        }
        fn shell_kind(&self) -> ShellKind {
            ShellKind::Posix
        }
        fn execute(&self, _: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
            Box::pin(async { Ok(ExecOutput::default()) })
        }
    }

    let router = FeatureRouter::new();
    let (a, b) = (SessionId::from(1u64), SessionId::from(2u64));
    assert!(router.executor_for([a, b]).is_none());
    router.register_out_of_band(b, Arc::new(Stub));
    assert!(
        router.executor_for([a]).is_none(),
        "session a has no channel"
    );
    assert!(router.executor_for([a, b]).is_some());
    let backend = router.backend(&host(), None, [a, b]);
    assert!(backend.is_available(Family::GitStatus));
    assert_eq!(backend.kind(Family::GitStatus), BackendKind::Shell);
    router.unregister_out_of_band(b);
    let backend = router.backend(&host(), None, [a, b]);
    assert!(!backend.is_available(Family::GitStatus));
}

#[test]
fn the_route_log_line_has_the_shape_the_lab_asserts() {
    assert_eq!(
        route_log_line(Family::FileContext, "shell", "unsupported_fallback"),
        "hop=backend_route family=file_context backend=shell reason=unsupported_fallback"
    );
    let router = FeatureRouter::new();
    let host = HostId::new("h".to_string());
    assert_eq!(
        router.route_reason(&host, Family::FileContext, None, false),
        ("unavailable", "no_channel")
    );
    assert_eq!(
        router.route_reason(&host, Family::FileContext, None, true),
        ("shell", "caps")
    );
    router.set_always_shell(AlwaysShell::all());
    assert_eq!(
        router.route_reason(&host, Family::FileContext, None, true),
        ("shell", "override")
    );
}

#[test]
fn own_writes_reach_refresh_listeners_with_the_host() {
    use std::sync::Mutex;
    let router = FeatureRouter::new();
    let seen: Arc<Mutex<Vec<(String, RemoteRefreshEvent)>>> = Arc::default();
    let sink = Arc::clone(&seen);
    router.subscribe_refresh(Arc::new(move |host, event| {
        sink.lock()
            .unwrap()
            .push((host.as_str().to_string(), event.clone()));
    }));
    router.emit_refresh(&host(), &RemoteRefreshEvent::AfterOwnWrite);
    assert_eq!(
        *seen.lock().unwrap(),
        vec![("h1".to_string(), RemoteRefreshEvent::AfterOwnWrite)]
    );
}
