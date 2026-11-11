use super::*;
use crate::keymap::Keystroke;
use pathfinder_geometry::vector::vec2f;

fn key_down(key: &str, chars: &str) -> Event {
    Event::KeyDown {
        keystroke: Keystroke::parse(key).expect("parses"),
        chars: chars.to_owned(),
        details: Default::default(),
        is_composing: false,
    }
}

#[test]
fn typed_text_is_given_by_its_length_only() {
    let secret = "hunter2-ppCI";
    let typed = describe_event(&Event::TypedCharacters {
        chars: format!("{secret}\n"),
    });
    assert_eq!(typed, "kind=TypedCharacters chars_len=13 lines=1");
    assert!(!typed.contains(secret));

    // A key that types one character is not named either.
    let key = describe_event(&key_down("q", "q"));
    assert!(key.contains("key=<char>"), "{key}");
    assert!(key.contains("chars_len=1"), "{key}");
    assert!(!key.contains("key=q"), "{key}");
}

#[test]
fn a_named_key_is_given_by_name() {
    let tab = describe_event(&key_down("tab", "\t"));
    assert!(tab.starts_with("kind=KeyDown key=tab "), "{tab}");
    let chord = describe_event(&key_down("ctrl-shift-enter", ""));
    assert!(chord.contains("key=enter ctrl=true"), "{chord}");
    assert!(chord.contains("shift=true"), "{chord}");
    assert!(chord.contains("chars_len=0"), "{chord}");
}

#[test]
fn the_event_line_carries_order_step_target_and_answer() {
    let hover = Event::MouseMoved {
        position: vec2f(336.27, 722.0),
        cmd: false,
        shift: false,
        is_synthetic: false,
    };
    assert_eq!(
        injected_event_line(
            7,
            "Hover over\nstatus",
            &hover,
            Some("editor:cursor_3"),
            false
        ),
        "integ_event seq=7 step=\"Hover over status\" kind=MouseMoved x=336.3 y=722.0 \
         synthetic=false position_id=editor:cursor_3 handled=false"
    );
    let line = injected_event_line(8, "Open menu", &key_down("tab", "\t"), None, true);
    assert!(line.starts_with("integ_event seq=8 step=\"Open menu\" kind=KeyDown key=tab "));
    assert!(line.ends_with(" handled=true"), "{line}");
    assert!(!line.contains("position_id"), "{line}");
}

#[test]
fn a_skip_line_names_the_scenario_and_what_it_needs() {
    assert_eq!(
        skip_line(
            "test_ssh_into_ash",
            Some("the remote shell `ash`\n is not installed")
        ),
        "INTEG-SKIP test_ssh_into_ash NEEDS: the remote shell `ash` is not installed"
    );
    let unnamed = skip_line("test_x", None);
    assert!(
        unnamed.starts_with("INTEG-SKIP test_x NEEDS: "),
        "{unnamed}"
    );
    assert!(unnamed.contains("no reason given"), "{unnamed}");
}

#[test]
fn the_first_skip_reason_wins_and_is_taken_once() {
    // The only test that touches the process-wide slot.
    assert!(needs(true, "never recorded"));
    assert!(!needs(false, "sshd"));
    assert!(!skip_because("a later gate"));
    assert_eq!(take_skip_reason().as_deref(), Some("sshd"));
    assert_eq!(take_skip_reason(), None);
}

#[test]
fn the_failure_state_is_one_bounded_line() {
    let line = failure_state_line(
        "Wait for\nbootstrapping",
        "input=\"cd \"\nblock output:\n$ ",
    );
    assert_eq!(
        line,
        "integ_failure_state step=\"Wait for bootstrapping\" input=\\\"cd \\\"\\nblock output:\\n$ "
    );
    let long = failure_state_line("s", &"x".repeat(10_000));
    assert!(!long.contains('\n'));
    assert!(long.contains("...[4000 chars cut]"), "{}", &long[..80]);
    assert!(long.ends_with("xxxx"));
}
