//! Memory accounting for opening a big file in the code editor's buffer (#1607, #1340, #1407).
//!
//! A counting global allocator (this test binary only; it holds a single test so nothing else
//! allocates concurrently) measures what `Buffer::replace_all` of a generated 7 MB file keeps and
//! what it peaks at, and checks the retained bytes against `Buffer::memory_stats`:
//!
//! * everything the buffer keeps after the load is its content tree: no second copy of the text,
//!   no undo entry, no leftover delta (`retained <= tree node bytes + slack`);
//! * the load's transient peak stays bounded (a loose guard against the multi-GB regressions of
//!   APP-5392 and #1340; the real-app number is the appci `editor_memory_7mb` case);
//! * dropping the input and the buffer gives everything back (`after drop <= baseline + slack`,
//!   the baseline taken before the input text existed).
//!
//! The numbers are printed (`--nocapture`) so a run doubles as a breakdown. Note that the
//! dev-dependency on `sum_tree/test-util` shrinks the tree's fan-out in test builds, so the absolute
//! tree size here is larger than in the app; the app's own figure is logged under the
//! `editor_memory` target when a file opens.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use warp_editor::content::buffer::Buffer;
use warp_editor::content::text::IndentBehavior;
use warpui::App;

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged to the system allocator.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            if new_size >= layout.size() {
                let live = LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed)
                    + (new_size - layout.size());
                PEAK.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

fn reset_peak() -> usize {
    let now = live();
    PEAK.store(now, Ordering::Relaxed);
    now
}

const MB: f64 = 1024.0 * 1024.0;

/// The appci case's plain-text file (`editor_mem_file(kind="txt")`): `line N: ` and 6-14 words,
/// ~80 bytes per line, generated with a fixed LCG so the size is reproducible.
fn seven_mb_text() -> String {
    const WORDS: [&str; 24] = [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
        "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi",
        "chi", "psi", "omega",
    ];
    let target = 7 * 1024 * 1024;
    let mut state: u64 = 1340;
    let mut next = move |n: u64| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) % n
    };
    let mut text = String::with_capacity(target + 256);
    let mut line = 0usize;
    while text.len() < target {
        text.push_str(&format!("line {line}:"));
        for _ in 0..6 + next(9) {
            text.push(' ');
            text.push_str(WORDS[next(WORDS.len() as u64) as usize]);
        }
        text.push('\n');
        line += 1;
    }
    text
}

#[test]
fn opening_a_7mb_file_keeps_only_the_content_tree_and_frees_it_on_drop() {
    App::test((), |mut app| async move {
        // Nothing of the load exists yet: the final drop check compares against this.
        let baseline = live();
        let text = seven_mb_text();
        let text_bytes = text.len();

        // `before` and `after` are both taken while `text` is live, so the input is in both and
        // cancels out: dropping it inside the window would hide a second copy of the same size.
        let before = reset_peak();
        let buffer = app.add_model(|_| Buffer::new(Box::new(|_, _| IndentBehavior::Ignore)));
        buffer.update(&mut app, |buffer, ctx| buffer.replace_all(&text, ctx));
        let peak = PEAK.load(Ordering::Relaxed);
        let after = live();
        drop(text);
        let stats = buffer.read(&app, |buffer, _| buffer.memory_stats());

        let retained = after.saturating_sub(before);
        let transient = peak.saturating_sub(before);
        eprintln!(
            "editor_memory test: text {:.1} MB, {} lines; retained {:.1} MB ({:.1}x text), peak +{:.1} MB ({:.1}x text); \
             tree: {} items in {} leaves + {} internal nodes = {:.1} MB, {} B per item; undo entries {}",
            text_bytes as f64 / MB,
            stats.lines,
            retained as f64 / MB,
            retained as f64 / text_bytes as f64,
            transient as f64 / MB,
            transient as f64 / text_bytes as f64,
            stats.tree.items,
            stats.tree.leaves,
            stats.tree.internal_nodes,
            stats.tree.node_bytes as f64 / MB,
            stats.item_bytes,
            stats.undo_entries,
        );

        assert!(stats.text_bytes >= text_bytes, "{stats:?}");
        assert_eq!(
            stats.undo_entries, 0,
            "a file load must leave no undo entry"
        );
        // Everything retained is the tree itself (2 MB slack for the model, the app's own
        // bookkeeping and allocator-independent noise).
        let slack = 2 * 1024 * 1024;
        assert!(
            retained <= stats.tree.node_bytes + slack,
            "the load retained {retained} B but the content tree accounts for {} B: something else keeps a copy",
            stats.tree.node_bytes
        );
        // Transient peak: the input, the formatted copy replace_all builds, the tree, the delta's
        // styled blocks. 20x the file is a loose bound (the pre-#1340 path peaked far above it).
        assert!(
            transient <= stats.tree.node_bytes + 20 * text_bytes,
            "the load peaked at +{transient} B for a {text_bytes} B file"
        );

        drop(buffer);
        // Let the app drop the released model.
        app.update(|_| {});
        let dropped = live();
        eprintln!(
            "editor_memory test: after dropping the text and the buffer {:+.1} MB vs before the load",
            (dropped as f64 - baseline as f64) / MB
        );
        assert!(
            dropped <= baseline + slack,
            "dropping the buffer kept {} B of the load",
            dropped.saturating_sub(baseline)
        );
    });
}
