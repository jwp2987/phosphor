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
    assert!(
        RemoteServerSetupState::Failed {
            error: "test".into()
        }
        .is_failed()
    );
    assert!(!RemoteServerSetupState::Ready.is_failed());
}

#[test]
fn state_is_terminal() {
    assert!(RemoteServerSetupState::Ready.is_terminal());
    assert!(
        RemoteServerSetupState::Failed {
            error: "test".into()
        }
        .is_terminal()
    );
    assert!(
        RemoteServerSetupState::Unsupported {
            reason: UnsupportedReason::NonGlibc {
                name: "musl".into()
            }
        }
        .is_terminal()
    );
    assert!(!RemoteServerSetupState::Checking.is_terminal());
    assert!(
        !RemoteServerSetupState::Installing {
            progress_percent: None,
        }
        .is_terminal()
    );
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
fn binary_name_is_the_bundled_helper_not_the_channel_cli() {
    // Owner decision (Wave H, #1804): the GitLab download of the full app binary is gone, so
    // the remote server is the bundled helper. `phosphor-oss` is only a leftover to clean up.
    assert_eq!(binary_name(), "phosphor-remote");
    assert_eq!(legacy_binary_name(), "phosphor-oss");
    assert!(remote_server_binary().ends_with(&helper_file_name()));
    assert!(helper_file_name().starts_with("phosphor-remote-"));
}

#[test]
fn nothing_in_the_install_path_can_reach_the_network() {
    // The owner rule: never download from GitLab or the internet. The script gets its bytes
    // from the client over the ssh channel it arrives on, and nothing else.
    let script = install_script(None, None);
    for forbidden in [
        "curl", "wget", "http://", "https://", "gitlab", "github", "fetch",
    ] {
        assert!(
            !script.to_ascii_lowercase().contains(forbidden),
            "the install script mentions {forbidden:?}"
        );
    }
    for source in [
        include_str!("setup.rs"),
        include_str!("../../../app/src/remote_server/ssh_transport.rs"),
    ] {
        assert!(!source.contains("RELEASE_REPO"));
        assert!(!source.contains("download_tarball_url"));
        assert!(!source.contains("fn download_url"));
    }
}

#[test]
fn helper_asset_names_follow_the_bundle_layout() {
    let platform = RemotePlatform {
        os: RemoteOs::Linux,
        arch: RemoteArch::Aarch64,
    };
    assert_eq!(
        helper_asset_name(&platform),
        "phosphor-remote-linux-aarch64"
    );
    let platform = RemotePlatform {
        os: RemoteOs::MacOs,
        arch: RemoteArch::X86_64,
    };
    assert_eq!(helper_asset_name(&platform), "phosphor-remote-macos-x86_64");
}

#[test]
fn install_script_renders_every_placeholder() {
    let script = install_script(Some("~/.phosphor/remote-server/.upload.gz"), None);
    for placeholder in [
        "{install_dir}",
        "{helper_file_name}",
        "{helper_current_link}",
        "{one_version_binaries}",
        "{daemon_version_hash}",
        "{staging_gz_path}",
        "{one_version_cleanup}",
        "{sha256_linux_x86_64}",
        "{sha256_linux_aarch64}",
        "{sha256_macos_x86_64}",
        "{sha256_macos_aarch64}",
    ] {
        assert!(
            !script.contains(placeholder),
            "{placeholder} survived substitution -- install_script() is missing its .replace()"
        );
    }
    assert!(script.contains("staging_gz_path=\"~/.phosphor/remote-server/.upload.gz\""));
    assert!(script.contains(&helper_file_name()));
}

#[test]
fn the_script_ends_in_a_single_call_so_the_payload_stays_unread() {
    let script = install_script(None, None);
    let last = script.lines().rev().find(|l| !l.trim().is_empty()).unwrap();
    assert_eq!(
        last, "main",
        "commands after `main` would never run in-band"
    );
    assert!(script.trim_end().ends_with("\nmain"));
}

#[test]
fn script_override_replaces_all_four_digest_placeholders() {
    let digest = "ab".repeat(32);
    let script = install_script(None, Some(&digest));
    assert_eq!(
        script
            .matches(&format!("expected_sha256=\"{digest}\""))
            .count(),
        4
    );
}

#[test]
fn pinned_helper_sha256_is_none_for_unknown_platforms() {
    assert_eq!(pinned_helper_sha256("plan9", "x86_64"), None);
    assert_eq!(pinned_helper_sha256("linux", "riscv64"), None);
}

#[test]
fn digest_decision_fails_closed_unless_a_source_build_computed_its_own() {
    let actual = "ab".repeat(32);
    // Pinned and equal (case-insensitively): the pinned digest is used as is.
    let upper = actual.to_ascii_uppercase();
    assert_eq!(
        decide_helper_digest(Some(&upper), &actual, false),
        Ok(HelperDigest::Pinned(actual.clone()))
    );
    assert_eq!(HelperDigest::Pinned(actual.clone()).script_override(), None);
    // Pinned and different: refused on every kind of build, with the reason.
    for dev in [false, true] {
        let err = decide_helper_digest(Some(&"0".repeat(64)), &actual, dev).unwrap_err();
        assert!(err.contains("does not match the checksum"), "{err}");
    }
    // Not pinned: a release build refuses, a source build continues on what it computed.
    assert_eq!(
        decide_helper_digest(None, &actual, false),
        Err(INSTALL_REFUSED_NO_PINNED_DIGEST.to_string())
    );
    let computed = decide_helper_digest(None, &actual, true).unwrap();
    assert_eq!(computed, HelperDigest::Computed(actual.clone()));
    assert_eq!(computed.script_override(), Some(actual.as_str()));
}

#[test]
fn staged_upload_name_is_what_failure_cleanup_removes() {
    let cmd = install_failure_cleanup_command_for("~/d", "~/d/phosphor-remote-v1");
    assert!(cmd.contains(&format!("~/d/{STAGED_UPLOAD_NAME}")));
    assert!(cmd.contains("~/d/.up.*"));
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

/// Regression: the install script\'s tilde-expansion logic must work across the bash versions
/// we actually invoke at install time (`run_ssh_script` pipes the script into `bash -s` on the
/// remote). Two interpreter quirks have to be avoided simultaneously:
///
///   1. bash 3.2 (macOS `/bin/bash`) keeps inner double-quotes around the replacement of
///      `${var/pattern/replacement}` literal, so `"$HOME"` ends up as 6 literal characters.
///   2. bash 5.2+ with `patsub_replacement` (default-on) treats `&` in the replacement as the
///      matched pattern, so a `$HOME` containing `&` resolves wrongly.
///
/// This test drives the *actual* production script (via [`install_script`]), truncated at the
/// `mkdir -p` checkpoint so no filesystem side effects leak, with the function body closed and
/// called so the probe is valid bash, and a marker `printf` capturing the resolved `install_dir`.
#[cfg(unix)]
#[test]
fn install_script_tilde_expansion_resolves_correctly() {
    let script = install_script(None, None);
    let cutoff = script.find("mkdir -p \"$install_dir\"").expect(
        "install script no longer contains the `mkdir -p \"$install_dir\"` checkpoint \
         this test relies on; update the test alongside the script change",
    );
    // The script body lives inside `main() {`; close it and call it.
    let probe = format!(
        "{prefix}\nprintf '%s' \"$install_dir\"\nexit 0\n}}\nmain\n",
        prefix = &script[..cutoff],
    );

    let cases = [
        ("/Users/test", "ordinary HOME"),
        (
            "/Users/A&B",
            "HOME with `&` (bash 5.2 patsub_replacement trap)",
        ),
    ];
    for (fake_home, label) in cases {
        let output = Command::new(bash_path())
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
            "[{label}] install_dir contains literal quote characters: {install_dir:?}",
        );
        let expected = remote_server_dir().replacen('~', fake_home, 1);
        assert_eq!(
            install_dir, expected,
            "[{label}] install_dir resolved incorrectly"
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

/// Pins `stable_short_hash`'s actual algorithm (FNV-1a, 64-bit, offset basis
/// `0xcbf2_9ce4_8422_2325`, prime `0x0000_0100_0000_01b3`, then the
/// MurmurHash3 `fmix64` finalizer) against literals computed offline from
/// that spec -- never by calling the function under test, which is exactly
/// what let the previous version of this test (`version_hash_is_deterministic`)
/// stay green while asserting nothing about this module.
///
/// That previous test re-implemented `std::collections::hash_map::DefaultHasher`
/// inline and asserted the copy against itself: `version_hash` was never
/// called, `stable_short_hash` was never called, and `DefaultHasher`'s
/// algorithm is explicitly unspecified across releases (see
/// `stable_short_hash`'s doc comment) -- the opposite of what production
/// switched to on 2026-08-21 specifically to avoid a silent path change on a
/// toolchain bump. It could not have caught that switch, or any future change
/// to the real algorithm; a pinned literal is the only assertion that can.
///
/// Goes through the public `remote_server_identity_dir_name` rather than
/// calling the private `stable_short_hash` directly, since both `version_hash`
/// and `remote_server_identity_dir_name` share the same underlying hash --
/// pinning one pins the other.
#[test]
fn version_hash_algorithm_is_pinned() {
    assert_eq!(
        remote_server_identity_dir_name("v0.2026.05.13.09.15.stable_01"),
        "725b7abd"
    );
    // A one-character change in the input must change the output -- guards
    // against a degenerate "always returns a fixed string" regression that a
    // single pinned literal alone would not catch.
    assert_eq!(
        remote_server_identity_dir_name("v0.2026.05.14.09.15.stable_01"),
        "1b6881b6"
    );
}

#[test]
fn bundled_resources_dir_is_global_and_version_independent() {
    let dir = remote_server_bundled_resources_dir();
    assert_eq!(
        dir,
        format!("{}/{}", remote_server_dir(), BUNDLED_RESOURCES_DIR_NAME)
    );
    // The whole point of the global location: no version in the path.
    assert!(!dir.contains(crate::naming::helper_version()));
}

#[test]
fn removal_command_removes_binary_but_leaves_global_resources() {
    let command = remote_server_removal_command();
    assert_eq!(command, format!("rm -f {}", remote_server_binary()));
    assert!(!command.contains(BUNDLED_RESOURCES_DIR_NAME));
}

// ---------------------------------------------------------------------------
// The helper install script, driven for real with bash (Wave H step 5, #1804)
//
// The script is piped into `bash -s` with the gzip-compressed helper appended on the same
// stdin, exactly as `ssh::run_ssh_script_with_payload` does over the ControlMaster. These tests
// cover the outcomes that matter: a good helper installs, a bad one does not, and each failure
// arrives as its own documented exit code.
// ---------------------------------------------------------------------------

/// What the script is given in a test.
#[cfg(unix)]
struct InstallRun {
    home: tempfile::TempDir,
    install_dir: std::path::PathBuf,
    output: std::process::Output,
}

/// A stand-in helper: starts, answers `--version`, does nothing else.
#[cfg(unix)]
const FAKE_HELPER: &[u8] = b"#!/bin/sh\nexit 0\n";

#[cfg(unix)]
fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut child = std::process::Command::new("gzip")
        .arg("-c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn gzip");
    child.stdin.take().unwrap().write_all(raw).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    output.stdout
}

#[cfg(unix)]
fn sha256_of_bytes(raw: &[u8]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f");
    std::fs::write(&path, raw).unwrap();
    sha256_of(&path)
}

/// Runs `script` under bash with `payload` on stdin after it, `HOME` in a fresh temp dir that
/// `seed` may populate, and `shim_dir` (if any) first on PATH.
#[cfg(unix)]
fn run_script(
    script: &str,
    payload: &[u8],
    seed: impl FnOnce(&std::path::Path),
    shims: &[(&str, &str)],
) -> InstallRun {
    use std::io::Write as _;
    let home = tempfile::tempdir().expect("failed to create temp HOME");
    let home_path = home.path().to_path_buf();
    let install_dir =
        std::path::PathBuf::from(remote_server_dir().replacen('~', home_path.to_str().unwrap(), 1));
    seed(&install_dir);

    let shim_dir = home_path.join("shim");
    std::fs::create_dir_all(&shim_dir).unwrap();
    for (tool, body) in shims {
        write_shim(&shim_dir.join(tool), body);
    }
    let path = format!(
        "{}:{}",
        shim_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut child = std::process::Command::new(bash_path())
        .arg("-s")
        .env("HOME", &home_path)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn bash");
    let mut stdin = child.stdin.take().unwrap();
    // A script that exits early closes the pipe; the broken pipe is the script's verdict, not
    // this test's failure.
    let _ = stdin.write_all(script.as_bytes());
    let _ = stdin.write_all(payload);
    drop(stdin);
    let output = child.wait_with_output().expect("bash did not finish");
    InstallRun {
        home,
        install_dir,
        output,
    }
}

#[cfg(unix)]
fn stderr_of(run: &InstallRun) -> String {
    String::from_utf8_lossy(&run.output.stderr).into_owned()
}

#[cfg(unix)]
fn installed_helper(run: &InstallRun) -> std::path::PathBuf {
    run.install_dir.join(helper_file_name())
}

/// The happy path, in-band: the helper arrives on stdin behind the script, is unpacked,
/// verified, made executable, linked as `current`, and nothing else is left in the directory.
#[cfg(unix)]
#[test]
fn in_band_upload_installs_verifies_and_links_the_helper() {
    use std::os::unix::fs::PermissionsExt as _;
    let digest = sha256_of_bytes(FAKE_HELPER);
    let script = install_script(None, Some(&digest));
    let run = run_script(&script, &gzip_bytes(FAKE_HELPER), |_| {}, &[]);
    assert!(run.output.status.success(), "stderr={}", stderr_of(&run));

    let helper = installed_helper(&run);
    assert_eq!(std::fs::read(&helper).unwrap(), FAKE_HELPER);
    assert_eq!(
        std::fs::metadata(&helper).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::read_link(run.install_dir.join(HELPER_CURRENT_LINK)).unwrap(),
        std::path::PathBuf::from(helper_file_name())
    );
    // A directory this run created is private to the user.
    assert_eq!(
        std::fs::metadata(&run.install_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let leftovers: Vec<_> = std::fs::read_dir(&run.install_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".up"))
        .collect();
    assert!(leftovers.is_empty(), "staging leftovers: {leftovers:?}");
    drop(run.home);
}

/// The SCP fallback: the file is already on the host, the script reads it (not stdin) and
/// deletes it after unpacking.
#[cfg(unix)]
#[test]
fn staged_upload_installs_and_removes_the_staged_file() {
    let digest = sha256_of_bytes(FAKE_HELPER);
    let staged = format!("{}/{}", remote_server_dir(), STAGED_UPLOAD_NAME);
    let script = install_script(Some(&staged), Some(&digest));
    let run = run_script(
        &script,
        b"",
        |install_dir| {
            std::fs::create_dir_all(install_dir).unwrap();
            std::fs::write(
                install_dir.join(STAGED_UPLOAD_NAME),
                gzip_bytes(FAKE_HELPER),
            )
            .unwrap();
        },
        &[],
    );
    assert!(run.output.status.success(), "stderr={}", stderr_of(&run));
    assert!(installed_helper(&run).is_file());
    assert!(!run.install_dir.join(STAGED_UPLOAD_NAME).exists());
    drop(run.home);
}

/// The security property itself: bytes that do not match the pinned digest are rejected, and
/// nothing is installed, linked, or made executable.
#[cfg(unix)]
#[test]
fn a_helper_failing_the_pinned_digest_is_rejected_with_the_integrity_code() {
    let script = install_script(None, Some(&"0".repeat(64)));
    let run = run_script(&script, &gzip_bytes(FAKE_HELPER), |_| {}, &[]);
    assert_eq!(
        run.output.status.code(),
        Some(6),
        "stderr={}",
        stderr_of(&run)
    );
    assert!(stderr_of(&run).contains("failed integrity check"));
    assert!(!installed_helper(&run).exists());
    assert!(!run.install_dir.join(HELPER_CURRENT_LINK).exists());
    drop(run.home);
}

/// A client built without a pinned digest refuses: fail closed, before reading any payload.
#[cfg(unix)]
#[test]
fn no_pinned_digest_is_refused_before_anything_is_read() {
    let script = install_script(None, Some(""));
    let run = run_script(&script, &gzip_bytes(FAKE_HELPER), |_| {}, &[]);
    assert_eq!(
        run.output.status.code(),
        Some(4),
        "stderr={}",
        stderr_of(&run)
    );
    assert!(stderr_of(&run).contains("refusing to install an unverified remote helper"));
    assert!(
        !run.install_dir.exists(),
        "a refused run left its directory behind"
    );
    drop(run.home);
}

/// "Could not compute a digest" must never be reported as "computed, and it is not the pinned
/// helper": opposite verdicts (see EXIT_NO_DIGEST_TOOL). The bytes here MATCH the pin; the only
/// variable is the broken tool, shadowed ahead of the real ones on PATH.
#[cfg(unix)]
#[test]
fn unusable_digest_tool_reports_no_digest_tool_never_a_mismatch() {
    let digest = sha256_of_bytes(FAKE_HELPER);
    for (situation, shim) in [
        ("a digest tool that exists and fails", "#!/bin/sh\nexit 1\n"),
        (
            "a digest tool that exits 0 and prints nothing",
            "#!/bin/sh\nexit 0\n",
        ),
        (
            "a digest tool that prints something that is not a digest",
            "#!/bin/sh\necho 'no digest for you'\n",
        ),
    ] {
        let script = install_script(None, Some(&digest));
        let shims = [("sha256sum", shim), ("shasum", shim), ("openssl", shim)];
        let run = run_script(&script, &gzip_bytes(FAKE_HELPER), |_| {}, &shims);
        assert_ne!(
            run.output.status.code(),
            Some(6),
            "{situation}: reported as tampering"
        );
        assert_eq!(
            run.output.status.code(),
            Some(5),
            "{situation}: {}",
            stderr_of(&run)
        );
        assert!(stderr_of(&run).contains("could not compute a SHA-256"));
        assert!(!installed_helper(&run).exists(), "{situation}");
        drop(run.home);
    }
}

/// Bytes that never arrive are a transport problem (7, SCP is a legitimate retry); bytes that
/// arrive but are not a gzip stream that decodes to anything are not (7 when nothing decoded).
#[cfg(unix)]
#[test]
fn an_empty_or_broken_stream_is_an_upload_failure_not_a_verdict() {
    let digest = sha256_of_bytes(FAKE_HELPER);
    let script = install_script(None, Some(&digest));
    for (situation, payload) in [
        ("no payload at all", Vec::new()),
        ("garbage that is not gzip", b"definitely not gzip".to_vec()),
    ] {
        let run = run_script(&script, &payload, |_| {}, &[]);
        assert_eq!(
            run.output.status.code(),
            Some(7),
            "{situation}: {}",
            stderr_of(&run)
        );
        assert!(!installed_helper(&run).exists());
        assert!(
            !run.install_dir.exists(),
            "{situation}: a failed run kept the directory it made"
        );
        drop(run.home);
    }
}

/// A helper that unpacks and matches its digest but does not start on this host is removed and
/// reported as an install failure, and nothing older is touched.
#[cfg(unix)]
#[test]
fn a_helper_that_does_not_start_is_removed_and_old_versions_are_kept() {
    let broken: &[u8] = b"#!/bin/sh\nexit 1\n";
    let digest = sha256_of_bytes(broken);
    let script = install_script(None, Some(&digest));
    let run = run_script(
        &script,
        &gzip_bytes(broken),
        |install_dir| {
            std::fs::create_dir_all(install_dir).unwrap();
            touch_executable(&install_dir.join("phosphor-remote-v0.0.1"));
        },
        &[],
    );
    assert_eq!(
        run.output.status.code(),
        Some(9),
        "stderr={}",
        stderr_of(&run)
    );
    assert!(!installed_helper(&run).exists());
    assert!(run.install_dir.join("phosphor-remote-v0.0.1").exists());
    drop(run.home);
}

/// After a verified install exactly one version remains: older helpers, the previous remote
/// server\'s leftovers under the channel\'s CLI name, and the legacy unversioned binary go.
#[cfg(unix)]
#[test]
fn install_leaves_exactly_one_version_and_removes_the_legacy_app_binary() {
    let digest = sha256_of_bytes(FAKE_HELPER);
    let script = install_script(None, Some(&digest));
    let run = run_script(
        &script,
        &gzip_bytes(FAKE_HELPER),
        |install_dir| {
            std::fs::create_dir_all(install_dir).unwrap();
            for name in [
                "phosphor-remote-v0.0.1",
                "phosphor-remote-v0.0.2",
                "phosphor-oss",
                "phosphor-oss-v0.1.17",
            ] {
                touch_executable(&install_dir.join(name));
            }
            std::fs::write(install_dir.join("unrelated"), "x").unwrap();
        },
        &[],
    );
    assert!(run.output.status.success(), "stderr={}", stderr_of(&run));
    assert_eq!(
        binaries_in(&run.install_dir, "phosphor-"),
        vec![helper_file_name()],
        "exactly one phosphor-* file must remain"
    );
    assert!(run.install_dir.join("unrelated").exists());
    drop(run.home);
}

/// A failed run that created the install directory removes it again; a pre-existing directory
/// (holding a working install) is never removed.
#[cfg(unix)]
#[test]
fn install_script_failure_does_not_leave_an_empty_install_dir() {
    let script = install_script(None, Some(""));
    let run = run_script(&script, b"", |_| {}, &[]);
    assert_eq!(run.output.status.code(), Some(4));
    assert!(!run.install_dir.exists());
    drop(run.home);

    let run = run_script(
        &script,
        b"",
        |install_dir| {
            std::fs::create_dir_all(install_dir).unwrap();
            std::fs::write(install_dir.join("keep-me"), "x").unwrap();
        },
        &[],
    );
    assert_eq!(run.output.status.code(), Some(4));
    assert!(run.install_dir.join("keep-me").exists());
    let leftovers: Vec<_> = std::fs::read_dir(&run.install_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "unexpected leftovers: {leftovers:?}");
    drop(run.home);
}

/// No code in the script\'s contract may collide with a status bash emits on its own behalf:
/// 1 (general error), 2 (usage/parse error), 126, 127 and 128+N (signals). A parse error aborts
/// the interpreter before any trap exists, and `install_script` builds this script by
/// substituting placeholders at runtime, so a mangled script must land in the client\'s
/// unrecognised-code branch (fail closed), not impersonate a verdict. 255 is OpenSSH\'s own.
#[test]
fn install_script_exit_codes_avoid_bash_reserved_statuses() {
    let script = install_script(None, None);
    let declared: Vec<(String, i32)> = script
        .lines()
        .filter_map(|line| line.trim().strip_prefix("EXIT_")?.split_once('='))
        .filter_map(|(name, value)| Some((format!("EXIT_{name}"), value.trim().parse().ok()?)))
        .collect();
    assert!(
        declared.len() >= 8,
        "expected the script to declare its exit-code contract as EXIT_*= assignments, \
         found {declared:?}",
    );
    for (name, code) in &declared {
        let reserved = matches!(*code, 0 | 1 | 2 | 126 | 127 | 255) || (128..=192).contains(code);
        assert!(
            !reserved,
            "{name}={code} collides with a status bash (or ssh) emits"
        );
    }
    for (index, (name, code)) in declared.iter().enumerate() {
        for (other_name, other_code) in &declared[index + 1..] {
            assert_ne!(
                code, other_code,
                "{name} and {other_name} share exit {code}"
            );
        }
    }
}

#[cfg(unix)]
fn write_shim(path: &std::path::Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(path, perms).unwrap();
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

/// `is_dev_source_build()` must be decided by "is this a distributed artifact"
/// (bundle feature + release tag) alone, never by the cargo profile.
///
/// Keying it on `debug_assertions` left `cargo run --release` in a gap where
/// neither install path worked: the dev cross-compile path was off because the
/// profile was release, and the download path 404s without a release tag. The
/// remote server then never installed and sessions silently fell back to the
/// ControlMaster executor, which opens an SSH channel per command and trips the
/// remote's `MaxSessions`.
///
/// Note this assertion can only *fail* under `cargo test --release` — in a debug
/// test run the removed `cfg!(debug_assertions)` term was true anyway, so both
/// the old and new implementations agree. That is precisely the blind spot that
/// let the original bug through, so the test is written to be meaningful in the
/// profile where it matters rather than omitted.
#[test]
fn dev_source_build_is_independent_of_the_cargo_profile() {
    // The env override short-circuits the whole function; skip if it is set.
    if std::env::var_os("WARP_REMOTE_SERVER_FROM_LOCAL").is_some() {
        return;
    }

    let is_distributed_artifact =
        ChannelState::is_release_bundle() || ChannelState::app_version().is_some();

    assert_eq!(
        is_dev_source_build(),
        !is_distributed_artifact,
        "is_dev_source_build() must depend only on the bundle feature and the release tag; \
         a release-profile source build is still a source build"
    );
}

// ---------------------------------------------------------------------------
// #931: a failed install must not leave an empty install directory that later
// reads as "already installed", and the prior-install probe must demand a
// complete executable binary, not just the directory.
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn sh_status(cmd: &str) -> i32 {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("failed to spawn sh")
        .code()
        .expect("terminated by signal")
}

/// The remote login shell may be fish/csh; the probe is wrapped in `sh -c '...'`, so run the
/// full command string through `sh -c` exactly like the ssh layer hands it over.
#[cfg(unix)]
#[test]
fn prior_install_probe_requires_an_executable_binary_not_just_the_directory() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("remote-server");
    let d = dir.to_str().unwrap();
    let probe = prior_install_probe_command_for(d, "phosphor-oss");

    // No directory at all.
    assert_eq!(sh_status(&probe), 1);

    // The empty directory a failed install used to leave behind: NOT an install.
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        sh_status(&probe),
        1,
        "an empty directory is not a prior install"
    );

    // Unrelated files (bundled resources, pid files, a staged tarball) are not an install.
    std::fs::create_dir_all(dir.join("bundled_resources")).unwrap();
    std::fs::write(dir.join("server-abc.pid"), "1").unwrap();
    std::fs::write(dir.join(".upload.gz"), "x").unwrap();
    assert_eq!(sh_status(&probe), 1);

    // A non-executable (partial) binary is not an install.
    let bin = dir.join("phosphor-oss-0.1.12");
    std::fs::write(&bin, "partial").unwrap();
    assert_eq!(
        sh_status(&probe),
        1,
        "a non-executable binary is not an install"
    );

    // A complete, executable versioned binary is.
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(sh_status(&probe), 0);

    // So is the unversioned dev-slot binary.
    std::fs::remove_file(&bin).unwrap();
    let dev = dir.join("phosphor-oss");
    std::fs::write(&dev, "x").unwrap();
    std::fs::set_permissions(&dev, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(sh_status(&probe), 0);
}

#[cfg(unix)]
#[test]
fn failure_cleanup_removes_leftovers_but_never_a_working_install() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("remote-server");
    let d = dir.to_str().unwrap();
    let bin = format!("{d}/phosphor-oss-0.1.12");
    let cleanup = install_failure_cleanup_command_for(d, &bin);

    // Empty directory (also what OLD builds left behind): repaired.
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(sh_status(&cleanup), 0);
    assert!(!dir.exists(), "an empty install dir must be removed");

    // Staged tarball + partial non-executable binary: both removed, then the dir.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".upload.gz"), "x").unwrap();
    std::fs::write(&bin, "partial").unwrap();
    assert_eq!(sh_status(&cleanup), 0);
    assert!(!dir.exists());

    // A working install (executable binary + resources) is left completely alone.
    std::fs::create_dir_all(dir.join("bundled_resources")).unwrap();
    std::fs::write(&bin, "good").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(sh_status(&cleanup), 0);
    assert!(std::path::Path::new(&bin).exists());
    assert!(dir.join("bundled_resources").is_dir());

    // Missing directory: still exits 0.
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(sh_status(&cleanup), 0);
}

// ---------------------------------------------------------------------------
// One version only: after a verified install the host keeps exactly one version (Wave H step 1)
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn touch_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn binaries_in(dir: &std::path::Path, prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect();
    names.sort();
    names
}

#[test]
fn one_version_cleanup_fragment_is_safe_inside_single_quotes() {
    // The standalone command embeds the fragment in `sh -c '...'`; one stray apostrophe in a
    // comment would end the quoting early and run the rest of the text in the login shell.
    assert!(
        !ONE_VERSION_CLEANUP_FRAGMENT.contains('\''),
        "one_version_cleanup.sh must not contain a single quote"
    );
}

#[cfg(unix)]
#[test]
fn one_version_cleanup_keeps_only_the_named_binary() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("remote-server");
    std::fs::create_dir_all(dir.join("bundled_resources")).unwrap();
    for name in [
        "phosphor-oss",
        "phosphor-oss-v0.1.11",
        "phosphor-oss-v0.1.12",
        "phosphor-oss-v0.1.13",
    ] {
        touch_executable(&dir.join(name));
    }
    std::fs::write(dir.join("unrelated-file"), "x").unwrap();
    let cmd = one_version_cleanup_command_for(
        dir.to_str().unwrap(),
        "phosphor-oss",
        "phosphor-oss-v0.1.12",
        "aaaa1111",
    );
    assert_eq!(sh_status(&cmd), 0);
    assert_eq!(
        binaries_in(&dir, "phosphor-"),
        vec!["phosphor-oss-v0.1.12".to_string()],
        "exactly one phosphor-* file must remain"
    );
    assert!(dir.join("unrelated-file").exists());
    assert!(dir.join("bundled_resources").is_dir());
    // Idempotent, and a missing directory is not an error.
    assert_eq!(sh_status(&cmd), 0);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(sh_status(&cmd), 0);
}

#[cfg(unix)]
#[test]
fn one_version_cleanup_keeps_the_unversioned_dev_binary_when_it_is_the_one_kept() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("remote-server");
    std::fs::create_dir_all(&dir).unwrap();
    touch_executable(&dir.join("phosphor-oss"));
    touch_executable(&dir.join("phosphor-oss-v0.1.12"));
    let cmd =
        one_version_cleanup_command_for(dir.to_str().unwrap(), "phosphor-oss", "phosphor-oss", "");
    assert_eq!(sh_status(&cmd), 0);
    assert_eq!(
        binaries_in(&dir, "phosphor-"),
        vec!["phosphor-oss".to_string()]
    );
}

#[cfg(unix)]
#[test]
fn one_version_cleanup_removes_dead_daemon_files_of_other_versions_but_not_live_ones() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("remote-server");
    let ident = dir.join("1a2b3c4d");
    std::fs::create_dir_all(&ident).unwrap();
    touch_executable(&dir.join("phosphor-oss-v0.1.12"));
    // Dead daemon of an old version: socket and pid both go.
    std::fs::write(ident.join("server-00000001.pid"), "999999999").unwrap();
    std::fs::write(ident.join("server-00000001.sock"), "").unwrap();
    // Live daemon of another version (this very process): left completely alone.
    std::fs::write(
        ident.join("server-00000002.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    std::fs::write(ident.join("server-00000002.sock"), "").unwrap();
    // Legacy unversioned files of a dead daemon: removed once the kept build is versioned.
    std::fs::write(ident.join("server.pid"), "999999999").unwrap();
    std::fs::write(ident.join("server.sock"), "").unwrap();
    // The kept version: untouched even though nothing is running.
    std::fs::write(ident.join("server-0badf00d.pid"), "999999999").unwrap();
    std::fs::write(ident.join("server-0badf00d.sock"), "").unwrap();
    // A socket with no pid file at all (crashed daemon) is litter too.
    std::fs::write(ident.join("server-00000003.sock"), "").unwrap();

    let cmd = one_version_cleanup_command_for(
        dir.to_str().unwrap(),
        "phosphor-oss",
        "phosphor-oss-v0.1.12",
        "0badf00d",
    );
    assert_eq!(sh_status(&cmd), 0);
    let mut left: Vec<String> = std::fs::read_dir(&ident)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec![
            "server-00000002.pid",
            "server-00000002.sock",
            "server-0badf00d.pid",
            "server-0badf00d.sock",
        ]
    );
}

#[cfg(unix)]
#[test]
fn one_version_cleanup_with_no_hash_sweeps_no_daemon_files() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("remote-server");
    let ident = dir.join("1a2b3c4d");
    std::fs::create_dir_all(&ident).unwrap();
    std::fs::write(ident.join("server.pid"), "999999999").unwrap();
    std::fs::write(ident.join("server.sock"), "").unwrap();
    let cmd =
        one_version_cleanup_command_for(dir.to_str().unwrap(), "phosphor-oss", "phosphor-oss", "");
    assert_eq!(sh_status(&cmd), 0);
    assert!(ident.join("server.pid").exists() && ident.join("server.sock").exists());
}

/// The quiet fall-back is keyed on the message prefix, so it must match the messages the
/// transport builds and nothing the install script can say.
#[test]
fn only_helper_unavailable_messages_take_the_quiet_fallback() {
    assert!(is_helper_unavailable(&format!(
        "{HELPER_UNAVAILABLE_PREFIX}linux-aarch64 (this app bundle has none); continuing without it"
    )));
    for real_failure in [
        INSTALL_REFUSED_NO_PINNED_DIGEST,
        INSTALL_REFUSED_DIGEST_MISMATCH_LOCAL,
        "install script failed (exit 9): boom",
        "ssh connection dropped",
    ] {
        assert!(!is_helper_unavailable(real_failure), "{real_failure}");
    }
}
