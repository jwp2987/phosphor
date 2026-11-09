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
