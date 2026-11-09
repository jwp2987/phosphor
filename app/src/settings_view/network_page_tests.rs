use super::*;

#[test]
fn add_normalises_to_one_origin_per_line_in_order() {
    let next = add_origin(
        "https://a.example, https://b.example  https://c.example",
        "d.example:8443",
    )
    .unwrap();
    assert_eq!(
        next,
        "https://a.example\nhttps://b.example\nhttps://c.example\nd.example:8443"
    );
}

#[test]
fn add_twice_adds_once_even_with_different_case() {
    let once = add_origin("", "https://A.example").unwrap();
    let twice = add_origin(&once, "https://a.example").unwrap();
    assert_eq!(once, "https://A.example");
    assert_eq!(twice, once);
}

#[test]
fn add_refuses_empty_and_multi_entry_input() {
    assert_eq!(add_origin("x.example", "   "), None);
    assert_eq!(add_origin("x.example", "a.example b.example"), None);
    assert_eq!(add_origin("x.example", "a.example,b.example"), None);
}

#[test]
fn remove_drops_only_the_named_origin_and_keeps_the_rest_normalised() {
    let text = "a.example,b.example\nc.example";
    assert_eq!(remove_origin(text, "b.example"), "a.example\nc.example");
    assert_eq!(remove_origin(text, "B.EXAMPLE"), "a.example\nc.example");
    assert_eq!(
        remove_origin(text, "zz.example"),
        "a.example\nb.example\nc.example"
    );
    assert_eq!(remove_origin("only.example", "only.example"), "");
}

#[test]
fn the_stored_text_parses_the_way_the_gate_parses_it() {
    // Same separators as `network_gate::Gate::set_user_allowed_origins`.
    let stored = add_origin("a.example,b.example", "c.example").unwrap();
    let gate = http_client::gate::Gate::new();
    gate.set_user_allowed_origins(&stored);
    for host in ["a.example", "b.example", "c.example"] {
        assert!(gate.is_explicit_host(host), "{host}");
    }
}

#[test]
fn a_hostless_denial_offers_no_allow() {
    // A hostless denial (the redirect cap) has nothing to allow.
    let e = http_client::gate::GateEvent {
        seq: 0,
        purpose: "p",
        host: String::new(),
        port: None,
        host_hash: 0,
        outcome: http_client::gate::Outcome::Denied,
    };
    assert_eq!(crate::network_gate_log::allow_text(&e), None);
}

#[test]
fn origin_hash_is_the_host_hash_and_ignores_scheme_port_path() {
    let want = format!("{:016x}", http_client::gate::hash_host("localhost"));
    for o in [
        "localhost",
        "localhost:8080",
        "http://LocalHost:9/x?y",
        "https://localhost/",
    ] {
        assert_eq!(origin_hash(o), want, "{o}");
    }
    let v6 = format!("{:016x}", http_client::gate::hash_host("::1"));
    assert_eq!(origin_hash("[::1]:80"), v6);
    assert!(!origin_hash("localhost:8080").contains("localhost"));
}

#[test]
fn ring_order_is_editor_then_removes_then_allows() {
    let stops = kb_stops(&["a:1".into(), "b:2".into()], &["c:3".into()]);
    assert_eq!(
        stops,
        vec![
            KbStop::OriginEditor,
            KbStop::Remove("a:1".into()),
            KbStop::Remove("b:2".into()),
            KbStop::Allow("c:3".into())
        ]
    );
    // Even with nothing listed, the editor is reachable.
    assert_eq!(kb_stops(&[], &[]), vec![KbStop::OriginEditor]);
}

#[test]
fn ring_steps_wrap_both_ways_and_start_at_the_ends() {
    assert_eq!(kb_step(None, 4, 1), 0);
    assert_eq!(kb_step(None, 4, -1), 3);
    assert_eq!(kb_step(Some(3), 4, 1), 0);
    assert_eq!(kb_step(Some(0), 4, -1), 3);
    assert_eq!(kb_step(Some(1), 4, 1), 2);
    assert_eq!(kb_step(Some(0), 1, 1), 0);
}

#[test]
fn enter_removes_or_allows_the_focused_stop_and_delete_only_removes() {
    let rm = KbStop::Remove("a:1".into());
    let al = KbStop::Allow("c:3".into());
    assert!(
        matches!(kb_action_for(&rm, false), Some(NetworkPageAction::RemoveOrigin(o)) if o == "a:1")
    );
    assert!(matches!(
        kb_action_for(&rm, true),
        Some(NetworkPageAction::RemoveOrigin(_))
    ));
    assert!(
        matches!(kb_action_for(&al, false), Some(NetworkPageAction::AllowOrigin(o)) if o == "c:3")
    );
    assert!(kb_action_for(&al, true).is_none());
    assert!(kb_action_for(&KbStop::OriginEditor, false).is_none());
}

#[test]
fn keyboard_add_then_remove_and_allow_reach_the_stored_text() {
    // What the keyboard actions apply: Enter in the editor adds, Enter on Remove removes,
    // Enter on a log Allow appends (same helpers the click path uses).
    let added = add_origin("", "localhost:4000").unwrap();
    assert_eq!(parse_origin_list(&added), vec!["localhost:4000"]);
    let allowed = add_origin(&added, "example.test:443").unwrap();
    assert_eq!(parse_origin_list(&allowed).len(), 2);
    let removed = remove_origin(&allowed, "LOCALHOST:4000");
    assert_eq!(parse_origin_list(&removed), vec!["example.test:443"]);
}
