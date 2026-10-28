use super::*;

#[test]
fn test_oss_appimage_download_url() {
    // The open-source build's AppImage is served from the fork's GitLab
    // release, under the name the pipeline publishes it as. This replaces the
    // old `repo_name` test, which pinned the GitHub slug the feed used to read.
    assert_eq!(
        crate::autoupdate::gitlab::asset_download_url_for(None, "0.1.15", OSS_APPIMAGE_ASSET),
        "https://gitlab.com/winters-phosphor/phosphor/-/releases/v0.1.15/downloads/Phosphor-x86_64.AppImage"
    );
}
