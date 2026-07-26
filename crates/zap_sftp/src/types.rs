//! SFTP protocol-layer public type definitions
//!
//! Defines types like file type, metadata, open options, rename options,
//! directory entry, etc., and provides conversion from raw ssh2 types to higher-level types.
//! author: logic
//! date: 2026-05-31

use std::path::PathBuf;

/// File type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Dir,
    File,
    Symlink,
    Other,
}

impl FileType {
    /// Parses the file type from unix permission mode bits
    pub fn from_mode(mode: u32) -> Self {
        match mode & 0o170000 {
            0o040000 => FileType::Dir,
            0o100000 => FileType::File,
            0o120000 => FileType::Symlink,
            _ => FileType::Other,
        }
    }
}

/// File permissions (Unix style)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilePermissions {
    pub owner_read: bool,
    pub owner_write: bool,
    pub owner_exec: bool,
    pub group_read: bool,
    pub group_write: bool,
    pub group_exec: bool,
    pub other_read: bool,
    pub other_write: bool,
    pub other_exec: bool,
}

impl FilePermissions {
    /// Parses permissions from unix mode bits
    pub fn from_mode(mode: u32) -> Self {
        Self {
            owner_read: mode & 0o400 != 0,
            owner_write: mode & 0o200 != 0,
            owner_exec: mode & 0o100 != 0,
            group_read: mode & 0o040 != 0,
            group_write: mode & 0o020 != 0,
            group_exec: mode & 0o010 != 0,
            other_read: mode & 0o004 != 0,
            other_write: mode & 0o002 != 0,
            other_exec: mode & 0o001 != 0,
        }
    }
}

/// File metadata
#[derive(Debug, Clone)]
pub struct Metadata {
    pub file_type: FileType,
    pub permissions: FilePermissions,
    pub size: u64,
    pub uid: u32,
    pub gid: u32,
    pub accessed: Option<std::time::SystemTime>,
    pub modified: Option<std::time::SystemTime>,
}

impl Metadata {
    /// Creates from an ssh2::FileStat
    pub fn from_ssh2(m: ssh2::FileStat) -> Self {
        let file_type = FileType::from_mode(m.perm.unwrap_or(0));
        Self {
            file_type,
            permissions: FilePermissions::from_mode(m.perm.unwrap_or(0)),
            size: m.size.unwrap_or(0),
            uid: m.uid.unwrap_or(0),
            gid: m.gid.unwrap_or(0),
            accessed: m.atime.map(|t| {
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t)
            }),
            modified: m.mtime.map(|t| {
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t)
            }),
        }
    }
}

/// Write mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Write,
    Append,
}

/// Open file type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenFileType {
    File,
    Dir,
}

/// File open options
#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub read: bool,
    pub write: Option<WriteMode>,
    pub create: bool,
    pub truncate: bool,
    pub mode: Option<u32>,
    pub file_type: OpenFileType,
}

impl OpenOptions {
    /// Read-only mode
    pub fn read() -> Self {
        Self {
            read: true,
            write: None,
            create: false,
            truncate: false,
            mode: None,
            file_type: OpenFileType::File,
        }
    }

    /// Write mode (create + truncate)
    pub fn write() -> Self {
        Self {
            read: false,
            write: Some(WriteMode::Write),
            create: true,
            truncate: true,
            mode: Some(0o644),
            file_type: OpenFileType::File,
        }
    }

    /// Append mode
    pub fn append() -> Self {
        Self {
            read: false,
            write: Some(WriteMode::Append),
            create: true,
            truncate: false,
            mode: Some(0o644),
            file_type: OpenFileType::File,
        }
    }

    /// Create-new-file mode
    pub fn create_new() -> Self {
        Self {
            read: false,
            write: Some(WriteMode::Write),
            create: true,
            truncate: false,
            mode: Some(0o644),
            file_type: OpenFileType::File,
        }
    }
}

/// Rename options
#[derive(Debug, Clone, Default)]
pub struct RenameOptions {
    pub overwrite: bool,
    pub atomic: bool,
    pub native: bool,
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub metadata: Metadata,
}
