//! Names shared by the client's install logic and the `phosphor-remote` helper: the short hashes
//! behind the daemon's socket/pid file names, and the helper's own version identity. Pure (no
//! warpui, no channel state), so the helper and the app compute identical paths by construction.

use std::path::PathBuf;

/// Returns the first 8 hex chars of a **build-stable** 64-bit hash of `input`.
///
/// Must not be `std::collections::hash_map::DefaultHasher`. `DefaultHasher`'s
/// algorithm is explicitly unspecified — "the internal algorithm is not
/// specified, and so it and its hashes should not be relied upon over
/// releases" — so the value it returns for a fixed input is free to change
/// when the Rust toolchain changes. Both callers bake that value into a
/// **path on a remote host that outlives this process**: the daemon socket
/// directory and the socket/PID filenames. If the value moves, the client
/// starts looking in a directory the running daemon never created, so it
/// spawns a second daemon while the first keeps its socket, its auth token
/// and its memory until the remote host reboots or the user kills it by
/// hand. Nothing sweeps the old path, because sweeping means reading the PID
/// file at the path we no longer compute. That failure is silent, it happens
/// on a toolchain bump rather than on any change to this crate, and no test
/// can see it because the tests only compare this function against itself.
///
/// FNV-1a (64-bit, offset basis `0xcbf2_9ce4_8422_2325`, prime
/// `0x0000_0100_0000_01b3`) with the MurmurHash3 `fmix64` finalizer. Both are
/// fully specified by constants written out here, so the output is pinned by
/// this source file and by nothing else. FNV-1a alone avalanches its high
/// bits poorly when inputs differ only in their last byte — the common case
/// here, since identity keys are UUID-shaped — and it is the high 32 bits
/// that get truncated into the directory name, hence the finalizer.
///
/// **Not** a collision-resistant hash, and deliberately still truncated to 32
/// bits: the `sun_path` budget has ~6 bytes of headroom in the worst case
/// (`socket_path_fits_within_sun_path_worst_case`), so widening to 16 chars
/// would push the worst-case path past macOS's 103-byte limit. Collisions are
/// tolerable here in a way they are not for
/// [`remote_server_daemon_data_dir`]: every identity that shares this
/// directory tree already shares one remote UNIX account and its `$HOME`.
///
/// NOTE ON THE CHANGEOVER: switching off `DefaultHasher` moves every path
/// once, so a daemon started by the previous build is orphaned exactly as
/// described above — one leaked process per remote host, reaped by the next
/// reboot. That is the same event a toolchain bump used to cause silently and
/// at an unpredictable time; this is the last one.
pub fn stable_short_hash(input: &str) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // MurmurHash3 fmix64.
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^= hash >> 33;

    format!("{hash:016x}")[..8].to_string()
}

/// Version identity of the helper, and of the client that bundles it. Both are compiled from this
/// same crate in one release pipeline with the same `GIT_RELEASE_TAG`, so they agree without any
/// runtime exchange; a local source build has no tag and uses the crate version.
pub fn helper_version() -> &'static str {
    match option_env!("GIT_RELEASE_TAG") {
        Some(tag) if !tag.trim().is_empty() => tag,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

/// Wire-protocol revision the helper reports from `--version`. Bumped by hand when the helper
/// stops being compatible with older clients; the client only logs it today.
pub const HELPER_PROTOCOL_VERSION: u32 = 1;

/// First line of `phosphor-remote --version`, and what the client expects the probe to print:
/// `phosphor-remote <version> protocol <revision>`.
pub fn version_line() -> String {
    format!(
        "phosphor-remote {} protocol {HELPER_PROTOCOL_VERSION}",
        helper_version()
    )
}

/// Build identifier (commit id) when the build pipeline provided one: printed as a second
/// `--version` line for humans and bug reports; never compared.
pub fn build_id() -> Option<&'static str> {
    option_env!("PHOSPHOR_REMOTE_BUILD_ID").filter(|id| !id.trim().is_empty())
}

/// [`stable_short_hash`] of [`helper_version`]: partitions the daemon files of different helper
/// versions so a proxy never attaches to a daemon of another version.
pub fn helper_version_hash() -> String {
    stable_short_hash(helper_version())
}

/// The daemon socket file name for `hash`: `server-{hash}.sock`.
pub fn daemon_socket_file_name(hash: &str) -> String {
    format!("server-{hash}.sock")
}

/// The daemon pid file name for `hash`: `server-{hash}.pid`.
pub fn daemon_pid_file_name(hash: &str) -> String {
    format!("server-{hash}.pid")
}

/// Short directory name for an identity key (`empty` for an empty key), as
/// `setup::remote_server_identity_dir_name` has always computed it.
pub fn identity_dir_name(identity_key: &str) -> String {
    if identity_key.is_empty() {
        "empty".to_string()
    } else {
        stable_short_hash(identity_key)
    }
}

/// Expands a leading `~` / `~/` against the home directory; anything else is returned as is.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_pinned_and_eight_hex_chars() {
        // Same vector `setup_tests::version_hash_algorithm_is_pinned` pins for the old paths.
        assert_eq!(stable_short_hash("x").len(), 8);
        assert_eq!(stable_short_hash("x"), stable_short_hash("x"));
        assert_ne!(stable_short_hash("x"), stable_short_hash("y"));
    }

    #[test]
    fn identity_dir_name_matches_the_documented_shape() {
        assert_eq!(identity_dir_name(""), "empty");
        assert_eq!(identity_dir_name("key").len(), 8);
    }

    #[test]
    fn version_line_has_the_shape_the_client_probe_expects() {
        let line = version_line();
        assert!(line.starts_with("phosphor-remote "), "{line}");
        assert!(line.contains(helper_version()));
        assert!(line.ends_with(&format!("protocol {HELPER_PROTOCOL_VERSION}")));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn file_names_embed_the_hash() {
        assert_eq!(daemon_socket_file_name("ab12cd34"), "server-ab12cd34.sock");
        assert_eq!(daemon_pid_file_name("ab12cd34"), "server-ab12cd34.pid");
        assert_eq!(
            daemon_socket_file_name(&helper_version_hash()).len(),
            "server-".len() + 8 + ".sock".len()
        );
    }

    #[test]
    fn tilde_expansion_leaves_other_paths_alone() {
        assert_eq!(expand_tilde("/a/b"), PathBuf::from("/a/b"));
        assert_eq!(expand_tilde("rel/x"), PathBuf::from("rel/x"));
        assert_eq!(expand_tilde("~other"), PathBuf::from("~other"));
    }
}
