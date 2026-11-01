//! ANSI / VT parser throughput into a real terminal model.
//!
//! ```text
//! cargo bench -p warp --features dev-support --bench ansi_bench
//! ```
//!
//! The input is 1 MB of mixed pty output (plain text, SGR colours, cursor movement, erase,
//! OSC titles, UTF-8, tabs and mode toggles), fed in 4 KiB reads like the pty reader does.
//!
//! Budget (owner): key-to-paint Phosphor overhead <= 2 ms. The echo of a key is a handful of
//! bytes, so what matters is the parser's steady throughput and that a full 4 KiB read never
//! holds the model's lock for long: at a throughput of >= 50 MB/s a 4 KiB read costs < 0.1 ms,
//! far inside the budget. A drop below ~10 MB/s (one read > 0.4 ms) is a regression to chase,
//! since the reader holds the model lock while it parses (#1403).

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use warp::terminal::model::terminal_model::TerminalModel;

const TOTAL_BYTES: usize = 1 << 20;
const READ_BYTES: usize = 4096;

/// A deterministic mixed stream (no rand dependency: a small LCG picks the next fragment).
fn mixed_stream() -> Vec<u8> {
    const FRAGMENTS: &[&[u8]] = &[
        b"The quick brown fox jumps over the lazy dog 0123456789\r\n",
        b"\x1b[31mred\x1b[0m \x1b[1;32mbold green\x1b[0m \x1b[38;5;208morange\x1b[0m\r\n",
        b"\x1b[38;2;12;200;99mtruecolor\x1b[0m text\r\n",
        b"\x1b[2K\x1b[1G\x1b[Aprompt $ \x1b[K",
        b"\x1b]0;window title\x07",
        b"\x1b[?25l\x1b[?25h\x1b[?2004h\x1b[?2004l",
        b"caf\xc3\xa9 \xe2\x9c\x93 \xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\r\n",
        b"col1\tcol2\tcol3\r\n",
        b"\x1b[10;20H\x1b[5C\x1b[2D\x1b[J",
        b"a long line without any escape sequences at all, just printable ascii text repeated to fill rows ",
    ];
    let mut out = Vec::with_capacity(TOTAL_BYTES + 128);
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    while out.len() < TOTAL_BYTES {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        out.extend_from_slice(FRAGMENTS[(state >> 33) as usize % FRAGMENTS.len()]);
    }
    out.truncate(TOTAL_BYTES);
    out
}

fn parser_throughput(c: &mut Criterion) {
    let stream = mixed_stream();
    let mut group = c.benchmark_group("ansi");
    group.throughput(Throughput::Bytes(stream.len() as u64));
    group.sample_size(20);
    group.bench_function("mixed_1mb_in_4k_reads", |b| {
        b.iter_batched(
            || TerminalModel::mock(None, None),
            |mut model| {
                for read in stream.chunks(READ_BYTES) {
                    model.process_bytes(read);
                }
                model
            },
            BatchSize::PerIteration,
        )
    });
    group.finish();
}

criterion_group!(benches, parser_throughput);
criterion_main!(benches);
