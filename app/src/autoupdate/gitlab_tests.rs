use std::io::Write as _;

use super::*;
use crate::autoupdate::{is_update_blocked, verify_asset_sha256_against};

/// `GET projects/88085499/releases/v0.1.15`, the first release published from
/// GitLab, trimmed: the description, author, commit, evidences and most
/// `_links` are dropped, and three of the four source archives. Every
/// `assets.links[]` entry is kept exactly as GitLab returned it.
const V0_1_15_JSON: &str = r#"{
  "name": "Phosphor v0.1.15",
  "tag_name": "v0.1.15",
  "description": "Phosphor release (fork of Zap Terminal, oss channel)",
  "created_at": "2026-10-27T19:45:18.751Z",
  "released_at": "2026-10-27T19:45:18.751Z",
  "upcoming_release": false,
  "commit_path": "/winters-phosphor/phosphor/-/commit/b0415b82df119a887b33b18192ab5ef974bf4a3d",
  "tag_path": "/winters-phosphor/phosphor/-/tags/v0.1.15",
  "assets": {
    "count": 12,
    "sources": [
      {"format": "zip", "url": "https://gitlab.com/winters-phosphor/phosphor/-/archive/v0.1.15/phosphor-v0.1.15.zip"}
    ],
    "links": [
      {"id": 13806362, "name": "phosphor_0.1.15_amd64.deb", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/phosphor_0.1.15_amd64.deb", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/phosphor_0.1.15_amd64.deb", "link_type": "package"},
      {"id": 13806361, "name": "phosphor-tui-macos-aarch64.tar.gz", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/phosphor-tui-macos-aarch64.tar.gz", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/phosphor-tui-macos-aarch64.tar.gz", "link_type": "package"},
      {"id": 13806360, "name": "phosphor-tui-linux-x86_64.tar.gz", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/phosphor-tui-linux-x86_64.tar.gz", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/phosphor-tui-linux-x86_64.tar.gz", "link_type": "package"},
      {"id": 13806359, "name": "phosphor-cli-macos-aarch64.tar.gz", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/phosphor-cli-macos-aarch64.tar.gz", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/phosphor-cli-macos-aarch64.tar.gz", "link_type": "package"},
      {"id": 13806358, "name": "phosphor-cli-linux-x86_64.tar.gz", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/phosphor-cli-linux-x86_64.tar.gz", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/phosphor-cli-linux-x86_64.tar.gz", "link_type": "package"},
      {"id": 13806357, "name": "Phosphor-x86_64.AppImage", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/Phosphor-x86_64.AppImage", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/Phosphor-x86_64.AppImage", "link_type": "package"},
      {"id": 13806356, "name": "Phosphor-windows-x86_64-Setup.exe", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/Phosphor-windows-x86_64-Setup.exe", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/Phosphor-windows-x86_64-Setup.exe", "link_type": "package"},
      {"id": 13806355, "name": "Phosphor-arm64.dmg", "url": "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.15/Phosphor-arm64.dmg", "direct_asset_url": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/Phosphor-arm64.dmg", "link_type": "package"}
    ]
  },
  "_links": {
    "self": "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15"
  }
}"#;

/// The asset name each platform's updater asks for, as `linux.rs`,
/// `windows.rs` and `mac.rs` spell it for `Channel::Oss`.
const LINUX_APPIMAGE: &str = "Phosphor-x86_64.AppImage";
const WINDOWS_INSTALLER: &str = "Phosphor-windows-x86_64-Setup.exe";
const MAC_ARM64_DMG: &str = "Phosphor-arm64.dmg";

/// SHA-256 of `b"hello\n"`, the content [`file_with`] writes in these tests.
const HELLO_SHA256: &str = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";

fn v0_1_15() -> GitlabRelease {
    serde_json::from_str(V0_1_15_JSON).expect("the real v0.1.15 response must parse")
}

/// A release shaped like v0.1.15 with a different tag, name and upcoming flag.
fn release(tag: &str, name: &str, upcoming: bool) -> GitlabRelease {
    let mut release = v0_1_15();
    release.tag_name = tag.to_owned();
    release.name = Some(name.to_owned());
    release.upcoming_release = Some(upcoming);
    release
}

/// v0.1.15 as it would look once the pipeline also publishes `SHA256SUMS`,
/// with `digests` as that file's parsed content.
fn v0_1_15_with_checksums(digests: &[(&str, &str)]) -> GitlabRelease {
    let mut release = v0_1_15();
    release.assets.links.push(GitlabAssetLink {
        name: CHECKSUMS_ASSET.to_owned(),
        direct_asset_url: Some(release_download_url("v0.1.15", CHECKSUMS_ASSET)),
    });
    release.checksums = Checksums::Present(
        digests
            .iter()
            .map(|(name, hex)| ((*name).to_owned(), (*hex).to_owned()))
            .collect(),
    );
    release
}

fn file_with(contents: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("create temp file");
    file.write_all(contents).expect("write temp file");
    file.flush().expect("flush temp file");
    file
}

#[test]
fn parses_the_real_v0_1_15_release() {
    let release = v0_1_15();
    assert_eq!(release.tag_name, "v0.1.15");
    assert_eq!(release.version(), "0.1.15");
    assert_eq!(release.name.as_deref(), Some("Phosphor v0.1.15"));
    assert_eq!(release.upcoming_release, Some(false));
    assert!(!release.is_prerelease());
    assert!(release.has_safe_tag());
    assert_eq!(release.assets.links.len(), 8);
    // Never part of the API response.
    assert_eq!(release.checksums, Checksums::NotFetched);
}

#[test]
fn every_platform_asset_the_updater_asks_for_is_published() {
    let release = v0_1_15();
    for asset in [LINUX_APPIMAGE, WINDOWS_INSTALLER, MAC_ARM64_DMG] {
        assert!(
            release.has_asset(asset),
            "v0.1.15 publishes no {asset}; the updater's asset name has drifted from the pipeline's"
        );
    }
    // The name the Windows updater used before #1056 (`<prefix>Setup.exe`) is
    // not what the pipeline publishes.
    assert!(!release.has_asset("PhosphorSetup.exe"));
}

#[test]
fn download_url_matches_gitlabs_own_permalink_for_every_asset() {
    let release = v0_1_15();
    for link in &release.assets.links {
        assert_eq!(
            Some(release.download_url(&link.name)).as_deref(),
            link.direct_asset_url.as_deref(),
            "built URL for {} differs from GitLab's direct_asset_url",
            link.name
        );
    }
}

#[test]
fn url_construction() {
    assert_eq!(
        api_project_url(),
        "https://gitlab.com/api/v4/projects/winters-phosphor%2Fphosphor"
    );
    assert_eq!(
        releases_api_url(),
        "https://gitlab.com/api/v4/projects/winters-phosphor%2Fphosphor/releases?order_by=released_at&sort=desc&per_page=20"
    );
    assert_eq!(
        release_download_url("v0.1.15", MAC_ARM64_DMG),
        "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/Phosphor-arm64.dmg"
    );
    assert_eq!(
        release_page_url("v0.1.15"),
        "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15"
    );
    for url in [
        api_project_url(),
        releases_api_url(),
        release_download_url("v0.1.15", LINUX_APPIMAGE),
        release_page_url("v0.1.15"),
    ] {
        assert!(!url.contains("github"), "{url} still names GitHub");
    }
}

#[test]
fn asset_url_uses_the_cached_release_tag_or_rebuilds_it() {
    let cached = v0_1_15();
    let expected = "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/Phosphor-x86_64.AppImage";
    // The version handed down is the `v`-stripped one; either spelling works.
    assert_eq!(
        asset_download_url_for(Some(&cached), "0.1.15", LINUX_APPIMAGE),
        expected
    );
    assert_eq!(
        asset_download_url_for(Some(&cached), "v0.1.15", LINUX_APPIMAGE),
        expected
    );
    // Nothing cached, or a cache for some other release: the tag is rebuilt.
    assert_eq!(
        asset_download_url_for(None, "0.1.15", LINUX_APPIMAGE),
        expected
    );
    assert_eq!(
        asset_download_url_for(Some(&cached), "0.1.16", WINDOWS_INSTALLER),
        "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.16/downloads/Phosphor-windows-x86_64-Setup.exe"
    );
}

#[test]
fn prerelease_and_upcoming_releases_are_skipped() {
    assert!(release("v0.1.17", "Phosphor v0.1.17", true).is_prerelease());
    assert!(release("v0.1.16", "Phosphor Beta v0.1.16 (pre-release)", false).is_prerelease());
    assert!(release("v0.1.16-beta.1", "Phosphor Beta v0.1.16-beta.1", false).is_prerelease());
    assert!(release("v0.1.16-rc1", "Phosphor v0.1.16-rc1", false).is_prerelease());
    // The migrated legacy betas carry neither flag, only the tag suffix.
    assert!(
        release(
            "v2026.08.14.1-beta",
            "Phosphor Beta v2026.08.14.1-beta",
            false
        )
        .is_prerelease()
    );
    assert!(!release("v0.1.16", "Phosphor v0.1.16", false).is_prerelease());

    // Newest first, as the API is asked to return them.
    let releases = vec![
        release("v0.1.17", "Phosphor v0.1.17", true),
        release(
            "v0.1.16-beta.1",
            "Phosphor Beta v0.1.16-beta.1 (pre-release)",
            false,
        ),
        v0_1_15(),
        release("v0.1.14", "Phosphor v0.1.14", false),
        release(
            "v2026.08.14.1-beta",
            "Phosphor Beta v2026.08.14.1-beta",
            false,
        ),
    ];
    let picked = select_latest_stable(&releases).expect("v0.1.15 is stable");
    assert_eq!(picked.tag_name, "v0.1.15");

    let only_prereleases = vec![
        release("v0.1.17", "Phosphor v0.1.17", true),
        release(
            "v2026.08.14.1-beta",
            "Phosphor Beta v2026.08.14.1-beta",
            false,
        ),
    ];
    assert!(select_latest_stable(&only_prereleases).is_none());
}

#[test]
fn a_tag_that_is_unsafe_in_a_url_is_never_selected() {
    let releases = vec![
        release("v0.1.16/../../evil", "Phosphor v0.1.16", false),
        release("v0.1.16?x=1", "Phosphor v0.1.16", false),
        v0_1_15(),
    ];
    let picked = select_latest_stable(&releases).expect("v0.1.15 is safe");
    assert_eq!(picked.tag_name, "v0.1.15");
}

#[test]
fn sha256sums_parsing() {
    let upper = HELLO_SHA256.to_ascii_uppercase();
    let text =
        format!("{HELLO_SHA256}  Phosphor-arm64.dmg\n{upper} *Phosphor-x86_64.AppImage\r\n\n");
    let digests = parse_sha256sums(&text).expect("valid sha256sum output");
    assert_eq!(digests.len(), 2);
    assert_eq!(digests[MAC_ARM64_DMG], HELLO_SHA256);
    // Binary-mode `*`, CRLF and upper-case hex are all normalised.
    assert_eq!(digests[LINUX_APPIMAGE], HELLO_SHA256);

    // The same file listed twice with the same digest is harmless.
    let repeated = format!("{HELLO_SHA256}  a\n{HELLO_SHA256}  a\n");
    assert_eq!(parse_sha256sums(&repeated).unwrap().len(), 1);

    let other = "0".repeat(64);
    for bad in [
        String::new(),
        "\n\n".to_owned(),
        HELLO_SHA256.to_owned(),
        format!("{}  a", &HELLO_SHA256[..63]),
        format!("{}g  a", &HELLO_SHA256[..63]),
        format!("md5:{HELLO_SHA256}  a"),
        format!("{HELLO_SHA256}  dir/a"),
        format!("{HELLO_SHA256}  a\n{other}  a\n"),
    ] {
        assert!(
            parse_sha256sums(&bad).is_err(),
            "{bad:?} must not parse as a digest list"
        );
    }

    assert!(matches!(
        checksums_from_body(&format!("{HELLO_SHA256}  a\n")),
        Checksums::Present(_)
    ));
    assert!(matches!(
        checksums_from_body("junk"),
        Checksums::Unusable(_)
    ));
    let huge = format!("{HELLO_SHA256}  a\n").repeat(1000);
    assert!(matches!(checksums_from_body(&huge), Checksums::Unusable(_)));
}

#[test]
fn digest_found_and_matching_is_accepted() {
    let file = file_with(b"hello\n");
    let release = v0_1_15_with_checksums(&[(MAC_ARM64_DMG, HELLO_SHA256)]);
    let verified = verify_asset_sha256_against(file.path(), Some(&release), MAC_ARM64_DMG)
        .expect("matching bytes must verify");
    assert_eq!(verified, HELLO_SHA256);
}

#[test]
fn digest_mismatch_is_refused() {
    let file = file_with(b"hello\n");
    let release = v0_1_15_with_checksums(&[(MAC_ARM64_DMG, "0".repeat(64).as_str())]);
    let err = verify_asset_sha256_against(file.path(), Some(&release), MAC_ARM64_DMG)
        .expect_err("a mismatch must be refused");
    assert!(is_update_blocked(&err), "{err:#}");
}

#[test]
fn missing_digest_is_refused_in_every_form() {
    let file = file_with(b"hello\n");

    // No release metadata at all.
    let err = verify_asset_sha256_against(file.path(), None, MAC_ARM64_DMG).unwrap_err();
    assert!(is_update_blocked(&err), "{err:#}");

    // The real v0.1.15: no SHA256SUMS asset, checksums never fetched.
    let err =
        verify_asset_sha256_against(file.path(), Some(&v0_1_15()), MAC_ARM64_DMG).unwrap_err();
    assert!(is_update_blocked(&err), "{err:#}");

    // As cached after a fetch that found no SHA256SUMS link.
    let mut missing = v0_1_15();
    missing.checksums = Checksums::Missing;
    let err = verify_asset_sha256_against(file.path(), Some(&missing), MAC_ARM64_DMG).unwrap_err();
    assert!(is_update_blocked(&err), "{err:#}");

    // SHA256SUMS present but unusable.
    let mut unusable = v0_1_15();
    unusable.checksums = Checksums::Unusable("line 1 has no file name".to_owned());
    let err = verify_asset_sha256_against(file.path(), Some(&unusable), MAC_ARM64_DMG).unwrap_err();
    assert!(is_update_blocked(&err), "{err:#}");

    // SHA256SUMS present, but with no line for this asset.
    let other_only = v0_1_15_with_checksums(&[(LINUX_APPIMAGE, HELLO_SHA256)]);
    let err =
        verify_asset_sha256_against(file.path(), Some(&other_only), MAC_ARM64_DMG).unwrap_err();
    assert!(is_update_blocked(&err), "{err:#}");

    // A digest for a file the release does not list as an asset.
    let unlisted = v0_1_15_with_checksums(&[("Phosphor-intel.dmg", HELLO_SHA256)]);
    let err = verify_asset_sha256_against(file.path(), Some(&unlisted), "Phosphor-intel.dmg")
        .unwrap_err();
    assert!(is_update_blocked(&err), "{err:#}");
}
