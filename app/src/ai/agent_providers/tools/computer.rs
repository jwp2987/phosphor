//! `request_computer_use` / `use_computer`: drive the user's mouse and keyboard.
//!
//! ## Why this file exists at all
//!
//! Every other descriptor in `tools::REGISTRY` mirrors a schema that Warp's server already
//! owns. Computer use is the exception: under the pin the client only sets
//! `AIRequestInput::computer_use_enabled` and Warp's server decides whether to offer the tool
//! and holds the JSON Schema server-side. BYOP builds the tool list locally, so the schema had
//! to be authored here from the Rust types.
//!
//! The contract is **not** the protobuf — it is
//! `TryFrom<api::message::tool_call::UseComputer> for AIAgentActionType` in
//! `crates/ai/src/agent/action/convert.rs`. Anything this schema permits that the conversion
//! rejects becomes a valid-looking tool call the model cannot fix. Every place this schema is
//! deliberately narrower than the conversion carries a comment saying why, at the point where
//! the narrowing happens.
//!
//! ## The two tools
//!
//! - `request_computer_use` — asks the user for permission, and on approval returns the screen
//!   dimensions, the platform, and the list of on-screen windows. It is the only way the model
//!   learns valid `window_id` / `pid` values, and `UseComputerExecutor::should_autoexecute`
//!   returns `true` precisely because approval was already taken here. It must be called
//!   first.
//! - `use_computer` — performs a batch of pointer / keyboard actions.
//!
//! ## Screenshots reach the model, but not through the tool result
//!
//! Both results can carry a `RawImage`. The BYOP tool-result channel cannot carry it: a tool
//! result is delivered as `genai::chat::ToolResponse { content: String }` (see
//! `lib/rust-genai/src/chat/tool/tool_response.rs`, where `content` is a plain `String` with no
//! parts), and `chat_stream::cap_tool_response_content` truncates it at 40 000 characters — two
//! orders of magnitude below a base64 screenshot, so truncation would yield a corrupt data URI
//! rather than a degraded image.
//!
//! So `result_to_json` still never embeds image bytes. Instead
//! `chat_stream::push_screenshot_attachments` appends the most recent captures to the request
//! as `ContentPart::Binary` parts on a **user** message placed after the tool results — the one
//! carrier every genai adapter auto-adapts (OpenAI `image_url`, Anthropic `image`, Gemini
//! `inline_data`). See the `screenshot attachments` section of `chat_stream.rs` for why that
//! route was chosen over widening `ToolResponse`.
//!
//! `result_to_json` has no request context (it is a bare `fn` pointer in the registry), so it
//! emits the *undecided* shape — `screenshot.captured` / `screenshot.attached` plus dimensions
//! and a `note` — and `chat_stream` overwrites `attached` / `note` through
//! [`annotate_screenshot_delivery`] once it knows what actually happened. The model is
//! therefore always told, in the result it is reading, whether it can see this capture;
//! silently returning bare metadata would leave it inferring that the screen was blank.
//!
//! The screenshot is captured, persisted and rendered in the block for the *user* regardless
//! of any of the above; the delivery decision only governs the model's copy.
//!
//! ## Keys
//!
//! `computer_use::Key` is `Keycode(i32)` or `Char(char)`, and the keycode is raw and
//! platform-specific: a macOS virtual keycode, a Windows VK, or an X11 keysym on Linux (both
//! X11 and Wayland — see `crates/computer_use/src/linux/keysym.rs`). Asking a model to emit
//! those blind is not realistic, and there is no name table anywhere in the tree. Without one
//! the tool cannot press Enter at all: `TypeText` resolves `'\n'` through `char_to_keysym` to
//! `0x0100000A`, which is in no keymap, and the macOS keycode cache has no entry for it either.
//!
//! `parse_key` therefore accepts a **key spec string** and resolves it here, in `from_args`,
//! into the exact `Key` the conversion accepts:
//!
//! - a single character (`"a"`, `"+"`, `" "`) → `Key::Char`
//! - a `0x`-prefixed platform keycode (`"0x24"`) → `Key::Keycode`, the raw escape hatch, the
//!   same form `crates/computer_use/src/bin/use_computer.rs::parse_key` already accepts
//! - a name from [`NAMED_KEYS`] (`"enter"`, `"ctrl"`, `"page_up"`) → `Key::Keycode`, resolved
//!   against the compile-time target platform
//!
//! The three tables are compiled on every platform (selection is `cfg!`, not `#[cfg]`) so all
//! of them stay type-checked and testable from any host.

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use warp_multi_agent_api as api;

use super::OpenAiTool;

pub const USE_COMPUTER_TOOL_NAME: &str = "use_computer";
pub const REQUEST_COMPUTER_USE_TOOL_NAME: &str = "request_computer_use";

/// Default long-edge cap applied when the model does not ask for one.
///
/// `convert_screenshot_params` treats `0` as "no constraint", so omitting this would persist a
/// full-resolution PNG per action in the conversation database. 1568 px is the long-edge cap
/// image-capable models converge on, and it keeps the block render sharp.
const DEFAULT_MAX_LONG_EDGE_PX: i32 = 1568;

/// Upper bound on a single `wait` action, in seconds.
///
/// `convert.rs` accepts any non-negative `Duration`; a runaway wait would block the whole
/// agent turn with no way back except user cancellation.
const MAX_WAIT_SECONDS: f64 = 300.0;

/// Upper bound on `click.count`, so one action cannot expand into an unbounded batch.
const MAX_CLICK_COUNT: u32 = 3;

// ---------------------------------------------------------------------------
// Shared argument types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize)]
struct Coord {
    x: i32,
    y: i32,
}

impl From<Coord> for api::Coordinates {
    fn from(c: Coord) -> Self {
        api::Coordinates { x: c.x, y: c.y }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Button {
    #[default]
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl Button {
    fn to_api(self) -> api::message::tool_call::use_computer::action::MouseButton {
        use api::message::tool_call::use_computer::action::MouseButton as B;
        match self {
            Button::Left => B::Left,
            Button::Right => B::Right,
            Button::Middle => B::Middle,
            Button::Back => B::Back,
            Button::Forward => B::Forward,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn to_api(self) -> api::message::tool_call::use_computer::action::mouse_wheel::Direction {
        use api::message::tool_call::use_computer::action::mouse_wheel::Direction as D;
        match self {
            Direction::Up => D::Up,
            Direction::Down => D::Down,
            Direction::Left => D::Left,
            Direction::Right => D::Right,
        }
    }
}

/// A window target.
///
/// Absent means the whole screen / frontmost application, which is exactly how
/// `convert_computer_use_target` reads an absent or `Screen` target. There is deliberately no
/// way to spell `Screen` explicitly — one representation, one meaning.
#[derive(Debug, Clone, Deserialize)]
struct WindowTarget {
    /// Opaque platform window id, echoed verbatim from a `windows[]` entry in a previous
    /// result. Kept a string because that is what the wire type is.
    window_id: String,
    pid: i32,
}

/// Builds the API target, rejecting ids the conversion would silently discard.
///
/// `convert_computer_use_target` falls back to `Target::Screen` when `window_id` does not parse
/// as a `u32` — a typo would quietly act on the whole screen instead. And `0` is the "unknown"
/// sentinel that `computer_use::Target::Window` documents as rejected by the actor. Both are
/// caught here, where the model gets a readable `invalid_arguments` tool result it can fix.
fn to_api_target(
    target: Option<&WindowTarget>,
) -> Result<Option<api::message::tool_call::ComputerUseTarget>> {
    use api::message::tool_call::computer_use_target::Target as ApiTarget;
    let Some(w) = target else {
        return Ok(None);
    };
    let parsed: u32 = w.window_id.trim().parse().map_err(|_| {
        anyhow!(
            "target.window_id must be one of the window_id values returned in a previous \
             result's `windows` list (an unsigned 32-bit integer written as a string), got {:?}",
            w.window_id
        )
    })?;
    if parsed == 0 {
        bail!("target.window_id 0 is the unknown-window sentinel and cannot be targeted");
    }
    Ok(Some(api::message::tool_call::ComputerUseTarget {
        target: Some(ApiTarget::Window(
            api::message::tool_call::computer_use_target::Window {
                window_id: parsed.to_string(),
                pid: w.pid,
            },
        )),
    }))
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ScreenshotArgs {
    #[serde(default)]
    max_long_edge_px: Option<i32>,
    #[serde(default)]
    max_total_px: Option<i32>,
    #[serde(default)]
    region: Option<RegionArgs>,
    #[serde(default)]
    target: Option<WindowTarget>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct RegionArgs {
    top_left: Coord,
    bottom_right: Coord,
}

impl ScreenshotArgs {
    fn to_api(&self) -> Result<api::message::tool_call::ScreenshotParams> {
        let region = match self.region {
            // `ScreenshotRegion::validate` rejects these at capture time with an actor error
            // that reads as a runtime failure; failing here turns it into an argument error
            // the model can correct.
            Some(r) => {
                if r.top_left.x < 0 || r.top_left.y < 0 {
                    bail!(
                        "screenshot.region.top_left must be non-negative, got ({}, {})",
                        r.top_left.x,
                        r.top_left.y
                    );
                }
                if r.bottom_right.x <= r.top_left.x || r.bottom_right.y <= r.top_left.y {
                    bail!(
                        "screenshot.region.bottom_right ({}, {}) must be strictly greater than \
                         top_left ({}, {}) in both dimensions",
                        r.bottom_right.x,
                        r.bottom_right.y,
                        r.top_left.x,
                        r.top_left.y
                    );
                }
                Some(api::message::tool_call::screenshot_params::Region {
                    top_left: Some(r.top_left.into()),
                    bottom_right: Some(r.bottom_right.into()),
                })
            }
            None => None,
        };
        Ok(api::message::tool_call::ScreenshotParams {
            // `convert_screenshot_params` reads any value <= 0 as "no constraint", so a
            // negative from the model is normalised rather than rejected.
            max_long_edge_px: self
                .max_long_edge_px
                .unwrap_or(DEFAULT_MAX_LONG_EDGE_PX)
                .max(0),
            max_total_px: self.max_total_px.unwrap_or(0).max(0),
            region,
            target: to_api_target(self.target.as_ref())?,
        })
    }
}

fn screenshot_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "max_long_edge_px": {
                "type": "integer",
                "minimum": 0,
                "description": format!(
                    "Downscale so the long edge is at most this many pixels. 0 means no \
                     constraint. Defaults to {DEFAULT_MAX_LONG_EDGE_PX}."
                )
            },
            "max_total_px": {
                "type": "integer",
                "minimum": 0,
                "description": "Downscale so the total pixel count is at most this. 0 means no constraint."
            },
            "region": {
                "type": "object",
                "description": "Capture only this rectangle of the target, in physical pixels relative to the target (screen-local for the screen, window-local for a window). bottom_right must be strictly greater than top_left in both dimensions.",
                "properties": {
                    "top_left": coordinate_schema("Top-left corner, non-negative."),
                    "bottom_right": coordinate_schema("Bottom-right corner, exclusive.")
                },
                "required": ["top_left", "bottom_right"],
                "additionalProperties": false
            },
            "target": window_target_schema("Capture this window instead of the screen.")
        },
        "additionalProperties": false
    })
}

fn coordinate_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "x": {"type": "integer"},
            "y": {"type": "integer"}
        },
        "required": ["x", "y"],
        "additionalProperties": false
    })
}

fn window_target_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!(
            "{description} Omit to act on the whole screen / frontmost application. Both fields \
             must be copied verbatim from an entry of the `windows` list in a previous \
             request_computer_use or use_computer result. If that list came back empty, this \
             client cannot drive individual windows — omit target and work on the screen."
        ),
        "properties": {
            "window_id": {
                "type": "string",
                "description": "Opaque window id, exactly as returned in a previous result's windows[].window_id."
            },
            "pid": {
                "type": "integer",
                "description": "Owning process id, exactly as returned in a previous result's windows[].pid."
            }
        },
        "required": ["window_id", "pid"],
        "additionalProperties": false
    })
}

// ---------------------------------------------------------------------------
// Key specs
// ---------------------------------------------------------------------------

/// The named keys [`parse_key`] resolves, in the order they are advertised to the model.
///
/// Kept to keys that exist with the same meaning on macOS, Windows and X11, so a name never
/// resolves on one platform and fails on another.
pub const NAMED_KEYS: &[&str] = &[
    "enter",
    "tab",
    "escape",
    "backspace",
    "delete",
    "space",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "page_up",
    "page_down",
    "shift",
    "control",
    "alt",
    "meta",
    "caps_lock",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

/// Folds spelling variants onto the canonical names in [`NAMED_KEYS`].
fn canonical_key_name(name: &str) -> &str {
    match name {
        "return" | "cr" => "enter",
        "esc" => "escape",
        "bksp" | "back_space" => "backspace",
        "del" | "forward_delete" => "delete",
        "spacebar" => "space",
        "arrow_up" => "up",
        "arrow_down" => "down",
        "arrow_left" => "left",
        "arrow_right" => "right",
        "pgup" | "pageup" | "prior" => "page_up",
        "pgdn" | "pagedown" | "next" => "page_down",
        "ctrl" => "control",
        "opt" | "option" => "alt",
        "cmd" | "command" | "super" | "win" | "windows" => "meta",
        "capslock" => "caps_lock",
        other => other,
    }
}

/// macOS virtual keycodes (`kVK_*` from `HIToolbox/Events.h`).
fn mac_keycode(name: &str) -> Option<i32> {
    Some(match name {
        "enter" => 0x24,
        "tab" => 0x30,
        "space" => 0x31,
        "backspace" => 0x33,
        "escape" => 0x35,
        "meta" => 0x37,
        "shift" => 0x38,
        "caps_lock" => 0x39,
        "alt" => 0x3A,
        "control" => 0x3B,
        "f5" => 0x60,
        "f6" => 0x61,
        "f7" => 0x62,
        "f3" => 0x63,
        "f8" => 0x64,
        "f9" => 0x65,
        "f11" => 0x67,
        "f10" => 0x6D,
        "f12" => 0x6F,
        "home" => 0x73,
        "page_up" => 0x74,
        "delete" => 0x75,
        "f4" => 0x76,
        "end" => 0x77,
        "f2" => 0x78,
        "page_down" => 0x79,
        "f1" => 0x7A,
        "left" => 0x7B,
        "right" => 0x7C,
        "down" => 0x7D,
        "up" => 0x7E,
        _ => return None,
    })
}

/// Windows virtual-key codes (`VK_*` from `winuser.h`).
fn windows_vk(name: &str) -> Option<i32> {
    Some(match name {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" => 0x0D,
        "shift" => 0x10,
        "control" => 0x11,
        "alt" => 0x12,
        "caps_lock" => 0x14,
        "escape" => 0x1B,
        "space" => 0x20,
        "page_up" => 0x21,
        "page_down" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "delete" => 0x2E,
        "meta" => 0x5B,
        "f1" => 0x70,
        "f2" => 0x71,
        "f3" => 0x72,
        "f4" => 0x73,
        "f5" => 0x74,
        "f6" => 0x75,
        "f7" => 0x76,
        "f8" => 0x77,
        "f9" => 0x78,
        "f10" => 0x79,
        "f11" => 0x7A,
        "f12" => 0x7B,
        _ => return None,
    })
}

/// X11 keysyms (`keysymdef.h`). Used by both the X11 and the Wayland backend — see the module
/// doc on `crates/computer_use/src/linux/keysym.rs`.
fn x11_keysym(name: &str) -> Option<i32> {
    Some(match name {
        "space" => 0x0020,
        "backspace" => 0xFF08,
        "tab" => 0xFF09,
        "enter" => 0xFF0D,
        "escape" => 0xFF1B,
        "home" => 0xFF50,
        "left" => 0xFF51,
        "up" => 0xFF52,
        "right" => 0xFF53,
        "down" => 0xFF54,
        "page_up" => 0xFF55,
        "page_down" => 0xFF56,
        "end" => 0xFF57,
        "f1" => 0xFFBE,
        "f2" => 0xFFBF,
        "f3" => 0xFFC0,
        "f4" => 0xFFC1,
        "f5" => 0xFFC2,
        "f6" => 0xFFC3,
        "f7" => 0xFFC4,
        "f8" => 0xFFC5,
        "f9" => 0xFFC6,
        "f10" => 0xFFC7,
        "f11" => 0xFFC8,
        "f12" => 0xFFC9,
        "shift" => 0xFFE1,
        "control" => 0xFFE3,
        "caps_lock" => 0xFFE5,
        "alt" => 0xFFE9,
        "meta" => 0xFFEB,
        "delete" => 0xFFFF,
        _ => return None,
    })
}

/// Resolves a canonical key name against the platform this binary was built for.
///
/// The client and the actor are the same process, so the build target *is* the target platform;
/// `cfg!` rather than `#[cfg]` keeps all three tables compiled and testable everywhere.
fn named_key_to_keycode(name: &str) -> Option<i32> {
    if cfg!(target_os = "macos") {
        mac_keycode(name)
    } else if cfg!(target_os = "windows") {
        windows_vk(name)
    } else {
        x11_keysym(name)
    }
}

/// Parses one key spec into the `Key` message `convert_key` accepts.
fn parse_key(spec: &str) -> Result<api::message::tool_call::use_computer::action::Key> {
    use api::message::tool_call::use_computer::action::{Key, key};

    let keycode = |code: i32| Key {
        data: Some(key::Data::Keycode(code)),
    };

    // A single character is taken literally, before any trimming, so " " means the space
    // character rather than an empty spec. `convert_key` requires exactly one `char`, which is
    // guaranteed here by construction.
    let mut chars = spec.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        return Ok(Key {
            data: Some(key::Data::Char(ch.to_string())),
        });
    }

    let normalized = spec.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    if normalized.is_empty() {
        bail!("key must not be empty");
    }
    if let Some(hex) = normalized.strip_prefix("0x") {
        let code = i32::from_str_radix(hex, 16)
            .map_err(|_| anyhow!("invalid hexadecimal platform keycode {spec:?}"))?;
        return Ok(keycode(code));
    }
    named_key_to_keycode(canonical_key_name(&normalized))
        .map(keycode)
        .ok_or_else(|| {
            anyhow!(
                "unknown key {spec:?}: expected a single character, one of [{}], or a \
                 0x-prefixed platform keycode",
                NAMED_KEYS.join(", ")
            )
        })
}

fn key_spec_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "description": format!(
            "{description} One of: a single character (\"a\", \"+\", \" \"); a named key, one \
             of [{}] (aliases such as return/esc/ctrl/cmd/pgup are accepted); or a \
             0x-prefixed platform keycode for anything else (a macOS virtual keycode, a \
             Windows VK, or an X11 keysym on Linux — check `platform` from \
             request_computer_use before using this form).",
            NAMED_KEYS.join(", ")
        )
    })
}

// ---------------------------------------------------------------------------
// use_computer
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UseComputerArgs {
    /// Purely human-facing (shown to the user describing what the batch does) — same shape
    /// as `apply_file_diffs`'s `summary` (see `edit.rs`), so it gets the same treatment: the
    /// advertised schema keeps it `required` for well-behaved models, but the parser accepts
    /// its absence rather than losing a whole batch of mouse/keyboard actions to a display-only
    /// field. When absent, `from_args` derives a fallback from `actions`.
    #[serde(default)]
    action_summary: Option<String>,
    actions: Vec<ActionArg>,
    #[serde(default)]
    screenshot: Option<ScreenshotArgs>,
}

/// Fallback used when the model omits `action_summary`. Mirrors `edit.rs`'s
/// `fallback_summary`: a short, generic description derived from the action kinds, so the
/// user still sees something meaningful instead of losing the whole action batch.
fn fallback_action_summary(actions: &[ActionArg]) -> String {
    if let [only] = actions {
        return action_kind_label(only).to_owned();
    }
    format!(
        "Perform {} computer action{}",
        actions.len(),
        if actions.len() == 1 { "" } else { "s" }
    )
}

fn action_kind_label(action: &ActionArg) -> &'static str {
    match action {
        ActionArg::MouseMove { .. } => "Move the mouse",
        ActionArg::MouseDown { .. } => "Press the mouse button",
        ActionArg::MouseUp { .. } => "Release the mouse button",
        ActionArg::Click { .. } => "Click",
        ActionArg::MouseWheel { .. } => "Scroll",
        ActionArg::Wait { .. } => "Wait",
        ActionArg::TypeText { .. } => "Type text",
        ActionArg::KeyDown { .. } => "Press a key",
        ActionArg::KeyUp { .. } => "Release a key",
        ActionArg::KeyPress { .. } => "Press a key",
    }
}

/// One entry of the `actions` array.
///
/// `click` and `key_press` are convenience shapes: they expand into the exact
/// `MouseDown`/`MouseUp` and `KeyDown`/`KeyUp` pairs the conversion accepts, and exist because
/// a model that has to emit the halves itself eventually leaves a button or a modifier held
/// down, which is a stuck input device rather than a failed tool call.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ActionArg {
    MouseMove {
        to: Coord,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    MouseDown {
        at: Coord,
        #[serde(default)]
        button: Button,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    MouseUp {
        #[serde(default)]
        button: Button,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    Click {
        at: Coord,
        #[serde(default)]
        button: Button,
        #[serde(default)]
        count: Option<u32>,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    MouseWheel {
        at: Coord,
        direction: Direction,
        #[serde(default)]
        pixels: Option<i32>,
        #[serde(default)]
        clicks: Option<i32>,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    Wait {
        seconds: f64,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    TypeText {
        text: String,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    KeyDown {
        key: String,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    KeyUp {
        key: String,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
    KeyPress {
        key: String,
        #[serde(default)]
        modifiers: Vec<String>,
        #[serde(default)]
        target: Option<WindowTarget>,
    },
}

impl ActionArg {
    fn target(&self) -> Option<&WindowTarget> {
        match self {
            ActionArg::MouseMove { target, .. }
            | ActionArg::MouseDown { target, .. }
            | ActionArg::MouseUp { target, .. }
            | ActionArg::Click { target, .. }
            | ActionArg::MouseWheel { target, .. }
            | ActionArg::Wait { target, .. }
            | ActionArg::TypeText { target, .. }
            | ActionArg::KeyDown { target, .. }
            | ActionArg::KeyUp { target, .. }
            | ActionArg::KeyPress { target, .. } => target.as_ref(),
        }
    }

    /// Expands one entry into the one-or-more oneof payloads it stands for.
    fn to_api_types(&self) -> Result<Vec<api::message::tool_call::use_computer::action::Type>> {
        use api::message::tool_call::use_computer::action::{
            self as act, KeyDown, KeyUp, MouseDown, MouseMove, MouseUp, MouseWheel, TypeText, Wait,
            mouse_wheel,
        };

        Ok(match self {
            ActionArg::MouseMove { to, .. } => vec![act::Type::MouseMove(MouseMove {
                to: Some((*to).into()),
            })],
            ActionArg::MouseDown { at, button, .. } => vec![act::Type::MouseDown(MouseDown {
                button: button.to_api().into(),
                at: Some((*at).into()),
            })],
            ActionArg::MouseUp { button, .. } => vec![act::Type::MouseUp(MouseUp {
                button: button.to_api().into(),
            })],
            ActionArg::Click {
                at, button, count, ..
            } => {
                let count = count.unwrap_or(1);
                if count == 0 || count > MAX_CLICK_COUNT {
                    bail!("click.count must be between 1 and {MAX_CLICK_COUNT}, got {count}");
                }
                let mut out = Vec::with_capacity(count as usize * 2);
                for _ in 0..count {
                    out.push(act::Type::MouseDown(MouseDown {
                        button: button.to_api().into(),
                        at: Some((*at).into()),
                    }));
                    out.push(act::Type::MouseUp(MouseUp {
                        button: button.to_api().into(),
                    }));
                }
                out
            }
            ActionArg::MouseWheel {
                at,
                direction,
                pixels,
                clicks,
                ..
            } => {
                // `to_scroll_distance` errors on an absent oneof, so exactly one of the two
                // must be present. Both would silently drop one on the proto side.
                let distance = match (pixels, clicks) {
                    (Some(_), Some(_)) => {
                        bail!("mouse_wheel takes exactly one of pixels or clicks, not both")
                    }
                    (Some(p), None) => mouse_wheel::Distance::Pixels(*p),
                    (None, Some(c)) => mouse_wheel::Distance::Clicks(*c),
                    (None, None) => {
                        bail!("mouse_wheel requires a scroll distance: set either pixels or clicks")
                    }
                };
                vec![act::Type::MouseWheel(MouseWheel {
                    at: Some((*at).into()),
                    direction: direction.to_api().into(),
                    distance: Some(distance),
                })]
            }
            ActionArg::Wait { seconds, .. } => {
                if !seconds.is_finite() || *seconds < 0.0 {
                    bail!("wait.seconds must be a non-negative number, got {seconds}");
                }
                if *seconds > MAX_WAIT_SECONDS {
                    bail!(
                        "wait.seconds must be at most {MAX_WAIT_SECONDS}, got {seconds}; split a \
                         longer pause across several actions or turns"
                    );
                }
                let whole = seconds.trunc();
                vec![act::Type::Wait(Wait {
                    duration: Some(prost_types::Duration {
                        seconds: whole as i64,
                        nanos: ((seconds - whole) * 1e9).round() as i32,
                    }),
                })]
            }
            ActionArg::TypeText { text, .. } => {
                vec![act::Type::TypeText(TypeText { text: text.clone() })]
            }
            ActionArg::KeyDown { key, .. } => vec![act::Type::KeyDown(KeyDown {
                key: Some(parse_key(key)?),
            })],
            ActionArg::KeyUp { key, .. } => vec![act::Type::KeyUp(KeyUp {
                key: Some(parse_key(key)?),
            })],
            ActionArg::KeyPress { key, modifiers, .. } => {
                let main = parse_key(key)?;
                let mods = modifiers
                    .iter()
                    .map(|m| parse_key(m))
                    .collect::<Result<Vec<_>>>()?;
                let mut out = Vec::with_capacity(mods.len() * 2 + 2);
                for m in &mods {
                    out.push(act::Type::KeyDown(KeyDown {
                        key: Some(m.clone()),
                    }));
                }
                out.push(act::Type::KeyDown(KeyDown {
                    key: Some(main.clone()),
                }));
                out.push(act::Type::KeyUp(KeyUp { key: Some(main) }));
                for m in mods.iter().rev() {
                    out.push(act::Type::KeyUp(KeyUp {
                        key: Some(m.clone()),
                    }));
                }
                out
            }
        })
    }
}

fn use_computer_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action_summary": {
                "type": "string",
                "description": "One short sentence shown to the user describing what this batch does (e.g. \"Click Save in the settings dialog\"). Write it in the same language as the user's messages."
            },
            "actions": {
                "type": "array",
                "minItems": 1,
                "description": "Actions to perform in order. Coordinates are physical pixels, screen-local when target is omitted and window-local when it is set.",
                "items": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": [
                                "mouse_move", "mouse_down", "mouse_up", "click", "mouse_wheel",
                                "wait", "type_text", "key_down", "key_up", "key_press"
                            ],
                            "description": "Which action this entry is. The other fields required depend on it: mouse_move needs to; mouse_down and click need at; mouse_wheel needs at, direction and exactly one of pixels or clicks; wait needs seconds; type_text needs text; key_down, key_up and key_press need key."
                        },
                        "to": coordinate_schema("mouse_move: where to move the pointer."),
                        "at": coordinate_schema("mouse_down / click / mouse_wheel: where the action happens."),
                        "button": {
                            "type": "string",
                            "enum": ["left", "right", "middle", "back", "forward"],
                            "description": "mouse_down / mouse_up / click: which button. Defaults to left."
                        },
                        "count": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": MAX_CLICK_COUNT,
                            "description": "click: number of clicks (2 for a double-click). Defaults to 1."
                        },
                        "direction": {
                            "type": "string",
                            "enum": ["up", "down", "left", "right"],
                            "description": "mouse_wheel: scroll direction."
                        },
                        "pixels": {
                            "type": "integer",
                            "description": "mouse_wheel: scroll distance in pixels. Set exactly one of pixels or clicks."
                        },
                        "clicks": {
                            "type": "integer",
                            "description": "mouse_wheel: scroll distance in wheel notches. Set exactly one of pixels or clicks."
                        },
                        "seconds": {
                            "type": "number",
                            "minimum": 0,
                            "maximum": MAX_WAIT_SECONDS,
                            "description": "wait: how long to pause before the next action, letting the UI settle."
                        },
                        "text": {
                            "type": "string",
                            "description": "type_text: literal text to type. It cannot produce Enter or Tab — use key_press for those."
                        },
                        "key": key_spec_schema("key_down / key_up / key_press: the key."),
                        "modifiers": {
                            "type": "array",
                            "items": key_spec_schema("A modifier held down for the duration of the key press, e.g. control, shift, alt, meta."),
                            "description": "key_press only: modifiers pressed before the key and released after it, in reverse order."
                        },
                        "target": window_target_schema("Perform this action on this window.")
                    },
                    "required": ["action"],
                    "additionalProperties": false
                }
            },
            "screenshot": screenshot_schema(
                "Constraints for the screenshot captured after the actions run. A screenshot is \
                 always captured; this only shapes it. Whether a copy of the image is sent back \
                 to you depends on the model — the result's `screenshot.delivery` field says \
                 which happened."
            )
        },
        // `action_summary` is `required` here so a well-behaved model still sends one — but
        // the parser (`UseComputerArgs::action_summary`, above) accepts its absence and
        // synthesizes a fallback. The schema is guidance for good models; the parser must be
        // forgiving of bad ones.
        "required": ["action_summary", "actions"],
        "additionalProperties": false
    })
}

fn use_computer_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: UseComputerArgs = serde_json::from_str(args)?;
    if parsed.actions.is_empty() {
        bail!("actions must contain at least one action");
    }
    let action_summary = parsed
        .action_summary
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| fallback_action_summary(&parsed.actions));
    let mut actions = Vec::with_capacity(parsed.actions.len());
    for arg in &parsed.actions {
        let target = to_api_target(arg.target())?;
        for r#type in arg.to_api_types()? {
            actions.push(api::message::tool_call::use_computer::Action {
                r#type: Some(r#type),
                target: target.clone(),
            });
        }
    }
    Ok(api::message::tool_call::Tool::UseComputer(
        api::message::tool_call::UseComputer {
            actions,
            // Always `Some`. Warp's server always requests a post-action capture, the restored
            // block render displays it to the user, and it is what refreshes `windows`.
            post_actions_screenshot_params: Some(parsed.screenshot.unwrap_or_default().to_api()?),
            action_summary,
        },
    ))
}

// ---------------------------------------------------------------------------
// History replay: tool_call → args JSON
// ---------------------------------------------------------------------------
//
// `chat_stream::serialize_outgoing_tool_call` replays every persisted `ToolCall` back to the
// model as its own past assistant turn. Its catch-all arm renames anything it does not know to
// `warp_internal_<Variant>` with `{}` arguments, so without the two functions below a
// multi-turn computer-use session shows the model a prior call named
// `warp_internal_UseComputer` that it never made and cannot correlate with the result it is
// looking at. Sightedness is worthless if the agent cannot tell which click produced the
// screen it is being shown.
//
// The inverse is deliberately *primitive*: `click` and `key_press` are convenience shapes that
// expanded into `MouseDown`/`MouseUp` and `KeyDown`/`KeyUp` pairs on the way in, and there is
// no way to tell a collapsed `click` from two hand-written halves after the fact. Emitting the
// halves is faithful to what was actually performed and still parses back through `from_args`,
// which accepts both forms.

fn button_name(button: api::message::tool_call::use_computer::action::MouseButton) -> &'static str {
    use api::message::tool_call::use_computer::action::MouseButton as B;
    match button {
        B::Left => "left",
        B::Right => "right",
        B::Middle => "middle",
        B::Back => "back",
        B::Forward => "forward",
    }
}

fn direction_name(
    direction: api::message::tool_call::use_computer::action::mouse_wheel::Direction,
) -> &'static str {
    use api::message::tool_call::use_computer::action::mouse_wheel::Direction as D;
    match direction {
        D::Up => "up",
        D::Down => "down",
        D::Left => "left",
        D::Right => "right",
    }
}

/// Renders a `Key` back into the spec string [`parse_key`] accepts.
///
/// A keycode always comes back as the `0x` escape hatch rather than a name: the name tables
/// are one-way (several names can share a code across platforms), and the escape hatch
/// round-trips exactly.
fn key_spec(key: &api::message::tool_call::use_computer::action::Key) -> String {
    use api::message::tool_call::use_computer::action::key;
    match key.data.as_ref() {
        Some(key::Data::Char(c)) => c.clone(),
        // Platform keycodes (macOS virtual keycodes, Windows VKs, X11 keysyms) are all
        // non-negative, so the hex form always parses back through `i32::from_str_radix`.
        Some(key::Data::Keycode(code)) => format!("0x{code:x}"),
        None => String::new(),
    }
}

fn coord_json(c: Option<&api::Coordinates>) -> Value {
    match c {
        Some(c) => json!({"x": c.x, "y": c.y}),
        None => json!({"x": 0, "y": 0}),
    }
}

/// Inverse of [`to_api_target`]. `Screen` and an absent target are the same thing to the
/// schema, so both come back as no `target` key at all.
fn target_json(target: Option<&api::message::tool_call::ComputerUseTarget>) -> Option<Value> {
    use api::message::tool_call::computer_use_target::Target;
    match target?.target.as_ref()? {
        Target::Window(w) => Some(json!({"window_id": w.window_id, "pid": w.pid})),
        Target::Screen(_) => None,
    }
}

fn screenshot_params_json(params: &api::message::tool_call::ScreenshotParams) -> Value {
    let mut out = json!({
        "max_long_edge_px": params.max_long_edge_px,
        "max_total_px": params.max_total_px,
    });
    if let Some(region) = params.region.as_ref() {
        out["region"] = json!({
            "top_left": coord_json(region.top_left.as_ref()),
            "bottom_right": coord_json(region.bottom_right.as_ref()),
        });
    }
    if let Some(target) = target_json(params.target.as_ref()) {
        out["target"] = target;
    }
    out
}

/// Replays a persisted `use_computer` call as the args JSON the model originally sent.
pub fn serialize_outgoing_use_computer(uc: &api::message::tool_call::UseComputer) -> Value {
    use api::message::tool_call::use_computer::action::{self as act, mouse_wheel};

    let actions: Vec<Value> = uc
        .actions
        .iter()
        .filter_map(|action| {
            let mut value = match action.r#type.as_ref()? {
                act::Type::MouseMove(m) => json!({
                    "action": "mouse_move",
                    "to": coord_json(m.to.as_ref()),
                }),
                act::Type::MouseDown(m) => json!({
                    "action": "mouse_down",
                    "at": coord_json(m.at.as_ref()),
                    "button": button_name(m.button()),
                }),
                act::Type::MouseUp(m) => json!({
                    "action": "mouse_up",
                    "button": button_name(m.button()),
                }),
                act::Type::MouseWheel(m) => {
                    let mut wheel = json!({
                        "action": "mouse_wheel",
                        "at": coord_json(m.at.as_ref()),
                        "direction": direction_name(m.direction()),
                    });
                    match m.distance.as_ref() {
                        Some(mouse_wheel::Distance::Pixels(p)) => wheel["pixels"] = json!(p),
                        Some(mouse_wheel::Distance::Clicks(c)) => wheel["clicks"] = json!(c),
                        // `from_args` rejects a wheel action with neither; a persisted one
                        // that has neither was never executable, so replay it as zero pixels
                        // rather than emit a shape the schema forbids.
                        None => wheel["pixels"] = json!(0),
                    }
                    wheel
                }
                act::Type::Wait(w) => {
                    let seconds = w
                        .duration
                        .as_ref()
                        .map(|d| d.seconds as f64 + d.nanos as f64 / 1e9)
                        .unwrap_or(0.0);
                    json!({"action": "wait", "seconds": seconds})
                }
                act::Type::TypeText(t) => json!({"action": "type_text", "text": t.text}),
                act::Type::KeyDown(k) => json!({
                    "action": "key_down",
                    "key": k.key.as_ref().map(key_spec).unwrap_or_default(),
                }),
                act::Type::KeyUp(k) => json!({
                    "action": "key_up",
                    "key": k.key.as_ref().map(key_spec).unwrap_or_default(),
                }),
            };
            if let Some(target) = target_json(action.target.as_ref()) {
                value["target"] = target;
            }
            Some(value)
        })
        .collect();

    let mut out = json!({
        "action_summary": uc.action_summary,
        "actions": actions,
    });
    if let Some(params) = uc.post_actions_screenshot_params.as_ref() {
        out["screenshot"] = screenshot_params_json(params);
    }
    out
}

/// Replays a persisted `request_computer_use` call as the args JSON the model originally sent.
pub fn serialize_outgoing_request_computer_use(
    rc: &api::message::tool_call::RequestComputerUse,
) -> Value {
    let mut out = json!({ "task_summary": rc.task_summary });
    if let Some(params) = rc.screenshot_params.as_ref() {
        out["screenshot"] = screenshot_params_json(params);
    }
    out
}

/// Shared by both tools: a `windows` array the model can copy `window_id` / `pid` out of.
fn windows_to_json(windows: &[api::WindowInfo]) -> Value {
    Value::Array(
        windows
            .iter()
            .map(|w| {
                json!({
                    "window_id": w.window_id,
                    "pid": w.pid,
                    "app_name": w.app_name,
                    "title": w.title,
                    "layer": w.layer,
                })
            })
            .collect(),
    )
}

/// The `note` `result_to_json` emits before `chat_stream` decides what to do with the bytes.
///
/// `result_to_json` is a bare `fn` pointer in the registry — it has no request context, so it
/// cannot know whether the model can take images or whether this particular capture is the one
/// being attached. It therefore emits the conservative shape (`attached: false`) plus this
/// note, and [`annotate_screenshot_delivery`] overwrites both once the answer is known. This
/// string is what survives if a future caller serializes a result *without* annotating it.
const SCREENSHOT_UNDECIDED_NOTE: &str = "The screenshot was captured and is shown to the user. Whether a copy reaches you is decided \
     when the request is assembled: it is never embedded in this tool result, only ever \
     attached to a separate user message that follows it. If no such message is present, you \
     cannot see this capture.";

/// `note` when the image travels with the request.
const SCREENSHOT_ATTACHED_NOTE: &str = "The image is attached to the user message that follows these tool results. Coordinates you \
     read off it are in the *image's* pixel space; that message states the scale factor to \
     apply before passing them to use_computer.";

/// `note` when the model is text-only.
const SCREENSHOT_MODEL_BLIND_NOTE: &str = "The screenshot cannot be sent to you: the model configured for this conversation does not \
     accept image input. You cannot see the screen. Work from the window list, the coordinates \
     you asked for, and what the user tells you, and ask the user to describe what is on \
     screen when you need to look at something.";

/// `note` when a newer capture won the attachment budget.
const SCREENSHOT_SUPERSEDED_NOTE: &str = "This capture is stale: a newer screenshot is attached instead, and only the most recent few \
     are sent to keep the conversation inside its context window. Read the current state off \
     the newest attached image, not off this result.";

/// `note` when preparing the bytes for the request failed or blew the size bound.
const SCREENSHOT_UNDELIVERABLE_NOTE: &str = "The screenshot was captured but could not be prepared for sending (it failed to decode, or \
     stayed over the size limit after downscaling), so you cannot see this one. Take another \
     screenshot with a smaller max_long_edge_px, or narrow it with a region.";

/// What happened to a captured screenshot, from the model's point of view.
///
/// Decided in `chat_stream` (which knows the model's `AttachmentCaps` and the per-request
/// attachment budget) and written back into the already-serialized result JSON by
/// [`annotate_screenshot_delivery`], so the text the model reads and the images it receives
/// can never disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotDelivery {
    /// The bytes travel with this request, on the user message that follows the tool results.
    Attached,
    /// The model cannot consume images at all (`AttachmentCaps::images == false`).
    ModelCannotSeeImages,
    /// The model can see images, but a newer capture took the budget.
    Superseded,
    /// Decoding / re-encoding / the size bound rejected these particular bytes.
    Undeliverable,
}

impl ScreenshotDelivery {
    fn attached(self) -> bool {
        matches!(self, ScreenshotDelivery::Attached)
    }

    fn note(self) -> &'static str {
        match self {
            ScreenshotDelivery::Attached => SCREENSHOT_ATTACHED_NOTE,
            ScreenshotDelivery::ModelCannotSeeImages => SCREENSHOT_MODEL_BLIND_NOTE,
            ScreenshotDelivery::Superseded => SCREENSHOT_SUPERSEDED_NOTE,
            ScreenshotDelivery::Undeliverable => SCREENSHOT_UNDELIVERABLE_NOTE,
        }
    }

    /// The machine-readable form, so a model does not have to parse English to branch on it.
    fn as_str(self) -> &'static str {
        match self {
            ScreenshotDelivery::Attached => "attached_to_following_user_message",
            ScreenshotDelivery::ModelCannotSeeImages => "model_cannot_see_images",
            ScreenshotDelivery::Superseded => "superseded_by_newer_screenshot",
            ScreenshotDelivery::Undeliverable => "undeliverable",
        }
    }
}

/// Rewrites the `screenshot` object of an already-serialized computer-use result so it states
/// what actually happened to the bytes.
///
/// A no-op unless `value.screenshot.captured` is `true`, so it is safe to call on any result
/// (including `rejected` / `error` / `cancelled` shapes, and on other tools' payloads).
pub fn annotate_screenshot_delivery(value: &mut Value, delivery: ScreenshotDelivery) {
    let Some(shot) = value.get_mut("screenshot").and_then(Value::as_object_mut) else {
        return;
    };
    if shot.get("captured").and_then(Value::as_bool) != Some(true) {
        return;
    }
    shot.insert("attached".to_owned(), Value::Bool(delivery.attached()));
    shot.insert(
        "delivery".to_owned(),
        Value::String(delivery.as_str().to_owned()),
    );
    shot.insert("note".to_owned(), Value::String(delivery.note().to_owned()));
}

/// The screenshot a computer-use result carries, if any.
///
/// Covers both descriptors: `use_computer`'s post-action capture and `request_computer_use`'s
/// initial capture. Returns `None` for every other tool's result, which is what lets
/// `chat_stream` use it as the "is this a computer-use result with an image?" test.
pub fn screenshot_of(result: &api::message::tool_call_result::Result) -> Option<&api::RawImage> {
    use api::message::tool_call_result::Result as R;
    match result {
        R::UseComputer(r) => match r.result.as_ref()? {
            api::use_computer_result::Result::Success(s) => s.screenshot.as_ref(),
            _ => None,
        },
        R::RequestComputerUseResult(r) => match r.result.as_ref()? {
            api::request_computer_use_result::Result::Approved(a) => a.initial_screenshot.as_ref(),
            _ => None,
        },
        _ => None,
    }
}

/// The size, in physical pixels, of the surface a computer-use screenshot depicts.
///
/// This is what makes an attached image *usable*: the capture is downscaled (see
/// `DEFAULT_MAX_LONG_EDGE_PX`) but `use_computer` coordinates are physical pixels of the
/// surface, so the model needs both numbers to map one to the other.
///
/// - `request_computer_use` approval carries `screen_dimensions` directly.
/// - `use_computer` success carries `captured_window` when a window was targeted; a
///   full-screen capture has no dimensions of its own, and the caller supplies the screen size
///   remembered from the approval.
pub fn captured_surface_px(result: &api::message::tool_call_result::Result) -> Option<(i32, i32)> {
    use api::message::tool_call_result::Result as R;
    match result {
        R::UseComputer(r) => match r.result.as_ref()? {
            api::use_computer_result::Result::Success(s) => s
                .captured_window
                .as_ref()
                .map(|w| (w.width_px, w.height_px)),
            _ => None,
        },
        R::RequestComputerUseResult(r) => match r.result.as_ref()? {
            api::request_computer_use_result::Result::Approved(a) => a
                .screen_dimensions
                .as_ref()
                .map(|d| (d.width_px, d.height_px)),
            _ => None,
        },
        _ => None,
    }
}

fn screenshot_to_json(image: Option<&api::RawImage>) -> Value {
    match image {
        Some(img) => json!({
            "captured": true,
            "attached": false,
            "delivery": "undecided",
            "width_px": img.width,
            "height_px": img.height,
            "mime_type": img.mime_type,
            "note": SCREENSHOT_UNDECIDED_NOTE,
        }),
        None => json!({ "captured": false, "attached": false }),
    }
}

fn use_computer_result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::message::tool_call_result::Result as R;
    use api::use_computer_result::Result as UR;
    let r = match result {
        R::UseComputer(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(UR::Success(s)) => json!({
            "status": "ok",
            "screenshot": screenshot_to_json(s.screenshot.as_ref()),
            "cursor_position": s.cursor_position.as_ref().map(|c| json!({"x": c.x, "y": c.y})),
            "captured_window": s.captured_window.as_ref().map(|c| json!({
                "window_id": c.window_id,
                "width_px": c.width_px,
                "height_px": c.height_px,
            })),
            "windows": windows_to_json(&s.windows),
        }),
        Some(UR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static USE_COMPUTER: OpenAiTool = OpenAiTool {
    name: USE_COMPUTER_TOOL_NAME,
    description: "Drive the user's mouse and keyboard: move, click, scroll, type, and press \
                  keys, optionally against one specific window. Only usable after \
                  request_computer_use has been approved in this conversation — that call is \
                  also how you learn the screen dimensions and the window_id / pid values you \
                  may target. Actions run in order, so batch a whole interaction (click a \
                  field, type into it, press enter) into one call and add short waits where the \
                  UI needs to settle. IMPORTANT: a screenshot is captured after the batch and \
                  shown to the user, but this model cannot accept image input, so you never see \
                  the screen. Do not guess at coordinates you were not given: work from what \
                  the user told you and from the window list in the result, and ask the user \
                  when you need to know what is on screen. Prefer keyboard navigation over \
                  pointer aiming for the same reason. This is a real, irreversible interaction \
                  with the user's desktop — stay inside what they asked for.",
    parameters: use_computer_parameters,
    from_args: use_computer_from_args,
    result_to_json: use_computer_result_to_json,
};

/// `use_computer`'s description for a model that *can* see the attached screenshot.
///
/// The static above is the blind wording, kept as the default because `AttachmentCaps` are a
/// per-request property the registry cannot see. `chat_stream::build_tools_array` swaps in this
/// text when `AttachmentCaps::images` is set — a tool description that tells a sighted model it
/// is blind is worse than no description at all: it stops the model looking at an image that is
/// right there.
const USE_COMPUTER_DESCRIPTION_SIGHTED: &str =
    "Drive the user's mouse and keyboard: move, click, scroll, type, and press keys, optionally \
     against one specific window. Only usable after request_computer_use has been approved in \
     this conversation — that call is also how you learn the screen dimensions and the \
     window_id / pid values you may target. Actions run in order, so batch a whole interaction \
     (click a field, type into it, press enter) into one call and add short waits where the UI \
     needs to settle. A screenshot is captured after the batch and attached to the user message \
     that follows the tool results, so you can see the effect of what you did — check it before \
     the next batch instead of assuming an action landed. That image is downscaled: the message \
     it arrives on states the scale factor to apply to any coordinate you read off it before \
     passing it to this tool. Only the most recent captures are kept, so read the current state \
     off the newest image. This is a real, irreversible interaction with the user's desktop — \
     stay inside what they asked for.";

/// `request_computer_use`'s description for a model that *can* see the attached screenshot.
const REQUEST_COMPUTER_USE_DESCRIPTION_SIGHTED: &str =
    "Ask the user for permission to control their computer, and on approval learn the screen \
     dimensions, the platform, and the list of on-screen windows you may target. Call this once \
     before any use_computer call in the conversation; use_computer runs without a further \
     prompt because the user already approved here. Only use it when the task genuinely needs \
     the desktop GUI — anything reachable from the shell or the filesystem should go through \
     those tools instead. If the result is `rejected`, do not ask again. On approval an initial \
     screenshot is attached to the user message that follows the tool results, so look at it \
     before deciding where to click.";

/// The description to advertise for `name` when the model accepts image input.
///
/// `None` for every tool whose description does not depend on that capability.
pub fn image_capable_description(name: &str) -> Option<&'static str> {
    match name {
        USE_COMPUTER_TOOL_NAME => Some(USE_COMPUTER_DESCRIPTION_SIGHTED),
        REQUEST_COMPUTER_USE_TOOL_NAME => Some(REQUEST_COMPUTER_USE_DESCRIPTION_SIGHTED),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// request_computer_use
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RequestComputerUseArgs {
    /// Purely human-facing — same shape as `apply_file_diffs`'s `summary` and `use_computer`'s
    /// `action_summary` (see `edit.rs`). Unlike those two, there is no operation list to derive
    /// a fallback from, so an absent value falls back to a fixed generic sentence; the request
    /// (and the approval prompt it drives) must never be lost just because this field is missing.
    #[serde(default)]
    task_summary: Option<String>,
    #[serde(default)]
    screenshot: Option<ScreenshotArgs>,
}

/// Fallback used when the model omits `task_summary`. There is no action list to summarize
/// (that only exists once `use_computer` is called), so this is a fixed sentence — still
/// enough for the user to make an approve/reject decision, which is the point of the field.
const FALLBACK_TASK_SUMMARY: &str = "Requesting control of your computer to complete the task.";

fn request_computer_use_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task_summary": {
                "type": "string",
                "description": "One short sentence shown to the user describing what you want to do on their computer, so they can decide whether to approve it. Write it in the same language as the user's messages."
            },
            "screenshot": screenshot_schema(
                "Constraints for the initial screen capture taken on approval. Whether a copy of \
                 the image is sent back to you depends on the model — the result's \
                 `screenshot.delivery` field says which happened."
            )
        },
        // `task_summary` is `required` here so a well-behaved model still sends one — but the
        // parser (`RequestComputerUseArgs::task_summary`, above) accepts its absence and falls
        // back to `FALLBACK_TASK_SUMMARY`. The schema is guidance for good models; the parser
        // must be forgiving of bad ones.
        "required": ["task_summary"],
        "additionalProperties": false
    })
}

fn request_computer_use_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: RequestComputerUseArgs = serde_json::from_str(args)?;
    let task_summary = parsed
        .task_summary
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| FALLBACK_TASK_SUMMARY.to_owned());
    Ok(api::message::tool_call::Tool::RequestComputerUse(
        api::message::tool_call::RequestComputerUse {
            task_summary,
            // Never `None`. `RequestComputerUseExecutor` maps a result with no screenshot to
            // `Error("Failed to capture initial screenshot")`, so an absent params message
            // makes the call fail every single time.
            screenshot_params: Some(parsed.screenshot.unwrap_or_default().to_api()?),
        },
    ))
}

fn request_computer_use_result_to_json(
    result: &api::message::tool_call_result::Result,
) -> Option<Value> {
    use api::message::tool_call_result::Result as R;
    use api::request_computer_use_result::Result as RR;
    let r = match result {
        // Note the asymmetry: the message-side oneof field is
        // `request_computer_use_result`, while the request-side one is
        // `request_computer_use`, so the generated variant names differ.
        R::RequestComputerUseResult(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(RR::Approved(a)) => json!({
            "status": "approved",
            "platform": a.platform().as_str_name(),
            "screen": a.screen_dimensions.as_ref().map(|d| json!({
                "width_px": d.width_px,
                "height_px": d.height_px,
            })),
            "screenshot": screenshot_to_json(a.initial_screenshot.as_ref()),
            "windows": windows_to_json(&a.windows),
        }),
        Some(RR::Rejected(_)) => json!({
            "status": "rejected",
            "message": "The user declined computer use. Do not retry; ask what to do instead.",
        }),
        Some(RR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static REQUEST_COMPUTER_USE: OpenAiTool = OpenAiTool {
    name: REQUEST_COMPUTER_USE_TOOL_NAME,
    description: "Ask the user for permission to control their computer, and on approval learn \
                  the screen dimensions, the platform, and the list of on-screen windows you \
                  may target. Call this once before any use_computer call in the conversation; \
                  use_computer runs without a further prompt because the user already approved \
                  here. Only use it when the task genuinely needs the desktop GUI — anything \
                  reachable from the shell or the filesystem should go through those tools \
                  instead. If the result is `rejected`, do not ask again. IMPORTANT: the \
                  initial screenshot is shown to the user, but this model cannot accept image \
                  input, so you will be working without sight of the screen.",
    parameters: request_computer_use_parameters,
    from_args: request_computer_use_from_args,
    result_to_json: request_computer_use_result_to_json,
};

#[cfg(test)]
#[path = "computer_tests.rs"]
mod computer_tests;
