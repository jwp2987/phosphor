use warpui::keymap::Context;

use super::*;

fn ctx_with(keys: &[&'static str]) -> Context {
    let mut context = Context::default();
    for key in keys {
        context.set.insert(*key);
    }
    context
}

/// A command prints in the foreground and the terminal has the keys: the shifted keys scroll,
/// with the keys at a prompt on or off.
#[test]
fn the_keys_are_live_under_a_running_command() {
    for at_prompt in [false, true] {
        let running = ctx_with(&["Terminal", "LongRunningCommand"]);
        assert!(scrollback_key_predicate(at_prompt).eval(&running));
        // A program that asked for the kitty keyboard protocol without leaving the block list (a
        // REPL, a CLI agent) still has a scrollback behind it.
        let kitty = ctx_with(&["Terminal", "LongRunningCommand", "KeyboardProtocolEnabled"]);
        assert!(scrollback_key_predicate(at_prompt).eval(&kitty));
    }
}

/// A full-screen program keeps Shift+Page Up / Shift+Home: nothing of ours is behind it. Also
/// with the keys live at a prompt, where "no command runs" must not let the alternate screen in.
#[test]
fn the_alternate_screen_keeps_the_keys() {
    for at_prompt in [false, true] {
        let predicate = scrollback_key_predicate(at_prompt);
        assert!(!predicate.eval(&ctx_with(&["Terminal", "AltScreen"])));
        assert!(!predicate.eval(&ctx_with(&["Terminal", "AltScreen", "EditorFocused"])));
        // Not a state `keymap_context` produces today; the predicate must not depend on that.
        let both = ctx_with(&["Terminal", "LongRunningCommand", "AltScreen"]);
        assert!(!predicate.eval(&both));
    }
}

/// Round 15, and the kill switch `PHOSPHOR_SCROLLBACK_KEYS_AT_PROMPT=0`: at a prompt, with the
/// input editor focused (also under a running CLI agent's rich input), or with an input method
/// composing, the keys are not taken.
#[test]
fn without_the_prompt_half_the_prompt_the_editor_and_an_open_ime_keep_the_keys() {
    let predicate = scrollback_key_predicate(false);
    assert!(!predicate.eval(&ctx_with(&["Terminal"])));
    assert!(!predicate.eval(&ctx_with(&["Terminal", "EditorFocused"])));
    let editor = ctx_with(&["Terminal", "LongRunningCommand", "EditorFocused"]);
    assert!(!predicate.eval(&editor));
    let ime = ctx_with(&["Terminal", "LongRunningCommand", "IMEOpen"]);
    assert!(!predicate.eval(&ime));
    assert!(!predicate.eval(&ctx_with(&["LongRunningCommand"])));
}

/// Round 16 (owner decision 2026-11-11): the keys also scroll at a prompt, whether the input
/// editor or the block list has the focus. Under a running command the input editor's focus
/// (the rich input of a CLI agent) still keeps them, an open input method keeps them, and a view
/// that is not a terminal never has them.
#[test]
fn the_keys_are_live_at_a_prompt() {
    let predicate = scrollback_key_predicate(true);
    assert!(predicate.eval(&ctx_with(&["Terminal", "EditorFocused"])));
    assert!(predicate.eval(&ctx_with(&["Terminal"])));
    let rich_input = ctx_with(&["Terminal", "LongRunningCommand", "EditorFocused"]);
    assert!(!predicate.eval(&rich_input));
    assert!(!predicate.eval(&ctx_with(&["Terminal", "EditorFocused", "IMEOpen"])));
    assert!(!predicate.eval(&ctx_with(&["EditorFocused"])));
}

/// The four bindings are editable, named, and carry the default keystrokes in `ALL` order.
#[test]
fn the_bindings_are_editable_and_named() {
    let names: Vec<&str> = ScrollbackKey::ALL
        .iter()
        .map(|key| key.binding_name())
        .collect();
    assert_eq!(
        names,
        vec![
            "terminal:scrollback_page_up",
            "terminal:scrollback_page_down",
            "terminal:scrollback_top",
            "terminal:scrollback_bottom",
        ]
    );
}

/// The prompt half is on unless either switch says `0`: the main switch turns everything off.
#[test]
fn the_prompt_kill_switch_is_off_only_for_zero() {
    assert!(at_prompt_enabled_from(None, None));
    assert!(at_prompt_enabled_from(Some("1"), Some("")));
    assert!(!at_prompt_enabled_from(None, Some("0")));
    assert!(!at_prompt_enabled_from(None, Some(" 0 ")));
    assert!(!at_prompt_enabled_from(Some("0"), None));
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
