use super::*;
use crate::proto::{
    delete_file_response, discard_files_response, save_buffer_response, server_message,
    write_file_response, DeleteFileResponse, DeleteFileSuccess, DiscardFilesError,
    DiscardFilesResponse, DiscardFilesSuccess, FileOperationError, SaveBufferResponse,
    SaveBufferSuccess, ServerMessage, WriteFileResponse, WriteFileSuccess,
};

fn msg(inner: server_message::Message) -> ServerMessage {
    ServerMessage {
        request_id: "req-1".to_string(),
        message: Some(inner),
    }
}

#[test]
fn write_file_success_is_ok_empty_is_err() {
    let success = msg(server_message::Message::WriteFileResponse(
        WriteFileResponse {
            result: Some(write_file_response::Result::Success(WriteFileSuccess {})),
        },
    ));
    assert!(write_file_result(&success).is_ok());

    let empty = msg(server_message::Message::WriteFileResponse(
        WriteFileResponse { result: None },
    ));
    assert!(write_file_result(&empty).is_err());
}

#[test]
fn write_file_error_propagates_message() {
    let err = msg(server_message::Message::WriteFileResponse(
        WriteFileResponse {
            result: Some(write_file_response::Result::Error(FileOperationError {
                message: "disk full".to_string(),
            })),
        },
    ));
    assert_eq!(write_file_result(&err).unwrap_err(), "disk full");
}

#[test]
fn write_file_wrong_variant_is_err() {
    let wrong = msg(server_message::Message::SaveBufferResponse(
        SaveBufferResponse {
            result: Some(save_buffer_response::Result::Success(SaveBufferSuccess {})),
        },
    ));
    assert!(write_file_result(&wrong).is_err());
}

#[test]
fn save_buffer_success_is_ok_empty_is_err() {
    let success = msg(server_message::Message::SaveBufferResponse(
        SaveBufferResponse {
            result: Some(save_buffer_response::Result::Success(SaveBufferSuccess {})),
        },
    ));
    assert!(save_buffer_result(&success).is_ok());

    let empty = msg(server_message::Message::SaveBufferResponse(
        SaveBufferResponse { result: None },
    ));
    assert!(save_buffer_result(&empty).is_err());
}

#[test]
fn save_buffer_error_propagates_message() {
    let err = msg(server_message::Message::SaveBufferResponse(
        SaveBufferResponse {
            result: Some(save_buffer_response::Result::Error(FileOperationError {
                message: "permission denied".to_string(),
            })),
        },
    ));
    assert_eq!(save_buffer_result(&err).unwrap_err(), "permission denied");
}

#[test]
fn delete_file_success_is_ok_empty_is_err() {
    let success = msg(server_message::Message::DeleteFileResponse(
        DeleteFileResponse {
            result: Some(delete_file_response::Result::Success(DeleteFileSuccess {})),
        },
    ));
    assert!(delete_file_result(&success).is_ok());

    let empty = msg(server_message::Message::DeleteFileResponse(
        DeleteFileResponse { result: None },
    ));
    assert!(delete_file_result(&empty).is_err());
}

#[test]
fn delete_file_error_propagates_message() {
    let err = msg(server_message::Message::DeleteFileResponse(
        DeleteFileResponse {
            result: Some(delete_file_response::Result::Error(FileOperationError {
                message: "no such file".to_string(),
            })),
        },
    ));
    assert_eq!(delete_file_result(&err).unwrap_err(), "no such file");
}

#[test]
fn discard_files_success_is_ok() {
    let success = msg(server_message::Message::DiscardFilesResponse(
        DiscardFilesResponse {
            result: Some(discard_files_response::Result::Success(
                DiscardFilesSuccess {},
            )),
        },
    ));
    assert!(discard_files_result(&success).is_ok());
}

#[test]
fn discard_files_error_propagates_message() {
    let err = msg(server_message::Message::DiscardFilesResponse(
        DiscardFilesResponse {
            result: Some(discard_files_response::Result::Error(DiscardFilesError {
                message: "merge conflict".to_string(),
            })),
        },
    ));
    assert_eq!(discard_files_result(&err).unwrap_err(), "merge conflict");
}

#[test]
fn discard_files_empty_result_is_err() {
    let empty = msg(server_message::Message::DiscardFilesResponse(
        DiscardFilesResponse { result: None },
    ));
    assert!(discard_files_result(&empty).is_err());
}

/// Guard: every host-scoped request variant must have an explicit, intentional
/// response disposition. This match is exhaustive, so adding a new
/// `host_scoped_request::Message` variant fails to compile until it is
/// classified here — a prompt to add a `host_response` parser (or document why
/// the response is parsed elsewhere).
///
/// Unlike the pinned oracle, this fork has no `manager` module for
/// interpreting the richer per-op responses (`GetBranches`, `GitPush`, etc.):
/// `RemoteServerClient` methods in `client/mod.rs` unwrap the response
/// envelope themselves and return the typed payload directly to the caller,
/// so most variants are classified as `client::<method>` rather than
/// `manager::<fn>`. `SaveBuffer` is the one case where even the client
/// forwards the raw `SaveBufferResponse` unconditionally — the
/// success/error split happens at the caller
/// (`app::code::global_buffer_model`) — even though a `host_response`
/// helper mirroring that split exists for isolated testing.
#[test]
fn every_host_scoped_request_has_a_response_disposition() {
    use crate::proto::host_scoped_request::Message as M;

    fn disposition(m: &M) -> &'static str {
        match m {
            // Parsed via the helpers in this module (and mirrored inline at
            // the matching `client::` call site).
            M::WriteFile(_) => "host_response::write_file_result",
            M::DeleteFile(_) => "host_response::delete_file_result",
            M::DiscardFiles(_) => "host_response::discard_files_result",
            // Full response returned to the caller for richer interpretation.
            M::ReadFileContext(_) => "client::read_file_context",
            M::SaveBuffer(_) => "client::save_buffer (caller parses result)",
            M::ResolveConflict(_) => "client::resolve_conflict",
            M::GetBranches(_) => "client::get_branches",
            M::GitCommitChain(_) => "client::git_commit_chain",
            M::GitPush(_) => "client::git_push",
            M::GitCreatePr(_) => "client::git_create_pr",
            M::GetCommittedBranchFiles(_) => "client::get_committed_branch_files",
            M::RipgrepSearch(_) => "client::ripgrep_search",
            M::ListDirectory(_) => "client::list_directory",
            M::ResolvePath(_) => "client::resolve_path",
            M::CreateDirectory(_) => "client::create_directory",
            M::ReadFileChunk(_) => "client::read_file_chunk",
            M::WriteFileChunk(_) => "client::write_file_chunk",
            // Codebase indexing (Delta D2, remote-daemon leg). The three
            // mutations are deliberately fire-and-forget: the daemon reports
            // what happened with `CodebaseIndexStatusUpdated` /
            // `CodebaseIndexStatusesSnapshot` pushes, which
            // `RemoteCodebaseIndexModel` consumes, rather than answering the
            // request. So there is no `host_response` parser to add for them —
            // an index run outlives any one request/response pair.
            M::IndexCodebase(_) => "manager::mutate_codebase_index (status via push)",
            M::ResyncCodebase(_) => "manager::resync_codebase (status via push)",
            M::DropCodebaseIndex(_) => "manager::drop_codebase_index (status via push)",
            // The codebase-index requests that answer directly.
            M::GetFragmentMetadataFromHash(_) => "manager::get_fragment_metadata_from_hash",
            M::SearchRemoteCodebase(_) => "manager::search_remote_codebase",
        }
    }

    // Referenced so the exhaustive match is compiled and checked.
    let _ = disposition;
}
