use super::*;

fn facts(editor_vim_keys: bool, shell_vi_mode: bool) -> ViKeyFacts {
    ViKeyFacts {
        editor_vim_keys,
        shell_vi_mode,
    }
}

/// The behaviour that is kept: without vi keys, Escape leaves the agent view.
#[test]
fn escape_leaves_the_agent_view_without_vi_keys() {
    assert_eq!(
        agent_view_escape(facts(false, false), true, true),
        AgentViewEscape::Leave
    );
}

/// #1984: the input editor has vim keybindings on; an Escape in normal mode stays.
#[test]
fn escape_stays_when_the_editor_has_vim_keys() {
    assert_eq!(
        agent_view_escape(facts(true, false), true, true),
        AgentViewEscape::StaysWithViKeys
    );
}

/// #1984: the shell edits its line in vi mode; the Escape habit stays in the view too.
#[test]
fn escape_stays_when_the_shell_is_in_vi_mode() {
    assert_eq!(
        agent_view_escape(facts(false, true), true, true),
        AgentViewEscape::StaysWithViKeys
    );
    assert_eq!(
        agent_view_escape(facts(true, true), true, true),
        AgentViewEscape::StaysWithViKeys
    );
}

/// Not changed: where the view cannot be left now (a running command, the agent's own command),
/// Escape keeps the meaning it had, vi keys or not.
#[test]
fn a_running_command_keeps_the_existing_escape() {
    for vi in [
        facts(false, false),
        facts(true, false),
        facts(false, true),
        facts(true, true),
    ] {
        assert_eq!(
            agent_view_escape(vi, false, true),
            AgentViewEscape::Leave,
            "{vi:?}"
        );
    }
}

/// The kill switch restores the behaviour before #1984 in every pane.
#[test]
fn the_kill_switch_makes_escape_leave_again() {
    for vi in [facts(true, false), facts(false, true), facts(true, true)] {
        assert_eq!(
            agent_view_escape(vi, true, false),
            AgentViewEscape::Leave,
            "{vi:?}"
        );
    }
}

/// Only `0` turns the rule off; unset, empty or anything else leaves it on.
#[test]
fn the_kill_switch_is_only_zero() {
    assert!(vi_escape_stays_enabled_from(None));
    assert!(vi_escape_stays_enabled_from(Some("")));
    assert!(vi_escape_stays_enabled_from(Some("1")));
    assert!(vi_escape_stays_enabled_from(Some("on")));
    assert!(!vi_escape_stays_enabled_from(Some("0")));
    assert!(!vi_escape_stays_enabled_from(Some(" 0 ")));
}
