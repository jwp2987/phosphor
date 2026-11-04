use pathfinder_geometry::vector::vec2f;
use warp_core::ui::{color::coloru_with_opacity, theme::Fill};
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext,
    WeakViewHandle,
    elements::{
        Align, ChildAnchor, Container, CrossAxisAlignment, Flex, Hoverable, MouseStateHandle,
        OffsetPositioning, ParentAnchor, ParentElement, ParentOffsetBounds, Stack, Text,
    },
    fonts::Weight,
    keymap::{FixedBinding, Keystroke},
    pane_key::{PaneKey, PaneKeyContext, PaneKeyKind, PaneKeyOutcome},
    platform::Cursor,
    ui_components::{
        button::ButtonVariant,
        components::{Coords, UiComponent, UiComponentStyles},
    },
};

use crate::{
    appearance::Appearance,
    editor::{EditorAction, HeldPasteKey},
    terminal::input::Input,
    ui_components::dialog::{Dialog, dialog_styles},
};

pub fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;

    app.register_fixed_bindings([
        FixedBinding::new(
            "escape",
            MultilineSubmitConfirmationAction::Cancel,
            id!(MultilineSubmitConfirmationDialog::ui_name()),
        ),
        FixedBinding::new(
            "enter",
            MultilineSubmitConfirmationAction::Confirm,
            id!(MultilineSubmitConfirmationDialog::ui_name()),
        ),
    ]);
}

const DIALOG_WIDTH: f32 = 460.;

pub struct MultilineSubmitConfirmationDialog {
    cancel_mouse_state: MouseStateHandle,
    confirm_mouse_state: MouseStateHandle,
    /// Lines in the buffer being submitted; shown in the dialog text.
    line_count: usize,
    /// The input that asked (#1652): the keys typed while the dialog has focus are held in its
    /// editor's hold and delivered to it once the dialog is answered.
    asking_input: Option<WeakViewHandle<Input>>,
}

impl Default for MultilineSubmitConfirmationDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl MultilineSubmitConfirmationDialog {
    pub fn new() -> Self {
        Self {
            cancel_mouse_state: Default::default(),
            confirm_mouse_state: Default::default(),
            line_count: 0,
            asking_input: None,
        }
    }

    /// Sets the input the dialog answers (#1652), returning the one it replaces.
    pub fn set_asking_input(
        &mut self,
        input: Option<WeakViewHandle<Input>>,
    ) -> Option<WeakViewHandle<Input>> {
        std::mem::replace(&mut self.asking_input, input)
    }

    /// Sets the count shown and schedules a re-render: without the notify the dialog's cached
    /// render kept the previous count (0 on the first show, then the last one) (#1046).
    pub fn set_line_count(&mut self, line_count: usize, ctx: &mut ViewContext<Self>) {
        self.line_count = line_count;
        ctx.notify();
    }

    pub fn line_count(&self) -> usize {
        self.line_count
    }
}

impl Entity for MultilineSubmitConfirmationDialog {
    type Event = MultilineSubmitConfirmationEvent;
}

impl View for MultilineSubmitConfirmationDialog {
    fn ui_name() -> &'static str {
        "MultilineSubmitConfirmationDialog"
    }

    fn on_focus(&mut self, _focus_ctx: &warpui::FocusContext, ctx: &mut ViewContext<Self>) {
        ctx.focus_self();
    }

    /// #1652: while the dialog has focus it is the key sink, so a key its own bindings (Enter,
    /// Escape) do not take is never dropped: it is held for the asking input's editor and
    /// delivered there, in typed order, once the dialog is answered.
    fn is_pane_key_sink(&self) -> bool {
        true
    }

    fn on_pane_key(
        &mut self,
        key: &PaneKey,
        _key_ctx: &PaneKeyContext,
        ctx: &mut ViewContext<Self>,
    ) -> PaneKeyOutcome {
        let Some(held) = dialog_held_key(&key.kind) else {
            return PaneKeyOutcome::NotMine;
        };
        let Some(input) = self
            .asking_input
            .as_ref()
            .and_then(|input| input.upgrade(ctx))
        else {
            return PaneKeyOutcome::NotMine;
        };
        let hold = input.as_ref(ctx).multiline_dialog_key_hold(ctx);
        warpui::input_order::log_key_hop("kh_multiline_dialog_hold", || {
            format!("seq={} kind={held:?}", key.seq)
        });
        if hold.modal_hold_push(held) {
            PaneKeyOutcome::Consumed
        } else {
            PaneKeyOutcome::NotMine
        }
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        let button_style = UiComponentStyles {
            font_size: Some(14.),
            font_weight: Some(Weight::Bold),
            height: Some(40.),
            ..Default::default()
        };

        // Build run button label with Enter keyboard shortcut indicator
        let enter_keystroke = Keystroke::parse("enter").expect("Valid keystroke");
        let text_color = theme.main_text_color(theme.accent()).into_solid();
        let run_button_label = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Text::new_inline(
                    crate::t!("multiline-submit-dialog-confirm"),
                    appearance.ui_font_family(),
                    14.,
                )
                .with_color(text_color)
                .finish(),
            )
            .with_child(
                Container::new(
                    appearance
                        .ui_builder()
                        .keyboard_shortcut(&enter_keystroke)
                        .with_style(UiComponentStyles {
                            font_size: Some(10.),
                            height: Some(16.),
                            padding: Some(Coords::uniform(1.)),
                            border_width: Some(1.),
                            border_color: Some(coloru_with_opacity(text_color, 60).into()),
                            font_color: Some(text_color),
                            font_family_id: Some(appearance.ui_font_family()),
                            ..Default::default()
                        })
                        .with_line_height_ratio(1.0)
                        .build()
                        .finish(),
                )
                .with_margin_left(8.)
                .finish(),
            )
            .finish();

        let run_button = appearance
            .ui_builder()
            .button(ButtonVariant::Accent, self.confirm_mouse_state.clone())
            .with_custom_label(run_button_label)
            .with_style(UiComponentStyles {
                padding: Some(Coords {
                    left: 16.,
                    right: 16.,
                    top: 0.,
                    bottom: 0.,
                }),
                ..button_style
            })
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(MultilineSubmitConfirmationAction::Confirm)
            })
            .finish();

        let cancel_text_color = theme.sub_text_color(theme.surface_2());
        let cancel_button = Container::new(
            Hoverable::new(self.cancel_mouse_state.clone(), move |mouse_state| {
                let color = if mouse_state.is_mouse_over_element() {
                    theme.main_text_color(theme.surface_2())
                } else {
                    cancel_text_color
                };
                Text::new_inline(
                    crate::t!("multiline-submit-dialog-cancel"),
                    appearance.ui_font_family(),
                    14.,
                )
                .with_color(color.into_solid())
                .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(MultilineSubmitConfirmationAction::Cancel)
            })
            .finish(),
        )
        .with_margin_right(16.)
        .finish();

        let dialog = Container::new(
            Dialog::new(
                crate::t!("multiline-submit-dialog-title", count = self.line_count),
                Some(crate::t!(
                    "multiline-submit-dialog-body",
                    count = self.line_count
                )),
                UiComponentStyles {
                    width: Some(DIALOG_WIDTH),
                    padding: Some(Coords::uniform(24.)),
                    ..dialog_styles(appearance)
                },
            )
            .with_bottom_row_child(cancel_button)
            .with_bottom_row_child(run_button)
            .build()
            .finish(),
        )
        .with_margin_top(35.)
        .finish();

        // Stack needed so that dialog can get bounds information
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

/// #1652: what a key that reaches the dialog's sink is held as, or `None` for a key the dialog
/// leaves alone (it continues to the elements, as before). Typed text (and an IME commit) is held as
/// text. A key down is held when it is one of the editor's fixed editing keys (the binding it would
/// have had in the editor, so it does there what it would have done); a printable key down is left
/// alone because its typed event follows and is the key. Enter and Escape never get here: the
/// dialog's own bindings take them first.
pub(crate) fn dialog_held_key(kind: &PaneKeyKind) -> Option<HeldPasteKey> {
    match kind {
        PaneKeyKind::Typed { chars, .. } if !chars.is_empty() => {
            Some(HeldPasteKey::Text(chars.clone()))
        }
        PaneKeyKind::KeyDown { keystroke, .. } => {
            let action = match keystroke.normalized().as_str() {
                "backspace" => EditorAction::Backspace,
                "delete" => EditorAction::Delete,
                "left" => EditorAction::Left,
                "right" => EditorAction::Right,
                "up" => EditorAction::Up,
                "down" => EditorAction::Down,
                "tab" => EditorAction::Tab,
                "shift-tab" => EditorAction::ShiftTab,
                _ => return None,
            };
            Some(HeldPasteKey::Action(action))
        }
        _ => None,
    }
}

pub enum MultilineSubmitConfirmationEvent {
    Confirm,
    Cancel,
}

#[derive(Debug)]
pub enum MultilineSubmitConfirmationAction {
    Confirm,
    Cancel,
}

impl TypedActionView for MultilineSubmitConfirmationDialog {
    type Action = MultilineSubmitConfirmationAction;

    fn handle_action(
        &mut self,
        action: &MultilineSubmitConfirmationAction,
        ctx: &mut ViewContext<Self>,
    ) {
        match action {
            MultilineSubmitConfirmationAction::Confirm => {
                ctx.emit(MultilineSubmitConfirmationEvent::Confirm);
            }
            MultilineSubmitConfirmationAction::Cancel => {
                ctx.emit(MultilineSubmitConfirmationEvent::Cancel);
            }
        }
    }
}
