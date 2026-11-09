/// Owner rule: the diagnostics bundle is never uploaded. The code that collects and builds it must
/// not name any network client, URL opener or mail hook (repeated by `script/check_outbound_redaction`).
#[test]
fn the_problem_report_collector_has_no_network_path() {
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
        "open_url",
        "mailto:",
        "ServerApi",
        "graphql",
    ];
    for (name, src) in [
        ("problem_report.rs", include_str!("problem_report.rs")),
        (
            "report_bundle.rs",
            include_str!("../../../crates/warp_logging/src/report_bundle.rs"),
        ),
    ] {
        for (n, line) in src.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for b in BANNED {
                assert!(!line.contains(b), "{name}:{} names `{b}`", n + 1);
            }
        }
    }
}

#[test]
fn the_output_dir_knob_wins() {
    // SAFETY-free: the variable is only read by `output_dir`; restore it afterwards.
    let key = "PHOSPHOR_DIAG_BUNDLE_DIR";
    let before = std::env::var_os(key);
    unsafe { std::env::set_var(key, "/tmp/phosphor-diag-knob") };
    assert_eq!(
        super::output_dir(),
        Some(std::path::PathBuf::from("/tmp/phosphor-diag-knob"))
    );
    match before {
        Some(v) => unsafe { std::env::set_var(key, v) },
        None => unsafe { std::env::remove_var(key) },
    }
}
