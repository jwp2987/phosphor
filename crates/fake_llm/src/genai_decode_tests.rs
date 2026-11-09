//! L0: the real genai client (the BYOP send path's decoder) against llmposter, a scripted wire
//! server and a byte-level chaos proxy.
//!
//! Every test name starts with `genai_decode` so `script/agent_fuzz` selects them with
//! `-E 'test(genai_decode)'`. (A unit-test module, not a `tests/` binary, so that the `--lib`
//! runs the repo's scripts use pick it up.)
//!
//! What is pinned:
//! * text and reasoning survive any cut of the byte stream (inside UTF-8, inside a JSON string,
//!   between `data:` and `\n\n`), byte-exact;
//! * a tool call's arguments, streamed as many deltas, are reassembled byte-exact, including
//!   deliberately invalid JSON (the app, not genai, decides what to do with it);
//! * a dropped connection is an error, never a short reply; a connection that closes cleanly
//!   before `[DONE]` is visibly an end without an End event (the app has a guard for that);
//! * 429 / 529 / 503 / 500 / HTML / context-overflow bodies reach the caller with the status or
//!   the provider's own message intact, so the app's classifiers have something to match.

use std::time::Duration;

use crate::decode::{Decoded, decode};
use crate::llm;
use crate::scenario::*;
use crate::strategies::{arb_split, arb_text};
use crate::wire::{ChaosProxy, Piece, WireCall, WireResponse, WireServer, cut, openai_frames};
use proptest::prelude::*;
use serde_json::{Value, json};

const DEADLINE: Duration = Duration::from_secs(5);

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime")
}

fn text_scenario(text: &str, chaos: Option<Chaos>) -> Scenario {
    let mut s = Scenario::new(0);
    s.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::Text {
            text: text.to_owned(),
        },
        chaos,
    });
    s
}

fn call_scenario(call: Call) -> Scenario {
    let mut s = Scenario::new(0);
    s.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls { calls: vec![call] },
        chaos: None,
    });
    s
}

fn assert_text(d: &Decoded, expected: &str) {
    assert!(d.is_clean(), "stream should end cleanly: {d:#?}");
    assert_eq!(d.text, expected, "text must reassemble byte-exact");
}

// ---- llmposter: text, tool calls --------------------------------------------------------

proptest! {
    #[test]
    fn genai_decode_llmposter_text_reassembles(text in arb_text(), chunk in 1usize..12) {
        let decoded = runtime().block_on(async {
            let mut chaos = Chaos::default();
            chaos.split = Some(Split::Bytes { n: chunk });
            let server = llm::serve(&text_scenario(&text, Some(chaos))).await;
            decode(&format!("{}/v1", server.url()), DEADLINE).await
        });
        assert_text(&decoded, &text);
    }
}

#[test]
fn genai_decode_llmposter_tool_call_arguments_parse() {
    for name in KNOWN_TOOLS {
        let call = Call::new(CallId::Fresh, name, Args::Valid);
        let expected: Value = serde_json::from_str(&call.render_args(&Default::default())).unwrap();
        let decoded = runtime().block_on(async {
            let server = llm::serve(&call_scenario(call.clone())).await;
            decode(&format!("{}/v1", server.url()), DEADLINE).await
        });
        assert!(decoded.is_clean(), "{name}: {decoded:#?}");
        let end = decoded
            .end_calls
            .as_ref()
            .expect("End carries the tool calls");
        assert_eq!(end.len(), 1, "{name}: exactly one call");
        assert_eq!(end[0].name, *name);
        assert_eq!(
            end[0].args, expected,
            "{name}: End parses the accumulated arguments"
        );
        // The chunk the app accumulates by id holds the raw text.
        let raw = decoded.chunk_calls[0]
            .args
            .as_str()
            .expect("raw string arguments");
        assert_eq!(
            serde_json::from_str::<Value>(raw).unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn genai_decode_llmposter_turns_follow_request_order() {
    let mut s = Scenario::new(0);
    for (n, text) in ["first", "second"].into_iter().enumerate() {
        s.turns.push(Turn {
            when: Match::Nth { n: n as u32 },
            reply: Reply::Text {
                text: text.to_owned(),
            },
            chaos: None,
        });
    }
    runtime().block_on(async {
        let server = llm::serve(&s).await;
        let base = format!("{}/v1", server.url());
        assert_eq!(decode(&base, DEADLINE).await.text, "first");
        assert_eq!(decode(&base, DEADLINE).await.text, "second");
        // Past the script, the fake answers END rather than hanging the app.
        assert_eq!(decode(&base, DEADLINE).await.text, "END");
    });
}

// ---- byte-level cuts through the chaos proxy --------------------------------------------

proptest! {
    #[test]
    fn genai_decode_proxy_cuts_keep_text_exact(text in arb_text(), split in arb_split()) {
        let decoded = runtime().block_on(async {
            let server = llm::serve(&text_scenario(&text, None)).await;
            let upstream = format!("127.0.0.1:{}", server.port()).parse().unwrap();
            let proxy = ChaosProxy::start(upstream, split, Duration::ZERO).await;
            decode(&proxy.base_url(), DEADLINE).await
        });
        assert_text(&decoded, &text);
    }
}

// ---- split tool-call arguments over the scripted wire -------------------------------------

fn wire_calls(args: &str, parts: usize) -> Vec<WireCall> {
    vec![WireCall {
        id: "call_wire_1".to_owned(),
        name: "run_shell_command".to_owned(),
        args: args.to_owned(),
        arg_parts: parts,
    }]
}

fn pieces(frames: Vec<Vec<u8>>, split: Option<&Split>) -> Vec<Piece> {
    let bytes: Vec<u8> = frames.concat();
    match split {
        Some(split) => cut(&bytes, split).into_iter().map(Piece::Bytes).collect(),
        None => frames.into_iter().map(Piece::Bytes).collect(),
    }
}

proptest! {
    // Not a hang: the wire server sleeps 1 ms after every piece so the reader sees each on its
    // own, so a case that cuts a few KB of SSE into 1-byte pieces costs seconds. 256 such cases
    // outlast the 60 s test timeout; 32 still cover every split shape (the other split tests
    // cover fixed shapes).
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Valid JSON arguments, many deltas, the frames themselves cut at arbitrary bytes.
    #[test]
    fn genai_decode_wire_split_arguments_are_byte_exact(
        command in arb_text(),
        parts in 1usize..12,
        split in arb_split(),
    ) {
        let args = json!({"command": command, "is_read_only": false}).to_string();
        let decoded = runtime().block_on(async {
            let frames = openai_frames("", &wire_calls(&args, parts), true);
            let server = WireServer::start(vec![WireResponse::sse(pieces(frames, Some(&split)))]).await;
            decode(&server.base_url(), DEADLINE).await
        });
        prop_assert!(decoded.is_clean(), "{:#?}", decoded);
        prop_assert_eq!(decoded.chunk_calls.len(), 1);
        prop_assert_eq!(decoded.chunk_calls[0].call_id.as_str(), "call_wire_1");
        prop_assert_eq!(decoded.chunk_calls[0].args.as_str(), Some(args.as_str()));
        prop_assert_eq!(&decoded.end_calls.as_ref().unwrap()[0].args, &serde_json::from_str::<Value>(&args).unwrap());
    }
}

#[test]
fn genai_decode_wire_invalid_json_arguments_arrive_as_the_raw_text() {
    for args in [
        "{\"command\": \"echo",
        "",
        "[1,2",
        "{\"a\":}",
        "not json at all",
    ] {
        let decoded = runtime().block_on(async {
            let frames = openai_frames("", &wire_calls(args, 3), true);
            let server = WireServer::start(vec![WireResponse::sse(pieces(frames, None))]).await;
            decode(&server.base_url(), DEADLINE).await
        });
        assert!(decoded.is_clean(), "{args:?}: {decoded:#?}");
        // Whatever genai does with the End copy, the accumulated chunk is the exact text, which
        // is what the app parses (and rejects) itself.
        let raw = decoded.chunk_calls.first().map(|c| c.args.clone());
        assert_eq!(raw, Some(Value::String(args.to_owned())), "{args:?}");
    }
}

#[test]
fn genai_decode_wire_two_calls_keep_their_ids_and_order() {
    let calls = vec![
        WireCall {
            id: "call_a".into(),
            name: "read_files".into(),
            args: "{\"files\":[]}".into(),
            arg_parts: 2,
        },
        WireCall {
            id: "call_b".into(),
            name: "grep".into(),
            args: "{\"queries\":[\"x\"]}".into(),
            arg_parts: 3,
        },
    ];
    let decoded = runtime().block_on(async {
        let frames = openai_frames("thinking", &calls, true);
        let server = WireServer::start(vec![WireResponse::sse(pieces(frames, None))]).await;
        decode(&server.base_url(), DEADLINE).await
    });
    assert!(decoded.is_clean(), "{decoded:#?}");
    assert_eq!(decoded.text, "thinking");
    let ids: Vec<_> = decoded
        .chunk_calls
        .iter()
        .map(|c| c.call_id.as_str())
        .collect();
    assert_eq!(ids, ["call_a", "call_b"]);
    let end: Vec<_> = decoded
        .end_calls
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(end, ["read_files", "grep"]);
}

// ---- broken wires -------------------------------------------------------------------------

#[test]
fn genai_decode_disconnect_mid_stream_is_an_error_not_a_short_reply() {
    for after in [1usize, 2, 3, 5] {
        let decoded = runtime().block_on(async {
            let mut body = pieces(openai_frames("hello there", &[], true), None);
            body.truncate(after);
            body.push(Piece::Disconnect);
            let server = WireServer::start(vec![WireResponse::sse(body)]).await;
            decode(&server.base_url(), DEADLINE).await
        });
        assert!(!decoded.saw_end, "after {after}: {decoded:#?}");
        assert!(
            decoded.stream_error.is_some() || decoded.open_error.is_some(),
            "after {after} frames: a dropped connection must surface as an error, got {decoded:#?}"
        );
    }
}

#[test]
fn genai_decode_clean_close_before_done_is_a_visibly_missing_end() {
    let decoded = runtime().block_on(async {
        let frames = openai_frames("hello there", &[], false);
        let server = WireServer::start(vec![WireResponse::sse(pieces(frames, None))]).await;
        decode(&server.base_url(), DEADLINE).await
    });
    assert!(!decoded.saw_end);
    assert!(
        decoded.ended_without_end,
        "genai reports this as a plain end of stream; the app's missing-End guard is the defence: {decoded:#?}"
    );
    assert_eq!(decoded.text, "hello there", "what arrived is delivered");
}

#[test]
fn genai_decode_stall_is_catchable_with_a_timeout() {
    let decoded = runtime().block_on(async {
        let mut body = pieces(openai_frames("hello", &[], true), None);
        body.truncate(2);
        body.push(Piece::Stall);
        let server = WireServer::start(vec![WireResponse::sse(body)]).await;
        decode(&server.base_url(), Duration::from_millis(400)).await
    });
    assert!(decoded.timed_out, "{decoded:#?}");
}

#[test]
fn genai_decode_llmposter_truncation_never_looks_complete() {
    for frames in [1u32, 2, 3, 4] {
        let decoded = runtime().block_on(async {
            let chaos = Chaos {
                disconnect_after: Some(frames),
                ..Chaos::default()
            };
            let server = llm::serve(&text_scenario(
                "a fairly long reply that needs several chunks",
                Some(chaos),
            ))
            .await;
            decode(&format!("{}/v1", server.url()), DEADLINE).await
        });
        assert!(
            !decoded.saw_end,
            "truncated after {frames} frames must not carry an End: {decoded:#?}"
        );
        assert!(
            decoded.ended_without_end
                || decoded.stream_error.is_some()
                || decoded.open_error.is_some(),
            "truncated after {frames}: {decoded:#?}"
        );
    }
}

// ---- status mapping ------------------------------------------------------------------------

fn error_text(d: &Decoded) -> String {
    format!(
        "{} {}",
        d.open_error.clone().unwrap_or_default(),
        d.stream_error.clone().unwrap_or_default()
    )
}

#[test]
fn genai_decode_llmposter_http_statuses_reach_the_caller() {
    for code in [429u16, 529, 503, 500] {
        let decoded = runtime().block_on(async {
            let chaos = Chaos {
                status: Some(StatusKind::Http { code }),
                ..Chaos::default()
            };
            let server = llm::serve(&text_scenario("never sent", Some(chaos))).await;
            decode(&format!("{}/v1", server.url()), DEADLINE).await
        });
        assert!(
            !decoded.saw_end && decoded.text.is_empty(),
            "{code}: {decoded:#?}"
        );
        let text = error_text(&decoded);
        assert!(
            text.contains(&code.to_string()),
            "{code}: status missing from {text:?}"
        );
    }
}

#[test]
fn genai_decode_wire_html_error_page_is_an_error_with_its_status() {
    let decoded = runtime().block_on(async {
        let server = WireServer::start(vec![WireResponse::error(
            502,
            "text/html",
            "<html><body><h1>502 Bad Gateway</h1></body></html>",
        )])
        .await;
        decode(&server.base_url(), DEADLINE).await
    });
    let text = error_text(&decoded);
    assert!(!decoded.saw_end && text.contains("502"), "{decoded:#?}");
}

#[test]
fn genai_decode_llmposter_context_overflow_keeps_the_providers_wording() {
    let decoded = runtime().block_on(async {
        let chaos = Chaos {
            context_overflow: true,
            ..Chaos::default()
        };
        let server = llm::serve(&text_scenario("never sent", Some(chaos))).await;
        decode(&format!("{}/v1", server.url()), DEADLINE).await
    });
    let text = error_text(&decoded);
    assert!(
        text.contains("maximum context length"),
        "the app's overflow detection matches on the provider's text: {text:?}"
    );
}

#[test]
fn genai_decode_in_stream_error_event_is_an_error() {
    let decoded = runtime().block_on(async {
        let mut body = pieces(openai_frames("partial", &[], true), None);
        body.truncate(2);
        body.push(Piece::Bytes(
            b"data: {\"error\":{\"message\":\"Overloaded\",\"type\":\"overloaded_error\"}}\n\n"
                .to_vec(),
        ));
        let server = WireServer::start(vec![WireResponse::sse(body)]).await;
        decode(&server.base_url(), DEADLINE).await
    });
    assert!(!decoded.saw_end);
    assert!(decoded.stream_error.is_some(), "{decoded:#?}");
}
