//! The Mermaid renderer behind every diagram the app draws (`mermaid_to_svg::render_mermaid_to_svg`,
//! the call inside the editor's `mermaid_asset_source`). Any input must give `Ok(svg)` or `Err`;
//! a panic, hang or stack overflow is a finding. An `Ok` must at least be an SVG document, and
//! rendering must be deterministic. Seeds: `fuzz/corpus/mermaid_render/` (copies of
//! `crates/editor/tests/fixtures/mermaid/*.mmd`). #1460.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(64 * 1024)];
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(svg) = mermaid_to_svg::render_mermaid_to_svg(source, None) {
        assert!(svg.contains("<svg"), "Ok output is not an SVG");
        let again = mermaid_to_svg::render_mermaid_to_svg(source, None).unwrap();
        assert_eq!(svg, again, "non-deterministic render");
    }
    let _ = mermaid_to_svg::is_mermaid_diagram(source);
    let _ = mermaid_to_svg::strip_mermaid_frontmatter(source);
});
