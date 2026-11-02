//! Mermaid corpus (#1460): every diagram in `tests/fixtures/mermaid/` is rendered through the path
//! the app uses (`mermaid_asset_source`'s fetch closure, which is `mermaid_to_svg::render_mermaid_to_svg`
//! plus the byte conversion the asset cache sees). For each file we assert no panic, deterministic
//! output, and a normalized summary (size, element counts, text labels) that is snapshotted with
//! insta. The raw SVG is not snapshotted: it is far too noisy (coordinates change with any layout
//! tweak). `invalid_*` files must end in the app's error path (an `Err` from the fetch, which the
//! asset cache turns into `FailedToLoad`, which `render_mermaid_diagram_section` turns into the
//! plain-source fallback) or a non-blank SVG; never a panic and never a blank block.
//!
//! Update snapshots on the build box: `INSTA_UPDATE=always cargo nextest run -p warp_editor --test
//! mermaid_corpus`, then review/commit `tests/snapshots/`. See docs/testing.md.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use warp_editor::content::mermaid_diagram::mermaid_asset_source;
use warpui::assets::asset_cache::AssetSource;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mermaid")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    String::from_utf8(bytes).expect("fixture is UTF-8")
}

/// Renders exactly as the app's asset fetch does.
fn app_render(source: &str) -> Result<String, String> {
    let AssetSource::Async { fetch, .. } = mermaid_asset_source(source) else {
        panic!("mermaid diagrams are async assets");
    };
    match futures_lite::future::block_on(fetch()) {
        Ok(bytes) => Ok(String::from_utf8(bytes.to_vec()).expect("SVG is UTF-8")),
        Err(e) => Err(format!("{e:#}")),
    }
}

/// Text content of every `<text>` element, nested tags (`<tspan>`) stripped, entities decoded for
/// the five XML escapes.
fn text_labels(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        let after = &rest[start..];
        // `<text` must be the element, not a prefix of another tag name.
        let tag_end = match after.find('>') {
            Some(i) => i,
            None => break,
        };
        let name_ok = after[5..]
            .chars()
            .next()
            .is_some_and(|c| c == ' ' || c == '>' || c == '/');
        if !name_ok {
            rest = &after[5..];
            continue;
        }
        if after[..tag_end].ends_with('/') {
            rest = &after[tag_end..];
            continue;
        }
        let Some(close) = after.find("</text>") else {
            break;
        };
        let inner = &after[tag_end + 1..close];
        let mut text = String::new();
        let mut in_tag = false;
        for c in inner.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => text.push(c),
                _ => {}
            }
        }
        let text = text
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() {
            out.push(text);
        }
        rest = &after[close + 7..];
    }
    out
}

fn count(svg: &str, tag: &str) -> usize {
    let needle = format!("<{tag}");
    svg.match_indices(&needle)
        .filter(|(i, _)| {
            svg[i + needle.len()..]
                .chars()
                .next()
                .is_some_and(|c| c == ' ' || c == '>' || c == '/' || c == '\n')
        })
        .count()
}

fn attr(svg: &str, name: &str) -> Option<String> {
    let head = &svg[svg.find("<svg")?..];
    let head = &head[..head.find('>')?];
    let key = format!(" {name}=\"");
    let start = head.find(&key)? + key.len();
    let end = head[start..].find('"')?;
    Some(head[start..start + end].to_string())
}

/// Stable, human-reviewable description of an SVG: coordinates are rounded to tens so a one-pixel
/// layout nudge does not churn the snapshot, but a node or label appearing/vanishing does.
fn summary(svg: &str) -> String {
    let round = |v: Option<String>| {
        v.and_then(|v| v.trim_end_matches("px").parse::<f64>().ok())
            .map(|v| format!("~{}", (v / 10.0).round() as i64 * 10))
            .unwrap_or_else(|| "?".into())
    };
    let mut s = String::new();
    s.push_str(&format!(
        "width: {}\nheight: {}\nviewBox: {}\n",
        round(attr(svg, "width")),
        round(attr(svg, "height")),
        attr(svg, "viewBox").is_some()
    ));
    for tag in [
        "g",
        "rect",
        "path",
        "circle",
        "ellipse",
        "polygon",
        "polyline",
        "line",
        "text",
        "tspan",
        "marker",
        "foreignObject",
        "image",
    ] {
        s.push_str(&format!("{tag}: {}\n", count(svg, tag)));
    }
    s.push_str("labels:\n");
    for t in text_labels(svg) {
        s.push_str(&format!("  - {t:?}\n"));
    }
    s
}

const VALID: &[(&str, &[&str])] = &[
    (
        "flowchart_td.mmd",
        &[
            "Start Node",
            "Is it ready?",
            "Ship It",
            "Fix Things",
            "Yes",
            "No",
        ],
    ),
    (
        "flowchart_lr.mmd",
        &[
            "Client App",
            "API Gateway",
            "Auth Service",
            "Order Service",
            "Database",
        ],
    ),
    ("sequence.mmd", &["Alice", "Bob", "Hello Bob", "Hi Alice"]),
    ("class.mmd", &["Animal", "Dog", "Cat", "makeSound()"]),
    ("state.mmd", &["Idle", "Running", "Paused"]),
    ("er.mmd", &["CUSTOMER", "ORDER", "LINE_ITEM"]),
    ("gantt.mmd", &["Release plan", "Compile", "Package"]),
    ("pie.mmd", &["Dogs", "Cats", "Rats"]),
    ("mindmap.mmd", &["Testing", "Unit", "Parser", "Real app"]),
    ("journey.mmd", &["My working day", "Make tea", "Sit down"]),
    ("timeline.mmd", &["History", "LinkedIn", "Facebook"]),
    ("frontmatter.mmd", &["Themed Start", "Themed Done"]),
    ("unicode_labels.mmd", &["日本語 ラベル", "Ünïcödé", "مرحبا"]),
    ("crlf.mmd", &["CRLF A", "CRLF B"]),
];

const INVALID: &[&str] = &[
    "invalid_unknown_type.mmd",
    "invalid_garbage.mmd",
    "invalid_empty_flowchart.mmd",
    "invalid_bad_direction.mmd",
    "invalid_sequence.mmd",
    "invalid_pie.mmd",
    "invalid_gantt.mmd",
    "invalid_empty.mmd",
    "invalid_comment_only.mmd",
];

fn is_nonblank_svg(svg: &str) -> bool {
    svg.contains("<svg")
        && svg.contains("</svg>")
        && [
            "text", "path", "rect", "circle", "ellipse", "polygon", "line",
        ]
        .iter()
        .any(|t| count(svg, t) > 0)
}

#[test]
fn valid_diagrams_render_with_their_labels() {
    for (file, labels) in VALID {
        let svg = app_render(&fixture(file))
            .unwrap_or_else(|e| panic!("{file} must render but the app path errored: {e}"));
        assert!(is_nonblank_svg(&svg), "{file}: blank or malformed SVG");
        let found = text_labels(&svg);
        for want in *labels {
            assert!(
                found.iter().any(|t| t.contains(want)),
                "{file}: label {want:?} missing from rendered SVG (found {found:?})"
            );
        }
    }
}

#[test]
fn valid_diagrams_render_deterministically() {
    for (file, _) in VALID {
        let src = fixture(file);
        let a = app_render(&src);
        let b = app_render(&src);
        assert_eq!(a, b, "{file}: two renders of the same source differ");
    }
}

#[test]
fn valid_diagram_summaries_are_stable() {
    for (file, _) in VALID {
        let svg = app_render(&fixture(file)).expect("renders");
        insta::assert_snapshot!(
            format!("summary__{}", file.trim_end_matches(".mmd")),
            summary(&svg)
        );
    }
}

/// Invalid input takes the app's error path or yields a visible diagram; it never panics and never
/// yields an empty/blank SVG that would draw as an empty block.
#[test]
fn invalid_diagrams_error_or_render_visibly_never_panic_or_blank() {
    for file in INVALID {
        let src = fixture(file);
        match app_render(&src) {
            Err(msg) => assert!(!msg.trim().is_empty(), "{file}: error has an empty message"),
            Ok(svg) => assert!(
                is_nonblank_svg(&svg),
                "{file}: rendered Ok but the SVG is blank (the app would draw an empty block)"
            ),
        }
        // Deterministic outcome too.
        assert_eq!(app_render(&src), app_render(&src), "{file}");
    }
}

#[test]
fn invalid_diagram_outcomes_are_snapshotted() {
    let mut out = String::new();
    for file in INVALID {
        let outcome = match app_render(&fixture(file)) {
            Err(_) => "Err (app falls back to showing the source)".to_string(),
            Ok(svg) => format!("Ok ({} labels)", text_labels(&svg).len()),
        };
        out.push_str(&format!("{file}: {outcome}\n"));
    }
    insta::assert_snapshot!("invalid_outcomes", out);
}

/// Hostile sources: arbitrary bytes-as-text, huge labels, deep nesting.
fn hostile_sources() -> Vec<String> {
    let mut sources = vec![
        "graph TD\n".to_string() + &"A".repeat(100_000),
        format!("graph TD\nA[{}] --> B\n", "x".repeat(50_000)),
        "graph TD\n".to_string() + &"subgraph s\n".repeat(200) + "A --> B\n" + &"end\n".repeat(200),
        "sequenceDiagram\n".to_string() + &"A->>B: m\n".repeat(2000),
        "pie\n".to_string() + &"\"k\" : 1\n".repeat(2000),
        "\0\0\0".to_string(),
        "graph\u{200f}TD\nA --> B".to_string(),
        "%%{init: {\"theme\": 5}}%%\ngraph TD\nA-->B".to_string(),
        "---\nconfig: [unterminated\n---\ngraph TD\nA-->B".to_string(),
        "---\nconfig:\n  fontSize: -1px\n  flowchart:\n    nodeSpacing: 999999999\n---\ngraph TD\nA-->B".to_string(),
    ];
    for kind in [
        "erDiagram",
        "classDiagram",
        "stateDiagram-v2",
        "gantt",
        "mindmap",
        "journey",
        "timeline",
        "gitGraph",
        "quadrantChart",
        "xychart-beta",
        "sankey-beta",
        "block-beta",
        "packet-beta",
        "radar-beta",
        "kanban",
        "requirementDiagram",
        "C4Context",
        "info",
    ] {
        sources.push(kind.to_string());
        sources.push(format!(
            "{kind}\n{}",
            "\u{0}-->:: ::: {{ }} [ ( \n".repeat(50)
        ));
    }
    sources
}

/// Set in the child process: render only this source and exit.
const HOSTILE_INDEX_ENV: &str = "MERMAID_HOSTILE_INDEX";

/// The claim is: no panic, no abort, no hang (the app's render is bounded by size limits and a
/// time budget, #1477) for any hostile source. A stack overflow is an abort that kills the whole
/// test process, so each source runs in its own child process (this same test binary, re-run with
/// `MERMAID_HOSTILE_INDEX=N`). A crash of one input then names that input instead of hiding the
/// rest (#1493).
#[test]
fn hostile_sources_never_panic() {
    let sources = hostile_sources();
    if let Ok(index) = std::env::var(HOSTILE_INDEX_ENV) {
        let index: usize = index.parse().expect("MERMAID_HOSTILE_INDEX is a number");
        let _ = app_render(&sources[index]);
        return;
    }
    let exe = std::env::current_exe().expect("test binary path");
    let mut failures = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let output = std::process::Command::new(&exe)
            .args([
                "--exact",
                "hostile_sources_never_panic",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(HOSTILE_INDEX_ENV, index.to_string())
            .output()
            .expect("spawn child test process");
        if !output.status.success() {
            let head: String = source.chars().take(80).collect();
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(3).collect();
            failures.push(format!(
                "#{index} {:?} (status {:?}) source {head:?}: {}",
                output.status,
                output.status.code(),
                tail.join(" | ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} hostile sources crashed or panicked:\n{}",
        failures.len(),
        sources.len(),
        failures.join("\n")
    );
}

fn big_flowchart(nodes: usize) -> String {
    let mut s = String::from("graph TD\n");
    for i in 1..nodes {
        s.push_str(&format!("N{} --> N{}\n", (i - 1) / 2, i));
    }
    s
}

/// A 300-node flowchart renders, keeps every node label, and finishes within a (very generous)
/// bound. The real budget is the criterion bench `mermaid_render_bench` (docs/testing.md).
#[test]
fn large_diagram_renders_all_nodes_within_bound() {
    let src = big_flowchart(300);
    let start = Instant::now();
    let svg = app_render(&src).expect("300-node flowchart renders");
    let elapsed = start.elapsed();
    let labels = text_labels(&svg);
    for i in [0usize, 1, 150, 299] {
        let want = format!("N{i}");
        assert!(labels.iter().any(|l| l == &want), "node {want} missing");
    }
    assert!(
        elapsed < Duration::from_secs(60),
        "300-node flowchart took {elapsed:?}"
    );
}

#[test]
fn large_sequence_diagram_renders_within_bound() {
    let mut src = String::from("sequenceDiagram\nparticipant A\nparticipant B\n");
    for i in 0..200 {
        src.push_str(&format!("A->>B: message {i}\nB-->>A: reply {i}\n"));
    }
    let start = Instant::now();
    let svg = app_render(&src).expect("400-message sequence renders");
    assert!(text_labels(&svg).iter().any(|l| l.contains("message 199")));
    assert!(start.elapsed() < Duration::from_secs(60));
}
