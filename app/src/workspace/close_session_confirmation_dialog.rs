use pathfinder_geometry::vector::vec2f;
use warp_core::ui::theme::Fill;
use warpui::{
    elements::{
        Align, ChildAnchor, Container, MouseStateHandle, OffsetPositioning, ParentAnchor,
        ParentOffsetBounds, Stack,
    },
    fonts::Weight,
    platform::Cursor,
    ui_components::{
        button::ButtonVariant,
        components::{Coords, UiComponent, UiComponentStyles},
        text::Span,
    },
    AppContext, Element, Entity, EntityId, SingletonEntity, TypedActionView, View, ViewContext,
    keymap::FixedBinding,
};

use crate::{
    appearance::Appearance,
    pane_group::PaneId,
    ui_components::dialog::{dialog_styles, Dialog},
    workspace::TabMovement,
};

#[allow(clippy::enum_variant_names)]
#[derive(Copy, Clone)]
/// Describes the action which opened the close session confirmation dialog
pub enum OpenDialogSource {
    /// Close a specific pane
    ClosePane {
        pane_group_id: EntityId,
        pane_id: PaneId,
    },
    /// Close a specific tab
    CloseTab { tab_index: usize },
    /// Close all tabs other than the tab_index
    CloseOtherTabs { tab_index: usize },
    /// Close all tabs to the right/left of tab_index
    CloseTabsDirection {
        tab_index: usize,
        direction: TabMovement,
    },
}

/// Which question the dialog is asking.
///
/// The two cases are not wording variants of one question: they have different
/// consequences, a different number of answers, and different rules about what
/// may be remembered. Keeping them one enum rather than two dialogs is what lets
/// `Workspace` keep a single dialog view and a single event handler.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CloseSessionConfirmationKind {
    /// The tab holds a session that is being shared with other people; closing
    /// it ends the share for everyone. Two answers, and "don't show again" is
    /// offered -- see [`CloseSessionConfirmationAction`] for why only here.
    #[default]
    SharedSession,
    /// The tab holds a daemon-owned remote session (`remote_server_tty`) with a
    /// command still running. Closing it the way a local tab closes sends
    /// `Message::Kill` -> `SignalSession`/`Kill` and destroys the session on the
    /// host; not sending that leaves the daemon holding the pty. Three answers,
    /// and nothing is remembered.
    RunningRemoteSession,
}

/// Binds `escape` to [`CloseSessionConfirmationAction::Cancel`].
///
/// The dialog was mouse-only until this existed: no keybinding referenced any of
/// its actions and nothing set an `on_dismiss`, so the only way past it was to
/// hit a button. That is a poor modal in general and a worse one here, where the
/// remote variant offers three answers and the reflex for "I did not mean to do
/// this" is Escape.
///
/// Escape maps to Cancel, never to either close. A dialog that asks whether to
/// destroy something on another machine must not treat an ambiguous dismissal as
/// an answer -- and of the three, Cancel is the only one that changes nothing.
///
/// Follows `lightbox_view::init`'s shape exactly, which is the fork's precedent
/// for a workspace-level overlay registering a fixed binding.
pub fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;
    let view_id = id!(CloseSessionConfirmationDialog::ui_name());
    app.register_fixed_bindings([FixedBinding::new(
        "escape",
        CloseSessionConfirmationAction::Cancel,
        view_id,
    )]);
}

pub struct CloseSessionConfirmationDialog {
    cancel_mouse_state: MouseStateHandle,
    confirm_mouse_state: MouseStateHandle,
    leave_running_mouse_state: MouseStateHandle,
    dont_show_again_mouse_state: MouseStateHandle,
    dont_show_again: bool,
    /// Which question is being asked. `SharedSession` is the default so that a
    /// dialog which somehow renders without being opened shows the historical
    /// two-answer form rather than offering to leave a session running that may
    /// not exist.
    kind: CloseSessionConfirmationKind,
    // Source will be None if dialog was never opened, since there is no reasonable default
    open_confirmation_source: Option<OpenDialogSource>,
}

#[allow(dead_code)]
impl CloseSessionConfirmationDialog {
    pub fn new() -> Self {
        Self {
            cancel_mouse_state: Default::default(),
            confirm_mouse_state: Default::default(),
            leave_running_mouse_state: Default::default(),
            dont_show_again_mouse_state: Default::default(),
            open_confirmation_source: None,
            dont_show_again: false,
            kind: CloseSessionConfirmationKind::default(),
        }
    }
    pub fn set_open_confirmation_source(&mut self, source: OpenDialogSource) {
        self.open_confirmation_source = Some(source);
    }

    pub fn get_open_confirmation_source(&self) -> Option<OpenDialogSource> {
        self.open_confirmation_source
    }

    pub fn set_confirmation_kind(&mut self, kind: CloseSessionConfirmationKind) {
        self.kind = kind;
        // A stale tick must not carry across: the checkbox is only reachable in
        // the `SharedSession` form, and a `true` left over from a cancelled
        // shared-session prompt would otherwise be read by the next
        // `CloseSession` this dialog emits.
        if kind != CloseSessionConfirmationKind::SharedSession {
            self.dont_show_again = false;
        }
    }

    pub fn confirmation_kind(&self) -> CloseSessionConfirmationKind {
        self.kind
    }
}

impl Entity for CloseSessionConfirmationDialog {
    type Event = CloseSessionConfirmationEvent;
}

impl CloseSessionConfirmationDialog {
    /// The historical two-answer dialog: cancel, or close and end the share.
    fn render_shared_session_dialog(&self, appearance: &Appearance) -> Box<dyn Element> {
        let button_style = UiComponentStyles {
            font_size: Some(14.),
            font_weight: Some(Weight::Bold),
            width: Some(202.),
            height: Some(40.),
            ..Default::default()
        };

        let dont_show_again_checkbox = appearance
            .ui_builder()
            .checkbox(self.dont_show_again_mouse_state.clone(), Some(14.))
            .with_label(Span::new(
                crate::t!("common-dont-show-again-with-period"),
                Default::default(),
            ))
            .check(self.dont_show_again)
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(|ctx, _, _| {
                ctx.dispatch_typed_action(CloseSessionConfirmationAction::ToggleDontShowAgain)
            })
            .finish();

        let dont_show_again_value = self.dont_show_again;
        let close_session_button = appearance
            .ui_builder()
            .button(ButtonVariant::Accent, self.confirm_mouse_state.clone())
            .with_centered_text_label(crate::t!("workspace-close-session"))
            .with_style(button_style)
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(CloseSessionConfirmationAction::CloseSession {
                    dont_show_again: dont_show_again_value,
                })
            })
            .finish();

        let cancel_button = appearance
            .ui_builder()
            .button(ButtonVariant::Basic, self.cancel_mouse_state.clone())
            .with_centered_text_label(crate::t!("common-cancel"))
            .with_style(button_style)
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(CloseSessionConfirmationAction::Cancel)
            })
            .finish();

        Container::new(
            Dialog::new(
                "Close session?".into(),
                Some(
                    "You are about to close a session that is currently being shared. Closing it will end sharing for everyone."
                        .into(),
                ),
                UiComponentStyles {
                    width: Some(460.),
                    padding: Some(Coords::uniform(24.)),
                    ..dialog_styles(appearance)
                },
            )
            .with_child(dont_show_again_checkbox)
            .with_bottom_row_child(cancel_button)
            .with_bottom_row_child(close_session_button)
            .build()
            .finish()
        )
        .with_margin_top(35.)
        .finish()
    }

    /// The three-answer dialog for a daemon-owned remote session with a command
    /// still running: cancel, close and stop the session, or close and leave it
    /// running on the host.
    ///
    /// No "don't show again" checkbox, and that is deliberate rather than an
    /// omission -- see [`CloseSessionConfirmationAction`].
    ///
    /// The strings are literals, not `crate::t!` keys, matching the title and
    /// body of the shared-session form directly above, which are literals too.
    /// `t!` resolves its key against `app/i18n/en/warp.ftl` at compile time, so
    /// a new key here means editing that file; adding the keys is left to
    /// whoever localises this dialog, and would fail to build if faked.
    fn render_running_remote_session_dialog(&self, appearance: &Appearance) -> Box<dyn Element> {
        // Three buttons have to share the same 460pt dialog the two-button form
        // uses, so they are narrower than that form's 202pt and spaced with a
        // margin rather than by the row (the dialog's bottom row is a plain
        // `Flex::row` with no gap of its own).
        let button_style = UiComponentStyles {
            font_size: Some(14.),
            font_weight: Some(Weight::Bold),
            width: Some(128.),
            height: Some(40.),
            ..Default::default()
        };

        // Accent, and last: leaving the session running is the answer that
        // destroys nothing, and the destructive one should not be the one a
        // reflex click lands on.
        let leave_running_button = appearance
            .ui_builder()
            .button(
                ButtonVariant::Accent,
                self.leave_running_mouse_state.clone(),
            )
            .with_centered_text_label("Leave running".to_string())
            .with_style(button_style)
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(CloseSessionConfirmationAction::CloseAndLeaveRunning)
            })
            .finish();

        let stop_session_button = appearance
            .ui_builder()
            .button(ButtonVariant::Basic, self.confirm_mouse_state.clone())
            .with_centered_text_label("Stop session".to_string())
            .with_style(button_style)
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(CloseSessionConfirmationAction::CloseSession {
                    // Never remembered from this form: the checkbox that would
                    // set it is not rendered here.
                    dont_show_again: false,
                })
            })
            .finish();

        let cancel_button = appearance
            .ui_builder()
            .button(ButtonVariant::Basic, self.cancel_mouse_state.clone())
            .with_centered_text_label(crate::t!("common-cancel"))
            .with_style(button_style)
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(CloseSessionConfirmationAction::Cancel)
            })
            .finish();

        let spaced =
            |button: Box<dyn Element>| Container::new(button).with_margin_left(8.).finish();

        Container::new(
            Dialog::new(
                "A command is still running on the remote host".into(),
                Some(
                    "Closing this tab stops the remote session and whatever it is running. You can leave the session running on the host instead and reattach to it later."
                        .into(),
                ),
                UiComponentStyles {
                    width: Some(460.),
                    padding: Some(Coords::uniform(24.)),
                    ..dialog_styles(appearance)
                },
            )
            .with_bottom_row_child(cancel_button)
            .with_bottom_row_child(spaced(stop_session_button))
            .with_bottom_row_child(spaced(leave_running_button))
            .build()
            .finish()
        )
        .with_margin_top(35.)
        .finish()
    }
}

impl View for CloseSessionConfirmationDialog {
    fn ui_name() -> &'static str {
        "CloseSessionConfirmation"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);

        let dialog = match self.kind {
            CloseSessionConfirmationKind::SharedSession => {
                self.render_shared_session_dialog(appearance)
            }
            CloseSessionConfirmationKind::RunningRemoteSession => {
                self.render_running_remote_session_dialog(appearance)
            }
        };

        // Stack needed so that dialog can get bounds information,
        // specifically to ensure no overlap with the window's traffic lights
        let mut stack = Stack::new();
        stack.add_positioned_child(
            dialog,
            OffsetPositioning::offset_from_parent(
                vec2f(0., 0.),
                ParentOffsetBounds::WindowByPosition,
                ParentAnchor::Center,
                ChildAnchor::Center,
            ),
        );

        // This blurs the background and makes it uninteractable
        Container::new(Align::new(stack.finish()).finish())
            .with_background_color(Fill::blur().into())
            .with_corner_radius(app.windows().window_corner_radius())
            .finish()
    }
}

pub enum CloseSessionConfirmationEvent {
    /// Close, and tear the session down with it. For a daemon-owned remote
    /// session this is the answer that sends `Message::Kill`.
    CloseSession {
        dont_show_again: bool,
        open_confirmation_source: OpenDialogSource,
    },
    /// Close, but leave the session's pty alive on the host: the tab goes away
    /// and no `Message::Kill` is sent, so dropping the manager detaches and the
    /// daemon keeps the session.
    ///
    /// A separate variant rather than a flag on `CloseSession` because the two
    /// are different acts with different consequences, and because it carries no
    /// `dont_show_again`: there is nothing to remember here (see
    /// [`CloseSessionConfirmationAction`]).
    CloseAndLeaveRunning {
        open_confirmation_source: OpenDialogSource,
    },
    Cancel,
}

/// The three answers, plus the checkbox toggle.
///
/// **On `dont_show_again`.** It is carried by `CloseSession` and by nothing
/// else, and the checkbox that sets it is rendered only in the
/// [`CloseSessionConfirmationKind::SharedSession`] form. That is a decision, not
/// an oversight:
///
/// - The setting it writes (`SessionSettings::should_confirm_close_session`) is
///   a single global boolean. It cannot tell "stop asking me about ending a
///   share" apart from "stop asking me about killing a remote session", so a box
///   ticked once on a local shared-session prompt would silently answer the
///   remote question too -- and its silent answer would be *kill*, taking a
///   two-hour build with it. That is the exact failure the remote prompt exists
///   to prevent.
/// - With three answers there is no single thing to remember. "Don't ask again"
///   would have to mean one of *stop* or *leave running*, and a checkbox next to
///   three buttons does not say which.
///
/// So the remote form remembers nothing, and `Workspace` does not gate it on
/// `should_confirm_close_session` either: that setting was answered about a
/// different question.
#[derive(Debug)]
pub enum CloseSessionConfirmationAction {
    CloseSession { dont_show_again: bool },
    CloseAndLeaveRunning,
    Cancel,
    ToggleDontShowAgain,
}

impl TypedActionView for CloseSessionConfirmationDialog {
    type Action = CloseSessionConfirmationAction;

    fn handle_action(
        &mut self,
        action: &CloseSessionConfirmationAction,
        ctx: &mut ViewContext<Self>,
    ) {
        match action {
            CloseSessionConfirmationAction::CloseSession { dont_show_again } => {
                let Some(open_confirmation_source) = self.open_confirmation_source else {
                    // Should not be possible.
                    log::error!(
                        "Close session button pressed with no open confirmation dialog source"
                    );
                    return;
                };
                ctx.emit(CloseSessionConfirmationEvent::CloseSession {
                    dont_show_again: *dont_show_again,
                    open_confirmation_source,
                });
            }
            CloseSessionConfirmationAction::CloseAndLeaveRunning => {
                let Some(open_confirmation_source) = self.open_confirmation_source else {
                    // Should not be possible, and the same log line as the
                    // `CloseSession` arm for the same reason: a source is set
                    // when the dialog is opened, and the buttons are only
                    // reachable once it is.
                    log::error!(
                        "Leave running button pressed with no open confirmation dialog source"
                    );
                    return;
                };
                ctx.emit(CloseSessionConfirmationEvent::CloseAndLeaveRunning {
                    open_confirmation_source,
                });
            }
            CloseSessionConfirmationAction::Cancel => {
                ctx.emit(CloseSessionConfirmationEvent::Cancel);
                self.dont_show_again = false;
            }
            CloseSessionConfirmationAction::ToggleDontShowAgain => {
                self.dont_show_again = !self.dont_show_again;
                ctx.notify();
            }
        }
    }
}
