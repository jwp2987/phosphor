//! The line-editor activity markers of the real bash bootstrap, in a pty (#1390, #1391).
//!
//! A Rust port of the core assertions of `script/test_line_editor_markers` (bash, honored
//! prompt): the app-side bootstrap scripts are assembled the way `bootstrap.rs` does, loaded
//! into a real interactive `bash`, and the hook stream the shell sends is checked:
//!
//! * the first prompt sends a `LineEditorReading` with the last `Precmd`'s `seq` and the session;
//! * every command produces `Preexec`, then `Precmd`, then exactly one `LineEditorReading`
//!   carrying that Precmd's `seq`.
//!
//! The test skips (passes with a message) when `bash` is not installed. Run:
//!
//! ```text
//! cargo test -p warp --test real_shell_markers
//! ```
//!
//! The zsh / fish / nu / pwsh and the continuation / builtin-read / markers-off cases stay in the
//! Python script, which drives the same shells.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use expectrl::{Regex, Session};
use serde_json::Value;

const SESSION_ID: &str = "4242";

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn read_bootstrap(name: &str) -> String {
    let path = assets().join("bundled").join("bootstrap").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Mirrors `bootstrap.rs` / the Python script's `assemble`: inline `#include`, collapse leading
/// whitespace to one space, drop blank and comment lines.
fn assemble(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        if let Some(include) = line.strip_prefix("#include ") {
            let included = std::fs::read_to_string(assets().join(include))
                .unwrap_or_else(|e| panic!("{include}: {e}"))
                .replace("@@USING_CON_PTY_BOOLEAN@@", "false");
            out.extend(assemble(&included).split('\n').map(str::to_owned));
            continue;
        }
        let stripped = line.trim_start();
        let line = if stripped.len() < line.len() {
            format!(" {stripped}")
        } else {
            line.to_owned()
        };
        let line = line.trim_end().to_owned();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

fn bash_installed() -> bool {
    Command::new("bash").arg("--version").output().is_ok()
}

/// One hook the shell sent: its name and `value` object.
struct Hook {
    name: String,
    value: Value,
}

impl Hook {
    fn seq(&self) -> Option<u64> {
        self.value.get("seq").and_then(Value::as_u64)
    }
}

/// Decodes the DCS (`ESC P $ d <hex> ST`) or unencoded OSC (`ESC ] 9278 ; f ; {json} BEL`) a
/// match of [`EVENT`] holds.
fn decode(matched: &[u8]) -> Hook {
    let json: Vec<u8> = if let Some(rest) = matched.strip_prefix(b"\x1bP$d") {
        let hex: Vec<u8> = rest
            .iter()
            .copied()
            .take_while(u8::is_ascii_hexdigit)
            .collect();
        hex.chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    } else {
        let rest = matched.strip_prefix(b"\x1b]9278;f;").expect("a 9278 f OSC");
        rest.strip_suffix(b"\x07").expect("ends in BEL").to_vec()
    };
    let parsed: Value = serde_json::from_slice(&json)
        .unwrap_or_else(|e| panic!("hook is not json ({e}): {}", String::from_utf8_lossy(&json)));
    Hook {
        name: parsed
            .get("hook")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned(),
        value: parsed.get("value").cloned().unwrap_or(Value::Null),
    }
}

/// A DCS hook (`ESC P $ d <hex>` ended by 0x9c, raw or UTF-8 encoded) or the unencoded OSC.
const EVENT: &str = r"(?s-u)\x1bP\$d[0-9a-f]*(?:\x9c|\xc2\x9c)|\x1b\]9278;f;\{.*?\}\x07";

fn next_hook(session: &mut Session) -> Hook {
    let found = session
        .expect(Regex(EVENT))
        .expect("the shell sent no further hook in time");
    let matched = found.get(0).expect("the whole match").to_vec();
    decode(&matched)
}

/// Reads hooks until one named `until` arrives; returns all of them, in order.
fn hooks_until(session: &mut Session, until: &str) -> Vec<Hook> {
    let mut seen = Vec::new();
    for _ in 0..40 {
        let hook = next_hook(session);
        let done = hook.name == until;
        seen.push(hook);
        if done {
            return seen;
        }
    }
    panic!(
        "no {until} hook; saw {:?}",
        seen.iter().map(|h| h.name.as_str()).collect::<Vec<_>>()
    );
}

fn names(hooks: &[Hook]) -> Vec<&str> {
    hooks.iter().map(|h| h.name.as_str()).collect()
}

#[test]
fn bash_bootstrap_sends_line_editor_markers_in_order() {
    if !bash_installed() {
        eprintln!("bash is not installed: skipped");
        return;
    }
    let home = tempfile::tempdir().expect("a temp home");
    let home_path = home.path().canonicalize().unwrap();

    let init = read_bootstrap("bash_init_shell.sh")
        .replace("@@WARP_SESSION_ID@@", SESSION_ID)
        .replace("@@USING_CON_PTY_BOOLEAN@@", "false");
    let rc = home_path.join("rc");
    std::fs::write(
        &rc,
        format!(
            "PS1='U> '\n{init}\n{}\n",
            assemble(&read_bootstrap("bash.sh"))
        ),
    )
    .unwrap();

    let mut command = Command::new("bash");
    command
        .args(["--noprofile", "--rcfile"])
        .arg(&rc)
        .arg("-i")
        .current_dir(&home_path)
        .env_clear()
        .env("HOME", &home_path)
        .env(
            "USER",
            std::env::var("USER").unwrap_or_else(|_| "user".into()),
        )
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("LANG", "C.UTF-8")
        .env("WARP_IS_LOCAL_SHELL_SESSION", "1")
        .env("WARP_HONOR_PS1", "1")
        .env("WARP_PROMPT_NODE_VERSION_ENABLED", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("PHOSPHOR_LINE_EDITOR_MARKERS", "1");

    let mut session = Session::spawn(command).expect("bash starts in a pty");
    session.set_expect_timeout(Some(Duration::from_secs(40)));

    // The bootstrap's own first prompt: Precmd, then one LineEditorReading with its seq.
    let boot = hooks_until(&mut session, "LineEditorReading");
    let precmd = boot
        .iter()
        .rev()
        .find(|h| h.name == "Precmd")
        .unwrap_or_else(|| panic!("no Precmd before the first reading: {:?}", names(&boot)));
    let reading = boot.last().unwrap();
    assert_eq!(
        reading.seq(),
        precmd.seq(),
        "first prompt: seq of the reading"
    );
    assert_eq!(
        reading.value.get("session_id").and_then(Value::as_u64),
        Some(SESSION_ID.parse().unwrap()),
        "first prompt: session of the reading"
    );

    // Ordinary commands: Preexec, Precmd, then exactly one reading carrying that Precmd's seq.
    for command in ["true", "echo hi"] {
        session.send(format!("{command}\r")).unwrap();
        let hooks = hooks_until(&mut session, "LineEditorReading");
        let order = names(&hooks);
        let position = |wanted: &str| order.iter().position(|name| *name == wanted);
        let (preexec, precmd_at, reading_at) = (
            position("Preexec"),
            position("Precmd"),
            position("LineEditorReading"),
        );
        assert!(
            matches!((preexec, precmd_at, reading_at), (Some(a), Some(b), Some(c)) if a < b && b < c),
            "{command:?}: expected Preexec, Precmd, LineEditorReading in order, got {order:?}"
        );
        assert_eq!(
            order
                .iter()
                .filter(|name| **name == "LineEditorReading")
                .count(),
            1,
            "{command:?}: exactly one reading, got {order:?}"
        );
        assert_eq!(
            hooks[precmd_at.unwrap()].seq(),
            hooks[reading_at.unwrap()].seq(),
            "{command:?}: the reading carries the Precmd's seq"
        );
    }

    // Leave the shell cleanly.
    let _ = session.send("exit\r");
}
