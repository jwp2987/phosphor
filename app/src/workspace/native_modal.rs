use crate::appearance::Appearance;
use crate::terminal::general_settings::{GeneralSettings, GeneralSettingsChangedEvent};
use crate::ui_components::dialog::{Dialog, dialog_styles};
use settings::Setting as _;
use warp_core::ui::theme::Fill;
use warpui::elements::{Align, Container, Empty, Flex, ParentElement};
use warpui::keymap::FixedBinding;
use warpui::modals::{AlertDialogWithCallbacks, AppModalCallback};
use warpui::ui_components::components::{Coords, UiComponent};
use warpui::{AppContext, ModelHandle, SingletonEntity, ViewContext};
use warpui::{
    Element, Entity, TypedActionView, View,
    elements::MouseStateHandle,
    fonts::Weight,
    platform::Cursor,
    ui_components::{button::ButtonVariant, components::UiComponentStyles, text::Span},
};

pub(super) fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;

    app.register_fixed_bindings(vec![
        FixedBinding::new("escape", NativeModalAction::Close, id!("NativeModal")),
        FixedBinding::new("enter", NativeModalAction::Confirm, id!("NativeModal")),
        // Only a dialog with a keyboard selection (`AlertDialogWithCallbacks::enter_button`)
        // does anything with these two.
        FixedBinding::new("tab", NativeModalAction::SelectNext, id!("NativeModal")),
        FixedBinding::new(
            "shift-tab",
            NativeModalAction::SelectPrevious,
            id!("NativeModal"),
        ),
        // #2132: Space presses the selected button, as Enter does. In a dialog without a
        // keyboard selection it does nothing, as before.
        FixedBinding::new(
            "space",
            NativeModalAction::PressSelected,
            id!("NativeModal"),
        ),
    ]);
}

/// Used to show a Zap-native modal dialog above a [`super::Workspace`]. The button Enter presses
/// is [`ButtonVariant::Accent`]: the first one, or the selected one in a dialog with a keyboard
/// selection. In a dialog with a keyboard selection the selected button also carries a focus
/// ring, and Space presses it too (#2132).
pub struct NativeModal {
    alert_dialog: Option<AlertDialogWithCallbacks<AppModalCallback>>,
    /// The keyboard selection, for a dialog that asked for one
    /// (`AlertDialogWithCallbacks::enter_button`); `None` for every other dialog, where Enter is
    /// the first button and Tab does nothing.
    selected_button: Option<usize>,
    dont_show_again: bool,
    modal_button_mouse_states: Vec<MouseStateHandle>,
    dont_show_again_mouse_state: MouseStateHandle,
}

#[derive(Debug)]
pub enum NativeModalAction {
    ToggleDontShowAgain,
    /// Trigger a callback registered in [`NativeModal::alert_dialog`] and reset the modal.
    TriggerButtonCallback(usize),
    /// Triggers the last button in the list, as we assume the last button is "cancel".
    Close,
    /// Triggers the first button in the list, as we assume the first button is "confirm". In a
    /// dialog with a keyboard selection: the selected button.
    Confirm,
    /// Tab / Shift-Tab: move the keyboard selection, in a dialog that has one.
    SelectNext,
    SelectPrevious,
    /// Space: trigger the selected button, in a dialog with a keyboard selection; nothing in
    /// any other dialog (#2132).
    PressSelected,
}

pub enum NativeModalEvent {
    Close,
}

impl NativeModal {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let general_settings = GeneralSettings::handle(ctx);
        let dont_show_again = !*general_settings
            .as_ref(ctx)
            .show_warning_before_quitting
            .value();
        ctx.subscribe_to_model(&general_settings, Self::handle_general_settings_event);
        NativeModal {
            alert_dialog: None,
            selected_button: None,
            dont_show_again,
            dont_show_again_mouse_state: Default::default(),
            modal_button_mouse_states: Default::default(),
        }
    }

    fn handle_general_settings_event(
        &mut self,
        general_settings: ModelHandle<GeneralSettings>,
        event: &GeneralSettingsChangedEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        if let GeneralSettingsChangedEvent::ShowWarningBeforeQuitting { .. } = event {
            self.dont_show_again = !general_settings.read(ctx, |settings, _| {
                *settings.show_warning_before_quitting.value()
            });
            ctx.notify();
        }
    }

    pub fn set_alert_dialog(&mut self, alert_dialog: AlertDialogWithCallbacks<AppModalCallback>) {
        self.modal_button_mouse_states.clear();
        for _ in 0..alert_dialog.button_data.len() {
            self.modal_button_mouse_states.push(Default::default());
        }
        self.selected_button =
            initial_selection(alert_dialog.enter_button, alert_dialog.button_data.len());
        self.alert_dialog = Some(alert_dialog);
    }

    fn reset(&mut self) {
        self.alert_dialog = None;
        self.selected_button = None;
        self.modal_button_mouse_states = Default::default();
    }

    #[cfg(any(test, feature = "integration_tests"))]
    pub(super) fn has_alert_dialog(&self) -> bool {
        self.alert_dialog.is_some()
    }

    /// `by`: what pressed the button (`pointer`, `enter`, `space`, `escape`), for the trace line
    /// the app tests read.
    fn trigger_button_callback(
        &mut self,
        idx: usize,
        by: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        // Once we trigger a callback from a button, we are guaranteed that the modal will close so
        // it's ok to take the alert dialog from Self.
        if let Some(mut dialog) = self.alert_dialog.take() {
            warpui::input_order::log_hop("native_modal", || pressed_trace(idx, by));
            let button = dialog.button_data.remove(idx);
            (button.on_click)(ctx);
            // #2092: a dialog without the checkbox never runs its disable callback. The checkbox
            // state starts from the setting, so with the setting already off a dialog that still
            // has to show (unsaved editor edits) would otherwise toggle the setting back on.
            if should_run_disable_callback(dialog.can_disable, self.dont_show_again) {
                (dialog.on_disable)(ctx);
            }
        }
        self.reset();
        ctx.emit(NativeModalEvent::Close);
    }
}

/// The selection a dialog starts with. `enter_button` is what the dialog asked for; an index past
/// the end is taken as the last button, which is the one Escape presses (never the first: a
/// dialog asks for this so that Enter is NOT its first button).
fn initial_selection(enter_button: Option<usize>, button_count: usize) -> Option<usize> {
    let last = button_count.checked_sub(1)?;
    enter_button.map(|index| index.min(last))
}

/// Tab (`forward`) or Shift-Tab on a dialog with `button_count` buttons: the next or previous
/// button, wrapping. A dialog without a selection stays without one.
fn moved_selection(selected: Option<usize>, button_count: usize, forward: bool) -> Option<usize> {
    let selected = selected?;
    if button_count == 0 {
        return None;
    }
    let step = if forward { 1 } else { button_count - 1 };
    Some((selected.min(button_count - 1) + step) % button_count)
}

/// The button Enter presses: the selected one, or the first in a dialog without a selection.
fn enter_button_index(selected: Option<usize>) -> usize {
    selected.unwrap_or(0)
}

/// The button Space presses (#2132): the selected one. `None` in a dialog without a keyboard
/// selection, where Space does nothing (Enter there is the first button; Space never was).
fn space_button_index(selected: Option<usize>) -> Option<usize> {
    selected
}

/// Whether button `index` carries the focus ring (#2132): the selected button of a dialog with
/// a keyboard selection. A dialog without one draws no ring (its Enter button is the accent
/// one, as before).
fn has_focus_ring(selected: Option<usize>, index: usize) -> bool {
    selected == Some(index)
}

/// Width of the focus ring, in pixels (a button's own border is 1).
const FOCUS_RING_WIDTH: f32 = 2.;

/// The lines the app tests read (`editor_daily`), `hop=native_modal`: indexes only, no text.
/// `event=selected button=<i>`: Tab / Shift-Tab moved the keyboard selection to button `i`.
fn selected_trace(index: usize) -> String {
    format!("event=selected button={index}")
}

/// `event=pressed button=<i> by=<pointer|enter|space|escape>`: button `i` was pressed.
fn pressed_trace(index: usize, by: &str) -> String {
    format!("event=pressed button={index} by={by}")
}

/// The button Escape presses: the last one ("Cancel"). `None` when no dialog is up.
fn last_button_index(button_count: usize) -> Option<usize> {
    button_count.checked_sub(1)
}

/// Env kill switch (#2108): `PHOSPHOR_MODAL_FOCUS_BACK=0` puts back the behaviour before the fix:
/// a dialog that closes leaves the focus where its button handler left it, which after Cancel is
/// this (no longer drawn) view. Read once per run.
pub(super) fn focus_back_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        focus_back_enabled_for(std::env::var("PHOSPHOR_MODAL_FOCUS_BACK").ok().as_deref())
    })
}

/// The kill switch's rule: on unless the variable is exactly `0`.
fn focus_back_enabled_for(value: Option<&str>) -> bool {
    value != Some("0")
}

/// Whether the workspace takes the focus back for its active tab when the dialog has closed
/// (#2108). Only while the dialog's view still has the focus: a button handler that moved the
/// focus itself (the editor's overwrite question, "Show running processes" opening the palette,
/// a close that focused the next pane) keeps what it chose.
pub(super) fn closed_dialog_hands_focus_to_tab(
    enabled: bool,
    dialog_view_still_focused: bool,
) -> bool {
    enabled && dialog_view_still_focused
}

/// Whether pressing a button also runs the dialog's "don't show again" callback: only for a
/// dialog that offers the checkbox, and only with the checkbox ticked.
fn should_run_disable_callback(can_disable: bool, dont_show_again: bool) -> bool {
    can_disable && dont_show_again
}

impl Entity for NativeModal {
    type Event = NativeModalEvent;
}

impl View for NativeModal {
    fn ui_name() -> &'static str {
        "NativeModal"
    }

    fn render(&self, app: &warpui::AppContext) -> Box<dyn warpui::Element> {
        let Some(alert_dialog) = self.alert_dialog.as_ref() else {
            // This is a persistent view that's populated on demand; an empty
            // state rendering Empty is a legitimate, expected branch — don't warn.
            return Empty::new().finish();
        };
        let appearance = Appearance::as_ref(app);
        let button_style = UiComponentStyles {
            font_size: Some(14.),
            font_weight: Some(Weight::Bold),
            width: Some(240.),
            height: Some(40.),
            ..Default::default()
        };

        let dont_show_again_checkbox = appearance
            .ui_builder()
            .checkbox(self.dont_show_again_mouse_state.clone(), Some(14.))
            .with_label(Span::new("Don't show again.", Default::default()))
            .check(self.dont_show_again)
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(|ctx, _, _| ctx.dispatch_typed_action(NativeModalAction::ToggleDontShowAgain))
            .finish();

        // #2092: no "Don't show again" on a question that no setting switches off.
        let mut dialog_column_contents = Vec::new();
        if alert_dialog.can_disable {
            dialog_column_contents.push(
                Container::new(dont_show_again_checkbox)
                    .with_padding_bottom(20.)
                    .finish(),
            );
        }

        // The accent marks the button Enter presses.
        let accent_button = enter_button_index(self.selected_button);
        // #2132: the button that has the keyboard also carries a ring in the theme's foreground
        // colour, so the selection does not rest on the accent colour alone. Given through the
        // button's own style, it holds while the pointer hovers or presses too.
        let ring_style = UiComponentStyles {
            border_width: Some(FOCUS_RING_WIDTH),
            border_color: Some(appearance.theme().foreground().into()),
            ..button_style
        };
        for (i, modal_button) in alert_dialog.button_data.iter().enumerate() {
            let button = Align::new(
                appearance
                    .ui_builder()
                    .button(
                        if i == accent_button {
                            ButtonVariant::Accent
                        } else {
                            ButtonVariant::Basic
                        },
                        self.modal_button_mouse_states
                            .get(i)
                            .expect("Modal button mouse state should be set")
                            .clone(),
                    )
                    .with_centered_text_label(modal_button.title.clone())
                    .with_style(if has_focus_ring(self.selected_button, i) {
                        ring_style
                    } else {
                        button_style
                    })
                    .build()
                    .with_cursor(Cursor::PointingHand)
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(NativeModalAction::TriggerButtonCallback(i))
                    })
                    .finish(),
            )
            .finish();

            dialog_column_contents.push(Container::new(button).with_padding_bottom(8.).finish());
        }

        let dialog_column = Flex::column()
            .with_children(dialog_column_contents)
            .finish();
        let dialog = Dialog::new(
            alert_dialog.message_text.clone(),
            Some(alert_dialog.info_text.clone()),
            UiComponentStyles {
                width: Some(280.),
                padding: Some(Coords::uniform(24.)),
                ..dialog_styles(appearance)
            },
        )
        .with_child(dialog_column)
        .build()
        .finish();

        // This blurs the background and makes it uninteractable.
        Container::new(Align::new(dialog).finish())
            .with_background_color(Fill::blur().into())
            .with_corner_radius(app.windows().window_corner_radius())
            .finish()
    }
}

impl TypedActionView for NativeModal {
    type Action = NativeModalAction;

    fn handle_action(&mut self, action: &NativeModalAction, ctx: &mut ViewContext<Self>) {
        match action {
            NativeModalAction::TriggerButtonCallback(idx) => {
                self.trigger_button_callback(*idx, "pointer", ctx);
            }
            NativeModalAction::ToggleDontShowAgain => {
                self.dont_show_again = !self.dont_show_again;
                ctx.notify();
            }
            NativeModalAction::Close => {
                // #2108: Escape with no dialog up (the focus was left on this view after a
                // dialog closed) has no last button; the workspace still hears `Close` and takes
                // the focus back.
                match last_button_index(self.modal_button_mouse_states.len()) {
                    Some(last_button_idx) => {
                        self.trigger_button_callback(last_button_idx, "escape", ctx)
                    }
                    None => ctx.emit(NativeModalEvent::Close),
                }
            }
            NativeModalAction::Confirm => {
                if self.alert_dialog.is_some() {
                    self.trigger_button_callback(
                        enter_button_index(self.selected_button),
                        "enter",
                        ctx,
                    );
                } else {
                    // #2108: as for Escape above, with no dialog up there is no button to press.
                    ctx.emit(NativeModalEvent::Close);
                }
            }
            NativeModalAction::SelectNext | NativeModalAction::SelectPrevious => {
                let forward = matches!(action, NativeModalAction::SelectNext);
                let moved = moved_selection(
                    self.selected_button,
                    self.modal_button_mouse_states.len(),
                    forward,
                );
                if moved != self.selected_button {
                    self.selected_button = moved;
                    if let Some(index) = moved {
                        warpui::input_order::log_hop("native_modal", || selected_trace(index));
                    }
                    ctx.notify();
                }
            }
            NativeModalAction::PressSelected => {
                if self.alert_dialog.is_some()
                    && let Some(index) = space_button_index(self.selected_button)
                {
                    self.trigger_button_callback(index, "space", ctx);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "native_modal_tests.rs"]
mod tests;
