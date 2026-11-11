//! The remote read of the diff application keeps the file's own line endings (round 10, lab job
//! 17885063245: `lab_fam_agent_edit_endings` on the no-helper arm).

use std::sync::Arc;

use remote_server::proto::{FailedFileRead, FileOperationError};
use warpui::App;

use super::super::diff_application::{DiffApplicationError, FileReadResult, apply_edits_strict};
use super::{
    as_read_file_context_normalises, classify_failed_read, with_the_files_own_line_endings,
};
use crate::ai::agent::{AIIdentifiers, FileEdit};
use crate::ai::blocklist::SessionContext;
use crate::auth::AuthState;
use ai::diff_validation::AIRequestedCodeDiff;

const CRLF: &str = "L01 alpha\r\nL02 beta\r\nL03 gamma\r\n";
const MIXED: &str = "L01 alpha\r\nL02 beta\nL03 gamma\r\nL04 delta";

fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

#[test]
fn a_crlf_file_is_read_with_its_crlf_endings() {
    assert_eq!(
        with_the_files_own_line_endings(lf(CRLF), Some(CRLF.as_bytes().to_vec())),
        CRLF
    );
}

#[test]
fn a_mixed_ending_file_keeps_each_lines_own_ending_and_its_missing_final_newline() {
    assert_eq!(
        with_the_files_own_line_endings(lf(MIXED), Some(MIXED.as_bytes().to_vec())),
        MIXED
    );
}

#[test]
fn an_lf_file_is_the_normalised_text() {
    let text = "L01 alpha\nL02 beta\n".to_owned();
    assert_eq!(
        with_the_files_own_line_endings(text.clone(), Some(text.clone().into_bytes())),
        text
    );
}

#[test]
fn bytes_of_a_different_text_are_never_used() {
    // The file changed between the two reads: the edit is computed against what the model
    // read, and the write guard then refuses it.
    let normalised = lf(CRLF);
    let changed = "L01 alpha\r\nL02 CHANGED\r\nL03 gamma\r\n";
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), Some(changed.as_bytes().to_vec())),
        normalised
    );
    // A read that was cut short.
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), Some(CRLF.as_bytes()[..12].to_vec())),
        normalised
    );
}

#[test]
fn a_failed_or_non_utf8_raw_read_keeps_the_normalised_text() {
    let normalised = lf(CRLF);
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), None),
        normalised
    );
    assert_eq!(
        with_the_files_own_line_endings(normalised.clone(), Some(vec![0xff, b'\r', b'\n'])),
        normalised
    );
}

#[test]
fn the_normaliser_matches_read_file_context() {
    // One `\r` in front of a `\n` (or at the very end) goes; any other `\r` stays.
    for (raw, want) in [
        ("a\r\nb\r\n", "a\nb\n"),
        ("a\r\nb", "a\nb"),
        ("a\r\r\nb\n", "a\r\nb\n"),
        ("a\rb\n", "a\rb\n"),
        ("a\nb\r", "a\nb"),
        ("", ""),
        ("\r\n", "\n"),
    ] {
        assert_eq!(as_read_file_context_normalises(raw), want, "{raw:?}");
    }
    // A lone `\r` in the middle of a line is not a line ending for `ReadFileContext`, so such
    // a file still round-trips byte for byte.
    let odd = "a\rb\r\nc\r\n";
    assert_eq!(
        with_the_files_own_line_endings(
            as_read_file_context_normalises(odd),
            Some(odd.as_bytes().to_vec())
        ),
        odd
    );
}

fn failure(not_found: bool, message: &str) -> FailedFileRead {
    FailedFileRead {
        path: "/srv/new.txt".to_string(),
        error: Some(FileOperationError {
            message: message.to_string(),
        }),
        not_found,
    }
}

/// #2151: the typed flag decides, whatever the text says.
#[test]
fn the_typed_not_found_flag_is_not_found_whatever_the_text() {
    for text in ["File does not exist", "anything the backend wrote", ""] {
        assert!(matches!(
            classify_failed_read(failure(true, text)),
            FileReadResult::NotFound
        ));
    }
    let no_error = FailedFileRead {
        path: "/srv/new.txt".to_string(),
        error: None,
        not_found: true,
    };
    assert!(matches!(
        classify_failed_read(no_error),
        FileReadResult::NotFound
    ));
}

/// A server that predates the flag still says "File does not exist" (the shell backend's and
/// the helper's wording, `FILE_DOES_NOT_EXIST_MESSAGE`).
#[test]
fn a_legacy_server_without_the_flag_is_still_not_found_by_the_readers_own_wording() {
    assert!(matches!(
        classify_failed_read(failure(
            false,
            crate::ai::blocklist::FILE_DOES_NOT_EXIST_MESSAGE
        )),
        FileReadResult::NotFound
    ));
}

/// Real read errors stay read errors, so a create is refused when existence is unknown.
#[test]
fn a_real_read_error_is_never_not_found() {
    for text in [
        "Could not read file: Permission denied (os error 13)",
        "Could not read file: Is a directory (os error 21)",
        "the host's read command printed nothing usable (exit 1)",
        "Not read: this call's total size limit was reached.",
        "File not found in the index but the host failed",
    ] {
        match classify_failed_read(failure(false, text)) {
            FileReadResult::ReadError(message) => assert_eq!(message, text),
            _ => panic!("{text:?} must stay a read error"),
        }
    }
    assert!(matches!(
        classify_failed_read(FailedFileRead {
            path: String::new(),
            error: None,
            not_found: false,
        }),
        FileReadResult::ReadError(message) if message == "Unknown error"
    ));
}

async fn create_with_read(
    app: &App,
    read: FileReadResult,
) -> Result<Vec<AIRequestedCodeDiff>, vec1::Vec1<DiffApplicationError>> {
    let read = std::sync::Mutex::new(Some(read));
    apply_edits_strict(
        vec![FileEdit::Create {
            file: Some("/srv/new.txt".to_owned()),
            content: Some("hello".to_owned()),
        }],
        &SessionContext::new_for_test(),
        &AIIdentifiers::default(),
        app.background_executor(),
        Arc::new(AuthState::new_for_test()),
        false,
        move |_path| {
            let read = read.lock().unwrap().take().unwrap();
            async move { read }
        },
    )
    .await
}

/// #2151: the no-helper shell backend's "File does not exist" lets the agent create the file.
#[test]
fn a_shell_backend_not_found_lets_the_agent_create_the_file() {
    App::test((), |app| async move {
        let read = classify_failed_read(failure(true, "File does not exist"));
        let diffs = create_with_read(&app, read)
            .await
            .expect("a create of a missing file must succeed");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].file_name, "/srv/new.txt");
    });
}

/// A real read error still refuses the create.
#[test]
fn a_real_read_error_still_refuses_the_create() {
    App::test((), |app| async move {
        let read = classify_failed_read(failure(
            false,
            "Could not read file: Permission denied (os error 13)",
        ));
        let errors = create_with_read(&app, read)
            .await
            .expect_err("an unknown existence must refuse the create");
        assert!(matches!(
            &errors[..],
            [DiffApplicationError::ReadFailed { .. }]
        ));
    });
}
