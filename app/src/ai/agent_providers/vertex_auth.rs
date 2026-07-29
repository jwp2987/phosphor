//! Vertex AI OAuth2 bearer-token minting for BYOP.
//!
//! Unlike every other BYOP provider type, Vertex does not use a static API key — each request
//! carries a short-lived GCP OAuth2 access token (~1h). We mint that token via the `gcloud` CLI,
//! which already handles the messy parts of GCP auth: Application Default Credentials, the active
//! logged-in account, and service-account impersonation. The token is cached in-process so we
//! don't spawn a subprocess on every conversation turn, and refreshed well before expiry.
//!
//! The single public entry point is [`access_token`]. It is called from the async
//! `ServiceTargetResolver` in `chat_stream::build_client_uncached` for `AgentProviderApiType::
//! Vertex`, so the token is fetched lazily per request while the underlying reqwest client
//! (connection pool) stays cached.
//!
//! Follow-up: for headless environments without the `gcloud` CLI, this is the natural place to
//! add a `gcp_auth`-based path that mints directly from a service-account JSON stored in
//! `AgentProviderSecrets`.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// gcloud access tokens are valid for roughly one hour. We refresh well before that to stay safe
/// against clock skew and long streaming turns, while still avoiding a `gcloud` subprocess on
/// every turn.
const TOKEN_TTL: Duration = Duration::from_secs(30 * 60);

/// Small cap on distinct credentials cached at once (a user rarely configures more than a couple
/// of Vertex providers).
const TOKEN_CACHE_CAP: usize = 8;

struct CachedToken {
    /// The credential key this token was minted for (empty = ADC / active account).
    credential: String,
    token: String,
    fetched_at: Instant,
}

static TOKEN_CACHE: Mutex<Vec<CachedToken>> = Mutex::new(Vec::new());

/// Serializes token minting so that concurrent cold-start callers (main chat +
/// title-gen + active-AI all firing on the first turn) don't each spawn a
/// separate `gcloud` subprocess. The window this guards is tiny and rare (once
/// per credential per ~30 min), so a single global mint lock is fine — the first
/// waiter to mint populates the cache for the rest.
static MINT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Returns a valid GCP OAuth2 access token for the given credential, minting a fresh one via
/// `gcloud` when the cache is empty or stale.
///
/// `credential` selects how the token is obtained:
/// - empty → the active `gcloud` account / Application Default Credentials.
/// - non-empty → treated as a service-account email to impersonate
///   (`--impersonate-service-account=<email>`); requires the active account to hold the
///   Service Account Token Creator role on that SA.
pub async fn access_token(credential: &str) -> Result<String, String> {
    let credential = credential.trim().to_string();

    if let Some(token) = cached_token(&credential) {
        return Ok(token);
    }

    // Single-flight: only one mint runs at a time. Re-check the cache after
    // acquiring the lock, so callers that queued behind the first minter get its
    // freshly-cached token instead of spawning another `gcloud`.
    let _guard = MINT_LOCK.lock().await;
    if let Some(token) = cached_token(&credential) {
        return Ok(token);
    }

    let token = mint_via_gcloud(&credential).await?;
    store_token(&credential, &token);
    Ok(token)
}

fn cached_token(credential: &str) -> Option<String> {
    let cache = TOKEN_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .iter()
        .find(|c| c.credential == credential && c.fetched_at.elapsed() < TOKEN_TTL)
        .map(|c| c.token.clone())
}

fn store_token(credential: &str, token: &str) {
    let mut cache = TOKEN_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.retain(|c| c.credential != credential);
    cache.insert(
        0,
        CachedToken {
            credential: credential.to_owned(),
            token: token.to_owned(),
            fetched_at: Instant::now(),
        },
    );
    cache.truncate(TOKEN_CACHE_CAP);
}

/// Resolves the `gcloud` binary to invoke, searching common install locations beyond the
/// inherited `PATH` on Unix.
///
/// A GUI app launched from Finder/Dock (or a Linux `.desktop` launcher) only inherits the
/// system PATH, not whatever a user's shell rc file adds -- the same root cause documented on
/// `cli_agent_search_dirs` in `terminal/cli_agent.rs`. `gcloud` is commonly installed either by
/// Homebrew (which does land on the inherited PATH via `/opt/homebrew`/`/usr/local`, but only
/// if those happen to already be in scope for a non-shell-launched process) or by the official
/// Google Cloud SDK installer, which puts it under the user's home directory and relies on a
/// shell rc file to add it to PATH -- exactly the case a GUI-launched process never sees.
#[cfg(unix)]
fn resolve_gcloud_path() -> std::path::PathBuf {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("gcloud");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    let mut candidates = vec![
        std::path::PathBuf::from("/opt/homebrew/bin/gcloud"),
        std::path::PathBuf::from("/usr/local/bin/gcloud"),
        std::path::PathBuf::from("/usr/local/google-cloud-sdk/bin/gcloud"),
        std::path::PathBuf::from("/snap/bin/gcloud"),
    ];
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        candidates.push(home.join("google-cloud-sdk/bin/gcloud"));
        candidates.push(home.join(".google-cloud-sdk/bin/gcloud"));
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        // Fall back to the bare command name so the resulting error message still names
        // `gcloud` (matching the pre-existing "is it installed and on PATH?" message) instead
        // of a nonsensical absolute path that was never found.
        .unwrap_or_else(|| std::path::PathBuf::from("gcloud"))
}

#[cfg(not(unix))]
fn resolve_gcloud_path() -> std::path::PathBuf {
    std::path::PathBuf::from("gcloud")
}

/// Spawns `gcloud auth login`, which opens the system browser for an interactive OAuth
/// consent flow. Fire-and-forget: this only reports whether the process could be *launched* --
/// the login flow itself completes asynchronously in the user's browser, independent of this
/// app's lifetime. Once it completes, the next [`access_token`] call mints fresh normally.
pub fn launch_gcloud_login() -> Result<(), String> {
    let gcloud_path = resolve_gcloud_path();
    command::r#async::Command::new(&gcloud_path)
        .arg("auth")
        .arg("login")
        .spawn()
        .map(|_child| ())
        .map_err(|e| {
            format!(
                "failed to launch `gcloud auth login` — is the gcloud CLI installed and on PATH? ({e})"
            )
        })
}

async fn mint_via_gcloud(credential: &str) -> Result<String, String> {
    let gcloud_path = resolve_gcloud_path();
    let mut cmd = command::r#async::Command::new(&gcloud_path);
    cmd.arg("auth").arg("print-access-token");
    if !credential.is_empty() {
        cmd.arg(format!("--impersonate-service-account={credential}"));
    }

    let output = cmd.output().await.map_err(|e| {
        format!(
            "failed to run `gcloud auth print-access-token` — is the gcloud CLI installed and on \
             PATH? ({e})"
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`gcloud auth print-access-token` failed ({}). Run `gcloud auth login` (or `gcloud \
             auth application-default login`) and confirm the project is set. Details: {}",
            output.status,
            stderr.trim()
        ));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("gcloud returned an empty access token".to_owned());
    }
    Ok(token)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::LazyLock;

    /// Guards process-wide PATH/HOME env-var mutation for the duration of a test, restoring
    /// the original values on drop (including on panic/unwind, since `#[serial]` alone doesn't
    /// protect against a mutated env leaking into whichever test runs next in this process).
    struct EnvGuard {
        path: Option<std::ffi::OsString>,
        home: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(path: &std::path::Path, home: &std::path::Path) -> Self {
            let guard = Self {
                path: std::env::var_os("PATH"),
                home: std::env::var_os("HOME"),
            };
            // SAFETY: serialized via #[serial] -- no other test in this process observes PATH
            // or HOME while this guard is alive.
            unsafe {
                std::env::set_var("PATH", path);
                std::env::set_var("HOME", home);
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: see EnvGuard::set.
            unsafe {
                match &self.path {
                    Some(v) => std::env::set_var("PATH", v),
                    None => std::env::remove_var("PATH"),
                }
                match &self.home {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }

    static ENV_LOCK: LazyLock<std::sync::Mutex<()>> = LazyLock::new(|| std::sync::Mutex::new(()));

    #[test]
    #[serial]
    fn resolve_gcloud_path_falls_back_to_bare_command_when_nowhere_to_be_found() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let empty_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set(empty_dir.path(), empty_dir.path());

        assert_eq!(resolve_gcloud_path(), std::path::PathBuf::from("gcloud"));
    }

    #[test]
    #[serial]
    fn resolve_gcloud_path_finds_official_installer_default_location() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let home_dir = tempfile::tempdir().unwrap();
        let sdk_bin = home_dir.path().join("google-cloud-sdk").join("bin");
        std::fs::create_dir_all(&sdk_bin).unwrap();
        let gcloud_path = sdk_bin.join("gcloud");
        std::fs::write(&gcloud_path, "#!/bin/sh\n").unwrap();

        // An empty PATH means the search must fall through to the HOME-relative candidates.
        let empty_path_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set(empty_path_dir.path(), home_dir.path());

        assert_eq!(resolve_gcloud_path(), gcloud_path);
    }

    #[test]
    #[serial]
    fn resolve_gcloud_path_prefers_path_over_home_fallback() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let path_dir = tempfile::tempdir().unwrap();
        let on_path = path_dir.path().join("gcloud");
        std::fs::write(&on_path, "#!/bin/sh\n").unwrap();

        // Also plant one at the HOME-relative fallback location, to confirm PATH wins.
        let home_dir = tempfile::tempdir().unwrap();
        let sdk_bin = home_dir.path().join("google-cloud-sdk").join("bin");
        std::fs::create_dir_all(&sdk_bin).unwrap();
        std::fs::write(sdk_bin.join("gcloud"), "#!/bin/sh\n").unwrap();

        let _guard = EnvGuard::set(path_dir.path(), home_dir.path());

        assert_eq!(resolve_gcloud_path(), on_path);
    }
}
