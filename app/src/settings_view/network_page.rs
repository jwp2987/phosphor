//! The "Network" settings page: global HTTP proxy configuration (see Issue #72).
//!
//! Design principle: every input box always shows the currently saved value and can be
//! edited directly (including clearing it), submitted via the adjacent "Save" button.
//! Password fields are masked using `is_password: true`. In System / Off mode
//! the input boxes are disabled + show a hint; only Custom mode is editable.

use std::sync::Arc;
use std::time::{Duration, Instant};

use settings::Setting;
use warpui::{
    AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
    elements::{
        Align, Clipped, ConstrainedBox, Container, CrossAxisAlignment, Element, Flex,
        MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement, Text,
    },
    fonts::{Properties, Weight},
    ui_components::{
        button::ButtonVariant,
        components::{Coords, UiComponent, UiComponentStyles},
        switch::SwitchStateHandle,
    },
};

use super::SettingsSection;
use super::settings_page::{
    AdditionalInfo, LocalOnlyIconState, MatchData, PageType, SettingsPageEvent, SettingsPageMeta,
    SettingsPageViewHandle, SettingsWidget, ToggleState, render_body_item, render_page_title,
    render_sub_header_with_description,
};
use crate::appearance::Appearance;
use crate::editor::{
    EditorView, Event as EditorEvent, InteractionState, PropagateAndNoOpNavigationKeys,
    SingleLineEditorOptions, TextOptions,
};
use crate::network_gate_log::{LoggedGateEvent, NetworkGateLog, NetworkGateLogEvent};
use crate::report_if_error;
use crate::settings::network::{NetworkSettings, ProxyMode};
use crate::settings::network_secrets::{ProxyCredentials, ProxyPasswordLoad};
use crate::view_components::dropdown::{Dropdown, DropdownItem};
use warp_editor::editor::NavigationKey;

/// The public URL used for "outbound connectivity" detection in System / Off mode.
/// `generate_204` is proxy-friendly, has no body, and always returns 204.
const PUBLIC_PROBE_URL: &str = "https://www.google.com/generate_204";

/// The maximum wait time for a single connection test.
const TEST_CONNECTION_TIMEOUT_SECS: u64 = 8;

/// The maximum width of the input box area (editor + two buttons), aligned with the slot
/// constraint to the right of the field label.
const INPUT_AREA_MAX_WIDTH: f32 = 420.0;

const BUTTON_PADDING: f32 = 6.0;

/// Reads the system proxy from environment variables (a minimal cross-platform set):
/// returns (https_proxy, http_proxy, no_proxy).
/// Deeper reads via Windows WinINET / macOS SCDynamicStore are left for a follow-up PR.
fn read_system_proxy_env() -> (String, String, String) {
    fn read(name_upper: &str) -> String {
        std::env::var(name_upper)
            .ok()
            .or_else(|| std::env::var(name_upper.to_lowercase()).ok())
            .unwrap_or_default()
    }
    (read("HTTPS_PROXY"), read("HTTP_PROXY"), read("NO_PROXY"))
}

/// Splits the stored allowed-origins text the way the gate does (commas or whitespace),
/// dropping empties and repeats (case-insensitive), keeping the first spelling and the order.
pub(crate) fn parse_origin_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in text.split(|c: char| c == ',' || c.is_whitespace()) {
        let item = item.trim();
        if item.is_empty() || out.iter().any(|o| o.eq_ignore_ascii_case(item)) {
            continue;
        }
        out.push(item.to_owned());
    }
    out
}

/// The stored form: one origin per line.
pub(crate) fn join_origin_list(list: &[String]) -> String {
    list.join("\n")
}

/// `text` with `origin` added. Returns `None` when `origin` is empty or holds a separator
/// (it would become two entries) -- the caller leaves the setting alone then. An origin
/// already listed changes nothing but the normalisation.
pub(crate) fn add_origin(text: &str, origin: &str) -> Option<String> {
    let origin = origin.trim();
    if origin.is_empty() || origin.contains(|c: char| c == ',' || c.is_whitespace()) {
        return None;
    }
    let mut list = parse_origin_list(text);
    if !list.iter().any(|o| o.eq_ignore_ascii_case(origin)) {
        list.push(origin.to_owned());
    }
    Some(join_origin_list(&list))
}

/// `text` without `origin` (case-insensitive).
pub(crate) fn remove_origin(text: &str, origin: &str) -> String {
    let list: Vec<String> = parse_origin_list(text)
        .into_iter()
        .filter(|o| !o.eq_ignore_ascii_case(origin.trim()))
        .collect();
    join_origin_list(&list)
}

/// Appends `origin` to the user's allowed origins setting (the Allow buttons and the denial
/// notice's link). Idempotent.
pub(crate) fn allow_origin(origin: &str, ctx: &mut AppContext) {
    let current = NetworkSettings::as_ref(ctx).allowed_origins.value().clone();
    let Some(next) = add_origin(&current, origin) else {
        return;
    };
    if next == current {
        return;
    }
    NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
        report_if_error!(settings.allowed_origins.set_value(next, ctx));
    });
}

/// The keymap flag the page sets while its focus ring sits on a Remove / Allow button, so
/// Enter / Space / Delete are bound only then (never while an editor is typing).
pub(crate) const KB_BUTTON_FLAG: &str = "NetworkPageKbButton";

/// A stop on the page's keyboard focus ring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KbStop {
    OriginEditor,
    Remove(String),
    Allow(String),
}

/// The origin's host hash as `hop=network_gate_ui` logs it: FNV-1a of the lower-cased host
/// (scheme, port, path and brackets dropped), the same hash a `network gate: denied` line
/// carries. Never the origin itself.
pub(crate) fn origin_hash(origin: &str) -> String {
    let mut o = origin.trim();
    if let Some((_, rest)) = o.split_once("://") {
        o = rest;
    }
    o = o.split(['/', '?', '#']).next().unwrap_or(o);
    let host = if let Some(rest) = o.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else if o.matches(':').count() == 1 {
        o.split(':').next().unwrap_or(o)
    } else {
        o
    };
    format!(
        "{:016x}",
        http_client::gate::hash_host(&host.to_ascii_lowercase())
    )
}

fn log_ui_hop(action: &str, origin: &str) {
    log::info!(
        "hop=network_gate_ui action={action} origin_hash={}",
        origin_hash(origin)
    );
}

/// The ring order: the editor, then one Remove per origin, then one Allow per denied log row
/// that has an origin. `rows` is the Allow origins in log order.
pub(crate) fn kb_stops(origins: &[String], allow_rows: &[String]) -> Vec<KbStop> {
    let mut v = vec![KbStop::OriginEditor];
    v.extend(origins.iter().cloned().map(KbStop::Remove));
    v.extend(allow_rows.iter().cloned().map(KbStop::Allow));
    v
}

/// What Enter / Space (`remove_only` false) or Delete / Backspace (true) does on `stop`.
pub(crate) fn kb_action_for(stop: &KbStop, remove_only: bool) -> Option<NetworkPageAction> {
    match stop {
        KbStop::Remove(o) => Some(NetworkPageAction::RemoveOrigin(o.clone())),
        KbStop::Allow(o) if !remove_only => Some(NetworkPageAction::AllowOrigin(o.clone())),
        _ => None,
    }
}

/// `current` moved one step in `dir` (+1 / -1) over `len` stops, wrapping; `None` starts at the
/// first (forward) or last (backward) stop.
pub(crate) fn kb_step(current: Option<usize>, len: usize, dir: i32) -> usize {
    match (current, dir >= 0) {
        (None, true) => 0,
        (None, false) => len.saturating_sub(1),
        (Some(i), true) => (i + 1) % len.max(1),
        (Some(i), false) => (i + len.max(1) - 1) % len.max(1),
    }
}

#[derive(Debug, Clone)]
pub enum NetworkPageAction {
    /// Flipped the network gate switch.
    ToggleGate,
    /// Clicked "Add" on the allowed-origins input.
    AddOrigin,
    RemoveOrigin(String),
    /// Clicked "Allow" on a denied row of the gate log; carries the origin text.
    AllowOrigin(String),
    ClearGateLog,
    /// Keyboard: move the page's focus ring to the next / previous stop (the origin editor,
    /// each origin's Remove, each denied log row's Allow), wrapping.
    KbNext,
    KbPrev,
    /// Keyboard: Enter / Space on the focused Remove or Allow.
    KbActivate,
    /// Keyboard: Delete / Backspace on a focused origin.
    KbRemove,
    /// Keyboard: Escape, back to the settings search box.
    KbLeave,
    /// A ProxyMode item was selected in the dropdown; persisted to settings.
    SetProxyMode(ProxyMode),
    /// Clicked the "Save" button on the URL field.
    SaveProxyUrl,
    /// Clicked the "Clear" button on the URL field.
    ClearProxyUrl,
    SaveProxyUsername,
    ClearProxyUsername,
    SaveProxyPassword,
    ClearProxyPassword,
    SaveProxyNoProxy,
    ClearProxyNoProxy,
    /// Clicked the "Test connection" button.
    TestConnection,
    /// The connection test completed.
    TestConnectionResult(TestOutcome),
}

/// The detection method chosen for this test. Used by the result copy to pick an appropriate description.
#[derive(Debug, Clone, Copy)]
enum TestKind {
    /// Probes the proxy host:port over TCP (verifies the proxy itself is reachable; suitable for
    /// corporate intranet / VPN proxies).
    /// Used for Custom mode and for System mode when a system proxy can be detected from
    /// environment variables.
    Tcp,
    /// HTTP GET probe of a public URL. Only used in Off mode, or as a fallback in System mode
    /// when the system proxy could not be detected.
    Http,
}

/// The test result (returned from the async task to the main thread's handle_action).
#[derive(Debug, Clone)]
pub struct TestOutcome {
    kind: TestKind,
    result: Result<u128, String>,
}

/// The current state of the connection test.
#[derive(Debug, Clone, Default)]
enum TestState {
    #[default]
    Idle,
    Running,
    Success {
        kind: TestKind,
        latency_ms: u128,
    },
    Failed {
        kind: TestKind,
        message: String,
    },
}

pub struct NetworkPageView {
    page: PageType<Self>,
    /// The proxy-mode dropdown.
    mode_dropdown: ViewHandle<Dropdown<NetworkPageAction>>,
    /// The editor for each field (the password field has the `is_password` mask enabled).
    url_editor: ViewHandle<EditorView>,
    username_editor: ViewHandle<EditorView>,
    password_editor: ViewHandle<EditorView>,
    no_proxy_editor: ViewHandle<EditorView>,
    /// Mouse state for each field's two buttons (Save + Clear).
    url_save_state: MouseStateHandle,
    url_clear_state: MouseStateHandle,
    username_save_state: MouseStateHandle,
    username_clear_state: MouseStateHandle,
    password_save_state: MouseStateHandle,
    password_clear_state: MouseStateHandle,
    no_proxy_save_state: MouseStateHandle,
    no_proxy_clear_state: MouseStateHandle,
    /// Mouse state and status for the test-connection button.
    test_button_state: MouseStateHandle,
    test_state: TestState,
    gate_switch_state: SwitchStateHandle,
    origin_editor: ViewHandle<EditorView>,
    origin_add_state: MouseStateHandle,
    /// One per listed origin / per log row; resized whenever the list changes.
    origin_remove_states: Vec<MouseStateHandle>,
    log_allow_states: Vec<MouseStateHandle>,
    /// Index into `kb_stops`; `None` while the keyboard ring is not on this page.
    kb_stop: Option<usize>,
    gate_log_clear_state: MouseStateHandle,
}

impl NetworkPageView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let mode_dropdown = ctx.add_typed_action_view(Dropdown::<NetworkPageAction>::new);
        mode_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_items(
                vec![
                    DropdownItem::new(
                        crate::t!("settings-network-mode-off"),
                        NetworkPageAction::SetProxyMode(ProxyMode::Off),
                    ),
                    DropdownItem::new(
                        crate::t!("settings-network-mode-system"),
                        NetworkPageAction::SetProxyMode(ProxyMode::System),
                    ),
                    DropdownItem::new(
                        crate::t!("settings-network-mode-custom"),
                        NetworkPageAction::SetProxyMode(ProxyMode::Custom),
                    ),
                ],
                ctx,
            );
        });

        let url_editor =
            build_text_editor(ctx, false, crate::t!("settings-network-url-placeholder"));
        let username_editor = build_text_editor(
            ctx,
            false,
            crate::t!("settings-network-username-placeholder"),
        );
        let password_editor = build_text_editor(
            ctx,
            true,
            crate::t!("settings-network-password-placeholder"),
        );
        let no_proxy_editor = build_text_editor(
            ctx,
            false,
            crate::t!("settings-network-no-proxy-placeholder"),
        );

        // Subscribes to settings / credentials changes -- whenever a field or the mode
        // changes externally, pushes the latest values back into each editor's buffer, and
        // syncs the dropdown's options.
        ctx.subscribe_to_model(
            &NetworkSettings::handle(ctx),
            |me: &mut Self, _, _event, ctx| {
                Self::sync_all_from_settings(me, ctx);
                ctx.notify();
            },
        );
        ctx.subscribe_to_model(
            &ProxyCredentials::handle(ctx),
            |me: &mut Self, _, _event, ctx| {
                Self::sync_password_from_credentials(me, ctx);
                ctx.notify();
            },
        );

        let origin_editor = build_text_editor_opts(
            ctx,
            false,
            crate::t!("settings-network-origins-placeholder"),
            true,
        );
        // Keyboard: Enter adds the typed origin, Tab / Shift-Tab walk the focus ring, Escape
        // returns to the settings search box.
        ctx.subscribe_to_view(&origin_editor, |me: &mut Self, _, event, ctx| match event {
            EditorEvent::Enter => me.handle_action(&NetworkPageAction::AddOrigin, ctx),
            EditorEvent::Navigate(NavigationKey::Tab) => {
                me.handle_action(&NetworkPageAction::KbNext, ctx)
            }
            EditorEvent::Navigate(NavigationKey::ShiftTab) => {
                me.handle_action(&NetworkPageAction::KbPrev, ctx)
            }
            EditorEvent::Escape => me.handle_action(&NetworkPageAction::KbLeave, ctx),
            _ => {}
        });
        ctx.subscribe_to_model(
            &NetworkGateLog::handle(ctx),
            |me: &mut Self, _, event, ctx| {
                if matches!(event, NetworkGateLogEvent::Changed) {
                    me.resize_row_states(ctx);
                    ctx.notify();
                }
            },
        );

        let mut me = Self {
            page: PageType::new_monolith(NetworkPageWidget::default(), None, false),
            mode_dropdown,
            url_editor,
            username_editor,
            password_editor,
            no_proxy_editor,
            url_save_state: MouseStateHandle::default(),
            url_clear_state: MouseStateHandle::default(),
            username_save_state: MouseStateHandle::default(),
            username_clear_state: MouseStateHandle::default(),
            password_save_state: MouseStateHandle::default(),
            password_clear_state: MouseStateHandle::default(),
            no_proxy_save_state: MouseStateHandle::default(),
            no_proxy_clear_state: MouseStateHandle::default(),
            test_button_state: MouseStateHandle::default(),
            test_state: TestState::Idle,
            gate_switch_state: SwitchStateHandle::default(),
            origin_editor,
            origin_add_state: MouseStateHandle::default(),
            origin_remove_states: Vec::new(),
            log_allow_states: Vec::new(),
            kb_stop: None,
            gate_log_clear_state: MouseStateHandle::default(),
        };
        me.resize_row_states(ctx);

        // Does an initial sync so the dropdown and each editor show the currently saved values.
        Self::sync_all_from_settings(&mut me, ctx);
        Self::sync_password_from_credentials(&mut me, ctx);
        me
    }

    fn current_stops(&self, ctx: &AppContext) -> Vec<KbStop> {
        let origins = parse_origin_list(NetworkSettings::as_ref(ctx).allowed_origins.value());
        let allow_rows: Vec<String> = NetworkGateLog::as_ref(ctx)
            .entries_newest_first()
            .iter()
            .filter(|e| e.event.outcome == http_client::gate::Outcome::Denied)
            .filter_map(|e| crate::network_gate_log::allow_text(&e.event))
            .collect();
        kb_stops(&origins, &allow_rows)
    }

    /// Puts the keyboard ring on `stop` (an index into `current_stops`): the editor takes real
    /// focus; a button stop focuses this page, whose bindings then take Enter / Space / Delete.
    fn kb_goto(&mut self, idx: usize, ctx: &mut ViewContext<Self>) {
        let stops = self.current_stops(ctx);
        let idx = idx.min(stops.len().saturating_sub(1));
        self.kb_stop = Some(idx);
        match stops.get(idx) {
            Some(KbStop::OriginEditor) | None => ctx.focus(&self.origin_editor),
            Some(_) => ctx.focus_self(),
        }
        ctx.notify();
    }

    /// Called by the settings view (Tab, or the command palette entry): enters the ring at the
    /// origin editor.
    pub(crate) fn kb_enter(&mut self, ctx: &mut ViewContext<Self>) {
        self.kb_goto(0, ctx);
    }

    /// Keeps one mouse state per listed origin and per log row.
    fn resize_row_states(&mut self, ctx: &mut ViewContext<Self>) {
        let origins = parse_origin_list(NetworkSettings::as_ref(ctx).allowed_origins.value());
        self.origin_remove_states
            .resize_with(origins.len(), MouseStateHandle::default);
        let rows = NetworkGateLog::as_ref(ctx).entries_newest_first().len();
        self.log_allow_states
            .resize_with(rows, MouseStateHandle::default);
    }

    /// Pushes the current NetworkSettings values into the dropdown and the three non-password editors.
    fn sync_all_from_settings(me: &mut Self, ctx: &mut ViewContext<Self>) {
        me.resize_row_states(ctx);
        let net = NetworkSettings::as_ref(ctx);
        let mode = *net.proxy_mode.value();
        let url = net.proxy_url.value().clone();
        let username = net.proxy_username.value().clone();
        let no_proxy = net.proxy_no_proxy.value().clone();

        // The dropdown's options follow the mode.
        let label: String = match mode {
            ProxyMode::Off => crate::t!("settings-network-mode-off"),
            ProxyMode::System => crate::t!("settings-network-mode-system"),
            ProxyMode::Custom => crate::t!("settings-network-mode-custom"),
        };
        me.mode_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_selected_by_name(label, ctx);
        });

        // The editor buffer follows the setting value; also switches InteractionState by mode.
        let editable = matches!(mode, ProxyMode::Custom);
        set_editor_text_and_state(&me.url_editor, &url, editable, ctx);
        set_editor_text_and_state(&me.username_editor, &username, editable, ctx);
        set_editor_text_and_state(&me.no_proxy_editor, &no_proxy, editable, ctx);

        // The password also switches interaction state by mode (its buffer is refreshed
        // separately by the ProxyCredentials subscription).
        me.password_editor.update(ctx, |editor, ctx| {
            editor.set_interaction_state(
                if editable {
                    InteractionState::Editable
                } else {
                    InteractionState::Disabled
                },
                ctx,
            );
        });
    }

    /// Pushes the current password into the password editor (managed separately by ProxyCredentials).
    ///
    /// The stored password loads off the UI thread (#1805): until it lands the placeholder says
    /// it is loading, and if the keyring could not be read it says so instead of looking empty.
    /// The load emits `PasswordChanged`, which re-enters here with the value.
    fn sync_password_from_credentials(me: &mut Self, ctx: &mut ViewContext<Self>) {
        ProxyCredentials::handle(ctx).update(ctx, |creds, ctx| creds.ensure_loaded(ctx));
        let creds = ProxyCredentials::as_ref(ctx);
        let pw = creds.password().to_string();
        let placeholder = match creds.load_state() {
            ProxyPasswordLoad::NotStarted | ProxyPasswordLoad::Loading => {
                crate::t!("settings-network-password-loading-placeholder")
            }
            ProxyPasswordLoad::Unavailable => {
                crate::t!("settings-network-password-unavailable-placeholder")
            }
            ProxyPasswordLoad::Loaded => crate::t!("settings-network-password-placeholder"),
        };
        me.password_editor.update(ctx, |editor, ctx| {
            editor.set_placeholder_text(placeholder, ctx);
            editor.set_buffer_text(&pw, ctx);
        });
    }
}

impl Entity for NetworkPageView {
    type Event = SettingsPageEvent;
}

impl TypedActionView for NetworkPageView {
    type Action = NetworkPageAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            NetworkPageAction::ToggleGate => {
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    let on = *settings.gate_enabled.value();
                    log::info!("hop=network_gate_ui action=toggle_gate on={}", !on);
                    report_if_error!(settings.gate_enabled.set_value(!on, ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::AddOrigin => {
                let text = self.origin_editor.as_ref(ctx).buffer_text(ctx);
                let current = NetworkSettings::as_ref(ctx).allowed_origins.value().clone();
                if let Some(next) = add_origin(&current, &text) {
                    log_ui_hop("add_origin", text.trim());
                    NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                        report_if_error!(settings.allowed_origins.set_value(next, ctx));
                    });
                    self.origin_editor
                        .update(ctx, |editor, ctx| editor.set_buffer_text("", ctx));
                }
                ctx.notify();
            }
            NetworkPageAction::RemoveOrigin(origin) => {
                let current = NetworkSettings::as_ref(ctx).allowed_origins.value().clone();
                let next = remove_origin(&current, origin);
                log_ui_hop("remove_origin", origin);
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.allowed_origins.set_value(next, ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::AllowOrigin(origin) => {
                log_ui_hop("allow_from_log", origin);
                allow_origin(origin, ctx);
                ctx.notify();
            }
            NetworkPageAction::KbNext | NetworkPageAction::KbPrev => {
                let n = self.current_stops(ctx).len();
                let dir = if matches!(action, NetworkPageAction::KbNext) {
                    1
                } else {
                    -1
                };
                let next = kb_step(self.kb_stop, n, dir);
                self.kb_goto(next, ctx);
            }
            NetworkPageAction::KbActivate | NetworkPageAction::KbRemove => {
                let stops = self.current_stops(ctx);
                let remove_only = matches!(action, NetworkPageAction::KbRemove);
                let stop = self.kb_stop.and_then(|i| stops.get(i));
                if let Some(next) = stop.and_then(|s| kb_action_for(s, remove_only)) {
                    self.handle_action(&next, ctx);
                }
            }
            NetworkPageAction::KbLeave => {
                self.kb_stop = None;
                ctx.emit(SettingsPageEvent::FocusModal);
                ctx.notify();
            }
            NetworkPageAction::ClearGateLog => {
                NetworkGateLog::handle(ctx).update(ctx, |log, ctx| log.clear(ctx));
                ctx.notify();
            }
            NetworkPageAction::SetProxyMode(mode) => {
                let mode = *mode;
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.proxy_mode.set_value(mode, ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::SaveProxyUrl => {
                let value = self.url_editor.as_ref(ctx).buffer_text(ctx);
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.proxy_url.set_value(value, ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::ClearProxyUrl => {
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.proxy_url.set_value(String::new(), ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::SaveProxyUsername => {
                let value = self.username_editor.as_ref(ctx).buffer_text(ctx);
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.proxy_username.set_value(value, ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::ClearProxyUsername => {
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.proxy_username.set_value(String::new(), ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::SaveProxyPassword => {
                let value = self.password_editor.as_ref(ctx).buffer_text(ctx);
                ProxyCredentials::handle(ctx).update(ctx, |creds, ctx| {
                    creds.set_password(value, ctx);
                });
                ctx.notify();
            }
            NetworkPageAction::ClearProxyPassword => {
                ProxyCredentials::handle(ctx).update(ctx, |creds, ctx| {
                    creds.set_password(String::new(), ctx);
                });
                ctx.notify();
            }
            NetworkPageAction::SaveProxyNoProxy => {
                let value = self.no_proxy_editor.as_ref(ctx).buffer_text(ctx);
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.proxy_no_proxy.set_value(value, ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::ClearProxyNoProxy => {
                NetworkSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.proxy_no_proxy.set_value(String::new(), ctx));
                });
                ctx.notify();
            }
            NetworkPageAction::TestConnection => {
                self.test_state = TestState::Running;
                ctx.notify();

                // Decides the test strategy based on the current mode:
                //   Custom -> probes the proxy host:port over TCP (proxy connectivity, unrelated
                //   to outbound access, suitable for corporate intranet proxies)
                //   System / Off -> HTTP GET probe of a public URL (outbound connectivity)
                let mode = *NetworkSettings::as_ref(ctx).proxy_mode.value();
                let proxy_url = NetworkSettings::as_ref(ctx).proxy_url.value().clone();
                spawn_test_connection(self, mode, proxy_url, ctx);
            }
            NetworkPageAction::TestConnectionResult(outcome) => {
                self.test_state = match &outcome.result {
                    Ok(latency_ms) => TestState::Success {
                        kind: outcome.kind,
                        latency_ms: *latency_ms,
                    },
                    Err(msg) => TestState::Failed {
                        kind: outcome.kind,
                        message: msg.clone(),
                    },
                };
                ctx.notify();
            }
        }
    }
}

impl View for NetworkPageView {
    fn ui_name() -> &'static str {
        "NetworkPage"
    }

    fn keymap_context(&self, app: &AppContext) -> warpui::keymap::Context {
        let mut context = Self::default_keymap_context();
        let on_button = self
            .kb_stop
            .and_then(|i| self.current_stops(app).into_iter().nth(i))
            .is_some_and(|s| s != KbStop::OriginEditor);
        if on_button {
            context.set.insert(KB_BUTTON_FLAG);
        }
        context
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for NetworkPageView {
    fn section() -> SettingsSection {
        SettingsSection::Network
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        true
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id);
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<NetworkPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<NetworkPageView>) -> Self {
        SettingsPageViewHandle::Network(view_handle)
    }
}

/// Chooses the detection method based on mode, spawns it to run in the background, and
/// returns the result to the main thread via an action.
fn spawn_test_connection(
    _view: &NetworkPageView,
    mode: ProxyMode,
    proxy_url: String,
    ctx: &mut ViewContext<NetworkPageView>,
) {
    let timeout = Duration::from_secs(TEST_CONNECTION_TIMEOUT_SECS);

    match mode {
        ProxyMode::Custom => {
            // The user-filled proxy: after parsing, probes host:port over TCP.
            let Some((host, port)) = parse_host_port(&proxy_url) else {
                ctx.spawn(
                    async move {
                        TestOutcome {
                            kind: TestKind::Tcp,
                            result: Err("invalid proxy URL".to_string()),
                        }
                    },
                    |me, outcome, ctx| {
                        me.handle_action(&NetworkPageAction::TestConnectionResult(outcome), ctx);
                    },
                );
                return;
            };
            spawn_tcp_probe(host, port, timeout, ctx);
        }
        ProxyMode::System => {
            // Prefers reading the system proxy from environment variables (a minimal
            // cross-platform set); if that succeeds, does a TCP probe; if not (macOS
            // SCDynamicStore / Windows WinINET are only used internally by reqwest),
            // falls back to an HTTP probe of a public URL.
            let (sys_https, sys_http, _) = read_system_proxy_env();
            let sys_proxy = if !sys_https.is_empty() {
                sys_https
            } else {
                sys_http
            };
            if let Some((host, port)) = parse_host_port(&sys_proxy) {
                spawn_tcp_probe(host, port, timeout, ctx);
            } else {
                spawn_http_probe(timeout, ctx);
            }
        }
        ProxyMode::Off => {
            // Off mode has no proxy to test, so it tests whether a "direct outbound connection" works.
            spawn_http_probe(timeout, ctx);
        }
    }
}

/// The synchronous TCP-probe logic factored out as a helper, reusable by both the Custom and System paths.
fn spawn_tcp_probe(
    host: String,
    port: u16,
    timeout: Duration,
    ctx: &mut ViewContext<NetworkPageView>,
) {
    ctx.spawn(
        async move {
            let start = Instant::now();
            let addr = format!("{host}:{port}");
            let result = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await;
            let outcome_result = match result {
                Ok(Ok(_stream)) => Ok(start.elapsed().as_millis()),
                Ok(Err(e)) => Err(format!("{e}")),
                Err(_) => Err(format!("timeout after {}s", timeout.as_secs())),
            };
            TestOutcome {
                kind: TestKind::Tcp,
                result: outcome_result,
            }
        },
        |me, outcome, ctx| {
            me.handle_action(&NetworkPageAction::TestConnectionResult(outcome), ctx);
        },
    );
}

/// The HTTP probe logic (goes through reqwest's global proxy settings). Only used for Off, or as a System fallback.
fn spawn_http_probe(timeout: Duration, ctx: &mut ViewContext<NetworkPageView>) {
    let client = Arc::new(http_client::Client::for_purpose(
        &http_client::gate::NETWORK_PROBE,
    ));
    let target = PUBLIC_PROBE_URL.to_string();
    ctx.spawn(
        async move {
            let start = Instant::now();
            let outcome_result = match client.get(&target).timeout(timeout).send().await {
                Ok(resp) => {
                    if resp.status().is_success() || resp.status().as_u16() == 204 {
                        Ok(start.elapsed().as_millis())
                    } else {
                        Err(format!("HTTP {}", resp.status().as_u16()))
                    }
                }
                Err(err) => Err(format!("{err:#}")),
            };
            TestOutcome {
                kind: TestKind::Http,
                result: outcome_result,
            }
        },
        |me, outcome, ctx| {
            me.handle_action(&NetworkPageAction::TestConnectionResult(outcome), ctx);
        },
    );
}

/// Extracts host + port from a "rough" proxy URL.
/// Supports the following inputs:
///   - `http://host:port`
///   - `https://host:port`
///   - `socks5://host:port`
///   - `host:port` (no scheme)
/// Returns `None` if it cannot be parsed.
fn parse_host_port(raw: &str) -> Option<(String, u16)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // If there's a scheme, first try parsing with url::Url; otherwise prepend `http://` and
    // parse.
    let normalized: String = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let url = url::Url::parse(&normalized).ok()?;
    let host = url.host_str()?.to_string();
    let port = url.port_or_known_default()?;
    Some((host, port))
}

/// A settings editor bounded to its cell: the paint is clipped to the field, so a line longer
/// than the field is not drawn over the label beside it (#2147). Not an ellipsis: the editor
/// scrolls to follow the caret, so the whole value stays reachable by editing it.
fn clipped_editor(editor: &ViewHandle<EditorView>) -> Box<dyn Element> {
    Clipped::new(warpui::elements::ChildView::new(editor).finish()).finish()
}

/// Constructs a single-line EditorView, with an optional password mask.
fn build_text_editor(
    ctx: &mut ViewContext<NetworkPageView>,
    is_password: bool,
    placeholder: String,
) -> ViewHandle<EditorView> {
    build_text_editor_opts(ctx, is_password, placeholder, false)
}

/// `propagate_nav` makes the editor hand Tab / Shift-Tab to the page (`EditorEvent::Navigate`)
/// instead of consuming them, so the keyboard ring can leave it.
fn build_text_editor_opts(
    ctx: &mut ViewContext<NetworkPageView>,
    is_password: bool,
    placeholder: String,
    propagate_nav: bool,
) -> ViewHandle<EditorView> {
    ctx.add_typed_action_view(move |ctx| {
        let appearance = Appearance::as_ref(ctx);
        let options = SingleLineEditorOptions {
            is_password,
            propagate_and_no_op_vertical_navigation_keys: if propagate_nav {
                PropagateAndNoOpNavigationKeys::Always
            } else {
                PropagateAndNoOpNavigationKeys::Never
            },
            text: TextOptions {
                font_size_override: Some(appearance.ui_font_size()),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut editor = EditorView::single_line(options, ctx);
        editor.set_placeholder_text(placeholder, ctx);
        editor
    })
}

/// Writes the current value into the editor buffer, and switches InteractionState per `editable`.
/// Note: `set_buffer_text` resets the cursor, and should not be called while the user is
/// focused editing -- this function is only used when settings change externally.
fn set_editor_text_and_state(
    editor: &ViewHandle<EditorView>,
    value: &str,
    editable: bool,
    ctx: &mut ViewContext<NetworkPageView>,
) {
    editor.update(ctx, |editor, ctx| {
        // Skips the set if the buffer already equals the target value, to avoid an unnecessary cursor reset.
        if editor.buffer_text(ctx) != value {
            editor.set_buffer_text(value, ctx);
        }
        editor.set_interaction_state(
            if editable {
                InteractionState::Editable
            } else {
                InteractionState::Disabled
            },
            ctx,
        );
    });
}

#[derive(Default)]
struct NetworkPageWidget;

impl SettingsWidget for NetworkPageWidget {
    type View = NetworkPageView;

    fn search_terms(&self) -> &str {
        "network proxy http https 代理 网络 vpn 公司 corporate system custom off no_proxy 测试连接 gate firewall allow allowed origins blocked denied log 闸门 防火墙 允许 来源 ゲート 許可 オリジン"
    }

    fn render(
        &self,
        view: &NetworkPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        // Note: the `_app` passed into SettingsWidget::render is the AppContext at render
        // time; it's needed to read the current mode. The parameter name is left as-is here
        // to avoid a file-wide rename; `_app` is used directly below.
        let page_title = crate::t!("settings-network-page-title");
        let header = crate::t!("settings-network-header");
        let description = crate::t!("settings-network-description");

        let mut content = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_child(render_page_title(&page_title, appearance));

        // The network gate section sits above the proxy settings.
        render_gate_section(&mut content, view, appearance, _app);

        content.add_child(render_sub_header_with_description(
            appearance,
            header,
            description,
        ));

        // 1. Mode dropdown -- always enabled
        content.add_child(render_body_item::<NetworkPageAction>(
            crate::t!("settings-network-mode-label"),
            None::<AdditionalInfo<NetworkPageAction>>,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            warpui::elements::ChildView::new(&view.mode_dropdown).finish(),
            Some(crate::t!("settings-network-mode-description")),
        ));

        // Field-render helper: an editor + Save button + Clear button, uniformly width-aligned.
        let render_field = |label: String,
                            description: String,
                            editor: &ViewHandle<EditorView>,
                            save_state: &MouseStateHandle,
                            clear_state: &MouseStateHandle,
                            save_action: NetworkPageAction,
                            clear_action: NetworkPageAction|
         -> Box<dyn Element> {
            let editor_element = clipped_editor(editor);
            // Note: don't put `margin` into the button's `UiComponentStyles`.
            // Inside `WrappableText::build()` (`Span::new(text, styles).build()`),
            // the same `styles.margin` gets applied to the label container too, causing the
            // label **inside** the button to also get pushed left by the same amount, which
            // visually looks like "text shifted right".
            // Here, an outer Container is used instead to set the horizontal spacing between
            // the button and the editor/adjacent button.
            let save_button = Container::new(
                appearance
                    .ui_builder()
                    .button(ButtonVariant::Accent, save_state.clone())
                    .with_style(UiComponentStyles {
                        font_size: Some(appearance.ui_font_body()),
                        padding: Some(Coords::uniform(BUTTON_PADDING)),
                        ..Default::default()
                    })
                    .with_text_label(crate::t!("settings-network-save"))
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(save_action.clone());
                    })
                    .finish(),
            )
            .with_margin_left(6.)
            .finish();
            let clear_button = Container::new(
                appearance
                    .ui_builder()
                    .button(ButtonVariant::Text, clear_state.clone())
                    .with_style(UiComponentStyles {
                        font_size: Some(appearance.ui_font_body()),
                        padding: Some(Coords::uniform(BUTTON_PADDING)),
                        ..Default::default()
                    })
                    .with_text_label(crate::t!("settings-network-clear"))
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(clear_action.clone());
                    })
                    .finish(),
            )
            .with_margin_left(4.)
            .finish();

            let input_area = ConstrainedBox::new(
                Flex::row()
                    .with_main_axis_size(MainAxisSize::Min)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(
                        // The editor takes up the remaining space, placed inside a
                        // finite-width container (avoiding issues with internal flex under
                        // unbounded constraints).
                        ConstrainedBox::new(editor_element)
                            .with_max_width(INPUT_AREA_MAX_WIDTH - 120.0)
                            .finish(),
                    )
                    .with_child(save_button)
                    .with_child(clear_button)
                    .finish(),
            )
            .with_max_width(INPUT_AREA_MAX_WIDTH)
            .finish();

            render_body_item::<NetworkPageAction>(
                label,
                None::<AdditionalInfo<NetworkPageAction>>,
                LocalOnlyIconState::Hidden,
                ToggleState::Enabled,
                appearance,
                input_area,
                Some(description),
            )
        };

        // 2. URL
        content.add_child(render_field(
            crate::t!("settings-network-url-label"),
            crate::t!("settings-network-url-description"),
            &view.url_editor,
            &view.url_save_state,
            &view.url_clear_state,
            NetworkPageAction::SaveProxyUrl,
            NetworkPageAction::ClearProxyUrl,
        ));

        // 3. Username
        content.add_child(render_field(
            crate::t!("settings-network-username-label"),
            crate::t!("settings-network-username-description"),
            &view.username_editor,
            &view.username_save_state,
            &view.username_clear_state,
            NetworkPageAction::SaveProxyUsername,
            NetworkPageAction::ClearProxyUsername,
        ));

        // 4. Password
        content.add_child(render_field(
            crate::t!("settings-network-password-label"),
            crate::t!("settings-network-password-description"),
            &view.password_editor,
            &view.password_save_state,
            &view.password_clear_state,
            NetworkPageAction::SaveProxyPassword,
            NetworkPageAction::ClearProxyPassword,
        ));

        // 5. no_proxy
        content.add_child(render_field(
            crate::t!("settings-network-no-proxy-label"),
            crate::t!("settings-network-no-proxy-description"),
            &view.no_proxy_editor,
            &view.no_proxy_save_state,
            &view.no_proxy_clear_state,
            NetworkPageAction::SaveProxyNoProxy,
            NetworkPageAction::ClearProxyNoProxy,
        ));

        // 6. Test connection -- same style as the Save button above.
        let mut test_button = appearance
            .ui_builder()
            .button(ButtonVariant::Accent, view.test_button_state.clone())
            .with_style(UiComponentStyles {
                font_size: Some(appearance.ui_font_body()),
                padding: Some(Coords::uniform(BUTTON_PADDING)),
                ..Default::default()
            })
            .with_centered_text_label(crate::t!("settings-network-test-button"))
            .build()
            .on_click(|ctx, _, _| {
                ctx.dispatch_typed_action(NetworkPageAction::TestConnection);
            });
        if matches!(view.test_state, TestState::Running) {
            test_button = test_button.disable();
        }

        // The Idle hint copy needs to match the current mode: Custom tests proxy connectivity,
        // System/Off tests outbound connectivity.
        let mode = *NetworkSettings::as_ref(_app).proxy_mode.value();
        let result_text: String = match &view.test_state {
            TestState::Idle => match mode {
                ProxyMode::Custom => crate::t!("settings-network-test-idle-tcp"),
                ProxyMode::System | ProxyMode::Off => {
                    crate::t!("settings-network-test-idle-http", url = PUBLIC_PROBE_URL)
                }
            },
            TestState::Running => crate::t!("settings-network-test-running"),
            TestState::Success { kind, latency_ms } => match kind {
                TestKind::Tcp => crate::t!(
                    "settings-network-test-success-tcp",
                    latency = (*latency_ms as i64)
                ),
                TestKind::Http => crate::t!(
                    "settings-network-test-success-http",
                    latency = (*latency_ms as i64)
                ),
            },
            TestState::Failed { kind, message } => match kind {
                TestKind::Tcp => {
                    crate::t!("settings-network-test-failed-tcp", error = message.clone())
                }
                TestKind::Http => {
                    crate::t!("settings-network-test-failed-http", error = message.clone())
                }
            },
        };
        let result_element = Text::new(
            result_text,
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(appearance.theme().nonactive_ui_text_color().into())
        .with_style(Properties::default().weight(Weight::Normal))
        .finish();

        // Wraps the outer layer in Align(left) to prevent the parent Flex from stretching the
        // button taller on the cross-axis;
        // the inner Flex::row uses MainAxisSize::Min, taking up only the width actually needed.
        content.add_child(
            Container::new(
                Align::new(
                    Flex::row()
                        .with_main_axis_size(MainAxisSize::Min)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_main_axis_alignment(MainAxisAlignment::Start)
                        .with_child(test_button.finish())
                        .with_child(
                            Container::new(result_element)
                                .with_padding_left(12.)
                                .finish(),
                        )
                        .finish(),
                )
                .left()
                .finish(),
            )
            .with_margin_top(20.)
            .finish(),
        );

        content.finish()
    }
}

fn plain_text(
    text: String,
    color: pathfinder_color::ColorU,
    appearance: &Appearance,
) -> Box<dyn Element> {
    Text::new(text, appearance.ui_font_family(), appearance.ui_font_size())
        .with_color(color)
        .with_style(Properties::default().weight(Weight::Normal))
        .finish()
}

fn text_button(
    label: String,
    variant: ButtonVariant,
    state: &MouseStateHandle,
    action: NetworkPageAction,
    appearance: &Appearance,
) -> Box<dyn Element> {
    Container::new(
        appearance
            .ui_builder()
            .button(variant, state.clone())
            .with_style(UiComponentStyles {
                font_size: Some(appearance.ui_font_body()),
                padding: Some(Coords::uniform(BUTTON_PADDING)),
                ..Default::default()
            })
            .with_text_label(label)
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(action.clone());
            })
            .finish(),
    )
    .with_margin_left(6.)
    .finish()
}

/// "> " in front of the row the keyboard ring is on, else nothing.
fn kb_marker(view: &NetworkPageView, app: &AppContext, stop: &KbStop) -> &'static str {
    let here = view
        .kb_stop
        .and_then(|i| view.current_stops(app).into_iter().nth(i));
    if here.as_ref() == Some(stop) {
        "> "
    } else {
        ""
    }
}

/// One log row's text: time, outcome, purpose, host:port.
fn log_row_text(entry: &LoggedGateEvent) -> String {
    use http_client::gate::Outcome;
    let outcome = match entry.event.outcome {
        Outcome::Allowed => crate::t!("settings-network-gate-outcome-allowed"),
        Outcome::Denied => crate::t!("settings-network-gate-outcome-denied"),
        Outcome::AllowedGateOff => crate::t!("settings-network-gate-outcome-gate-off"),
    };
    let target = match (entry.event.host.is_empty(), entry.event.port) {
        (true, _) => "-".to_owned(),
        (false, Some(port)) => format!("{}:{port}", entry.event.host),
        (false, None) => entry.event.host.clone(),
    };
    format!(
        "{}  {}  {}  {}",
        entry.at.format("%H:%M:%S"),
        outcome,
        entry.event.purpose,
        target
    )
}

fn render_gate_section(
    content: &mut Flex,
    view: &NetworkPageView,
    appearance: &Appearance,
    app: &AppContext,
) {
    let theme = appearance.theme();
    let muted: pathfinder_color::ColorU = theme.nonactive_ui_text_color().into();
    let gate_on = *NetworkSettings::as_ref(app).gate_enabled.value();

    content.add_child(render_sub_header_with_description(
        appearance,
        crate::t!("settings-network-gate-header"),
        format!(
            "{} {}",
            crate::t!("settings-network-gate-description"),
            crate::t!("settings-network-gate-mode-note")
        ),
    ));

    // The switch.
    let switch = appearance
        .ui_builder()
        .switch(view.gate_switch_state.clone())
        .check(gate_on)
        .build()
        .on_click(|ctx, _, _| ctx.dispatch_typed_action(NetworkPageAction::ToggleGate))
        .finish();
    content.add_child(render_body_item::<NetworkPageAction>(
        crate::t!("settings-network-gate-toggle-label"),
        None::<AdditionalInfo<NetworkPageAction>>,
        LocalOnlyIconState::Hidden,
        ToggleState::Enabled,
        appearance,
        switch,
        Some(crate::t!("settings-network-gate-toggle-description")),
    ));
    if !gate_on {
        content.add_child(
            Container::new(plain_text(
                crate::t!("settings-network-gate-off-warning"),
                theme.ui_error_color().into(),
                appearance,
            ))
            .with_margin_bottom(8.)
            .finish(),
        );
    }

    // Allowed origins: one row per origin with Remove, then the input and Add.
    let origins = parse_origin_list(NetworkSettings::as_ref(app).allowed_origins.value());
    let mut list = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Start);
    if origins.is_empty() {
        list.add_child(plain_text(
            crate::t!("settings-network-origins-empty"),
            muted,
            appearance,
        ));
    }
    for (i, origin) in origins.iter().enumerate() {
        let state = view
            .origin_remove_states
            .get(i)
            .cloned()
            .unwrap_or_default();
        list.add_child(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Min)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(plain_text(
                    format!(
                        "{}{origin}",
                        kb_marker(view, app, &KbStop::Remove(origin.clone()))
                    ),
                    theme.active_ui_text_color().into(),
                    appearance,
                ))
                .with_child(text_button(
                    crate::t!("settings-network-origins-remove"),
                    ButtonVariant::Text,
                    &state,
                    NetworkPageAction::RemoveOrigin(origin.clone()),
                    appearance,
                ))
                .finish(),
        );
    }
    list.add_child(
        Container::new(
            ConstrainedBox::new(
                Flex::row()
                    .with_main_axis_size(MainAxisSize::Min)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(
                        ConstrainedBox::new(clipped_editor(&view.origin_editor))
                            .with_max_width(INPUT_AREA_MAX_WIDTH - 80.0)
                            .finish(),
                    )
                    .with_child(text_button(
                        crate::t!("settings-network-origins-add"),
                        ButtonVariant::Accent,
                        &view.origin_add_state,
                        NetworkPageAction::AddOrigin,
                        appearance,
                    ))
                    .finish(),
            )
            .with_max_width(INPUT_AREA_MAX_WIDTH)
            .finish(),
        )
        .with_margin_top(4.)
        .finish(),
    );
    content.add_child(render_body_item::<NetworkPageAction>(
        crate::t!("settings-network-origins-label"),
        None::<AdditionalInfo<NetworkPageAction>>,
        LocalOnlyIconState::Hidden,
        ToggleState::Enabled,
        appearance,
        list.finish(),
        Some(crate::t!("settings-network-origins-description")),
    ));

    // The decision log, newest first.
    let entries = NetworkGateLog::as_ref(app).entries_newest_first();
    let mut log = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Start);
    log.add_child(text_button(
        crate::t!("settings-network-gate-log-clear"),
        ButtonVariant::Text,
        &view.gate_log_clear_state,
        NetworkPageAction::ClearGateLog,
        appearance,
    ));
    if entries.is_empty() {
        log.add_child(plain_text(
            crate::t!("settings-network-gate-log-empty"),
            muted,
            appearance,
        ));
    }
    for (i, entry) in entries.iter().enumerate() {
        let mut row = Flex::row()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(plain_text(
                format!(
                    "{}{}",
                    crate::network_gate_log::allow_text(&entry.event)
                        .filter(|_| entry.event.outcome == http_client::gate::Outcome::Denied)
                        .map(|o| kb_marker(view, app, &KbStop::Allow(o)))
                        .unwrap_or(""),
                    log_row_text(entry)
                ),
                theme.active_ui_text_color().into(),
                appearance,
            ));
        if entry.event.outcome == http_client::gate::Outcome::Denied {
            if let Some(origin) = crate::network_gate_log::allow_text(&entry.event) {
                let state = view.log_allow_states.get(i).cloned().unwrap_or_default();
                row.add_child(text_button(
                    crate::t!("settings-network-gate-log-allow"),
                    ButtonVariant::Accent,
                    &state,
                    NetworkPageAction::AllowOrigin(origin),
                    appearance,
                ));
            }
        }
        log.add_child(row.finish());
    }
    content.add_child(render_body_item::<NetworkPageAction>(
        crate::t!("settings-network-gate-log-label"),
        None::<AdditionalInfo<NetworkPageAction>>,
        LocalOnlyIconState::Hidden,
        ToggleState::Enabled,
        appearance,
        log.finish(),
        Some(crate::t!("settings-network-gate-log-description")),
    ));
}

#[cfg(test)]
#[path = "network_page_tests.rs"]
mod tests;

/// Key bindings of the page's keyboard ring (see `KbStop`). Tab / Shift-Tab and Escape work from
/// the origin editor too (it propagates them as `EditorEvent`s); Enter, Space, Delete and
/// Backspace are bound only while the ring sits on a Remove / Allow button.
pub(crate) fn register_bindings(app: &mut warpui::AppContext) {
    use warpui::id;
    use warpui::keymap::FixedBinding;
    let page = id!("NetworkPage") & !id!("IMEOpen");
    let on_button = page.clone() & id!(KB_BUTTON_FLAG);
    app.register_fixed_bindings([
        FixedBinding::new("tab", NetworkPageAction::KbNext, page.clone()),
        FixedBinding::new("shift-tab", NetworkPageAction::KbPrev, page.clone()),
        FixedBinding::new("escape", NetworkPageAction::KbLeave, page),
        FixedBinding::new("enter", NetworkPageAction::KbActivate, on_button.clone()),
        FixedBinding::new("space", NetworkPageAction::KbActivate, on_button.clone()),
        FixedBinding::new("delete", NetworkPageAction::KbRemove, on_button.clone()),
        FixedBinding::new("backspace", NetworkPageAction::KbRemove, on_button),
    ]);
}
