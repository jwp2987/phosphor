//! A small, subtle chip over a Phosphorized ssh session that runs WITHOUT the remote helper.
//!
//! The no-helper path is the primary design, so the chip is informational only: it never blocks,
//! never reappears for a host once dismissed, and a setting hides it entirely. Clicking it
//! expands a short explanation of what the optional helper adds. The helper can only be
//! installed from the connect-time prompt (there is no after-the-fact install), so the
//! explanation names the setting that controls that prompt instead of offering a button.

use warp_core::ui::theme::color::internal_colors;
use warpui::EntityId;
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext,
    elements::{
        Container, CornerRadius, CrossAxisAlignment, Flex, Hoverable, MainAxisSize,
        MouseStateHandle, ParentElement, Radius, Text,
    },
    platform::Cursor,
};

use settings::Setting;

use super::TerminalView;
use super::rich_content::{RichContentInsertionPosition, RichContentMetadata};
use crate::settings::{InputSettings, no_helper_chip_visible};
use crate::{Appearance, terminal::model::session::SessionId};

/// The chip's label.
pub(crate) const CHIP_LABEL: &str = "shell mode";

/// What the helper adds, and how it is offered. Shown when the chip is clicked.
pub(crate) const CHIP_EXPLANATION: &str = "This ssh session works without the optional remote \
    helper: commands, completions and history all run. Today the helper only adds faster remote \
    file transfer, command running, branch lists and project search; more is planned. It is \
    offered when you connect; set warpify.ssh.ssh_extension_install_mode to always_install to \
    install it on the next connection.";

#[derive(Clone, Debug)]
pub enum SshNoHelperChipAction {
    ToggleDetails,
    DismissForHost,
    HideEverywhere,
}

#[derive(Clone, Debug)]
pub enum SshNoHelperChipEvent {
    Dismissed,
}

pub struct SshNoHelperChip {
    session_id: SessionId,
    host: String,
    expanded: bool,
    chip_state: MouseStateHandle,
    host_state: MouseStateHandle,
    hide_state: MouseStateHandle,
}

impl SshNoHelperChip {
    pub fn new(session_id: SessionId, host: String) -> Self {
        Self {
            session_id,
            host,
            expanded: false,
            chip_state: MouseStateHandle::default(),
            host_state: MouseStateHandle::default(),
            hide_state: MouseStateHandle::default(),
        }
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }
}

impl Entity for SshNoHelperChip {
    type Event = SshNoHelperChipEvent;
}

fn text_button(
    label: String,
    state: MouseStateHandle,
    action: SshNoHelperChipAction,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let fg = theme.foreground().into_solid();
    let font_family = appearance.ui_font_family();
    let font_size = appearance.monospace_font_size() - 2.;
    let background = internal_colors::fg_overlay_2(theme);
    let hover_background = internal_colors::fg_overlay_3(theme);
    Hoverable::new(state, move |mouse| {
        Container::new(
            Text::new(label.clone(), font_family, font_size)
                .with_color(fg)
                .finish(),
        )
        .with_background(if mouse.is_hovered() {
            hover_background
        } else {
            background
        })
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .with_horizontal_padding(10.)
        .with_vertical_padding(4.)
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| ctx.dispatch_typed_action(action.clone()))
    .finish()
}

impl View for SshNoHelperChip {
    fn ui_name() -> &'static str {
        "SshNoHelperChip"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let muted = internal_colors::neutral_5(theme);
        let small = appearance.monospace_font_size() - 2.;
        let font_family = appearance.ui_font_family();

        let chip = Hoverable::new(self.chip_state.clone(), move |mouse| {
            Container::new(
                Text::new(CHIP_LABEL, font_family, small)
                    .with_color(muted)
                    .finish(),
            )
            .with_background(if mouse.is_hovered() {
                internal_colors::fg_overlay_2(theme)
            } else {
                internal_colors::fg_overlay_1(theme)
            })
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
            .with_horizontal_padding(8.)
            .with_vertical_padding(2.)
            .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(|ctx, _, _| ctx.dispatch_typed_action(SshNoHelperChipAction::ToggleDetails))
        .finish();

        let mut content = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_child(chip);

        if self.expanded {
            content = content
                .with_child(
                    Container::new(
                        Text::new(CHIP_EXPLANATION, appearance.ui_font_family(), small)
                            .soft_wrap(true)
                            .with_color(muted)
                            .finish(),
                    )
                    .with_margin_top(6.)
                    .finish(),
                )
                .with_child(
                    Container::new(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_child(text_button(
                                format!("Don't show for {}", self.host),
                                self.host_state.clone(),
                                SshNoHelperChipAction::DismissForHost,
                                appearance,
                            ))
                            .with_child(
                                Container::new(text_button(
                                    "Never show".to_owned(),
                                    self.hide_state.clone(),
                                    SshNoHelperChipAction::HideEverywhere,
                                    appearance,
                                ))
                                .with_margin_left(8.)
                                .finish(),
                            )
                            .finish(),
                    )
                    .with_margin_top(6.)
                    .finish(),
                );
        }

        Container::new(content.finish())
            .with_horizontal_padding(12.)
            .with_vertical_padding(4.)
            .finish()
    }
}

impl TypedActionView for SshNoHelperChip {
    type Action = SshNoHelperChipAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SshNoHelperChipAction::ToggleDetails => {
                self.expanded = !self.expanded;
                ctx.notify();
            }
            SshNoHelperChipAction::DismissForHost => {
                let host = self.host.clone();
                InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                    let mut hosts = settings.no_helper_chip_dismissed_hosts.value().clone();
                    if !hosts.iter().any(|h| h.eq_ignore_ascii_case(&host)) {
                        hosts.push(host);
                    }
                    if let Err(e) = settings
                        .no_helper_chip_dismissed_hosts
                        .set_value(hosts, ctx)
                    {
                        log::error!("Failed to persist no_helper_chip_dismissed_hosts: {e}");
                    }
                });
                ctx.emit(SshNoHelperChipEvent::Dismissed);
            }
            SshNoHelperChipAction::HideEverywhere => {
                InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                    if let Err(e) = settings.show_no_helper_chip.set_value(false, ctx) {
                        log::error!("Failed to persist show_no_helper_chip: {e}");
                    }
                });
                ctx.emit(SshNoHelperChipEvent::Dismissed);
            }
        }
    }
}

impl TerminalView {
    /// Shows the chip for a no-helper ssh session, unless the setting hides it or the host was
    /// dismissed. At most one chip per session.
    pub(super) fn show_ssh_no_helper_chip(
        &mut self,
        session_id: SessionId,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(host) = self
            .sessions
            .as_ref(ctx)
            .get(session_id)
            .map(|session| session.hostname().to_owned())
        else {
            return;
        };
        let visible = {
            let settings = InputSettings::as_ref(ctx);
            no_helper_chip_visible(
                *settings.show_no_helper_chip.value(),
                settings.no_helper_chip_dismissed_hosts.value(),
                &host,
            )
        };
        if !visible {
            return;
        }
        let already = self.rich_content_views.iter().any(|view| {
            matches!(
                view.metadata(),
                Some(RichContentMetadata::SshNoHelperChip { handle })
                if handle.as_ref(ctx).session_id() == session_id
            )
        });
        if already {
            return;
        }
        let chip = ctx.add_typed_action_view(|_| SshNoHelperChip::new(session_id, host));
        ctx.subscribe_to_view(&chip, move |me, _, event, ctx| match event {
            SshNoHelperChipEvent::Dismissed => me.remove_ssh_no_helper_chip(session_id, ctx),
        });
        self.insert_rich_content(
            None,
            chip.clone(),
            Some(RichContentMetadata::SshNoHelperChip { handle: chip }),
            RichContentInsertionPosition::Append {
                insert_below_long_running_block: true,
            },
            ctx,
        );
    }

    /// Removes the chip of `session_id`, if any.
    pub(super) fn remove_ssh_no_helper_chip(
        &mut self,
        session_id: SessionId,
        ctx: &mut ViewContext<Self>,
    ) {
        let ids: Vec<EntityId> = self
            .rich_content_views
            .iter()
            .filter(|view| {
                matches!(
                    view.metadata(),
                    Some(RichContentMetadata::SshNoHelperChip { handle })
                    if handle.as_ref(ctx).session_id() == session_id
                )
            })
            .map(|view| view.view_id())
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut model = self.model.lock();
        for id in &ids {
            model.block_list_mut().remove_rich_content(*id);
        }
        drop(model);
        self.rich_content_views
            .retain(|view| !ids.contains(&view.view_id()));
        ctx.notify();
    }
}

#[cfg(test)]
#[path = "ssh_no_helper_chip_tests.rs"]
mod tests;
