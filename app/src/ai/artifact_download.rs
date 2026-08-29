use std::path::Path;

pub(crate) fn sanitized_basename(path_or_filename: &str) -> Option<String> {
    let file_name = Path::new(path_or_filename).file_name()?.to_str()?;
    if file_name.is_empty() {
        return None;
    }
    Some(file_name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_basename_accepts_plain_filename() {
        assert_eq!(
            sanitized_basename("report.txt"),
            Some("report.txt".to_string())
        );
    }

    #[test]
    fn sanitized_basename_extracts_from_path() {
        assert_eq!(
            sanitized_basename("outputs/report.txt"),
            Some("report.txt".to_string())
        );
    }

    /// Ported from the pin's `e2e_traversal_in_filename_cannot_escape_the_handoff_dir`
    /// (upstream 4111d08f9, `app/src/ai/agent_sdk/driver/attachments_tests.rs`).
    ///
    /// Adapted: the pinned test drives the whole handoff-snapshot download
    /// pipeline (`fetch_and_download_handoff_snapshot_attachments`) against a
    /// `mockito` server and `crate::server::server_api::ai::MockAIClient`. None
    /// of that exists here — `driver/attachments.rs` was never ported and
    /// `app/src/server/server_api/` is absent — but the *security property* the
    /// test exists for is enforced by this fork's `sanitized_basename`, which is
    /// byte-identical to the pin's and is the same reduction the pin applies
    /// before joining a server-supplied name onto the handoff directory. So the
    /// assertion is made directly against the guard: a name carrying traversal
    /// segments reduces to a bare basename, which can only ever be joined
    /// *inside* the target directory.
    #[test]
    fn sanitized_basename_strips_traversal_segments() {
        assert_eq!(
            sanitized_basename("../escape.json"),
            Some("escape.json".to_string())
        );
        assert_eq!(
            sanitized_basename("../../../../etc/passwd"),
            Some("passwd".to_string())
        );
        assert_eq!(
            sanitized_basename("/etc/passwd"),
            Some("passwd".to_string())
        );
        // The reduced name has no separator and is not a traversal token, so a
        // caller joining it onto a directory cannot land outside that directory.
        for raw in ["../escape.json", "../../../../etc/passwd", "/etc/passwd"] {
            let safe = sanitized_basename(raw).expect("these all have a basename");
            assert!(!safe.contains('/'), "{safe:?} still carries a separator");
            assert_ne!(safe, "..", "{raw:?} reduced to a traversal token");
        }
    }

    /// Ported from the pin's `e2e_filename_without_a_basename_is_rejected_before_downloading`
    /// (upstream 4111d08f9, `app/src/ai/agent_sdk/driver/attachments_tests.rs`).
    ///
    /// Adapted for the same reason as the sibling above: the pinned test asserts
    /// that a name with no basename fails the guard *before* any request is sent
    /// (its mock is registered with `.expect(0)`). This fork has no download
    /// pipeline to short-circuit, so the assertion is made against the guard's
    /// own contract — `None`, which every call site must treat as a refusal
    /// rather than a path.
    #[test]
    fn sanitized_basename_rejects_names_without_a_basename() {
        assert_eq!(sanitized_basename(".."), None);
        assert_eq!(sanitized_basename("../.."), None);
        assert_eq!(sanitized_basename("foo/.."), None);
        assert_eq!(sanitized_basename("/"), None);
        assert_eq!(sanitized_basename(""), None);
    }
}
