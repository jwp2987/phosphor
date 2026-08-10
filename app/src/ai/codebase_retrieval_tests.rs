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
