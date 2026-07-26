//! SFTP remote file handle module
//!
//! Wraps ssh2::File to provide remote file read/write operations, supporting
//! the Read/Write traits and streaming transfer.
//! author: logic
//! date: 2026-05-31

use std::io::{Read, Write};

use crate::error::SftpError;
use crate::types::OpenOptions;

/// SFTP remote file handle
pub struct File {
    handle: ssh2::File,
}

impl File {
    /// Opens a remote file
    pub(crate) fn open(
        sftp: &ssh2::Sftp,
        path: &std::path::Path,
        options: &OpenOptions,
    ) -> Result<Self, SftpError> {
        let mut flags = ssh2::OpenFlags::empty();
        if options.read {
            flags |= ssh2::OpenFlags::READ;
        }
        if options.write.is_some() {
            flags |= ssh2::OpenFlags::WRITE;
        }
        if options.create && options.truncate {
            flags |= ssh2::OpenFlags::CREATE;
            flags |= ssh2::OpenFlags::TRUNCATE;
        } else if options.create {
            flags |= ssh2::OpenFlags::CREATE;
        }
        if matches!(options.write, Some(crate::types::WriteMode::Append)) {
            flags |= ssh2::OpenFlags::APPEND;
        }

        let handle = sftp.open_mode(
            path,
            flags,
            options.mode.unwrap_or(0o644) as i32,
            ssh2::OpenType::File,
        )?;
        Ok(File { handle })
    }

    /// Reads the entire contents of the file
    pub fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<u64, SftpError> {
        let n = self.handle.read_to_end(buf)?;
        Ok(n as u64)
    }

    /// Writes the entire contents
    pub fn write_all(&mut self, buf: &[u8]) -> Result<(), SftpError> {
        self.handle.write_all(buf)?;
        Ok(())
    }

    /// Flushes the write buffer
    pub fn flush(&mut self) -> Result<(), SftpError> {
        self.handle.flush()?;
        Ok(())
    }

    /// Gets the file metadata
    pub fn stat(&mut self) -> Result<crate::types::Metadata, SftpError> {
        let stat = self.handle.stat()?;
        Ok(crate::types::Metadata::from_ssh2(stat))
    }

    /// Reads a chunk of data into the buffer
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, SftpError> {
        let n = self.handle.read(buf)?;
        Ok(n)
    }
}
