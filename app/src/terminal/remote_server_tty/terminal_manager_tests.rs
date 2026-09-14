use warpui::{AddSingletonModel, App};

use super::*;

fn host(name: &str) -> HostId {
    HostId::new(name.to_string())
}

fn session(name: &str) -> RemotePtySessionId {
    RemotePtySessionId::from(name.to_string())
}

// The invariant `EventLoop::start` records but cannot enforce, pinned at the
// only layer that can: a client is looked up exactly once, at construction, and
// of the manager's events only `SessionReconnected` ever carries another one. So
// a manager built for a host that has never connected holds an empty client slot
// with nothing that will fill it -- a terminal that accepts keystrokes, drops
// every one, and looks healthy while doing it. The failure is silent, which is
// why the refusal has to be structural rather than a comment.
//
// Both branches of the refusal are covered: no manager at all (a test, or a
// build that never registered one), and a manager that simply holds no client
// for this host.
//
// Breaks if: `for_spawned_session` stops consulting `RemoteServerManager` and
// hands back a witness for any pair of ids -- which is all it takes, since the
// witness is the only way to reach `TerminalManager::create_model`.
#[test]
fn no_connected_client_means_no_terminal() {
    App::test((), |mut app| async move {
        assert!(
            app.read(|ctx| ConnectedRemotePtySession::for_spawned_session(
                host("build-box"),
                session("session-a"),
                ctx
            ))
            .is_none(),
            "with no RemoteServerManager there is no transport at all"
        );

        app.add_singleton_model(RemoteServerManager::new);

        assert!(
            app.read(|ctx| ConnectedRemotePtySession::for_spawned_session(
                host("build-box"),
                session("session-a"),
                ctx
            ))
            .is_none(),
            "a manager holding no client for this host cannot carry its RPCs either"
        );
    });
}

// NOTE on what is deliberately NOT tested here, and why it is absence rather
// than an oversight.
//
// The positive direction -- that a host *with* a connected client yields a
// witness -- cannot be asserted at this layer. It needs an
// `Arc<RemoteServerClient>` lodged in `RemoteServerManager` as a `Connected`
// session, and a client has no constructor outside `crates/remote_server`'s own
// duplex-backed harness while the manager's connected state is only reachable
// through the real connect path. So the test above is half-blind by
// construction: it would still pass against a `for_spawned_session` that always
// returned `None`. That failure mode is loud (no remote terminal ever opens)
// where the one being guarded against is silent, which is why this is the half
// worth having.
//
// `create_model` itself is likewise untested: it is the blueprint's channel
// wiring with a different event loop at the end of it, every step of which needs
// a window, a font cache and a live view. What was worth pinning in this module
// is the refusal above; what the transport does once it has a client is pinned
// in `event_loop_tests.rs`, on the pure functions extracted there for exactly
// that reason.
