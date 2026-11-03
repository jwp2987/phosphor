use std::{
    hash::{DefaultHasher, Hash, Hasher},
    panic::AssertUnwindSafe,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use warpui::{
    AppContext, SingletonEntity,
    assets::asset_cache::{AssetCache, AssetSource, AssetState, AsyncAssetId, AsyncAssetType},
    r#async::FutureExt as _,
    image_cache::ImageType,
    units::{IntoPixels, Pixels},
};

use crate::render::{
    layout::TextLayout,
    model::{BlockSpacing, ImageBlockConfig},
};

const DEFAULT_MERMAID_HEIGHT_LINE_MULTIPLIER: f32 = 10.0;
const FAILED_MERMAID_HEIGHT_LINE_MULTIPLIER: f32 = 2.0;

/// The largest diagram source we hand to the renderer. Real diagrams are a few KB.
const MAX_MERMAID_SOURCE_BYTES: usize = 256 * 1024;
/// The longest line we hand to the renderer. The renderer wraps a label by re-measuring its whole
/// remaining text once per output line, so one unbroken 100k-character token costs minutes of CPU
/// (the hang found by the `hostile_sources_never_panic` corpus test, #1477).
const MAX_MERMAID_LINE_CHARS: usize = 4_000;
/// The longest unbroken (whitespace-free) token we hand to the renderer. The renderer's word
/// splitter costs roughly (token length)^2 / (wrap width) per label, so the cost of one token is
/// bounded by bounding its length; no real label has a 1000-character word (#1479).
const MAX_MERMAID_TOKEN_CHARS: usize = 1_000;
/// How many render threads may be alive at once. A thread cannot be killed, so a render that
/// outlives its budget keeps its core until it finishes; capping the live threads bounds how many
/// cores abandoned renders can hold (#1479). Further renders fail to the source fallback.
const MAX_CONCURRENT_RENDERS: usize = 8;
/// The deepest block nesting (subgraphs, composite states, `block`s, loop/alt groups, indentation)
/// and bracket nesting within one line we hand to the renderer. The parser and layout recurse per
/// level, and a stack overflow is an abort, not a panic: neither `catch_unwind` nor the time budget
/// can contain it (#1493). Real diagrams nest a handful of levels.
const MAX_MERMAID_NESTING: usize = 32;
/// Stack for the render thread: far more than the 2 MiB default, as defence in depth behind the
/// nesting limit. Address space only; pages are committed as they are touched.
const MERMAID_RENDER_STACK_BYTES: usize = 64 * 1024 * 1024;
/// Live `mermaid-render` threads.
static LIVE_RENDERS: AtomicUsize = AtomicUsize::new(0);

/// One of the `MAX_CONCURRENT_RENDERS` slots; released when dropped (when the render thread ends,
/// even by panic).
struct RenderSlot(&'static AtomicUsize);

impl RenderSlot {
    fn acquire(live: &'static AtomicUsize, cap: usize) -> Option<Self> {
        live.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < cap).then_some(n + 1)
        })
        .ok()
        .map(|_| Self(live))
    }
}

impl Drop for RenderSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// How long a render may take before the diagram falls back to showing its source. Event-driven
/// give-up for anything the size limits above did not anticipate; the abandoned render thread
/// finishes (or not) on its own and its result is dropped.
const MERMAID_RENDER_TIMEOUT: Duration = Duration::from_secs(8);

struct MermaidDiagramAsset;

impl AsyncAssetType for MermaidDiagramAsset {}

pub fn mermaid_asset_source(source: &str) -> AssetSource {
    let source = source.to_string();
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let id = format!("configured:{:x}", hasher.finish());
    let fetch_source = source.clone();

    AssetSource::Async {
        id: AsyncAssetId::new::<MermaidDiagramAsset>(id),
        fetch: Arc::new(move || {
            let source = fetch_source.clone();
            Box::pin(render_bounded(source, MERMAID_RENDER_TIMEOUT))
        }),
    }
}

/// Rejects sources the renderer cannot handle in bounded time. An `Err` takes the app's normal
/// failed-diagram path (the plain-source fallback).
fn check_render_limits(source: &str) -> Result<(), anyhow::Error> {
    if source.len() > MAX_MERMAID_SOURCE_BYTES {
        anyhow::bail!(
            "diagram source is too large to render ({} bytes, limit {MAX_MERMAID_SOURCE_BYTES})",
            source.len()
        );
    }
    if let Some(line) = source
        .lines()
        .find(|line| line.chars().count() > MAX_MERMAID_LINE_CHARS)
    {
        anyhow::bail!(
            "diagram has a line too long to render ({} characters, limit {MAX_MERMAID_LINE_CHARS})",
            line.chars().count()
        );
    }
    // U+0000 is the compound-graph root sentinel in the renderer's graph library (graphlib_rust
    // `GRAPH_NODE`). A node whose id is "\0" becomes its own parent and child, and every
    // tree walk in the layout (e.g. dagre nesting_graph `tree_depths`) recurses forever: a stack
    // overflow, which aborts the app. No real diagram contains a NUL (the real cause of the
    // remaining #1493 abort).
    if source.contains('\0') {
        anyhow::bail!("diagram contains a NUL character, which the renderer cannot lay out");
    }
    let nesting = max_nesting(source);
    if nesting > MAX_MERMAID_NESTING {
        anyhow::bail!(
            "diagram is nested too deeply to render ({nesting} levels, limit {MAX_MERMAID_NESTING})"
        );
    }
    if let Some(id) = subgraph_cycle(source) {
        anyhow::bail!(
            "diagram nests subgraph `{id}` inside itself, which the renderer cannot lay out"
        );
    }
    if let Some(token) = source
        .split_whitespace()
        .find(|token| token.chars().count() > MAX_MERMAID_TOKEN_CHARS)
    {
        anyhow::bail!(
            "diagram has a word too long to render ({} characters, limit {MAX_MERMAID_TOKEN_CHARS})",
            token.chars().count()
        );
    }
    Ok(())
}

/// A subgraph id that is (transitively) its own ancestor, if any. The renderer records each
/// subgraph's parent by id, so `subgraph s` nested inside `subgraph s` (or `a` in `b` in `a`,
/// even in separate blocks) makes a parent cycle, and its layout walks the parent links without a
/// visited set: unbounded recursion that overflows any stack, however large, and aborts the app
/// (the real cause of #1493; nesting depth alone was never the problem). Mirrors the renderer's
/// parsing: the id of `subgraph id`, `subgraph id[title]`; a multi-word title gets a unique id; a
/// line that is exactly `end` closes a block, and an unmatched `end` stops the renderer's parse.
fn subgraph_cycle(source: &str) -> Option<String> {
    use std::collections::HashMap;
    let mut open: Vec<String> = Vec::new();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut indegree: HashMap<String, usize> = HashMap::new();
    let mut anonymous = 0usize;
    for line in source.lines() {
        let line = line.trim();
        if let Some(after) = line.strip_prefix("subgraph ") {
            let after = after.trim();
            let id = match after.find('[') {
                Some(bracket) if after.ends_with(']') => after[..bracket].trim().to_string(),
                Some(_) => after.to_string(),
                None if after.split_whitespace().count() > 1 => {
                    anonymous += 1;
                    format!("\0anonymous{anonymous}")
                }
                None => after.to_string(),
            };
            indegree.entry(id.clone()).or_insert(0);
            if let Some(parent) = open.last() {
                children.entry(parent.clone()).or_default().push(id.clone());
                *indegree.entry(id.clone()).or_insert(0) += 1;
            }
            open.push(id);
        } else if line == "end" && open.pop().is_none() {
            break;
        }
    }
    // Topological sort: whatever cannot be removed lies on, or below, a cycle.
    let mut ready: Vec<String> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut removed = 0usize;
    while let Some(id) = ready.pop() {
        removed += 1;
        for child in children.get(&id).into_iter().flatten() {
            if let Some(degree) = indegree.get_mut(child) {
                *degree -= 1;
                if *degree == 0 {
                    ready.push(child.clone());
                }
            }
        }
    }
    if removed == indegree.len() {
        return None;
    }
    indegree
        .into_iter()
        .filter(|(_, degree)| *degree > 0)
        .map(|(id, _)| id)
        .min()
}

/// The deepest nesting in `source`: open block keywords (closed by `end` or `}`), leading
/// indentation (tabs count as 4 columns, 2 columns per level), and brackets within a line.
fn max_nesting(source: &str) -> usize {
    const OPENERS: &str = "subgraph block loop alt opt par critical break rect box namespace";
    let mut blocks = 0usize;
    let mut deepest = 0usize;
    for line in source.lines() {
        let trimmed = line.trim_start();
        let columns: usize = line[..line.len() - trimmed.len()]
            .chars()
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum();
        deepest = deepest.max(columns / 2);
        let first = trimmed.split_whitespace().next().unwrap_or("");
        let word = first.trim_end_matches([':', '{']);
        let opens_block = OPENERS.split(' ').any(|o| o == word)
            || ((word == "state" || word == "class") && trimmed.trim_end().ends_with('{'));
        if opens_block {
            blocks += 1;
        } else if (first == "end" || first == "}") && blocks > 0 {
            blocks -= 1;
        }
        deepest = deepest.max(blocks);
        let mut brackets = 0usize;
        for c in line.chars() {
            match c {
                '(' | '[' | '{' => {
                    brackets += 1;
                    deepest = deepest.max(brackets);
                }
                ')' | ']' | '}' => brackets = brackets.saturating_sub(1),
                _ => {}
            }
        }
    }
    deepest
}

/// Renders on a worker thread so a runaway render neither blocks the caller past `timeout` nor
/// takes the caller down with a panic. Both end as an `Err`, never a hang or a crash.
async fn render_bounded(source: String, timeout: Duration) -> anyhow::Result<Bytes> {
    check_render_limits(&source)?;
    let slot = RenderSlot::acquire(&LIVE_RENDERS, MAX_CONCURRENT_RENDERS).ok_or_else(|| {
        anyhow::anyhow!("too many diagram renders are already running; showing the source")
    })?;
    #[cfg(not(target_family = "wasm"))]
    if let Some(exe) = SUBPROCESS_RENDERER.get() {
        let mut command = std::process::Command::new(exe);
        command.arg(MERMAID_WORKER_SUBCOMMAND);
        return render_in_subprocess(command, source, timeout, slot).await;
    }
    let (tx, rx) = async_channel::bounded(1);
    std::thread::Builder::new()
        .name("mermaid-render".into())
        .stack_size(MERMAID_RENDER_STACK_BYTES)
        .spawn(move || {
            let _slot = slot;
            let rendered = std::panic::catch_unwind(AssertUnwindSafe(|| {
                mermaid_to_svg::render_mermaid_to_svg(&source, None)
            }));
            let _ = tx.send_blocking(rendered);
        })
        .map_err(|e| anyhow::anyhow!("could not start the diagram renderer: {e}"))?;
    match rx.recv().with_timeout(timeout).await {
        Ok(Ok(Ok(Ok(svg)))) => Ok(Bytes::from(svg.into_bytes())),
        Ok(Ok(Ok(Err(e)))) => Err(e.into()),
        Ok(Ok(Err(_))) => Err(anyhow::anyhow!("the diagram renderer panicked")),
        Ok(Err(_)) => Err(anyhow::anyhow!("the diagram renderer stopped unexpectedly")),
        Err(_) => {
            log::warn!("mermaid render exceeded {timeout:?}; showing the source instead");
            Err(anyhow::anyhow!(
                "rendering the diagram took longer than {timeout:?}"
            ))
        }
    }
}

/// The hidden `warp_cli` worker subcommand that runs [`run_render_worker`]. Must match
/// `WorkerCommand::MermaidRender`'s clap name.
pub const MERMAID_WORKER_SUBCOMMAND: &str = "mermaid-render";
/// The largest SVG we accept back from the renderer process.
const MAX_MERMAID_SVG_BYTES: usize = 32 * 1024 * 1024;

/// The executable to re-run as the render worker. Unset (tests, benches, embedders that do not
/// dispatch the worker subcommand) means render in-process.
#[cfg(not(target_family = "wasm"))]
static SUBPROCESS_RENDERER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Renders every diagram in a short-lived child process (`exe mermaid-render`, source on stdin,
/// SVG on stdout) from now on. A stack overflow in the renderer is a process abort that nothing
/// in-process can contain (#1557); in a child it only fails that diagram, which then takes the
/// source fallback. `exe` must dispatch [`MERMAID_WORKER_SUBCOMMAND`] to [`run_render_worker`].
/// Only the first call has an effect.
#[cfg(not(target_family = "wasm"))]
pub fn use_subprocess_renderer(exe: std::path::PathBuf) {
    let _ = SUBPROCESS_RENDERER.set(exe);
}

/// The render worker's body: read the diagram source from stdin, write its SVG to stdout. An
/// error (or a crash, which the parent sees as a failed exit) means "show the source". Runs the
/// render on a big-stack thread, the same defence in depth as the in-process path.
#[cfg(not(target_family = "wasm"))]
pub fn run_render_worker() -> anyhow::Result<()> {
    use std::io::{Read as _, Write as _};
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_MERMAID_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let source = String::from_utf8(bytes)?;
    check_render_limits(&source)?;
    let svg = std::thread::Builder::new()
        .name("mermaid-render".into())
        .stack_size(MERMAID_RENDER_STACK_BYTES)
        .spawn(move || mermaid_to_svg::render_mermaid_to_svg(&source, None))?
        .join()
        .map_err(|_| anyhow::anyhow!("the diagram renderer panicked"))??;
    let mut out = std::io::stdout().lock();
    out.write_all(svg.as_bytes())?;
    out.flush()?;
    Ok(())
}

/// State shared between the host thread and the awaiting caller so the caller can kill the child
/// at the deadline.
#[cfg(not(target_family = "wasm"))]
#[derive(Default)]
struct ChildHandle {
    child: std::sync::Mutex<Option<std::process::Child>>,
    cancelled: std::sync::atomic::AtomicBool,
}

#[cfg(not(target_family = "wasm"))]
impl ChildHandle {
    fn kill(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.child.lock()
            && let Some(child) = guard.as_mut()
        {
            let _ = child.kill();
        }
    }
}

/// Runs `command` as the render worker: `source` on its stdin, SVG back on its stdout, bounded
/// by `timeout` and `MAX_MERMAID_SVG_BYTES`. The UI never blocks: all pipe I/O and the wait run
/// on a helper thread; the caller awaits a channel. A crash, non-zero exit, oversize or timeout
/// is an `Err` (the source fallback), and the child is killed and reaped.
#[cfg(not(target_family = "wasm"))]
async fn render_in_subprocess(
    mut command: std::process::Command,
    source: String,
    timeout: Duration,
    slot: RenderSlot,
) -> anyhow::Result<Bytes> {
    use std::process::Stdio;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let handle = Arc::new(ChildHandle::default());
    let (tx, rx) = async_channel::bounded(1);
    let host = handle.clone();
    std::thread::Builder::new()
        .name("mermaid-render-host".into())
        .spawn(move || {
            let _slot = slot;
            let _ = tx.send_blocking(run_worker_process(command, source, &host));
        })
        .map_err(|e| anyhow::anyhow!("could not start the diagram renderer: {e}"))?;
    match rx.recv().with_timeout(timeout).await {
        Ok(Ok(result)) => result.map(Bytes::from),
        Ok(Err(_)) => Err(anyhow::anyhow!("the diagram renderer stopped unexpectedly")),
        Err(_) => {
            handle.kill();
            log::warn!("mermaid render exceeded {timeout:?}; showing the source instead");
            Err(anyhow::anyhow!(
                "rendering the diagram took longer than {timeout:?}"
            ))
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn run_worker_process(
    mut command: std::process::Command,
    source: String,
    handle: &ChildHandle,
) -> anyhow::Result<Vec<u8>> {
    use std::io::{Read as _, Write as _};
    let mut child = command
        .spawn()
        .map_err(|e| anyhow::anyhow!("could not start the diagram renderer process: {e}"))?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    {
        let mut guard = handle
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("diagram renderer state poisoned"))?;
        if handle.cancelled.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("diagram render was cancelled");
        }
        *guard = Some(child);
    }
    // Written from its own thread so a child that stops reading cannot deadlock us against a
    // full stdout pipe; a write error just means the child already exited.
    if let Some(mut stdin) = stdin {
        std::thread::spawn(move || {
            let _ = stdin.write_all(source.as_bytes());
        });
    }
    let mut svg = Vec::new();
    let read = match stdout {
        Some(stdout) => stdout
            .take(MAX_MERMAID_SVG_BYTES as u64 + 1)
            .read_to_end(&mut svg),
        None => Ok(0),
    };
    let oversize = svg.len() > MAX_MERMAID_SVG_BYTES;
    if oversize || read.is_err() {
        handle.kill();
    }
    let status = {
        let mut guard = handle
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("diagram renderer state poisoned"))?;
        guard.take().map(|mut child| child.wait())
    };
    if oversize {
        anyhow::bail!("diagram renderer output is too large ({MAX_MERMAID_SVG_BYTES} byte limit)");
    }
    read.map_err(|e| anyhow::anyhow!("could not read the diagram renderer output: {e}"))?;
    match status {
        Some(Ok(status)) if status.success() && !svg.is_empty() => Ok(svg),
        Some(Ok(status)) => Err(anyhow::anyhow!(
            "the diagram renderer process failed ({status}); showing the source"
        )),
        Some(Err(e)) => Err(anyhow::anyhow!(
            "could not wait for the diagram renderer: {e}"
        )),
        None => Err(anyhow::anyhow!("diagram render was cancelled")),
    }
}

pub fn mermaid_diagram_layout(
    source: &str,
    layout: &TextLayout,
    spacing: BlockSpacing,
    app: &AppContext,
) -> (AssetSource, ImageBlockConfig) {
    let asset_source = mermaid_asset_source(source);
    // The available width can be non-positive when the editor's viewport has not
    // been measured yet, or when the block's horizontal insets are wider than the
    // viewport. Clamp it so the block never claims a negative width.
    let max_width = (layout.max_width() - spacing.x_axis_offset()).max(Pixels::zero());
    let (width, height) = mermaid_diagram_size(&asset_source, max_width, app).unwrap_or_else(|| {
        // A diagram that failed to render collapses to a short placeholder
        // height instead of reserving the full loading-state height.
        let height = layout.rich_text_styles().base_line_height()
            * mermaid_diagram_fallback_height_line_multiplier(&asset_source, app).into_pixels();
        (max_width, height)
    });

    (
        asset_source,
        ImageBlockConfig {
            width,
            height,
            spacing,
        },
    )
}

/// The line-height multiplier for a diagram with no intrinsic size yet: a
/// short placeholder once it has failed to load, otherwise the taller
/// loading/default reservation.
fn mermaid_diagram_fallback_height_line_multiplier(asset_source: &AssetSource, app: &AppContext) -> f32 {
    let asset_cache = AssetCache::as_ref(app);
    match asset_cache.load_asset::<ImageType>(asset_source.clone()) {
        AssetState::FailedToLoad(_) => FAILED_MERMAID_HEIGHT_LINE_MULTIPLIER,
        AssetState::Loading { .. } | AssetState::Loaded { .. } | AssetState::Evicted => {
            DEFAULT_MERMAID_HEIGHT_LINE_MULTIPLIER
        }
    }
}

fn mermaid_diagram_size(
    asset_source: &AssetSource,
    max_width: Pixels,
    app: &AppContext,
) -> Option<(Pixels, Pixels)> {
    // Scaling the diagram's intrinsic size into a zero-width slot produces a block
    // of zero height, which is indistinguishable from "no block" to every
    // height-keyed lookup in the layout sum tree (hit testing, autoscroll,
    // `block_at_height`). Until the viewport reports a usable width, keep the
    // reserved placeholder height instead.
    if max_width <= Pixels::zero() {
        return None;
    }
    let asset_cache = AssetCache::as_ref(app);
    let AssetState::Loaded { data } = asset_cache.load_asset::<ImageType>(asset_source.clone())
    else {
        return None;
    };
    let ImageType::Svg { svg } = data.as_ref() else {
        return None;
    };
    let intrinsic_size = svg.size();
    let intrinsic_width = intrinsic_size.width();
    let intrinsic_height = intrinsic_size.height();
    if intrinsic_width <= 0. || intrinsic_height <= 0. {
        return None;
    }
    let width = Pixels::new(max_width.as_f32().min(intrinsic_width));
    let height = Pixels::new(width.as_f32() * intrinsic_height / intrinsic_width);
    Some((width, height))
}

#[cfg(test)]
#[path = "mermaid_diagram_tests.rs"]
mod tests;

#[cfg(test)]
mod render_limit_tests {
    use super::*;

    fn render(source: &str, timeout: Duration) -> anyhow::Result<Bytes> {
        futures_lite::future::block_on(render_bounded(source.to_string(), timeout))
    }

    /// #1477: one unbroken 100k-character token made the renderer spin for minutes. It is
    /// refused up front, in well under a second, and takes the failed-diagram path.
    #[test]
    fn a_huge_unbroken_token_is_refused_not_rendered() {
        let started = std::time::Instant::now();
        let err = render(
            &format!("graph TD\n{}", "A".repeat(100_000)),
            MERMAID_RENDER_TIMEOUT,
        )
        .expect_err("a 100k-character line must not render");
        assert!(err.to_string().contains("too long"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// #1479: an unbroken token under the line cap but long enough to make the renderer's
    /// word splitter slow is refused up front too.
    #[test]
    fn a_long_unbroken_token_under_the_line_cap_is_refused_fast() {
        let started = std::time::Instant::now();
        let err = render(
            &format!("graph TD\nA[{}]\n", "A".repeat(3_000)),
            MERMAID_RENDER_TIMEOUT,
        )
        .expect_err("a 3000-character word must not render");
        assert!(err.to_string().contains("word too long"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// #1479: a long line made of many ordinary words (the other wrap shape) finishes well
    /// inside the budget instead of timing out.
    #[test]
    fn a_long_many_word_line_finishes_fast() {
        let words = "word ".repeat(700);
        let started = std::time::Instant::now();
        let result = render(
            &format!("graph TD\nA[{words}] --> B[End]\n"),
            MERMAID_RENDER_TIMEOUT,
        );
        if let Err(e) = &result {
            assert!(!e.to_string().contains("took longer"), "{e}");
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn render_slots_are_capped_and_released_on_drop() {
        static LIVE: AtomicUsize = AtomicUsize::new(0);
        let a = RenderSlot::acquire(&LIVE, 2).expect("first slot");
        let _b = RenderSlot::acquire(&LIVE, 2).expect("second slot");
        assert!(RenderSlot::acquire(&LIVE, 2).is_none(), "cap of 2 reached");
        drop(a);
        assert!(
            RenderSlot::acquire(&LIVE, 2).is_some(),
            "slot freed by drop"
        );
    }

    /// #1493: the hostile corpus's 200 nested subgraphs overflowed the render thread's stack
    /// (SIGABRT). Deep nesting is refused before parsing and takes the source fallback.
    #[test]
    fn deeply_nested_subgraphs_fall_back_instead_of_overflowing_the_stack() {
        let source = "graph TD\n".to_string()
            + &"subgraph s\n".repeat(200)
            + "A --> B\n"
            + &"end\n".repeat(200);
        let err = render(&source, MERMAID_RENDER_TIMEOUT).expect_err("200 levels must not render");
        assert!(err.to_string().contains("nested too deeply"), "{err}");
    }

    /// The actual cause of #1493: `subgraph s` inside `subgraph s` makes the renderer's subgraph
    /// parent links a cycle, and its layout recurses around it forever. Two levels, well under the
    /// nesting limit, is enough; it must take the source fallback, not abort the process.
    #[test]
    fn a_subgraph_nested_inside_its_own_id_falls_back_instead_of_overflowing_the_stack() {
        let source = "graph TD\nsubgraph s\nsubgraph s\nA --> B\nend\nend\n";
        let err =
            render(source, MERMAID_RENDER_TIMEOUT).expect_err("a parent cycle must not render");
        assert!(err.to_string().contains("inside itself"), "{err}");
    }

    #[test]
    fn subgraph_parent_cycles_are_found_however_they_are_spelled() {
        // Three levels, titled forms, and a cycle spread over separate blocks (a in b, then b in a).
        let titled = "graph TD\nsubgraph a\nsubgraph b\nsubgraph a[Title]\nend\nend\nend\n";
        assert!(subgraph_cycle(titled).is_some());
        let split =
            "graph TD\nsubgraph a\nsubgraph b\nA-->B\nend\nend\nsubgraph b\nsubgraph a\nend\nend\n";
        assert!(subgraph_cycle(split).is_some());
        // Legitimate shapes are untouched: nesting, siblings, repeated reopening, multi-word titles.
        let nested = "graph TD\nsubgraph a\nsubgraph b\nA-->B\nend\nend\nsubgraph c\nend\n";
        assert!(subgraph_cycle(nested).is_none());
        assert!(subgraph_cycle("graph TD\nsubgraph a\nend\nsubgraph a\nend\n").is_none());
        let titles = "graph TD\nsubgraph One two\nsubgraph One two\nend\nend\n";
        assert!(subgraph_cycle(titles).is_none());
    }

    /// #1493: `\0` is the graph library's root sentinel; a node with that id recurses forever in
    /// the layout. These are the shapes of the corpus inputs that aborted CI run r18cq.
    #[test]
    fn nul_node_ids_are_refused_before_the_renderer_sees_them() {
        for kind in ["stateDiagram-v2", "classDiagram", "erDiagram", "graph TD"] {
            let source = format!("{kind}\n{}", "\u{0}-->:: ::: {{ }} [ ( \n".repeat(50));
            let err = check_render_limits(&source).expect_err("NUL must be refused");
            assert!(err.to_string().contains("NUL"), "{err}");
        }
        assert!(check_render_limits("graph TD\nA --> B\n").is_ok());
    }

    #[test]
    fn nesting_measure_ignores_normal_diagrams() {
        assert!(max_nesting("graph TD\nsubgraph a\nsubgraph b\nA[(db)] --> B\nend\nend\n") <= 4);
        let parens = format!("graph TD\nA{}x{}\n", "(".repeat(100), ")".repeat(100));
        assert!(max_nesting(&parens) > MAX_MERMAID_NESTING);
        assert!(max_nesting(&format!("mindmap\n{}x\n", " ".repeat(200))) > MAX_MERMAID_NESTING);
        assert!(max_nesting("stateDiagram-v2\nstate a {\nstate b {\n}\n}\n") == 2);
    }

    #[test]
    fn an_oversized_source_is_refused() {
        let source = "graph TD\n".to_string() + &"A --> B\n".repeat(MAX_MERMAID_SOURCE_BYTES / 8);
        let err = render(&source, MERMAID_RENDER_TIMEOUT).expect_err("too large");
        assert!(err.to_string().contains("too large"), "{err}");
    }

    #[test]
    fn a_normal_diagram_still_renders() {
        let svg = render("graph TD\nA[Start] --> B[End]\n", MERMAID_RENDER_TIMEOUT)
            .expect("a small flowchart renders");
        assert!(String::from_utf8_lossy(&svg).contains("<svg"));
    }

    /// A render that outlasts its budget ends as an error at the budget, not a hang: a 300-node
    /// flowchart cannot lay out in a millisecond.
    #[test]
    fn a_render_over_its_budget_ends_as_an_error() {
        let mut source = String::from("graph TD\n");
        for i in 1..300 {
            source.push_str(&format!("N{} --> N{}\n", (i - 1) / 2, i));
        }
        let err = render(&source, Duration::from_millis(1)).expect_err("budget of 1 ms");
        assert!(err.to_string().contains("took longer"), "{err}");
    }
}

#[cfg(all(test, unix))]
mod subprocess_tests {
    use super::*;

    fn sh(script: &str) -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    fn run(script: &str, source: &str, timeout: Duration) -> anyhow::Result<Bytes> {
        static LIVE: AtomicUsize = AtomicUsize::new(0);
        let slot = RenderSlot::acquire(&LIVE, 1000).expect("slot");
        futures_lite::future::block_on(render_in_subprocess(
            sh(script),
            source.to_string(),
            timeout,
            slot,
        ))
    }

    #[test]
    fn svg_comes_back_from_the_child() {
        let svg = run("cat", "<svg>ok</svg>", Duration::from_secs(5)).expect("renders");
        assert_eq!(&svg[..], b"<svg>ok</svg>");
    }

    /// #1557: the child dying by signal (what a stack overflow does) fails that one diagram.
    #[test]
    fn a_child_that_aborts_is_an_error_not_a_crash() {
        let err = run("kill -SEGV $$", "x", Duration::from_secs(5)).expect_err("crash");
        assert!(err.to_string().contains("failed"), "{err}");
        let err = run("kill -ABRT $$", "x", Duration::from_secs(5)).expect_err("abort");
        assert!(err.to_string().contains("failed"), "{err}");
    }

    #[test]
    fn a_failing_or_silent_child_is_an_error() {
        assert!(run("echo partial; exit 1", "x", Duration::from_secs(5)).is_err());
        assert!(run("exit 0", "x", Duration::from_secs(5)).is_err(), "empty output");
    }

    #[test]
    fn a_hung_child_is_killed_at_the_deadline() {
        let started = std::time::Instant::now();
        let err = run("sleep 60", "x", Duration::from_millis(200)).expect_err("timeout");
        assert!(err.to_string().contains("took longer"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_child_flooding_stdout_is_cut_off() {
        let err = run("yes", "x", Duration::from_secs(20)).expect_err("oversize");
        assert!(err.to_string().contains("too large"), "{err}");
    }

    #[test]
    fn a_large_source_does_not_deadlock_a_child_that_ignores_stdin() {
        let source = "A".repeat(MAX_MERMAID_SOURCE_BYTES);
        let svg = run("echo '<svg/>'", &source, Duration::from_secs(5)).expect("renders");
        assert!(String::from_utf8_lossy(&svg).contains("<svg/>"));
    }

    #[test]
    fn a_missing_executable_is_an_error() {
        static LIVE: AtomicUsize = AtomicUsize::new(0);
        let slot = RenderSlot::acquire(&LIVE, 1000).expect("slot");
        let command = std::process::Command::new("/nonexistent/phosphor");
        let err = futures_lite::future::block_on(render_in_subprocess(
            command,
            "x".into(),
            Duration::from_secs(5),
            slot,
        ))
        .expect_err("spawn fails");
        assert!(err.to_string().contains("could not start"), "{err}");
    }
}
