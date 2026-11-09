use std::io::Read;

use chrono::{Duration, TimeZone, Utc};

use super::*;

fn id() -> Identity {
    Identity {
        home: Some("/home/alice".to_string()),
        username: Some("alice".to_string()),
        hostname: Some("alice-box".to_string()),
    }
}

fn scrub(line: &str) -> String {
    scrub_line(line, &id())
}

#[test]
fn home_user_and_host_of_this_machine_are_removed() {
    let out =
        scrub("2026-11-08T12:00:00Z [INFO] opened /home/alice/work/x.rs on alice-box as alice");
    for leak in ["alice", "/home", "work", "x.rs"] {
        assert!(!out.contains(leak), "{leak} leaked: {out}");
    }
    assert!(out.contains("[INFO]"));
}

#[test]
fn windows_home_in_every_spelling_is_removed() {
    let id = Identity {
        home: Some("C:\\Users\\Bob".to_string()),
        username: Some("Bob".to_string()),
        hostname: None,
    };
    for line in [
        "cwd C:\\Users\\Bob\\proj",
        "cwd C:\\\\Users\\\\Bob\\\\proj",
        "cwd C:/Users/Bob/proj",
    ] {
        let out = scrub_line(line, &id);
        assert!(!out.contains("Bob") && !out.contains("proj"), "{out}");
    }
}

#[test]
fn other_absolute_paths_emails_ips_and_hosts_are_removed() {
    let out = scrub(
        "read /etc/passwd and /opt/tool/bin mail me@example.com ip 192.168.1.20:22 v6 fe80::1 host db.internal.example.org url https://example.com/a?b=c tmp ~/secret.txt",
    );
    for leak in [
        "/etc",
        "passwd",
        "/opt",
        "me@",
        "example.com",
        "192.168",
        "fe80",
        "db.internal",
        "~/secret",
        "https://",
    ] {
        assert!(!out.contains(leak), "{leak} leaked: {out}");
    }
}

#[test]
fn user_at_host_is_removed() {
    let out = scrub("ssh deploy@build01 started");
    assert!(!out.contains("deploy") && !out.contains("build01"), "{out}");
}

#[test]
fn keys_tokens_and_authorization_values_are_removed() {
    let cases = [
        "Authorization: Bearer abcdef0123456789abcdef0123456789",
        "header authorization=Basic dXNlcjpwYXNz",
        "api_key=sk-live-0123456789abcdef",
        "token=\"ghp_0123456789abcdefghij\"",
        "password: hunter2",
        "request used Bearer zzzTOP9secret",
        "found AKIAIOSFODNN7EXAMPLE in env",
        "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abc",
        "blob Xk3j9Zq0aB7cD2eF5gH8iJ1kL4mN6oP",
    ];
    for c in cases {
        let out = scrub(c);
        for leak in [
            "abcdef0123",
            "dXNlcjpw",
            "sk-live",
            "ghp_",
            "hunter2",
            "zzzTOP9secret",
            "AKIAIOS",
            "eyJhbGci",
            "Xk3j9Zq0",
        ] {
            assert!(!out.contains(leak), "{leak} leaked from {c:?}: {out}");
        }
    }
}

#[test]
fn typed_text_command_lines_and_prompts_are_removed() {
    let cases = [
        (
            "input_order hop=pty_write_bytes n=9 bytes=\"rm -rf /data\" t=1.5",
            "rm -rf",
        ),
        ("cmd=ls -la", "-la"),
        ("command: \"git push origin main\"", "git push"),
        (
            "agent prompt=\"find the bug in main.rs\" model=x",
            "find the bug",
        ),
        ("Submitting 'curl -H secret example' now", "curl"),
        ("text=hello world", "hello"),
    ];
    for (c, leak) in cases {
        let out = scrub(c);
        assert!(!out.contains(leak), "{leak} leaked from {c:?}: {out}");
    }
}

#[test]
fn timing_lines_survive() {
    let line = "2026-11-08T12:00:00.123456Z [INFO] input_order hop=ui_item kind=frame ms=4.25 render_ms=1.00 layout_paint_ms=3.25 t=1731067200.123456";
    assert_eq!(scrub(line), line);
    let ssh = "2026-11-08T12:00:00Z [INFO] ssh hold: discarded 3 held input(s)";
    assert_eq!(scrub(ssh), ssh);
    let hop = "hop=enter_begin conv=0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0 state=Running";
    assert_eq!(scrub(hop), hop);
}

#[test]
fn scrubbing_is_idempotent() {
    let once = scrub("path /home/alice/x cmd=\"ls\" me@example.com 10.0.0.1 token=abc");
    assert_eq!(scrub(&once), once);
}

#[test]
fn settings_secrets_and_free_text_are_removed() {
    let toml = "# my notes\n[ai]\nenabled = true\nopacity = 0.5\nmode = \"agent\"\nopenai_api_key = \"sk-abc\"\ndefault_dir = \"/home/alice/work\"\nredaction_regexes = [\n  \"corp-[0-9]+\",\n]\n[ssh.hosts.prod]\nhost = \"db.example.org\"\n";
    let out = scrub_settings_toml(toml, &id());
    for leak in ["my notes", "sk-abc", "alice", "corp-", "prod", "db.example"] {
        assert!(!out.contains(leak), "{leak} leaked: {out}");
    }
    assert!(
        out.contains("enabled = true")
            && out.contains("opacity = 0.5")
            && out.contains("mode = \"agent\"")
    );
}

#[test]
fn window_keeps_only_recent_lines_and_wrapped_continuations() {
    let now = Utc.with_ymd_and_hms(2026, 11, 8, 12, 0, 0).unwrap();
    let lines = [
        "no timestamp before the first one",
        "2026-11-08T11:30:00Z [INFO] old",
        "  wrapped old continuation",
        "2026-11-08T11:45:00Z [INFO] edge",
        "2026-11-08T11:59:59Z [INFO] new",
        "  wrapped new continuation",
    ]
    .map(String::from);
    let (kept, dropped) = filter_window(lines, now, Duration::minutes(15));
    assert_eq!(kept.len(), 3, "{kept:?}");
    assert!(kept[0].contains("edge") && kept[1].contains("new") && kept[2].contains("wrapped new"));
    assert_eq!(dropped, 3);
}

#[test]
fn bundle_has_the_window_the_manifest_and_scrubbed_settings_only() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("p.log");
    let now = Utc::now();
    let fmt = |d: Duration| (now - d).to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    std::fs::write(
        &log,
        format!(
            "{} [INFO] ancient-line cwd=/home/alice/old\n{} [INFO] recent-line hop=enter_begin cmd=\"secret-cmd\" at /home/alice/p\n",
            fmt(Duration::minutes(60)),
            fmt(Duration::minutes(2)),
        ),
    )
    .unwrap();
    let out = dir.path().join("out.zip");
    let summary = build_report_bundle(
        &out,
        &ReportInputs {
            log_files: vec![log],
            manifest: "Version: 1.2.3\nOS: linux\nhome /home/alice\n".to_string(),
            settings_toml: Some("token = \"abc\"\nx = true\n".to_string()),
            identity: id(),
            now,
            window: Duration::minutes(REPORT_WINDOW_MINUTES),
            extra_scrub: None,
        },
    )
    .unwrap();
    assert_eq!(summary.log_lines, 1);
    assert_eq!(summary.dropped_out_of_window, 1);

    let mut zip = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
    let mut all = String::new();
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let name = f.name().expect("zip entry name").to_string();
        names.push(name.clone());
        let mut body = String::new();
        f.read_to_string(&mut body).unwrap();
        all.push_str(&body);
        if name == "app-log-window.txt" {
            assert!(body.contains("recent-line") && body.contains("hop=enter_begin"));
            assert!(!body.contains("ancient-line"));
        }
    }
    names.sort();
    assert_eq!(
        names,
        [
            "README.txt",
            "app-log-window.txt",
            "manifest.txt",
            "settings-scrubbed.toml"
        ]
    );
    for leak in ["alice", "secret-cmd", "abc\"", "/home"] {
        assert!(!all.contains(leak), "{leak} leaked into the bundle");
    }
    assert!(all.contains("Version: 1.2.3") && all.contains("x = true"));
}

#[test]
fn the_extra_pass_runs_on_every_log_line() {
    fn shout(s: &mut String) {
        *s = s.replace("recent", "RECENT");
    }
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("p.log");
    let now = Utc::now();
    std::fs::write(
        &log,
        format!(
            "{} [INFO] recent\n",
            now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
        ),
    )
    .unwrap();
    let out = dir.path().join("o.zip");
    build_report_bundle(
        &out,
        &ReportInputs {
            log_files: vec![log],
            manifest: String::new(),
            settings_toml: None,
            identity: Identity::default(),
            now,
            window: Duration::minutes(15),
            extra_scrub: Some(shout),
        },
    )
    .unwrap();
    let mut zip = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
    let mut body = String::new();
    zip.by_name("app-log-window.txt")
        .unwrap()
        .read_to_string(&mut body)
        .unwrap();
    assert!(body.contains("RECENT"));
}

/// Owner rule: the report is never uploaded. No network crate or client type may be named in the
/// code that builds it (this file's sibling and the app-side collector).
#[test]
fn no_network_client_is_reachable_from_the_report_code() {
    const BANNED: &[&str] = &[
        "reqwest",
        "hyper",
        "ureq",
        "TcpStream",
        "UdpSocket",
        "http_client",
        "HttpClient",
        "std::net",
        "tokio::net",
        "isahc",
        "surf::",
        "curl::",
        "open_url",
        "mailto:",
        "lettre",
    ];
    let sources = [("report_bundle.rs", include_str!("report_bundle.rs"))];
    for (name, src) in sources {
        // the module docs and this guard name the words; only code lines count
        for (n, line) in src.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for b in BANNED {
                assert!(!line.contains(b), "{name}:{} names `{b}`", n + 1);
            }
        }
    }
}
