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
    elements::{
        Align, ConstrainedBox, Container, CrossAxisAlignment, Element, Flex, MainAxisAlignment,
        MainAxisSize, MouseStateHandle, ParentElement, Text,
    },
    fonts::{Properties, Weight},
    ui_components::{
        button::ButtonVariant,
        components::{Coords, UiComponent, UiComponentStyles},
    },
    AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use super::settings_page::{
    render_body_item, render_page_title, render_sub_header_with_description, AdditionalInfo,
    LocalOnlyIconState, MatchData, PageType, SettingsPageEvent, SettingsPageMeta,
    SettingsPageViewHandle, SettingsWidget, ToggleState,
};
use super::SettingsSection;
use crate::appearance::Appearance;
use crate::editor::{EditorView, InteractionState, SingleLineEditorOptions, TextOptions};
use crate::report_if_error;
use crate::settings::network::{NetworkSettings, ProxyMode};
use crate::settings::network_secrets::ProxyCredentials;
use crate::view_components::dropdown::{Dropdown, DropdownItem};

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

#[derive(Debug, Clone)]
pub enum NetworkPageAction {
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
        };

        // Does an initial sync so the dropdown and each editor show the currently saved values.
        Self::sync_all_from_settings(&mut me, ctx);
        Self::sync_password_from_credentials(&mut me, ctx);
        me
    }

    /// Pushes the current NetworkSettings values into the dropdown and the three non-password editors.
    fn sync_all_from_settings(me: &mut Self, ctx: &mut ViewContext<Self>) {
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
    fn sync_password_from_credentials(me: &mut Self, ctx: &mut ViewContext<Self>) {
        let pw = ProxyCredentials::as_ref(ctx).password().to_string();
        me.password_editor.update(ctx, |editor, ctx| {
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
    let client = Arc::new(http_client::Client::new());
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

/// Constructs a single-line EditorView, with an optional password mask.
fn build_text_editor(
    ctx: &mut ViewContext<NetworkPageView>,
    is_password: bool,
    placeholder: String,
) -> ViewHandle<EditorView> {
    ctx.add_typed_action_view(move |ctx| {
        let appearance = Appearance::as_ref(ctx);
        let options = SingleLineEditorOptions {
            is_password,
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
        "network proxy http https 代理 网络 vpn 公司 corporate system custom off no_proxy 测试连接"
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
            .with_child(render_page_title(&page_title, appearance))
            .with_child(render_sub_header_with_description(
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
            let editor_element = warpui::elements::ChildView::new(editor).finish();
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
