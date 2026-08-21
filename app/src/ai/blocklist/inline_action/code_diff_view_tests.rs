use std::io;
use std::path::PathBuf;

use super::*;

/// The exact shape of a refusal from the pre-write conflict check in
/// `warp_files::FileModel::save_if_unchanged`: a complete user-facing sentence
/// that names the file, states that nothing was written, and says the user's
/// own edits survived. Every such refusal arrives as `FileSaveError::Other`.
fn conflict_refusal(path: &str) -> FileSaveError {
    FileSaveError::Other(format!(
        "{path} changed on disk after this change was proposed, so it was not overwritten \
         and your edits are intact. Re-run the request to work from the current file."
    ))
}

/// Regression: the toast used to be a fixed `Failed to save file {path}`, with
/// `error` reaching only the log line. The conflict refusal — the entire
/// user-facing product of the guarded write — never reached the user, who could
/// not tell a refused overwrite (their edits are safe, re-run the request) from
/// a failed write (something is wrong with the disk).
#[test]
fn toast_message_carries_the_conflict_refusal_verbatim() {
    let error = conflict_refusal("/work/src/main.rs");

    let message = save_failure_toast_message("/work/src/main.rs", &error);

    assert_eq!(
        message,
        "/work/src/main.rs changed on disk after this change was proposed, so it was not \
         overwritten and your edits are intact. Re-run the request to work from the current file."
    );
    // The refusal already names the file; it must not be prefixed with the
    // same path a second time.
    assert!(!message.starts_with("Failed to save file"));
}

/// The other `Other` producer: `InlineDiffView::expected_disk_state` refuses
/// when the diff's base content is gone, so there is nothing to compare
/// against. Same requirement — the reason has to survive.
#[test]
fn toast_message_carries_any_other_reason_verbatim() {
    let error = FileSaveError::Other(
        "/work/src/lib.rs was not written: the original contents this edit was based on are \
         no longer available. Nothing was changed."
            .to_owned(),
    );

    let message = save_failure_toast_message("/work/src/lib.rs", &error);

    assert!(
        message.contains("the original contents this edit was based on are no longer available"),
        "reason dropped from toast: {message}"
    );
}

/// `FileSaveError::IOError`'s own `Display` is the constant "IO error when
/// saving file." and drops both the path and the underlying cause, so this
/// variant is the one that needs wrapping. The wrapper is localized.
#[test]
fn toast_message_wraps_io_errors_with_file_and_cause() {
    crate::i18n::init(Some("en"));
    let error = FileSaveError::IOError {
        error: io::Error::new(io::ErrorKind::PermissionDenied, "permission denied"),
        path: PathBuf::from("/work/src/main.rs"),
    };

    let message = save_failure_toast_message("/work/src/main.rs", &error);

    assert_eq!(
        message,
        "Failed to save file /work/src/main.rs: permission denied"
    );
}
