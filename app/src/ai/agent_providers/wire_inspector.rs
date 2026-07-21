//! Pop-out "wire inspector" panel for BYOP traffic.
//!
//! A floating in-app modal that shows, per exchange, the *new* outbound messages
//! and the model's response, with a context-usage bar, category filtering, text
//! search, and diff highlighting of the structured tools/skills/env context
//! between turns. Data comes from [`wire_log`], which only captures while this
//! panel is open (see `wire_log::set_enabled`).

use std::collections::HashSet;
use std::time::Duration;

use warp_core::ui::appearance::Appearance;
use warpui::elements::new_scrollable::SingleAxisConfig;
use warpui::elements::ClippedScrollStateHandle;
use warpui::elements::{
    ChildView, ConstrainedBox, Container, CrossAxisAlignment, Fill, Flex, MainAxisSize,
    NewScrollable, ParentElement,
};
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::clipboard::ClipboardContent;
use warpui::r#async::{SpawnedFutureHandle, Timer};
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use pathfinder_color::ColorU;

use crate::editor::{
    EditorView, Event as EditorEvent, PropagateAndNoOpNavigationKeys, SingleLineEditorOptions,
    TextOptions,
};
use crate::modal::{Modal, ModalEvent};
use crate::view_components::action_button::{ActionButton, ButtonSize, SecondaryTheme};

use super::wire_log::{self, Direction, Kind, Payload, WireEntry};

const MODAL_WIDTH: f32 = 720.;
const LIST_MAX_HEIGHT: f32 = 520.;
const PAYLOAD_PREVIEW_CHARS: usize = 4000;
/// How often the panel polls the capture buffer so entries recorded mid-turn
/// (agent-driven round-trips that arrive while the window is open) show up live.
const REFRESH_INTERVAL: Duration = Duration::from_millis(400);

// ---------------------------------------------------------------------------
// Actions / events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum WireInspectorAction {
    /// Toggle whether a category (by `Kind::slug`) is shown.
    ToggleKind(String),
    /// Empty the capture buffer.
    ClearBuffer,
    /// Pause/resume capture without closing the window.
    ToggleCapture,
    /// Copy the currently-shown (filtered) transcript to the clipboard.
    CopyAll,
}

#[derive(Debug, Clone)]
pub enum WireInspectorEvent {
    Close,
}

// ---------------------------------------------------------------------------
// Inner view
// ---------------------------------------------------------------------------

pub struct WireInspectorView {
    search_editor: ViewHandle<EditorView>,
    search: String,
    scroll_state: ClippedScrollStateHandle,
    /// Category slugs currently hidden by the filter.
    hidden_kinds: HashSet<String>,
    /// One filter chip per `Kind`, kept in `Kind::ALL` order.
    filter_buttons: Vec<(&'static str, ViewHandle<ActionButton>)>,
    clear_button: ViewHandle<ActionButton>,
    /// Copy the filtered transcript to the clipboard.
    copy_button: ViewHandle<ActionButton>,
    /// Pause/resume capture; label reflects `wire_log::is_enabled()`.
    capture_button: ViewHandle<ActionButton>,
    /// Last `wire_log::generation()` we rendered, so the refresh tick only
    /// repaints when new traffic actually landed.
    last_generation: u64,
    /// Keeps the self-rescheduling refresh timer alive (dropping it aborts it).
    _refresh_handle: Option<SpawnedFutureHandle>,
}

impl WireInspectorView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let search_editor = ctx.add_typed_action_view(|ctx| {
            EditorView::single_line(
                SingleLineEditorOptions {
                    text: TextOptions::default(),
                    soft_wrap: false,
                    propagate_and_no_op_vertical_navigation_keys:
                        PropagateAndNoOpNavigationKeys::Always,
                    ..Default::default()
                },
                ctx,
            )
        });
        ctx.subscribe_to_view(&search_editor, |me, editor, event, ctx| {
            if let EditorEvent::Edited(_) = event {
                me.search = editor.as_ref(ctx).buffer_text(ctx).to_string();
                ctx.notify();
            }
        });

        let filter_buttons = Kind::ALL
            .iter()
            .map(|kind| {
                let slug = kind.slug();
                let label = kind_label(*kind).to_string();
                let button = ctx.add_typed_action_view(move |btn_ctx| {
                    let mut button = ActionButton::new(label.clone(), SecondaryTheme)
                        .with_size(ButtonSize::Small)
                        .on_click(move |ctx| {
                            ctx.dispatch_typed_action(WireInspectorAction::ToggleKind(
                                slug.to_string(),
                            ));
                        });
                    // All categories are shown by default; "active" marks an included
                    // filter. Using active (not disabled) keeps the chip clickable so
                    // it can be toggled back on.
                    button.set_active(true, btn_ctx);
                    button
                });
                (slug, button)
            })
            .collect();

        let clear_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Clear".to_string(), SecondaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(WireInspectorAction::ClearBuffer))
        });

        let copy_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Copy".to_string(), SecondaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(WireInspectorAction::CopyAll))
        });

        let capture_button = ctx.add_typed_action_view(|_| {
            ActionButton::new(capture_button_label(), SecondaryTheme)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(WireInspectorAction::ToggleCapture))
        });

        let refresh_handle = Self::spawn_refresh_tick(ctx);

        Self {
            search_editor,
            search: String::new(),
            scroll_state: ClippedScrollStateHandle::new(),
            hidden_kinds: HashSet::new(),
            filter_buttons,
            clear_button,
            copy_button,
            capture_button,
            last_generation: wire_log::generation(),
            _refresh_handle: Some(refresh_handle),
        }
    }

    /// Spawn a one-shot timer; on fire it repaints (if new traffic arrived) and
    /// reschedules itself, giving a lightweight live refresh while the window is
    /// open. The poll is cheap (an atomic load) and only calls `notify` on change.
    fn spawn_refresh_tick(ctx: &mut ViewContext<Self>) -> SpawnedFutureHandle {
        ctx.spawn(
            async move {
                Timer::after(REFRESH_INTERVAL).await;
            },
            |me, _, ctx| me.on_refresh_tick(ctx),
        )
    }

    fn on_refresh_tick(&mut self, ctx: &mut ViewContext<Self>) {
        let generation = wire_log::generation();
        if generation != self.last_generation {
            self.last_generation = generation;
            ctx.notify();
        }
        self._refresh_handle = Some(Self::spawn_refresh_tick(ctx));
    }

    fn kind_visible(&self, kind: Kind) -> bool {
        !self.hidden_kinds.contains(kind.slug())
    }

    fn entry_matches_search(&self, entry: &WireEntry) -> bool {
        if self.search.trim().is_empty() {
            return true;
        }
        let needle = self.search.to_ascii_lowercase();
        let hay = format!(
            "{} {} {} {}",
            kind_label(entry.kind),
            entry.model_id,
            entry.adapter,
            payload_text(&entry.payload),
        )
        .to_ascii_lowercase();
        hay.contains(&needle)
    }

    // -- rendering ----------------------------------------------------------

    fn render_header(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);

        let mut chips = Flex::row().with_main_axis_size(MainAxisSize::Min);
        for (_slug, button) in &self.filter_buttons {
            chips.add_child(
                Container::new(button.as_ref(app).render(app))
                    .with_margin_right(6.)
                    .finish(),
            );
        }
        chips.add_child(
            Container::new(self.capture_button.as_ref(app).render(app))
                .with_margin_left(6.)
                .finish(),
        );
        chips.add_child(
            Container::new(self.copy_button.as_ref(app).render(app))
                .with_margin_left(6.)
                .finish(),
        );
        chips.add_child(
            Container::new(self.clear_button.as_ref(app).render(app))
                .with_margin_left(6.)
                .finish(),
        );

        let search_input = appearance
            .ui_builder()
            .text_input(self.search_editor.clone())
            .with_style(UiComponentStyles {
                width: Some(MODAL_WIDTH - 48.),
                padding: Some(Coords {
                    top: 4.,
                    bottom: 4.,
                    left: 8.,
                    right: 8.,
                }),
                ..Default::default()
            })
            .build()
            .finish();

        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(chips.finish())
            .with_child(
                Container::new(search_input)
                    .with_margin_top(8.)
                    .with_margin_bottom(8.)
                    .finish(),
            )
            .finish()
    }

    fn render_context_bar(&self, app: &AppContext, entries: &[WireEntry]) -> Option<Box<dyn Element>> {
        let usage = entries
            .iter()
            .rev()
            .find_map(|e| e.usage.as_ref())?;
        let appearance = Appearance::as_ref(app);
        let pct = usage.pct.clamp(0.0, 100.0);
        let color = if pct >= 90.0 {
            appearance.theme().ui_error_color()
        } else if pct >= 70.0 {
            appearance.theme().ui_warning_color()
        } else {
            appearance.theme().ui_green_color()
        };
        let label = format!(
            "Context: {:.1}% of {} ({} tokens{})",
            pct,
            usage.context_window,
            usage
                .active_kv_tokens
                .unwrap_or(usage.prompt + usage.completion),
            usage
                .active_kv_tokens
                .map(|_| " active KV")
                .unwrap_or(""),
        );
        Some(text_line(app, &label, 12., color))
    }

    fn render_entry(
        &self,
        app: &AppContext,
        entry: &WireEntry,
        prev_out: Option<&WireEntry>,
    ) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let (dir_color, dir_glyph) = match entry.direction {
            Direction::Out => (theme.accent().into_solid(), "→"),
            Direction::In => (theme.ui_green_color(), "←"),
        };

        let header = format!(
            "{dir_glyph} {}  ·  {}  ·  {}",
            kind_label(entry.kind),
            entry.model_id,
            entry.adapter,
        );

        let mut column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(text_line(app, &header, 13., dir_color));

        // Structured context (Out entries only). Tools/skills get an always-present
        // summary line (so they are never silently missing), plus +/- diff lines vs
        // the previous Out entry; env is diffed line by line.
        if let Some(snap) = &entry.context {
            let prev = prev_out.and_then(|e| e.context.as_ref());

            column.add_child(summary_element(app, "tools", &snap.tools, true));
            for line in diff_lines("tools", prev.map(|p| &p.tools), &snap.tools) {
                column.add_child(diff_element(app, line));
            }
            column.add_child(summary_element(app, "skills", &snap.skills, false));
            for line in diff_lines("skills", prev.map(|p| &p.skills), &snap.skills) {
                column.add_child(diff_element(app, line));
            }
            let prev_env = prev.map(|p| env_strings(&p.env));
            let cur_env = env_strings(&snap.env);
            for line in diff_lines("env", prev_env.as_ref(), &cur_env) {
                column.add_child(diff_element(app, line));
            }

            // System prompt: shown in full when it changes from the previous turn
            // (first turn always shows it); not char-diffed, per the requirements.
            if let Some(system) = &snap.system {
                let prev_system = prev.and_then(|p| p.system.as_ref());
                if prev_system != Some(system) {
                    let header = format!("system prompt ({} chars)", system.chars().count());
                    column.add_child(Container::new(text_line(
                        app,
                        &header,
                        12.,
                        theme.accent().into_solid(),
                    ))
                    .with_margin_top(4.)
                    .finish());
                    let shown = truncate_preview(system);
                    column.add_child(payload_block(app, &shown));
                } else {
                    column.add_child(text_line(app, "system prompt: unchanged", 11., muted(app)));
                }
            }
        }

        // Payload (pretty JSON or a flag), truncated for very large bodies.
        let shown = truncate_preview(&payload_text(&entry.payload));
        column.add_child(
            Container::new(payload_block(app, &shown))
                .with_margin_top(4.)
                .finish(),
        );

        Container::new(column.finish())
            .with_margin_bottom(12.)
            .finish()
    }

    fn render_list(&self, app: &AppContext) -> Box<dyn Element> {
        let all = wire_log::snapshot();

        // Track the previous Out entry (unfiltered) so diffs stay stable even when
        // the filter/search hides intervening rows.
        let mut prev_out: Option<WireEntry> = None;
        let mut rows = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
        let mut shown_any = false;

        for entry in &all {
            let prev_for_this = prev_out.clone();
            if entry.direction == Direction::Out {
                prev_out = Some(entry.clone());
            }
            if !self.kind_visible(entry.kind) || !self.entry_matches_search(entry) {
                continue;
            }
            shown_any = true;
            rows.add_child(self.render_entry(app, entry, prev_for_this.as_ref()));
        }

        if !shown_any {
            let msg = if all.is_empty() {
                "No traffic captured yet. Send a message to a BYOP model with a configured context window."
            } else {
                "No entries match the current filter/search."
            };
            return Container::new(text_line(app, msg, 12., muted(app)))
                .with_margin_top(8.)
                .finish();
        }

        let theme = Appearance::as_ref(app).theme();
        let scrollable = NewScrollable::vertical(
            SingleAxisConfig::Clipped {
                handle: self.scroll_state.clone(),
                child: rows.finish(),
            },
            theme.nonactive_ui_detail().into(),
            theme.active_ui_detail().into(),
            Fill::None,
        )
        .finish();

        ConstrainedBox::new(scrollable)
            .with_max_height(LIST_MAX_HEIGHT)
            .finish()
    }
}

impl Entity for WireInspectorView {
    type Event = WireInspectorEvent;
}

impl View for WireInspectorView {
    fn ui_name() -> &'static str {
        "WireInspectorView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let entries = wire_log::snapshot();
        let theme = Appearance::as_ref(app).theme();
        let mut column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(self.render_header(app));

        // Capture is only meaningful when the active model has a context window
        // defined — that same value gates the send path. Surface a clear error
        // rather than silently sitting empty.
        match super::active_context_window(app) {
            None => {
                let msg = "Capture disabled: the active model has no context window set \
                    (must be > 0). Set it in Settings → AI → provider models, then reopen.";
                column.add_child(
                    Container::new(text_line(app, msg, 12., theme.ui_error_color()))
                        .with_margin_top(8.)
                        .with_margin_bottom(8.)
                        .finish(),
                );
            }
            Some(window) => {
                let (status, color) = if wire_log::is_enabled() {
                    (
                        format!("● Capturing · context window {window} tokens"),
                        theme.ui_green_color(),
                    )
                } else {
                    ("⏸ Paused".to_string(), muted(app))
                };
                column.add_child(
                    Container::new(text_line(app, &status, 12., color))
                        .with_margin_top(8.)
                        .with_margin_bottom(4.)
                        .finish(),
                );
                if let Some(bar) = self.render_context_bar(app, &entries) {
                    column.add_child(Container::new(bar).with_margin_bottom(8.).finish());
                }
            }
        }

        column.add_child(self.render_list(app));
        ConstrainedBox::new(column.finish())
            .with_width(MODAL_WIDTH)
            .finish()
    }
}

impl TypedActionView for WireInspectorView {
    type Action = WireInspectorAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            WireInspectorAction::ToggleKind(slug) => {
                if self.hidden_kinds.contains(slug) {
                    self.hidden_kinds.remove(slug);
                } else {
                    self.hidden_kinds.insert(slug.clone());
                }
                let shown = !self.hidden_kinds.contains(slug);
                if let Some((_, button)) = self.filter_buttons.iter().find(|(s, _)| s == slug) {
                    // active == included; keep the chip clickable so it can toggle back.
                    button.update(ctx, |button, ctx| button.set_active(shown, ctx));
                }
                ctx.notify();
            }
            WireInspectorAction::ClearBuffer => {
                wire_log::clear();
                ctx.notify();
            }
            WireInspectorAction::ToggleCapture => {
                wire_log::set_enabled(!wire_log::is_enabled());
                let label = capture_button_label();
                self.capture_button
                    .update(ctx, |button, ctx| button.set_label(label, ctx));
                ctx.notify();
            }
            WireInspectorAction::CopyAll => {
                let transcript = self.transcript();
                ctx.clipboard()
                    .write(ClipboardContent::plain_text(transcript));
            }
        }
    }
}

impl WireInspectorView {
    /// Full text of the currently-shown (filtered) entries, for the clipboard.
    /// Payloads are included untruncated here — the on-screen preview clips, the
    /// copy does not.
    fn transcript(&self) -> String {
        let mut out = String::new();
        for entry in wire_log::snapshot() {
            if !self.kind_visible(entry.kind) || !self.entry_matches_search(&entry) {
                continue;
            }
            let glyph = match entry.direction {
                Direction::Out => "→",
                Direction::In => "←",
            };
            out.push_str(&format!(
                "{glyph} {}  ·  {}  ·  {}\n",
                kind_label(entry.kind),
                entry.model_id,
                entry.adapter,
            ));
            if let Some(snap) = &entry.context {
                if !snap.tools.is_empty() {
                    out.push_str(&format!("tools ({}): {}\n", snap.tools.len(), snap.tools.join(", ")));
                }
                if !snap.skills.is_empty() {
                    out.push_str(&format!("skills ({}): {}\n", snap.skills.len(), snap.skills.join(", ")));
                }
                for (k, v) in &snap.env {
                    out.push_str(&format!("env {k}={v}\n"));
                }
                if let Some(system) = &snap.system {
                    out.push_str("--- system prompt ---\n");
                    out.push_str(system);
                    out.push_str("\n--- end system prompt ---\n");
                }
            }
            out.push_str(&payload_text(&entry.payload));
            out.push_str("\n\n");
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Modal wrapper
// ---------------------------------------------------------------------------

pub struct WireInspectorModal {
    modal: ViewHandle<Modal<WireInspectorView>>,
    #[allow(dead_code)]
    view: ViewHandle<WireInspectorView>,
}

impl WireInspectorModal {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let view = ctx.add_typed_action_view(WireInspectorView::new);
        let body = view.clone();
        let modal = ctx.add_typed_action_view(|ctx| {
            Modal::new(Some("BYOP wire inspector".to_string()), body, ctx)
                .with_modal_style(UiComponentStyles {
                    width: Some(MODAL_WIDTH + 48.),
                    ..Default::default()
                })
                .with_background_opacity(100)
                .with_dismiss_on_click()
        });
        ctx.subscribe_to_view(&modal, |me, _, event, ctx| match event {
            ModalEvent::Close => me.handle_close(ctx),
        });
        Self { modal, view }
    }

    fn handle_close(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(WireInspectorModalEvent::Close);
    }
}

#[derive(Debug, Clone)]
pub enum WireInspectorModalEvent {
    Close,
}

impl Entity for WireInspectorModal {
    type Event = WireInspectorModalEvent;
}

impl View for WireInspectorModal {
    fn ui_name() -> &'static str {
        "WireInspectorModal"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        ChildView::new(&self.modal).finish()
    }
}

impl TypedActionView for WireInspectorModal {
    type Action = ();
    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {}
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Truncate a large body for display, appending a marker when clipped.
fn truncate_preview(s: &str) -> String {
    if s.chars().count() > PAYLOAD_PREVIEW_CHARS {
        let mut out: String = s.chars().take(PAYLOAD_PREVIEW_CHARS).collect();
        out.push_str("\n… (truncated — use Copy to get the full text)");
        out
    } else {
        s.to_owned()
    }
}

/// Always-present one-line summary of a structured list, e.g. `tools (3): a, b, c`.
/// When empty, hints that the items may live in the system prompt instead.
fn summary_element(
    app: &AppContext,
    label: &str,
    items: &[String],
    hint_when_empty: bool,
) -> Box<dyn Element> {
    let text = if items.is_empty() {
        if hint_when_empty {
            format!("{label}: none (may be defined in the system prompt)")
        } else {
            format!("{label}: none")
        }
    } else {
        let joined = items.join(", ");
        let joined = if joined.chars().count() > 300 {
            joined.chars().take(300).collect::<String>() + "…"
        } else {
            joined
        };
        format!("{label} ({}): {joined}", items.len())
    };
    text_line(app, &text, 12., muted(app))
}

fn capture_button_label() -> String {
    if wire_log::is_enabled() {
        "Pause capture".to_string()
    } else {
        "Resume capture".to_string()
    }
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::UserDelta => "Outgoing",
        Kind::Response => "Response",
        Kind::TitleGen => "Title",
        Kind::Oneshot => "One-shot",
        Kind::Compaction => "Compaction",
    }
}

fn payload_text(payload: &Payload) -> String {
    match payload {
        Payload::Json(s) => s.clone(),
        Payload::Flagged(reason) => format!("[non-JSON: {reason}]"),
    }
}

fn env_strings(env: &[(String, String)]) -> Vec<String> {
    env.iter().map(|(k, v)| format!("{k}={v}")).collect()
}

/// One diff line: `(sign, text)` where sign is '+', '-', or ' '.
fn diff_lines(label: &str, prev: Option<&Vec<String>>, cur: &[String]) -> Vec<(char, String)> {
    let prev_set: HashSet<&String> = prev.map(|p| p.iter().collect()).unwrap_or_default();
    let cur_set: HashSet<&String> = cur.iter().collect();
    let mut out = Vec::new();
    // Removed (in prev, not in cur) — only when we have a prev to compare against.
    if let Some(prev) = prev {
        for item in prev {
            if !cur_set.contains(item) {
                out.push(('-', format!("{label}: {item}")));
            }
        }
    }
    for item in cur {
        let sign = if prev.is_some() && !prev_set.contains(item) {
            '+'
        } else {
            ' '
        };
        // Only surface unchanged lines when there's no prior (first turn), to keep
        // later turns focused on what actually changed.
        if sign == ' ' && prev.is_some() {
            continue;
        }
        out.push((sign, format!("{label}: {item}")));
    }
    out
}

fn diff_element(app: &AppContext, line: (char, String)) -> Box<dyn Element> {
    let theme = Appearance::as_ref(app).theme();
    let (sign, text) = line;
    let color = match sign {
        '+' => theme.ui_green_color(),
        '-' => theme.ui_error_color(),
        _ => muted(app),
    };
    text_line(app, &format!("{sign} {text}"), 12., color)
}

fn muted(app: &AppContext) -> ColorU {
    Appearance::as_ref(app)
        .theme()
        .disabled_ui_text_color()
        .into_solid()
}

fn text_line(app: &AppContext, s: &str, size: f32, color: ColorU) -> Box<dyn Element> {
    Appearance::as_ref(app)
        .ui_builder()
        .paragraph(s.to_string())
        .with_style(UiComponentStyles {
            font_size: Some(size),
            font_color: Some(color),
            ..Default::default()
        })
        .build()
        .finish()
}

fn payload_block(app: &AppContext, s: &str) -> Box<dyn Element> {
    let color = Appearance::as_ref(app)
        .theme()
        .active_ui_text_color()
        .into_solid();
    Appearance::as_ref(app)
        .ui_builder()
        .paragraph(s.to_string())
        .with_style(UiComponentStyles {
            font_size: Some(11.),
            font_color: Some(color),
            ..Default::default()
        })
        .build()
        .finish()
}
