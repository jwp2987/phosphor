#[cfg(unix)]
use std::process::Stdio;

#[cfg(unix)]
use command::blocking::Command;

use super::*;

#[test]
fn parse_uname_linux_x86_64() {
    let platform = parse_uname_output("Linux x86_64").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_linux_aarch64() {
    let platform = parse_uname_output("Linux aarch64").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::Aarch64);
}

#[test]
fn parse_uname_darwin_arm64() {
    let platform = parse_uname_output("Darwin arm64").unwrap();
    assert_eq!(platform.os, RemoteOs::MacOs);
    assert_eq!(platform.arch, RemoteArch::Aarch64);
}

#[test]
fn parse_uname_darwin_x86_64() {
    let platform = parse_uname_output("Darwin x86_64").unwrap();
    assert_eq!(platform.os, RemoteOs::MacOs);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_linux_armv8l() {
    let platform = parse_uname_output("Linux armv8l").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::Aarch64);
}

#[test]
fn parse_uname_skips_shell_initialization_output() {
    let output = "Last login: Mon Apr  7 10:00:00 2025\nWelcome to Ubuntu\nLinux x86_64";
    let platform = parse_uname_output(output).unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_trims_whitespace() {
    let platform = parse_uname_output("  Linux x86_64  \n").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_unsupported_os() {
    let result = parse_uname_output("Windows x86_64");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported OS"));
}

#[test]
fn parse_uname_unsupported_arch() {
    let result = parse_uname_output("Linux mips");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported arch"));
}

#[test]
fn parse_uname_empty_output() {
    let result = parse_uname_output("");
    assert!(result.is_err());
}

#[test]
fn parse_uname_missing_arch() {
    let result = parse_uname_output("Linux");
    assert!(result.is_err());
}

// Ported from the oracle. `amd64` was missing as an alias for `x86_64` in
// this fork's `parse_uname_output` match arm (regression fixed alongside
// this port — see the `"x86_64" | "amd64"` arm above).
#[test]
fn parse_uname_linux_amd64() {
    let platform = parse_uname_output("Linux amd64").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_unsupported_armv7l() {
    let result = parse_uname_output("Linux armv7l");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported arch"));
}

// NOTE: the oracle also has `parse_uname_unsupported_armv8l`, asserting that
// `armv8l` is rejected. Not ported: this fork's `parse_uname_output`
// deliberately treats `armv8l` as `RemoteArch::Aarch64` (32-bit userland on
// aarch64 hardware, e.g. Raspberry Pi OS) — see `parse_uname_linux_armv8l`
// above, which already covers this fork's (intentionally different)
// behavior.

#[test]
fn state_is_ready() {
    assert!(RemoteServerSetupState::Ready.is_ready());
    assert!(!RemoteServerSetupState::Checking.is_ready());
    assert!(!RemoteServerSetupState::Initializing.is_ready());
}

#[test]
fn state_is_failed() {
    assert!(RemoteServerSetupState::Failed {
        error: "test".into()
    }
    .is_failed());
    assert!(!RemoteServerSetupState::Ready.is_failed());
}

#[test]
fn state_is_terminal() {
    assert!(RemoteServerSetupState::Ready.is_terminal());
    assert!(RemoteServerSetupState::Failed {
        error: "test".into()
    }
    .is_terminal());
    assert!(RemoteServerSetupState::Unsupported {
        reason: UnsupportedReason::NonGlibc {
            name: "musl".into()
        }
    }
    .is_terminal());
    assert!(!RemoteServerSetupState::Checking.is_terminal());
    assert!(!RemoteServerSetupState::Installing {
        progress_percent: None,
    }
    .is_terminal());
    assert!(!RemoteServerSetupState::Updating.is_terminal());
    assert!(!RemoteServerSetupState::Initializing.is_terminal());
}

// NOTE: the oracle also has `parse_preinstall_unsupported_glibc_too_old` and
// `parse_preinstall_unsupported_non_glibc`, asserting that these reasons
// produce `PreinstallStatus::Unsupported`. Not ported: this fork's
// `parse_status` (see `setup.rs`) already moved past the oracle's pinned
// revision — remote-server is a static musl binary here, so these two
// reasons are treated as `Supported` rather than `Unsupported`. That
// (newer, correct) behavior is already covered by
// `parse_preinstall_legacy_glibc_too_old_now_supported` and
// `parse_preinstall_legacy_non_glibc_now_supported` below.

#[test]
fn parse_preinstall_supported_glibc() {
    let stdout = "required_glibc=2.31\n\
                  libc_family=glibc\n\
                  libc_version=2.35\n\
                  status=supported\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert_eq!(result.libc, RemoteLibc::Glibc(GlibcVersion::new(2, 35)));
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_legacy_glibc_too_old_now_supported() {
    // remote-server is now a static musl binary, so the old script's
    // `glibc_too_old` gate no longer applies. Even if an old remote side is
    // caching the old script and reports this reason, the client should treat
    // it as supported rather than falling back to ControlMaster.
    let stdout = "required_glibc=2.17\n\
                  libc_family=glibc\n\
                  libc_version=2.17\n\
                  status=unsupported\n\
                  reason=glibc_too_old\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_legacy_non_glibc_now_supported() {
    // Same reasoning: musl/uclibc hosts can also run the static binary. The
    // old script's `non_glibc` no longer triggers a fall-back.
    let stdout = "required_glibc=2.17\n\
                  libc_family=musl\n\
                  status=unsupported\n\
                  reason=non_glibc\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert_eq!(
        result.libc,
        RemoteLibc::NonGlibc {
            name: "musl".to_string()
        }
    );
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_musl_host_supported() {
    // The new script reports `status=supported` directly on a musl host.
    let stdout = "required_glibc=2.17\n\
                  libc_family=musl\n\
                  status=supported\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_old_glibc_host_supported() {
    // The new script also reports `status=supported` directly on an old glibc (< 2.35) host.
    let stdout = "required_glibc=2.17\n\
                  libc_family=glibc\n\
                  libc_version=2.17\n\
                  status=supported\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_missing_status_falls_open() {
    // Garbled / partial script output — missing status field. Confirms
    // the fail-open invariant: anything we can't positively classify as
    // unsupported degrades to Unknown and is treated as supported, so a
    // flaky probe doesn't block the install.
    let stdout = "libc_family=glibc\nlibc_version=2.35\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Unknown);
    assert!(result.is_supported());
}

#[test]
fn oss_remote_server_dir_uses_phosphor_namespace() {
    assert_eq!(remote_server_dir(), "~/.phosphor/remote-server");
}

#[test]
fn oss_binary_name_matches_phosphor_cli() {
    assert_eq!(binary_name(), "phosphor-oss");
}

#[test]
fn oss_download_tarball_url_uses_github_release_asset() {
    let platform = RemotePlatform {
        os: RemoteOs::Linux,
        arch: RemoteArch::X86_64,
    };

    let url = download_tarball_url(&platform);

    assert_eq!(
        url,
        "https://github.com/jwp2987/phosphor/releases/latest/download/phosphor-cli-linux-x86_64.tar.gz"
    );
    assert!(!url.contains("app.warp.dev"));
    assert!(!url.contains("/download/cli"));

    // The two halves of the 2026-08-11 bug, asserted separately so a
    // regression names itself. Until then this URL pointed at `zerx-lab/warp`
    // and asked for a `zap-` asset, so it 404'd for every user of this fork and
    // remote-server setup over SSH could not succeed at all. The old test
    // passed throughout, because it asserted the wrong string confidently.
    assert!(
        !url.contains("zerx-lab"),
        "remote-server installs must not fetch from upstream Zap's releases: {url}"
    );
    assert!(
        url.contains("/phosphor-cli-"),
        "asset prefix must match what the release workflow publishes, not the \
         channel command name (`phosphor-oss`): {url}"
    );
}

#[test]
fn install_script_uses_release_asset_prefix_and_staging_placeholder() {
    let script = install_script(Some("~/.phosphor/remote-server/phosphor-upload.tar.gz"));

    assert!(
        script.contains("staging_tarball_path=\"~/.phosphor/remote-server/phosphor-upload.tar.gz\"")
    );
    assert!(script.contains("phosphor-cli-$os_name-$arch_name.tar.gz"));
    // Deliberately changed on 2026-08-11: the script asked for a `zap-` asset,
    // but the release workflow publishes `phosphor-cli-`, so every remote
    // install 404'd. `phosphor-oss` is the channel COMMAND name, not the ASSET name.
    assert!(
        !script.contains("/zap-$os_name-$arch_name.tar.gz"),
        "install script must ask for the published asset name, not the channel \
         command name"
    );
    assert!(!script.contains("app.warp.dev"));
    assert!(!script.contains("/download/cli"));
}

#[test]
fn binary_check_runs_version() {
    assert_eq!(
        binary_check_command(),
        format!("{} --version", remote_server_binary())
    );
}

/// Regression: guards against re-introducing the
/// `${var/pattern/replacement}` tilde-substitution form, which has two
/// known interpreter bugs (see `install_script_tilde_expansion_resolves_correctly`
/// below for details).
#[test]
fn install_script_avoids_pattern_substitution_for_tilde_expansion() {
    let template = INSTALL_SCRIPT_TEMPLATE;
    assert!(
        !template.contains(r"/#\~/"),
        "install_remote_server.sh uses `${{var/#\\~/...}}` for tilde \
         expansion. This form has two known interpreter bugs that \
         silently mis-resolve the install path:\n\
         \n\
           1. bash 3.2 (macOS /bin/bash) keeps inner double-quotes \
              around the replacement literal, so `\"$HOME\"` ends up \
              as 6 literal characters including the quotes.\n\
           2. bash 5.2+ enables `patsub_replacement` by default, which \
              makes `&` in the replacement expand to the matched \
              pattern, so a `$HOME` containing `&` resolves wrong.\n\
         \n\
         Use `case`/`${{var#\\~}}` instead — see install_remote_server.sh \
         for the pattern.",
    );
}

/// Regression: the install script's tilde-expansion logic must work across
/// the bash versions we actually invoke at install time (`run_ssh_script`
/// pipes the script into `bash -s` on the remote). Two interpreter quirks
/// have to be avoided simultaneously:
///
///   1. bash 3.2 (macOS `/bin/bash`) keeps inner double-quotes around the
///      replacement of `${var/pattern/replacement}` literal, so `"$HOME"`
///      ends up as 6 literal characters and the install lands under a
///      directory tree literally named `"`.
///   2. bash 5.2+ with `patsub_replacement` (default-on) treats `&` in the
///      replacement as the matched pattern, so a `$HOME` containing `&`
///      resolves to a `~`-substituted path.
///
/// This test drives the *actual* production script (via [`install_script`])
/// rather than a hand-copied snippet, and runs it against several `HOME`
/// values to exercise the patsub-`&` trap as well as the quote-literal
/// trap. We truncate just before `mkdir -p` so no filesystem side effects
/// leak out of the test, and append a marker `printf` to capture the
/// resolved `install_dir`.
///
/// Gated to Unix because the test invokes `/bin/bash` (or `bash` from
/// PATH) directly.
#[cfg(unix)]
#[test]
fn install_script_tilde_expansion_resolves_correctly() {
    let bash = if std::path::Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "bash"
    };

    let script = install_script(None);
    let cutoff = script.find("mkdir -p \"$install_dir\"").expect(
        "install script no longer contains the `mkdir -p \"$install_dir\"` \
         checkpoint this test relies on; update the test alongside the \
         script change",
    );
    let probe = format!(
        "{prefix}\nprintf '%s' \"$install_dir\"\nexit 0\n",
        prefix = &script[..cutoff],
    );

    // Run the probe against a matrix of HOME values. The first is an
    // ordinary path; the second contains `&`, which exercises bash 5.2's
    // patsub_replacement (where it would otherwise expand to the matched
    // `~`).
    let cases = [
        ("/Users/test", "ordinary HOME"),
        (
            "/Users/A&B",
            "HOME with `&` (bash 5.2 patsub_replacement trap)",
        ),
    ];

    for (fake_home, label) in cases {
        let output = Command::new(bash)
            .arg("-c")
            .arg(&probe)
            .env("HOME", fake_home)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("failed to spawn bash");

        assert!(
            output.status.success(),
            "[{label}] bash exited with {:?}: stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );

        let install_dir = String::from_utf8_lossy(&output.stdout);
        assert!(
            !install_dir.contains('"'),
            "[{label}] install_dir contains literal quote characters \
             (bash 3.2 quote-literal regression): {install_dir:?}",
        );

        // Cross-check against the production layout: tilde must resolve to
        // HOME, so the result equals `remote_server_dir()` with the leading
        // `~` replaced.
        let expected = remote_server_dir().replacen('~', fake_home, 1);
        assert_eq!(
            install_dir, expected,
            "[{label}] install_dir resolved incorrectly",
        );
    }
}

#[test]
fn identity_dir_name_is_deterministic() {
    let key = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
    assert_eq!(
        remote_server_identity_dir_name(key),
        remote_server_identity_dir_name(key)
    );
}

#[test]
fn identity_dir_name_differs_for_different_keys() {
    assert_ne!(
        remote_server_identity_dir_name("key-a"),
        remote_server_identity_dir_name("key-b")
    );
}

/// Regression (see `remote_server_identity_dir_name`'s doc comment): this
/// used to percent-encode the raw identity key, which is a no-op for a
/// UUID-shaped key and defeats the `sun_path`-safety hashing this function
/// exists for.
#[test]
fn identity_dir_name_is_short_hash() {
    let name = remote_server_identity_dir_name("a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    assert_eq!(name.len(), 8, "identity dir should be 8 hex chars: {name}");
    assert!(
        name.chars().all(|c| c.is_ascii_hexdigit()),
        "identity dir should be hex: {name}"
    );
}

#[test]
fn data_dir_uses_percent_encoded_identity_key() {
    let data_dir = remote_server_daemon_data_dir("user@example.com/ssh host");
    assert_eq!(
        data_dir,
        format!(
            "{}/user%40example%2Ecom%2Fssh%20host/data",
            remote_server_dir()
        )
    );
}

#[test]
fn data_dir_handles_empty_identity_key() {
    let data_dir = remote_server_daemon_data_dir("");
    assert_eq!(data_dir, format!("{}/empty/data", remote_server_dir()));
}

#[test]
fn daemon_dir_and_data_dir_use_different_identity_paths() {
    let key = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
    let daemon_dir = remote_server_daemon_dir(key);
    let data_dir = remote_server_daemon_data_dir(key);
    // Daemon dir uses the 8-char hash.
    assert!(daemon_dir.contains(&remote_server_identity_dir_name(key)));
    // Data dir uses the full key (no collision risk for persistent state).
    assert!(data_dir.contains(key));
    // They must be different paths.
    assert!(!data_dir.starts_with(&daemon_dir));
}

#[test]
fn daemon_socket_name_is_short() {
    // Without GIT_RELEASE_TAG (typical in tests), falls back to unversioned.
    let name = daemon_socket_name();
    // In test builds without GIT_RELEASE_TAG, we get "server.sock".
    // In release builds, we get "server-{8hex}.sock" = 24 chars.
    // Either way, the name must be <= 24 chars.
    assert!(
        name.len() <= 24,
        "daemon_socket_name is too long ({} chars): {name}",
        name.len()
    );
    assert!(name.starts_with("server"));
    assert!(name.ends_with(".sock"));
}

#[test]
fn daemon_pid_name_is_short() {
    let name = daemon_pid_name();
    assert!(
        name.len() <= 22,
        "daemon_pid_name is too long ({} chars): {name}",
        name.len()
    );
    assert!(name.starts_with("server"));
    assert!(name.ends_with(".pid"));
}

#[test]
fn socket_path_fits_within_sun_path_worst_case() {
    // Worst case: preview channel (longest base dir) + 32-char username
    // (Linux max) + hashed identity (8 chars) + hashed socket (20 chars).
    //
    // Path: /home/{user}/.warp-preview/remote-server/{hash8}/server-{hash8}.sock
    //       6 + 32 + 1 + 29 + 8 + 1 + 20 = 97 bytes -> well under 103 (macOS)
    let long_home = "/home/a]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]";
    let identity_dir = remote_server_identity_dir_name("a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    assert_eq!(identity_dir.len(), 8);

    let hashed_socket = "server-a1b2c3d4.sock";
    let old_socket = "server-v0.2026.05.13.09.15.stable_01.sock";

    // Use .warp-preview (longest channel base dir) for worst case.
    let daemon_dir = format!("{long_home}/.warp-preview/remote-server/{identity_dir}");

    let hashed_path = format!("{daemon_dir}/{hashed_socket}");

    // Must fit within macOS sun_path limit (103 bytes), the stricter of the
    // two platforms.
    assert!(
        hashed_path.len() <= 103,
        "hashed socket path exceeds macOS sun_path limit: {} bytes ({})",
        hashed_path.len(),
        hashed_path,
    );

    // The OLD naming scheme (full version + unhashed identity) should
    // exceed the limit, confirming the regression.
    let old_identity = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"; // 36 chars unhashed
    let old_daemon_dir = format!("{long_home}/.warp-preview/remote-server/{old_identity}");
    let old_full_path = format!("{old_daemon_dir}/{old_socket}");
    assert!(
        old_full_path.len() > 107,
        "old socket path should exceed Linux sun_path limit to confirm the \
         regression: {} bytes ({})",
        old_full_path.len(),
        old_full_path,
    );
}

#[test]
fn version_hash_is_deterministic() {
    // version_hash uses the compile-time GIT_RELEASE_TAG which is typically
    // unset in test builds, so it returns None. We test the hashing logic
    // directly instead.
    use std::hash::{Hash, Hasher};

    let version = "v0.2026.05.13.09.15.stable_01";
    let hash = |v: &str| -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        v.hash(&mut hasher);
        format!("{:016x}", hasher.finish())[..8].to_string()
    };

    // Same input produces the same hash.
    assert_eq!(hash(version), hash(version));
    // Different inputs produce different hashes.
    assert_ne!(hash(version), hash("v0.2026.05.14.09.15.stable_01"));
    // Hash is exactly 8 hex chars.
    assert_eq!(hash(version).len(), 8);
    assert!(hash(version).chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn bundled_resources_dir_is_global_and_version_independent() {
    let dir = remote_server_bundled_resources_dir();
    assert_eq!(
        dir,
        format!("{}/{}", remote_server_dir(), BUNDLED_RESOURCES_DIR_NAME)
    );
    // The whole point of the global location: no version in the path.
    assert!(!dir.contains(pinned_version()));
}

#[test]
fn removal_command_removes_binary_but_leaves_global_resources() {
    let command = remote_server_removal_command();
    assert_eq!(command, format!("rm -f {}", remote_server_binary()));
    assert!(!command.contains(BUNDLED_RESOURCES_DIR_NAME));
}

#[test]
fn install_script_substitutes_bundled_resources_dir_name() {
    let script = install_script(None);
    assert!(
        !script.contains("{bundled_resources_dir_name}"),
        "the placeholder survived substitution -- install_script() is missing its \
         .replace() for it, so the script would create a directory named after the \
         literal placeholder",
    );
    assert!(
        script.contains(BUNDLED_RESOURCES_DIR_NAME),
        "the resources directory name never made it into the script",
    );
}

/// Drives the *actual* production script end to end against a fabricated
/// release tarball, so the assertion is about what a real install leaves on
/// disk rather than about the template's text.
///
/// Uses the staging-tarball path (`install_script(Some(..))`) to skip the
/// network download, and points `HOME` at a temp dir so `~/.phosphor/remote-server`
/// resolves inside it.
///
/// Gated to Unix because the test invokes `/bin/bash` and `tar` directly.
#[cfg(unix)]
#[test]
fn install_script_installs_binary_and_global_resources() {
    let (home, script, install_dir) = run_install_with_tarball(true);

    let binary = install_dir.join(format!("{}{}", binary_name(), version_suffix()));
    assert!(
        binary.is_file(),
        "binary was not installed at {binary:?} (script: {script})",
    );

    // The resources tree landed at the global, version-independent path the
    // daemon reads -- not under a versioned directory.
    let resources = install_dir.join(BUNDLED_RESOURCES_DIR_NAME);
    assert!(
        resources.is_dir(),
        "resources tree was not installed at {resources:?}",
    );
    assert_eq!(
        std::fs::read_to_string(resources.join("skills/demo.md")).unwrap(),
        "bundled skill",
        "resources tree was installed but its contents are wrong",
    );

    // No staging leftovers: a concurrent daemon start must not find a
    // half-populated directory or a stray .new/.old sibling.
    for entry in std::fs::read_dir(&install_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains(".new.") && !name.contains(".old."),
            "install left a staging directory behind: {name}",
        );
    }

    drop(home);
}

/// A tarball with no `resources/` tree must still install the binary and exit
/// zero. This is the normal case for dev-mode installs, which cross-compile a
/// bare binary, and for release artifacts that predate the resources tree.
#[cfg(unix)]
#[test]
fn install_script_tolerates_tarball_without_resources() {
    let (home, script, install_dir) = run_install_with_tarball(false);

    let binary = install_dir.join(format!("{}{}", binary_name(), version_suffix()));
    assert!(
        binary.is_file(),
        "binary was not installed at {binary:?} (script: {script})",
    );
    assert!(
        !install_dir.join(BUNDLED_RESOURCES_DIR_NAME).exists(),
        "the script invented a resources directory the tarball never contained",
    );

    drop(home);
}

/// Builds a release-shaped tarball (binary, plus a `resources/` tree when
/// `with_resources`), runs the production install script against it with
/// `HOME` pointed at a temp dir, and returns the temp dir guard, the script
/// text (for failure messages) and the resolved install directory.
#[cfg(unix)]
fn run_install_with_tarball(with_resources: bool) -> (tempfile::TempDir, String, std::path::PathBuf) {
    let home = tempfile::tempdir().expect("failed to create temp HOME");
    let home_path = home.path().to_path_buf();

    // Lay out the tarball contents: the binary the script looks for, and
    // optionally the resources tree the release pipeline ships.
    let staging = home_path.join("tarball-src");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join(binary_name()), "#!/bin/sh\nexit 0\n").unwrap();
    if with_resources {
        std::fs::create_dir_all(staging.join("resources/skills")).unwrap();
        std::fs::write(staging.join("resources/skills/demo.md"), "bundled skill").unwrap();
    }

    let tarball = home_path.join("phosphor-upload.tar.gz");
    let tar_status = Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(&staging)
        .arg(".")
        .status()
        .expect("failed to spawn tar");
    assert!(tar_status.success(), "tar failed to build the test artifact");

    let script = install_script(Some(tarball.to_str().unwrap()));

    let bash = if std::path::Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "bash"
    };
    let output = Command::new(bash)
        .arg("-c")
        .arg(&script)
        .env("HOME", &home_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to spawn bash");
    assert!(
        output.status.success(),
        "install script exited with {:?}: stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let install_dir = std::path::PathBuf::from(
        remote_server_dir().replacen('~', home_path.to_str().unwrap(), 1),
    );
    (home, script, install_dir)
}

// ---------------------------------------------------------------------------
// Remote-server tarball integrity verification
//
// The install script runs on a remote host and fetches the CLI tarball from GitHub over the
// public internet. The digest it checks against is compiled into this client and delivered
// inside the script text over the user's authenticated SSH connection, so a tampered release
// cannot install even if GitHub's copy is replaced. These tests cover the three outcomes that
// matter: the digest reaches the script, a good artifact installs, and a bad one does not.
// ---------------------------------------------------------------------------

#[test]
fn install_script_substitutes_every_platform_digest_placeholder() {
    let script = install_script(None);
    for placeholder in [
        "{sha256_linux_x86_64}",
        "{sha256_linux_aarch64}",
        "{sha256_macos_x86_64}",
        "{sha256_macos_aarch64}",
    ] {
        assert!(
            !script.contains(placeholder),
            "{placeholder} survived substitution -- install_script() is missing its \
             .replace() for it, so the script would compare the downloaded tarball against \
             the literal placeholder text and every install would fail",
        );
    }
}

#[test]
fn expected_sha256_is_empty_for_unknown_platform() {
    assert_eq!(expected_sha256("plan9", "x86_64"), "");
    assert_eq!(expected_sha256("linux", "riscv64"), "");
}

/// A client built without the pinned digests must REFUSE the download path, not warn and
/// continue. This is the fail-closed property: a misconfigured build must not quietly become
/// a build that installs unverified binaries.
///
/// Test builds never set `PHOSPHOR_CLI_SHA256_*`, so this exercises the real production
/// condition. It also proves the check runs *before* the download -- there is no network in
/// this test, and the script still exits with the documented code.
#[cfg(unix)]
#[test]
fn install_script_refuses_unverified_download_when_no_digest_is_pinned() {
    let home = tempfile::tempdir().expect("failed to create temp HOME");
    let script = install_script(None);

    let output = Command::new(bash_path())
        .arg("-c")
        .arg(&script)
        .env("HOME", home.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to spawn bash");

    assert_eq!(
        output.status.code(),
        Some(4),
        "expected the documented fail-closed exit code; stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing to install an unverified remote server"),
        "the refusal must say why; stderr={stderr}",
    );
}

/// A tarball whose digest matches installs normally.
#[cfg(unix)]
#[test]
fn install_script_accepts_download_matching_pinned_digest() {
    let (home, install_dir, output) = run_download_install_with_digest(DigestUnderTest::Matching);

    assert!(
        output.status.success(),
        "a matching digest must install; stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let binary = install_dir.join(format!("{}{}", binary_name(), version_suffix()));
    assert!(binary.is_file(), "binary was not installed at {binary:?}");

    drop(home);
}

/// The security property itself: a tarball that does NOT match the pinned digest is rejected,
/// and nothing is installed or made executable.
#[cfg(unix)]
#[test]
fn install_script_rejects_download_failing_pinned_digest() {
    let (home, install_dir, output) = run_download_install_with_digest(DigestUnderTest::Mismatched);

    assert_eq!(
        output.status.code(),
        Some(6),
        "expected the documented integrity-failure exit code; stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed integrity check"),
        "the rejection must name the cause; stderr={stderr}",
    );

    let binary = install_dir.join(format!("{}{}", binary_name(), version_suffix()));
    assert!(
        !binary.exists(),
        "a tarball that failed verification was installed anyway at {binary:?} -- \
         verification must happen before the binary is moved into place",
    );

    drop(home);
}

#[cfg(unix)]
enum DigestUnderTest {
    Matching,
    Mismatched,
}

/// Drives the production install script down its **download** branch without touching the
/// network, by putting a `curl` shim ahead of the real one on PATH that copies a local
/// tarball to the requested output path.
///
/// The script text itself is left intact apart from pinning a digest into the platform case
/// arms -- the digests are a compile-time input this test cannot set, so substituting the
/// value it would have had is the only way to exercise the comparison. The comparison logic,
/// the ordering relative to `chmod +x`/`mv`, and the exit codes are all the real thing.
#[cfg(unix)]
fn run_download_install_with_digest(
    which: DigestUnderTest,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::process::Output,
) {
    let home = tempfile::tempdir().expect("failed to create temp HOME");
    let home_path = home.path().to_path_buf();

    let staging = home_path.join("tarball-src");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join(binary_name()), "#!/bin/sh\nexit 0\n").unwrap();

    let tarball = home_path.join("release.tar.gz");
    let tar_status = Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(&staging)
        .arg(".")
        .status()
        .expect("failed to spawn tar");
    assert!(tar_status.success(), "tar failed to build the test artifact");

    let real_digest = sha256_of(&tarball);
    let pinned = match which {
        DigestUnderTest::Matching => real_digest,
        // Same length and alphabet as a real digest, so the failure is the comparison and
        // not some incidental parsing difference.
        DigestUnderTest::Mismatched => "0".repeat(64),
    };

    // A `curl` that serves the local tarball. Honours `-o <path>`, which is how the script
    // invokes it; everything else is ignored.
    let shim_dir = home_path.join("shim");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let curl_shim = shim_dir.join("curl");
    std::fs::write(
        &curl_shim,
        format!(
            "#!/bin/sh\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  \
             if [ \"$prev\" = \"-o\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\n\
             if [ -z \"$out\" ]; then exit 1; fi\ncp {} \"$out\"\n",
            tarball.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&curl_shim).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&curl_shim, perms).unwrap();

    // Pin the digest into every platform arm so the test does not depend on which platform
    // it happens to run on.
    let script = install_script(None).replace(
        "expected_sha256=\"\"",
        &format!("expected_sha256=\"{pinned}\""),
    );
    assert!(
        script.contains(&format!("expected_sha256=\"{pinned}\"")),
        "the test failed to pin a digest -- the script's case arms changed shape",
    );

    let path = format!(
        "{}:{}",
        shim_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(bash_path())
        .arg("-c")
        .arg(&script)
        .env("HOME", &home_path)
        .env("PATH", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to spawn bash");

    let install_dir = std::path::PathBuf::from(
        remote_server_dir().replacen('~', home_path.to_str().unwrap(), 1),
    );
    (home, install_dir, output)
}

#[cfg(unix)]
fn sha256_of(path: &std::path::Path) -> String {
    let (program, args): (&str, Vec<&str>) = if which_exists("sha256sum") {
        ("sha256sum", vec![])
    } else {
        ("shasum", vec!["-a", "256"])
    };
    let output = Command::new(program)
        .args(args)
        .arg(path)
        .output()
        .expect("failed to spawn a SHA-256 tool");
    assert!(output.status.success(), "SHA-256 tool failed");
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .expect("no digest in tool output")
        .to_owned()
}

#[cfg(unix)]
fn which_exists(program: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {program} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn bash_path() -> &'static str {
    if std::path::Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "bash"
    }
}
