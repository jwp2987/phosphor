use dirs::home_dir;

use super::*;

#[test]
fn test_data_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    // ChannelState, by default, is configured for Channel::Oss.
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            // Channel-keyed rather than app-id-keyed, but renamed alongside it.
            assert_eq!(data_dir(), home_dir.join(".phosphor"));
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            assert_eq!(data_dir(), home_dir.join(".local/share/phosphor"));
        } else if #[cfg(windows)] {
            assert_eq!(data_dir(), home_dir.join("AppData\\Roaming\\phosphor\\Phosphor\\data"));
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_config_local_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    // ChannelState, by default, is configured for Channel::Oss.
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            // Channel-keyed rather than app-id-keyed, but renamed alongside it.
            assert_eq!(config_local_dir(), home_dir.join(".phosphor"));
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            assert_eq!(config_local_dir(), home_dir.join(".config/phosphor"));
        } else if #[cfg(windows)] {
            assert_eq!(config_local_dir(), home_dir.join("AppData\\Local\\phosphor\\Phosphor\\config"));
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_macos_config_dir_name_scopes_to_data_profile() {
    assert_eq!(macos_config_dir_name_for(Channel::Stable, None), ".warp");
    assert_eq!(
        macos_config_dir_name_for(Channel::Local, None),
        ".warp-local"
    );

    // Each development profile must get its own directory so shared config
    // (notably settings.toml) cannot leak between profiles.
    assert_eq!(
        macos_config_dir_name_for(Channel::Local, Some("myprofile")),
        ".warp-local-myprofile"
    );
    assert_eq!(
        macos_config_dir_name_for(Channel::Stable, Some("myprofile")),
        ".warp-myprofile"
    );
}

#[test]
fn test_warp_home_config_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    let expected_dir_name = match ChannelState::data_profile() {
        Some(data_profile) => format!(".phosphor-{data_profile}"),
        None => ".phosphor".to_string(),
    };

    assert_eq!(
        warp_home_config_dir(),
        Some(home_dir.join(expected_dir_name))
    );
}

#[test]
fn test_warp_home_skills_and_mcp_paths() {
    let Some(config_dir) = warp_home_config_dir() else {
        panic!("Should be able to compute Zap home config directory");
    };

    assert_eq!(warp_home_skills_dir(), Some(config_dir.join("skills")));
    assert_eq!(
        warp_home_mcp_config_file_path(),
        Some(config_dir.join(".mcp.json"))
    );
}
#[test]
fn test_cache_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    // ChannelState, by default, is configured for Channel::Oss.
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(cache_dir(), home_dir.join("Library/Application Support/dev.phosphor.Phosphor"));
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            assert_eq!(cache_dir(), home_dir.join(".cache/phosphor"));
        } else if #[cfg(windows)] {
            assert_eq!(cache_dir(), home_dir.join("AppData\\Local\\phosphor\\Phosphor\\cache"));
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_state_dir_path() {
    let home_dir = home_dir().expect("Should be able to compute home directory");
    cfg_if::cfg_if! {
        // ChannelState, by default, is configured for Channel::Oss.
        if #[cfg(target_os = "macos")] {
            assert_eq!(state_dir(), home_dir.join("Library/Application Support/dev.phosphor.Phosphor"));
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            assert_eq!(state_dir(), home_dir.join(".local/state/phosphor"));
        } else if #[cfg(windows)] {
            assert_eq!(state_dir(), home_dir.join("AppData\\Local\\phosphor\\Phosphor\\data"));
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}

/// The channel gate behind [`secure_state_dir`], tested directly.
///
/// This replaces an `assert_eq!(secure_state_dir(), None)` that could not fail
/// on the platform CI runs. Off macOS `secure_state_dir` has no non-`None`
/// return path at all — the App Group lookup is inside
/// `#[cfg(target_os = "macos")]` — so on Linux that assertion held with the
/// `Channel::Oss` arm deleted, i.e. it advertised coverage of the gate while
/// testing nothing. The gate itself is platform-independent, so testing it
/// directly bites everywhere.
#[test]
fn test_secure_state_dir_channel_gate() {
    // Zap must not resolve its state through the official Zap App Group from an
    // OSS build: macOS treats that as accessing another app's data and prompts
    // for permission. See `channel_may_use_secure_state_dir` for what this arm
    // does and does not guarantee — routine access, not the marker-guarded
    // legacy-DB rescue in `app/src/persistence/sqlite.rs`, which reads the
    // container deliberately and relies on this arm staying false.
    assert!(!channel_may_use_secure_state_dir(Channel::Oss));
    // Integration tests get a temporary home directory and must stay in it.
    assert!(!channel_may_use_secure_state_dir(Channel::Integration));

    // The first-party channels are the whole reason the directory exists; if
    // these flipped to false the App Group would go unused rather than
    // over-used, which is the failure this half catches.
    assert!(channel_may_use_secure_state_dir(Channel::Stable));
    assert!(channel_may_use_secure_state_dir(Channel::Preview));
    assert!(channel_may_use_secure_state_dir(Channel::Dev));
    assert!(channel_may_use_secure_state_dir(Channel::Local));
}

/// The end-to-end assertion. **This test currently runs on no machine in CI,
/// and even where it does run it is usually vacuous.** Both facts are stated
/// here because the version this replaces claimed the opposite.
///
/// What it is: macOS is the only platform on which `secure_state_dir` has a
/// non-`None` return path, so it is the only platform on which the assertion
/// can distinguish anything at all — hence the `cfg`. That much is true, and it
/// is why the unconditional form was worth removing.
///
/// What it is *not* is coverage:
///
/// * **No CI job executes it.** The only macOS job is `check-macos`
///   (`.github/workflows/pr-check.yml:675`), whose test steps are
///   `cargo nextest run -p warp --features gui ... -E 'test(/login_item/)'`
///   (`:718`, `:741`). This test is in `warp_core`, and its name does not match
///   that filter. Its `cargo check` steps are `-p warp` too (`:697`), so it is
///   not even compiled on macOS in CI.
/// * **On a developer Mac it is usually vacuous anyway.**
///   `app_group_container_path` (`paths.rs`) returns `None` unless the group
///   container directory already exists *and* `tempfile::tempfile_in` succeeds
///   inside it, which in practice requires official Warp to have created it. If
///   it returns `None`, `secure_state_dir` returns `None` for every channel and
///   the assertion holds with the `Oss` arm deleted — the same failure mode the
///   Linux version had.
///
/// The gate itself is covered, platform-independently, by
/// `test_secure_state_dir_channel_gate` above; that is where the real
/// protection lives. Making *this* one bite would take either a
/// channel-override hook on `ChannelState` (there is none — `channel()` reads a
/// process-global `Mutex` with no test setter, `channel/state.rs:204-206`) or a
/// macOS CI job that runs the `warp_core` suite. Until one of those exists,
/// treat this as documentation of the intended end-to-end behaviour that
/// happens to be executable, not as a guard.
#[cfg(target_os = "macos")]
#[test]
fn test_oss_secure_state_dir_is_disabled() {
    // Pin the precondition the assertion below depends on, so a future default
    // channel change fails here rather than turning the test silently green.
    assert_eq!(ChannelState::channel(), Channel::Oss);
    assert_eq!(secure_state_dir(), None);
}

#[test]
fn test_project_path_for_zap_dev_app_id() {
    // Covers the `starts_with("Zap")` branch in `project_dirs_for_app_id` on Linux,
    // which maps suffixed application names like `ZapDev` to a dashed lowercase
    // directory matching the Linux package name (e.g. `zap-dev`).
    let project_dirs = project_dirs_for_app_id(AppId::new("dev", "zap", "ZapDev"), None)
        .expect("should be able to compute project dirs");
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(project_dirs.project_path(), "dev.zap.ZapDev");
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            assert_eq!(project_dirs.project_path(), "zap-dev");
        } else if #[cfg(windows)] {
            assert_eq!(project_dirs.project_path(), "zap\\ZapDev");
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_project_path_for_legacy_zap_app_id() {
    // NOT the OSS identity any more — that is `dev.phosphor.Phosphor`, covered
    // by `test_project_path_for_phosphor_app_id` below. This now covers the
    // retained `"Zap" => "zap"` arm in `project_dirs_for_app_id`, which exists
    // only so the pre-rename application name still maps to its historical
    // directory if anything asks. The expectations are unchanged; only the
    // name, which claimed to describe the live OSS identity, is corrected.
    let project_dirs = project_dirs_for_app_id(AppId::new("dev", "zap", "Zap"), None)
        .expect("should be able to compute project dirs");
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(project_dirs.project_path(), "dev.zap.Zap");
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            assert_eq!(project_dirs.project_path(), "zap");
        } else if #[cfg(windows)] {
            assert_eq!(project_dirs.project_path(), "zap\\Zap");
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_project_path_for_phosphor_app_id() {
    // The post-rename identity, and the one both OSS binaries now configure
    // (`app/src/bin/phosphor_oss.rs`, `crates/warp_tui/src/bin/oss.rs`) as well
    // as `ChannelState::init`'s default. Pinned on all three platforms because
    // there is no migration: these directories are where user data starts
    // accumulating from first launch, and a wrong answer here is permanent.
    let project_dirs = project_dirs_for_app_id(AppId::new("dev", "phosphor", "Phosphor"), None)
        .expect("should be able to compute project dirs");
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(project_dirs.project_path(), "dev.phosphor.Phosphor");
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            // Lowercased to match the Linux package name. Note that
            // `directories` lowercases the application name itself, so this
            // holds with or without the explicit arm in
            // `project_dirs_for_app_id`.
            assert_eq!(project_dirs.project_path(), "phosphor");
        } else if #[cfg(windows)] {
            assert_eq!(project_dirs.project_path(), "phosphor\\Phosphor");
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}

#[test]
fn test_project_path_for_suffixed_phosphor_app_id() {
    // Covers the `starts_with("Phosphor")` branch: a suffixed application name
    // must become a dashed lowercase directory (`phosphor-dev`), not the
    // run-together `phosphordev` that plain lowercasing would produce.
    let project_dirs = project_dirs_for_app_id(AppId::new("dev", "phosphor", "PhosphorDev"), None)
        .expect("should be able to compute project dirs");
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            assert_eq!(project_dirs.project_path(), "dev.phosphor.PhosphorDev");
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            assert_eq!(project_dirs.project_path(), "phosphor-dev");
        } else if #[cfg(windows)] {
            assert_eq!(project_dirs.project_path(), "phosphor\\PhosphorDev");
        } else {
            unimplemented!("Need to update tests for current platform!");
        }
    }
}
