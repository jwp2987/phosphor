//! Filesystem requests of the helper: directory listing, path resolution, directory creation,
//! chunked reads and writes, and whole-file write and delete. All blocking and short; the
//! server runs them on the core thread except where noted.
//!
//! Behaviour matches the app daemon's handlers (`app/src/remote_server/server_model.rs`) that
//! these were ported from, so a client cannot tell the two servers apart for these requests.

use std::path::PathBuf;

use crate::naming::expand_tilde;
use crate::proto::{
    CreateDirectory, CreateDirectoryResponse, CreateDirectorySuccess, DeleteFile,
    DeleteFileResponse, DeleteFileSuccess, DirEntry, FileOperationError, FileSystemEntryKind,
    ListDirectory, ListDirectoryResponse, ListDirectorySuccess, ReadFileChunk,
    ReadFileChunkResponse, ReadFileChunkSuccess, ResolvePath, ResolvePathResponse,
    ResolvePathSuccess, WriteFile, WriteFileChunk, WriteFileChunkResponse, WriteFileChunkSuccess,
    WriteFileResponse, WriteFileSuccess, create_directory_response, delete_file_response,
    list_directory_response, read_file_chunk_response, resolve_path_response, server_message,
    write_file_chunk_response, write_file_response,
};

/// Largest chunk one `ReadFileChunk` returns, whatever the client asked for.
const MAX_READ_CHUNK_BYTES: u64 = 8 * 1024 * 1024;

fn entry_kind(file_type: Option<&std::fs::FileType>, metadata: Option<&std::fs::Metadata>) -> i32 {
    if file_type.is_some_and(|ft| ft.is_symlink()) {
        return FileSystemEntryKind::Symlink as i32;
    }
    if metadata.is_some_and(|metadata| metadata.is_dir()) {
        return FileSystemEntryKind::Directory as i32;
    }
    if metadata.is_some_and(|metadata| metadata.is_file()) {
        return FileSystemEntryKind::File as i32;
    }
    FileSystemEntryKind::Other as i32
}

fn system_time_to_epoch_millis(time: std::time::SystemTime) -> Option<u64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn canonical_string(path: PathBuf) -> String {
    path.canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

pub(super) fn list_directory(msg: ListDirectory) -> server_message::Message {
    log::info!("Handling ListDirectory path={}", msg.path);
    let path = expand_tilde(&msg.path);
    let result = match std::fs::read_dir(&path) {
        Ok(read_dir) => {
            let mut entries = Vec::new();
            for entry in read_dir.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                // Prefer `file_type()` (does not follow symlinks, no extra stat); the metadata
                // call does follow them.
                let file_type = entry.file_type().ok();
                let metadata = entry.metadata().ok();
                let kind = entry_kind(file_type.as_ref(), metadata.as_ref());
                let is_dir = kind == FileSystemEntryKind::Directory as i32;
                let size_bytes = metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len());
                let modified_epoch_millis = metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(system_time_to_epoch_millis);
                entries.push(DirEntry {
                    name,
                    is_dir,
                    kind,
                    size_bytes,
                    modified_epoch_millis,
                });
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            list_directory_response::Result::Success(ListDirectorySuccess {
                entries,
                canonical_path: canonical_string(path),
            })
        }
        Err(err) => list_directory_response::Result::Error(FileOperationError {
            message: format!("Failed to list directory {}: {err}", msg.path),
        }),
    };
    server_message::Message::ListDirectoryResponse(ListDirectoryResponse {
        result: Some(result),
    })
}

pub(super) fn resolve_path(msg: ResolvePath) -> server_message::Message {
    let path = expand_tilde(&msg.path);
    let result = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            let file_type = metadata.file_type();
            let kind = entry_kind(Some(&file_type), Some(&metadata));
            resolve_path_response::Result::Success(ResolvePathSuccess {
                canonical_path: canonical_string(path),
                kind,
                size_bytes: metadata.is_file().then_some(metadata.len()),
            })
        }
        Err(err) => resolve_path_response::Result::Error(FileOperationError {
            message: format!("Failed to resolve path {}: {err}", msg.path),
        }),
    };
    server_message::Message::ResolvePathResponse(ResolvePathResponse {
        result: Some(result),
    })
}

pub(super) fn create_directory(msg: CreateDirectory) -> server_message::Message {
    let path = expand_tilde(&msg.path);
    let result = match std::fs::create_dir_all(&path) {
        Ok(()) => create_directory_response::Result::Success(CreateDirectorySuccess {
            canonical_path: canonical_string(path),
        }),
        Err(err) => create_directory_response::Result::Error(FileOperationError {
            message: format!("Failed to create directory {}: {err}", msg.path),
        }),
    };
    server_message::Message::CreateDirectoryResponse(CreateDirectoryResponse {
        result: Some(result),
    })
}

pub(super) fn read_file_chunk(msg: ReadFileChunk) -> server_message::Message {
    use std::io::{Read, Seek, SeekFrom};

    let path = expand_tilde(&msg.path);
    let result = (|| -> std::io::Result<ReadFileChunkSuccess> {
        let mut file = std::fs::File::open(&path)?;
        let total_size = file.metadata().ok().map(|m| m.len());
        file.seek(SeekFrom::Start(msg.offset))?;
        let max_bytes = msg.max_bytes.min(MAX_READ_CHUNK_BYTES) as usize;
        let mut bytes = vec![0; max_bytes];
        let read = file.read(&mut bytes)?;
        bytes.truncate(read);
        let next_offset = msg.offset + read as u64;
        let eof = total_size.is_some_and(|size| next_offset >= size) || read == 0;
        Ok(ReadFileChunkSuccess {
            bytes,
            next_offset,
            total_size,
            eof,
        })
    })();
    let result = match result {
        Ok(success) => read_file_chunk_response::Result::Success(success),
        Err(err) => read_file_chunk_response::Result::Error(FileOperationError {
            message: format!("Failed to read file chunk {}: {err}", msg.path),
        }),
    };
    server_message::Message::ReadFileChunkResponse(ReadFileChunkResponse {
        result: Some(result),
    })
}

pub(super) fn write_file_chunk(msg: WriteFileChunk) -> server_message::Message {
    use std::io::{Seek, SeekFrom, Write};

    let path = expand_tilde(&msg.path);
    let result = (|| -> std::io::Result<WriteFileChunkSuccess> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).write(true);
        if msg.truncate {
            options.truncate(true);
        }
        let mut file = options.open(&path)?;
        file.seek(SeekFrom::Start(msg.offset))?;
        file.write_all(&msg.bytes)?;
        if let Some(executable) = msg.executable {
            use std::os::unix::fs::PermissionsExt;
            let mode = if executable { 0o755 } else { 0o644 };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
        }
        Ok(WriteFileChunkSuccess {
            next_offset: msg.offset + msg.bytes.len() as u64,
        })
    })();
    let result = match result {
        Ok(success) => write_file_chunk_response::Result::Success(success),
        Err(err) => write_file_chunk_response::Result::Error(FileOperationError {
            message: format!("Failed to write file chunk {}: {err}", msg.path),
        }),
    };
    server_message::Message::WriteFileChunkResponse(WriteFileChunkResponse {
        result: Some(result),
    })
}

/// Whether the file at `path` is in the state `expect` names (#1837). An unreadable file, an
/// unreadable-as-bytes file or an empty `WriteExpect` do not match: the write is refused.
fn write_expectation_holds(path: &std::path::Path, expect: &crate::proto::WriteExpect) -> bool {
    use crate::proto::write_expect::State;
    match &expect.state {
        Some(State::Absent(true)) => {
            std::fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        }
        Some(State::Sha256(want)) => std::fs::read(path)
            .map(|bytes| crate::index::sha256_hex(&bytes).eq_ignore_ascii_case(want.trim()))
            .unwrap_or(false),
        _ => false,
    }
}

/// `WriteFile`: write the whole content, creating parent directories. The path is used as sent
/// (no `~` expansion), like the app daemon's `FileModel`-backed handler.
pub(super) fn write_file(msg: WriteFile) -> server_message::Message {
    log::info!("Handling WriteFile path={}", msg.path);
    let path = PathBuf::from(&msg.path);
    if let Some(expect) = &msg.expect {
        if !write_expectation_holds(&path, expect) {
            return server_message::Message::WriteFileResponse(WriteFileResponse {
                result: Some(write_file_response::Result::Error(FileOperationError {
                    message: crate::shellcmd::write::write_failure_message(
                        &msg.path,
                        crate::shellcmd::write::EXIT_CONFLICT,
                        "",
                    ),
                })),
            });
        }
    }
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&path, msg.content.as_bytes())
    })();
    let result = match result {
        Ok(()) => write_file_response::Result::Success(WriteFileSuccess {}),
        Err(err) => write_file_response::Result::Error(FileOperationError {
            message: format!("Failed to write {}: {err}", msg.path),
        }),
    };
    server_message::Message::WriteFileResponse(WriteFileResponse {
        result: Some(result),
    })
}

pub(super) fn delete_file(msg: DeleteFile) -> server_message::Message {
    log::info!("Handling DeleteFile path={}", msg.path);
    let result = match std::fs::remove_file(&msg.path) {
        Ok(()) => delete_file_response::Result::Success(DeleteFileSuccess {}),
        Err(err) => delete_file_response::Result::Error(FileOperationError {
            message: format!("Failed to delete {}: {err}", msg.path),
        }),
    };
    server_message::Message::DeleteFileResponse(DeleteFileResponse {
        result: Some(result),
    })
}

#[cfg(test)]
#[path = "fs_ops_tests.rs"]
mod tests;
