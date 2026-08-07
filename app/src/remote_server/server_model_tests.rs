use std::collections::HashMap;

use std::fs;

use super::super::proto::{
    list_directory_response, read_file_chunk_response, resolve_path_response, server_message,
    write_file_chunk_response, Authenticate, CreateDirectory, Initialize, ListDirectory,
    ReadFileChunk, ResolvePath, WriteFileChunk,
};
use super::super::protocol::RequestId;
#[cfg(feature = "local_fs")]
use super::super::server_buffer_tracker::ServerBufferTracker;
use super::{PendingFileOps, ServerModel};

fn test_model() -> ServerModel {
    ServerModel {
        connection_senders: HashMap::new(),
        snapshot_sent_roots_by_connection: HashMap::new(),
        #[cfg(feature = "local_fs")]
        diff_state_subscriptions: HashMap::new(),
        grace_timer_cancel: None,
        in_progress: HashMap::new(),
        host_id: "test-host-id".to_string(),
        executors: HashMap::new(),
        pending_file_ops: PendingFileOps::new(),
        #[cfg(feature = "local_fs")]
        buffers: ServerBufferTracker::new(),
        auth_token: None,
    }
}

fn request_id() -> RequestId {
    RequestId::from("test-request".to_string())
}

#[test]
fn fresh_model_starts_without_auth_token() {
    let model = test_model();

    assert_eq!(model.auth_token(), None);
}

#[test]
fn initialize_with_auth_token_stores_token() {
    let mut model = test_model();

    model.handle_initialize(
        Initialize {
            auth_token: "initial-token".to_string(),
        },
        &request_id(),
    );

    assert_eq!(model.auth_token(), Some("initial-token"));
}

#[test]
fn empty_initialize_preserves_existing_auth_token() {
    let mut model = test_model();
    model.handle_initialize(
        Initialize {
            auth_token: "initial-token".to_string(),
        },
        &request_id(),
    );

    model.handle_initialize(
        Initialize {
            auth_token: String::new(),
        },
        &request_id(),
    );

    assert_eq!(model.auth_token(), Some("initial-token"));
}

#[test]
fn authenticate_with_auth_token_replaces_auth_token() {
    let mut model = test_model();
    model.handle_initialize(
        Initialize {
            auth_token: "initial-token".to_string(),
        },
        &request_id(),
    );

    model.handle_authenticate(Authenticate {
        auth_token: "rotated-token".to_string(),
    });

    assert_eq!(model.auth_token(), Some("rotated-token"));
}

#[test]
fn empty_authenticate_preserves_existing_auth_token() {
    let mut model = test_model();
    model.handle_initialize(
        Initialize {
            auth_token: "initial-token".to_string(),
        },
        &request_id(),
    );

    model.handle_authenticate(Authenticate {
        auth_token: String::new(),
    });

    assert_eq!(model.auth_token(), Some("initial-token"));
}

#[cfg(feature = "local_fs")]
#[test]
fn resolve_path_reports_file_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("note.txt");
    fs::write(&file_path, "hello").unwrap();
    let model = test_model();

    let response = model.handle_resolve_path(ResolvePath {
        path: file_path.to_string_lossy().to_string(),
    });

    let server_message::Message::ResolvePathResponse(response) = response.into_message() else {
        panic!("expected ResolvePathResponse");
    };
    let Some(resolve_path_response::Result::Success(success)) = response.result else {
        panic!("expected resolve path success");
    };
    assert_eq!(
        success.canonical_path,
        fs::canonicalize(&file_path).unwrap().to_string_lossy()
    );
    assert_eq!(success.kind, super::super::proto::FileSystemEntryKind::File as i32);
    assert_eq!(success.size_bytes, Some(5));
}

#[cfg(feature = "local_fs")]
#[test]
fn list_directory_returns_sorted_metadata() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("b.txt"), "b").unwrap();
    fs::create_dir(dir.path().join("a-dir")).unwrap();
    let model = test_model();

    let response = model.handle_list_directory(ListDirectory {
        path: dir.path().to_string_lossy().to_string(),
    });

    let server_message::Message::ListDirectoryResponse(response) = response.into_message() else {
        panic!("expected ListDirectoryResponse");
    };
    let Some(list_directory_response::Result::Success(success)) = response.result else {
        panic!("expected list directory success");
    };
    assert_eq!(
        success.canonical_path,
        fs::canonicalize(dir.path()).unwrap().to_string_lossy()
    );
    assert_eq!(success.entries.len(), 2);
    assert_eq!(success.entries[0].name, "a-dir");
    assert_eq!(
        success.entries[0].kind,
        super::super::proto::FileSystemEntryKind::Directory as i32
    );
    assert_eq!(success.entries[1].name, "b.txt");
    assert_eq!(
        success.entries[1].kind,
        super::super::proto::FileSystemEntryKind::File as i32
    );
    assert_eq!(success.entries[1].size_bytes, Some(1));
}

#[cfg(feature = "local_fs")]
#[test]
fn read_and_write_file_chunks_round_trip_binary_data() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("blob.bin");
    let model = test_model();

    let write_response = model.handle_write_file_chunk(WriteFileChunk {
        path: file_path.to_string_lossy().to_string(),
        offset: 0,
        bytes: vec![0, 1, 2, 3],
        truncate: true,
        executable: None,
    });
    let server_message::Message::WriteFileChunkResponse(write_response) =
        write_response.into_message()
    else {
        panic!("expected WriteFileChunkResponse");
    };
    let Some(write_file_chunk_response::Result::Success(write_success)) = write_response.result
    else {
        panic!("expected write chunk success");
    };
    assert_eq!(write_success.next_offset, 4);

    let read_response = model.handle_read_file_chunk(ReadFileChunk {
        path: file_path.to_string_lossy().to_string(),
        offset: 1,
        max_bytes: 2,
    });
    let server_message::Message::ReadFileChunkResponse(read_response) =
        read_response.into_message()
    else {
        panic!("expected ReadFileChunkResponse");
    };
    let Some(read_file_chunk_response::Result::Success(read_success)) = read_response.result else {
        panic!("expected read chunk success");
    };
    assert_eq!(read_success.bytes, vec![1, 2]);
    assert_eq!(read_success.next_offset, 3);
    assert_eq!(read_success.total_size, Some(4));
    assert!(!read_success.eof);
}

#[cfg(feature = "local_fs")]
#[test]
fn create_directory_creates_nested_directories() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a/b/c");
    let model = test_model();

    let response = model.handle_create_directory(CreateDirectory {
        path: nested.to_string_lossy().to_string(),
    });

    let server_message::Message::CreateDirectoryResponse(response) = response.into_message() else {
        panic!("expected CreateDirectoryResponse");
    };
    assert!(matches!(
        response.result,
        Some(super::super::proto::create_directory_response::Result::Success(_))
    ));
    assert!(nested.is_dir());
}

#[test]
fn commit_chain_mode_from_proto_maps_each_variant() {
    use super::super::proto::GitCommitChainMode;
    use crate::util::git::CommitChainMode;

    assert_eq!(
        ServerModel::commit_chain_mode_from_proto(GitCommitChainMode::CommitOnly),
        CommitChainMode::CommitOnly
    );
    assert_eq!(
        ServerModel::commit_chain_mode_from_proto(GitCommitChainMode::CommitAndPush),
        CommitChainMode::CommitAndPush
    );
    assert_eq!(
        ServerModel::commit_chain_mode_from_proto(GitCommitChainMode::CommitAndCreatePr),
        CommitChainMode::CommitAndCreatePr
    );
}

/// The daemon-side guard is the ONLY thing standing between a remote git
/// write-op and a repository that is mid-merge/rebase: the client-side
/// pre-emptive check (`is_git_operation_blocked`) probes the *client's*
/// filesystem, and `RemoteDiffStateModel` returns `false` unconditionally on
/// the stated promise that the daemon owns this. Mirrors Warp, which bails out
/// of all three mutating handlers on the same condition.
#[test]
fn guard_git_operation_in_progress_blocks_only_mid_operation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let repo = dir.path();
    std::fs::create_dir(repo.join(".git")).expect("create .git");

    // A quiescent repository is not blocked.
    assert!(super::guard_git_operation_in_progress(repo).is_ok());

    // An in-progress merge is, and the message names the states so the user can
    // act on it rather than seeing a raw git failure.
    std::fs::write(repo.join(".git").join("MERGE_HEAD"), "").expect("write MERGE_HEAD");
    let err = super::guard_git_operation_in_progress(repo)
        .expect_err("a mid-merge repository must block git write-ops");
    let message = format!("{err}");
    assert!(
        message.contains("another git operation is in progress"),
        "unexpected guard message: {message}"
    );

    // Completing the merge unblocks it; the guard is not sticky.
    std::fs::remove_file(repo.join(".git").join("MERGE_HEAD")).expect("remove MERGE_HEAD");
    assert!(super::guard_git_operation_in_progress(repo).is_ok());
}

/// A held `index.lock` means another git process is mutating the index right
/// now; committing into that races it.
#[test]
fn guard_git_operation_in_progress_blocks_on_held_index_lock() {
    let dir = tempfile::tempdir().expect("temp dir");
    let repo = dir.path();
    std::fs::create_dir(repo.join(".git")).expect("create .git");
    std::fs::write(repo.join(".git").join("index.lock"), "").expect("write index.lock");

    assert!(
        super::guard_git_operation_in_progress(repo).is_err(),
        "a held index.lock must block git write-ops"
    );
}
