//! Unit tests for the `use_computer` / `request_computer_use` descriptors.
//!
//! The round-trip tests are the point of this file: a JSON tool call matching the advertised
//! schema must survive `from_args` *and* `TryFrom<tool_call::UseComputer> for
//! AIAgentActionType` (`crates/ai/src/agent/action/convert.rs`), which is the real contract.
//! A schema that only survives the first half produces valid-looking calls that die in
//! conversion, which is exactly the failure mode these tests exist to catch.

use ai::agent::action::UseComputerRequest;
use warp_multi_agent_api as api;

use super::*;
use crate::ai::agent::AIAgentActionType;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn use_computer_tool(args: &str) -> api::message::tool_call::UseComputer {
    match (USE_COMPUTER.from_args)(args).expect("from_args should accept this call") {
        api::message::tool_call::Tool::UseComputer(uc) => uc,
        other => panic!("expected Tool::UseComputer, got {other:?}"),
    }
}

/// Runs a call all the way through the conversion that the executor sees.
fn converted(args: &str) -> UseComputerRequest {
    let uc = use_computer_tool(args);
    match AIAgentActionType::try_from(uc).expect("conversion should accept from_args output") {
        AIAgentActionType::UseComputer(req) => req,
        other => panic!("expected AIAgentActionType::UseComputer, got {other:?}"),
    }
}

fn use_computer_err(args: &str) -> String {
    (USE_COMPUTER.from_args)(args)
        .expect_err("from_args should reject this call")
        .to_string()
}

// ---------------------------------------------------------------------------
// Registration and schema
// ---------------------------------------------------------------------------

#[test]
fn both_tools_are_registered() {
    assert_eq!(USE_COMPUTER.name, USE_COMPUTER_TOOL_NAME);
    assert_eq!(REQUEST_COMPUTER_USE.name, REQUEST_COMPUTER_USE_TOOL_NAME);
    assert!(
        super::super::lookup(USE_COMPUTER_TOOL_NAME).is_some(),
        "use_computer must be present in tools::REGISTRY"
    );
    assert!(
        super::super::lookup(REQUEST_COMPUTER_USE_TOOL_NAME).is_some(),
        "request_computer_use must be present in tools::REGISTRY"
    );
}

#[test]
fn use_computer_schema_shape() {
    let schema = (USE_COMPUTER.parameters)();
    assert_eq!(schema["type"], "object");
    assert_eq!(
        schema["required"],
        serde_json::json!(["action_summary", "actions"])
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["actions"]["type"], "array");

    let item = &schema["properties"]["actions"]["items"];
    assert_eq!(item["required"], serde_json::json!(["action"]));
    let kinds = item["properties"]["action"]["enum"]
        .as_array()
        .expect("action must be an enum");
    for expected in [
        "mouse_move",
        "mouse_down",
        "mouse_up",
        "click",
        "mouse_wheel",
        "wait",
        "type_text",
        "key_down",
        "key_up",
        "key_press",
    ] {
        assert!(
            kinds.iter().any(|k| k == expected),
            "action enum must advertise {expected}"
        );
    }
}

#[test]
fn request_computer_use_schema_shape() {
    let schema = (REQUEST_COMPUTER_USE.parameters)();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"], serde_json::json!(["task_summary"]));
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["screenshot"]["type"], "object");
}

/// The window-target schema must tell the model where valid ids come from, because an id it
/// invents is rejected rather than silently applied to the whole screen.
#[test]
fn window_target_schema_points_at_the_windows_list() {
    let schema = (USE_COMPUTER.parameters)();
    let target = &schema["properties"]["actions"]["items"]["properties"]["target"];
    let description = target["description"].as_str().expect("description");
    assert!(description.contains("windows"), "{description}");
    assert_eq!(target["required"], serde_json::json!(["window_id", "pid"]));
    assert_eq!(target["properties"]["window_id"]["type"], "string");
}

/// Both descriptions must state that the model does not get the screenshot, since the schema
/// alone would suggest otherwise.
///
/// These statics are the *blind* wording, which is the default because `AttachmentCaps` are a
/// per-request property the tool registry cannot see. The sighted swap is asserted by
/// [`image_capable_descriptions_replace_the_blind_wording`]; the two tests are a pair, so a
/// change to one wording should be checked against the other.
#[test]
fn descriptions_admit_that_screenshots_are_not_returned() {
    for tool in [&USE_COMPUTER, &REQUEST_COMPUTER_USE] {
        let d = tool.description.to_lowercase();
        assert!(
            d.contains("cannot accept image input"),
            "{} must say the screenshot does not reach the model: {d}",
            tool.name
        );
    }
}

// ---------------------------------------------------------------------------
// action_summary / task_summary are optional in the parser
//
// Same shape as `apply_file_diffs`'s `summary` (see `edit_tests.rs`): a purely human-facing
// field must never be able to sink the whole tool call just because a smaller BYOP model
// dropped it.
// ---------------------------------------------------------------------------

#[test]
fn use_computer_missing_action_summary_still_parses_and_gets_a_fallback() {
    let uc = use_computer_tool(r#"{"actions": [{"action": "type_text", "text": "hi"}]}"#);
    assert_eq!(uc.action_summary, "Type text");
}

#[test]
fn use_computer_blank_action_summary_is_treated_the_same_as_absent() {
    let uc = use_computer_tool(
        r#"{"action_summary": "  ", "actions": [{"action": "wait", "seconds": 1}]}"#,
    );
    assert_eq!(uc.action_summary, "Wait");
}

#[test]
fn use_computer_fallback_action_summary_for_a_multi_action_batch_is_generic() {
    let uc = use_computer_tool(
        r#"{
            "actions": [
                {"action": "mouse_move", "to": {"x": 1, "y": 1}},
                {"action": "click", "at": {"x": 1, "y": 1}}
            ]
        }"#,
    );
    assert_eq!(uc.action_summary, "Perform 2 computer actions");
}

#[test]
fn use_computer_provided_action_summary_is_kept_verbatim() {
    let uc = use_computer_tool(
        r#"{"action_summary": "Open the file menu", "actions": [{"action": "wait", "seconds": 0}]}"#,
    );
    assert_eq!(uc.action_summary, "Open the file menu");
}

/// `actions` is still required — omitting it (or sending garbage) must still fail loudly
/// rather than silently produce an empty-looking call.
#[test]
fn use_computer_missing_actions_still_fails_to_parse() {
    use_computer_err(r#"{"action_summary": "do something"}"#);
}

#[test]
fn request_computer_use_missing_task_summary_still_parses_with_a_fallback() {
    let tool = (REQUEST_COMPUTER_USE.from_args)(r#"{}"#).expect("from_args should accept this");
    let api::message::tool_call::Tool::RequestComputerUse(rcu) = tool else {
        panic!("expected Tool::RequestComputerUse");
    };
    assert!(
        !rcu.task_summary.trim().is_empty(),
        "an absent task_summary must not leave the approval prompt blank"
    );
}

#[test]
fn request_computer_use_provided_task_summary_is_kept_verbatim() {
    let tool = (REQUEST_COMPUTER_USE.from_args)(r#"{"task_summary": "look at the browser"}"#)
        .expect("from_args");
    let api::message::tool_call::Tool::RequestComputerUse(rcu) = tool else {
        panic!("expected Tool::RequestComputerUse");
    };
    assert_eq!(rcu.task_summary, "look at the browser");
}

#[test]
fn request_computer_use_completely_malformed_json_still_fails_to_parse() {
    (REQUEST_COMPUTER_USE.from_args)("not json at all")
        .expect_err("garbage input must not silently produce a tool call");
}

// ---------------------------------------------------------------------------
// use_computer round trips
// ---------------------------------------------------------------------------

#[test]
fn full_batch_round_trips_through_convert() {
    let req = converted(
        r#"{
            "action_summary": "Open the file menu and save",
            "actions": [
                {"action": "mouse_move", "to": {"x": 10, "y": 20}},
                {"action": "click", "at": {"x": 30, "y": 40}, "button": "right"},
                {"action": "mouse_wheel", "at": {"x": 5, "y": 6}, "direction": "down", "clicks": 3},
                {"action": "wait", "seconds": 0.5},
                {"action": "type_text", "text": "hello"},
                {"action": "key_down", "key": "enter"},
                {"action": "key_up", "key": "enter"}
            ]
        }"#,
    );

    assert_eq!(req.action_summary, "Open the file menu and save");
    assert!(req.screenshot_params.is_some());

    let enter = computer_use::Key::Keycode(named_key_to_keycode("enter").expect("enter"));
    let expected = vec![
        computer_use::Action::MouseMove {
            to: computer_use::Vector2I::new(10, 20),
        },
        computer_use::Action::MouseDown {
            button: computer_use::MouseButton::Right,
            at: computer_use::Vector2I::new(30, 40),
        },
        computer_use::Action::MouseUp {
            button: computer_use::MouseButton::Right,
        },
        computer_use::Action::MouseWheel {
            at: computer_use::Vector2I::new(5, 6),
            direction: computer_use::ScrollDirection::Down,
            distance: computer_use::ScrollDistance::Clicks(3),
        },
        computer_use::Action::Wait(std::time::Duration::from_millis(500)),
        computer_use::Action::TypeText {
            text: "hello".to_owned(),
        },
        computer_use::Action::KeyDown { key: enter.clone() },
        computer_use::Action::KeyUp { key: enter },
    ];
    let actual: Vec<_> = req.actions.iter().map(|a| a.action.clone()).collect();
    assert_eq!(actual, expected);
    assert!(
        req.actions
            .iter()
            .all(|a| a.target == computer_use::Target::Screen),
        "an omitted target must mean the whole screen"
    );
}

/// `click` is sugar: it must expand into the down/up pairs the conversion accepts, so a batch
/// can never leave a button held.
#[test]
fn click_expands_into_balanced_down_up_pairs() {
    let req = converted(
        r#"{
            "action_summary": "double click",
            "actions": [{"action": "click", "at": {"x": 1, "y": 2}, "count": 2}]
        }"#,
    );
    assert_eq!(req.actions.len(), 4);
    let downs = req
        .actions
        .iter()
        .filter(|a| matches!(a.action, computer_use::Action::MouseDown { .. }))
        .count();
    let ups = req
        .actions
        .iter()
        .filter(|a| matches!(a.action, computer_use::Action::MouseUp { .. }))
        .count();
    assert_eq!((downs, ups), (2, 2));
}

/// `key_press` must release modifiers in reverse order, so a batch can never leave a modifier
/// stuck down.
#[test]
fn key_press_wraps_the_key_in_its_modifiers() {
    let req = converted(
        r#"{
            "action_summary": "select all",
            "actions": [{"action": "key_press", "key": "a", "modifiers": ["control", "shift"]}]
        }"#,
    );
    let ctrl = computer_use::Key::Keycode(named_key_to_keycode("control").expect("control"));
    let shift = computer_use::Key::Keycode(named_key_to_keycode("shift").expect("shift"));
    let a = computer_use::Key::Char('a');
    let expected = vec![
        computer_use::Action::KeyDown { key: ctrl.clone() },
        computer_use::Action::KeyDown { key: shift.clone() },
        computer_use::Action::KeyDown { key: a.clone() },
        computer_use::Action::KeyUp { key: a },
        computer_use::Action::KeyUp { key: shift },
        computer_use::Action::KeyUp { key: ctrl },
    ];
    let actual: Vec<_> = req.actions.iter().map(|a| a.action.clone()).collect();
    assert_eq!(actual, expected);
}

#[test]
fn window_target_round_trips_onto_every_expanded_action() {
    let req = converted(
        r#"{
            "action_summary": "click in the background window",
            "actions": [{
                "action": "click",
                "at": {"x": 3, "y": 4},
                "target": {"window_id": "4711", "pid": 99}
            }]
        }"#,
    );
    assert_eq!(req.actions.len(), 2);
    for action in &req.actions {
        assert_eq!(
            action.target,
            computer_use::Target::Window {
                window_id: 4711,
                pid: 99
            }
        );
    }
}

#[test]
fn pixel_scroll_distance_round_trips() {
    let req = converted(
        r#"{
            "action_summary": "scroll",
            "actions": [{"action": "mouse_wheel", "at": {"x": 0, "y": 0}, "direction": "left", "pixels": 120}]
        }"#,
    );
    assert_eq!(
        req.actions[0].action,
        computer_use::Action::MouseWheel {
            at: computer_use::Vector2I::new(0, 0),
            direction: computer_use::ScrollDirection::Left,
            distance: computer_use::ScrollDistance::Pixels(120),
        }
    );
}

/// `RequestComputerUseExecutor` turns a result with no screenshot into a hard error, so the
/// descriptor must never emit an absent `screenshot_params`, however the model calls it.
#[test]
fn request_computer_use_always_sends_screenshot_params() {
    for args in [
        r#"{"task_summary": "look at the browser"}"#,
        r#"{"task_summary": "look", "screenshot": {"max_long_edge_px": 800}}"#,
    ] {
        let tool = (REQUEST_COMPUTER_USE.from_args)(args).expect("from_args");
        let api::message::tool_call::Tool::RequestComputerUse(rcu) = tool else {
            panic!("expected Tool::RequestComputerUse");
        };
        assert!(
            rcu.screenshot_params.is_some(),
            "screenshot_params must always be present, or the executor returns \
             Error(\"Failed to capture initial screenshot\") every time"
        );
        // And it must survive the conversion the executor actually reads.
        match AIAgentActionType::from(rcu) {
            AIAgentActionType::RequestComputerUse(req) => {
                assert!(req.screenshot_params.is_some());
            }
            other => panic!("expected AIAgentActionType::RequestComputerUse, got {other:?}"),
        }
    }
}

/// The same defaulting applies to `use_computer`: the capture is what refreshes the window
/// list and what the block render shows the user.
#[test]
fn use_computer_defaults_the_screenshot_params() {
    let uc = use_computer_tool(
        r#"{"action_summary": "s", "actions": [{"action": "type_text", "text": "x"}]}"#,
    );
    let params = uc
        .post_actions_screenshot_params
        .expect("post_actions_screenshot_params must always be present");
    assert_eq!(params.max_long_edge_px, DEFAULT_MAX_LONG_EDGE_PX);
    assert_eq!(params.max_total_px, 0);
    assert!(params.region.is_none());
}

// ---------------------------------------------------------------------------
// Argument rejection: everything the conversion would reject or silently reinterpret
// ---------------------------------------------------------------------------

#[test]
fn mouse_wheel_without_a_distance_is_rejected_before_conversion() {
    // `to_scroll_distance` returns MissingComputerUseScrollDistance for this, but the model
    // only ever sees the executor's generic failure; rejecting here gives it a fixable
    // message instead.
    let err = use_computer_err(
        r#"{"action_summary": "s", "actions": [{"action": "mouse_wheel", "at": {"x": 0, "y": 0}, "direction": "up"}]}"#,
    );
    assert!(err.contains("pixels or clicks"), "{err}");
}

#[test]
fn mouse_wheel_with_both_distances_is_rejected() {
    let err = use_computer_err(
        r#"{"action_summary": "s", "actions": [{"action": "mouse_wheel", "at": {"x": 0, "y": 0}, "direction": "up", "pixels": 1, "clicks": 1}]}"#,
    );
    assert!(err.contains("exactly one"), "{err}");
}

/// `convert_computer_use_target` falls back to the whole screen for an unparseable id. That is
/// the worst possible outcome for a mis-typed window id, so it is caught here instead.
#[test]
fn unparseable_window_id_is_rejected_rather_than_silently_targeting_the_screen() {
    let err = use_computer_err(
        r#"{"action_summary": "s", "actions": [{"action": "mouse_move", "to": {"x": 1, "y": 1}, "target": {"window_id": "0x1f", "pid": 1}}]}"#,
    );
    assert!(err.contains("window_id"), "{err}");
}

#[test]
fn zero_window_id_is_rejected() {
    let err = use_computer_err(
        r#"{"action_summary": "s", "actions": [{"action": "mouse_move", "to": {"x": 1, "y": 1}, "target": {"window_id": "0", "pid": 1}}]}"#,
    );
    assert!(err.contains("sentinel"), "{err}");
}

#[test]
fn negative_and_overlong_waits_are_rejected() {
    let negative = use_computer_err(
        r#"{"action_summary": "s", "actions": [{"action": "wait", "seconds": -1}]}"#,
    );
    assert!(negative.contains("non-negative"), "{negative}");
    let overlong = use_computer_err(
        r#"{"action_summary": "s", "actions": [{"action": "wait", "seconds": 100000}]}"#,
    );
    assert!(overlong.contains("at most"), "{overlong}");
}

#[test]
fn inverted_screenshot_regions_are_rejected() {
    let err = use_computer_err(
        r#"{
            "action_summary": "s",
            "actions": [{"action": "type_text", "text": "x"}],
            "screenshot": {"region": {"top_left": {"x": 10, "y": 10}, "bottom_right": {"x": 5, "y": 20}}}
        }"#,
    );
    assert!(err.contains("strictly greater"), "{err}");
}

#[test]
fn empty_action_lists_are_rejected() {
    let err = use_computer_err(r#"{"action_summary": "s", "actions": []}"#);
    assert!(err.contains("at least one"), "{err}");
}

#[test]
fn unknown_key_names_report_the_accepted_set() {
    let err = use_computer_err(
        r#"{"action_summary": "s", "actions": [{"action": "key_down", "key": "hyperspace"}]}"#,
    );
    assert!(err.contains("hyperspace"), "{err}");
    assert!(
        err.contains("enter"),
        "the error must list the named keys: {err}"
    );
}

// ---------------------------------------------------------------------------
// Key specs
// ---------------------------------------------------------------------------

#[test]
fn single_characters_become_char_keys() {
    use api::message::tool_call::use_computer::action::key::Data;
    for (spec, expected) in [("a", "a"), ("+", "+"), (" ", " ")] {
        let key = parse_key(spec).expect("single character keys are always valid");
        assert_eq!(key.data, Some(Data::Char(expected.to_owned())), "{spec}");
    }
}

#[test]
fn hex_specs_become_raw_platform_keycodes() {
    use api::message::tool_call::use_computer::action::key::Data;
    let key = parse_key("0x24").expect("hex keycode");
    assert_eq!(key.data, Some(Data::Keycode(0x24)));
}

#[test]
fn key_aliases_fold_onto_canonical_names() {
    for (alias, canonical) in [
        ("Return", "enter"),
        ("ESC", "escape"),
        ("ctrl", "control"),
        ("Cmd", "meta"),
        ("page-up", "page_up"),
        ("Arrow Down", "down"),
    ] {
        assert_eq!(
            canonical_key_name(&alias.to_ascii_lowercase().replace(['-', ' '], "_")),
            canonical,
            "{alias}"
        );
        parse_key(alias).unwrap_or_else(|e| panic!("{alias} should resolve: {e}"));
    }
}

/// Every advertised name must resolve on every platform. A name that resolves only on the
/// host would ship a tool whose key set silently shrinks on someone else's machine.
#[test]
fn every_named_key_resolves_on_every_platform() {
    for name in NAMED_KEYS {
        assert!(mac_keycode(name).is_some(), "macOS is missing {name}");
        assert!(windows_vk(name).is_some(), "Windows is missing {name}");
        assert!(x11_keysym(name).is_some(), "X11 is missing {name}");
    }
}

/// Spot-checks against the platform headers, so a transcription slip in the tables shows up
/// as a test failure rather than as a key that presses something else.
#[test]
fn platform_tables_match_the_platform_headers() {
    // HIToolbox/Events.h: kVK_Return, kVK_Tab, kVK_Escape, kVK_ANSI_Delete(backspace),
    // kVK_LeftArrow, kVK_F1.
    assert_eq!(mac_keycode("enter"), Some(0x24));
    assert_eq!(mac_keycode("tab"), Some(0x30));
    assert_eq!(mac_keycode("escape"), Some(0x35));
    assert_eq!(mac_keycode("backspace"), Some(0x33));
    assert_eq!(mac_keycode("left"), Some(0x7B));
    assert_eq!(mac_keycode("f1"), Some(0x7A));

    // winuser.h: VK_RETURN, VK_TAB, VK_ESCAPE, VK_BACK, VK_LEFT, VK_F1.
    assert_eq!(windows_vk("enter"), Some(0x0D));
    assert_eq!(windows_vk("tab"), Some(0x09));
    assert_eq!(windows_vk("escape"), Some(0x1B));
    assert_eq!(windows_vk("backspace"), Some(0x08));
    assert_eq!(windows_vk("left"), Some(0x25));
    assert_eq!(windows_vk("f1"), Some(0x70));

    // keysymdef.h: XK_Return, XK_Tab, XK_Escape, XK_BackSpace, XK_Left, XK_F1.
    assert_eq!(x11_keysym("enter"), Some(0xFF0D));
    assert_eq!(x11_keysym("tab"), Some(0xFF09));
    assert_eq!(x11_keysym("escape"), Some(0xFF1B));
    assert_eq!(x11_keysym("backspace"), Some(0xFF08));
    assert_eq!(x11_keysym("left"), Some(0xFF51));
    assert_eq!(x11_keysym("f1"), Some(0xFFBE));
}

/// `convert_key` rejects a multi-character `char`, so the descriptor must never build one.
#[test]
fn named_keys_never_become_multi_character_char_keys() {
    use api::message::tool_call::use_computer::action::key::Data;
    for name in NAMED_KEYS {
        let key = parse_key(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        match key.data {
            Some(Data::Keycode(_)) => {}
            other => panic!("{name} must resolve to a keycode, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Result serialization
// ---------------------------------------------------------------------------

fn raw_image() -> api::RawImage {
    api::RawImage {
        data: vec![1, 2, 3],
        mime_type: "image/png".to_owned(),
        width: 800,
        height: 600,
    }
}

fn window_info() -> api::WindowInfo {
    api::WindowInfo {
        window_id: "4711".to_owned(),
        pid: 99,
        app_name: "Notes".to_owned(),
        title: "Untitled".to_owned(),
        layer: 0,
    }
}

#[test]
fn use_computer_success_serializes_metadata_without_image_bytes() {
    let result = api::message::tool_call_result::Result::UseComputer(api::UseComputerResult {
        result: Some(api::use_computer_result::Result::Success(
            api::use_computer_result::Success {
                screenshot: Some(raw_image()),
                cursor_position: Some(api::Coordinates { x: 12, y: 34 }),
                captured_window: Some(api::use_computer_result::success::CapturedWindow {
                    window_id: "4711".to_owned(),
                    width_px: 800,
                    height_px: 600,
                }),
                windows: vec![window_info()],
            },
        )),
    });
    let json =
        (USE_COMPUTER.result_to_json)(&result).expect("use_computer must claim its own result");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["screenshot"]["captured"], true);
    // `result_to_json` is a context-free `fn` pointer: it cannot know the model's
    // AttachmentCaps or the per-request attachment budget, so it emits the undecided shape and
    // `chat_stream` overwrites it through `annotate_screenshot_delivery`. Anything that
    // serializes a result *without* annotating it must still leave the model correctly
    // pessimistic, which is what these two assertions pin.
    assert_eq!(json["screenshot"]["attached"], false);
    assert_eq!(json["screenshot"]["delivery"], "undecided");
    assert_eq!(json["screenshot"]["width_px"], 800);
    assert!(
        json["screenshot"]["note"]
            .as_str()
            .is_some_and(|n| n.contains("never embedded in this tool result")),
        "the payload must say why the image is missing"
    );
    assert_eq!(
        json["cursor_position"],
        serde_json::json!({"x": 12, "y": 34})
    );
    assert_eq!(json["captured_window"]["window_id"], "4711");
    assert_eq!(json["windows"][0]["window_id"], "4711");
    assert_eq!(json["windows"][0]["pid"], 99);

    // The whole point: no base64, no bytes, nothing that could blow the 40k tool-result cap.
    let serialized = serde_json::to_string(&json).expect("serializable");
    assert!(
        serialized.len() < 2_000,
        "payload grew unexpectedly: {serialized}"
    );
    assert!(
        !serialized.contains("data"),
        "image bytes must not be serialized: {serialized}"
    );
}

#[test]
fn use_computer_error_and_cancelled_serialize() {
    let error = api::message::tool_call_result::Result::UseComputer(api::UseComputerResult {
        result: Some(api::use_computer_result::Result::Error(
            api::use_computer_result::Error {
                message: "no display".to_owned(),
            },
        )),
    });
    let json = (USE_COMPUTER.result_to_json)(&error).expect("error result");
    assert_eq!(json["status"], "error");
    assert_eq!(json["message"], "no display");

    let cancelled = api::message::tool_call_result::Result::UseComputer(api::UseComputerResult {
        result: None,
    });
    let json = (USE_COMPUTER.result_to_json)(&cancelled).expect("cancelled result");
    assert_eq!(json["status"], "cancelled");
}

#[test]
fn request_computer_use_approval_serializes_the_window_list_and_screen() {
    let result = api::message::tool_call_result::Result::RequestComputerUseResult(
        api::RequestComputerUseResult {
            result: Some(api::request_computer_use_result::Result::Approved(
                api::request_computer_use_result::Approved {
                    screen_dimensions: Some(api::ScreenDimensions {
                        width_px: 2560,
                        height_px: 1440,
                    }),
                    initial_screenshot: Some(raw_image()),
                    platform: api::request_computer_use_result::approved::Platform::LinuxX11.into(),
                    windows: vec![window_info()],
                },
            )),
        },
    );
    let json = (REQUEST_COMPUTER_USE.result_to_json)(&result).expect("approval result");
    assert_eq!(json["status"], "approved");
    assert_eq!(json["platform"], "LINUX_X11");
    assert_eq!(json["screen"]["width_px"], 2560);
    assert_eq!(json["screenshot"]["attached"], false);
    assert_eq!(json["windows"][0]["app_name"], "Notes");
}

#[test]
fn request_computer_use_rejection_tells_the_model_not_to_retry() {
    let result = api::message::tool_call_result::Result::RequestComputerUseResult(
        api::RequestComputerUseResult {
            result: Some(api::request_computer_use_result::Result::Rejected(
                api::request_computer_use_result::Rejected {},
            )),
        },
    );
    let json = (REQUEST_COMPUTER_USE.result_to_json)(&result).expect("rejection result");
    assert_eq!(json["status"], "rejected");
    assert!(
        json["message"]
            .as_str()
            .is_some_and(|m| m.contains("Do not retry")),
        "a rejection must not read as a transient failure"
    );
}

// ---------------------------------------------------------------------------
// History replay
// ---------------------------------------------------------------------------

/// `chat_stream::serialize_outgoing_tool_call` replays a persisted call back to the model as
/// its own past turn. If the replay is not the inverse of `from_args`, the model is shown a
/// call it did not make — and with computer use it then cannot tell which action produced the
/// screenshot it is looking at.
#[test]
fn outgoing_use_computer_replay_round_trips_through_from_args() {
    let args = r#"{
        "action_summary": "Save the file",
        "actions": [
            {"action": "click", "at": {"x": 10, "y": 20}, "button": "right", "count": 2},
            {"action": "key_press", "key": "enter", "modifiers": ["control"]},
            {"action": "type_text", "text": "hello", "target": {"window_id": "4711", "pid": 99}},
            {"action": "mouse_wheel", "at": {"x": 1, "y": 2}, "direction": "down", "clicks": 3},
            {"action": "mouse_wheel", "at": {"x": 3, "y": 4}, "direction": "up", "pixels": 120},
            {"action": "wait", "seconds": 1.5},
            {"action": "mouse_move", "to": {"x": 5, "y": 6}},
            {"action": "key_down", "key": "a"},
            {"action": "key_up", "key": "a"}
        ],
        "screenshot": {
            "max_long_edge_px": 800,
            "region": {"top_left": {"x": 0, "y": 0}, "bottom_right": {"x": 100, "y": 50}},
            "target": {"window_id": "4711", "pid": 99}
        }
    }"#;
    let first = use_computer_tool(args);
    let replayed = serialize_outgoing_use_computer(&first).to_string();
    let second = use_computer_tool(&replayed);
    assert_eq!(
        first, second,
        "replayed args must rebuild the identical call; replay was {replayed}"
    );
}

#[test]
fn outgoing_request_computer_use_replay_round_trips_through_from_args() {
    let args = r#"{
        "task_summary": "Open the settings dialog",
        "screenshot": {"max_long_edge_px": 1024, "max_total_px": 400000}
    }"#;
    let first = match (REQUEST_COMPUTER_USE.from_args)(args).expect("from_args accepts this") {
        api::message::tool_call::Tool::RequestComputerUse(r) => r,
        other => panic!("expected Tool::RequestComputerUse, got {other:?}"),
    };
    let replayed = serialize_outgoing_request_computer_use(&first).to_string();
    let second = match (REQUEST_COMPUTER_USE.from_args)(&replayed).expect("replay must reparse") {
        api::message::tool_call::Tool::RequestComputerUse(r) => r,
        other => panic!("expected Tool::RequestComputerUse, got {other:?}"),
    };
    assert_eq!(
        first, second,
        "replayed args must rebuild the identical call; replay was {replayed}"
    );
}

// ---------------------------------------------------------------------------
// Screenshot delivery
// ---------------------------------------------------------------------------

fn use_computer_success(screenshot: Option<api::RawImage>) -> api::message::tool_call_result::Result
{
    api::message::tool_call_result::Result::UseComputer(api::UseComputerResult {
        result: Some(api::use_computer_result::Result::Success(
            api::use_computer_result::Success {
                screenshot,
                cursor_position: None,
                captured_window: None,
                windows: vec![],
            },
        )),
    })
}

fn approved(screenshot: Option<api::RawImage>) -> api::message::tool_call_result::Result {
    api::message::tool_call_result::Result::RequestComputerUseResult(api::RequestComputerUseResult {
        result: Some(api::request_computer_use_result::Result::Approved(
            api::request_computer_use_result::Approved {
                screen_dimensions: Some(api::ScreenDimensions {
                    width_px: 2560,
                    height_px: 1440,
                }),
                initial_screenshot: screenshot,
                platform: api::request_computer_use_result::approved::Platform::LinuxX11.into(),
                windows: vec![],
            },
        )),
    })
}

/// Every delivery outcome must be legible both to a parser (`delivery`) and to the model
/// reading prose (`note`), and `attached` must agree with both.
#[test]
fn annotate_screenshot_delivery_rewrites_attached_delivery_and_note() {
    let result = use_computer_success(Some(raw_image()));
    let cases = [
        (ScreenshotDelivery::Attached, true, "attached_to_following_user_message"),
        (
            ScreenshotDelivery::ModelCannotSeeImages,
            false,
            "model_cannot_see_images",
        ),
        (
            ScreenshotDelivery::Superseded,
            false,
            "superseded_by_newer_screenshot",
        ),
        (ScreenshotDelivery::Undeliverable, false, "undeliverable"),
    ];
    let mut notes: Vec<String> = Vec::new();
    for (delivery, attached, tag) in cases {
        let mut json = (USE_COMPUTER.result_to_json)(&result).expect("use_computer result");
        annotate_screenshot_delivery(&mut json, delivery);
        assert_eq!(json["screenshot"]["attached"], attached, "{tag}");
        assert_eq!(json["screenshot"]["delivery"], tag);
        let note = json["screenshot"]["note"]
            .as_str()
            .expect("every delivery must carry a note")
            .to_owned();
        assert!(!note.is_empty(), "{tag} must explain itself");
        // The metadata is never dropped by the rewrite — the model still needs the dimensions
        // to reason about coordinates even when it cannot see the image.
        assert_eq!(json["screenshot"]["width_px"], 800);
        notes.push(note);
    }
    notes.sort();
    notes.dedup();
    assert_eq!(
        notes.len(),
        4,
        "each outcome needs its own wording; a shared note tells the model nothing"
    );
}

/// The annotation runs on every outgoing tool result, so it must be inert for results that
/// carry no capture (rejections, errors, other tools' payloads).
#[test]
fn annotate_screenshot_delivery_is_inert_without_a_capture() {
    let mut json = (USE_COMPUTER.result_to_json)(&use_computer_success(None))
        .expect("use_computer result without a screenshot");
    let before = json.clone();
    annotate_screenshot_delivery(&mut json, ScreenshotDelivery::Attached);
    assert_eq!(json, before, "a result with no capture must not be rewritten");

    let mut unrelated = serde_json::json!({"status": "ok", "stdout": "hi"});
    let before = unrelated.clone();
    annotate_screenshot_delivery(&mut unrelated, ScreenshotDelivery::Attached);
    assert_eq!(unrelated, before, "another tool's payload must be untouched");
}

/// `screenshot_of` doubles as the "is this a computer-use result carrying an image?" test in
/// `chat_stream`, so a false positive would attach bytes for an unrelated tool.
#[test]
fn screenshot_of_covers_both_descriptors_and_nothing_else() {
    assert!(screenshot_of(&use_computer_success(Some(raw_image()))).is_some());
    assert!(screenshot_of(&approved(Some(raw_image()))).is_some());
    assert!(screenshot_of(&use_computer_success(None)).is_none());
    assert!(screenshot_of(&approved(None)).is_none());
    assert!(
        screenshot_of(&api::message::tool_call_result::Result::RunShellCommand(
            api::RunShellCommandResult {
                command: "ls".to_owned(),
                output: String::new(),
                exit_code: 0,
                result: None,
            }
        ))
        .is_none(),
        "another tool's result must never look like a screenshot"
    );
}

/// The surface size is what lets the model scale a coordinate read off a downscaled image back
/// into the physical pixels `use_computer` takes; getting it from the wrong field would move
/// every click by that ratio.
#[test]
fn captured_surface_px_reads_window_then_screen() {
    assert_eq!(captured_surface_px(&approved(Some(raw_image()))), Some((2560, 1440)));

    let windowed = api::message::tool_call_result::Result::UseComputer(api::UseComputerResult {
        result: Some(api::use_computer_result::Result::Success(
            api::use_computer_result::Success {
                screenshot: Some(raw_image()),
                cursor_position: None,
                captured_window: Some(api::use_computer_result::success::CapturedWindow {
                    window_id: "4711".to_owned(),
                    width_px: 1600,
                    height_px: 1200,
                }),
                windows: vec![],
            },
        )),
    });
    assert_eq!(captured_surface_px(&windowed), Some((1600, 1200)));

    // A full-screen capture has no surface size of its own; the caller substitutes the screen
    // size remembered from the approval.
    assert_eq!(captured_surface_px(&use_computer_success(Some(raw_image()))), None);
}

/// A sighted model must not be told it is blind — that stops it looking at an image it was
/// just handed.
#[test]
fn image_capable_descriptions_replace_the_blind_wording() {
    for name in [USE_COMPUTER_TOOL_NAME, REQUEST_COMPUTER_USE_TOOL_NAME] {
        let sighted = image_capable_description(name)
            .unwrap_or_else(|| panic!("{name} needs an image-capable description"));
        assert!(
            !sighted.contains("never see the screen")
                && !sighted.contains("without sight of the screen"),
            "{name}'s image-capable description still claims blindness"
        );
        assert!(
            sighted.contains("attached"),
            "{name}'s image-capable description must say the screenshot arrives"
        );
    }
    assert!(
        image_capable_description("run_shell_command").is_none(),
        "only the computer-use descriptors depend on image capability"
    );

    // The blind defaults must keep saying so, or a text-only model silently assumes it can see.
    assert!(USE_COMPUTER.description.contains("never see the screen"));
    assert!(
        REQUEST_COMPUTER_USE
            .description
            .contains("without sight of the screen")
    );
}

/// Each descriptor must claim only its own variant, or `tools::serialize_result` — which
/// returns the first `Some` from the registry — would hand back another tool's payload.
#[test]
fn descriptors_do_not_claim_each_others_results() {
    let use_computer =
        api::message::tool_call_result::Result::UseComputer(api::UseComputerResult {
            result: None,
        });
    let request = api::message::tool_call_result::Result::RequestComputerUseResult(
        api::RequestComputerUseResult { result: None },
    );
    assert!((REQUEST_COMPUTER_USE.result_to_json)(&use_computer).is_none());
    assert!((USE_COMPUTER.result_to_json)(&request).is_none());
}
