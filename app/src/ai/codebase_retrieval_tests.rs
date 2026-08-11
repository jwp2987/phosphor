//! Tests for the codebase-index retrieval lifecycle's user-visible contract.
//!
//! The lifecycle itself (start / supersede / resolve) needs a registered
//! `CodebaseIndexManager` and a running app, so it is exercised at the integration
//! level. What is locked here is the part that decides whether a user sees noise: how
//! every way retrieval can decline to answer is classified and named. Those states are
//! not failures in a fork where the user brings the provider — the default
//! configuration reaches them on every query.

use ai::index::full_source_code_embedding::manager::RetrieveFileError;

use super::*;

#[test]
fn every_failure_has_a_distinct_stable_status_token() {
    let cases = [
        (RetrievalFailure::NoIndex, "no_index"),
        (RetrievalFailure::Syncing, "index_syncing"),
        (
            RetrievalFailure::IndexUnavailable("no provider".to_owned()),
            "index_unavailable",
        ),
        (
            RetrievalFailure::Failed("boom".to_owned()),
            "retrieval_failed",
        ),
        (RetrievalFailure::Superseded, "superseded"),
        (RetrievalFailure::Unavailable, "unavailable"),
        (RetrievalFailure::HostUnreachable, "host_unreachable"),
    ];

    for (failure, expected) in &cases {
        assert_eq!(failure.status(), *expected);
    }

    // Distinct, because the model is shown these and "the index is still building" and
    // "you have not configured an embedding provider" call for different responses.
    let tokens: std::collections::HashSet<&str> =
        cases.iter().map(|(failure, _)| failure.status()).collect();
    assert_eq!(tokens.len(), cases.len(), "status tokens must be distinct");

    // None of them is "error": none of them is a fault.
    assert!(!tokens.contains("error"));
}

#[test]
fn a_missing_index_is_reported_as_absent_not_broken() {
    assert_eq!(
        RetrievalFailure::from(RetrieveFileError::IndexNotFound),
        RetrievalFailure::NoIndex
    );
}

#[test]
fn an_index_still_building_is_reported_as_syncing() {
    assert_eq!(
        RetrievalFailure::from(RetrieveFileError::IndexSyncing),
        RetrievalFailure::Syncing
    );
}

// ── Remote leg (TODO.md "UNWIRED-CODE AUDIT 2026-08-10" finding #5) ────────
//
// `CodebaseRetrievalHandle::Remote::retrieve` itself needs a live `HostRequestHandle`
// talking to a daemon, so it is exercised at the integration level (and its
// transport-failure path, "no connected session", end to end in
// `crates/remote_server/src/manager_tests.rs::
// search_remote_codebase_without_connected_host_resolves_immediately`). What's locked
// here is the wire-error classification -- it must mirror `From<RetrieveFileError>`
// above exactly, since the daemon computed its error code from the same
// `RetrieveFileError` in the first place (see
// `app::remote_server::server_model::search_remote_codebase_error_response_from_error`).
#[cfg(not(target_family = "wasm"))]
mod remote_leg {
    use remote_server::proto::{RemoteCodebaseSearchError, RemoteCodebaseSearchErrorCode};

    use super::*;

    fn error(code: RemoteCodebaseSearchErrorCode, message: &str) -> RemoteCodebaseSearchError {
        RemoteCodebaseSearchError {
            code: code as i32,
            message: message.to_owned(),
        }
    }

    #[test]
    fn remote_error_classification_mirrors_the_local_retrieve_file_error_mapping() {
        let cases = [
            (
                RemoteCodebaseSearchErrorCode::IndexNotFound,
                RetrievalFailure::NoIndex,
            ),
            (
                RemoteCodebaseSearchErrorCode::IndexSyncing,
                RetrievalFailure::Syncing,
            ),
            (
                RemoteCodebaseSearchErrorCode::IndexFailed,
                RetrievalFailure::IndexUnavailable("boom".to_owned()),
            ),
            (
                RemoteCodebaseSearchErrorCode::NotEnabled,
                RetrievalFailure::IndexUnavailable("boom".to_owned()),
            ),
        ];
        for (code, expected) in cases {
            assert_eq!(RetrievalFailure::from(error(code, "boom")), expected);
        }
    }

    #[test]
    fn a_retrieval_that_failed_after_starting_is_reported_as_a_failure_not_an_index_state() {
        // Distinct from `IndexFailed`/`NotEnabled` above: the index *was* ready enough
        // to accept the query, and something went wrong after that -- so this must not
        // be folded into `IndexUnavailable`, which means "nothing was even attempted."
        assert_eq!(
            RetrievalFailure::from(error(
                RemoteCodebaseSearchErrorCode::RetrievalFailed,
                "embedding call timed out"
            )),
            RetrievalFailure::Failed("embedding call timed out".to_owned())
        );
    }

    #[test]
    fn an_unrecognized_or_invalid_path_error_degrades_to_a_generic_failure_not_a_panic() {
        for code in [
            RemoteCodebaseSearchErrorCode::InvalidRepoPath,
            RemoteCodebaseSearchErrorCode::Unspecified,
        ] {
            assert_eq!(
                RetrievalFailure::from(error(code, "bad")),
                RetrievalFailure::Failed("bad".to_owned())
            );
        }
    }

    #[test]
    fn host_unreachable_is_not_reachable_via_the_wire_error_mapping() {
        // `HostUnreachable` comes only from a transport-layer `HostRequestError`
        // (`CodebaseRetrievalHandle::Remote::retrieve`'s `Err(_) =>` arm), never from a
        // `RemoteCodebaseSearchError` the daemon actually answered with -- a daemon that
        // replied is, by definition, reachable.
        for code in [
            RemoteCodebaseSearchErrorCode::NotEnabled,
            RemoteCodebaseSearchErrorCode::InvalidRepoPath,
            RemoteCodebaseSearchErrorCode::IndexNotFound,
            RemoteCodebaseSearchErrorCode::IndexSyncing,
            RemoteCodebaseSearchErrorCode::IndexFailed,
            RemoteCodebaseSearchErrorCode::RetrievalFailed,
            RemoteCodebaseSearchErrorCode::Unspecified,
        ] {
            assert_ne!(
                RetrievalFailure::from(error(code, "x")),
                RetrievalFailure::HostUnreachable
            );
        }
    }
}
