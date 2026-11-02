use regex::Regex;
use std::time::Duration;
use warpui::{
    async_assert, async_assert_eq,
    integration::{AssertionOutcome, TestStep},
};

use crate::{
    integration_testing::{
        step::assert_no_pending_model_events,
        terminal::{
            assert_long_running_block_executing_for_single_terminal_in_tab, validate_block_output,
            wait_until_bootstrapped_pane,
        },
        view_getters::{single_terminal_view, terminal_view},
    },
    terminal::{model::rich_content::RichContentType, view::WithinBlockBanner},
};

use super::util::{SSH_KEY_PASSPHRASE, ssh_command, ssh_command_with_key};

/// Initiates an SSH connection, executing the necessary command and then
/// waiting briefly (to avoid any subsequent steps performing assertions about
/// long-running command state faster than the ssh command can fail).
pub fn enter_ssh_command(shell: &str) -> TestStep {
    let ssh_command = ssh_command(shell, true);
    TestStep::new(&format!("Start ssh connection with remote shell '{shell}'"))
        .with_typed_characters(&[&ssh_command])
        .with_keystrokes(&["enter"])
        .set_post_step_pause(Duration::from_millis(250))
}

/// Like [`enter_ssh_command`] but authenticates with the key that has no passphrase, so the
/// connect never prompts, and does NOT pause afterwards: whatever the next step types arrives
/// while the connect and the remote bootstrap are still under way (typeahead during connect).
pub fn enter_ssh_command_without_prompt(shell: &str) -> TestStep {
    let ssh_command = ssh_command_with_key(shell, true, false);
    TestStep::new(&format!(
        "Start ssh connection (no passphrase prompt) with remote shell '{shell}'"
    ))
    .with_typed_characters(&[&ssh_command])
    .with_keystrokes(&["enter"])
}

pub fn enter_remote_subshell_command(shell: &str) -> TestStep {
    let ssh_command = ssh_command(shell, false);
    TestStep::new(&format!("Start ssh connection with remote shell '{shell}'"))
        .with_typed_characters(&[&ssh_command])
        .with_keystrokes(&["enter"])
        .set_post_step_pause(Duration::from_millis(250))
}

/// Waits for the prompt that asks for the credential: for the local sshd fixture, the
/// passphrase of the throw-away client key (an unprivileged sshd cannot offer a password).
pub fn wait_for_password_prompt(tab_index: usize, _shell: &str) -> TestStep {
    let regex = Regex::new(r"Enter passphrase for key '[^']*':[\s]*$")
        .expect("regex should not fail to compile");
    TestStep::new("Wait for password prompt")
        // Wait up to 40 seconds for the prompt to appear.  This is
        // intended to reduce flakiness due to still-not-understood delays
        // in the prompt appearing.
        .set_timeout(Duration::from_secs(40))
        .add_assertion(assert_long_running_block_executing_for_single_terminal_in_tab(true, 0))
        .add_assertion(move |app, window_id| {
            validate_block_output(&regex, tab_index, 0, window_id, app)
        })
}

/// Enters the passphrase of the client key the local sshd fixture accepts.
pub fn enter_ssh_password() -> TestStep {
    TestStep::new("Enter ssh password").with_typed_characters(&[&format!("{SSH_KEY_PASSPHRASE}\n")])
}

pub fn enter_local_subshell_command(shell: &str) -> TestStep {
    TestStep::new(&format!("Enter local subshell command for {shell}"))
        .with_input_string(shell, Some(&["enter"]))
        // Wait for shell line editor to become active before moving to next test step.
        .set_post_step_pause(Duration::from_millis(50))
}

pub fn assert_subshell_banner_is_showing() -> TestStep {
    TestStep::new("Assert the Warpify banner is visible")
        .add_assertion(move |app, window_id| {
            let terminal_view = single_terminal_view(app, window_id);
            terminal_view.read(app, |view, _ctx| {
                async_assert!(matches!(
                    view.model
                        .lock()
                        .block_list_mut()
                        .active_block()
                        .block_banner(),
                    Some(WithinBlockBanner::WarpifyBanner(..))
                ))
            })
        })
        // Wait for outstanding model events to finish before moving to the next step
        .add_named_assertion("no pending model events", assert_no_pending_model_events())
        .set_post_step_pause(Duration::from_millis(50))
}

pub fn trigger_subshell_bootstrap() -> TestStep {
    TestStep::new("Trigger subshell bootstrap").with_keystrokes(&["ctrl-i"])
}

pub fn assert_subshell_is_bootstrapped(tab_index: usize, pane_index: usize) -> TestStep {
    wait_until_bootstrapped_pane(tab_index, pane_index).add_named_assertion(
        "Subshell info block was displayed and no extraneous blocks added",
        move |app, window_id| {
            let terminal_view = terminal_view(app, window_id, tab_index, pane_index);
            terminal_view.read(app, |view, _ctx| {
                let model = view.model.lock();

                let Some((success_block_index, rich_content_type)) = model
                    .block_list()
                    .last_non_hidden_rich_content_block_after_block(None)
                    .map(|(success_block_index, block)| (success_block_index, block.content_type))
                else {
                    return AssertionOutcome::failure("No rich content block found!".to_owned());
                };

                match rich_content_type {
                    Some(RichContentType::WarpifySuccessBlock) => {}
                    _ => {
                        return AssertionOutcome::failure(
                            "Warpify success block wasn't added to the blocklist".to_owned(),
                        );
                    }
                }

                let success_block_index: usize = success_block_index.into();
                // Make sure there are no non-in-band-generator blocks added to the blocklist in
                // between the static block and the active block (which is not yet finished).
                async_assert_eq!(
                    model.block_list().blocks()[success_block_index + 1..]
                        .iter()
                        .filter(|block| !block.is_in_band_command_block() && block.finished())
                        .count(),
                    0,
                    "Added extraneous blocks to the block list.",
                )
            })
        },
    )
}

pub fn accept_tmux_install() -> TestStep {
    TestStep::new("Accept tmux install").with_keystrokes(&["enter"])
}

pub fn run_exit_command() -> TestStep {
    TestStep::new("Run exit command").with_keystrokes(&["e", "x", "i", "t", "enter"])
}
