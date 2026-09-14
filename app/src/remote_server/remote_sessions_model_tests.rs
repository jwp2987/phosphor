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
//!
//! The same applies to `RemoteSessionsModel::forget_exited_sessions`: issuing
//! `ForgetSession` needs a client, and whether the daemon accepts or refuses one
//! is a fact about a live daemon. What is tested here is everything that decides
//! *which* sessions it would name, whether it is allowed to start at all, and
//! what the pane is told afterwards.

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
        buffered_bytes: Some(0),
        dropped_bytes_total: Some(0),
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
    let mut reaps = HashMap::new();
    let h = host("alpha");
    apply_in_flight(&mut per_host, &h);

    assert!(
        apply_disconnect(&mut per_host, &mut reaps, &h),
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
    let mut reaps = HashMap::new();
    let h = host("alpha");
    let other = host("beta");
    per_host.insert(
        h.clone(),
        HostSessionsFetch::Fetched {
            fetched_at: at(0),
            sessions: vec![summary("s1", None)],
        },
    );

    assert!(apply_disconnect(&mut per_host, &mut reaps, &h));
    assert_eq!(per_host.get(&h), None);
    assert!(
        !apply_disconnect(&mut per_host, &mut reaps, &other),
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

/// The daemon refuses `ForgetSession` for a session that is still running
/// (`handle_forget_session`: "still running; signal it to exit before
/// forgetting it"), so naming one is asking for a refusal we already know we
/// would get -- and a control that provokes refusals is worse than no control.
///
/// // Breaks if: `exited_session_ids` ever selects on anything but the presence
/// of `exit`, in particular if it starts requiring an exit *code* and so skips
/// a signalled session, which has none by construction.
#[test]
fn only_exited_sessions_are_ever_named_for_forgetting() {
    let sessions = vec![
        summary("running", None),
        summary(
            "exited",
            Some(RemoteSessionExit {
                exit_code: Some(0),
                signal_killed: false,
            }),
        ),
        summary(
            "signalled",
            Some(RemoteSessionExit {
                exit_code: None,
                signal_killed: true,
            }),
        ),
        // The shape nothing produces today but the wire admits: a bare
        // `exit {}`, both fields at their proto3 defaults. It has still exited.
        summary(
            "exited-somehow",
            Some(RemoteSessionExit {
                exit_code: None,
                signal_killed: false,
            }),
        ),
    ];

    assert_eq!(
        exited_session_ids(&sessions),
        vec![
            "exited".to_string(),
            "signalled".to_string(),
            "exited-somehow".to_string()
        ],
        "every exited shape is reapable, and the running one is never named"
    );
}

/// // Breaks if: `exited_session_ids` starts inventing a name for a listing
/// that holds only live sessions -- an empty result is what makes the pane hide
/// the control entirely.
#[test]
fn a_host_holding_only_running_sessions_offers_nothing_to_forget() {
    let sessions = vec![summary("a", None), summary("b", None)];

    assert!(exited_session_ids(&sessions).is_empty());
}

/// Two overlapping reaps would be forgetting id sets read from the same
/// listing; the loser would report failures for sessions the winner had already
/// removed, and the pane would show an error for a reap that worked.
///
/// // Breaks if: `apply_reap_start` reports `Issue` while a reap is outstanding.
#[test]
fn reaps_do_not_stack() {
    let mut reaps = HashMap::new();
    let h = host("alpha");

    assert_eq!(apply_reap_start(&mut reaps, &h, 3), ReapStart::Issue);
    assert_eq!(reaps.get(&h), Some(&ReapState::InFlight { count: 3 }));
    assert_eq!(
        apply_reap_start(&mut reaps, &h, 3),
        ReapStart::AlreadyInFlight
    );
}

/// A new attempt must not inherit the last one's reason: the sessions it names
/// were re-read, and the old text describes an attempt that is being replaced.
///
/// // Breaks if: `apply_reap_start` starts merging with, rather than replacing,
/// whatever was there.
#[test]
fn starting_a_reap_clears_the_previous_failure() {
    let mut reaps = HashMap::new();
    let h = host("alpha");
    reaps.insert(
        h.clone(),
        ReapState::Failed {
            failed_at: at(1),
            reason: "1 of 1 could not be forgotten: s1: Connection was dropped".to_string(),
        },
    );

    assert_eq!(apply_reap_start(&mut reaps, &h, 2), ReapStart::Issue);
    assert_eq!(reaps.get(&h), Some(&ReapState::InFlight { count: 2 }));
}

/// "1 of 6 failed" is a stray refusal and "6 of 6 failed" is a dead connection.
/// A message that said only "some sessions could not be forgotten" would render
/// those two identically.
///
/// // Breaks if: `reap_outcome` stops reporting both counts, or stops quoting a
/// reason at all.
#[test]
fn a_partial_reap_says_how_much_of_it_failed_and_why() {
    let outcome = reap_outcome(
        6,
        vec![
            "s1: Connection was dropped".to_string(),
            "s2: Connection was dropped".to_string(),
        ],
    );

    let Err(reason) = outcome else {
        panic!("a reap with failures must not report success");
    };
    assert!(reason.contains('2'), "{reason:?}");
    assert!(reason.contains('6'), "{reason:?}");
    assert!(reason.contains("Connection was dropped"), "{reason:?}");
}

/// // Breaks if: `reap_outcome` ever reports an error for a reap where nothing
/// failed, which would put a permanent "forget failed" line under a host whose
/// sessions were all forgotten.
#[test]
fn a_reap_with_no_failures_is_a_success() {
    assert_eq!(reap_outcome(4, Vec::new()), Ok(()));
}

/// Success leaves no state behind: the refreshed listing is the evidence, and a
/// second announcement of it could only ever disagree with the rows.
///
/// // Breaks if: `apply_reap_result` starts recording successes, or stops
/// clearing the in-flight entry -- either would leave "Forgetting N..." on
/// screen forever.
#[test]
fn a_successful_reap_leaves_nothing_to_say() {
    let mut reaps = HashMap::new();
    let h = host("alpha");
    apply_reap_start(&mut reaps, &h, 2);

    apply_reap_result(&mut reaps, &h, Ok(()), at(4));

    assert_eq!(reaps.get(&h), None);
}

/// The sessions are still listed whether a reap worked or not, so a failure
/// that said nothing would be indistinguishable from a click that did nothing.
///
/// // Breaks if: `apply_reap_result` drops the reason, or clears the entry on
/// failure the way it does on success.
#[test]
fn a_failed_reap_is_recorded_so_the_pane_can_say_so() {
    let mut reaps = HashMap::new();
    let h = host("alpha");
    apply_reap_start(&mut reaps, &h, 1);

    apply_reap_result(&mut reaps, &h, Err("nope".to_string()), at(7));

    assert_eq!(
        reaps.get(&h),
        Some(&ReapState::Failed {
            failed_at: at(7),
            reason: "nope".to_string(),
        })
    );
}

/// The late-arrival case for a reap, mirroring
/// `a_result_arriving_after_a_disconnect_is_discarded`: the host dropped while
/// the `ForgetSession` requests were in the air.
///
/// // Breaks if: `apply_reap_result` re-creates an entry for a host that has
/// none, pinning a failure to a connection that no longer exists -- which the
/// pane would then render under the host if it reconnected.
#[test]
fn a_reap_result_arriving_after_a_disconnect_is_discarded() {
    let mut per_host = HashMap::new();
    let mut reaps = HashMap::new();
    let h = host("alpha");
    apply_reap_start(&mut reaps, &h, 2);

    assert!(
        apply_disconnect(&mut per_host, &mut reaps, &h),
        "a host with only reap state still has something to drop"
    );

    apply_reap_result(&mut reaps, &h, Err("too late".to_string()), at(8));

    assert_eq!(reaps.get(&h), None);
}

/// `apply_disconnect` is the single definition of "forget everything about this
/// host". Splitting it in two is how a reap failure from a dead connection ends
/// up displayed under a host that has since reconnected.
///
/// // Breaks if: it stops clearing the reap map, or stops reporting a change
/// when the reap map was the only thing holding anything.
#[test]
fn a_disconnect_clears_reap_state_as_well_as_the_listing() {
    let mut per_host = HashMap::new();
    let mut reaps = HashMap::new();
    let h = host("alpha");
    per_host.insert(
        h.clone(),
        HostSessionsFetch::Fetched {
            fetched_at: at(0),
            sessions: vec![summary("s1", None)],
        },
    );
    reaps.insert(
        h.clone(),
        ReapState::Failed {
            failed_at: at(1),
            reason: "nope".to_string(),
        },
    );

    assert!(apply_disconnect(&mut per_host, &mut reaps, &h));
    assert_eq!(per_host.get(&h), None);
    assert_eq!(reaps.get(&h), None);
}
