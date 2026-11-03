//! #1596: the banner over a running ssh block that shows text the ssh hand-back KEPT while that
//! block runs (`TerminalView::hand_back_unexecuted_ssh_text`). Before it the text was invisible
//! until the ssh session ended; now it is shown with two actions:
//! - "Type into session" writes it to the session's pty, on the click (a user event, never a timer),
//!   only while the same ssh block is still the running one;
//! - "Discard" drops it.
//!
//! The banner goes when either action runs, or when the block ends; then the existing end-of-block
//! hand-back (`Input::restore_handed_back_text`) puts the text into the editor, unchanged.

use warp_core::ui::theme::color::internal_colors;
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext,
    elements::{
        Container, CornerRadius, CrossAxisAlignment, Flex, Hoverable, MainAxisSize,
        MouseStateHandle, ParentElement, Radius, Text,
    },
    platform::Cursor,
};

use warpui::{EntityId, ViewHandle};

use super::TerminalView;
use super::rich_content::{RichContentInsertionPosition, RichContentMetadata};
use crate::{Appearance, terminal::model::block::BlockId};

/// Characters of the kept text the banner shows before it truncates.
pub(crate) const KEPT_TEXT_PREVIEW_CHARS: usize = 80;

/// The kept text on one line for the banner: a line break shows as `⏎`, any other control
/// character as a space, and text past [`KEPT_TEXT_PREVIEW_CHARS`] is cut with `…`.
pub(crate) fn kept_text_preview(text: &str) -> String {
    let flat: Vec<char> = text
        .chars()
        .map(|c| match c {
            '\n' => '⏎',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    if flat.len() <= KEPT_TEXT_PREVIEW_CHARS {
        flat.into_iter().collect()
    } else {
        let mut cut: String = flat[..KEPT_TEXT_PREVIEW_CHARS - 1].iter().collect();
        cut.push('…');
        cut
    }
}

/// The bytes "Type into session" writes for the kept text: the text as typed, each line break as
/// the Enter that made it (`\r`, what the key sends). No Enter is added after the last line: the
/// user reviews it in the session and presses Enter there.
pub(crate) fn kept_text_bytes(text: &str) -> Vec<u8> {
    text.replace("\r\n", "\n").replace('\n', "\r").into_bytes()
}

#[derive(Clone, Debug)]
pub enum SshKeptTextBannerAction {
    TypeIntoSession,
    Discard,
}

#[derive(Clone, Debug)]
pub enum SshKeptTextBannerEvent {
    TypeIntoSession,
    Discard,
}

pub struct SshKeptTextBanner {
    /// The ssh block the text was kept for.
    block_id: BlockId,
    /// The kept text (all of it; the banner shows a preview).
    text: String,
    /// Why "Type into session" did not write, once it was refused.
    note: Option<String>,
    type_mouse_state: MouseStateHandle,
    discard_mouse_state: MouseStateHandle,
}

impl SshKeptTextBanner {
    pub fn new(block_id: BlockId, text: String) -> Self {
        Self {
            block_id,
            text,
            note: None,
            type_mouse_state: MouseStateHandle::default(),
            discard_mouse_state: MouseStateHandle::default(),
        }
    }

    pub fn block_id(&self) -> &BlockId {
        &self.block_id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn set_text(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        self.text = text;
        ctx.notify();
    }

    pub fn set_note(&mut self, note: Option<String>, ctx: &mut ViewContext<Self>) {
        self.note = note;
        ctx.notify();
    }

    /// The banner's title line, with the char count of the whole kept text.
    pub(crate) fn title(&self) -> String {
        let n = self.text.chars().count();
        format!(
            "Text typed while the SSH session connected was kept, not run ({n} char{})",
            if n == 1 { "" } else { "s" }
        )
    }
}

impl Entity for SshKeptTextBanner {
    type Event = SshKeptTextBannerEvent;
}

fn banner_button(
    label: &'static str,
    mouse_state: MouseStateHandle,
    action: SshKeptTextBannerAction,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let fg = theme.foreground().into_solid();
    let font_family = appearance.ui_font_family();
    let font_size = appearance.monospace_font_size() - 2.;
    let background = internal_colors::fg_overlay_2(theme);
    let hover_background = internal_colors::fg_overlay_3(theme);
    Hoverable::new(mouse_state, move |state| {
        Container::new(
            Text::new(label, font_family, font_size)
                .with_color(fg)
                .finish(),
        )
        .with_background(if state.is_hovered() {
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

impl View for SshKeptTextBanner {
    fn ui_name() -> &'static str {
        "SshKeptTextBanner"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let fg_color = theme.foreground().into_solid();
        let muted_color = internal_colors::neutral_5(theme);
        let font_size = appearance.monospace_font_size();
        let small_font_size = font_size - 2.;

        let title = Text::new(self.title(), appearance.ui_font_family(), font_size)
            .with_color(fg_color)
            .finish();
        let preview = Container::new(
            Text::new(
                kept_text_preview(&self.text),
                appearance.monospace_font_family(),
                small_font_size,
            )
            .with_color(fg_color)
            .finish(),
        )
        .with_background(internal_colors::fg_overlay_1(theme))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .with_uniform_padding(8.)
        .with_margin_top(6.)
        .finish();

        let buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(banner_button(
                "Type into session",
                self.type_mouse_state.clone(),
                SshKeptTextBannerAction::TypeIntoSession,
                appearance,
            ))
            .with_child(
                Container::new(banner_button(
                    "Discard",
                    self.discard_mouse_state.clone(),
                    SshKeptTextBannerAction::Discard,
                    appearance,
                ))
                .with_margin_left(8.)
                .finish(),
            )
            .finish();

        let mut content = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(title)
            .with_child(preview);
        if let Some(note) = &self.note {
            content = content.with_child(
                Container::new(
                    Text::new(note.clone(), appearance.ui_font_family(), small_font_size)
                        .soft_wrap(true)
                        .with_color(muted_color)
                        .finish(),
                )
                .with_margin_top(6.)
                .finish(),
            );
        }
        let content = content
            .with_child(Container::new(buttons).with_margin_top(8.).finish())
            .finish();

        Container::new(content)
            .with_background(internal_colors::fg_overlay_1(theme))
            .with_uniform_padding(12.)
            .finish()
    }
}

impl TypedActionView for SshKeptTextBanner {
    type Action = SshKeptTextBannerAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SshKeptTextBannerAction::TypeIntoSession => {
                ctx.emit(SshKeptTextBannerEvent::TypeIntoSession)
            }
            SshKeptTextBannerAction::Discard => ctx.emit(SshKeptTextBannerEvent::Discard),
        }
    }
}

/// The note shown when "Type into session" is refused because the ssh block the text was kept
/// for is no longer the running command.
pub(crate) const NOT_RUNNING_NOTE: &str = "The SSH session this text was typed for is no longer \
    the running command, so it was not typed anywhere. It stays here and comes back into the \
    input when the running command ends.";

impl TerminalView {
    /// The kept-text banner, if one is up.
    pub(super) fn ssh_kept_text_banner(&self) -> Option<ViewHandle<SshKeptTextBanner>> {
        self.rich_content_views
            .iter()
            .find_map(|view| match view.metadata() {
                Some(RichContentMetadata::SshKeptTextBanner { handle }) => Some(handle.clone()),
                _ => None,
            })
    }

    /// Shows (or refreshes) the banner for the text the input keeps for the running ssh block
    /// (#1596). Called right after `hand_back_unexecuted_ssh_text` stashed it.
    pub(super) fn show_ssh_kept_text_banner(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(text) = self
            .input
            .as_ref(ctx)
            .kept_handed_back_text()
            .map(str::to_owned)
        else {
            return;
        };
        let chars = text.chars().count();
        let block_id = self.model.lock().active_block_id().clone();
        if let Some(banner) = self.ssh_kept_text_banner() {
            if banner.as_ref(ctx).block_id() == &block_id {
                // A second hand-back for the same block: the input keeps both, the banner shows both.
                banner.update(ctx, |banner, ctx| {
                    banner.set_text(text, ctx);
                    banner.set_note(None, ctx);
                });
                warpui::input_order::log_hop("ssh_kept_text", || {
                    format!("action=refreshed chars={chars}")
                });
                return;
            }
            self.remove_ssh_kept_text_banner(ctx);
        }
        let banner = ctx.add_typed_action_view(|_| SshKeptTextBanner::new(block_id, text));
        ctx.subscribe_to_view(&banner, |me, _, event, ctx| match event {
            SshKeptTextBannerEvent::TypeIntoSession => me.type_ssh_kept_text_into_session(ctx),
            SshKeptTextBannerEvent::Discard => me.discard_ssh_kept_text(ctx),
        });
        self.insert_rich_content(
            None,
            banner.clone(),
            Some(RichContentMetadata::SshKeptTextBanner { handle: banner }),
            RichContentInsertionPosition::Append {
                insert_below_long_running_block: true,
            },
            ctx,
        );
        warpui::input_order::log_hop("ssh_kept_text", || format!("action=shown chars={chars}"));
    }

    /// Removes the kept-text banner, if one is up. The kept text itself is the input's.
    pub(super) fn remove_ssh_kept_text_banner(&mut self, ctx: &mut ViewContext<Self>) {
        let ids: Vec<EntityId> = self
            .rich_content_views
            .iter()
            .filter(|view| {
                matches!(
                    view.metadata(),
                    Some(RichContentMetadata::SshKeptTextBanner { .. })
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

    /// The block ended (#1596): the banner goes; the input's own end-of-block hand-back puts the
    /// kept text into the editor, unchanged.
    pub(super) fn ssh_kept_text_block_ended(&mut self, ctx: &mut ViewContext<Self>) {
        if self.ssh_kept_text_banner().is_none() {
            return;
        }
        warpui::input_order::log_hop("ssh_kept_text", || "action=block_ended".to_owned());
        self.remove_ssh_kept_text_banner(ctx);
    }

    /// "Type into session" (#1596): the user's click writes the kept text to this session's pty,
    /// as typed, once. Only while the ssh block it was kept for is still the running command;
    /// otherwise nothing is written and the banner says so.
    pub(super) fn type_ssh_kept_text_into_session(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(banner) = self.ssh_kept_text_banner() else {
            return;
        };
        let banner_block = banner.as_ref(ctx).block_id().clone();
        let running = self.is_long_running();
        let active_block = self.model.lock().active_block_id().clone();
        if !running || active_block != banner_block {
            warpui::input_order::log_hop("ssh_kept_text", || {
                format!("action=type_refused running={running} same_block=false")
            });
            banner.update(ctx, |banner, ctx| {
                banner.set_note(Some(NOT_RUNNING_NOTE.to_owned()), ctx)
            });
            return;
        }
        let text = self
            .input
            .update(ctx, |input, _| input.take_kept_handed_back_text());
        self.remove_ssh_kept_text_banner(ctx);
        let Some(text) = text else {
            return;
        };
        let bytes = kept_text_bytes(&text);
        warpui::input_order::log_hop("ssh_kept_text", || {
            format!("action=typed bytes={}", bytes.len())
        });
        self.report_possible_typeahead(&String::from_utf8_lossy(&bytes));
        self.write_user_bytes_to_pty(bytes, ctx);
    }

    /// "Discard" (#1596): the kept text is dropped and nothing is written.
    pub(super) fn discard_ssh_kept_text(&mut self, ctx: &mut ViewContext<Self>) {
        let dropped = self
            .input
            .update(ctx, |input, _| input.take_kept_handed_back_text());
        warpui::input_order::log_hop("ssh_kept_text", || {
            format!(
                "action=discarded chars={}",
                dropped.as_ref().map_or(0, |text| text.chars().count())
            )
        });
        self.remove_ssh_kept_text_banner(ctx);
    }
}

#[cfg(test)]
#[path = "ssh_kept_text_banner_tests.rs"]
mod tests;
