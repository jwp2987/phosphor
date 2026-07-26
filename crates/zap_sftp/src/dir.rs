//! SFTP remote directory operations module
//!
//! Provides remote directory reading, automatically filtering out . and ..
//! entries, and sorting directories first + alphabetically.
//! author: logic
//! date: 2026-05-31

use std::path::Path;

use crate::error::SftpError;
use crate::types::{DirEntry, FileType, Metadata};

/// SFTP remote directory operations
pub struct Dir;

impl Dir {
    /// Reads the contents of a remote directory
    pub(crate) fn read_dir(sftp: &ssh2::Sftp, path: &Path) -> Result<Vec<DirEntry>, SftpError> {
        let mut entries = Vec::new();
        for entry in sftp.readdir(path)? {
            let name = entry
                .0
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name == "." || name == ".." {
                continue;
            }
            let metadata = Metadata::from_ssh2(entry.1);
            entries.push(DirEntry {
                name,
                path: entry.0,
                metadata,
            });
        }
        entries.sort_by(|a, b| {
            let a_is_dir = a.metadata.file_type == FileType::Dir;
            let b_is_dir = b.metadata.file_type == FileType::Dir;
            b_is_dir
                .cmp(&a_is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }
}
