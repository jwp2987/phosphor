//! SFTP backend operation abstraction layer
//!
//! Defines the SftpBackend trait, decoupling the UI layer from the protocol
//! layer. LiveSftpBackend delegates to a real SFTP connection;
//! InMemorySftpBackend uses the local filesystem for testing.
//! author: logic
//! date: 2026-05-30

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use dunce;

use super::sftp_ops::{self, ProgressCallback, SftpOpsError};
use super::types::{FileEntry, FileEntryType};

/// SFTP backend operation abstraction, used to decouple the UI layer from the protocol layer
pub trait SftpBackend: Send + Sync {
    /// Lists directory contents, returning a list of file entries
    fn list_dir(&self, path: &Path) -> Result<Vec<FileEntry>, SftpOpsError>;

    /// Deletes a remote file
    fn delete_file(&self, path: &Path) -> Result<(), SftpOpsError>;

    /// Recursively deletes a remote directory
    fn delete_dir_recursive(&self, path: &Path) -> Result<(), SftpOpsError>;

    /// Creates a remote directory
    fn create_dir(&self, path: &Path) -> Result<(), SftpOpsError>;

    /// Renames a remote file or directory
    fn rename(&self, old_path: &Path, new_path: &Path) -> Result<(), SftpOpsError>;

    /// Resolves the real path
    fn realpath(&self, path: &Path) -> Result<PathBuf, SftpOpsError>;

    /// Gets file/directory details
    fn stat(&self, path: &Path) -> Result<FileEntry, SftpOpsError>;

    /// Streams a local file up to the remote side
    fn upload_file(
        &self,
        local_path: &Path,
        remote_path: &Path,
        progress_cb: Option<&ProgressCallback>,
        cancel_flag: Option<&AtomicBool>,
    ) -> Result<(), SftpOpsError>;

    /// Streams a remote file down to the local side
    fn download_file(
        &self,
        remote_path: &Path,
        local_path: &Path,
        progress_cb: Option<&ProgressCallback>,
        cancel_flag: Option<&AtomicBool>,
    ) -> Result<(), SftpOpsError>;
}

// ============================================================
// LiveSftpBackend — delegates to a real SFTP connection
// ============================================================

/// Real SFTP backend, wrapping zap_sftp::Sftp
pub struct LiveSftpBackend {
    sftp: zap_sftp::Sftp,
}

impl LiveSftpBackend {
    /// Creates a backend from an Sftp instance
    pub fn new(sftp: zap_sftp::Sftp) -> Self {
        Self { sftp }
    }

    /// Gets the inner Sftp reference (used for the realpath call in connect_to_server)
    pub fn inner(&self) -> &zap_sftp::Sftp {
        &self.sftp
    }
}

impl SftpBackend for LiveSftpBackend {
    fn list_dir(&self, path: &Path) -> Result<Vec<FileEntry>, SftpOpsError> {
        sftp_ops::list_dir(&self.sftp, path)
    }

    fn delete_file(&self, path: &Path) -> Result<(), SftpOpsError> {
        sftp_ops::delete_file(&self.sftp, path)
    }

    fn delete_dir_recursive(&self, path: &Path) -> Result<(), SftpOpsError> {
        sftp_ops::delete_dir_recursive(&self.sftp, path)
    }

    fn create_dir(&self, path: &Path) -> Result<(), SftpOpsError> {
        sftp_ops::create_dir(&self.sftp, path)
    }

    fn rename(&self, old_path: &Path, new_path: &Path) -> Result<(), SftpOpsError> {
        sftp_ops::rename(&self.sftp, old_path, new_path)
    }

    fn realpath(&self, path: &Path) -> Result<PathBuf, SftpOpsError> {
        self.sftp.realpath(path).map_err(|e| SftpOpsError::Operation(e.to_string()))
    }

    fn stat(&self, path: &Path) -> Result<FileEntry, SftpOpsError> {
        let metadata = self.sftp.stat(path)?;
        let file_type = match metadata.file_type {
            zap_sftp::types::FileType::Dir => FileEntryType::Directory,
            zap_sftp::types::FileType::File => FileEntryType::File,
            zap_sftp::types::FileType::Symlink => FileEntryType::Symlink,
            zap_sftp::types::FileType::Other => FileEntryType::Other,
        };
        let modified = metadata.modified.map(|t| {
            let datetime: chrono::DateTime<chrono::Local> = t.into();
            datetime.format("%Y-%m-%d %H:%M").to_string()
        });
        let perms = &metadata.permissions;
        let owner = sftp_ops::bool_to_rwx(perms.owner_read, perms.owner_write, perms.owner_exec);
        let group = sftp_ops::bool_to_rwx(perms.group_read, perms.group_write, perms.group_exec);
        let other = sftp_ops::bool_to_rwx(perms.other_read, perms.other_write, perms.other_exec);
        let permissions = Some(format!("{owner}{group}{other}"));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(FileEntry {
            name,
            path: path.to_path_buf(),
            file_type,
            size: metadata.size,
            modified,
            permissions,
        })
    }

    fn upload_file(
        &self,
        local_path: &Path,
        remote_path: &Path,
        progress_cb: Option<&ProgressCallback>,
        cancel_flag: Option<&AtomicBool>,
    ) -> Result<(), SftpOpsError> {
        static NEVER_CANCEL: AtomicBool = AtomicBool::new(false);
        let flag = cancel_flag.unwrap_or(&NEVER_CANCEL);
        sftp_ops::upload_file_streaming(&self.sftp, local_path, remote_path, progress_cb, flag)
    }

    fn download_file(
        &self,
        remote_path: &Path,
        local_path: &Path,
        progress_cb: Option<&ProgressCallback>,
        cancel_flag: Option<&AtomicBool>,
    ) -> Result<(), SftpOpsError> {
        static NEVER_CANCEL: AtomicBool = AtomicBool::new(false);
        let flag = cancel_flag.unwrap_or(&NEVER_CANCEL);
        sftp_ops::download_file_streaming(&self.sftp, remote_path, local_path, progress_cb, flag)
    }
}


// ============================================================
// InMemorySftpBackend — test implementation backed by the local filesystem
// ============================================================

/// In-memory (local temp directory) SFTP backend, used for testing
pub struct InMemorySftpBackend {
    /// Root directory, simulating the root of the remote filesystem
    root: PathBuf,
}

impl InMemorySftpBackend {
    /// Creates a new in-memory backend, using the given directory as the root
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Gets the root directory path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Maps a "remote" path to a local absolute path
    ///
    /// Remote paths start with /, mapped to a relative path under root.
    fn to_local(&self, remote_path: &Path) -> PathBuf {
        let relative = remote_path.strip_prefix("/").unwrap_or(remote_path);
        self.root.join(relative)
    }

    /// Converts a local path to a "remote" path
    fn to_remote(&self, local_path: &Path) -> PathBuf {
        match local_path.strip_prefix(&self.root) {
            Ok(rel) => {
                if rel.as_os_str().is_empty() {
                    PathBuf::from("/")
                } else {
                    PathBuf::from("/").join(rel)
                }
            }
            Err(_) => PathBuf::from("/").join(local_path),
        }
    }

    /// Builds a FileEntry from std::fs::Metadata
    fn metadata_to_entry(
        &self,
        name: String,
        local_path: &Path,
        meta: &std::fs::Metadata,
    ) -> FileEntry {
        let file_type = if meta.is_symlink() {
            FileEntryType::Symlink
        } else if meta.is_dir() {
            FileEntryType::Directory
        } else {
            FileEntryType::File
        };
        let modified = meta.modified().ok().map(|t| {
            let datetime: chrono::DateTime<chrono::Local> = t.into();
            datetime.format("%Y-%m-%d %H:%M").to_string()
        });
        FileEntry {
            name,
            path: self.to_remote(local_path),
            file_type,
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified,
            permissions: None,
        }
    }
}

impl SftpBackend for InMemorySftpBackend {
    fn list_dir(&self, path: &Path) -> Result<Vec<FileEntry>, SftpOpsError> {
        let local = self.to_local(path);
        let p = path.display();
        let entries = fs::read_dir(&local).map_err(|e| {
            SftpOpsError::Operation(format!("Failed to list directory {p}: {e}"))
        })?;

        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                SftpOpsError::Operation(format!("Failed to read directory entry: {e}"))
            })?;
            let name = entry.file_name().to_string_lossy().to_string();
            // Filter out . and ..
            if name == "." || name == ".." {
                continue;
            }
            let meta = fs::symlink_metadata(entry.path()).map_err(|e| {
                SftpOpsError::Operation(format!("Failed to read metadata: {e}"))
            })?;
            result.push(self.metadata_to_entry(name, &entry.path(), &meta));
        }

        Ok(result)
    }

    fn delete_file(&self, path: &Path) -> Result<(), SftpOpsError> {
        let local = self.to_local(path);
        let p = path.display();
        fs::remove_file(&local).map_err(|e| {
            SftpOpsError::Operation(format!("Failed to delete file {p}: {e}"))
        })
    }

    fn delete_dir_recursive(&self, path: &Path) -> Result<(), SftpOpsError> {
        let local = self.to_local(path);
        let p = path.display();
        fs::remove_dir_all(&local).map_err(|e| {
            SftpOpsError::Operation(format!("Failed to remove directory recursively {p}: {e}"))
        })
    }

    fn create_dir(&self, path: &Path) -> Result<(), SftpOpsError> {
        let local = self.to_local(path);
        let p = path.display();
        fs::create_dir(&local).map_err(|e| {
            SftpOpsError::Operation(format!("Failed to create directory {p}: {e}"))
        })
    }

    fn rename(&self, old_path: &Path, new_path: &Path) -> Result<(), SftpOpsError> {
        let old_local = self.to_local(old_path);
        let new_local = self.to_local(new_path);
        fs::rename(&old_local, &new_local).map_err(|e| {
            SftpOpsError::Operation(format!(
                "Failed to rename {} -> {}: {e}",
                old_path.display(),
                new_path.display()
            ))
        })
    }

    fn realpath(&self, path: &Path) -> Result<PathBuf, SftpOpsError> {
        let local = self.to_local(path);
        let p = path.display();
        let canonical = dunce::canonicalize(&local).map_err(|e| {
            SftpOpsError::Operation(format!("Failed to resolve path {p}: {e}"))
        })?;
        Ok(self.to_remote(&canonical))
    }

    fn stat(&self, path: &Path) -> Result<FileEntry, SftpOpsError> {
        let local = self.to_local(path);
        let p = path.display();
        let meta = fs::symlink_metadata(&local).map_err(|e| {
            SftpOpsError::Operation(format!("Failed to stat file {p}: {e}"))
        })?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(self.metadata_to_entry(name, &local, &meta))
    }

    fn upload_file(
        &self,
        local_path: &Path,
        remote_path: &Path,
        _progress_cb: Option<&ProgressCallback>,
        _cancel_flag: Option<&AtomicBool>,
    ) -> Result<(), SftpOpsError> {
        let dest = self.to_local(remote_path);
        // Ensure the parent directory exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SftpOpsError::LocalIo(format!("Failed to create directory: {e}"))
            })?;
        }
        fs::copy(local_path, &dest).map_err(|e| {
            SftpOpsError::LocalIo(format!("Failed to upload file: {e}"))
        })?;
        Ok(())
    }

    fn download_file(
        &self,
        remote_path: &Path,
        local_path: &Path,
        _progress_cb: Option<&ProgressCallback>,
        _cancel_flag: Option<&AtomicBool>,
    ) -> Result<(), SftpOpsError> {
        let src = self.to_local(remote_path);
        // Ensure the local parent directory exists
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SftpOpsError::LocalIo(format!("Failed to create directory: {e}"))
            })?;
        }
        let mut src_file = fs::File::open(&src).map_err(|e| {
            SftpOpsError::LocalIo(format!("Failed to open remote file: {e}"))
        })?;
        let mut dest_file = fs::File::create(local_path).map_err(|e| {
            SftpOpsError::LocalIo(format!("Failed to create local file: {e}"))
        })?;

        // Copy in chunks to simulate streaming transfer
        const CHUNK_SIZE: usize = 32 * 1024;
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let n = src_file.read(&mut buf).map_err(|e| {
                SftpOpsError::LocalIo(format!("Read failed: {e}"))
            })?;
            if n == 0 {
                break;
            }
            dest_file.write_all(&buf[..n]).map_err(|e| {
                SftpOpsError::LocalIo(format!("Write failed: {e}"))
            })?;
        }
        dest_file.flush().map_err(|e| {
            SftpOpsError::LocalIo(format!("Flush failed: {e}"))
        })?;
        Ok(())
    }
}

/// Convenience method for creating an Arc<dyn SftpBackend>
impl InMemorySftpBackend {
    /// Creates and wraps as Arc<dyn SftpBackend>
    pub fn into_backend(self) -> Arc<dyn SftpBackend> {
        Arc::new(self)
    }
}
