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

/// The remedy named in the one [`mint_via_gcloud`] failure branch that an interactive login can
/// actually fix: `gcloud auth print-access-token` exiting non-zero.
///
/// Bound to a constant and interpolated into that message so the phrase cannot drift away from
/// the branch [`is_reauth_required`] uses it to identify. The other failures deliberately do not
/// carry it — a missing `gcloud` binary, an unusable token, or a malformed impersonation email
/// are all cases where opening a browser would be noise.
const REAUTH_HINT: &str = "Run `gcloud auth login`";

/// How long to wait before a second failure is allowed to open another browser window.
///
/// The Vertex `ServiceTargetResolver` runs on *every* request and a failing turn may retry, so
/// without this an outage spawns a login tab per attempt.
const LOGIN_LAUNCH_DEBOUNCE: Duration = Duration::from_secs(120);

static LAST_LOGIN_LAUNCH: Mutex<Option<Instant>> = Mutex::new(None);

/// Whether `err` -- an error string from [`access_token`] -- describes a failure that signing in
/// again could plausibly fix.
///
/// This matches the branch where `gcloud auth print-access-token` exited non-zero, which covers
/// absent credentials but also expired ones, an unset project, and denied impersonation. That is
/// the right set: it is exactly the set whose message already tells the user to sign in.
pub fn is_reauth_required(err: &str) -> bool {
    err.contains(REAUTH_HINT)
}

/// Whether enough time has passed since `last` to open another login window.
fn login_launch_allowed(last: Option<Instant>) -> bool {
    !last.is_some_and(|at| at.elapsed() < LOGIN_LAUNCH_DEBOUNCE)
}

/// Starts the interactive login flow for a failed [`access_token`] call, if `err` is one this can
/// fix and one hasn't just been started.
///
/// Returns whether a browser was opened, so the caller can say so in the error the user sees --
/// a browser appearing with no explanation is worse than the original failure.
pub fn maybe_launch_login_for_error(err: &str) -> bool {
    if !is_reauth_required(err) {
        return false;
    }

    {
        let mut last = match LAST_LOGIN_LAUNCH.lock() {
            Ok(last) => last,
            // A poisoned lock here would mean a previous caller panicked mid-check. Declining to
            // launch is the safe read: the cost of a missed login prompt is an error message the
            // user can act on, the cost of a wrong one is unsolicited browser windows.
            Err(_) => return false,
        };
        if !login_launch_allowed(*last) {
            return false;
        }
        *last = Some(Instant::now());
    }

    match launch_gcloud_login() {
        Ok(()) => {
            log::info!("[byop-vertex] no usable credentials; launched `gcloud auth login`");
            true
        }
        Err(e) => {
            log::warn!("[byop-vertex] could not launch `gcloud auth login`: {e}");
            false
        }
    }
}

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

    // Defense-in-depth: `credential` came from Settings -> AI -> Vertex provider config
    // (persisted as an ordinary secret string, no schema enforcement) and is about to be
    // forwarded verbatim into a `gcloud` argv entry. It isn't attacker-controlled input in
    // the usual sense (no other user reaches this path), but a garbled/pasted-wrong value
    // should fail here with a clear message rather than being handed to `gcloud
    // --impersonate-service-account=<value>` and producing a confusing shell/gcloud-side
    // error instead.
    if !credential.is_empty() && !is_plausible_service_account_email(&credential) {
        return Err(format!(
            "invalid Vertex impersonation service-account email {credential:?} — expected \
             the form name@project-id.iam.gserviceaccount.com; leave the field empty to use \
             the active gcloud account / Application Default Credentials instead"
        ));
    }

    if let Some(token) = cached_token(&credential) {
        // Which credential went out is otherwise unrecoverable after the fact: a cached token
        // is reused for up to TOKEN_TTL, so a bad one keeps failing with no record of where it
        // came from.
        log::info!(
            "[byop-vertex] access token: cache=hit source={} shape={}",
            if credential.is_empty() {
                "active gcloud account / ADC"
            } else {
                "impersonated service account"
            },
            token_shape(&token)
        );
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

/// A loose `local-part@domain` shape check for a service-account email, not full RFC 5321
/// validation (GCP's own API is the real authority on whether the SA exists/is impersonable).
/// Rejects: empty local/domain parts, no `@` or more than one `@`, a domain with no dot (SA
/// emails are always under a real domain, e.g. `*.iam.gserviceaccount.com`), a domain starting
/// or ending with `.`, and any whitespace/control character anywhere (in particular `\n`/`\r`,
/// which must never reach a subprocess argument unexamined).
fn is_plausible_service_account_email(email: &str) -> bool {
    if email.is_empty() || email.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    if email.matches('@').count() != 1 {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && !domain.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
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
            "`gcloud auth print-access-token` failed ({}). {REAUTH_HINT} (or `gcloud \
             auth application-default login`) and confirm the project is set. Details: {}",
            output.status,
            stderr.trim()
        ));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let token = raw.trim().to_string();
    if token.is_empty() {
        return Err("gcloud returned an empty access token".to_owned());
    }
    // An OAuth2 access token is a single opaque word. `gcloud` occasionally writes advisory
    // lines to *stdout* alongside it (component-update notices, survey prompts), and taking
    // all of stdout verbatim then sends that blob as the bearer. Vertex answers with a 401
    // `ACCESS_TOKEN_TYPE_UNSUPPORTED` — "Expected OAuth 2 access token, login cookie or other
    // valid authentication credential" — which names neither gcloud nor the extra output, so
    // the real cause is invisible from the error alone. Observed twice on 2026-08-14 with a
    // CLI-minted token succeeding against the same endpoint seconds later.
    //
    // Rejecting here converts that into a local, self-describing failure. It cannot reject a
    // valid token: access tokens contain no whitespace.
    if token.split_whitespace().count() != 1 {
        return Err(format!(
            "`gcloud auth print-access-token` wrote {} whitespace-separated words to stdout; an \
             access token is a single word, so this output is not usable as a bearer token. \
             gcloud most likely printed an advisory notice alongside it — run `gcloud components \
             update` (or `gcloud config set survey/disable_prompts true`) and retry",
            token.split_whitespace().count()
        ));
    }
    log::info!(
        "[byop-vertex] minted access token via gcloud: shape={} len={}",
        token_shape(&token),
        token.len()
    );
    Ok(token)
}

/// Describes a token for logging **without ever emitting its value**.
///
/// The distinction matters because Vertex's 401 for a wrong-*type* credential
/// (`ACCESS_TOKEN_TYPE_UNSUPPORTED`) reads identically whether it was handed an identity
/// token, an API key, or a truncated string — so the log has to say which one went out.
fn token_shape(token: &str) -> &'static str {
    if token.starts_with("ya29.") {
        "ya29 (oauth2 access token)"
    } else if token.starts_with("eyJ") {
        "JWT (identity token, NOT an access token)"
    } else if token.starts_with("AIza") {
        "AIza (api key, NOT an access token)"
    } else {
        "unrecognized"
    }
}

#[cfg(test)]
mod token_shape_tests {
    use super::token_shape;

    #[test]
    fn classifies_the_credential_kinds_that_produce_indistinguishable_401s() {
        assert_eq!(token_shape("ya29.a0AfB_abc"), "ya29 (oauth2 access token)");
        assert_eq!(
            token_shape("eyJhbGciOiJSUzI1NiJ9.payload.sig"),
            "JWT (identity token, NOT an access token)"
        );
        assert_eq!(
            token_shape("AIzaSyExampleKeyValue"),
            "AIza (api key, NOT an access token)"
        );
        assert_eq!(token_shape("something-else"), "unrecognized");
    }
}

#[cfg(test)]
mod service_account_email_tests {
    use super::*;

    #[test]
    fn accepts_typical_gcp_service_account_emails() {
        assert!(is_plausible_service_account_email(
            "my-sa@my-project-123.iam.gserviceaccount.com"
        ));
        assert!(is_plausible_service_account_email(
            "deploy@internal-tools.example.com"
        ));
    }

    #[test]
    fn rejects_missing_or_doubled_at() {
        assert!(!is_plausible_service_account_email("not-an-email"));
        assert!(!is_plausible_service_account_email("a@b@c.com"));
    }

    #[test]
    fn rejects_empty_local_or_domain_part() {
        assert!(!is_plausible_service_account_email("@example.com"));
        assert!(!is_plausible_service_account_email("sa@"));
        assert!(!is_plausible_service_account_email(""));
    }

    #[test]
    fn rejects_domain_without_a_dot() {
        assert!(!is_plausible_service_account_email("sa@localhost"));
    }

    #[test]
    fn rejects_domain_with_leading_or_trailing_dot() {
        assert!(!is_plausible_service_account_email("sa@.example.com"));
        assert!(!is_plausible_service_account_email("sa@example.com."));
    }

    #[test]
    fn rejects_embedded_control_and_whitespace_characters() {
        // The concrete risk this guards against: a value that would smuggle extra bytes into
        // the `gcloud --impersonate-service-account=<value>` subprocess argument.
        assert!(!is_plausible_service_account_email("sa@example.com\n--verbosity=debug"));
        assert!(!is_plausible_service_account_email("sa@example.com\r"));
        assert!(!is_plausible_service_account_email("sa @example.com"));
        assert!(!is_plausible_service_account_email("sa@exa mple.com"));
    }

    #[tokio::test]
    async fn access_token_rejects_a_malformed_credential_before_touching_gcloud() {
        let err = access_token("not-an-email")
            .await
            .expect_err("a malformed SA email must be rejected up front");
        assert!(
            err.contains("invalid Vertex impersonation service-account email"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn access_token_rejects_a_credential_with_an_embedded_newline() {
        let err = access_token("sa@example.com\n--extra-flag")
            .await
            .expect_err("a credential with an embedded newline must be rejected up front");
        assert!(
            err.contains("invalid Vertex impersonation service-account email"),
            "{err}"
        );
    }
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

    /// The whole point of `REAUTH_HINT` being a constant is that this branch's message keeps
    /// carrying it. If someone rewrites the message without it, the connector silently stops
    /// offering to sign the user in -- no compile error, no other failing test.
    #[test]
    fn print_access_token_failure_is_classified_as_reauth() {
        let err = format!(
            "`gcloud auth print-access-token` failed (exit status: 1). {REAUTH_HINT} (or `gcloud \
             auth application-default login`) and confirm the project is set. Details: ERROR: \
             (gcloud.auth.print-access-token) Your current active account does not have any \
             valid credentials"
        );
        assert!(is_reauth_required(&err));
    }

    /// The other failure branches must not open a browser: signing in fixes none of them, and an
    /// unsolicited login window is worse than the error the user already has.
    #[test]
    fn other_failures_are_not_classified_as_reauth() {
        for err in [
            // gcloud absent -- launching it would fail the same way.
            "failed to run `gcloud auth print-access-token` — is the gcloud CLI installed and on \
             PATH? (No such file or directory (os error 2))",
            "gcloud returned an empty access token",
            // A token exists; it just has advisory noise around it.
            "`gcloud auth print-access-token` wrote 4 whitespace-separated words to stdout; an \
             access token is a single word, so this output is not usable as a bearer token.",
            "invalid Vertex impersonation service-account email \"nope\" — expected the form \
             name@project-id.iam.gserviceaccount.com",
        ] {
            assert!(!is_reauth_required(err), "should not offer login for: {err}");
        }
    }

    #[test]
    fn login_launch_is_debounced() {
        assert!(
            login_launch_allowed(None),
            "the first failure should be able to launch"
        );
        assert!(
            !login_launch_allowed(Some(Instant::now())),
            "a launch that just happened should suppress the next one"
        );

        let long_ago = Instant::now()
            .checked_sub(LOGIN_LAUNCH_DEBOUNCE * 2)
            .expect("clock should support subtracting the debounce window");
        assert!(
            login_launch_allowed(Some(long_ago)),
            "a launch older than the debounce window should not suppress"
        );
    }
}
