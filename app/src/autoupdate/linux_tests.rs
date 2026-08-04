use super::*;

#[test]
fn test_repo_name() {
    // The open-source build's release assets are served from the fork's GitHub
    // repository.
    assert_eq!(repo_name(Channel::Oss), "jwp2987/phosphor");
    // The inherited channels resolve to the same single release repository (the
    // fork does not run Warp's multi-repo release infrastructure).
    assert_eq!(repo_name(Channel::Stable), "jwp2987/phosphor");
}
