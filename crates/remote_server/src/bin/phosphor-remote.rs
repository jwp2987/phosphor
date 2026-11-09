//! `phosphor-remote`: the small helper the Phosphor app uploads to an ssh host (Wave H, #1804).
//!
//! ```text
//! phosphor-remote --version        version, protocol revision (and build id when known)
//! phosphor-remote proxy <key>      start the daemon if needed, bridge stdio to it (run over ssh)
//! phosphor-remote daemon <key>     the daemon itself (started by `proxy`, not by hand)
//! phosphor-remote index-scan <repo> [--known <file>]
//!                                  print the repo's file manifest, fragment hashes and root hash
//!                                  as JSON; with a previous output, only what changed
//! ```
//!
//! It is optional: when it cannot be installed, or the user declines it, the app works through
//! its own no-helper path. A plain `match` on the arguments on purpose; no argument parser.

use std::process::ExitCode;

const USAGE: &str = "usage: phosphor-remote --version | proxy <identity-key> | daemon <identity-key> | index-scan <repo> [--known <file>]";

#[cfg(unix)]
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--version"] | ["-V"] => {
            println!("{}", remote_server::naming::version_line());
            if let Some(build) = remote_server::naming::build_id() {
                println!("build {build}");
            }
            ExitCode::SUCCESS
        }
        ["proxy", key] => finish(remote_server::daemon::run_proxy(key)),
        ["daemon", key] => finish(remote_server::daemon::run_daemon(key)),
        ["index-scan", repo] => index_scan(repo, None),
        ["index-scan", repo, "--known", known] => index_scan(repo, Some(known)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(unix)]
fn index_scan(repo: &str, known: Option<&str>) -> ExitCode {
    use std::io::Write as _;
    match remote_server::daemon::run_index_scan(
        std::path::Path::new(repo),
        known.map(std::path::Path::new),
    ) {
        Ok(json) => match std::io::stdout().write_all(json.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(e) => {
            eprintln!("phosphor-remote: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(unix)]
fn finish(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("phosphor-remote: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(unix))]
fn main() -> ExitCode {
    eprintln!("phosphor-remote runs on unix hosts only. {USAGE}");
    ExitCode::FAILURE
}
