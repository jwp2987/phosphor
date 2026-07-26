//! Automatic SSH password / passphrase injection. Subscribes to a terminal
//! pane's PTY output broadcast, and once a `password:` / `passphrase:`
//! line-ending prompt is matched, writes the secret + `\n` **once**.
//!
//! ## Key design tradeoffs
//!
//! - **8KB sliding window + strict line-end matching**: the regex
//!   `(?im)(password|passphrase)[^\n]*:\s*$` only matches at line end
//!   (avoiding false hits on the word "password" in a motd / banner) + the
//!   sliding window bounds memory usage.
//!
//! - **15s timeout**: a typical SSH public-key negotiation is < 2s, and a
//!   password prompt < 5s. 15s is a reasonable ceiling for public-key auth
//!   failure + falling back to a password. **The passwordless-login-via-key
//!   boundary case** (authorized_keys configured + we also have a stored
//!   password): if the public-key handshake succeeds → no prompt appears →
//!   the injector silently times out and exits, **never mis-injecting into
//!   the post-login shell**.
//!
//! - **One-shot trigger**: breaks immediately on a match, the injector
//!   future exits → the InactiveReceiver is dropped → subsequent PTY stream
//!   is no longer seen by this injector, **preventing double injection**.
//!
//! - **bytes::Regex**: PTY output may contain incomplete UTF-8 bytes, so
//!   `regex::bytes` is used for safety.

use std::sync::Arc;
use std::time::Duration;

use async_broadcast::InactiveReceiver;
use warpui::r#async::FutureExt;
use warpui::{ViewContext, WeakViewHandle};
use zeroize::Zeroizing;

use crate::ssh_manager::password_prompt::bytes_look_like_password_prompt;
use crate::terminal::TerminalView;

/// Upper bound for the injection timeout.
const INJECT_TIMEOUT: Duration = Duration::from_secs(15);
/// The sliding window keeps this many recent bytes of PTY output for regex matching.
const SLIDING_WINDOW_BYTES: usize = 8 * 1024;
/// Once the buffer exceeds this value, it's drained down to the sliding window size.
const BUFFER_HARD_LIMIT: usize = 16 * 1024;

/// Spawns a one-shot injection task in the owner=Workspace context. The task
/// is automatically cancelled when the Workspace drops; the owner doesn't
/// need to abort it.
///
/// Precondition: `pty_reads_rx` is obtained from
/// `terminal_view.inactive_pty_reads_rx(ctx)`, and the future is only
/// actually spawned when it's **Some**; a wasm / remote session that gets
/// None is a direct no-op.
pub fn spawn_password_injector<O>(
    pty_reads_rx: Option<InactiveReceiver<Arc<Vec<u8>>>>,
    terminal_view: WeakViewHandle<TerminalView>,
    secret: Zeroizing<String>,
    ctx: &mut ViewContext<O>,
) where
    O: warpui::View + 'static,
{
    let Some(rx) = pty_reads_rx else {
        log::debug!("ssh secret injector: no pty_reads_rx (non-local session) — skip");
        return;
    };
    if secret.is_empty() {
        log::debug!("ssh secret injector: empty secret — skip");
        return;
    }

    // Set in-flight to true right away, telling the OneKey listener not to
    // pop its menu before this injection completes. This way, whether the
    // injector finishes injecting first and onekey sees the same bytes
    // afterward, or onekey sees them first, the semantics are consistent:
    // **the injector takes priority**.
    if let Some(view) = terminal_view.upgrade(ctx) {
        view.update(ctx, |view, _| {
            view.set_ssh_secret_auto_injection_in_flight(true);
        });
    }

    let owned_secret = secret.clone();
    let future = async move {
        match watch_for_prompt(rx).with_timeout(INJECT_TIMEOUT).await {
            Ok(true) => Some(owned_secret),
            Ok(false) | Err(_) => None, // EOF or timeout → no-op
        }
    };
    ctx.spawn(future, move |_owner, secret_opt, ctx| {
        let Some(view) = terminal_view.upgrade(ctx) else {
            log::debug!("ssh secret injector: terminal view dropped before injection");
            return;
        };
        let Some(secret) = secret_opt else {
            log::debug!("ssh secret injector: no prompt seen within timeout");
            view.update(ctx, |view, _| {
                view.set_ssh_secret_auto_injection_in_flight(false);
            });
            return;
        };
        view.update(ctx, |view, ctx| {
            // Write the password + newline as bytes to the PTY, equivalent
            // to simulating a keypress in response to the interactive
            // prompt. At this point ssh is already running (bootstrap
            // finished earlier), so a direct write_to_pty is the right approach.
            let mut bytes = secret.as_bytes().to_vec();
            bytes.push(b'\n');
            view.write_to_pty(bytes, ctx);
            view.note_ssh_secret_auto_injected(ctx);
            view.set_ssh_secret_auto_injection_in_flight(false);
        });
    });
}

/// Async loop: consumes the PTY broadcast, appending to the sliding window;
/// **returns true as soon as the regex hits a line-end prompt**; returns
/// false on EOF. Timeout is wrapped by the caller via `with_timeout`.
async fn watch_for_prompt(rx: InactiveReceiver<Arc<Vec<u8>>>) -> bool {
    let mut active = rx.activate_cloned();
    let mut buf: Vec<u8> = Vec::with_capacity(SLIDING_WINDOW_BYTES);
    while let Ok(chunk) = active.recv().await {
        buf.extend_from_slice(&chunk);
        if buf.len() > BUFFER_HARD_LIMIT {
            let drop_n = buf.len() - SLIDING_WINDOW_BYTES;
            buf.drain(..drop_n);
        }
        if bytes_look_like_password_prompt(&buf) {
            return true;
        }
    }
    false
}
