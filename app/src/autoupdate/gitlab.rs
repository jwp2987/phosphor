// The open-source build's (Channel::Oss) release feed: this project's GitLab
// Releases, rather than official Zap's channel_versions / GCS. This module is
// only responsible for "fetching the latest release metadata and its published
// SHA-256 digests" + "naming the URL of an asset"; the actual downloading-to-disk
// + opening the directory is done by linux.rs / windows.rs / mac.rs, and the
// digest comparison by `super::verify_oss_asset_sha256`.
//
// Dormant: none of this runs in a shipped build, because the `autoupdate` cargo
// feature is off everywhere (#630, `script/check_no_autoupdate`). It is kept
// correct for GitLab (#1056) so that turning the feature on is a decision, not
// a port.
//
// Integrity, and why it needs a release asset: a GitLab release link carries
// no digest (GitHub's release-asset API had a `digest` field; GitLab's
// `assets.links[]` has only a name and URLs). The release pipeline therefore
// publishes a `SHA256SUMS` asset alongside the installers (the `release-publish`
// job in `.gitlab-ci.yml` builds it from the per-platform manifests the build
// jobs computed over the exact bytes they uploaded), and the updater fetches it
// at check time. Every way of not having a usable digest -- no `SHA256SUMS`
// link, an unparseable file, no line for the asset -- is a refusal, never a
// pass. See `super::verify_oss_asset_sha256` for what this does and does not
// protect against.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use lazy_static::lazy_static;
use serde::Deserialize;

// Shared with every user-visible link that names the project (the issue
// tracker, the feedback form, the manual), so the update feed and those links
// cannot drift apart. `remote_server::setup` keeps its own private copy of the
// same `owner/repo` (`RELEASE_REPO`) because that crate cannot depend on `app`.
use crate::util::links::{REPO_NAME, REPO_OWNER, repo_url};

/// Not required by GitLab, but it makes the updater's requests identifiable in
/// the project's logs.
const USER_AGENT: &str = "Phosphor-Autoupdate";

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// How many of the newest releases one check looks through for a stable one.
/// Pre-releases are skipped, so this only has to be larger than the longest
/// run of consecutive pre-releases at the head of the list.
const RELEASES_PAGE_SIZE: usize = 20;

/// The release asset holding the `sha256sum`-format digests of every other
/// asset. Its name is part of the contract with `.gitlab-ci.yml`'s
/// `release-publish` job.
pub const CHECKSUMS_ASSET: &str = "SHA256SUMS";

/// A real `SHA256SUMS` is a dozen lines. Anything this large is not one.
const MAX_CHECKSUMS_BYTES: usize = 64 * 1024;

/// The unauthenticated v4 API root for this project. The path is URL-encoded
/// (`owner%2Frepo`) rather than the numeric id so it derives from the same
/// constants as every other project link.
pub fn api_project_url() -> String {
    format!("https://gitlab.com/api/v4/projects/{REPO_OWNER}%2F{REPO_NAME}")
}

/// The newest releases first. `released_at` is what GitLab's own
/// `/-/releases/permalink/latest` orders by; the list (rather than that
/// permalink) is used so pre-releases can be skipped -- GitLab has no
/// prerelease flag, so "latest" can be a beta.
pub fn releases_api_url() -> String {
    format!(
        "{}/releases?order_by=released_at&sort=desc&per_page={RELEASES_PAGE_SIZE}",
        api_project_url()
    )
}

/// `https://gitlab.com/<owner>/<repo>/-/releases/<tag>/downloads/<asset>`: the
/// release's permanent asset URL (the `direct_asset_path` the pipeline gives
/// every link), which redirects to the package registry. The same shape
/// `remote_server::setup::download_url` uses for the CLI tarballs.
pub fn release_download_url(tag: &str, asset: &str) -> String {
    format!("{}/-/releases/{tag}/downloads/{asset}", repo_url())
}

/// The human-readable release page, for the "update manually" fallback.
pub fn release_page_url(tag: &str) -> String {
    format!("{}/-/releases/{tag}", repo_url())
}

/// A release as `GET /projects/:id/releases` returns it. Only the fields the
/// updater reads are modelled; everything else in the response is ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct GitlabRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    /// `true` when `released_at` is in the future.
    #[serde(default)]
    pub upcoming_release: Option<bool>,
    #[serde(default)]
    pub assets: GitlabAssets,
    /// Filled in by [`fetch_latest_release`] from the release's
    /// [`CHECKSUMS_ASSET`]; never part of the API response.
    #[serde(skip)]
    pub checksums: Checksums,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GitlabAssets {
    #[serde(default)]
    pub links: Vec<GitlabAssetLink>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitlabAssetLink {
    pub name: String,
    /// The `/-/releases/<tag>/downloads/<name>` permalink, when the link has a
    /// `direct_asset_path`. Only compared against the URL the updater builds
    /// itself (see [`asset_download_url_for`]); the link's `url` (the package
    /// registry it redirects to) is not modelled at all.
    #[serde(default)]
    pub direct_asset_url: Option<String>,
}

/// What is known about a release's published digests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Checksums {
    /// Not looked for (a release fresh from the API, before
    /// [`fetch_latest_release`] has fetched its [`CHECKSUMS_ASSET`]).
    #[default]
    NotFetched,
    /// The release has no [`CHECKSUMS_ASSET`] link. True of every release
    /// published before #1056 (v0.1.15 and older).
    Missing,
    /// The asset exists but could not be used; the string says why.
    Unusable(String),
    /// Asset name -> lowercase-hex SHA-256.
    Present(BTreeMap<String, String>),
}

impl GitlabRelease {
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }

    /// Whether a stable-channel client should skip this release.
    ///
    /// GitLab releases carry no prerelease flag (GitHub's `/releases/latest`
    /// skipped flagged prereleases for us), so the signals the pipeline does
    /// leave are read instead, and any one of them is enough:
    /// * `upcoming_release` -- scheduled for the future, not out yet;
    /// * a name ending in `(pre-release)` -- what `release-publish` appends for
    ///   a channel whose `is_prerelease` is true;
    /// * a semver pre-release suffix on the tag (`v0.1.16-beta.1`,
    ///   `v0.1.16-rc1`, the migrated `v2026.08.14.1-beta`). `release-meta`
    ///   accepts any `v<cargo version>-<suffix>` tag but only names `beta`
    ///   ones as pre-releases; a suffixed tag orders *below* its release under
    ///   semver anyway, so offering it as the latest stable would be wrong.
    pub fn is_prerelease(&self) -> bool {
        self.upcoming_release.unwrap_or(false)
            || self
                .name
                .as_deref()
                .is_some_and(|name| name.trim_end().ends_with("(pre-release)"))
            || self.version().contains('-')
    }

    /// The tag is spliced into URL paths, so only the characters a package
    /// version may contain (`script/ci/upload_package` enforces the same set)
    /// are accepted. Anything else is not a release this pipeline made.
    pub fn has_safe_tag(&self) -> bool {
        !self.tag_name.is_empty()
            && self
                .tag_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
    }

    pub fn has_asset(&self, name: &str) -> bool {
        self.find_asset(name).is_some()
    }

    pub fn find_asset(&self, name: &str) -> Option<&GitlabAssetLink> {
        self.assets.links.iter().find(|link| link.name == name)
    }

    /// The permanent download URL for one of this release's assets.
    pub fn download_url(&self, asset: &str) -> String {
        release_download_url(&self.tag_name, asset)
    }

    /// The lowercase-hex SHA-256 this release publishes for `asset`, or why
    /// there is none. Every `Err` is a reason to refuse the file.
    pub fn expected_sha256(&self, asset: &str) -> std::result::Result<String, String> {
        let tag = &self.tag_name;
        if !self.has_asset(asset) {
            return Err(format!(
                "release {tag} lists no asset by that name, so its expected SHA-256 is unknown"
            ));
        }
        match &self.checksums {
            Checksums::NotFetched => Err(format!(
                "the {CHECKSUMS_ASSET} of release {tag} was never fetched"
            )),
            Checksums::Missing => Err(format!(
                "release {tag} publishes no {CHECKSUMS_ASSET} asset, so no SHA-256 is known for any of its files. Autoupdate will keep failing for this release; download it manually in the meantime."
            )),
            Checksums::Unusable(reason) => Err(format!(
                "the {CHECKSUMS_ASSET} of release {tag} is unusable ({reason})"
            )),
            Checksums::Present(digests) => digests.get(asset).cloned().ok_or_else(|| {
                format!("the {CHECKSUMS_ASSET} of release {tag} has no line for {asset}")
            }),
        }
    }
}

/// The newest release a stable-channel client should be offered, from a list
/// in `released_at`-descending order (as [`releases_api_url`] requests it).
pub fn select_latest_stable(releases: &[GitlabRelease]) -> Option<&GitlabRelease> {
    releases
        .iter()
        .find(|release| release.has_safe_tag() && !release.is_prerelease())
}

/// Parses `sha256sum` output: `<64 hex>  <name>` per line, with `*<name>` (binary
/// mode) accepted. Strict on purpose: a malformed line, a non-SHA-256 digest, or
/// one name listed with two different digests makes the whole file unusable
/// rather than silently dropping the line.
pub fn parse_sha256sums(text: &str) -> std::result::Result<BTreeMap<String, String>, String> {
    let mut digests = BTreeMap::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            continue;
        }
        let line_number = index + 1;
        let (hex, rest) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| format!("line {line_number} has no file name"))?;
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("line {line_number} does not start with a SHA-256"));
        }
        let rest = rest.trim_start();
        let name = rest.strip_prefix('*').unwrap_or(rest);
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(format!("line {line_number} has no usable file name"));
        }
        let hex = hex.to_ascii_lowercase();
        if let Some(previous) = digests.insert(name.to_owned(), hex.clone()) {
            if previous != hex {
                return Err(format!("{name} is listed with two different digests"));
            }
        }
    }
    if digests.is_empty() {
        return Err("it lists no files".to_owned());
    }
    Ok(digests)
}

/// What a fetched [`CHECKSUMS_ASSET`] body amounts to.
pub fn checksums_from_body(body: &str) -> Checksums {
    if body.len() > MAX_CHECKSUMS_BYTES {
        return Checksums::Unusable(format!(
            "{} bytes, more than the {MAX_CHECKSUMS_BYTES} a digest list can plausibly need",
            body.len()
        ));
    }
    match parse_sha256sums(body) {
        Ok(digests) => Checksums::Present(digests),
        Err(reason) => Checksums::Unusable(reason),
    }
}

/// The download URL for `asset` of the release whose version is `version`
/// (with or without the `v`).
///
/// Uses the cached release's own tag when it is that version, so the bytes come
/// from the same release whose `SHA256SUMS` they will be checked against;
/// otherwise rebuilds the tag as `v<version>`, which is how every tag this
/// project publishes is spelled. A URL for an asset the release does not list
/// is still returned (and logged), because the refusal belongs to the digest
/// check, which will make it.
///
/// The URL is always built from this project's own constants and the tag, never
/// taken from the API response, so a response cannot point a download at
/// another host. GitLab's `direct_asset_url` should be the same string; a
/// difference is logged, because it means the URL shape (or the project path)
/// has moved under us.
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub fn asset_download_url_for(
    cached: Option<&GitlabRelease>,
    version: &str,
    asset: &str,
) -> String {
    let version = version.trim_start_matches('v');
    match cached {
        Some(release) if release.version() == version => {
            let url = release.download_url(asset);
            match release.find_asset(asset) {
                None => log::warn!(
                    "Phosphor: release {} has no asset named {asset}; the download will be refused",
                    release.tag_name
                ),
                Some(link) => {
                    if let Some(direct) = link.direct_asset_url.as_deref() {
                        if direct != url {
                            log::warn!(
                                "Phosphor: GitLab lists {asset} at {direct}, not at the expected {url}; downloading from {url}"
                            );
                        }
                    }
                }
            }
            url
        }
        _ => release_download_url(&format!("v{version}"), asset),
    }
}

/// [`asset_download_url_for`] against the cached release.
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub fn oss_asset_download_url(version: &str, asset: &str) -> String {
    asset_download_url_for(cached_release().as_ref(), version, asset)
}

lazy_static! {
    /// The most recently fetched release, with its digests. Written by
    /// fetch_version, read by download_update. This means the download stage
    /// doesn't need to re-request the API, and avoids a race (the release
    /// changing between the two requests).
    static ref LATEST_RELEASE: Mutex<Option<GitlabRelease>> = Mutex::new(None);
}

/// The cached release, or `None` if nothing has been fetched yet.
///
/// A poisoned mutex is recovered rather than discarded. `Option<GitlabRelease>`
/// has no invariant a panicking writer could have broken -- it is either
/// replaced wholesale or not touched -- and `.ok()` here would fuse "the lock
/// was poisoned once" with "we have no release metadata". That fusion is not
/// recoverable: poisoning is permanent for the life of the process, so a single
/// panic anywhere while this lock was held would make this return `None`
/// forever. Every caller of `verify_oss_asset_sha256` reads the expected digest
/// through here, and a `None` there is an `UpdateBlocked` refusal, so the whole
/// update channel would stay dark until the user restarted the app -- a
/// one-shot failure recorded permanently.
pub fn cached_release() -> Option<GitlabRelease> {
    LATEST_RELEASE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Same reasoning on the write side: dropping the store on a poisoned lock
/// would leave `cached_release()` empty for the rest of the process.
fn store_cached(release: GitlabRelease) {
    *LATEST_RELEASE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(release);
}

/// Fetches the newest stable release and its `SHA256SUMS`, caches both, and
/// returns the release.
///
/// A network failure fetching `SHA256SUMS` fails the whole check: it is
/// transient, and the next poll retries it, whereas carrying on would download
/// an installer only to refuse it. A release that has *no* usable `SHA256SUMS`
/// is a permanent property of that release, so it is cached as such and the
/// refusal happens -- visibly, as `UpdateBlocked` -- at verification.
pub async fn fetch_latest_release(client: &http_client::Client) -> Result<GitlabRelease> {
    let url = releases_api_url();
    log::info!("Fetching the latest releases from {url}");
    let releases: Vec<GitlabRelease> = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .context("Failed to call the GitLab Releases API")?
        .error_for_status()
        .context("GitLab Releases API returned a non-2xx status code")?
        .json()
        .await
        .context("Failed to parse GitLab Releases JSON")?;
    let mut release = select_latest_stable(&releases).cloned().ok_or_else(|| {
        anyhow!(
            "none of the {} newest GitLab releases is a stable release",
            releases.len()
        )
    })?;

    release.checksums = if release.has_asset(CHECKSUMS_ASSET) {
        let checksums_url = release.download_url(CHECKSUMS_ASSET);
        let body = client
            .get(&checksums_url)
            .header("User-Agent", USER_AGENT)
            .timeout(FETCH_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("Failed to fetch {checksums_url}"))?
            .error_for_status()
            .with_context(|| format!("{checksums_url} returned a non-2xx status code"))?
            .text()
            .await
            .with_context(|| format!("Failed to read {checksums_url}"))?;
        checksums_from_body(&body)
    } else {
        Checksums::Missing
    };
    match &release.checksums {
        Checksums::Present(digests) => log::info!(
            "GitLab latest release: tag={} assets={} digests={}",
            release.tag_name,
            release.assets.links.len(),
            digests.len()
        ),
        other => log::warn!(
            "GitLab latest release: tag={} assets={}; no usable {CHECKSUMS_ASSET} ({other:?}), so its downloads will be refused",
            release.tag_name,
            release.assets.links.len()
        ),
    }
    store_cached(release.clone());
    Ok(release)
}

#[cfg(test)]
#[path = "gitlab_tests.rs"]
mod tests;
