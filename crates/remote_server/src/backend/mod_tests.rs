use super::*;
use crate::capability::{HELPER_CAPABILITIES, LEGACY_ONLY_CAPABILITIES};

#[test]
fn every_family_has_a_distinct_capability_that_round_trips() {
    let mut seen = std::collections::HashSet::new();
    for f in Family::ALL {
        assert!(seen.insert(f.capability()), "{f:?} reuses a capability");
        assert_eq!(Family::from_capability(f.capability()), Some(*f));
    }
    assert_eq!(Family::from_capability("nope"), None);
}

#[test]
fn every_known_capability_belongs_to_a_family() {
    for cap in HELPER_CAPABILITIES.iter().chain(LEGACY_ONLY_CAPABILITIES) {
        // A refinement of `file_write` (the request field `expect`), not a family of its own: the
        // router asks for it on top of the FileWrite decision.
        if *cap == crate::capability::FILE_WRITE_EXPECT
            || *cap == crate::capability::FILE_DELETE_EXPECT
        {
            continue;
        }
        assert!(
            Family::from_capability(cap).is_some(),
            "{cap} has no Family"
        );
    }
}

#[test]
fn error_classification() {
    let unsupported = BackendError::Unsupported {
        family: Family::GitWrite,
        what: "git_push",
    };
    assert!(unsupported.is_unsupported() && !unsupported.is_connection_lost());

    let server = BackendError::Client(ClientError::ServerError {
        code: ErrorCode::Unsupported,
        message: "x".into(),
    });
    assert!(server.is_unsupported());

    let other_server = BackendError::Client(ClientError::ServerError {
        code: ErrorCode::InvalidRequest,
        message: "x".into(),
    });
    assert!(!other_server.is_unsupported() && !other_server.is_connection_lost());

    assert!(BackendError::Client(ClientError::Disconnected).is_connection_lost());
    assert!(BackendError::Client(ClientError::ResponseChannelClosed).is_connection_lost());
    assert!(
        !BackendError::Client(ClientError::Timeout(std::time::Duration::from_secs(1)))
            .is_connection_lost()
    );
    assert!(!BackendError::Unavailable("x".into()).is_unsupported());
}
