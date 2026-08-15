use std::sync::OnceLock;

use command::blocking::Command;

/// The GCP project, zone and instance that every SSH integration test connects
/// to. Kept as separate constants so [`PROXY_COMMAND`] and
/// [`ssh_test_fixture_available`] cannot drift apart: the reachability probe
/// must ask about the same host the ProxyCommand will tunnel to.
const FIXTURE_PROJECT: &str = "warp-ssh-integration-testing";
const FIXTURE_ZONE: &str = "us-east4-a";
const FIXTURE_INSTANCE: &str = "ubuntu-14-04";
const FIXTURE_PORT: &str = "25784";

/// The command used to proxy ssh requests through GCP's Identity-Aware Proxy.
fn proxy_command() -> String {
    format!(
        "gcloud compute start-iap-tunnel {FIXTURE_INSTANCE} {FIXTURE_PORT} --listen-on-stdin \
         --project={FIXTURE_PROJECT} --zone={FIXTURE_ZONE}"
    )
}

/// Whether the SSH integration-test fixture is reachable from this machine.
///
/// Every SSH integration test drives a real `ssh` into one specific host:
/// [`FIXTURE_INSTANCE`] in the GCP project [`FIXTURE_PROJECT`], reached through
/// an Identity-Aware Proxy tunnel (see [`proxy_command`]). That project belongs
/// to Warp; this fork has no credentials for it and cannot obtain any. Without
/// access, the ProxyCommand fails and its diagnostics land in the terminal, so
/// the tests do not fail on the behaviour they test -- they fail after a
/// 40-second timeout with
///
///     assertion failed: The output should be Regex("bash@ubuntu-14-04's password:[\s]*$"),
///     but got "ERROR: (gcloud.compute.start-iap-tunnel) You do not currently have
///     an active account selected."
///
/// That was five permanently-red tests and ~3.5 minutes of dead wall clock on
/// every run of `cargo nextest -p integration`.
///
/// This is a genuine environmental precondition, checked the same way these
/// tests already check for PowerShell and for the tmux-wrapper feature flag: on
/// a machine that IS authorised for the project they run unchanged, with every
/// assertion intact. It is deliberately NOT `#[ignore]`, which would also skip
/// on the machines where the fixture does work.
///
/// The probe asks about the instance itself rather than merely whether some
/// account is logged in -- `gcloud auth list` reports an active account for any
/// developer with a personal Google login, which says nothing about access to
/// Warp's project. It runs under the same `CLOUDSDK_CONFIG` the tests export
/// (`$ORIGINAL_HOME/.config/gcloud`, see `subshell::setup_gcloud_sdk`), because
/// the harness rewrites `HOME` to a per-test temporary directory and a probe
/// reading the empty config there would report "unavailable" everywhere.
///
/// If this fork ever stands up its own SSH fixture (a container carrying the
/// `bash`/`zsh`/`sh`/`ash` accounts these tests expect), repoint the constants
/// above and this becomes a reachability check for that instead; the tests do
/// not change.
pub fn ssh_test_fixture_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let mut command = Command::new("gcloud");
        command.args([
            "compute",
            "instances",
            "describe",
            FIXTURE_INSTANCE,
            &format!("--project={FIXTURE_PROJECT}"),
            &format!("--zone={FIXTURE_ZONE}"),
            "--format=value(name)",
        ]);
        if let Ok(original_home) = std::env::var("ORIGINAL_HOME") {
            command.env("CLOUDSDK_CONFIG", format!("{original_home}/.config/gcloud"));
        }
        match command.output() {
            Ok(output) => output.status.success(),
            // gcloud not installed at all.
            Err(_) => false,
        }
    })
}

/// Produces a user/host pair for testing a given remote shell.
pub fn user_host(shell: &str) -> String {
    format!("{shell}@{FIXTURE_INSTANCE}")
}

/// Produces the full ssh command to run to ssh into a given remote shell.
pub fn ssh_command(shell: &str, should_use_ssh_wrapper: bool) -> String {
    [
        if should_use_ssh_wrapper {
            "ssh"
        } else {
            "command ssh"
        },
        &user_host(shell),
        &format!("-p {FIXTURE_PORT}"),
        &format!("-o ProxyCommand=\"{}\"", proxy_command()),
        "-o StrictHostKeyChecking=no",
        "-o UserKnownHostsFile=/dev/null",
    ]
    .join(" ")
}
