//! Tests for the dashboard's per-host session-fetch state machine.
//!
//! Everything here exercises the free functions in `remote_sessions_model.rs`
//! over a plain `HashMap`, the same shape
//! `terminal/remote_server_tty/event_loop.rs` uses for `outbound_rpc_for`.
//!
//! **What is deliberately not tested, because it needs a live daemon.**
//! `RemoteSessionsModel::start_fetch` asks `RemoteServerManager::client_for_host`
//! for a client and, when it gets one, issues a real `list_sessions` RPC over an
//! SSH connection. Neither the lookup nor the round trip can be stood up here,
//! so the no-dial guarantee is enforced structurally instead: `client_for_host`
//! is the only route to a client in that function, and it is a lookup over
//! sessions the manager already holds in `Connected` state. What *is* tested is
//! every decision taken around that call.

use super::*;
use remote_server::proto::RemoteSessionExit;

fn host(id: &str) -> HostId {
    HostId::new(id.to_string())
}

fn summary(id: &str, exit: Option<RemoteSessionExit>) -> RemoteSessionSummary {
    RemoteSessionSummary {
        remote_session_id: id.to_string(),
        cwd: "/home/dev".to_string(),
        shell: Some("/bin/bash".to_string()),
        rows: 24,
        cols: 80,
        exit,
    }
}

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).expect("fixed timestamp is in range")
}

/// The rule this whole model exists to keep: a listing read over a connection
/// that is gone must never reach the screen.
///
/// // Breaks if: `host_sessions_display` ever consults `fetch` before
/// `connected`, letting a leftover `Fetched` entry render as a live session
/// list for a host with no client.
#[test]
fn a_disconnected_host_never_renders_a_leftover_listing() {
    let fetched = HostSessionsFetch::Fetched {
        fetched_at: at(0),
        sessions: vec![summary("s1", None)],
    };

    assert_eq!(
        host_sessions_display(false, Some(&fetched)),
        HostSessionsDisplay::NotConnected,
        "a host with no live client must read as not connected, whatever is still cached for it"
    );
}

/// "We never asked" and "we asked and the host is holding none" are different
/// facts about another machine. A dashboard that renders an empty list for a
/// host it never queried is lying.
///
/// // Breaks if: `NotConnected`, `NeverFetched` and `Fetched { sessions: [] }`
/// are ever collapsed into one display state.
#[test]
fn not_connected_never_fetched_and_fetched_empty_are_three_different_things() {
    let empty = HostSessionsFetch::Fetched {
        fetched_at: at(0),
        sessions: Vec::new(),
    };

    let not_connected = host_sessions_display(false, None);
    let never_fetched = host_sessions_display(true, None);
    let fetched_empty = host_sessions_display(true, Some(&empty));

    assert_eq!(not_connected, HostSessionsDisplay::NotConnected);
    assert_eq!(never_fetched, HostSessionsDisplay::NeverFetched);
    assert_eq!(
        fetched_empty,
        HostSessionsDisplay::Fetched {
            fetched_at: at(0),
            sessions: &[],
        }
    );
    assert_ne!(not_connected, never_fetched);
    assert_ne!(never_fetched, fetched_empty);
    assert_ne!(not_connected, fetched_empty);
}

/// A failed listing must not degrade into an empty one -- an empty list reads
/// as "this host is holding no sessions", which is a claim the failure did not
/// support.
///
/// // Breaks if: `apply_fetch_result` ever stores `Err` as
/// `Fetched { sessions: vec![] }`, or drops the reason text.
#[test]
fn a_failed_listing_is_a_failure_not_an_empty_list() {
    let mut per_host = HashMap::new();
    let h = host("alpha");
    apply_in_flight(&mut per_host, &h);

    let outcome = apply_fetch_result(&mut per_host, &h, Err("timed out".to_string()), at(5));

    assert_eq!(outcome, AfterFetch::Settled);
    assert_eq!(
        per_host.get(&h),
        Some(&HostSessionsFetch::Failed {
            failed_at: at(5),
            reason: "timed out".to_string(),
        })
    );
    let display = host_sessions_display(true, per_host.get(&h));
    assert_eq!(
        display,
        HostSessionsDisplay::Failed {
            failed_at: at(5),
            reason: "timed out",
        }
    );
    assert_ne!(
        display,
        HostSessionsDisplay::Fetched {
            fetched_at: at(5),
            sessions: &[],
        }
    );
}

/// Two concurrent `list_sessions` calls can complete out of order, which would
/// let an older listing overwrite a newer one.
///
/// // Breaks if: `apply_in_flight` ever reports `Issue` while a request is
/// already outstanding, or stops recording that a trigger fired mid-flight.
#[test]
fn fetches_do_not_stack_and_a_mid_flight_trigger_is_remembered() {
    let mut per_host = HashMap::new();
    let h = host("alpha");

    assert_eq!(apply_in_flight(&mut per_host, &h), StartFetch::Issue);
    assert_eq!(
        per_host.get(&h),
        Some(&HostSessionsFetch::InFlight { superseded: false })
    );

    assert_eq!(
        apply_in_flight(&mut per_host, &h),
        StartFetch::AlreadyInFlight,
        "a second trigger must never issue a second concurrent RPC"
    );
    assert_eq!(
        per_host.get(&h),
        Some(&HostSessionsFetch::InFlight { superseded: true })
    );
}

/// Without the follow-up, an exit that lands while a fetch is in flight leaves
/// the pane showing that session as running until the next connect.
///
/// // Breaks if: `apply_fetch_result` stops reporting `Refetch` for a result
/// that was superseded while in flight.
#[test]
fn a_superseded_fetch_asks_for_one_follow_up() {
    let mut per_host = HashMap::new();
    let h = host("alpha");
    apply_in_flight(&mut per_host, &h);
    apply_in_flight(&mut per_host, &h);

    let outcome = apply_fetch_result(&mut per_host, &h, Ok(vec![summary("s1", None)]), at(9));

    assert_eq!(outcome, AfterFetch::Refetch);
    // The superseded result is still stored rather than thrown away: it is the
    // most recent thing actually known, and the follow-up will replace it.
    assert!(matches!(
        per_host.get(&h),
        Some(HostSessionsFetch::Fetched { .. })
    ));
}

/// The late-arrival case: the host dropped while its request was in the air.
///
/// // Breaks if: `apply_fetch_result` ever re-creates an entry for a host that
/// has none, resurrecting a listing read over a dead connection.
#[test]
fn a_result_arriving_after_a_disconnect_is_discarded() {
    let mut per_host = HashMap::new();
    let h = host("alpha");
    apply_in_flight(&mut per_host, &h);

    assert!(
        apply_disconnect(&mut per_host, &h),
        "the in-flight entry should have been there to drop"
    );

    let outcome = apply_fetch_result(&mut per_host, &h, Ok(vec![summary("s1", None)]), at(12));

    assert_eq!(outcome, AfterFetch::Discarded);
    assert_eq!(per_host.get(&h), None);
    assert_eq!(
        host_sessions_display(false, per_host.get(&h)),
        HostSessionsDisplay::NotConnected
    );
}

/// // Breaks if: `apply_disconnect` starts leaving the entry in place (or
/// starts reporting a change when there was nothing to drop, which would make
/// the pane repaint on every unrelated host's disconnect).
#[test]
fn disconnect_drops_the_listing_and_reports_whether_it_had_one() {
    let mut per_host = HashMap::new();
    let h = host("alpha");
    let other = host("beta");
    per_host.insert(
        h.clone(),
        HostSessionsFetch::Fetched {
            fetched_at: at(0),
            sessions: vec![summary("s1", None)],
        },
    );

    assert!(apply_disconnect(&mut per_host, &h));
    assert_eq!(per_host.get(&h), None);
    assert!(
        !apply_disconnect(&mut per_host, &other),
        "a host this model was holding nothing for is not a change"
    );
}

/// Both in-flight shapes are one thing to a reader: a request is outstanding.
///
/// // Breaks if: the internal `superseded` bookkeeping ever leaks into the
/// display, giving the pane two "loading" states to tell apart.
#[test]
fn superseded_is_internal_bookkeeping_and_not_a_display_state() {
    let plain = HostSessionsFetch::InFlight { superseded: false };
    let superseded = HostSessionsFetch::InFlight { superseded: true };

    assert_eq!(
        host_sessions_display(true, Some(&plain)),
        HostSessionsDisplay::Loading
    );
    assert_eq!(
        host_sessions_display(true, Some(&superseded)),
        HostSessionsDisplay::Loading
    );
}

/// A fetched listing must survive intact -- the running/exited distinction the
/// dashboard draws lives in `RemoteSessionSummary::exit`, and dropping it here
/// would put the pane back where it was before the field existed.
///
/// // Breaks if: `apply_fetch_result` ever normalizes, filters or re-orders the
/// sessions it was handed.
#[test]
fn a_successful_listing_is_stored_exactly_as_the_daemon_reported_it() {
    let mut per_host = HashMap::new();
    let h = host("alpha");
    apply_in_flight(&mut per_host, &h);

    let reported = vec![
        summary("running", None),
        summary(
            "signalled",
            Some(RemoteSessionExit {
                exit_code: None,
                signal_killed: true,
            }),
        ),
    ];
    apply_fetch_result(&mut per_host, &h, Ok(reported.clone()), at(3));

    match per_host.get(&h) {
        Some(HostSessionsFetch::Fetched {
            fetched_at,
            sessions,
        }) => {
            assert_eq!(*fetched_at, at(3));
            assert_eq!(sessions, &reported);
        }
        other => panic!("expected a fetched listing, got {other:?}"),
    }
}
