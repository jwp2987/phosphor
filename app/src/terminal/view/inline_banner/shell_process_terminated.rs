use warpui::{
    Element,
    elements::{MouseStateHandle, Text},
};

use crate::{appearance::Appearance, terminal::view::TerminalAction};

use super::{
    InlineBannerButtonState, InlineBannerContent, InlineBannerIcon, InlineBannerStyle,
    InlineBannerTextButton, InlineBannerTextButtonVariant, render_inline_block_list_banner,
};

/// Position-cache id of the "Restart shell" button, for tests and the real-app harness.
pub const RESTART_SHELL_BUTTON_POSITION_ID: &str = "shell_process_terminated_banner:restart_shell";

/// The "Restart shell" button's label.
pub const RESTART_SHELL_BUTTON_LABEL: &str = "Restart shell";

pub fn render_shell_process_terminated_banner(
    appearance: &Appearance,
    was_premature_termination: bool,
    restart_shell_button: Option<&MouseStateHandle>,
) -> Box<dyn Element> {
    if was_premature_termination {
        render_inline_block_list_banner(
            InlineBannerStyle::CallToAction,
            appearance,
            InlineBannerContent {
                title: "Shell process exited prematurely!".to_string(),
                header_icon: Some(InlineBannerIcon {
                    asset_path: "bundled/svg/warning.svg",
                    aspect_ratio: 1.,
                    color_override: Some(appearance.theme().foreground().into_solid()),
                }),
                content: Some(vec![Text::new(
                    "The output from Phosphor's initialization script is visible above to assist with debugging.",
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )]),
                ..Default::default()
            },
        )
    } else {
        // Offered only after Windows ended the shell for a restart or sign-out that may yet be
        // cancelled (jwp2987/phosphor#1103). A click restarts this pane's shell in place.
        let (content, buttons) = match restart_shell_button {
            Some(mouse_state_handle) => (
                Some(vec![Text::new(
                    "Windows ended this shell when a restart or sign-out was requested.",
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )]),
                vec![InlineBannerTextButton {
                    text: RESTART_SHELL_BUTTON_LABEL.to_owned(),
                    text_color: appearance.theme().active_ui_text_color().into_solid(),
                    button_state: InlineBannerButtonState {
                        on_click_event: TerminalAction::RestartExitedShell,
                        mouse_state_handle: mouse_state_handle.clone(),
                    },
                    font: Default::default(),
                    position_id: Some(RESTART_SHELL_BUTTON_POSITION_ID.to_owned()),
                    variant: InlineBannerTextButtonVariant::Primary,
                }],
            ),
            None => (None, Vec::new()),
        };
        render_inline_block_list_banner(
            InlineBannerStyle::LowPriority,
            appearance,
            InlineBannerContent {
                title: "Shell process exited".to_string(),
                header_icon: Some(InlineBannerIcon {
                    asset_path: "bundled/svg/info.svg",
                    aspect_ratio: 1.,
                    ..Default::default()
                }),
                content,
                buttons,
                ..Default::default()
            },
        )
    }
}
