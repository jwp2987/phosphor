//! The TUI runtime, additive behind the `tui` feature: the alternate-screen
//! lifecycle and the draw + event loop that drives a [`TuiView`] through the
//! shared [`App`].
//!
//! Placement: the GUI has no in-core analog of this module — its runtime is
//! the platform event loop in the `warpui` crate — so the TUI runtime stands
//! alone as an additive top-level module rather than a backend submodule of an
//! existing one.
//!
//! [`TuiRuntime`] mirrors the GUI's invalidate→redraw flow. On
//! [`enter`](TuiRuntime::enter) it puts the host terminal into raw mode + the
//! alternate screen (restored on drop) and subscribes to the window's
//! invalidation signal; [`run_until`](TuiRuntime::run_until) then repeatedly
//! redraws when dirty and polls crossterm for input, converting each event with
//! [`crossterm_event_to_tui_event`] and dispatching it — first through the
//! shared keymap (the focused view's responder chain, exactly like the GUI
//! window event path), then through the rendered element tree.
//!
//! The host terminal is abstracted behind [`TuiTerminal`] so the loop and the
//! frame renderer can be exercised headlessly against an in-memory writer
//! without a real tty. The concrete [`CrosstermTerminal`] is the production
//! implementation.

use std::cell::{Cell, RefCell};
use std::io::{self, Stdout, Write, stdout};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use instant::Instant;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event as CrosstermEvent, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};

use crate::r#async::executor::ForegroundTask;
use crate::r#async::{Timer, block_on};
use crate::elements::tui::{TuiEvent, TuiEventContext, TuiPoint, TuiRect, TuiSize};
use crate::event::ModifiersState;
use crate::platform::TerminationMode;
use crate::presenter::tui::TuiPresenter;
use crate::report_error::report_error;
use crate::{App, AppContext, TuiView, ViewHandle, WindowId};

mod event_conversion;
mod renderer;
mod terminal_probe;

pub use event_conversion::crossterm_event_to_tui_event;
use event_conversion::{ClickTracker, ShiftKeyTracker, ShiftRestoration};
pub use renderer::TuiFrameRenderer;
pub use terminal_probe::{
    BackgroundLuminance, ProbedRgb, ProbedTerminalColors, TuiProbe, background_luminance,
    probe_terminal_background, probe_terminal_colors, read_terminal_background_reply,
    write_terminal_background_query,
};

/// The host terminal the runtime draws to and reads input from. Abstracted so
/// the draw + event loop is testable against an in-memory target.
pub trait TuiTerminal {
    /// The current terminal size in cells (each axis at least 1).
    fn size(&self) -> io::Result<TuiSize>;

    /// Blocks up to `timeout` for the next input event, returning `None` on
    /// timeout.
    fn poll_event(&mut self, timeout: Duration) -> io::Result<Option<CrosstermEvent>>;

    /// The writer the renderer flushes frames to.
    fn writer(&mut self) -> &mut dyn Write;
}

/// Controls whether a TUI driver constrains keyboard focus to views embedded in
/// the most recently presented frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TuiFocusPolicy {
    /// Preserve app-managed focus even when its view is not currently presented.
    #[default]
    Unrestricted,
    /// Return focus to the root when the focused view is outside the presented tree.
    PresentedTree,
}

/// Why [`spawn_tui_driver`] could not bring a TUI session up.
///
/// The split exists so the caller can tell "the user's terminal went away
/// before we ever drew a frame" (nothing actionable; exit quietly) from "the
/// driver is broken" (worth reporting and worth a non-zero exit).
#[derive(Debug, thiserror::Error)]
pub enum TuiDriverStartupError {
    #[error("host terminal disconnected while starting the TUI driver: {0}")]
    TerminalDisconnected(#[source] io::Error),
    #[error("failed to start the TUI driver: {0}")]
    Unexpected(#[source] io::Error),
}

impl From<io::Error> for TuiDriverStartupError {
    fn from(error: io::Error) -> Self {
        if is_terminal_disconnect(&error) {
            Self::TerminalDisconnected(error)
        } else {
            Self::Unexpected(error)
        }
    }
}

/// Which half of the driver's terminal I/O failed, used only to label the
/// error the failure path logs or reports.
#[derive(Clone, Copy)]
enum TuiDriverIoOperation {
    DrawFrame,
    ReadEvent,
}

impl TuiDriverIoOperation {
    fn error_context(self) -> &'static str {
        match self {
            Self::DrawFrame => "failed to draw a TUI frame",
            Self::ReadEvent => "failed to read a terminal event",
        }
    }
}

/// What the input-reader thread forwards to the foreground runtime: either a
/// terminal event, or the I/O failure that ended the read loop. Reader
/// failures travel this channel rather than being logged on the reader thread
/// so the foreground half can act on them (cancel repaints, terminate).
enum TuiDriverEvent {
    Terminal(CrosstermEvent),
    InputFailed(io::Error),
}

/// Whether `error` means the host terminal is gone, as opposed to a transient
/// or genuinely unexpected I/O failure.
///
/// A disconnected terminal is not actionable — the process should exit, not
/// report — so this is the predicate that decides between the quiet and the
/// reported termination paths.
fn is_terminal_disconnect(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
    ) {
        return true;
    }

    // A pty whose master end closed reports `EIO` on both reads and writes, and
    // a controlling terminal that no longer exists reports `ENXIO`. Neither maps
    // onto a distinct `io::ErrorKind`, so they are matched by errno.
    #[cfg(unix)]
    if matches!(error.raw_os_error(), Some(libc::EIO | libc::ENXIO)) {
        return true;
    }

    // `ERROR_PIPE_NOT_CONNECTED` (233), which Windows reports for a console handle
    // whose other end has closed. Note this is NOT `ERROR_BROKEN_PIPE` (109) --
    // Rust already maps that one to `io::ErrorKind::BrokenPipe`, caught above.
    #[cfg(windows)]
    if error.raw_os_error() == Some(233) {
        return true;
    }

    false
}

/// Terminates the TUI session after a terminal I/O failure, exactly once.
///
/// `failed` is the shared latch that makes this idempotent: the first caller
/// wins and every later one returns immediately, so a failure that surfaces on
/// both the draw and the read path reports once rather than twice. Latching it
/// also stops [`draw_and_schedule_repaint`] from drawing again, and taking the
/// timer slot cancels any repaint already scheduled — without both, every
/// subsequent invalidation would attempt another frame against a terminal that
/// no longer exists.
fn fail_tui_driver(
    error: io::Error,
    operation: TuiDriverIoOperation,
    failed: &Rc<Cell<bool>>,
    repaint_timer: &Rc<RefCell<Option<ForegroundTask>>>,
    ctx: &mut AppContext,
) {
    if failed.replace(true) {
        return;
    }
    repaint_timer.borrow_mut().take();

    let error_context = operation.error_context();
    if is_terminal_disconnect(&error) {
        // Expected and not actionable: log it once and exit without turning a
        // vanished terminal into a non-zero exit status.
        log::error!("{error_context}: {error}");
        ctx.terminate_app(TerminationMode::ForceTerminate, None);
        return;
    }

    let error = anyhow::Error::new(error).context(error_context);
    report_error!(&error);
    ctx.terminate_app(TerminationMode::ForceTerminate, Some(Err(error)));
}

/// The rendering half of the TUI: owns the presenter, renderer, and host
/// terminal for one window and paints that window's view tree. Kept separate
/// from input dispatch so the invalidation-driven redraw (which paints inside
/// `flush_effects`) never collides with a borrow the input path holds.
struct TuiScreen<T, R: TuiTerminal> {
    window_id: WindowId,
    root_view: ViewHandle<T>,
    presenter: TuiPresenter,
    renderer: TuiFrameRenderer,
    terminal: R,
    focus_policy: TuiFocusPolicy,
    /// Synthesizes multi-click counts for left mouse presses, which crossterm
    /// does not report.
    click_tracker: ClickTracker,
    /// Restores Shift after crossterm substitutes a layout-produced alternate
    /// character and removes the modifier bit.
    shift_key_tracker: ShiftKeyTracker,
    /// The pointer position from the most recent positional event, replayed as
    /// a synthetic `MouseMoved` after each draw so hover state tracks elements
    /// that move under a stationary pointer.
    last_mouse_position: Option<TuiPoint>,
    /// Shared with the input reader thread so a focus-triggered background
    /// probe's OSC query cannot land in the middle of a frame write. Held
    /// only for the duration of the frame flush below.
    stdout_write_lock: Arc<Mutex<()>>,
}

impl<T: TuiView, R: TuiTerminal> TuiScreen<T, R> {
    fn new(
        window_id: WindowId,
        root_view: ViewHandle<T>,
        terminal: R,
        stdout_write_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            window_id,
            root_view,
            presenter: TuiPresenter::new(),
            renderer: TuiFrameRenderer::new(),
            terminal,
            focus_policy: TuiFocusPolicy::default(),
            click_tracker: ClickTracker::default(),
            shift_key_tracker: ShiftKeyTracker::default(),
            last_mouse_position: None,
            stdout_write_lock,
        }
    }

    fn with_focus_policy(mut self, focus_policy: TuiFocusPolicy) -> Self {
        self.focus_policy = focus_policy;
        self
    }

    fn size(&self) -> io::Result<TuiSize> {
        self.terminal.size()
    }

    /// Lays out and paints the root view through the presenter, then flushes the
    /// final frame to the terminal. Draining this window's invalidations keeps
    /// the manual + autotracking sets from accumulating (the frame is repainted
    /// in full regardless). After each presentation, the last pointer position
    /// is replayed as a synthetic `MouseMoved`; resulting invalidations rebuild
    /// the frame within this call, capped at three iterations like the GUI. Only
    /// the final reconciled frame is flushed, matching the GUI's presentation.
    ///
    /// Returns the final frame's earliest requested repaint deadline so the
    /// caller can schedule a timed redraw.
    fn draw(&mut self, ctx: &mut AppContext) -> io::Result<Option<Instant>> {
        let size = self.terminal.size()?;
        let area = TuiRect::new(0, 0, size.width, size.height);

        // Mirrors the GUI's `build_scene` loop: pointer replay can invalidate
        // hover-dependent layout, requiring another presentation and replay.
        // The first iteration always presents; later ones only run if the
        // replay invalidated something. Cap at three total presentations so a
        // hover/layout feedback loop cannot hang the redraw.
        let mut frame = None;
        for _ in 0..3 {
            let invalidation = ctx.take_all_invalidations_for_window(self.window_id);
            if frame.is_some() && invalidation.updated.is_empty() && !invalidation.redraw_requested
            {
                break;
            }
            self.presenter
                .invalidate(&invalidation, ctx, self.window_id);
            frame = Some(self.presenter.present(ctx, &self.root_view, area));
            if self.focus_policy == TuiFocusPolicy::PresentedTree {
                self.repair_focus_outside_presented_tree(ctx);
            }
            self.replay_mouse_position(ctx);
        }
        let frame = frame.expect("loop always presents at least once");

        // Hold the lock through the complete frame write so a reader-thread
        // OSC query cannot split the renderer's escape sequence.
        let _frame_write_guard = self
            .stdout_write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut writer = self.terminal.writer();
        self.renderer
            .draw(&mut writer, &frame.buffer, frame.cursor)?;
        Ok(frame.repaint_at)
    }

    /// Hands focus back to the root when it is owned by a view that painted
    /// nothing in the frame just presented.
    ///
    /// Only runs under [`TuiFocusPolicy::PresentedTree`]. A retained but hidden
    /// view (a background session, say) can otherwise stay the window's focus
    /// owner indefinitely, swallowing keystrokes the user is aiming at what is
    /// actually on screen. The root is the safe destination because it can
    /// re-delegate to whatever it currently shows.
    fn repair_focus_outside_presented_tree(&mut self, ctx: &mut AppContext) {
        let Some(focused_view_id) = ctx.focused_view_id(self.window_id) else {
            return;
        };
        if !self.presenter.presented_views.contains(&focused_view_id) {
            self.root_view.update(ctx, |_, ctx| ctx.focus_self());
        }
    }

    /// Redispatches the last known pointer position as a synthetic
    /// `MouseMoved` through the freshly rendered tree, so hover state tracks
    /// elements that moved under a stationary pointer (e.g. a collapsible
    /// expanding and pushing its header out from under the mouse). A state
    /// change invalidates the notified views, which `draw`'s loop picks up to
    /// rebuild the frame within the same call.
    fn replay_mouse_position(&mut self, ctx: &mut AppContext) {
        let Some(position) = self.last_mouse_position else {
            return;
        };
        let event = TuiEvent::MouseMoved {
            position,
            modifiers: ModifiersState::default(),
            is_synthetic: true,
        };
        self.dispatch_event(ctx, &event);
    }

    /// Converts a raw crossterm event into the TUI vocabulary, restoring Shift
    /// from modifier lifecycle events and synthesizing mouse multi-click counts.
    /// Returns `None` for events with no TUI equivalent.
    fn convert_event(&mut self, mut event: CrosstermEvent) -> Option<TuiEvent> {
        let restoration = self.shift_key_tracker.update(&mut event);
        let mut tui_event = crossterm_event_to_tui_event(event)?;
        if restoration == ShiftRestoration::Symbol
            && let TuiEvent::KeyDown {
                keystroke, details, ..
            } = &mut tui_event
        {
            // Crossterm replaced the symbol's base key with the character the
            // layout produced, which already encodes Shift. Keeping the bit
            // would make one chord need two spellings: `ctrl-shift-!` where the
            // layout shifts the symbol and `ctrl-!` where it does not. The base
            // key is also unrecoverable from the produced character.
            keystroke.shift = false;
            details.key_without_modifiers = None;
        }
        self.click_tracker.annotate(&mut tui_event, Instant::now());
        Some(tui_event)
    }

    /// Dispatches a converted input event into the cached element tree, returning
    /// whether it was handled. Uses the last rendered element tree cached by the
    /// presenter (the same tree that was painted), with a `TuiLayoutContext` so
    /// `TuiChildView` can resolve its child from `rendered_views`.
    fn dispatch_event(&mut self, ctx: &mut AppContext, event: &TuiEvent) -> bool {
        if let Some(position) = event.position() {
            self.last_mouse_position = Some(position);
        }

        // Keymap pass (GUI parity): offer a keystroke to the focused view's
        // responder chain first, exactly like the GUI window event path.
        // `ModifierKeyChanged` bypasses this pass and continues to element
        // dispatch because keymaps represent press-driven keystrokes.
        if let Some((keystroke, is_composing)) = event.key_down() {
            let responder_chain = ctx.get_responder_chain(self.window_id);
            match ctx.dispatch_keystroke(self.window_id, &responder_chain, keystroke, is_composing)
            {
                Ok(true) => return true,
                Ok(false) => {}
                Err(error) => {
                    log::error!("{:#}", error.context("error dispatching keystroke"))
                }
            }
        }

        // Element-tree pass: walk the last rendered+laid-out element tree.
        // Access the two presenter fields directly so Rust sees disjoint borrows.
        let (Some(element), Some(scene)) = (
            self.presenter.last_element.as_mut(),
            self.presenter.last_scene.clone(),
        ) else {
            return false; // no draw has happened yet
        };
        let root_view_id = self.root_view.id();
        let mut event_ctx = TuiEventContext::new(scene, &mut self.presenter.rendered_views);
        event_ctx.set_origin_view(Some(root_view_id));
        let handled = element.dispatch_event(event, &mut event_ctx, ctx);

        let notified = event_ctx.take_notified();
        for view_id in notified {
            ctx.notify_view_observers(self.window_id, view_id);
        }

        for action in event_ctx.take_typed_actions() {
            // Dispatch through the shared responder chain (the origin view's
            // ancestors), so an action raised inside an embedded child view
            // bubbles to ancestor handlers.
            ctx.dispatch_typed_action_for_view(
                self.window_id,
                action.origin_view_id,
                action.action.as_ref(),
            );
        }
        handled
    }
}

/// Crossterm treats the first primary-device-attributes response it reads as a
/// definitive negative result. Retry that result once because an older queued
/// response can precede the keyboard-flags response from the current query.
fn probe_keyboard_enhancement_support(mut probe: impl FnMut() -> io::Result<bool>) -> bool {
    match probe() {
        Ok(true) => true,
        Ok(false) => matches!(probe(), Ok(true)),
        Err(_) => false,
    }
}

/// A **development/test harness** that drives a single [`TuiView`] window with a
/// *blocking* loop ([`run_until`](Self::run_until)): it redraws when dirty and
/// polls the terminal for input. It backs the interactive `tui_*` examples and
/// the runtime unit tests; it is **not** used by the shipping app, which drives
/// the TUI with the non-blocking, invalidation-driven [`spawn_tui_driver`]
/// instead. It is intentionally not `#[cfg(test)]`-gated because the examples
/// (which compile outside `cfg(test)`) depend on it.
pub struct TuiRuntime<T, R = CrosstermTerminal>
where
    R: TuiTerminal,
{
    screen: TuiScreen<T, R>,
    dirty: Rc<Cell<bool>>,
    last_size: Option<TuiSize>,
    /// The earliest element-requested repaint deadline from the last draw; the
    /// loop marks itself dirty once it passes.
    pending_repaint: Option<Instant>,
    /// Whether the host terminal currently has focus. Timed animation repaints
    /// may be suspended while false, but ordinary invalidations may still draw.
    focused: bool,
    /// Whether timed repaints should be suspended while the host terminal is
    /// unfocused. Disabled by default so focus-based suspension is opt-in.
    freeze_repaints_when_unfocused: bool,
    /// Restores the terminal when the runtime is dropped (the `enter` path).
    /// Held only for its `Drop`.
    _terminal_guard: Option<TuiTerminalGuard>,
}

impl<T> TuiRuntime<T, CrosstermTerminal>
where
    T: TuiView,
{
    /// Enters the alternate screen + raw mode and prepares to drive `root_view`.
    /// The terminal is restored when the returned runtime is dropped.
    pub fn enter(app: &App, window_id: WindowId, root_view: ViewHandle<T>) -> io::Result<Self> {
        let guard = TuiTerminalGuard::enter(false)?;
        let mut runtime = Self::with_terminal(app, window_id, root_view, CrosstermTerminal::new());
        runtime._terminal_guard = Some(guard);
        Ok(runtime)
    }
}

impl<T, R> TuiRuntime<T, R>
where
    T: TuiView,
    R: TuiTerminal,
{
    /// Builds a runtime over an arbitrary [`TuiTerminal`]. Subscribes to the
    /// window's invalidation signal so a `notify` schedules a redraw, and marks
    /// the runtime dirty so the first loop iteration paints.
    pub fn with_terminal(
        app: &App,
        window_id: WindowId,
        root_view: ViewHandle<T>,
        terminal: R,
    ) -> Self {
        let dirty = Rc::new(Cell::new(true));
        let dirty_for_callback = dirty.clone();
        app.on_window_invalidated(window_id, move |_, _| dirty_for_callback.set(true));
        Self {
            screen: TuiScreen::new(window_id, root_view, terminal, Arc::new(Mutex::new(()))),
            dirty,
            last_size: None,
            pending_repaint: None,
            focused: true,
            freeze_repaints_when_unfocused: false,
            _terminal_guard: None,
        }
    }

    /// Runs the draw + input loop until `should_quit` returns `true`, redrawing
    /// when invalidated (or resized) and dispatching converted input events.
    pub fn run_until(
        &mut self,
        app: &mut App,
        mut should_quit: impl FnMut(&App) -> bool,
    ) -> io::Result<()> {
        while !should_quit(app) {
            self.draw_if_dirty(app)?;
            // 250 ms is a standard event-poll heartbeat: short enough to feel
            // responsive to resize, long enough to avoid busy-waiting. A timeout
            // is not an error — `poll_event` returns `Ok(None)`, making the loop
            // iteration a no-op before the next draw-if-dirty check. A pending
            // element-requested repaint shortens the wait so the redraw lands
            // on time.
            let heartbeat = Duration::from_millis(250);
            let timeout = match self.pending_repaint {
                Some(deadline) => {
                    let now = Instant::now();
                    if deadline > now {
                        (deadline - now).min(heartbeat)
                    } else {
                        Duration::ZERO
                    }
                }
                None => heartbeat,
            };
            self.poll_and_dispatch(app, timeout)?;
        }
        Ok(())
    }

    /// The terminal this runtime draws to. Primarily useful for inspecting an
    /// in-memory terminal's captured output in tests.
    pub fn terminal(&self) -> &R {
        &self.screen.terminal
    }

    fn draw_if_dirty(&mut self, app: &mut App) -> io::Result<()> {
        let size = self.screen.size()?;
        if self.last_size != Some(size) {
            self.dirty.set(true);
        }
        if should_schedule_repaints(self.focused, self.freeze_repaints_when_unfocused)
            && self
                .pending_repaint
                .is_some_and(|deadline| deadline <= Instant::now())
        {
            self.pending_repaint = None;
            self.dirty.set(true);
        }
        if !self.dirty.replace(false) {
            return Ok(());
        }
        let screen = &mut self.screen;
        let requested_repaint = app.update(|ctx| screen.draw(ctx))?;
        self.pending_repaint =
            if should_schedule_repaints(self.focused, self.freeze_repaints_when_unfocused) {
                requested_repaint
            } else {
                None
            };
        self.last_size = Some(size);
        Ok(())
    }

    fn poll_and_dispatch(&mut self, app: &mut App, timeout: Duration) -> io::Result<()> {
        let Some(event) = self.screen.terminal.poll_event(timeout)? else {
            return Ok(());
        };

        match event {
            CrosstermEvent::Resize(_, _) => self.dirty.set(true),
            event => {
                match &event {
                    CrosstermEvent::FocusGained => {
                        self.focused = true;
                        self.dirty.set(true);
                    }
                    CrosstermEvent::FocusLost => {
                        self.focused = false;
                        if self.freeze_repaints_when_unfocused {
                            self.pending_repaint = None;
                        }
                    }
                    _ => {}
                }
                let screen = &mut self.screen;
                if let Some(tui_event) = screen.convert_event(event) {
                    let handled = app.update(|ctx| screen.dispatch_event(ctx, &tui_event));
                    if handled {
                        self.dirty.set(true);
                    }
                }
            }
        }
        Ok(())
    }
}

/// The production [`TuiTerminal`]: writes to the process stdout and reports the
/// terminal size. Raw mode + the alternate screen are managed separately by a
/// [`TuiTerminalGuard`], so the terminal-mode lifetime can be detached from the
/// writer (the headless driver keeps the guard in its [`TuiDriverHandle`] for a
/// deterministic restore, independent of when the async draw loop is dropped).
pub struct CrosstermTerminal {
    stdout: Stdout,
}

impl CrosstermTerminal {
    /// Builds a terminal over the process stdout. Does not change terminal
    /// modes; pair it with a [`TuiTerminalGuard`] to enter raw mode + the
    /// alternate screen.
    pub fn new() -> Self {
        Self { stdout: stdout() }
    }
}

impl Default for CrosstermTerminal {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiTerminal for CrosstermTerminal {
    fn size(&self) -> io::Result<TuiSize> {
        let (width, height) = terminal::size()?;
        Ok(TuiSize::new(width.max(1), height.max(1)))
    }

    fn poll_event(&mut self, timeout: Duration) -> io::Result<Option<CrosstermEvent>> {
        if event::poll(timeout)? {
            Ok(Some(event::read()?))
        } else {
            Ok(None)
        }
    }

    fn writer(&mut self) -> &mut dyn Write {
        &mut self.stdout
    }
}

/// Owns the terminal's raw mode + alternate screen for as long as it is alive,
/// restoring the terminal on drop. Held by [`TuiRuntime::enter`] (so the
/// `run_until` path restores when the runtime drops) or by a [`TuiDriverHandle`]
/// (so a headless app restores deterministically when its session is dropped).
pub struct TuiTerminalGuard {
    _guard: RawModeGuard<CrosstermModeControl>,
    keyboard_enhancement_supported: bool,
    modifier_key_lifecycle_enabled: bool,
}

impl TuiTerminalGuard {
    /// Enables raw mode and switches to the alternate screen, restoring both
    /// when the guard is dropped.
    pub fn enter(report_modifier_key_lifecycle: bool) -> io::Result<Self> {
        let keyboard_enhancement_supported =
            probe_keyboard_enhancement_support(terminal::supports_keyboard_enhancement);
        Ok(Self {
            _guard: RawModeGuard::enter(CrosstermModeControl {
                keyboard_enhancement_supported,
                report_modifier_key_lifecycle,
            })?,
            keyboard_enhancement_supported,
            modifier_key_lifecycle_enabled: keyboard_enhancement_supported
                && report_modifier_key_lifecycle,
        })
    }

    /// Whether the host terminal supports the Kitty keyboard-enhancement protocol.
    pub fn keyboard_enhancement_supported(&self) -> bool {
        self.keyboard_enhancement_supported
    }

    /// Whether standalone modifier press/release reporting is active.
    pub fn modifier_key_lifecycle_enabled(&self) -> bool {
        self.modifier_key_lifecycle_enabled
    }
}

/// Keeps a headless TUI session alive. Store it for the lifetime of the app
/// (e.g. in a singleton model) so the session lives as long as the app does;
/// dropping it tears the session down. Its `Drop` implementation first stops
/// the reader thread from starting another probe and waits for any in-flight
/// probe I/O (see the `Drop` impl below), then fields drop in declaration
/// order, which is also the teardown order:
/// - `_task`: the input-dispatch loop. It is an [`async_task::Task`], so
///   dropping it *cancels* the future (we intentionally don't `detach()`),
///   which in turn drops the channel receiver feeding it.
/// - `_reader`: the blocking input-reader thread. Dropping a `JoinHandle`
///   detaches rather than joins, so this doesn't stop the thread directly; the
///   thread exits on its own once the receiver above is gone (its next `send`
///   fails) or when the process exits. The handle is held so the session owns
///   the thread it spawned.
/// - `_guard`: restores raw mode + the alternate screen on drop.
pub struct TuiDriverHandle {
    _task: ForegroundTask,
    /// The pending element-requested repaint timer, if any (see
    /// [`draw_and_schedule_repaint`]). Dropping it cancels the timer.
    repaint_timer: Rc<RefCell<Option<ForegroundTask>>>,
    focused: Rc<Cell<bool>>,
    freeze_repaints_when_unfocused: Rc<Cell<bool>>,
    _reader: thread::JoinHandle<()>,
    /// Tells the reader thread to stop reading further events/probes at its
    /// next loop boundary, checked before teardown restores the terminal.
    reader_shutdown: Arc<AtomicBool>,
    /// Held by the reader thread only while a focus-triggered probe's query
    /// and reply are in flight; teardown waits on it so the terminal is never
    /// restored mid-probe.
    probe_lifecycle_lock: Arc<Mutex<()>>,
    _guard: TuiTerminalGuard,
}

impl TuiDriverHandle {
    /// Whether the host terminal supports the Kitty keyboard-enhancement protocol.
    pub fn keyboard_enhancement_supported(&self) -> bool {
        self._guard.keyboard_enhancement_supported()
    }

    /// Whether standalone modifier press/release reporting is active.
    pub fn modifier_key_lifecycle_enabled(&self) -> bool {
        self._guard.modifier_key_lifecycle_enabled()
    }

    /// Controls whether timed repaints stop while the host terminal is unfocused.
    pub fn set_freeze_repaints_when_unfocused(&mut self, freeze: bool) {
        self.freeze_repaints_when_unfocused.set(freeze);
        if freeze && !self.focused.get() {
            self.repaint_timer.borrow_mut().take();
        }
    }
}

impl Drop for TuiDriverHandle {
    fn drop(&mut self) {
        self.reader_shutdown.store(true, Ordering::Release);
        // Blocks until any probe I/O the reader thread already started
        // completes, so `_guard`'s restore below never races a live query.
        let _probe_lifecycle_guard = self
            .probe_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}

/// Starts a headless TUI session that draws `root_view` and feeds terminal input
/// back into the shared core.
///
/// This is the headless counterpart to [`TuiRuntime::run_until`]: instead of
/// owning the main thread with a blocking loop, it cooperates with a real app's
/// event loop. Rendering is **invalidation-driven**: an `on_window_invalidated`
/// callback repaints the window, so any `notify()` (an input handler, a model or
/// async update, or the resize handling below) schedules a redraw via the core's
/// normal `flush_effects` pass. Input is read on a background thread and only
/// *dispatched* on the foreground executor. Every event — including `Ctrl-C`,
/// which raw mode delivers as a key event rather than a `SIGINT` — flows
/// through the keymap + element-tree dispatch, so quitting is owned by the
/// app's views (e.g. a double-`Ctrl-C` exit handler), not the driver.
///
/// The returned [`TuiDriverHandle`] owns the session: keep it alive for as long
/// as the session should run, and drop it (e.g. on app teardown) to restore the
/// terminal.
///
/// `probe`, if given, registers a focus-triggered background re-probe on the
/// input reader thread — the thread's sole ownership of stdin lets it write
/// the OSC query and read the reply without racing normal input (see
/// `terminal_probe::TuiProbe`).
pub fn spawn_tui_driver<T: TuiView>(
    ctx: &mut AppContext,
    window_id: WindowId,
    root_view: ViewHandle<T>,
    focus_policy: TuiFocusPolicy,
    probe: Option<TuiProbe>,
    report_modifier_key_lifecycle: bool,
    freeze_repaints_when_unfocused: bool,
) -> Result<TuiDriverHandle, TuiDriverStartupError> {
    let guard = TuiTerminalGuard::enter(report_modifier_key_lifecycle)?;

    // Shared with the reader thread so a focus-triggered probe's OSC query
    // can never land in the middle of a frame write (see `TuiScreen::draw`).
    let stdout_write_lock = Arc::new(Mutex::new(()));

    // The presenter + renderer + terminal live behind an `Rc<RefCell<_>>` owned
    // by the invalidation callback. The input path never borrows it, so painting
    // inside `flush_effects` can't collide with dispatch.
    let screen = Rc::new(RefCell::new(
        TuiScreen::new(
            window_id,
            root_view,
            CrosstermTerminal::new(),
            stdout_write_lock.clone(),
        )
        .with_focus_policy(focus_policy),
    ));

    // Repaint scheduling: at most one pending timer, held in this slot. Every
    // draw reports the earliest element-requested repaint deadline for the
    // whole frame, so each draw replaces (cancelling) the previous timer with
    // one for its own deadline — or clears it when nothing is animating.
    let repaint_timer: Rc<RefCell<Option<ForegroundTask>>> = Rc::default();
    let focused = Rc::new(Cell::new(true));
    let freeze_repaints_when_unfocused = Rc::new(Cell::new(freeze_repaints_when_unfocused));
    // Latched by the first terminal I/O failure. Everything downstream reads it
    // to stop drawing; see `fail_tui_driver`.
    let failed = Rc::new(Cell::new(false));

    // Redraw whenever the window is invalidated. `update_windows` invokes this at
    // the end of every `flush_effects`, so any `notify()` repaints. (The callback
    // is removed from the registry while it runs, so a draw that itself
    // invalidates can't re-enter it.)
    {
        let screen = screen.clone();
        let repaint_timer = repaint_timer.clone();
        let focused = focused.clone();
        let freeze_repaints_when_unfocused = freeze_repaints_when_unfocused.clone();
        let failed = failed.clone();
        ctx.on_window_invalidated(window_id, move |_, ctx| {
            if let Err(error) = draw_and_schedule_repaint(
                &screen,
                &repaint_timer,
                &focused,
                &freeze_repaints_when_unfocused,
                &failed,
                ctx,
            ) {
                // A draw failure here used to be logged (throttled) and otherwise
                // ignored, so every later invalidation drew into a terminal that
                // was already gone. Terminating latches the failure instead, which
                // both silences the flood and ends the session.
                fail_tui_driver(
                    error,
                    TuiDriverIoOperation::DrawFrame,
                    &failed,
                    &repaint_timer,
                    ctx,
                );
            }
        });
    }

    // Paint the first frame now, which also consumes the window's initial
    // invalidation so the callback doesn't redundantly repaint it on the next
    // flush. This runs during setup (unlike the invalidation callback above,
    // which is in the event loop and can only log), so a failure is propagated:
    // returning `Err` here drops `guard` (restoring the terminal) and lets the
    // caller surface the error, rather than leaving a live raw-mode session with
    // no usable frame.
    if let Err(error) = draw_and_schedule_repaint(
        &screen,
        &repaint_timer,
        &focused,
        &freeze_repaints_when_unfocused,
        &failed,
        ctx,
    ) {
        failed.set(true);
        return Err(error.into());
    }

    let weak_app = ctx.weak_app();
    let (sender, receiver) = async_channel::unbounded::<TuiDriverEvent>();

    let reader_shutdown = Arc::new(AtomicBool::new(false));
    let probe_lifecycle_lock = Arc::new(Mutex::new(()));

    // Blocking terminal reads run off the main thread and are forwarded to the
    // foreground executor through the channel, so the main thread's event loop
    // is never blocked waiting for input. The reader also performs a
    // background-color probe when the terminal regains focus, since it is the
    // sole owner of stdin and can keep the reply out of the normal crossterm
    // event stream.
    let reader = thread::Builder::new()
        .name("warp-tui-input".to_owned())
        .spawn({
            let reader_shutdown = reader_shutdown.clone();
            let probe_lifecycle_lock = probe_lifecycle_lock.clone();
            move || {
                run_tui_input_reader(
                    sender,
                    probe,
                    stdout_write_lock,
                    reader_shutdown,
                    probe_lifecycle_lock,
                )
            }
        })?;

    let dispatch_screen = screen.clone();
    let dispatch_repaint_timer = repaint_timer.clone();
    let dispatch_focused = focused.clone();
    let dispatch_freeze_repaints_when_unfocused = freeze_repaints_when_unfocused.clone();
    let dispatch_failed = failed.clone();
    let task = ctx.foreground_executor().spawn(async move {
        while let Ok(driver_event) = receiver.recv().await {
            let Some(mut app) = weak_app.upgrade() else {
                break;
            };
            let screen = dispatch_screen.clone();
            let repaint_timer = dispatch_repaint_timer.clone();
            let focused = dispatch_focused.clone();
            let freeze_repaints_when_unfocused = dispatch_freeze_repaints_when_unfocused.clone();
            let failed = dispatch_failed.clone();
            let event = match driver_event {
                TuiDriverEvent::Terminal(event) => event,
                // The reader thread has already stopped; ending the session is
                // the foreground half's job, and there is nothing left to read.
                TuiDriverEvent::InputFailed(error) => {
                    app.update(move |ctx| {
                        fail_tui_driver(
                            error,
                            TuiDriverIoOperation::ReadEvent,
                            &failed,
                            &repaint_timer,
                            ctx,
                        );
                    });
                    break;
                }
            };
            // Dispatch reuses the shared screen's cached element tree (so embedded
            // child views resolve their elements). Edits queue effects that flush
            // when this `update` returns — firing the invalidation callback to
            // repaint — so the screen is never borrowed re-entrantly.
            app.update(move |ctx| match event {
                CrosstermEvent::Resize(_, _) => ctx.invalidate_all_views(),
                event => {
                    match &event {
                        CrosstermEvent::FocusGained => {
                            focused.set(true);
                            ctx.invalidate_all_views();
                        }
                        CrosstermEvent::FocusLost => {
                            focused.set(false);
                            if freeze_repaints_when_unfocused.get() {
                                repaint_timer.borrow_mut().take();
                            }
                        }
                        _ => {}
                    }
                    let mut screen = screen.borrow_mut();
                    if let Some(tui_event) = screen.convert_event(event) {
                        screen.dispatch_event(ctx, &tui_event);
                    }
                }
            });
        }
    });

    Ok(TuiDriverHandle {
        _task: task,
        repaint_timer,
        focused,
        freeze_repaints_when_unfocused,
        _reader: reader,
        reader_shutdown,
        probe_lifecycle_lock,
        _guard: guard,
    })
}

/// Reads terminal input events on a dedicated thread and forwards them to the
/// foreground executor via `sender`. When `probe` is registered and enabled,
/// a `FocusGained` event (with no further input already queued behind it)
/// triggers an inline background-color re-query before the event is
/// forwarded: this thread is the sole reader of stdin, so the query/reply
/// round-trip cannot race the normal event stream.
fn run_tui_input_reader(
    sender: async_channel::Sender<TuiDriverEvent>,
    probe: Option<TuiProbe>,
    stdout_write_lock: Arc<Mutex<()>>,
    reader_shutdown: Arc<AtomicBool>,
    probe_lifecycle_lock: Arc<Mutex<()>>,
) {
    let mut stdout = stdout();
    loop {
        // Stop before another blocking read once teardown has requested a
        // shutdown, or either downstream consumer has gone away.
        if reader_shutdown.load(Ordering::Acquire)
            || sender.is_closed()
            || probe
                .as_ref()
                .is_some_and(|probe| probe.results.is_closed())
        {
            break;
        }

        match event::read() {
            Ok(event) => {
                let probe_enabled = probe.as_ref().is_some_and(|probe| (probe.is_enabled)());
                // Only probe when no further input is already queued: a probe
                // blocks this thread for up to its deadline, and a burst of
                // typed-ahead keys should never be delayed behind one.
                let should_probe = matches!(event, CrosstermEvent::FocusGained)
                    && probe_enabled
                    && !event::poll(Duration::ZERO).unwrap_or(true);
                if should_probe {
                    if let Some(probe) = probe.as_ref() {
                        let background = {
                            // Teardown waits on this lock before restoring
                            // terminal modes, so a probe that is already
                            // in flight always finishes cleanly.
                            let _probe_lifecycle_guard = probe_lifecycle_lock
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            if reader_shutdown.load(Ordering::Acquire)
                                || sender.is_closed()
                                || probe.results.is_closed()
                            {
                                break;
                            }
                            let query_result = {
                                let _query_write_guard = stdout_write_lock
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                                (probe.write_query)(&mut stdout)
                            };
                            query_result.ok().and_then(|()| (probe.read_reply)())
                        };
                        if block_on(probe.results.send(background)).is_err() {
                            break;
                        }
                    }
                }
                // The reader runs on a dedicated thread, so blocking on the
                // send is fine; an error means the receiver was dropped.
                if block_on(sender.send(TuiDriverEvent::Terminal(event))).is_err() {
                    break;
                }
            }
            // A signal interrupted the blocking read. The terminal is fine and
            // the next read will succeed, so retry rather than permanently
            // killing input on a stray `EINTR`.
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                // Hand the failure to the foreground half, which decides whether
                // it is a disconnect (quiet exit) or unexpected (reported), then
                // stop: this thread cannot read again either way.
                let _ = block_on(sender.send(TuiDriverEvent::InputFailed(error)));
                break;
            }
        }
    }
}

/// Draws a frame and schedules a timer for its element-requested repaint
/// deadline, if any.
///
/// Paint traverses the full tree, so each frame's reported deadline is the
/// authoritative next repaint: the new timer replaces — and thereby cancels —
/// any previously pending one, and a frame with no deadline clears the slot.
/// The timer redraws through this same function, so the cycle is
/// self-sustaining while elements animate and fully idle otherwise.
fn draw_and_schedule_repaint<T: TuiView, R: TuiTerminal + 'static>(
    screen: &Rc<RefCell<TuiScreen<T, R>>>,
    timer_slot: &Rc<RefCell<Option<ForegroundTask>>>,
    focused: &Rc<Cell<bool>>,
    freeze_repaints_when_unfocused: &Rc<Cell<bool>>,
    failed: &Rc<Cell<bool>>,
    ctx: &mut AppContext,
) -> io::Result<()> {
    // The host terminal is already gone; drawing again would only reproduce the
    // failure that latched this, once per invalidation, forever.
    if failed.get() {
        return Ok(());
    }
    let deadline = screen.borrow_mut().draw(ctx)?;
    let timer = deadline
        .filter(|_| should_schedule_repaints(focused.get(), freeze_repaints_when_unfocused.get()))
        .map(|deadline| {
            let screen = screen.clone();
            let focused = Rc::clone(focused);
            let freeze_repaints_when_unfocused = Rc::clone(freeze_repaints_when_unfocused);
            let failed = Rc::clone(failed);
            // Weak, or the slot (held by the task) and the task (held by the slot)
            // would keep each other alive.
            let weak_slot = Rc::downgrade(timer_slot);
            let weak_app = ctx.weak_app();
            ctx.foreground_executor().spawn(async move {
                let now = Instant::now();
                if deadline > now {
                    Timer::after(deadline - now).await;
                }
                let (Some(mut app), Some(timer_slot)) = (weak_app.upgrade(), weak_slot.upgrade())
                else {
                    return;
                };
                app.update(move |ctx| {
                    // The draw below replaces the slot, dropping this task's own
                    // handle; `async_task` defers destruction, so this in-flight
                    // poll completes normally.
                    if let Err(error) = draw_and_schedule_repaint(
                        &screen,
                        &timer_slot,
                        &focused,
                        &freeze_repaints_when_unfocused,
                        &failed,
                        ctx,
                    ) {
                        // Same failure path as the invalidation callback above, so a
                        // failure that surfaces on both reports once and terminates
                        // once (`failed` is the latch that guarantees it).
                        fail_tui_driver(
                            error,
                            TuiDriverIoOperation::DrawFrame,
                            &failed,
                            &timer_slot,
                            ctx,
                        );
                    }
                });
            })
        });
    *timer_slot.borrow_mut() = timer;
    Ok(())
}

fn should_schedule_repaints(focused: bool, freeze_repaints_when_unfocused: bool) -> bool {
    focused || !freeze_repaints_when_unfocused
}

/// The alternate-screen + raw-mode operations a [`RawModeGuard`] toggles.
/// Behind a trait so the guard's enter/leave lifecycle can be exercised without
/// a real terminal.
trait TerminalModeControl {
    fn enter(&mut self) -> io::Result<()>;
    fn leave(&mut self);
}

struct CrosstermModeControl {
    keyboard_enhancement_supported: bool,
    report_modifier_key_lifecycle: bool,
}

fn enter_terminal_screen(
    out: &mut impl Write,
    keyboard_enhancement_supported: bool,
    report_modifier_key_lifecycle: bool,
) -> io::Result<()> {
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        // Lets the input reader thread re-probe the host terminal's
        // background when the TUI regains focus (see `terminal_probe`).
        EnableFocusChange,
        Hide
    )?;

    // Opt into the Kitty keyboard protocol so protocol-aware terminals (Ghostty,
    // kitty, foot, WezTerm) report modified keys distinctly. This only affects
    // the TUI's own host terminal — the GUI never enters raw mode / the alt
    // screen and never runs this.
    //
    // Always request the backwards-compatible baseline so modified keys remain
    // distinct even if capability detection produced a false negative.
    // Alternate/all-key reporting is more invasive and remains restricted to
    // confirmed terminals when modifier lifecycle events are required.
    let flags =
        keyboard_enhancement_flags(keyboard_enhancement_supported && report_modifier_key_lifecycle);
    let _ = execute!(out, PushKeyboardEnhancementFlags(flags));
    Ok(())
}

fn keyboard_enhancement_flags(report_modifier_key_lifecycle: bool) -> KeyboardEnhancementFlags {
    let mut flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES;
    if report_modifier_key_lifecycle {
        flags |= KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
    }
    flags
}

fn leave_terminal_screen(out: &mut impl Write) -> io::Result<()> {
    let _ = execute!(out, PopKeyboardEnhancementFlags);
    execute!(
        out,
        Show,
        DisableBracketedPaste,
        DisableMouseCapture,
        DisableFocusChange,
        LeaveAlternateScreen
    )
}

impl TerminalModeControl for CrosstermModeControl {
    fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let mut out = stdout();
        if let Err(error) = enter_terminal_screen(
            &mut out,
            self.keyboard_enhancement_supported,
            self.report_modifier_key_lifecycle,
        ) {
            let _ = leave_terminal_screen(&mut out);
            let _ = terminal::disable_raw_mode();
            return Err(error);
        }
        Ok(())
    }

    fn leave(&mut self) {
        let mut out = stdout();
        let _ = leave_terminal_screen(&mut out);
        let _ = terminal::disable_raw_mode();
    }
}

/// Restores the host terminal on drop, so a panic or early return never strands
/// it in the alternate screen or raw mode.
struct RawModeGuard<C: TerminalModeControl> {
    control: C,
}

impl<C: TerminalModeControl> RawModeGuard<C> {
    fn enter(mut control: C) -> io::Result<Self> {
        control.enter()?;
        Ok(Self { control })
    }
}

impl<C: TerminalModeControl> Drop for RawModeGuard<C> {
    fn drop(&mut self) {
        self.control.leave();
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
