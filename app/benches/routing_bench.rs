//! Input-type routing of LARGE text (#1604).
//!
//! ```text
//! cargo bench -p warp --features dev-support --bench routing_bench
//! ```
//!
//! Before #1604 anything over 4096 bytes was never classified, so a pasted script Enter-ed in
//! agent mode went to the agent. Now `input_classifier::util::routing::route` and the Enter-time
//! predicates run on the whole text, off the UI thread for text over `MAX_SYNC_ROUTING_BYTES`
//! (4 KiB), and the Enter waits for that verdict with a compute allowance of 1 ms per 4 KiB
//! (`large_text_compute_allowance`): 1 MB earns ~255 ms, 10 MB ~2.5 s. A classification slower
//! than its allowance is handed back to the editor, not misrouted, so the allowance is the real
//! budget here, and the `max_ns` limits in `script/bench_thresholds.json` are set AT it (not the
//! usual 10x headroom): a bench over its limit means an Enter on a paste that size would be handed
//! back. Expected release numbers are a few times under the limits (~50-100 ms per MB for a line
//! whose every word is examined; the rules are one pass per rule with a bounded amount of work per
//! word and no regex).
//!
//! * `routing/route_1mb_line`: a 1 MB single-line command whose every word reaches the body
//!   rules (no operator, so no early return).
//! * `routing/route_10mb_line`: the same at 10 MB.
//! * `routing/route_10mb_script`: a 10 MB multi-line shell script (function-word vetoes, so it
//!   falls through every rule).
//! * `routing/enter_predicates_10mb_line`: the predicates the Enter path runs besides `route`
//!   (`is_unambiguous_shell_command`, `has_no_prose_evidence`, `reads_as_request_to_agent`).
//!
//! Linearity itself is asserted by the nextest test
//! `util_tests::test_routing_rules_are_linear_in_the_text` (10 MB vs 1 MB ratio); these benches
//! hold the absolute release-build time.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use input_classifier::util::routing::{
    self, RoutingContext, ShellFamily, Surface, Typeahead as RoutingTypeahead,
};
use input_classifier::util::{
    has_no_prose_evidence, is_unambiguous_shell_command, reads_as_request_to_agent,
};
use warp_completer::ParsedTokensSnapshot;

const ONE_MB: usize = 1024 * 1024;

/// The facts the app has at the keystroke for a buffer over the parse cap: no parse, nothing
/// known about the head.
fn context(surface: Surface) -> RoutingContext {
    RoutingContext {
        surface,
        shell: ShellFamily::Posix,
        head_known_to_shell: None,
        commands_fully_loaded: false,
        command_data_current: false,
        is_agent_follow_up: false,
        ran_before_exactly: false,
        denylisted: false,
        typeahead: RoutingTypeahead::None,
    }
}

fn text_of(unit: &str, bytes: usize) -> String {
    let mut text = unit.repeat(bytes / unit.len() + 1);
    text.truncate(bytes);
    while !text.is_char_boundary(text.len()) {
        text.pop();
    }
    text
}

fn unparsed(text: &str) -> ParsedTokensSnapshot {
    ParsedTokensSnapshot {
        buffer_text: text.to_owned(),
        parsed_tokens: Vec::new(),
    }
}

const LINE_UNIT: &str = "cp ./src/main.rs ./backup/main_v2.rs ";
const SCRIPT_UNIT: &str = "for f in src/*.rs; do grep -n TODO $f | sort > /tmp/out.txt; done\n";

fn route_both_surfaces(snapshot: &ParsedTokensSnapshot) -> (routing::Decision, routing::Decision) {
    (
        routing::route(snapshot, &context(Surface::PlainTerminal)),
        routing::route(snapshot, &context(Surface::AgentConversation)),
    )
}

fn routing_large_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("routing");
    group.sample_size(10);

    let line_1mb = unparsed(&text_of(LINE_UNIT, ONE_MB));
    group.throughput(Throughput::Bytes(line_1mb.buffer_text.len() as u64));
    group.bench_function("route_1mb_line", |b| {
        b.iter(|| black_box(route_both_surfaces(black_box(&line_1mb))))
    });

    let line_10mb = unparsed(&text_of(LINE_UNIT, 10 * ONE_MB));
    group.throughput(Throughput::Bytes(line_10mb.buffer_text.len() as u64));
    group.bench_function("route_10mb_line", |b| {
        b.iter(|| black_box(route_both_surfaces(black_box(&line_10mb))))
    });

    let script_10mb = unparsed(&text_of(SCRIPT_UNIT, 10 * ONE_MB));
    group.throughput(Throughput::Bytes(script_10mb.buffer_text.len() as u64));
    group.bench_function("route_10mb_script", |b| {
        b.iter(|| black_box(route_both_surfaces(black_box(&script_10mb))))
    });

    group.throughput(Throughput::Bytes(line_10mb.buffer_text.len() as u64));
    group.bench_function("enter_predicates_10mb_line", |b| {
        b.iter(|| {
            let snapshot = black_box(&line_10mb);
            black_box((
                is_unambiguous_shell_command(snapshot),
                has_no_prose_evidence(&snapshot.buffer_text),
                reads_as_request_to_agent(&snapshot.buffer_text),
            ))
        })
    });
    group.finish();
}

criterion_group!(benches, routing_large_text);
criterion_main!(benches);
