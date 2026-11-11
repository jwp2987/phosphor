use warpui::keymap::Context;

use super::*;

fn ctx_with(keys: &[&'static str]) -> Context {
    let mut context = Context::default();
    for key in keys {
        context.set.insert(*key);
    }
    context
}

/// A command prints in the foreground and the terminal has the keys: the shifted keys scroll.
#[test]
fn the_keys_are_live_under_a_running_command() {
    let running = ctx_with(&["Terminal", "LongRunningCommand"]);
    assert!(scrollback_key_predicate().eval(&running));
    // A program that asked for the kitty keyboard protocol without leaving the block list (a
    // REPL, a CLI agent) still has a scrollback behind it.
    let kitty = ctx_with(&["Terminal", "LongRunningCommand", "KeyboardProtocolEnabled"]);
    assert!(scrollback_key_predicate().eval(&kitty));
}

/// A full-screen program keeps Shift+Page Up / Shift+Home: nothing of ours is behind it.
#[test]
fn the_alternate_screen_keeps_the_keys() {
    assert!(!scrollback_key_predicate().eval(&ctx_with(&["Terminal", "AltScreen"])));
    // Not a state `keymap_context` produces today; the predicate must not depend on that.
    let both = ctx_with(&["Terminal", "LongRunningCommand", "AltScreen"]);
    assert!(!scrollback_key_predicate().eval(&both));
}

/// At a prompt, with the input editor focused (also under a running CLI agent's rich input), or
/// with an input method composing, the keys are not taken: Shift+Home / Shift+End select text in
/// the editor.
#[test]
fn the_prompt_the_editor_and_an_open_ime_keep_the_keys() {
    assert!(!scrollback_key_predicate().eval(&ctx_with(&["Terminal"])));
    let editor = ctx_with(&["Terminal", "LongRunningCommand", "EditorFocused"]);
    assert!(!scrollback_key_predicate().eval(&editor));
    let ime = ctx_with(&["Terminal", "LongRunningCommand", "IMEOpen"]);
    assert!(!scrollback_key_predicate().eval(&ime));
    assert!(!scrollback_key_predicate().eval(&ctx_with(&["LongRunningCommand"])));
}

/// Each key is one shifted navigation key and asks for the scroll its plain key makes at a prompt.
#[test]
fn each_key_has_its_keystroke_and_its_scroll() {
    let table: Vec<(&str, String)> = ScrollbackKey::ALL
        .iter()
        .map(|key| (key.keystroke(), format!("{:?}", key.scroll_update())))
        .collect();
    assert_eq!(
        table,
        vec![
            ("shift-pageup", "AfterPageUp".to_string()),
            ("shift-pagedown", "AfterPageDown".to_string()),
            ("shift-home", "AfterHome".to_string()),
            ("shift-end", "AfterEnd".to_string()),
        ]
    );
    for key in ScrollbackKey::ALL {
        let keystroke = warpui::keymap::Keystroke::parse(key.keystroke()).expect("parses");
        assert!(keystroke.shift && !keystroke.ctrl && !keystroke.alt && !keystroke.cmd);
    }
}

/// The log line the real-app case reads; a change here must change `cases_r15_defects.py` too.
#[test]
fn the_hop_detail_format_is_pinned() {
    let details: Vec<String> = ScrollbackKey::ALL
        .iter()
        .map(|key| scrollback_key_hop_detail(*key))
        .collect();
    assert_eq!(
        details,
        vec!["key=page_up", "key=page_down", "key=top", "key=bottom"]
    );
}

#[test]
fn the_kill_switch_is_off_only_for_zero() {
    assert!(scrollback_keys_enabled_from(None));
    assert!(scrollback_keys_enabled_from(Some("")));
    assert!(scrollback_keys_enabled_from(Some("1")));
    assert!(!scrollback_keys_enabled_from(Some("0")));
    assert!(!scrollback_keys_enabled_from(Some(" 0 ")));
}
