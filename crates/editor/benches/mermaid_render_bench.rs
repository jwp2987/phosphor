//! Render time of the Mermaid diagram corpus through `mermaid_to_svg::render_mermaid_to_svg`, the
//! call behind every diagram the app shows (editor `mermaid_asset_source`, notebooks clipboard
//! export). Run on the build box: `script/agent-cargo <name> bench -p warp_editor --bench
//! mermaid_render_bench`.
//!
//! Budget (a diagram is rendered off the main thread by the asset cache, but the user waits for it):
//! a typical small diagram (the 14 corpus files) under 50 ms each, the 300-node flowchart under
//! 2 s, the 400-message sequence diagram under 2 s. Set `MERMAID_BENCH_ENFORCE=1` to turn a
//! blown budget (measured as the median of 5 renders before criterion starts) into a failure; the
//! default only prints a warning, because laptop and box numbers differ. #1460.

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};

const SMALL: &[(&str, &str)] = &[
    (
        "flowchart_td",
        include_str!("../tests/fixtures/mermaid/flowchart_td.mmd"),
    ),
    (
        "flowchart_lr",
        include_str!("../tests/fixtures/mermaid/flowchart_lr.mmd"),
    ),
    (
        "sequence",
        include_str!("../tests/fixtures/mermaid/sequence.mmd"),
    ),
    ("class", include_str!("../tests/fixtures/mermaid/class.mmd")),
    ("state", include_str!("../tests/fixtures/mermaid/state.mmd")),
    ("er", include_str!("../tests/fixtures/mermaid/er.mmd")),
    ("gantt", include_str!("../tests/fixtures/mermaid/gantt.mmd")),
    ("pie", include_str!("../tests/fixtures/mermaid/pie.mmd")),
    (
        "mindmap",
        include_str!("../tests/fixtures/mermaid/mindmap.mmd"),
    ),
    (
        "journey",
        include_str!("../tests/fixtures/mermaid/journey.mmd"),
    ),
    (
        "timeline",
        include_str!("../tests/fixtures/mermaid/timeline.mmd"),
    ),
    (
        "frontmatter",
        include_str!("../tests/fixtures/mermaid/frontmatter.mmd"),
    ),
    (
        "unicode_labels",
        include_str!("../tests/fixtures/mermaid/unicode_labels.mmd"),
    ),
    (
        "invalid_garbage",
        include_str!("../tests/fixtures/mermaid/invalid_garbage.mmd"),
    ),
];

fn big_flowchart(nodes: usize) -> String {
    let mut s = String::from("graph TD\n");
    for i in 1..nodes {
        s.push_str(&format!("N{} --> N{}\n", (i - 1) / 2, i));
    }
    s
}

fn big_sequence(pairs: usize) -> String {
    let mut s = String::from("sequenceDiagram\nparticipant A\nparticipant B\n");
    for i in 0..pairs {
        s.push_str(&format!("A->>B: message {i}\nB-->>A: reply {i}\n"));
    }
    s
}

fn median_render(src: &str) -> Duration {
    let mut times: Vec<Duration> = (0..5)
        .map(|_| {
            let start = Instant::now();
            let _ = black_box(mermaid_to_svg::render_mermaid_to_svg(black_box(src), None));
            start.elapsed()
        })
        .collect();
    times.sort();
    times[2]
}

fn check_budget(name: &str, src: &str, budget: Duration) {
    let median = median_render(src);
    if median > budget {
        let msg = format!("mermaid budget blown: {name} median {median:?} > {budget:?}");
        if std::env::var_os("MERMAID_BENCH_ENFORCE").is_some() {
            panic!("{msg}");
        }
        eprintln!("WARNING: {msg}");
    }
}

fn bench_mermaid(c: &mut Criterion) {
    let flow = big_flowchart(300);
    let seq = big_sequence(200);

    for (name, src) in SMALL {
        check_budget(name, src, Duration::from_millis(50));
    }
    check_budget("flowchart_300_nodes", &flow, Duration::from_secs(2));
    check_budget("sequence_400_messages", &seq, Duration::from_secs(2));

    let mut group = c.benchmark_group("mermaid_render");
    group.sample_size(20);
    for (name, src) in SMALL {
        group.bench_function(*name, |b| {
            b.iter(|| mermaid_to_svg::render_mermaid_to_svg(black_box(src), None))
        });
    }
    group.sample_size(10);
    group.bench_function("flowchart_300_nodes", |b| {
        b.iter(|| mermaid_to_svg::render_mermaid_to_svg(black_box(&flow), None))
    });
    group.bench_function("sequence_400_messages", |b| {
        b.iter(|| mermaid_to_svg::render_mermaid_to_svg(black_box(&seq), None))
    });
    group.finish();
}

criterion_group!(benches, bench_mermaid);
criterion_main!(benches);
