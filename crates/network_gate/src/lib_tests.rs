//! Unit tests for the network gate. Each test makes its own `Gate`, so none touches the
//! process-wide one.

use super::*;
use std::net::{IpAddr, Ipv4Addr};

static BACKGROUND: Purpose = Purpose {
    name: "t-background",
    kind: Kind::Automatic,
};
static USER_ACTION: Purpose = Purpose {
    name: "t-user-action",
    kind: Kind::UserAction(&["models.dev", "gitlab.com"]),
};
static WEB: Purpose = Purpose {
    name: "t-web",
    kind: Kind::PublicWeb,
};
static MCP: Purpose = Purpose {
    name: "t-mcp",
    kind: Kind::UserConfigured,
};

fn gate_with(configured: &[&str]) -> Gate {
    let g = Gate::new();
    g.set_configured_origins(configured.iter().copied());
    g
}

#[test]
fn unlisted_host_is_denied_for_background_requests() {
    let g = Gate::new();
    assert!(g.check_url("https://example.org/x", &BACKGROUND).is_err());
    assert!(g.check_url("http://example.org/x", &USER_ACTION).is_err());
}

#[test]
fn loopback_ip_literals_pass() {
    let g = Gate::new();
    for url in [
        "http://127.0.0.1:8080/v1",
        "http://[::1]:11434/api",
        "http://127.5.5.5/",
    ] {
        assert!(g.check_url(url, &BACKGROUND).is_ok(), "{url}");
    }
}

#[test]
fn configured_endpoints_pass_on_their_port_only() {
    let g = gate_with(&["https://api.example.com/v1", "http://localhost:11434"]);
    assert!(
        g.check_url("https://api.example.com/v1/chat", &BACKGROUND)
            .is_ok()
    );
    assert!(
        g.check_url("https://api.example.com:443/models", &BACKGROUND)
            .is_ok()
    );
    assert!(
        g.check_url("https://api.example.com:8443/models", &BACKGROUND)
            .is_err()
    );
    assert!(
        g.check_url("https://other.example.com/", &BACKGROUND)
            .is_err()
    );
    assert!(
        g.check_url("http://localhost:11434/api/tags", &BACKGROUND)
            .is_ok()
    );
    assert!(g.check_url("http://localhost:9999/", &BACKGROUND).is_err());
}

#[test]
fn loopback_by_name_is_refused_unless_configured() {
    let g = Gate::new();
    for url in [
        "http://localhost:8080/",
        "http://foo.localhost/",
        "http://LOCALHOST/",
    ] {
        assert!(g.check_url(url, &BACKGROUND).is_err(), "{url}");
        assert!(g.check_url(url, &WEB).is_err(), "{url}");
        assert!(g.check_url(url, &USER_ACTION).is_err(), "{url}");
    }
    g.set_user_allowed_origins("localhost");
    assert!(g.check_url("http://localhost:8080/", &BACKGROUND).is_ok());
}

#[test]
fn a_name_that_resolves_to_loopback_is_refused_unless_configured() {
    let g = gate_with(&["http://ollama.lan:11434"]);
    let lo = [IpAddr::V4(Ipv4Addr::LOCALHOST)];
    let public = [IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))];
    let mixed = [public[0], lo[0]];
    assert!(!g.resolved_ok("rebind.example.com", &lo));
    assert!(!g.resolved_ok("rebind.example.com", &mixed));
    assert!(!g.resolved_ok("rebind.example.com", &[IpAddr::V4(Ipv4Addr::UNSPECIFIED)]));
    assert!(g.resolved_ok("rebind.example.com", &public));
    assert!(
        g.resolved_ok("ollama.lan", &lo),
        "a configured host may be loopback"
    );
}

#[test]
fn a_redirect_to_a_host_that_is_not_allowed_is_denied() {
    let g = gate_with(&["https://api.example.com"]);
    assert!(
        g.check_redirect("https://api.example.com/v2", 1, &BACKGROUND)
            .is_ok()
    );
    assert!(
        g.check_redirect("https://evil.example.net/steal", 1, &BACKGROUND)
            .is_err()
    );
    // Same for a user action: only its own hosts follow.
    assert!(
        g.check_redirect("https://models.dev/api.json", 0, &USER_ACTION)
            .is_ok()
    );
    assert!(
        g.check_redirect("https://evil.example.net/", 0, &USER_ACTION)
            .is_err()
    );
    // A user-configured client (MCP) is lenient on the first request, strict on hops.
    assert!(g.check_url("https://mcp.example.net/sse", &MCP).is_ok());
    assert!(
        g.check_redirect("https://evil.example.net/", 0, &MCP)
            .is_err()
    );
    assert!(
        g.check_redirect("https://api.example.com/", 0, &MCP)
            .is_ok()
    );
}

#[test]
fn a_redirect_chain_is_capped() {
    let g = Gate::new();
    assert!(
        g.check_redirect("http://127.0.0.1/", MAX_REDIRECT_HOPS - 1, &BACKGROUND)
            .is_ok()
    );
    assert!(
        g.check_redirect("http://127.0.0.1/", MAX_REDIRECT_HOPS, &BACKGROUND)
            .is_err()
    );
}

#[test]
fn user_actions_reach_only_their_named_hosts_and_subdomains() {
    let g = Gate::new();
    assert!(
        g.check_url("https://models.dev/api.json", &USER_ACTION)
            .is_ok()
    );
    assert!(
        g.check_url("https://gitlab.com/a/b/-/raw/main/x.json", &USER_ACTION)
            .is_ok()
    );
    assert!(
        g.check_url("https://cdn.models.dev/x", &USER_ACTION)
            .is_ok()
    );
    assert!(
        g.check_url("https://notmodels.dev/x", &USER_ACTION)
            .is_err()
    );
    assert!(
        g.check_url("https://models.dev.evil.net/x", &USER_ACTION)
            .is_err()
    );
    // The same URL from a background request is denied.
    assert!(
        g.check_url("https://models.dev/api.json", &BACKGROUND)
            .is_err()
    );
}

#[test]
fn public_web_refuses_private_and_loopback_literals() {
    let g = Gate::new();
    assert!(g.check_url("https://docs.example.org/", &WEB).is_ok());
    for url in [
        "http://10.0.0.5/",
        "http://192.168.1.1/",
        "http://169.254.169.254/latest/meta-data",
        "http://[fd00::1]/",
        "http://0.0.0.0/",
    ] {
        assert!(g.check_url(url, &WEB).is_err(), "{url}");
    }
}

#[test]
fn user_configured_clients_may_use_their_own_url() {
    let g = Gate::new();
    assert!(g.check_url("http://localhost:3000/mcp", &MCP).is_ok());
    assert!(g.check_url("https://mcp.example.net/sse", &MCP).is_ok());
}

#[test]
fn non_http_schemes_and_garbage_are_denied() {
    let g = Gate::new();
    for url in ["file:///etc/passwd", "ftp://example.org/", "not a url", ""] {
        assert!(g.check_url(url, &MCP).is_err(), "{url:?}");
        assert!(g.check_url(url, &WEB).is_err(), "{url:?}");
    }
}

#[test]
fn the_user_allowed_list_accepts_urls_hosts_and_separators() {
    let g = Gate::new();
    g.set_user_allowed_origins("https://a.example.com, b.example.com:8443\nc.example.com");
    assert!(g.check_url("https://a.example.com/x", &BACKGROUND).is_ok());
    assert!(
        g.check_url("https://b.example.com:8443/x", &BACKGROUND)
            .is_ok()
    );
    assert!(
        g.check_url("https://b.example.com:9/x", &BACKGROUND)
            .is_err()
    );
    assert!(
        g.check_url("http://c.example.com:81/x", &BACKGROUND)
            .is_ok()
    );
    g.set_user_allowed_origins("");
    assert!(g.check_url("https://a.example.com/x", &BACKGROUND).is_err());
}

#[test]
fn a_denial_never_contains_the_url_or_host() {
    let g = Gate::new();
    let secret = "https://user:pw@private-host.example.net/secret/path?key=abc";
    let d = g.check_url(secret, &BACKGROUND).unwrap_err();
    let shown = format!("{d} / {d:?}");
    for needle in [
        "private-host",
        "example.net",
        "secret",
        "key=abc",
        "pw",
        "user",
    ] {
        assert!(!shown.contains(needle), "denial leaked {needle:?}: {shown}");
    }
    assert!(shown.contains("t-background"));
    assert_eq!(d.host_hash, hash_host("PRIVATE-HOST.example.net"));
    // The error handed back to callers carries no URL either.
    let e = denied_error();
    assert!(e.url().is_none());
}

#[test]
fn the_test_purpose_is_not_checked() {
    let g = Gate::new();
    assert!(g.check_url("https://anything.example.org/", &TEST).is_ok());
}

#[test]
fn the_configured_proxy_applier_is_used_and_clearable() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    set_configured_proxy(Some(Arc::new(|b| {
        CALLS.fetch_add(1, Ordering::SeqCst);
        b
    })));
    let _ = apply_proxy_policy(reqwest::ClientBuilder::new());
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    set_configured_proxy(None);
    let _ = apply_proxy_policy(reqwest::ClientBuilder::new());
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        1,
        "a cleared applier is not called"
    );
}

#[test]
fn the_purpose_table_is_classified_as_documented() {
    assert_eq!(UNCLASSIFIED.kind, Kind::Automatic);
    assert_eq!(AI_ENDPOINT.kind, Kind::Automatic);
    assert!(matches!(MODELS_DEV.kind, Kind::UserAction(_)));
    assert!(matches!(UPDATE.kind, Kind::UserAction(_)));
    assert_eq!(AGENT_WEB.kind, Kind::PublicWeb);
    assert_eq!(MCP_SERVER.kind, Kind::UserConfigured);
}
