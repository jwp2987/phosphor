# SFTP File Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native SFTP file manager to the Zap terminal, using the ssh2 crate for the protocol layer and WarpUI for the browser Pane panel.

**Architecture:** Add a new `warp_sftp` crate that wraps ssh2 protocol operations (connections, file read/write, directory management); implement the WarpUI browser view in `app/src/sftp_manager/`, integrated into the Pane system via `SftpPane`. Reuse the existing `warp_ssh_manager` to obtain host info and credentials.

**Tech Stack:** Rust, ssh2 crate (libssh2), smol, thiserror, WarpUI

---

## File list

### New files

| File | Responsibility |
|------|------|
| `crates/warp_sftp/Cargo.toml` | crate dependency declarations |
| `crates/warp_sftp/build.rs` | Windows link against advapi32 |
| `crates/warp_sftp/src/lib.rs` | module root, exports the public API |
| `crates/warp_sftp/src/error.rs` | SftpError / SftpChannelError |
| `crates/warp_sftp/src/types.rs` | FileType / Metadata / DirEntry / OpenOptions, etc. |
| `crates/warp_sftp/src/session.rs` | SftpSession (SSH connection management, auth) |
| `crates/warp_sftp/src/sftp.rs` | Sftp (SFTP channel, file/directory operations) |
| `crates/warp_sftp/src/dir.rs` | Dir (directory reading and sorting) |
| `crates/warp_sftp/src/file.rs` | File (file read/write) |
| `app/src/sftp_manager/mod.rs` | UI module root |
| `app/src/sftp_manager/types.rs` | UI types |
| `app/src/sftp_manager/sftp_ops.rs` | High-level operation bridge |
| `app/src/sftp_manager/browser.rs` | SftpBrowserView main view |
| `app/src/sftp_manager/file_list.rs` | File list rendering |
| `app/src/sftp_manager/breadcrumb.rs` | Breadcrumb navigation |
| `app/src/sftp_manager/context_menu.rs` | Context menu |
| `app/src/sftp_manager/dialogs.rs` | Dialogs |
| `app/src/sftp_manager/transfer_panel.rs` | Transfer progress panel |
| `app/src/pane_group/pane/sftp_pane.rs` | SftpPane (PaneContent implementation) |

### Modified files

| File | Change |
|------|----------|
| `Cargo.toml` | workspace members auto-include (crates/*), no change needed |
| `app/Cargo.toml` | add the warp_sftp dependency |
| `app/src/lib.rs` | declare the sftp_manager module |
| `app/src/app_state.rs` | add the LeafContents::Sftp variant |
| `app/src/pane_group/pane/mod.rs` | add IPaneType::Sftp + Display + PaneId + render + module declaration |
| `app/src/pane_group/mod.rs` | add an Sftp branch to restore_leaf_from_snapshot |
| `app/src/ssh_manager/panel.rs` | add the "Browse SFTP" context menu option |
| `app/src/workspace/view.rs` | add the open_sftp_pane method |

---

### Task 1: Create the branch and initialize the warp_sftp crate

**Files:**
- Create: `crates/warp_sftp/Cargo.toml`
- Create: `crates/warp_sftp/build.rs`
- Create: `crates/warp_sftp/src/lib.rs`
- Create: `crates/warp_sftp/src/error.rs`
- Create: `crates/warp_sftp/src/types.rs`
- Create: `crates/warp_sftp/src/session.rs`
- Create: `crates/warp_sftp/src/sftp.rs`
- Create: `crates/warp_sftp/src/dir.rs`
- Create: `crates/warp_sftp/src/file.rs`

- [ ] **Step 1: Create the feature branch**

```bash
git checkout -b feature/sftp-manager
```

- [ ] **Step 2: Create the crate directory structure**

```bash
mkdir -p crates/warp_sftp/src
```

- [ ] **Step 3: Create Cargo.toml**

```toml
[package]
name = "warp_sftp"
version = "0.1.0"
edition = "2021"

[dependencies]
ssh2 = { version = "0.9", features = ["openssl-on-win32"] }
openssl-sys = { version = "*", features = ["vendored"] }
smol = "2"
thiserror = "2"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create build.rs**

```rust
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
```

- [ ] **Step 5: Create error.rs**

```rust
use thiserror::Error;

/// SFTP protocol-level error
#[derive(Debug, Error)]
pub enum SftpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH2 error: {0}")]
    Ssh2(#[from] ssh2::Error),

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("operation timed out")]
    Timeout,

    #[error("file not found: {0}")]
    NoSuchFile(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("operation failed: {0}")]
    General(String),
}

/// SFTP channel error
#[derive(Debug, Error)]
pub enum SftpChannelError {
    #[error("SFTP error: {0}")]
    Sftp(#[from] SftpError),

    #[error("failed to send request: {0}")]
    SendFailed(String),

    #[error("failed to receive response: {0}")]
    RecvFailed(String),
}

impl From<ssh2::Error> for SftpChannelError {
    fn from(e: ssh2::Error) -> Self {
        SftpChannelError::Sftp(SftpError::Ssh2(e))
    }
}
```

- [ ] **Step 6: Create types.rs**

```rust
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
    /// Parse the file type from unix permission mode bits
    pub fn from_mode(mode: u32) -> Self {
        match mode & 0o170000 {
            0o040000 => FileType::Dir,
            0o100000 => FileType::File,
            0o120000 => FileType::Symlink,
            _ => FileType::Other,
        }
    }
}

/// File permissions (Unix-style)
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
    /// Parse permissions from unix mode bits
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
    /// Build from ssh2::FileStat
    pub fn from_ssh2(m: ssh2::FileStat) -> Self {
        let file_type = if m.is_dir() {
            FileType::Dir
        } else if m.is_file() {
            FileType::File
        } else {
            FileType::Other
        };
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
```

- [ ] **Step 7: Create session.rs**

```rust
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::SftpError;
use crate::sftp::Sftp;

/// Authentication method
#[derive(Debug, Clone)]
pub enum AuthMethod {
    Password { password: String },
    PublicKey { key_path: PathBuf, passphrase: Option<String> },
}

/// SFTP session, wrapping an ssh2 connection
pub struct SftpSession {
    session: Arc<ssh2::Session>,
    _tcp: TcpStream,
}

impl SftpSession {
    /// Establish an SSH connection with the given parameters
    pub fn connect(
        host: &str,
        port: u16,
        username: &str,
        auth: AuthMethod,
    ) -> Result<Self, SftpError> {
        let addr = format!("{host}:{port}");
        let tcp = TcpStream::connect(&addr)
            .map_err(|e| SftpError::ConnectionFailed(format!("failed to connect to {addr}: {e}")))?;

        let mut session = ssh2::Session::new()
            .map_err(|e| SftpError::ConnectionFailed(format!("failed to create SSH session: {e}")))?;

        let tcp_for_session = tcp.try_clone()
            .map_err(|e| SftpError::ConnectionFailed(format!("failed to clone TCP stream: {e}")))?;
        session.set_tcp_stream(tcp_for_session);
        session.handshake()
            .map_err(|e| SftpError::ConnectionFailed(format!("SSH handshake failed: {e}")))?;

        match &auth {
            AuthMethod::Password { password } => {
                session.userauth_password(username, password)
                    .map_err(|e| SftpError::AuthFailed(format!("password authentication failed: {e}")))?;
            }
            AuthMethod::PublicKey { key_path, passphrase } => {
                let pass = passphrase.as_deref();
                session.userauth_pubkey_file(username, None, key_path, pass)
                    .map_err(|e| SftpError::AuthFailed(format!("key authentication failed: {e}")))?;
            }
        }

        if !session.authenticated() {
            return Err(SftpError::AuthFailed("authentication did not succeed".into()));
        }

        Ok(Self {
            session: Arc::new(session),
            _tcp: tcp,
        })
    }

    /// Get the SFTP channel
    pub fn sftp(&self) -> Result<Sftp, SftpError> {
        let sftp = self.session.sftp()?;
        Ok(Sftp::new(sftp))
    }

    /// Disconnect
    pub fn disconnect(&self) -> Result<(), SftpError> {
        self.session.disconnect(None, "bye", None)?;
        Ok(())
    }

    /// Check whether the connection is still alive
    pub fn is_authenticated(&self) -> bool {
        self.session.authenticated()
    }
}

impl Drop for SftpSession {
    fn drop(&mut self) {
        let _ = self.session.disconnect(None, "bye", None);
    }
}
```

- [ ] **Step 8: Create sftp.rs**

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use crate::dir::Dir;
use crate::error::SftpError;
use crate::file::File;
use crate::types::{DirEntry, Metadata, OpenOptions, RenameOptions};

/// SFTP channel, the entry point for all remote file system operations
#[derive(Clone)]
pub struct Sftp {
    inner: Arc<Mutex<ssh2::Sftp>>,
}

impl Sftp {
    /// Build an Sftp instance from ssh2::Sftp
    pub(crate) fn new(sftp: ssh2::Sftp) -> Self {
        Self {
            inner: Arc::new(Mutex::new(sftp)),
        }
    }

    /// Open a remote file
    pub fn open(&self, path: &Path, options: OpenOptions) -> Result<File, SftpError> {
        let sftp = self.inner.lock().unwrap();
        File::open(&sftp, path, &options)
    }

    /// Create a directory
    pub fn create_dir(&self, path: &Path) -> Result<(), SftpError> {
        let sftp = self.inner.lock().unwrap();
        sftp.mkdir(path, 0o755)?;
        Ok(())
    }

    /// Remove a directory (must be empty)
    pub fn remove_dir(&self, path: &Path) -> Result<(), SftpError> {
        let sftp = self.inner.lock().unwrap();
        sftp.rmdir(path)?;
        Ok(())
    }

    /// Remove a file
    pub fn remove_file(&self, path: &Path) -> Result<(), SftpError> {
        let sftp = self.inner.lock().unwrap();
        sftp.unlink(path)?;
        Ok(())
    }

    /// Rename/move
    pub fn rename(&self, src: &Path, dst: &Path, opts: RenameOptions) -> Result<(), SftpError> {
        let sftp = self.inner.lock().unwrap();
        let mut flags = ssh2::RenameFlags::empty();
        if opts.overwrite {
            flags |= ssh2::RenameFlags::OVERWRITE;
        }
        if opts.atomic {
            flags |= ssh2::RenameFlags::ATOMIC;
        }
        if opts.native {
            flags |= ssh2::RenameFlags::NATIVE;
        }
        sftp.rename(src, dst, Some(flags))?;
        Ok(())
    }

    /// Get file metadata (following symlinks)
    pub fn stat(&self, path: &Path) -> Result<Metadata, SftpError> {
        let sftp = self.inner.lock().unwrap();
        let stat = sftp.stat(path)?;
        Ok(Metadata::from_ssh2(stat))
    }

    /// Get file metadata (not following symlinks)
    pub fn lstat(&self, path: &Path) -> Result<Metadata, SftpError> {
        let sftp = self.inner.lock().unwrap();
        let stat = sftp.lstat(path)?;
        Ok(Metadata::from_ssh2(stat))
    }

    /// Read directory contents
    pub fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, SftpError> {
        let sftp = self.inner.lock().unwrap();
        Dir::read_dir(&sftp, path)
    }

    /// Create a symlink
    pub fn symlink(&self, src: &Path, dst: &Path) -> Result<(), SftpError> {
        let sftp = self.inner.lock().unwrap();
        sftp.symlink(src, dst)?;
        Ok(())
    }

    /// Read a symlink's target
    pub fn readlink(&self, path: &Path) -> Result<PathBuf, SftpError> {
        let sftp = self.inner.lock().unwrap();
        let target = sftp.readlink(path)?;
        Ok(target)
    }
}
```

- [ ] **Step 9: Create dir.rs**

```rust
use std::path::Path;

use crate::error::SftpError;
use crate::types::{DirEntry, FileType, Metadata};

/// SFTP remote directory operations
pub struct Dir;

impl Dir {
    /// Read remote directory contents
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
```

- [ ] **Step 10: Create file.rs**

```rust
use std::io::{Read, Write};

use crate::error::SftpError;
use crate::types::OpenOptions;

/// SFTP remote file handle
pub struct File {
    handle: ssh2::File,
}

impl File {
    /// Open a remote file
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

    /// Read the entire file contents
    pub fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<u64, SftpError> {
        let n = self.handle.read_to_end(buf)?;
        Ok(n as u64)
    }

    /// Write the entire contents
    pub fn write_all(&mut self, buf: &[u8]) -> Result<(), SftpError> {
        self.handle.write_all(buf)?;
        Ok(())
    }

    /// Flush the write buffer
    pub fn flush(&mut self) -> Result<(), SftpError> {
        self.handle.flush()?;
        Ok(())
    }

    /// Get file metadata
    pub fn stat(&mut self) -> Result<crate::types::Metadata, SftpError> {
        let stat = self.handle.stat()?;
        Ok(crate::types::Metadata::from_ssh2(stat))
    }

    /// Read a chunk of data into the buffer
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, SftpError> {
        use std::io::Read;
        let n = self.handle.read(buf)?;
        Ok(n)
    }
}
```

- [ ] **Step 11: Create lib.rs**

```rust
pub mod dir;
pub mod error;
pub mod file;
pub mod session;
pub mod sftp;
pub mod types;

pub use dir::Dir;
pub use error::{SftpChannelError, SftpError};
pub use file::File;
pub use session::{AuthMethod, SftpSession};
pub use sftp::Sftp;
pub use types::*;
```

- [ ] **Step 12: Verify the crate compiles**

Run: `cargo check -p warp_sftp`
Expected: compiles successfully (may need to download the openssl-sys vendored dependency)

- [ ] **Step 13: Commit**

```bash
git add crates/warp_sftp/
git commit -m "feat: add the warp_sftp crate implementing the SFTP protocol layer"
```

---

### Task 2: Add the warp_sftp dependency to app and scaffold the UI module

**Files:**
- Modify: `app/Cargo.toml`
- Modify: `app/src/lib.rs`
- Create: `app/src/sftp_manager/mod.rs`
- Create: `app/src/sftp_manager/types.rs`
- Create: `app/src/sftp_manager/sftp_ops.rs`

- [ ] **Step 1: Add the dependency in app/Cargo.toml**

Find the `warp_ssh_manager` line in the `[dependencies]` section and add nearby:

```toml
warp_sftp = { path = "crates/warp_sftp" }
```

- [ ] **Step 2: Declare the module in app/src/lib.rs**

Find the other module declarations (e.g. `pub mod ssh_manager;`) and add nearby:

```rust
pub mod sftp_manager;
```

- [ ] **Step 3: Create sftp_manager/mod.rs**

```rust
pub mod breadcrumb;
pub mod browser;
pub mod context_menu;
pub mod dialogs;
pub mod file_list;
pub mod sftp_ops;
pub mod transfer_panel;
pub mod types;

#[allow(unused_imports)]
pub use browser::{SftpBrowserAction, SftpBrowserView};
#[allow(unused_imports)]
pub use types::*;
```

- [ ] **Step 4: Create sftp_manager/types.rs**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// File entry type (UI layer)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEntryType {
    File,
    Directory,
    Symlink,
    Other,
}

/// File entry (for UI display)
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub file_type: FileEntryType,
    pub size: u64,
    pub modified: Option<String>,
    pub permissions: Option<String>,
}

/// Transfer direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Upload,
    Download,
}

/// Transfer state
#[derive(Debug, Clone)]
pub enum TransferState {
    Pending,
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
}

/// Transfer task
#[derive(Debug, Clone)]
pub struct TransferTask {
    pub id: usize,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub direction: TransferDirection,
    pub total_size: u64,
    pub transferred: u64,
    pub state: TransferState,
    pub cancel_flag: Arc<AtomicBool>,
}

impl TransferTask {
    /// Create a new transfer task
    pub fn new(
        id: usize,
        source_path: PathBuf,
        target_path: PathBuf,
        direction: TransferDirection,
        total_size: u64,
    ) -> Self {
        Self {
            id,
            source_path,
            target_path,
            direction,
            total_size,
            transferred: 0,
            state: TransferState::Pending,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Compute the progress percentage (0-100)
    pub fn progress_percent(&self) -> u8 {
        if self.total_size == 0 {
            return 0;
        }
        ((self.transferred as f64 / self.total_size as f64) * 100.0) as u8
    }

    /// Cancel the transfer
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// Check whether it's been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }
}

/// Dialog type
#[derive(Debug, Clone)]
pub enum Dialog {
    DeleteConfirm { paths: Vec<PathBuf> },
    Rename {
        path: PathBuf,
        original_name: String,
    },
    CreateFolder {
        parent_path: PathBuf,
    },
    Move {
        source: PathBuf,
        target_dir: PathBuf,
    },
    OverwriteConfirm {
        source: PathBuf,
        target: PathBuf,
    },
    FileDetails { entry: FileEntry },
}

/// Connection state
#[derive(Debug)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Failed(String),
}

/// Format a file size
pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if size >= GB {
        format!("{:.1} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
    }
}
```

- [ ] **Step 5: Create sftp_manager/sftp_ops.rs**

```rust
//! SFTP operation wrapper layer

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use warp_sftp::session::{AuthMethod, SftpSession};
use warp_sftp::types::OpenOptions;
use warp_sftp::Sftp;
use warp_ssh_manager::secrets::{SecretKind, SshSecretStore};
use warp_ssh_manager::types::{AuthType, SshServerInfo};

use super::types::{FileEntry, FileEntryType};

/// Maximum number of parallel transfers
const MAX_PARALLEL_TRANSFERS: usize = 2;

/// SFTP operation error
#[derive(Debug)]
pub enum SftpOpsError {
    Connection(String),
    Operation(String),
    LocalIo(String),
    NoCredentials(String),
    Cancelled,
}

impl std::fmt::Display for SftpOpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SftpOpsError::Connection(msg) => write!(f, "connection error: {msg}"),
            SftpOpsError::Operation(msg) => write!(f, "operation error: {msg}"),
            SftpOpsError::LocalIo(msg) => write!(f, "local IO error: {msg}"),
            SftpOpsError::NoCredentials(msg) => write!(f, "no credentials found: {msg}"),
            SftpOpsError::Cancelled => write!(f, "transfer cancelled"),
        }
    }
}

impl From<warp_sftp::SftpError> for SftpOpsError {
    fn from(e: warp_sftp::SftpError) -> Self {
        SftpOpsError::Operation(e.to_string())
    }
}

impl From<std::io::Error> for SftpOpsError {
    fn from(e: std::io::Error) -> Self {
        SftpOpsError::LocalIo(e.to_string())
    }
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send>;

/// Establish an SFTP connection using the server configuration
pub fn connect_from_server(
    server: &SshServerInfo,
    secret_store: &dyn SshSecretStore,
) -> Result<SftpSession, SftpOpsError> {
    let auth = build_auth_method(server, secret_store)?;
    SftpSession::connect(&server.host, server.port, &server.username, auth)
        .map_err(|e| SftpOpsError::Connection(e.to_string()))
}

/// List remote directory contents
pub fn list_dir(sftp: &Sftp, path: &Path) -> Result<Vec<FileEntry>, SftpOpsError> {
    let entries = sftp.read_dir(path)?;
    let result = entries
        .into_iter()
        .map(|entry| {
            let file_type = match entry.metadata.file_type {
                warp_sftp::types::FileType::Dir => FileEntryType::Directory,
                warp_sftp::types::FileType::File => FileEntryType::File,
                warp_sftp::types::FileType::Symlink => FileEntryType::Symlink,
                warp_sftp::types::FileType::Other => FileEntryType::Other,
            };
            let modified = entry.metadata.modified.map(|t| {
                let datetime: chrono::DateTime<chrono::Local> = t.into();
                datetime.format("%Y-%m-%d %H:%M").to_string()
            });
            let perms = &entry.metadata.permissions;
            let permissions = Some(format!(
                "{}{}{}{}{}{}{}{}{}",
                if perms.owner_read { 'r' } else { '-' },
                if perms.owner_write { 'w' } else { '-' },
                if perms.owner_exec { 'x' } else { '-' },
                if perms.group_read { 'r' } else { '-' },
                if perms.group_write { 'w' } else { '-' },
                if perms.group_exec { 'x' } else { '-' },
                if perms.other_read { 'r' } else { '-' },
                if perms.other_write { 'w' } else { '-' },
                if perms.other_exec { 'x' } else { '-' },
            ));
            FileEntry {
                name: entry.name,
                path: entry.path,
                file_type,
                size: entry.metadata.size,
                modified,
                permissions,
            }
        })
        .collect();
    Ok(result)
}

/// Delete a remote file
pub fn delete_file(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    sftp.remove_file(path)?;
    Ok(())
}

/// Recursively delete a remote directory
pub fn delete_dir_recursive(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    let entries = sftp.read_dir(path)?;
    for entry in entries {
        match entry.metadata.file_type {
            warp_sftp::types::FileType::Dir => {
                delete_dir_recursive(sftp, &entry.path)?;
            }
            _ => {
                sftp.remove_file(&entry.path)?;
            }
        }
    }
    sftp.remove_dir(path)?;
    Ok(())
}

/// Create a remote directory
pub fn create_dir(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    sftp.create_dir(path)?;
    Ok(())
}

/// Rename a remote file or directory
pub fn rename(sftp: &Sftp, old_path: &Path, new_path: &Path) -> Result<(), SftpOpsError> {
    let opts = warp_sftp::types::RenameOptions {
        overwrite: false,
        atomic: false,
        native: false,
    };
    sftp.rename(old_path, new_path, opts)?;
    Ok(())
}

/// Stream-upload a local file to the remote
pub fn upload_file_streaming(
    sftp: &Sftp,
    local_path: &Path,
    remote_path: &Path,
    progress_cb: Option<&ProgressCallback>,
) -> Result<(), SftpOpsError> {
    let mut local_file =
        fs::File::open(local_path).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    let total_size = local_file.metadata().map(|m| m.len()).unwrap_or(0);

    let mut remote_file = sftp.open(remote_path, OpenOptions::write())?;

    const CHUNK_SIZE: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    loop {
        let n = std::io::Read::read(&mut local_file, &mut buf)
            .map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
        if n == 0 {
            break;
        }
        remote_file.write_all(&buf[..n])?;
        transferred += n as u64;
        if let Some(cb) = progress_cb {
            cb(transferred, total_size);
        }
    }

    remote_file.flush()?;
    Ok(())
}

/// Stream-download a remote file to local
pub fn download_file_streaming(
    sftp: &Sftp,
    remote_path: &Path,
    local_path: &Path,
    progress_cb: Option<&ProgressCallback>,
) -> Result<(), SftpOpsError> {
    let mut remote_file = sftp.open(remote_path, OpenOptions::read())?;
    let metadata = remote_file.stat()?;
    let total_size = metadata.size;

    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    }

    let mut local_file =
        fs::File::create(local_path).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;

    const CHUNK_SIZE: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    loop {
        let n = remote_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        local_file
            .write_all(&buf[..n])
            .map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
        transferred += n as u64;
        if let Some(cb) = progress_cb {
            cb(transferred, total_size);
        }
    }

    local_file.flush().map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    Ok(())
}

/// Build the auth method from the server configuration
fn build_auth_method(
    server: &SshServerInfo,
    secret_store: &dyn SshSecretStore,
) -> Result<AuthMethod, SftpOpsError> {
    match server.auth_type {
        AuthType::Password => {
            let password = secret_store
                .get(&server.node_id, SecretKind::Password)
                .map_err(|e| SftpOpsError::NoCredentials(format!("failed to read password: {e}")))?
                .ok_or_else(|| {
                    SftpOpsError::NoCredentials(format!(
                        "no password stored for server {}",
                        server.host
                    ))
                })?;
            Ok(AuthMethod::Password {
                password: password.to_string(),
            })
        }
        AuthType::Key => {
            let key_path = server
                .key_path
                .as_ref()
                .ok_or_else(|| {
                    SftpOpsError::NoCredentials("key authentication but no key path specified".to_string())
                })?;
            let expanded = shellexpand_path(key_path);
            let passphrase = secret_store
                .get(&server.node_id, SecretKind::Passphrase)
                .ok()
                .flatten()
                .map(|p| p.to_string());
            Ok(AuthMethod::PublicKey {
                key_path: PathBuf::from(expanded),
                passphrase,
            })
        }
    }
}

/// Expand a leading ~ in a path to the user's home directory
fn shellexpand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), &path[2..]);
        }
    }
    path.to_string()
}
```

- [ ] **Step 6: Create placeholder modules for the remaining UI files (to ensure compilation succeeds)**

Create `browser.rs`:

```rust
//! SFTP browser view (placeholder, fleshed out in Task 3)

use std::path::PathBuf;

/// SFTP browser Action
#[derive(Debug, Clone)]
pub enum SftpBrowserAction {
    NavigateTo(PathBuf),
    GoUp,
}

/// SFTP browser view (placeholder)
pub struct SftpBrowserView;

impl SftpBrowserView {
    pub fn new(_node_id: String, _ctx: &mut warpui::ViewContext<Self>) -> Self {
        Self
    }
    pub fn pane_configuration(&self) -> warpui::ModelHandle<crate::pane_group::PaneConfiguration> {
        unimplemented!("implemented in Task 3")
    }
}

impl warpui::Entity for SftpBrowserView {
    type Event = crate::pane_group::PaneEvent;
}

impl warpui::TypedActionView for SftpBrowserView {
    type Action = SftpBrowserAction;
    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut warpui::ViewContext<Self>) {}
}

impl warpui::View for SftpBrowserView {
    fn ui_name() -> &'static str { "SftpBrowserView" }
    fn render(&self, _app: &warpui::AppContext) -> Box<dyn warpui::Element> {
        use warpui::elements::Flex;
        Flex::column().finish()
    }
}

impl crate::pane_group::BackingView for SftpBrowserView {
    type PaneHeaderOverflowMenuAction = SftpBrowserAction;
    type CustomAction = ();
    type AssociatedData = ();

    fn handle_pane_header_overflow_menu_action(
        &mut self,
        _action: &Self::PaneHeaderOverflowMenuAction,
        _ctx: &mut warpui::ViewContext<Self>,
    ) {}
    fn close(&mut self, _ctx: &mut warpui::ViewContext<Self>) {
        _ctx.emit(crate::pane_group::PaneEvent::Close);
    }
    fn focus_contents(&mut self, _ctx: &mut warpui::ViewContext<Self>) {}
    fn render_header_content(
        &self,
        _ctx: &crate::pane_group::pane::view::HeaderRenderContext<'_>,
        _app: &warpui::AppContext,
    ) -> crate::pane_group::pane::view::HeaderContent {
        crate::pane_group::pane::view::HeaderContent::simple("SFTP Browser".to_string())
    }
    fn set_focus_handle(
        &mut self,
        _focus_handle: crate::pane_group::focus_state::PaneFocusHandle,
        _ctx: &mut warpui::ViewContext<Self>,
    ) {}
}
```

Create `file_list.rs`:

```rust
//! SFTP file list rendering (placeholder)
use warpui::Element;
use warp_core::ui::appearance::Appearance;
use std::collections::HashSet;
use warpui::elements::MouseStateHandle;
use super::types::FileEntry;

pub fn render_header(_appearance: &Appearance) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}

pub fn render_file_rows(
    _entries: &[FileEntry],
    _selected: &HashSet<usize>,
    _mouse_handles: &[MouseStateHandle],
    _appearance: &Appearance,
) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
```

Create `breadcrumb.rs`:

```rust
//! SFTP breadcrumb navigation (placeholder)
use std::path::PathBuf;
use warpui::Element;
use warp_core::ui::appearance::Appearance;

pub fn render_breadcrumb(_current_path: &PathBuf, _appearance: &Appearance) -> Vec<Box<dyn Element>> {
    Vec::new()
}
```

Create `context_menu.rs`:

```rust
//! SFTP context menu (placeholder)
use warpui::Element;
use warp_core::ui::appearance::Appearance;

#[derive(Debug)]
pub struct ContextMenuState {
    pub entry_index: usize,
    pub position: (f32, f32),
}

impl ContextMenuState {
    pub fn new(entry_index: usize, position: (f32, f32)) -> Self {
        Self { entry_index, position }
    }
}

pub fn render_context_menu(_state: &ContextMenuState, _appearance: &Appearance) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
```

Create `dialogs.rs`:

```rust
//! SFTP dialog rendering (placeholder)
use warpui::Element;
use warp_core::ui::appearance::Appearance;
use crate::editor::EditorView;
use super::types::Dialog;

pub fn render_dialog(
    _dialog: &Dialog,
    _rename_editor: &warpui::ViewHandle<EditorView>,
    _new_folder_editor: &warpui::ViewHandle<EditorView>,
    _appearance: &Appearance,
) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
```

Create `transfer_panel.rs`:

```rust
//! SFTP transfer panel (placeholder)
use warpui::Element;
use warp_core::ui::appearance::Appearance;
use super::types::TransferTask;

pub fn render_transfer_panel(
    _transfers: &[TransferTask],
    _is_expanded: bool,
    _appearance: &Appearance,
) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check -p warp`
Expected: compiles successfully (some unused warnings are fine)

- [ ] **Step 8: Commit**

```bash
git add app/Cargo.toml app/src/lib.rs app/src/sftp_manager/
git commit -m "feat: add the sftp_manager UI module scaffold and operation wrapper layer"
```

---

### Task 3: Implement SftpPane and integrate with the Pane system

**Files:**
- Create: `app/src/pane_group/pane/sftp_pane.rs`
- Modify: `app/src/app_state.rs` — add LeafContents::Sftp
- Modify: `app/src/pane_group/pane/mod.rs` — add IPaneType::Sftp and related registration
- Modify: `app/src/pane_group/mod.rs` — add an Sftp branch to restore_leaf_from_snapshot

- [ ] **Step 1: Add the Sftp variant to the LeafContents enum in app_state.rs**

Add after `LeafContents::SshServer { node_id: String }`:

```rust
Sftp { node_id: String },
```

In the `is_persisted()` method, add after `LeafContents::SshServer { .. } => false,`:

```rust
LeafContents::Sftp { .. } => false,
```

- [ ] **Step 2: Add IPaneType::Sftp in pane/mod.rs**

Add an `Sftp` variant after `SshServer` in the `IPaneType` enum.

Add to the `Display` impl:

```rust
IPaneType::Sftp => write!(f, "SFTP"),
```

Add factory methods to `PaneId`:

```rust
pub fn from_sftp_pane_ctx(ctx: &ViewContext<PaneView<SftpBrowserView>>) -> Self {
    Self::new_from_ctx(IPaneType::Sftp, ctx)
}

pub fn from_sftp_pane_view(
    sftp_pane_view: &ViewHandle<PaneView<SftpBrowserView>>,
) -> Self {
    Self::new(IPaneType::Sftp, sftp_pane_view)
}
```

Add to `PaneId::render`:

```rust
IPaneType::Sftp => {
    ChildView::<PaneView<SftpBrowserView>>::with_id(self.0.pane_view_id).finish()
}
```

Add to the module declarations:

```rust
pub(crate) mod sftp_pane;
```

In the `use` statements at the top of the file, find the SshServerView import and add nearby:

```rust
use crate::sftp_manager::browser::SftpBrowserView;
```

- [ ] **Step 3: Create sftp_pane.rs**

```rust
use warpui::{AppContext, ModelHandle, ViewContext, ViewHandle};

use crate::pane_group::focus_state::PaneFocusHandle;
use crate::pane_group::pane::view;
use crate::pane_group::{
    BackingView, DetachType, LeafContents, PaneConfiguration, PaneContent, PaneEvent,
    PaneGroup, PaneId,
};
use crate::sftp_manager::browser::SftpBrowserView;

use super::view::{ChildView, PaneView};

pub struct SftpPane {
    view: ViewHandle<PaneView<SftpBrowserView>>,
    pane_configuration: ModelHandle<PaneConfiguration>,
    node_id: String,
}

impl SftpPane {
    pub fn new(node_id: String, ctx: &mut ViewContext<impl warpui::View>) -> Self {
        let id_for_view = node_id.clone();
        let server_view =
            ctx.add_typed_action_view(move |ctx| SftpBrowserView::new(id_for_view, ctx));
        let pane_configuration = server_view.as_ref(ctx).pane_configuration();
        let pane_view = ctx.add_typed_action_view(|ctx| {
            let pane_id = PaneId::from_sftp_pane_ctx(ctx);
            PaneView::new(pane_id, server_view, (), pane_configuration.clone(), ctx)
        });
        Self { view: pane_view, pane_configuration, node_id }
    }
}

impl PaneContent for SftpPane {
    fn id(&self) -> PaneId { PaneId::from_sftp_pane_view(&self.view) }

    fn attach(
        &self,
        _group: &PaneGroup,
        focus_handle: PaneFocusHandle,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        self.view.update(ctx, |view, ctx| view.set_focus_handle(focus_handle, ctx));
        let child = self.view.as_ref(ctx).child(ctx);
        let pane_id = self.id();
        ctx.subscribe_to_view(&child, move |pane_group, _, event, ctx| {
            pane_group.handle_pane_event(pane_id, event, ctx);
        });
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        _detach_type: DetachType,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        let child = self.view.as_ref(ctx).child(ctx);
        ctx.unsubscribe_to_view(&child);
    }

    fn snapshot(&self, _ctx: &AppContext) -> LeafContents {
        LeafContents::Sftp { node_id: self.node_id.clone() }
    }

    fn has_application_focus(&self, ctx: &mut ViewContext<PaneGroup>) -> bool {
        self.view.is_self_or_child_focused(ctx)
    }

    fn focus(&self, ctx: &mut ViewContext<PaneGroup>) {
        self.view.as_ref(ctx).child(ctx).update(ctx, BackingView::focus_contents)
    }

    fn shareable_link(
        &self,
        _ctx: &mut ViewContext<PaneGroup>,
    ) -> Result<crate::pane_group::ShareableLink, crate::pane_group::ShareableLinkError> {
        Ok(crate::pane_group::ShareableLink::Base)
    }

    fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn is_pane_being_dragged(&self, ctx: &AppContext) -> bool {
        self.view.as_ref(ctx).is_being_dragged()
    }
}
```

- [ ] **Step 4: Add an Sftp branch to restore_leaf_from_snapshot in pane_group/mod.rs**

Find the `LeafContents::SshServer { .. }` match arm and add after it:

```rust
LeafContents::Sftp { .. } => {
    Err(anyhow::anyhow!(
        "SFTP pane should not have been persisted, as it cannot be restored"
    ))
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p warp`
Expected: compiles successfully

- [ ] **Step 6: Commit**

```bash
git add app/src/app_state.rs app/src/pane_group/
git commit -m "feat: implement SftpPane and integrate with the Pane system"
```

---

### Task 4: Wire up the SSH manager context menu and Workspace

**Files:**
- Modify: `app/src/ssh_manager/panel.rs` — add a "Browse SFTP" menu item
- Modify: `app/src/workspace/view.rs` — add the open_sftp_pane method

- [ ] **Step 1: Add the event and menu item in ssh_manager/panel.rs**

Add to the `SshManagerPanelEvent` enum:

```rust
OpenSftpPane { node_id: String, server: SshServerInfo },
```

In the server context-menu item list, find the `"Connect"` menu item and add a "Browse SFTP" menu item after it, dispatching `SshManagerPanelEvent::OpenSftpPane { node_id, server }`.

Specific location: find the code that builds the server context-menu items, and add a new "Browse SFTP" menu item after the "Connect" menu item.

- [ ] **Step 2: Add the handling in workspace/view.rs**

In the `LeftPanelEvent` handling match, find the `OpenSshTerminal` handling branch and add nearby:

```rust
LeftPanelEvent::OpenSftpPane { node_id, server: _ } => {
    self.open_sftp_pane(node_id.clone(), ctx);
}
```

Add the `open_sftp_pane` method:

```rust
pub fn open_sftp_pane(&mut self, node_id: String, ctx: &mut ViewContext<Self>) {
    use crate::pane_group::pane::sftp_pane::SftpPane;
    self.active_tab_pane_group().update(ctx, |pane_group, ctx| {
        let pane = SftpPane::new(node_id, ctx);
        let smart_split_direction = pane_group.smart_split_direction(ctx, WORKFLOW_AND_ENV_VAR_SPLIT_RATIO);
        pane_group.add_pane_with_direction(smart_split_direction, pane, true, ctx);
    });
}
```

Note: confirm whether `LeftPanelEvent` already has an `OpenSftpPane` variant; if not, add it at the definition site.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p warp`
Expected: compiles successfully

- [ ] **Step 4: Commit**

```bash
git add app/src/ssh_manager/panel.rs app/src/workspace/view.rs
git commit -m "feat: wire up the SSH manager context menu to open the SFTP browser panel"
```

---

### Task 5: Flesh out the SftpBrowserView main view

**Files:**
- Modify: `app/src/sftp_manager/browser.rs` — replace the placeholder with the full implementation

- [ ] **Step 1: Replace browser.rs with the full implementation**

Replace the placeholder `browser.rs` created in Task 2 with the full implementation. For the full code, refer to openwarp's `app/src/sftp_manager/browser.rs` (~1097 lines); key changes:

1. `use warp_sftp::session_bridge::` → `use warp_sftp::session::`
2. Keep `use crate::sftp_manager::sftp_ops` unchanged
3. Confirm all `warp_core::ui::appearance::Appearance` uses the same import path in zap-2
4. Confirm the variant names in the `Icon` enum exist in zap-2's `ui_components::icons::Icon`

The full implementation includes:
- The `SftpBrowserAction` enum (all Actions)
- The `SftpBrowserView` struct (connection state, navigation, transfers, dialogs, and all other fields)
- The `new()` constructor (initializes all fields + subscribes to editor events + auto-connects)
- The `connect_to_server()` method
- The `refresh_dir()` / `navigate_to()` / `go_up()` / `go_back()` / `go_forward()` methods
- The `open_entry()` / `delete_selected()` / `confirm_delete()` / `download_entry()` / `show_details()` / `rename_entry()` methods
- The `render_toolbar_btn()` / `render_toolbar()` / `render_breadcrumb()` / `render_connection_state()` / `render_error()` methods
- The `TypedActionView` impl (handle_action, handling all Actions)
- The `View` impl (render, building the full UI layout)
- The `BackingView` impl (all trait methods)
- Helper functions `build_rename_path` / `build_new_folder_path` / `build_upload_remote_path`

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p warp`
Expected: compiles successfully

- [ ] **Step 3: Commit**

```bash
git add app/src/sftp_manager/browser.rs
git commit -m "feat: implement the full SftpBrowserView browser view"
```

---

### Task 6: Flesh out the UI submodules

**Files:**
- Modify: `app/src/sftp_manager/file_list.rs` — replace the placeholder with the full implementation
- Modify: `app/src/sftp_manager/breadcrumb.rs` — replace the placeholder with the full implementation
- Modify: `app/src/sftp_manager/context_menu.rs` — replace the placeholder with the full implementation
- Modify: `app/src/sftp_manager/dialogs.rs` — replace the placeholder with the full implementation
- Modify: `app/src/sftp_manager/transfer_panel.rs` — replace the placeholder with the full implementation

- [ ] **Step 1: Replace file_list.rs with the full implementation**

Replace with openwarp's full `file_list.rs` code (~224 lines). Includes:
- The `file_icon()` function
- The `render_file_row()` function (single-row rendering, including icon/name/size/date, click/double-click events)
- The `render_header()` public function (header row: name/size/modified time)
- The `render_file_rows()` public function (the file row list)

- [ ] **Step 2: Replace breadcrumb.rs with the full implementation**

Replace with openwarp's full `breadcrumb.rs` code (~110 lines). Includes:
- The `render_breadcrumb()` public function (clickable path segments + separator icons)

- [ ] **Step 3: Replace context_menu.rs with the full implementation**

Replace with openwarp's full `context_menu.rs` code (~139 lines). Includes:
- The `ContextMenuState` struct (unchanged, already complete)
- The `MenuItem` struct
- The `build_file_menu_items()` function
- The `render_menu_item()` function
- The `render_context_menu()` public function

- [ ] **Step 4: Replace dialogs.rs with the full implementation**

Replace with openwarp's full `dialogs.rs` code (~382 lines). Includes:
- The `dialog_shell()` function
- The `render_button()` function
- The `render_delete_confirm()` / `render_rename()` / `render_create_folder()` / `render_file_details()` functions
- The `render_dialog()` public function

- [ ] **Step 5: Replace transfer_panel.rs with the full implementation**

Replace with openwarp's full `transfer_panel.rs` code (~200 lines). Includes:
- The `render_direction_icon()` / `render_state_label()` / `render_progress_bar()` functions
- The `render_transfer_row()` function
- The `render_transfer_panel()` public function

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p warp`
Expected: compiles successfully

- [ ] **Step 7: Commit**

```bash
git add app/src/sftp_manager/
git commit -m "feat: implement all SFTP browser UI submodules"
```

---

### Task 7: Address compile errors and missed integration points

**Files:**
- Various (modified based on compilation results)

- [ ] **Step 1: Full compilation check**

Run: `cargo check -p warp 2>&1 | head -100`
Expected: there may be some import path differences or missing match arms

- [ ] **Step 2: Fix compile errors one by one**

Common issues to fix:
1. The `LeftPanelEvent` enum is missing the `OpenSftpPane` variant → add it
2. The `LeafContents` match in `persistence/sqlite.rs` is missing an `Sftp` branch → add it (not persisted)
3. Some variant names in the `Icon` enum differ → replace with the corresponding names in zap-2
4. Import path differences (`warp_core::ui::` vs other paths) → fix them

- [ ] **Step 3: Full compilation again**

Run: `cargo check -p warp`
Expected: compiles successfully, only unused warnings remain

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "fix: fix SFTP integration compile errors"
```

---

### Task 8: Verify the full build

- [ ] **Step 1: Full release build**

Run: `cargo build -p warp --release`
Expected: build succeeds

- [ ] **Step 2: Final commit**

```bash
git add -A
git commit -m "feat: complete the SFTP file manager feature"
```
