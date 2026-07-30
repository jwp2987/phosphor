//! su password confirmation prompt. Continuously monitors PTY output, and
//! when a password prompt appears after detecting the user typing `su root`
//! / `su - root` or another command that switches to root, shows a
//! confirmation menu; once the user confirms, injects the root password or
//! the shared OneKey password.
//!
//! Only injects for a root target; switching to other users via `su lg`,
//! etc., doesn't trigger it.
//! Waits for the shell prompt to appear first (indicating SSH login has
//! completed) before starting detection, to avoid conflicting with the
//! login password.
//! Uses `spawn_stream_local` + `stream!` for continuous monitoring; it
//! triggers on every `su root`.

use std::sync::Arc;
use std::time::Duration;

use async_broadcast::InactiveReceiver;
use async_stream::stream;
use lazy_static::lazy_static;
use regex::bytes::Regex;
use warpui::r#async::FutureExt;
use warpui::{ViewContext, WeakViewHandle};
use zeroize::Zeroizing;

use crate::ssh_manager::shell_prompt::bytes_look_like_shell_prompt;
use crate::terminal::TerminalView;

const SLIDING_WINDOW_BYTES: usize = 8 * 1024;
const BUFFER_HARD_LIMIT: usize = 16 * 1024;
/// Maximum duration to wait for the shell prompt in phase 1. On timeout, the
/// entire stream is abandoned (and `in_flight` is reset in `on_done`).
const SHELL_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// Once an `su`/`sudo`-to-root invocation has just been echoed, how long we
/// stay "armed" waiting for the password prompt that should follow it. A
/// real prompt appears near-instantly; this bounds how long a stale arm can
/// stick around waiting to be (mis-)matched against unrelated later output.
const SU_PASSWORD_ARM_TIMEOUT: Duration = Duration::from_secs(10);

lazy_static! {
    /// Password-prompt regex — strictly matches two categories:
    /// 1. `password` / `passphrase` / `密码` (Chinese for "password") at the
    ///    end of a line, followed by a half-width colon `:` or full-width
    ///    colon `：`
    /// 2. Kylin V10's colon-less `输入密码` ("enter password", used by that
    ///    distro's su prompt)
    ///
    /// NOTE: the CJK literals `密码` / `输入密码` above are semantic match
    /// targets, not comments — they detect real-world Chinese-language `su`
    /// prompts (e.g. on Kylin OS) and must not be translated or removed.
    ///
    /// The old implementation made the colon optional, so any line ending
    /// containing "password" (e.g. `Your password has expired`) would be a false positive.
    static ref PASSWORD_PROMPT_REGEX: Regex = Regex::new(
        r"(?im)(?:(?:password|passphrase|密码)[^\n]*(?::|：)\s*$|输入密码\s*$)"
    )
    .expect("su password prompt regex must compile");

    /// su-command regex — matches an `su` command targeting root (at line end):
    /// `su` / `su -` / `su -l` / `su --login` / `su root` / `su - root` /
    /// `su -l root` / `su --login root`. Does not match forms that switch
    /// to another user like `su lg` / `su - lg`; `sudo su` still matches
    /// the trailing `su` due to the `\bsu` word boundary.
    static ref SU_ROOT_CMD_REGEX: Regex =
        Regex::new(r"(?m)\bsu(?:\s+(?:-l?|--login|-))*(?:\s+root)?\s*$")
            .expect("su root cmd regex must compile");
}

/// Spawns the su-password continuous monitoring stream in the owner context.
pub fn spawn_su_password_injector<O>(
    pty_reads_rx: Option<InactiveReceiver<Arc<Vec<u8>>>>,
    terminal_view: WeakViewHandle<TerminalView>,
    root_password: Option<Zeroizing<String>>,
    ctx: &mut ViewContext<O>,
) where
    O: warpui::View + 'static,
{
    let Some(rx) = pty_reads_rx else {
        log::debug!("ssh su password injector: no pty_reads_rx — skip");
        return;
    };
    let Some(root_password) = root_password.filter(|password| !password.is_empty()) else {
        log::debug!("ssh su password injector: empty root password - skip");
        return;
    };
    // Set the in-flight flag, preventing the OneKey credential picker from popping up while waiting for the shell prompt.
    if let Some(view) = terminal_view.upgrade(ctx) {
        view.update(ctx, |view, _| {
            view.set_ssh_secret_auto_injection_in_flight(true);
        });
    }

    let prompt_stream = stream! {
        let mut active = rx.activate_cloned();
        let mut buf: Vec<u8> = Vec::with_capacity(SLIDING_WINDOW_BYTES);

        // Phase 1: wait for the shell prompt (SHELL_READY_TIMEOUT timeout), indicating login has completed
        loop {
            match active.recv().with_timeout(SHELL_READY_TIMEOUT).await {
                Ok(Ok(chunk)) => {
                    buf.extend_from_slice(&chunk);
                    if buf.len() > BUFFER_HARD_LIMIT {
                        let drop_n = buf.len() - SLIDING_WINDOW_BYTES;
                        buf.drain(..drop_n);
                    }
                    if bytes_look_like_shell_prompt(&buf) {
                        break;
                    }
                }
                _ => return,
            }
        }

        // Phase 2: an idle <-> armed state machine, resuming monitoring after each yield.
        //
        // Security (finding #9): the old implementation matched
        // `PASSWORD_PROMPT_REGEX` and `is_su_to_root` independently against
        // the whole 8KB sliding window, so an `su root` line anywhere in
        // scrollback (e.g. in a `cat`'d script) plus an *unrelated*
        // "Password:"-looking line anywhere else in that same window (e.g.
        // `cat`ing a file with a literal `Password:` line, or a heredoc)
        // could co-occur and pop the su/sudo password confirmation menu
        // over content that was never an actual prompt.
        //
        // See `next_su_password_event` for the tightened state machine: the
        // password-prompt check now only runs *after* an su/sudo-to-root
        // invocation has just been freshly echoed.
        buf.clear();
        loop {
            match next_su_password_event(&mut active, &mut buf).await {
                SuPasswordEvent::PromptFired => yield (),
                SuPasswordEvent::StoodDown => {}
                SuPasswordEvent::Eof => break,
            }
        }
    };

    // on_done must reset in_flight: if phase 1 (waiting for the shell
    // prompt) times out / hits EOF, it exits the stream directly via
    // `return` without ever going through on_item; if it isn't reset in
    // on_done, OneKey would be permanently blocked on that terminal.
    let terminal_view_done = terminal_view.clone();
    let _ = ctx.spawn_stream_local(
        prompt_stream,
        move |_owner, (), ctx| {
            let Some(view) = terminal_view.upgrade(ctx) else {
                return;
            };
            view.update(ctx, |view, ctx| {
                view.su_root_password = Some(root_password.clone());
                view.show_su_root_confirm_menu(ctx);
                view.set_ssh_secret_auto_injection_in_flight(false);
            });
        },
        move |_owner, ctx| {
            if let Some(view) = terminal_view_done.upgrade(ctx) {
                view.update(ctx, |view, _| {
                    view.set_ssh_secret_auto_injection_in_flight(false);
                });
            }
        },
    );
}

/// Result of one `next_su_password_event` cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuPasswordEvent {
    /// A genuine password prompt appeared right after an su/sudo-to-root
    /// invocation — the caller should pop the confirmation menu.
    PromptFired,
    /// An su/sudo-to-root invocation was seen but didn't lead to a genuine
    /// password prompt (shell prompt reappeared first, or the arm window
    /// timed out) — the caller should keep watching for the next one.
    StoodDown,
    /// The underlying PTY broadcast closed.
    Eof,
}

/// One full idle -> armed cycle of the su/sudo-to-root password-prompt
/// state machine (see the security note above phase 2 in
/// `spawn_su_password_injector`):
///
/// 1. **Idle**: wait until `buf` shows a genuine su/sudo-to-root invocation.
/// 2. **Armed**: from that point on (bounded by `SU_PASSWORD_ARM_TIMEOUT`),
///    the *first* thing to appear decides the outcome — a password prompt
///    fires, a shell prompt (or a timeout, or EOF) stands down. Either way
///    `buf` is cleared before returning, so unrelated text from far outside
///    this window can never combine with a later invocation to false-fire.
///
/// Pulled out of the `stream!` block as a plain function so it can be
/// driven directly in tests via `warpui::r#async::block_on`, without the
/// full `ViewContext`/`TerminalView` plumbing `ctx.spawn_stream_local`
/// requires.
async fn next_su_password_event(
    active: &mut async_broadcast::Receiver<Arc<Vec<u8>>>,
    buf: &mut Vec<u8>,
) -> SuPasswordEvent {
    // Idle: wait for a genuine su/sudo-to-root invocation to be echoed.
    loop {
        if is_su_to_root(buf) {
            break;
        }
        match active.recv().await {
            Ok(chunk) => {
                buf.extend_from_slice(&chunk);
                if buf.len() > BUFFER_HARD_LIMIT {
                    let drop_n = buf.len() - SLIDING_WINDOW_BYTES;
                    buf.drain(..drop_n);
                }
            }
            Err(_) => return SuPasswordEvent::Eof,
        }
    }

    // Armed: an su/sudo-to-root invocation was just observed. Deliberately
    // does NOT clear `buf` here first: the password prompt may have
    // arrived in the very same PTY chunk as the su/sudo echo (a single
    // `recv()` covering both is common), so the already-buffered bytes are
    // checked before waiting for more.
    let armed_result = async {
        loop {
            if bytes_look_like_shell_prompt(buf) {
                // su/sudo already resolved (e.g. NOPASSWD / cached
                // credentials) without ever asking for a password — stand down.
                return ArmOutcome::StoodDown;
            }
            if PASSWORD_PROMPT_REGEX.is_match(buf) {
                return ArmOutcome::PromptSeen;
            }
            match active.recv().await {
                Ok(chunk) => {
                    buf.extend_from_slice(&chunk);
                    if buf.len() > BUFFER_HARD_LIMIT {
                        let drop_n = buf.len() - SLIDING_WINDOW_BYTES;
                        buf.drain(..drop_n);
                    }
                }
                Err(_) => return ArmOutcome::StoodDown,
            }
        }
    }
    .with_timeout(SU_PASSWORD_ARM_TIMEOUT)
    .await
    .unwrap_or(ArmOutcome::StoodDown);

    buf.clear();
    match armed_result {
        ArmOutcome::PromptSeen => SuPasswordEvent::PromptFired,
        ArmOutcome::StoodDown => SuPasswordEvent::StoodDown,
    }
}

/// Outcome of the "armed" wait in `next_su_password_event` that follows a
/// freshly-observed su/sudo-to-root invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArmOutcome {
    /// A genuine password prompt appeared right after the invocation.
    PromptSeen,
    /// A shell prompt reappeared first (su/sudo resolved without asking for
    /// a password), or the arm window timed out without seeing either.
    StoodDown,
}

/// Checks whether the buffer contains an su command targeting root.
fn is_su_to_root(buf: &[u8]) -> bool {
    SU_ROOT_CMD_REGEX.is_match(buf)
}

pub(crate) fn should_spawn_su_password_injector(root_password: Option<&Zeroizing<String>>) -> bool {
    root_password.is_some_and(|password| !password.is_empty())
}

#[cfg(test)]
#[path = "su_password_injector_tests.rs"]
mod tests;
