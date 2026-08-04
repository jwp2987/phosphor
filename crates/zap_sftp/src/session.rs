//! SFTP session management module
//!
//! Wraps SSH2 connection establishment, authentication, and SFTP subsystem channel creation.
//! author: logic
//! date: 2026-05-31

use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ssh2::{CheckResult, HostKeyType, KnownHostFileKind, KnownHosts};

use crate::error::SftpError;
use crate::sftp::Sftp;

/// Default connection timeout (10 seconds)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// Marks whether the connection has already been explicitly disconnected, preventing a double disconnect on Drop
    disconnected: Arc<AtomicBool>,
}

impl SftpSession {
    /// Establishes an SSH connection with the given parameters
    ///
    /// # Parameters
    /// - `host`: server address
    /// - `port`: server port
    /// - `username`: username
    /// - `auth`: authentication method
    /// - `timeout`: optional timeout; `None` uses the default 10 seconds
    pub fn connect(
        host: &str,
        port: u16,
        username: &str,
        auth: AuthMethod,
        timeout: Option<Duration>,
    ) -> Result<Self, SftpError> {
        let effective_timeout = timeout.unwrap_or(DEFAULT_TIMEOUT);
        let addr = format!("{host}:{port}");

        // Resolve DNS via ToSocketAddrs, supporting both hostnames and IP addresses
        let socket_addr = addr.to_socket_addrs()
            .map_err(|e| SftpError::ConnectionFailed(format!("Address resolution failed: {e}")))?
            .next()
            .ok_or_else(|| SftpError::ConnectionFailed(format!("DNS resolution yielded no results: {addr}")))?;

        // Use a TCP connection with a timeout
        let tcp = TcpStream::connect_timeout(&socket_addr, effective_timeout)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    SftpError::Timeout
                } else {
                    SftpError::ConnectionFailed(format!("Failed to connect to {addr}: {e}"))
                }
            })?;

        let mut session = ssh2::Session::new()
            .map_err(|e| SftpError::ConnectionFailed(format!("Failed to create SSH session: {e}")))?;

        let tcp_for_session = tcp.try_clone()
            .map_err(|e| SftpError::ConnectionFailed(format!("Failed to clone TCP stream: {e}")))?;
        session.set_tcp_stream(tcp_for_session);

        // Sets the SSH session timeout (milliseconds), affecting the handshake and all subsequent blocking operations
        session.set_timeout(effective_timeout.as_millis() as u32);

        session.handshake()
            .map_err(|e| {
                if is_timeout_error(&e) {
                    SftpError::Timeout
                } else {
                    SftpError::ConnectionFailed(format!("SSH handshake failed: {e}"))
                }
            })?;

        // Verify the server's host key against the user's known_hosts store
        // *before* sending any credentials. Without this, the connection is
        // silently vulnerable to a MITM attacker who presents their own host
        // key: the client would happily hand over the password / private-key
        // signature to whoever is on the other end of the TCP connection.
        verify_host_key(&session, host, port, default_known_hosts_path().as_deref())?;

        match &auth {
            AuthMethod::Password { password } => {
                session.userauth_password(username, password)
                    .map_err(|e| {
                        if is_timeout_error(&e) {
                            SftpError::Timeout
                        } else {
                            SftpError::AuthFailed(format!("Password authentication failed: {e}"))
                        }
                    })?;
            }
            AuthMethod::PublicKey { key_path, passphrase } => {
                let pass = passphrase.as_deref();
                session.userauth_pubkey_file(username, None, key_path, pass)
                    .map_err(|e| {
                        if is_timeout_error(&e) {
                            SftpError::Timeout
                        } else {
                            SftpError::AuthFailed(format!("Key authentication failed: {e}"))
                        }
                    })?;
            }
        }

        if !session.authenticated() {
            return Err(SftpError::AuthFailed("Authentication did not succeed".into()));
        }

        // Sets the operation timeout (30 seconds), avoiding operations blocking indefinitely on network issues
        session.set_timeout(30_000);

        Ok(Self {
            session: Arc::new(session),
            _tcp: tcp,
            disconnected: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Gets the SFTP channel
    pub fn sftp(&self) -> Result<Sftp, SftpError> {
        let sftp = self.session.sftp()?;
        Ok(Sftp::new(sftp))
    }

    /// Disconnects
    pub fn disconnect(&self) -> Result<(), SftpError> {
        if self.disconnected.swap(true, Ordering::SeqCst) {
            // Already disconnected, skip
            return Ok(());
        }
        self.session.disconnect(None, "bye", None)?;
        Ok(())
    }

    /// Checks whether the connection is still alive
    pub fn is_authenticated(&self) -> bool {
        self.session.authenticated()
    }
}

impl Drop for SftpSession {
    fn drop(&mut self) {
        if !self.disconnected.swap(true, Ordering::SeqCst) {
            let _ = self.session.disconnect(None, "bye", None);
        }
    }
}

/// Determines whether an ssh2 error is a timeout error
fn is_timeout_error(error: &ssh2::Error) -> bool {
    // ssh2 error code Session(-37) corresponds to LIBSSH2_ERROR_SOCKET_TIMEOUT
    error.code() == ssh2::ErrorCode::Session(-37)
}

/// Default known_hosts path: `~/.ssh/known_hosts`, matching OpenSSH's
/// default. Returns `None` if the home directory can't be resolved (in
/// which case verification still runs, but as an in-memory-only, non-persistent
/// check — see `verify_host_key`).
fn default_known_hosts_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

/// Verifies the just-handshaked session's host key against `known_hosts_path`
/// (trust-on-first-use + pinning), mirroring the precedent set by the terminal
/// SSH path (which shells out to the real `ssh` binary and thus gets OpenSSH's
/// own known_hosts checking for free). `zap_sftp` talks libssh2 directly, so
/// that verification has to be done here explicitly.
///
/// Must be called after `session.handshake()` and before any
/// authentication is attempted, so a MITM never gets a shot at credentials.
fn verify_host_key(
    session: &ssh2::Session,
    host: &str,
    port: u16,
    known_hosts_path: Option<&Path>,
) -> Result<(), SftpError> {
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| SftpError::ConnectionFailed("Server did not present a host key".into()))?;
    // Copy out of the session before taking further (immutable, but
    // lock-guarded) borrows of `session` via `known_hosts()`.
    let key = key.to_vec();

    let mut known_hosts = session
        .known_hosts()
        .map_err(|e| SftpError::ConnectionFailed(format!("Failed to initialize known_hosts store: {e}")))?;

    if let Some(path) = known_hosts_path {
        if path.exists() {
            known_hosts.read_file(path, KnownHostFileKind::OpenSSH).map_err(|e| {
                SftpError::ConnectionFailed(format!(
                    "Failed to read known_hosts file {}: {e}",
                    path.display()
                ))
            })?;
        }
    }

    check_and_pin_host_key(&mut known_hosts, host, port, &key, key_type, known_hosts_path)
}

/// Pure(ish) decision logic for host-key verification, split out from
/// `verify_host_key` so it's testable without a live TCP/SSH handshake:
/// `ssh2::Session::known_hosts()` works on an unconnected session, so tests
/// can seed a `KnownHosts` store with synthetic entries and exercise the
/// exact Match/Mismatch/NotFound/Failure decision paths below.
fn check_and_pin_host_key(
    known_hosts: &mut KnownHosts,
    host: &str,
    port: u16,
    key: &[u8],
    key_type: HostKeyType,
    known_hosts_path: Option<&Path>,
) -> Result<(), SftpError> {
    match known_hosts.check_port(host, port, key) {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch => Err(SftpError::HostKeyMismatch(format!(
            "The host key for {host}:{port} does NOT match the one on record. This may mean \
             someone is intercepting the connection (MITM), or the server's host key was \
             legitimately regenerated. Refusing to connect. If the change is expected, remove \
             the stale entry for {host} from your known_hosts file and reconnect."
        ))),
        CheckResult::NotFound => {
            // Trust-on-first-use: never seen this host before. Pin the key
            // now so any *future* connection takes the Mismatch branch
            // above if the key ever changes.
            log::warn!(
                "zap_sftp: host key for {host}:{port} not found in known_hosts; trusting on \
                 first use and pinning it for future connections"
            );
            if let Some(path) = known_hosts_path {
                let fmt = ssh2::KnownHostKeyFormat::from(key_type);
                if let Err(e) = known_hosts.add(host, key, host, fmt) {
                    log::warn!("zap_sftp: failed to record known_hosts entry for {host}: {e}");
                    return Ok(());
                }
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = known_hosts.write_file(path, KnownHostFileKind::OpenSSH) {
                    log::warn!(
                        "zap_sftp: failed to persist known_hosts file {}: {e}",
                        path.display()
                    );
                }
            }
            Ok(())
        }
        CheckResult::Failure => Err(SftpError::ConnectionFailed(format!(
            "Failed to verify host key for {host}:{port} against known_hosts"
        ))),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
