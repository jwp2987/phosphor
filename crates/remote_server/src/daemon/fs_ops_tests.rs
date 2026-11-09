use crate::proto::{
    FileSystemEntryKind, create_directory_response, delete_file_response, list_directory_response,
    read_file_chunk_response, resolve_path_response, server_message, write_file_chunk_response,
    write_file_response,
};

use super::*;

fn p(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn list_directory_sorts_entries_and_reports_kinds() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.txt"), "12345").unwrap();
    std::fs::create_dir(dir.path().join("a_dir")).unwrap();
    let server_message::Message::ListDirectoryResponse(resp) = list_directory(ListDirectory {
        path: p(dir.path()),
    }) else {
        panic!("wrong response type");
    };
    let Some(list_directory_response::Result::Success(ok)) = resp.result else {
        panic!("expected success");
    };
    let names: Vec<_> = ok.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["a_dir", "b.txt"]);
    assert!(ok.entries[0].is_dir);
    assert_eq!(ok.entries[0].kind, FileSystemEntryKind::Directory as i32);
    assert_eq!(ok.entries[1].kind, FileSystemEntryKind::File as i32);
    assert_eq!(ok.entries[1].size_bytes, Some(5));
}

#[test]
fn list_directory_of_a_missing_path_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let server_message::Message::ListDirectoryResponse(resp) = list_directory(ListDirectory {
        path: p(&dir.path().join("nope")),
    }) else {
        panic!("wrong response type");
    };
    assert!(matches!(
        resp.result,
        Some(list_directory_response::Result::Error(_))
    ));
}

#[test]
fn resolve_path_reports_symlinks_without_following_them() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("t.txt");
    std::fs::write(&target, "x").unwrap();
    let link = dir.path().join("l");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let server_message::Message::ResolvePathResponse(resp) =
        resolve_path(ResolvePath { path: p(&link) })
    else {
        panic!("wrong response type");
    };
    let Some(resolve_path_response::Result::Success(ok)) = resp.result else {
        panic!("expected success");
    };
    assert_eq!(ok.kind, FileSystemEntryKind::Symlink as i32);
    assert_eq!(
        std::path::Path::new(&ok.canonical_path),
        target.canonicalize().unwrap()
    );
}

#[test]
fn create_directory_creates_nested_directories() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a/b/c");
    let server_message::Message::CreateDirectoryResponse(resp) =
        create_directory(CreateDirectory { path: p(&nested) })
    else {
        panic!("wrong response type");
    };
    assert!(matches!(
        resp.result,
        Some(create_directory_response::Result::Success(_))
    ));
    assert!(nested.is_dir());
}

#[test]
fn chunked_write_then_chunked_read_round_trips_and_reports_eof() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sub/f.bin");
    let write = |offset: u64, bytes: &[u8], truncate: bool| {
        let server_message::Message::WriteFileChunkResponse(resp) =
            write_file_chunk(WriteFileChunk {
                path: p(&file),
                offset,
                bytes: bytes.to_vec(),
                truncate,
                executable: None,
            })
        else {
            panic!("wrong response type");
        };
        let Some(write_file_chunk_response::Result::Success(ok)) = resp.result else {
            panic!("expected success");
        };
        ok.next_offset
    };
    assert_eq!(write(0, b"hello ", true), 6);
    assert_eq!(write(6, b"world", false), 11);

    let read = |offset: u64, max: u64| {
        let server_message::Message::ReadFileChunkResponse(resp) = read_file_chunk(ReadFileChunk {
            path: p(&file),
            offset,
            max_bytes: max,
        }) else {
            panic!("wrong response type");
        };
        let Some(read_file_chunk_response::Result::Success(ok)) = resp.result else {
            panic!("expected success");
        };
        ok
    };
    let first = read(0, 5);
    assert_eq!(first.bytes, b"hello");
    assert!(!first.eof);
    let rest = read(first.next_offset, 100);
    assert_eq!(rest.bytes, b" world");
    assert!(rest.eof);
    assert_eq!(rest.total_size, Some(11));
}

#[test]
fn write_file_creates_parents_and_delete_file_removes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x/y/z.txt");
    let server_message::Message::WriteFileResponse(resp) = write_file(WriteFile {
        path: p(&file),
        content: "content".to_string(),
    }) else {
        panic!("wrong response type");
    };
    assert!(matches!(
        resp.result,
        Some(write_file_response::Result::Success(_))
    ));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "content");

    let server_message::Message::DeleteFileResponse(resp) =
        delete_file(DeleteFile { path: p(&file) })
    else {
        panic!("wrong response type");
    };
    assert!(matches!(
        resp.result,
        Some(delete_file_response::Result::Success(_))
    ));
    assert!(!file.exists());

    let server_message::Message::DeleteFileResponse(resp) =
        delete_file(DeleteFile { path: p(&file) })
    else {
        panic!("wrong response type");
    };
    assert!(matches!(
        resp.result,
        Some(delete_file_response::Result::Error(_))
    ));
}
