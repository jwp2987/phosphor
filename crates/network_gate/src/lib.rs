//! The runtime network gate.
//!
//! One place decides whether the app may send a request: [`check_url`]. One place builds the
//! HTTP clients that make requests: [`builder`] (`http_client::Client::for_purpose`
//! is built on it). `script/check_network_gate` fails the build of any other construction site.
//!
//! The idea (a runtime allowlist behind every outbound request, not a promise that call sites
//! behave) comes from Warp forks that added a "network lockdown"; this is our own design.
//!
//! # Rules
//!
//! - Every request carries a [`Purpose`]. A purpose is a name plus a [`Kind`].
//! - A host passes when it is (1) a loopback IP literal, (2) an origin the user configured
//!   (BYOP providers, the local proxy, Ollama, the models.dev URL, an MCP server) or added
//!   under Settings > Network > allowed origins, (3) a host the purpose's `Kind::UserAction`
//!   names (the explicit user actions that need the network), or (4) any public host for
//!   `Kind::PublicWeb` (the agent's web tools, which the user enabled).
//! - `Kind::Automatic` (background work, an unclassified client) passes only (1) and (2).
//! - A name that is loopback by name (`localhost`, `*.localhost`) is refused unless it is
//!   explicitly configured, and a name that RESOLVES to loopback is refused by the resolver
//!   in [`builder`] unless it is explicitly configured (DNS rebinding).
//! - Every redirect hop is checked again, against the same purpose.
//! - System proxy environment variables are ignored. Only a proxy the user configured in
//!   Phosphor settings is used ([`apply_proxy_policy`]).
//! - A denial never carries the URL: only the purpose and a hash of the host
//!   ([`Denied`]). Logs and the UI show that, nothing else.
//!
//! # Settings
//!
//! The app pushes the configured origins with [`set_configured_origins`] and the user's
//! allowed-origin list with [`set_user_allowed_origins`] (the same "settings -> global slot"
//! pattern as the proxy config). A denial is also published to a listener
//! ([`set_denial_listener`]) so the UI can say what was blocked and offer the setting.

use std::collections::BTreeSet;
use std::fmt;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// How a request relates to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Background / automatic work, or a client nobody classified. Allowlist only.
    Automatic,
    /// An explicit user action (a button, a command). Allowlist plus these fixed hosts; a
    /// host matches when it equals an entry or is a subdomain of it.
    UserAction(&'static [&'static str]),
    /// The URL is the user's own configuration for this feature (an MCP server they typed).
    /// Any host passes, including `localhost`; redirects are still re-checked against the
    /// allowlist.
    UserConfigured,
    /// The agent's web tools (webfetch / websearch), enabled by the user. Any public host.
    PublicWeb,
    /// Test clients. Nothing is checked. Only `Client::new_for_test` uses it.
    Unrestricted,
}

/// What a request is for. Constants live at the bottom of this module so the whole list of
/// network uses is in one file.
#[derive(Debug, PartialEq, Eq)]
pub struct Purpose {
    pub name: &'static str,
    pub kind: Kind,
}

/// A request the gate refused. Display shows the purpose and a host hash, never the URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Denied {
    pub purpose: &'static str,
    pub host_hash: u64,
}

impl fmt::Display for Denied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "network gate denied a request for \"{}\" (host {:016x}); add its origin under \
             Settings > Network > Allowed origins if you want it",
            self.purpose, self.host_hash
        )
    }
}

impl std::error::Error for Denied {}

/// FNV-1a 64: stable across runs and platforms, so a host hash in a log can be compared with
/// `hash_host` output for a host the user already knows about.
pub fn hash_host(host: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in host.to_ascii_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// What the gate decided for one request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Allowed,
    Denied,
    /// The rules would have refused it; the user turned the gate off, so it went through.
    AllowedGateOff,
}

/// One gate decision, for the Settings > Network log. `host` is for the local UI only (so the
/// user can press Allow); `Debug` and `Display` never print it, and it is never logged,
/// serialised or uploaded. Only loopback-free allowed requests and denials are recorded.
#[derive(Clone, PartialEq, Eq)]
pub struct GateEvent {
    pub seq: u64,
    pub purpose: &'static str,
    /// Empty when the request had no usable host (unparseable URL, redirect cap).
    pub host: String,
    pub port: Option<u16>,
    pub host_hash: u64,
    pub outcome: Outcome,
}

impl fmt::Debug for GateEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GateEvent")
            .field("seq", &self.seq)
            .field("purpose", &self.purpose)
            .field("host_hash", &format_args!("{:016x}", self.host_hash))
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for GateEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "#{} {:?} \"{}\" (host {:016x})",
            self.seq, self.outcome, self.purpose, self.host_hash
        )
    }
}

type EventListener = Arc<dyn Fn(&GateEvent) + Send + Sync>;

/// `host` (lowercased, no brackets) plus an optional port. `port: None` matches any port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Origin {
    host: String,
    port: Option<u16>,
}

impl Origin {
    /// Accepts `https://host:port/path`, `host:port` or `host`. A URL without an explicit
    /// port gets its scheme's default; a bare host matches any port.
    fn parse(text: &str) -> Option<Origin> {
        let t = text.trim();
        if t.is_empty() {
            return None;
        }
        let with_scheme = if t.contains("://") {
            t.to_owned()
        } else {
            format!("https://{t}")
        };
        let url = reqwest::Url::parse(&with_scheme).ok()?;
        let host = url
            .host_str()?
            .trim_matches(['[', ']'])
            .to_ascii_lowercase();
        let port = if t.contains("://") {
            url.port_or_known_default()
        } else {
            // `host:port` keeps its port; a bare host (default port after parsing) means any.
            url.port()
        };
        Some(Origin { host, port })
    }

    fn matches(&self, host: &str, port: Option<u16>) -> bool {
        self.host == host
            && match (self.port, port) {
                (None, _) | (_, None) => true,
                (Some(a), Some(b)) => a == b,
            }
    }
}

fn is_loopback_name(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost")
}

fn host_literal_ip(host: &str) -> Option<IpAddr> {
    host.trim_matches(['[', ']']).parse::<IpAddr>().ok()
}

/// Loopback, unspecified, link-local, private and multicast: what `PublicWeb` refuses.
fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_unspecified()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_multicast()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique local
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link local
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
                })
        }
    }
}

/// The allowlist. A process has one ([`global`]); tests make their own.
pub struct Gate {
    configured: RwLock<BTreeSet<Origin>>,
    user_allowed: RwLock<BTreeSet<Origin>>,
    /// The user's master switch (Settings > Network). On by default.
    enabled: AtomicBool,
    seq: AtomicU64,
    events: RwLock<Option<EventListener>>,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            configured: RwLock::default(),
            user_allowed: RwLock::default(),
            enabled: AtomicBool::new(true),
            seq: AtomicU64::new(0),
            events: RwLock::default(),
        }
    }
}

impl Gate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Turns the gate on or off. Off: every check returns `Ok` (the redirect hop cap stays).
    /// Proxy rules are not affected. The change is logged once.
    pub fn set_enabled(&self, on: bool) {
        if self.enabled.swap(on, Ordering::SeqCst) != on {
            log::warn!("network gate: turned {}", if on { "ON" } else { "OFF" });
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Installs the function told about every recorded decision on this gate.
    pub fn set_event_listener(&self, listener: impl Fn(&GateEvent) + Send + Sync + 'static) {
        if let Ok(mut g) = self.events.write() {
            *g = Some(Arc::new(listener));
        }
    }

    fn emit(&self, url: &str, purpose: &'static str, outcome: Outcome, denied: Option<&Denied>) {
        let (host, port) = match reqwest::Url::parse(url) {
            Ok(u) => (
                u.host_str()
                    .map(|h| h.trim_matches(['[', ']']).to_ascii_lowercase())
                    .unwrap_or_default(),
                u.port_or_known_default(),
            ),
            Err(_) => (String::new(), None),
        };
        // The redirect cap says nothing about the target: keep the host out of the event so
        // the UI offers no Allow for it.
        let (host, port) = match denied {
            Some(d) if d.host_hash == 0 => (String::new(), None),
            _ => (host, port),
        };
        // Loopback traffic that was allowed is not interesting (the app talks to itself).
        let loopback = host_literal_ip(&host).is_some_and(|ip| ip.is_loopback());
        if outcome == Outcome::Allowed && (loopback || is_loopback_name(&host)) {
            return;
        }
        let host_hash = denied.map_or_else(|| hash_host(&host), |d| d.host_hash);
        let listener = self.events.read().ok().and_then(|g| g.clone());
        if let Some(l) = listener {
            l(&GateEvent {
                seq: self.seq.fetch_add(1, Ordering::SeqCst),
                purpose,
                host,
                port,
                host_hash,
                outcome,
            });
        }
    }

    /// Applies the switch and records the decision.
    fn finish(
        &self,
        url: &str,
        purpose: &'static Purpose,
        result: Result<(), Denied>,
    ) -> Result<(), Denied> {
        match result {
            Ok(()) => {
                self.emit(url, purpose.name, Outcome::Allowed, None);
                Ok(())
            }
            Err(d) if !self.is_enabled() && d.host_hash != 0 => {
                log::debug!(
                    "network gate (off): passed \"{}\" (host hash {:016x})",
                    d.purpose,
                    d.host_hash
                );
                self.emit(url, purpose.name, Outcome::AllowedGateOff, Some(&d));
                Ok(())
            }
            Err(d) => {
                self.emit(url, purpose.name, Outcome::Denied, Some(&d));
                Err(d)
            }
        }
    }

    /// Replaces the origins the user configured as endpoints (providers, proxy, MCP, URLs).
    /// Unparseable entries are skipped.
    pub fn set_configured_origins<I, S>(&self, urls: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let set: BTreeSet<Origin> = urls
            .into_iter()
            .filter_map(|u| Origin::parse(u.as_ref()))
            .collect();
        if let Ok(mut g) = self.configured.write() {
            *g = set;
        }
    }

    /// Replaces the user's allowed-origin list from its settings text (comma, whitespace or
    /// newline separated).
    pub fn set_user_allowed_origins(&self, text: &str) {
        let set: BTreeSet<Origin> = text
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter_map(Origin::parse)
            .collect();
        if let Ok(mut g) = self.user_allowed.write() {
            *g = set;
        }
    }

    fn explicit(&self, host: &str, port: Option<u16>) -> bool {
        let in_set = |lock: &RwLock<BTreeSet<Origin>>| {
            lock.read()
                .map(|g| g.iter().any(|o| o.matches(host, port)))
                .unwrap_or(false)
        };
        in_set(&self.configured) || in_set(&self.user_allowed)
    }

    /// Whether `host` was configured by the user (used by the resolver to let a configured
    /// `localhost` through).
    pub fn is_explicit_host(&self, host: &str) -> bool {
        self.explicit(&host.to_ascii_lowercase(), None)
    }

    /// The single decision. The URL is only ever parsed here; it is never stored or echoed.
    pub fn check_url(&self, url: &str, purpose: &'static Purpose) -> Result<(), Denied> {
        if purpose.kind == Kind::Unrestricted {
            return Ok(());
        }
        let result = self.decide_url(url, purpose);
        self.finish(url, purpose, result)
    }

    /// The rules, without the switch or the event.
    fn decide_url(&self, url: &str, purpose: &'static Purpose) -> Result<(), Denied> {
        if purpose.kind == Kind::Unrestricted {
            return Ok(());
        }
        let denied = |host: &str| Denied {
            purpose: purpose.name,
            host_hash: hash_host(host),
        };
        let Ok(parsed) = reqwest::Url::parse(url) else {
            return Err(denied(""));
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(denied(parsed.host_str().unwrap_or("")));
        }
        let Some(host) = parsed.host_str() else {
            return Err(denied(""));
        };
        let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
        let port = parsed.port_or_known_default();

        // The URL is the user's own configuration for this feature: explicit by construction,
        // `localhost` included. Only its redirects are held to the allowlist.
        if purpose.kind == Kind::UserConfigured {
            return Ok(());
        }
        // (1) loopback IP literal.
        if let Some(ip) = host_literal_ip(&host) {
            if ip.is_loopback() {
                return Ok(());
            }
        }
        // (2) configured / user-allowed origin (this is how `localhost` gets through).
        if self.explicit(&host, port) {
            return Ok(());
        }
        // Loopback by name is refused from here on.
        if is_loopback_name(&host) {
            return Err(denied(&host));
        }
        match purpose.kind {
            Kind::Automatic | Kind::Unrestricted => Err(denied(&host)),
            Kind::UserAction(hosts) => {
                let hit = hosts
                    .iter()
                    .any(|h| host == *h || host.ends_with(&format!(".{h}")));
                if hit { Ok(()) } else { Err(denied(&host)) }
            }
            Kind::UserConfigured => Ok(()), // returned above
            Kind::PublicWeb => match host_literal_ip(&host) {
                Some(ip) if is_non_public_ip(ip) => Err(denied(&host)),
                _ => Ok(()),
            },
        }
    }

    /// The redirect decision for one hop. `UserConfigured` clients never follow a redirect
    /// to a host that is not explicitly allowed (their kind would otherwise let anything
    /// through), and every kind caps the chain.
    pub fn check_redirect(
        &self,
        url: &str,
        hops_so_far: usize,
        purpose: &'static Purpose,
    ) -> Result<(), Denied> {
        if hops_so_far >= MAX_REDIRECT_HOPS {
            // The cap stays even with the gate off.
            let d = Denied {
                purpose: purpose.name,
                host_hash: 0,
            };
            self.emit(url, purpose.name, Outcome::Denied, Some(&d));
            return Err(d);
        }
        if purpose.kind == Kind::Unrestricted {
            return Ok(());
        }
        let result = if purpose.kind == Kind::UserConfigured {
            let strict = Purpose {
                name: purpose.name,
                kind: Kind::Automatic,
            };
            // `Purpose` is not 'static here, so decide by hand with the Automatic rules.
            self.check_url_kind_automatic(url, &strict)
        } else {
            self.decide_url(url, purpose)
        };
        self.finish(url, purpose, result)
    }

    /// [`Self::check_redirect`] for a hop of the chain that began at `first_url`.
    ///
    /// A purpose whose requests carry a credential for the first URL's origin
    /// ([`redirects_stay_on_first_origin`]) follows a hop only to that origin. The allowlist
    /// alone is not enough for it: every configured origin is allowed, so provider B's 30x to
    /// provider A's host would hand A the key and custom headers meant for B (reqwest strips
    /// only `Authorization`, `Cookie` and `Proxy-Authorization` across hosts, not `x-api-key`,
    /// `api-key` or user headers). This rule protects a credential rather than limiting where
    /// the app may connect, so it holds with the gate switched off, and the event carries no
    /// host: allowing the target would not make the hop legal.
    pub fn check_redirect_from(
        &self,
        first_url: &str,
        url: &str,
        hops_so_far: usize,
        purpose: &'static Purpose,
    ) -> Result<(), Denied> {
        if redirects_stay_on_first_origin(purpose) && !same_origin_or_tls_upgrade(first_url, url) {
            log::warn!(
                "network gate: \"{}\" redirect leaves the origin of its first request; not followed",
                purpose.name
            );
            let d = Denied {
                purpose: purpose.name,
                host_hash: 0,
            };
            self.emit(url, purpose.name, Outcome::Denied, Some(&d));
            return Err(d);
        }
        self.check_redirect(url, hops_so_far, purpose)
    }

    fn check_url_kind_automatic(&self, url: &str, p: &Purpose) -> Result<(), Denied> {
        let denied = |host: &str| Denied {
            purpose: p.name,
            host_hash: hash_host(host),
        };
        let Ok(parsed) = reqwest::Url::parse(url) else {
            return Err(denied(""));
        };
        let Some(host) = parsed.host_str() else {
            return Err(denied(""));
        };
        let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
        if host_literal_ip(&host).is_some_and(|ip| ip.is_loopback())
            || self.explicit(&host, parsed.port_or_known_default())
        {
            Ok(())
        } else {
            Err(denied(&host))
        }
    }

    /// Resolver-side rule: a name that resolved to loopback (or an unspecified address) is
    /// refused unless the user configured that host.
    pub fn resolved_ok(&self, host: &str, addrs: &[IpAddr]) -> bool {
        if !self.is_enabled() {
            return true;
        }
        let loopbacky = addrs
            .iter()
            .any(|ip| ip.is_loopback() || ip.is_unspecified());
        !loopbacky || self.is_explicit_host(host)
    }
}

/// Maximum redirect hops, matching reqwest's default.
pub const MAX_REDIRECT_HOPS: usize = 10;

static GLOBAL: OnceLock<Gate> = OnceLock::new();

/// The process-wide gate.
pub fn global() -> &'static Gate {
    GLOBAL.get_or_init(Gate::new)
}

/// [`Gate::check_url`] on the process-wide gate. Logs a denial (purpose and host hash only)
/// and tells the denial listener.
pub fn check_url(url: &str, purpose: &'static Purpose) -> Result<(), Denied> {
    let result = global().check_url(url, purpose);
    if let Err(d) = &result {
        report_denial(d);
    }
    result
}

/// [`Gate::set_enabled`] on the process-wide gate.
pub fn set_enabled(on: bool) {
    global().set_enabled(on);
}

pub fn is_enabled() -> bool {
    global().is_enabled()
}

/// Installs the function told about every recorded decision (the Settings > Network log).
pub fn set_event_listener(listener: impl Fn(&GateEvent) + Send + Sync + 'static) {
    global().set_event_listener(listener);
}

pub fn set_configured_origins<I, S>(urls: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    global().set_configured_origins(urls);
}

pub fn set_user_allowed_origins(text: &str) {
    global().set_user_allowed_origins(text);
}

type DenialListener = Arc<dyn Fn(&Denied) + Send + Sync>;
static LISTENER: OnceLock<RwLock<Option<DenialListener>>> = OnceLock::new();

/// Installs the function told about every denial (the UI toast). It receives the purpose and
/// host hash, never a URL.
pub fn set_denial_listener(listener: impl Fn(&Denied) + Send + Sync + 'static) {
    let slot = LISTENER.get_or_init(|| RwLock::new(None));
    if let Ok(mut g) = slot.write() {
        *g = Some(Arc::new(listener));
    }
}

/// Whether `purpose`'s requests carry a credential that belongs to the origin of the first
/// request (the provider API key and the user's custom auth headers on [`AI_ENDPOINT`]), so a
/// redirect may not leave that origin.
fn redirects_stay_on_first_origin(purpose: &Purpose) -> bool {
    purpose.name == AI_ENDPOINT.name
}

/// Whether `hop` is the same host and port as `first`, or the same host moving from `http` to
/// `https` (the one port change that sends the credential to the same service over a safer
/// channel). Anything unparseable is not the same origin.
fn same_origin_or_tls_upgrade(first: &str, hop: &str) -> bool {
    let (Ok(first), Ok(hop)) = (reqwest::Url::parse(first), reqwest::Url::parse(hop)) else {
        return false;
    };
    let (Some(first_host), Some(hop_host)) = (first.host_str(), hop.host_str()) else {
        return false;
    };
    if !first_host.eq_ignore_ascii_case(hop_host) {
        return false;
    }
    let tls_upgrade = first.scheme() == "http"
        && hop.scheme() == "https"
        && first.port().is_none()
        && hop.port().is_none();
    tls_upgrade
        || (first.scheme() == hop.scheme()
            && first.port_or_known_default() == hop.port_or_known_default())
}

fn report_denial(d: &Denied) {
    log::warn!(
        "network gate: denied \"{}\" (host hash {:016x})",
        d.purpose,
        d.host_hash
    );
    let listener = LISTENER
        .get()
        .and_then(|s| s.read().ok().and_then(|g| g.clone()));
    if let Some(l) = listener {
        l(d);
    }
}

/// The error a denied request returns. `reqwest::Error` cannot be built outside reqwest, so
/// this is the builder error of an unparseable URL: it carries no URL, and the real reason is
/// in the log and the denial listener.
pub fn denied_error() -> reqwest::Error {
    // `IntoUrl::into_url` is sealed, so build (never send) a request to a scheme-less URL:
    // `RequestBuilder::build` reports the parse failure as a builder error.
    match reqwest::Client::new()
        .get("//denied-by-network-gate")
        .build()
    {
        Err(e) => e.without_url(),
        // Cannot happen: a scheme-less URL does not parse. Keep the type total.
        Ok(_) => unreachable!("scheme-less URL parsed"),
    }
}

/// Applies a proxy to a client builder (the user's configured one).
pub type ProxyApplier = Arc<dyn Fn(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send + Sync>;

static PROXY_APPLIER: OnceLock<RwLock<Option<ProxyApplier>>> = OnceLock::new();

/// Installs (or clears) the proxy the user configured in Phosphor settings. `http_client`
/// calls this from `set_global_proxy_config`: `Some` only for `ProxyMode::Custom`.
pub fn set_configured_proxy(applier: Option<ProxyApplier>) {
    let slot = PROXY_APPLIER.get_or_init(|| RwLock::new(None));
    if let Ok(mut g) = slot.write() {
        *g = applier;
    }
}

/// Applies the gate's proxy policy: system proxy environment variables are IGNORED. Only a
/// proxy the user configured in Phosphor settings is used. The `System` proxy mode therefore
/// behaves like `Off` (owner decision, Wave N); the mode's name is kept so existing settings
/// files stay valid.
pub fn apply_proxy_policy(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    // `no_proxy()` first switches reqwest's environment / system detection off. The user's
    // proxy, if any, is added on top; the applier's fall-back paths (empty or invalid URL)
    // leave the builder as it is, i.e. with no proxy at all.
    let builder = builder.no_proxy();
    let applier = PROXY_APPLIER
        .get()
        .and_then(|s| s.read().ok().and_then(|g| g.clone()));
    match applier {
        Some(apply) => apply(builder),
        None => builder,
    }
}

/// The redirect policy: every hop is checked against the gate for `purpose`. A denied hop
/// STOPS the chain (the caller sees the 3xx response, not an error that would carry the URL).
pub fn redirect_policy(purpose: &'static Purpose) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        let hops = attempt.previous().len();
        let result = match attempt.previous().first() {
            Some(first) => {
                global().check_redirect_from(first.as_str(), attempt.url().as_str(), hops, purpose)
            }
            None => global().check_redirect(attempt.url().as_str(), hops, purpose),
        };
        match result {
            Ok(()) => attempt.follow(),
            Err(d) => {
                report_denial(&d);
                attempt.stop()
            }
        }
    })
}

/// A DNS resolver that refuses names resolving to loopback unless the user configured the
/// host. Not available on wasm (reqwest has no DNS hook there).
#[cfg(not(target_family = "wasm"))]
pub struct GateResolver;

#[cfg(not(target_family = "wasm"))]
impl reqwest::dns::Resolve for GateResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            use std::net::ToSocketAddrs;
            let lookup = host.clone();
            let addrs: Vec<std::net::SocketAddr> = tokio::task::spawn_blocking(
                move || -> std::io::Result<Vec<std::net::SocketAddr>> {
                    Ok((lookup.as_str(), 0).to_socket_addrs()?.collect())
                },
            )
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            let ips: Vec<IpAddr> = addrs.iter().map(|a| a.ip()).collect();
            if !global().resolved_ok(&host, &ips) {
                let d = Denied {
                    purpose: "dns",
                    host_hash: hash_host(&host),
                };
                report_denial(&d);
                return Err(Box::new(d) as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// THE client builder. Every reqwest client the app makes starts here (the guard enforces
/// it): proxy policy, redirect re-check and the loopback-by-name resolver are applied, plus
/// the HTTP/2 keep-alive settings the app has always used.
pub fn builder(purpose: &'static Purpose) -> reqwest::ClientBuilder {
    let mut b = reqwest::ClientBuilder::new();
    if purpose.kind == Kind::Unrestricted {
        // Test clients: no TLS roots, no proxy, no checks.
        return b.tls_certs_only([]).no_proxy();
    }
    #[cfg(not(target_family = "wasm"))]
    {
        b = b
            .http2_keep_alive_interval(std::time::Duration::from_secs(60))
            .http2_keep_alive_timeout(std::time::Duration::from_secs(15))
            .http2_keep_alive_while_idle(true)
            .dns_resolver(Arc::new(GateResolver));
        b = apply_proxy_policy(b);
    }
    b.redirect(redirect_policy(purpose))
}

/// Builds a finished `reqwest::Client` for `purpose`, for the transports (MCP, OAuth, the
/// agent's web tools) that need a raw reqwest client rather than [`crate::Client`].
pub fn reqwest_client(purpose: &'static Purpose) -> reqwest::Result<reqwest::Client> {
    builder(purpose).build()
}

/// A blocking client for the loopback-only callers (the local-control CLI talking to the
/// running app). Environment proxies are off and redirects are never followed; the caller
/// checks its URL with [`check_url`] first.
#[cfg(not(target_family = "wasm"))]
pub fn blocking_loopback_client() -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

// -- The purposes. Every network use in the app is one of these. ----------------------------

/// Hosts a user-clicked "check for updates" may reach.
pub const UPDATE_HOSTS: &[&str] = &["gitlab.com"];
/// models.dev catalog Refresh (the default URL is the repo copy on GitLab; a URL the user
/// set is in the configured origins).
pub const MODELS_DEV_HOSTS: &[&str] = &["gitlab.com", "models.dev"];
/// The Settings > Network "Test connection" probe.
pub const PROBE_HOSTS: &[&str] = &["www.google.com", "google.com"];
/// Language-server downloads the user asked for by opening a project that needs one.
pub const LSP_HOSTS: &[&str] = &[
    "api.github.com",
    "github.com",
    "githubusercontent.com",
    "registry.npmjs.org",
];

/// Unclassified client: allowlist only. `Client::new()` and `from_client_builder` use it.
pub static UNCLASSIFIED: Purpose = Purpose {
    name: "unclassified",
    kind: Kind::Automatic,
};
/// Chat, model-list and embeddings calls to the user's configured provider.
pub static AI_ENDPOINT: Purpose = Purpose {
    name: "ai-endpoint",
    kind: Kind::Automatic,
};
/// The models.dev catalog Refresh button, or the opt-in `models_dev_fetch` setting.
pub static MODELS_DEV: Purpose = Purpose {
    name: "models-dev",
    kind: Kind::UserAction(MODELS_DEV_HOSTS),
};
/// Release check / download the user started.
pub static UPDATE: Purpose = Purpose {
    name: "update",
    kind: Kind::UserAction(UPDATE_HOSTS),
};
/// Settings > Network "Test connection".
pub static NETWORK_PROBE: Purpose = Purpose {
    name: "network-probe",
    kind: Kind::UserAction(PROBE_HOSTS),
};
/// Language-server install.
pub static LSP_INSTALL: Purpose = Purpose {
    name: "lsp-install",
    kind: Kind::UserAction(LSP_HOSTS),
};
/// Remote images and fonts the UI loads by URL (avatars, markdown images, changelog images).
/// Allowlist only: a remote image from an unlisted origin is not fetched.
pub static REMOTE_ASSET: Purpose = Purpose {
    name: "remote-asset",
    kind: Kind::Automatic,
};
/// An MCP server (and its OAuth endpoints) the user configured.
pub static MCP_SERVER: Purpose = Purpose {
    name: "mcp-server",
    kind: Kind::UserConfigured,
};
/// The agent's webfetch / websearch tools.
pub static AGENT_WEB: Purpose = Purpose {
    name: "agent-web",
    kind: Kind::PublicWeb,
};
/// The local-control CLI talking to the running app on loopback.
pub static LOCAL_CONTROL: Purpose = Purpose {
    name: "local-control",
    kind: Kind::Automatic,
};
/// Test clients only.
pub static TEST: Purpose = Purpose {
    name: "test",
    kind: Kind::Unrestricted,
};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
