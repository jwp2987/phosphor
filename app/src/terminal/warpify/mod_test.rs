//! Guards the bundled auto-bootstrap snippet text against Warp branding
//! leaking back in. These `.txt` files are spliced verbatim into the "Run the
//! following to automatically Phosphorize in the future:" snippet
//! (`success_block.rs`) and, for the shell variants, written into the user's
//! own rc file (`~/.config/fish/config.fish`, `~/.bashrc`, `~/.zshrc`) as a
//! `#`-comment the user will read the next time they open it. `# Auto-Warpify`
//! shipped there even though `check_brand_strings` treats `Warp` inside an
//! identifier like `SourcedRcFileForWarp` as lineage, not branding: the guard
//! only scans Rust string literals and Fluent values, not bundled assets, so
//! a plain-text asset file was a blind spot. These tests close that
//! particular gap without teaching the shell script to parse Rust.
//!
//! `SourcedRcFileForWarp` itself is intentionally NOT asserted away here: it
//! is the wire value of the DCS hook this fork's own ANSI parser matches on
//! (`DProtoHook::SourcedRcFileForWarp`, `terminal/model/ansi/dcs_hooks.rs`),
//! so renaming the string in the asset without renaming the enum variant (and
//! every terminal on the other end of the pipe) would just break detection.

const FISH_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/fish_subshell_bootstrap_block_output.txt"
));
const BASH_ZSH_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/bash_zsh_subshell_bootstrap_block_output.txt"
));
const LEGACY_REMOTE_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/legacy_remote_subshell_bootstrap_block_output.txt"
));

/// The one identifier these snippets are allowed to carry: the DCS hook name
/// itself, which is a wire value, not prose.
const ALLOWED_WARP_IDENTIFIER: &str = "SourcedRcFileForWarp";

fn assert_no_stray_warp_branding(snippet: &str, label: &str) {
    let scrubbed = snippet.replace(ALLOWED_WARP_IDENTIFIER, "");
    assert!(
        !scrubbed.contains("Warp") && !scrubbed.contains("warp"),
        "{label} still names Warp outside of the {ALLOWED_WARP_IDENTIFIER} wire value: {snippet:?}"
    );
}

#[test]
fn fish_bootstrap_snippet_comment_says_phosphorize() {
    assert!(
        FISH_SNIPPET.contains("# Auto-Phosphorize"),
        "expected the fish rc-file comment to say Phosphorize: {FISH_SNIPPET:?}"
    );
    assert_no_stray_warp_branding(FISH_SNIPPET, "fish bootstrap snippet");
}

#[test]
fn bash_zsh_bootstrap_snippet_comment_says_phosphorize() {
    assert!(
        BASH_ZSH_SNIPPET.contains("# Auto-Phosphorize"),
        "expected the bash/zsh rc-file comment to say Phosphorize: {BASH_ZSH_SNIPPET:?}"
    );
    assert_no_stray_warp_branding(BASH_ZSH_SNIPPET, "bash/zsh bootstrap snippet");
}

#[test]
fn legacy_remote_snippet_names_phosphor() {
    assert!(
        LEGACY_REMOTE_SNIPPET.contains("Phosphor runs commands"),
        "expected the legacy remote-subshell snippet to name Phosphor: {LEGACY_REMOTE_SNIPPET:?}"
    );
    assert_no_stray_warp_branding(LEGACY_REMOTE_SNIPPET, "legacy remote-subshell snippet");
}

/// Both hook-carrying snippets emit the same DCS hook the ANSI parser expects
/// (`dcs_hooks.rs`'s `"SourcedRcFileForWarp" => Some(DProtoHook::SourcedRcFileForWarp { .. })`).
/// If this ever drifts, auto-bootstrap silently stops being recognized on the
/// other end.
#[test]
fn shell_snippets_still_emit_the_expected_wire_hook() {
    for (label, snippet) in [("fish", FISH_SNIPPET), ("bash/zsh", BASH_ZSH_SNIPPET)] {
        assert!(
            snippet.contains(r#""hook": "SourcedRcFileForWarp""#),
            "{label} snippet no longer emits the SourcedRcFileForWarp hook: {snippet:?}"
        );
    }
}
