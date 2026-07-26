//! SFTP protocol-layer error type definitions
//!
//! Defines the SftpError and SftpChannelError error enums, covering
//! connection, authentication, timeout, permission, and other error scenarios.
//! author: logic
//! date: 2026-05-31

use thiserror::Error;

/// SFTP protocol-level error
#[derive(Debug, Error)]
pub enum SftpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH2 error: {0}")]
    Ssh2(#[from] ssh2::Error),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("File not found: {0}")]
    NoSuchFile(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Operation failed: {0}")]
    General(String),
}

/// SFTP channel error
#[derive(Debug, Error)]
pub enum SftpChannelError {
    #[error("SFTP error: {0}")]
    Sftp(#[from] SftpError),

    #[error("Failed to send request: {0}")]
    SendFailed(String),

    #[error("Failed to receive response: {0}")]
    RecvFailed(String),
}

impl From<ssh2::Error> for SftpChannelError {
    fn from(e: ssh2::Error) -> Self {
        SftpChannelError::Sftp(SftpError::Ssh2(e))
    }
}
