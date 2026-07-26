//! SFTP operation wrapper layer
//!
//! Wraps the zap_sftp protocol-layer API into high-level operations directly
//! usable by the UI layer.
//! author: logic
//! date: 2026-05-26

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use warp_ssh_manager::SshRepository;
use warp_ssh_manager::secrets::SshSecretStore;
use warp_ssh_manager::types::{AuthType, ResolvedSshAuth, SshServerInfo};
use zap_sftp::Sftp;
use zap_sftp::session::{AuthMethod, SftpSession};
use zap_sftp::types::OpenOptions;

use super::types::{FileEntry, FileEntryType};

/// SFTP operation error
#[derive(Debug)]
pub enum SftpOpsError {
    /// Connection error
    Connection(String),
    /// Operation error
    Operation(String),
    /// Local IO error
    LocalIo(String),
    /// Credentials not found
    NoCredentials(String),
    /// Transfer cancelled
    Cancelled,
}

impl std::fmt::Display for SftpOpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SftpOpsError::Connection(msg) => write!(f, "Connection error: {msg}"),
            SftpOpsError::Operation(msg) => write!(f, "Operation error: {msg}"),
            SftpOpsError::LocalIo(msg) => write!(f, "Local IO error: {msg}"),
            SftpOpsError::NoCredentials(msg) => write!(f, "Credentials not found: {msg}"),
            SftpOpsError::Cancelled => write!(f, "Transfer cancelled"),
        }
    }
}

impl From<zap_sftp::SftpError> for SftpOpsError {
    fn from(e: zap_sftp::SftpError) -> Self {
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

/// Connection timeout
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Establishes an SFTP connection using the server configuration
pub fn connect_from_server(
    server: &SshServerInfo,
    secret_store: &dyn SshSecretStore,
) -> Result<SftpSession, SftpOpsError> {
    let resolved_auth = resolve_sftp_auth(server)?;
    let auth = build_auth_method(server, &resolved_auth, secret_store)?;
    SftpSession::connect(
        &server.host,
        server.port,
        &resolved_auth.username,
        auth,
        Some(CONNECT_TIMEOUT),
    )
    .map_err(|e| SftpOpsError::Connection(e.to_string()))
}

fn resolve_sftp_auth(server: &SshServerInfo) -> Result<ResolvedSshAuth, SftpOpsError> {
    warp_ssh_manager::with_conn(|conn| Ok(SshRepository::resolve_server_auth(conn, server)?))
        .map_err(|e| SftpOpsError::NoCredentials(format!("Failed to parse auth: {e}")))
}

/// Lists remote directory contents, converting them to UI-layer FileEntry
pub fn list_dir(sftp: &Sftp, path: &Path) -> Result<Vec<FileEntry>, SftpOpsError> {
    let entries = sftp.read_dir(path)?;
    let result = entries
        .into_iter()
        .map(|entry| {
            let file_type = match entry.metadata.file_type {
                zap_sftp::types::FileType::Dir => FileEntryType::Directory,
                zap_sftp::types::FileType::File => FileEntryType::File,
                zap_sftp::types::FileType::Symlink => FileEntryType::Symlink,
                zap_sftp::types::FileType::Other => FileEntryType::Other,
            };
            let modified = entry.metadata.modified.map(|t| {
                let datetime: chrono::DateTime<chrono::Local> = t.into();
                datetime.format("%Y-%m-%d %H:%M").to_string()
            });
            let perms = &entry.metadata.permissions;
            let owner = bool_to_rwx(perms.owner_read, perms.owner_write, perms.owner_exec);
            let group = bool_to_rwx(perms.group_read, perms.group_write, perms.group_exec);
            let other = bool_to_rwx(perms.other_read, perms.other_write, perms.other_exec);
            let permissions = Some(format!("{owner}{group}{other}"));
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

/// Deletes a remote file
pub fn delete_file(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    sftp.remove_file(path)?;
    Ok(())
}

/// Recursively deletes a remote directory
pub fn delete_dir_recursive(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    let entries = sftp.read_dir(path)?;
    for entry in entries {
        match entry.metadata.file_type {
            zap_sftp::types::FileType::Dir => {
                delete_dir_recursive(sftp, &entry.path)?;
            }
            zap_sftp::types::FileType::File
            | zap_sftp::types::FileType::Symlink
            | zap_sftp::types::FileType::Other => {
                sftp.remove_file(&entry.path)?;
            }
        }
    }
    sftp.remove_dir(path)?;
    Ok(())
}

/// Creates a remote directory
pub fn create_dir(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    sftp.create_dir(path)?;
    Ok(())
}

/// Renames a remote file or directory
pub fn rename(sftp: &Sftp, old_path: &Path, new_path: &Path) -> Result<(), SftpOpsError> {
    let opts = zap_sftp::types::RenameOptions {
        overwrite: false,
        atomic: false,
        native: false,
    };
    sftp.rename(old_path, new_path, opts)?;
    Ok(())
}

/// Streams a local file up to the remote side
///
/// Uses a temp-file pattern: first upload to a temp path with a
/// `.sftp_partial` suffix, then rename to the target path on completion;
/// cleans up the temp file on cancellation or failure, to avoid truncating
/// an existing remote file and losing data.
pub fn upload_file_streaming(
    sftp: &Sftp,
    local_path: &Path,
    remote_path: &Path,
    progress_cb: Option<&ProgressCallback>,
    cancel_flag: &AtomicBool,
) -> Result<(), SftpOpsError> {
    let mut local_file =
        fs::File::open(local_path).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    let total_size = local_file.metadata().map(|m| m.len()).unwrap_or(0);

    // Upload to a temp path to avoid truncating an existing file
    let remote_display = remote_path.display();
    let temp_remote_path = PathBuf::from(format!("{remote_display}.sftp_partial"));
    let mut remote_file = sftp.open(&temp_remote_path, OpenOptions::write())?;

    const CHUNK_SIZE: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    let result = (|| -> Result<(), SftpOpsError> {
        loop {
            if cancel_flag.load(Ordering::SeqCst) {
                return Err(SftpOpsError::Cancelled);
            }
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
    })();

    match &result {
        Ok(()) => {
            // Upload succeeded: rename the temp file to the target path
            let rename_result = sftp.rename(
                &temp_remote_path,
                remote_path,
                zap_sftp::types::RenameOptions {
                    overwrite: true,
                    atomic: false,
                    native: false,
                },
            );

            // Some servers don't support the OVERWRITE flag; use a
            // backup-and-rename strategy to avoid data loss
            let rename_result = match rename_result {
                Ok(()) => Ok(()),
                Err(_) => {
                    let remote_display = remote_path.display();
                    let backup_path = PathBuf::from(format!("{remote_display}.sftp_backup"));
                    let backup_created = sftp
                        .rename(
                            remote_path,
                            &backup_path,
                            zap_sftp::types::RenameOptions {
                                overwrite: false,
                                atomic: false,
                                native: false,
                            },
                        )
                        .is_ok();

                    match sftp.rename(
                        &temp_remote_path,
                        remote_path,
                        zap_sftp::types::RenameOptions {
                            overwrite: false,
                            atomic: false,
                            native: false,
                        },
                    ) {
                        Ok(()) => {
                            if backup_created {
                                let _ = sftp.remove_file(&backup_path);
                            }
                            Ok(())
                        }
                        Err(e) => {
                            // Rename failed: restore the backup
                            if backup_created {
                                let _ = sftp.rename(
                                    &backup_path,
                                    remote_path,
                                    zap_sftp::types::RenameOptions {
                                        overwrite: false,
                                        atomic: false,
                                        native: false,
                                    },
                                );
                            }
                            Err(e)
                        }
                    }
                }
            };

            if let Err(e) = rename_result {
                // If rename fails, keep the remote temp file to avoid data loss
                let temp_display = temp_remote_path.display();
                return Err(SftpOpsError::Operation(format!(
                    "Failed to rename remote temp file: {e}. Temp file: {temp_display}"
                )));
            }
        }
        Err(_) => {
            // Cancelled or failed: clean up the temp file
            let _ = sftp.remove_file(&temp_remote_path);
        }
    }

    result
}

/// Streams a remote file down to the local side
///
/// Uses a temp-file pattern: first write to a temp file with a
/// `.sftp_partial` suffix, then rename to the target path on completion;
/// cleans up the temp file on cancellation or failure, to avoid truncating
/// an existing local file and losing data.
pub fn download_file_streaming(
    sftp: &Sftp,
    remote_path: &Path,
    local_path: &Path,
    progress_cb: Option<&ProgressCallback>,
    cancel_flag: &AtomicBool,
) -> Result<(), SftpOpsError> {
    let mut remote_file = sftp.open(remote_path, OpenOptions::read())?;
    let metadata = remote_file.stat()?;
    let total_size = metadata.size;

    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    }

    // Download to a temp path to avoid truncating an existing file
    let local_display = local_path.display();
    let temp_local_path = PathBuf::from(format!("{local_display}.sftp_partial"));
    let mut local_file =
        fs::File::create(&temp_local_path).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;

    const CHUNK_SIZE: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    let result = (|| -> Result<(), SftpOpsError> {
        loop {
            if cancel_flag.load(Ordering::SeqCst) {
                return Err(SftpOpsError::Cancelled);
            }
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
        local_file
            .flush()
            .map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
        Ok(())
    })();

    match &result {
        Ok(()) => {
            // Download succeeded: rename the temp file to the target path
            if let Err(e) = fs::rename(&temp_local_path, local_path) {
                // If rename fails, keep the local temp file to avoid data loss
                let temp_display = temp_local_path.display();
                return Err(SftpOpsError::LocalIo(format!(
                    "Rename failed: {e}. Downloaded temp file kept at: {temp_display}"
                )));
            }
        }
        Err(_) => {
            // Cancelled or failed: clean up the temp file
            let _ = fs::remove_file(&temp_local_path);
        }
    }

    result
}

/// Recursively uploads a local directory to the remote side
pub fn upload_dir_recursive(
    sftp: &Sftp,
    local_dir: &Path,
    remote_dir: &Path,
    progress_cb: Option<&ProgressCallback>,
    cancel_flag: &AtomicBool,
) -> Result<(), SftpOpsError> {
    if cancel_flag.load(Ordering::SeqCst) {
        return Err(SftpOpsError::Cancelled);
    }

    sftp.create_dir(remote_dir)?;

    let entries = fs::read_dir(local_dir).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;

    for entry in entries {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err(SftpOpsError::Cancelled);
        }

        let entry = entry.map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
        let file_name = entry.file_name();
        let remote_path = normalize_remote_path(&remote_dir.join(&file_name));

        let file_type = entry
            .file_type()
            .map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;

        if file_type.is_dir() {
            upload_dir_recursive(sftp, &entry.path(), &remote_path, progress_cb, cancel_flag)?;
        } else {
            upload_file_streaming(sftp, &entry.path(), &remote_path, progress_cb, cancel_flag)?;
        }
    }

    Ok(())
}

/// Recursively downloads a remote directory to the local side
pub fn download_dir_recursive(
    sftp: &Sftp,
    remote_dir: &Path,
    local_dir: &Path,
    progress_cb: Option<&ProgressCallback>,
    cancel_flag: &AtomicBool,
) -> Result<(), SftpOpsError> {
    if cancel_flag.load(Ordering::SeqCst) {
        return Err(SftpOpsError::Cancelled);
    }

    fs::create_dir_all(local_dir).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;

    let entries = sftp.read_dir(remote_dir)?;

    for entry in entries {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err(SftpOpsError::Cancelled);
        }

        // Path traversal guard: validate the safety of filenames returned by the remote server
        if entry.name.is_empty()
            || entry.name.starts_with('/')
            || entry.name.starts_with('\\')
            || entry.name.contains("..")
            || entry.name.contains('/')
            || entry.name.contains('\\')
        {
            continue;
        }

        let safe_remote_path = normalize_remote_path(&remote_dir.join(&entry.name));
        let local_path = local_dir.join(&entry.name);

        match entry.metadata.file_type {
            zap_sftp::types::FileType::Dir => {
                download_dir_recursive(
                    sftp,
                    &safe_remote_path,
                    &local_path,
                    progress_cb,
                    cancel_flag,
                )?;
            }
            zap_sftp::types::FileType::File
            | zap_sftp::types::FileType::Symlink
            | zap_sftp::types::FileType::Other => {
                download_file_streaming(
                    sftp,
                    &safe_remote_path,
                    &local_path,
                    progress_cb,
                    cancel_flag,
                )?;
            }
        }
    }

    Ok(())
}

/// Builds the auth method from the server configuration
fn build_auth_method(
    server: &SshServerInfo,
    resolved_auth: &ResolvedSshAuth,
    secret_store: &dyn SshSecretStore,
) -> Result<AuthMethod, SftpOpsError> {
    match resolved_auth.auth_type {
        AuthType::Password | AuthType::OneKey => {
            let password = secret_store
                .get(&resolved_auth.secret_lookup_id, resolved_auth.secret_kind)
                .map_err(|e| SftpOpsError::NoCredentials(format!("Failed to read password: {e}")))?
                .ok_or_else(|| {
                    SftpOpsError::NoCredentials(format!("No password stored for server {}", server.host))
                })?;
            Ok(AuthMethod::Password {
                password: password.to_string(),
            })
        }
        AuthType::Key => {
            let key_path = resolved_auth.key_path.as_ref().ok_or_else(|| {
                SftpOpsError::NoCredentials("Key authentication selected but no key path specified".to_string())
            })?;
            let expanded = shellexpand_path(key_path);
            let passphrase = secret_store
                .get(&resolved_auth.secret_lookup_id, resolved_auth.secret_kind)
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

/// Expands `~` in a path to the user's home directory
fn shellexpand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            let home_path = home.display();
            let suffix = &path[2..];
            return format!("{home_path}/{suffix}");
        }
    }
    path.to_string()
}

/// Converts read/write/execute booleans into an rwx permission string
pub(crate) fn bool_to_rwx(read: bool, write: bool, exec: bool) -> String {
    let mut s = String::with_capacity(3);
    s.push(if read { 'r' } else { '-' });
    s.push(if write { 'w' } else { '-' });
    s.push(if exec { 'x' } else { '-' });
    s
}

/// Normalizes a remote path, replacing Windows backslashes with forward slashes
///
/// The remote server (Linux) only accepts forward-slash path separators, but
/// PathBuf::join produces backslashes on Windows, so conversion is required.
pub(crate) fn normalize_remote_path(path: &PathBuf) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests SftpOpsError::Connection Display output
    #[test]
    fn test_sftp_ops_error_display_connection() {
        assert_eq!(
            SftpOpsError::Connection("refused".into()).to_string(),
            "Connection error: refused"
        );
    }

    /// Tests SftpOpsError::Operation Display output
    #[test]
    fn test_sftp_ops_error_display_operation() {
        assert_eq!(
            SftpOpsError::Operation("not found".into()).to_string(),
            "Operation error: not found"
        );
    }

    /// Tests SftpOpsError::LocalIo Display output
    #[test]
    fn test_sftp_ops_error_display_local_io() {
        assert_eq!(
            SftpOpsError::LocalIo("disk full".into()).to_string(),
            "Local IO error: disk full"
        );
    }

    /// Tests SftpOpsError::NoCredentials Display output
    #[test]
    fn test_sftp_ops_error_display_no_credentials() {
        assert_eq!(
            SftpOpsError::NoCredentials("no key".into()).to_string(),
            "Credentials not found: no key"
        );
    }

    /// Tests SftpOpsError::Cancelled Display output
    #[test]
    fn test_sftp_ops_error_display_cancelled() {
        assert_eq!(SftpOpsError::Cancelled.to_string(), "Transfer cancelled");
    }

    /// Tests conversion from std::io::Error to SftpOpsError
    #[test]
    fn test_sftp_ops_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ops_err: SftpOpsError = io_err.into();
        assert!(matches!(ops_err, SftpOpsError::LocalIo(_)));
    }

    /// Tests conversion from zap_sftp::SftpError to SftpOpsError
    #[test]
    fn test_sftp_ops_error_from_sftp_error() {
        let sftp_err = zap_sftp::SftpError::General("test error".into());
        let ops_err: SftpOpsError = sftp_err.into();
        assert!(matches!(ops_err, SftpOpsError::Operation(_)));
    }

    /// Tests shellexpand_path expanding a ~/ path
    #[test]
    fn test_shellexpand_path_home() {
        let home = dirs::home_dir().unwrap_or_default();
        let result = shellexpand_path("~/test");
        if !home.as_os_str().is_empty() {
            assert!(!result.starts_with('~'));
            assert!(result.contains("test"));
        }
    }

    /// Tests shellexpand_path leaving an absolute path unchanged
    #[test]
    fn test_shellexpand_path_absolute() {
        let result = shellexpand_path("/absolute/path");
        assert_eq!(result, "/absolute/path");
    }

    /// Tests shellexpand_path leaving a relative path unchanged
    #[test]
    fn test_shellexpand_path_relative() {
        let result = shellexpand_path("relative/path");
        assert_eq!(result, "relative/path");
    }

    /// Tests shellexpand_path leaving a bare ~ unexpanded
    #[test]
    fn test_shellexpand_path_tilde_only() {
        let result = shellexpand_path("~");
        assert_eq!(result, "~");
    }

    /// Tests shellexpand_path with an empty path
    #[test]
    fn test_shellexpand_path_empty() {
        let result = shellexpand_path("");
        assert_eq!(result, "");
    }

    // ==================== bool_to_rwx tests ====================

    /// Tests all permissions rwx
    #[test]
    fn test_bool_to_rwx_all_true() {
        assert_eq!(bool_to_rwx(true, true, true), "rwx");
    }

    /// Tests no permissions
    #[test]
    fn test_bool_to_rwx_all_false() {
        assert_eq!(bool_to_rwx(false, false, false), "---");
    }

    /// Tests read-only permission
    #[test]
    fn test_bool_to_rwx_read_only() {
        assert_eq!(bool_to_rwx(true, false, false), "r--");
    }

    /// Tests write-only permission
    #[test]
    fn test_bool_to_rwx_write_only() {
        assert_eq!(bool_to_rwx(false, true, false), "-w-");
    }

    /// Tests exec-only permission
    #[test]
    fn test_bool_to_rwx_exec_only() {
        assert_eq!(bool_to_rwx(false, false, true), "--x");
    }

    /// Tests read-write permission
    #[test]
    fn test_bool_to_rwx_read_write() {
        assert_eq!(bool_to_rwx(true, true, false), "rw-");
    }

    /// Tests read-exec permission
    #[test]
    fn test_bool_to_rwx_read_exec() {
        assert_eq!(bool_to_rwx(true, false, true), "r-x");
    }

    /// Tests write-exec permission
    #[test]
    fn test_bool_to_rwx_write_exec() {
        assert_eq!(bool_to_rwx(false, true, true), "-wx");
    }

    /// Tests the return value's length is always 3
    #[test]
    fn test_bool_to_rwx_length() {
        for r in [true, false] {
            for w in [true, false] {
                for x in [true, false] {
                    assert_eq!(bool_to_rwx(r, w, x).len(), 3);
                }
            }
        }
    }

    /// Tests each position's character can only be the expected character
    #[test]
    fn test_bool_to_rwx_valid_chars() {
        for r in [true, false] {
            for w in [true, false] {
                for x in [true, false] {
                    let s = bool_to_rwx(r, w, x);
                    let chars: Vec<char> = s.chars().collect();
                    assert!((chars[0] == 'r') || (chars[0] == '-'));
                    assert!((chars[1] == 'w') || (chars[1] == '-'));
                    assert!((chars[2] == 'x') || (chars[2] == '-'));
                }
            }
        }
    }

    // ==================== SftpOpsError boundary scenario tests ====================

    /// Tests SftpOpsError::Connection with an empty message
    #[test]
    fn test_sftp_ops_error_connection_empty() {
        assert_eq!(
            SftpOpsError::Connection(String::new()).to_string(),
            "Connection error: "
        );
    }

    /// Tests SftpOpsError::Operation with an empty message
    #[test]
    fn test_sftp_ops_error_operation_empty() {
        assert_eq!(
            SftpOpsError::Operation(String::new()).to_string(),
            "Operation error: "
        );
    }

    /// Tests SftpOpsError::LocalIo with an empty message
    #[test]
    fn test_sftp_ops_error_local_io_empty() {
        assert_eq!(
            SftpOpsError::LocalIo(String::new()).to_string(),
            "Local IO error: "
        );
    }

    /// Tests SftpOpsError::NoCredentials with an empty message
    #[test]
    fn test_sftp_ops_error_no_credentials_empty() {
        assert_eq!(
            SftpOpsError::NoCredentials(String::new()).to_string(),
            "Credentials not found: "
        );
    }

    /// Tests SftpOpsError::Cancelled is always the same fixed text
    #[test]
    fn test_sftp_ops_error_cancelled_consistent() {
        let s1 = SftpOpsError::Cancelled.to_string();
        let s2 = SftpOpsError::Cancelled.to_string();
        assert_eq!(s1, s2);
        assert_eq!(s1, "Transfer cancelled");
    }

    /// Tests shellexpand_path expanding a multi-level ~/ path
    #[test]
    fn test_shellexpand_path_home_nested() {
        let result = shellexpand_path("~/a/b/c");
        assert!(!result.starts_with('~'));
        assert!(result.contains("a/b/c"));
    }

    /// Tests shellexpand_path with just ~ followed by / and no extra path
    #[test]
    fn test_shellexpand_path_home_root() {
        let result = shellexpand_path("~/");
        let home = dirs::home_dir().unwrap_or_default();
        if !home.as_os_str().is_empty() {
            assert!(!result.starts_with('~'));
        }
    }
}
