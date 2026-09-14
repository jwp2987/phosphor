use std::time::Duration;

use remote_server::proto::ErrorCode;
use remote_server::protocol::ProtocolError;

use super::*;

fn host(name: &str) -> HostId {
    HostId::new(name.to_string())
}

fn session(name: &str) -> RemotePtySessionId {
    RemotePtySessionId::from(name.to_string())
}

// The one error the daemon itself authored, and the only one that licenses
// *not* chasing an orphan.
//
// It is not a reading of the message text: `spawn_session` produces this
// variant only from `spawn_session_response::Result::Error`, which
// `ServerModel::handle_spawn_session` returns after `self.session_store
// .remove(&id)`, and `LiveSession::spawn` guards every fallible step after the
// pty exists with `KillPtyOnDrop`. So the host really is back where it started.
//
// Breaks if: this is widened to cover errors the daemon did not author -- a
// timeout, a dropped connection mid-request -- which would stop the kill in
// `spawn_and_settle` from firing and leave a pty running on someone else's
// machine with nothing pointing at it.
#[test]
fn a_refusal_from_the_daemon_leaves_nothing_behind() {
    assert_eq!(
        spawn_aftermath_for(&ClientError::SessionOperationFailed(
            "unsupported shell: /bin/tcsh".to_string()
        )),
        SpawnAftermath::NothingSpawned
    );
}

// `Disconnected` is the precise arm, and precision here is the point: it is the
// *common* failure (the host is down), so classifying it as a possible orphan
// would put an orphan warning on every ordinary attempt and train everyone to
// ignore the warning that matters.
//
// It is sound because `RemoteServerClient::send_request` returns `Disconnected`
// from exactly two places, both before the message reaches the wire -- the
// `self.disconnected` check, and `outbound_tx.send(msg).await.is_err()`. A
// connection lost *after* a successful enqueue surfaces as
// `ResponseChannelClosed` or `Timeout`, which the test below keeps on the
// orphan side.
//
// Breaks if: `send_request` starts enqueuing before checking, or grows a retry
// that can report `Disconnected` after the bytes went out. Then this arm is
// wrong and must move -- which is a change to `crates/remote_server`, not to
// this test's expectation.
#[test]
fn a_disconnect_before_the_request_is_sent_leaves_nothing_behind() {
    assert_eq!(
        spawn_aftermath_for(&ClientError::Disconnected),
        SpawnAftermath::NothingSpawned
    );
}

// Every error where the daemon's answer is unknown has to be assumed to have
// spawned something, because the alternative -- assuming it did not -- is
// silent and permanent: a pty on a remote host that nothing in this app has a
// handle on.
//
// `FileOperationFailed` is in here despite being unreachable for this RPC. It
// is the file-operation arm of a shared enum, and grouping it with the
// conservative answer means that if it ever does become reachable the default
// is a spurious kill rather than a hidden orphan.
//
// Breaks if: any of these is moved to `NothingSpawned` -- `spawn_and_settle`
// would then skip the `SignalSession`/`Kill` cleanup for it.
#[test]
fn every_ambiguous_failure_is_treated_as_a_possible_orphan() {
    for error in [
        ClientError::Protocol(ProtocolError::UnexpectedEof),
        ClientError::ResponseChannelClosed,
        ClientError::UnexpectedResponse,
        ClientError::ServerError {
            code: ErrorCode::Internal,
            message: "boom".to_string(),
        },
        ClientError::Timeout(Duration::from_secs(30)),
        ClientError::FileOperationFailed("not ours".to_string()),
    ] {
        assert_eq!(
            spawn_aftermath_for(&error),
            SpawnAftermath::PossiblyOrphaned,
            "{error} must be assumed to have left a session running"
        );
    }
}

// `Spawned` must be the *only* outcome that lets the caller go on to build a
// witness, because the witness means "the daemon has this session" and nothing
// else here can attest to that.
//
// Breaks if: a failing outcome starts mapping to `None`, which would send
// `finish_spawn` on to `ConnectedRemotePtySession::for_spawned_session` for a
// session that was never spawned -- and that check would pass, since it only
// asks whether the host has a connected client. The witness's second
// obligation has no other guard.
#[test]
fn only_a_spawned_session_is_allowed_to_reach_the_witness() {
    assert_eq!(
        failure_for(session("s-1"), SpawnRpcOutcome::Spawned, &host("build-box")),
        None
    );

    for outcome in [
        SpawnRpcOutcome::NoClient,
        SpawnRpcOutcome::Refused("refused".to_string()),
        SpawnRpcOutcome::Indeterminate {
            detail: "timed out".to_string(),
            cleanup: OrphanCleanup::Killed,
        },
    ] {
        assert!(
            failure_for(session("s-1"), outcome.clone(), &host("build-box")).is_some(),
            "{outcome:?} must not reach the witness"
        );
    }
}

// The session id has to survive into the failure, and only into the failures
// where an id was actually used.
//
// This is the difference between a report someone can act on ("session
// 9f2c... may be running on build-box") and one nobody can ("spawning failed").
// `NoClientForHost` deliberately carries no id: nothing was sent, so quoting an
// id there would send a reader looking for a session that never existed.
//
// Breaks if: `failure_for` mints, drops or substitutes an id -- for instance by
// building `Refused` from a default rather than the id the RPC was sent under.
#[test]
fn a_failure_names_the_session_that_may_be_running() {
    let id = session("session-a");

    assert_eq!(
        failure_for(
            id.clone(),
            SpawnRpcOutcome::Indeterminate {
                detail: "timed out".to_string(),
                cleanup: OrphanCleanup::NotKilled("gone".to_string()),
            },
            &host("build-box")
        ),
        Some(RemoteSessionSpawnFailure::Indeterminate {
            session: id.clone(),
            detail: "timed out".to_string(),
            cleanup: OrphanCleanup::NotKilled("gone".to_string()),
        })
    );

    assert_eq!(
        failure_for(
            id.clone(),
            SpawnRpcOutcome::Refused("no such shell".to_string()),
            &host("build-box")
        ),
        Some(RemoteSessionSpawnFailure::Refused {
            session: id,
            detail: "no such shell".to_string(),
        })
    );

    assert_eq!(
        failure_for(
            session("unused"),
            SpawnRpcOutcome::NoClient,
            &host("build-box")
        ),
        Some(RemoteSessionSpawnFailure::NoClientForHost {
            host_id: host("build-box"),
        }),
        "nothing was sent, so no session id may be quoted"
    );
}

// The client must not guess anything about the far side.
//
// `cwd` is the one worth guarding: defaulting it to this machine's working
// directory is the obvious-looking thing to write, and it is wrong in a way
// that only shows up once the bootstrap starts honouring it -- the daemon
// exports it as `WARP_INITIAL_WORKING_DIR` for the shell's init script to `cd`
// to, and a local path almost never exists on the remote host. `shell` is the
// same mistake in a different shape: the local user's shell is not the remote
// user's, and `None` asks the daemon to resolve the remote user's own.
//
// Breaks if: `for_host` starts filling either from local state.
#[test]
fn a_default_request_guesses_nothing_about_the_remote_host() {
    let request = RemoteSessionSpawnRequest::for_host(host("build-box"));

    assert_eq!(
        request.cwd, "",
        "the remote home directory is not ours to name"
    );
    assert_eq!(
        request.shell, None,
        "the remote user's shell is not ours to pick"
    );
    assert!(request.environment_variables.is_empty());
}

// Rows and columns are adjacent `u32`s in `spawn_session`'s positional
// signature, so a transposition compiles, type-checks, and spawns a 80-row by
// 24-column pty. Asserted with deliberately unequal values, since equal ones
// would pass either way -- the same hazard `event_loop_tests.rs` pins for
// `ResizeSession`.
//
// Breaks if: the two defaults are swapped, or `for_host` stops supplying a
// plausible geometry at all (a zero-sized pty makes `vim` and `less` draw
// nothing, which reads as a hung session rather than a bad size).
#[test]
fn the_provisional_geometry_keeps_rows_and_columns_the_right_way_round() {
    let request = RemoteSessionSpawnRequest::for_host(host("build-box"));

    assert_eq!((request.rows, request.cols), (24, 80));
    assert_ne!(request.rows, request.cols);
}

// NOTE on what is deliberately NOT tested here, and why each is absence rather
// than an oversight.
//
// **Anything that sends an RPC.** `spawn_remote_session`, `spawn_and_settle`
// and the orphan cleanup all need an `Arc<RemoteServerClient>`, which has no
// constructor outside `crates/remote_server`'s own duplex-backed harness, and a
// real one needs a daemon on a real host. No live daemon is available here, so
// these are untestable at this layer rather than untested -- which is why every
// decision they make was extracted into the pure functions above.
//
// **`NO_BOOTSTRAP`.** It is a constant with no input, so a test could only
// restate the literal, and `script/check_stub_coverage` exists to stop exactly
// that. What would actually break if it were flipped -- a daemon-spawned bash
// emitting DCS hooks under a session id this client never registered, so
// `Processor::validate_hook_session_id` rejects every one of them -- is
// observable only against a live daemon. The argument is in the constant's doc
// comment, which is the only thing holding it.
//
// **`finish_spawn`'s witness step.** It needs an `AppContext` with a
// `RemoteServerManager` holding a *connected* client, which is unreachable
// without the real connect path; `terminal_manager_tests.rs` records the same
// limit for the same reason. The half that can be pinned without one --
// "nothing but `Spawned` reaches the witness" -- is pinned above.
