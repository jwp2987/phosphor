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
//! ## Screenshots do not reach the model
//!
//! Both results can carry a `RawImage`. The BYOP tool-result channel cannot: a tool result is
//! delivered as `genai::chat::ToolResponse { content: String }` (see `chat_stream.rs`, and
//! `lib/rust-genai/src/chat/tool/tool_response.rs` where `content` is a plain `String`), and
//! `chat_stream::cap_tool_response_content` truncates it at 40 000 characters — two orders of
//! magnitude below a base64 screenshot. There is no per-provider fallback to choose between:
//! **no** provider can receive an image through a tool result here, however capable the model
//! is (`attachment_caps::AttachmentCaps` only governs *user-message* attachments, and is not
//! reachable from `result_to_json`, which takes no request context).
//!
//! So `result_to_json` never embeds image bytes. It reports the capture explicitly —
//! `screenshot.captured` / `screenshot.attached` plus dimensions and a `note` — so the model
//! is told, in the result it is reading, that it is acting blind. Silently returning an empty
//! object would leave it inferring that the screen was blank. The screenshot is still captured,
//! persisted, and rendered in the block for the *user*; only the model's copy is missing.
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
    action_summary: String,
    actions: Vec<ActionArg>,
    #[serde(default)]
    screenshot: Option<ScreenshotArgs>,
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
                 always captured; this only shapes it. Note that the image itself is NOT \
                 returned to you — see the tool description."
            )
        },
        "required": ["action_summary", "actions"],
        "additionalProperties": false
    })
}

fn use_computer_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: UseComputerArgs = serde_json::from_str(args)?;
    if parsed.actions.is_empty() {
        bail!("actions must contain at least one action");
    }
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
            action_summary: parsed.action_summary,
        },
    ))
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

/// Explicit degradation for a captured-but-undeliverable screenshot.
///
/// See the module doc: the BYOP tool-result channel is a plain string, so the bytes cannot
/// travel. Saying so in the payload is the difference between a model that knows it is blind
/// and one that concludes the screen was empty.
const SCREENSHOT_NOT_ATTACHED_NOTE: &str = "The screenshot was captured and is shown to the user, but it cannot be attached to this \
     tool result: this client delivers tool results as text only. You cannot see the screen. \
     Work from the window list, the coordinates you asked for, and what the user tells you, \
     and ask the user to describe or attach a screenshot when you need to look at something.";

fn screenshot_to_json(image: Option<&api::RawImage>) -> Value {
    match image {
        Some(img) => json!({
            "captured": true,
            "attached": false,
            "width_px": img.width,
            "height_px": img.height,
            "mime_type": img.mime_type,
            "note": SCREENSHOT_NOT_ATTACHED_NOTE,
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
                  shown to the user, but this client cannot send images back to you, so you \
                  never see the screen. Do not guess at coordinates you were not given: work \
                  from what the user told you and from the window list in the result, and ask \
                  the user when you need to know what is on screen. Prefer keyboard navigation \
                  over pointer aiming for the same reason. This is a real, irreversible \
                  interaction with the user's desktop — stay inside what they asked for.",
    parameters: use_computer_parameters,
    from_args: use_computer_from_args,
    result_to_json: use_computer_result_to_json,
};

// ---------------------------------------------------------------------------
// request_computer_use
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RequestComputerUseArgs {
    task_summary: String,
    #[serde(default)]
    screenshot: Option<ScreenshotArgs>,
}

fn request_computer_use_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task_summary": {
                "type": "string",
                "description": "One short sentence shown to the user describing what you want to do on their computer, so they can decide whether to approve it. Write it in the same language as the user's messages."
            },
            "screenshot": screenshot_schema(
                "Constraints for the initial screen capture taken on approval. The image itself \
                 is NOT returned to you — see the tool description."
            )
        },
        "required": ["task_summary"],
        "additionalProperties": false
    })
}

fn request_computer_use_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: RequestComputerUseArgs = serde_json::from_str(args)?;
    Ok(api::message::tool_call::Tool::RequestComputerUse(
        api::message::tool_call::RequestComputerUse {
            task_summary: parsed.task_summary,
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
                  initial screenshot is shown to the user but cannot be sent back to you, so \
                  you will be working without sight of the screen.",
    parameters: request_computer_use_parameters,
    from_args: request_computer_use_from_args,
    result_to_json: request_computer_use_result_to_json,
};

#[cfg(test)]
#[path = "computer_tests.rs"]
mod computer_tests;
