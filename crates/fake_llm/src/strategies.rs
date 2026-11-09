//! proptest strategies over the scenario schema. Shared by the L0 wire proptests and the L1
//! agent fuzzer, so a shrunk case from either is a plain [`Scenario`] (printed as schema JSON).

use proptest::prelude::*;

use crate::scenario::*;

/// Text with the awkward characters: multi-byte, quotes, backslashes, newlines, emoji.
pub fn arb_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            8 => prop::char::range('a', 'z'),
            2 => Just(' '),
            1 => Just('"'),
            1 => Just('\\'),
            1 => Just('\n'),
            1 => Just('é'),
            1 => Just('✓'),
            1 => Just('日'),
            1 => Just('😀'),
        ],
        0..60,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

pub fn arb_split() -> impl Strategy<Value = Split> {
    prop_oneof![
        (1usize..12).prop_map(|n| Split::Bytes { n }),
        prop::collection::vec(1usize..9, 1..6).prop_map(|cuts| Split::Offsets { cuts }),
    ]
}

pub fn arb_status() -> impl Strategy<Value = StatusKind> {
    prop_oneof![
        Just(StatusKind::Http { code: 429 }),
        Just(StatusKind::Http { code: 529 }),
        Just(StatusKind::Http { code: 503 }),
        Just(StatusKind::Http { code: 500 }),
        Just(StatusKind::Html),
    ]
}

/// Mostly no chaos, so most cases exercise the app's logic rather than only its error paths.
pub fn arb_chaos() -> impl Strategy<Value = Option<Chaos>> {
    prop_oneof![
        10 => Just(None),
        2 => (arb_split(), 0u64..3).prop_map(|(split, delay_ms)| Some(Chaos {
            split: Some(split),
            delay_ms,
            ..Chaos::default()
        })),
        1 => (1u32..6).prop_map(|n| Some(Chaos {
            disconnect_after: Some(n),
            ..Chaos::default()
        })),
        1 => (1u32..6).prop_map(|n| Some(Chaos {
            stall_after: Some(n),
            ..Chaos::default()
        })),
        1 => arb_status().prop_map(|s| Some(Chaos {
            status: Some(s),
            ..Chaos::default()
        })),
        1 => Just(Some(Chaos {
            context_overflow: true,
            ..Chaos::default()
        })),
        1 => Just(Some(Chaos {
            never_end: true,
            ..Chaos::default()
        })),
    ]
}

/// The scenario command keys every generated scenario defines.
pub const CMD_KEYS: [&str; 3] = ["c0", "c1", "c2"];

pub fn arb_cmd_behaviour() -> impl Strategy<Value = CmdBehaviour> {
    prop_oneof![
        4 => arb_text().prop_map(|out| CmdBehaviour::Quick { out }),
        2 => arb_text().prop_map(|s| CmdBehaviour::Silent { s }),
        2 => arb_text().prop_map(|s| CmdBehaviour::TrailingColon { s: format!("{s}:") }),
        2 => arb_text().prop_map(|s| CmdBehaviour::TrailingQuestion { s: format!("{s}?") }),
        2 => (arb_text(), any::<bool>())
            .prop_map(|(prompt, secret)| CmdBehaviour::RealPrompt { prompt, secret }),
        1 => Just(CmdBehaviour::NeverEnds),
        1 => Just(CmdBehaviour::SetE),
        1 => Just(CmdBehaviour::AltScreen),
        1 => Just(CmdBehaviour::Pager),
    ]
}

fn arb_tool_name() -> impl Strategy<Value = String> {
    prop_oneof![
        12 => prop::sample::select(KNOWN_TOOLS.to_vec()).prop_map(str::to_owned),
        1 => Just("not_a_real_tool".to_owned()),
        1 => Just("apply_file_diffs".to_owned()),
    ]
}

fn arb_args(name: &str) -> BoxedStrategy<Args> {
    let cmd = prop::sample::select(CMD_KEYS.to_vec()).prop_map(str::to_owned);
    let common = prop_oneof![
        10 => Just(Args::Valid),
        1 => Just(Args::Empty),
        1 => Just(Args::EmptyObject),
        1 => Just(Args::Malformed),
        1 => Just(Args::WrongTypes),
        1 => Just(Args::ExtraFields),
    ];
    match name {
        "run_shell_command" => prop_oneof![
            6 => cmd.prop_map(|cmd| Args::Command { cmd }),
            3 => common,
        ]
        .boxed(),
        "ask_user_question" => prop_oneof![
            5 => Just(Args::Valid),
            2 => Just(Args::AskQuestionsEmpty),
            1 => Just(Args::AskBareStrings),
            1 => Just(Args::AskOptionsEmpty),
            1 => Just(Args::AskFieldMissing),
            1 => Just(Args::Empty),
            1 => Just(Args::Malformed),
        ]
        .boxed(),
        "transfer_shell_command_control_to_user"
        | "write_to_long_running_shell_command"
        | "read_shell_command_output" => prop_oneof![
            4 => cmd.prop_map(|cmd| Args::RefersTo { cmd }),
            3 => common,
        ]
        .boxed(),
        _ => common.boxed(),
    }
}

/// `k` is the index of the call within its reply, for `DuplicateOf`.
fn arb_call(k: usize) -> impl Strategy<Value = Call> {
    arb_tool_name().prop_flat_map(move |name| {
        let id = prop_oneof![
            12 => Just(CallId::Fresh),
            1 => Just(CallId::Missing),
            1 => Just(CallId::Literal { id: "call_fixed".to_owned() }),
            if k == 0 { 0 } else { 1 } => (0..k.max(1)).prop_map(|k| CallId::DuplicateOf { k }),
        ];
        (id, arb_args(&name)).prop_map(move |(id, args)| Call {
            id,
            name: name.clone(),
            args,
        })
    })
}

fn arb_calls() -> impl Strategy<Value = Vec<Call>> {
    (1usize..4).prop_flat_map(|n| (0..n).map(arb_call).collect::<Vec<_>>())
}

pub fn arb_reply() -> impl Strategy<Value = Reply> {
    prop_oneof![
        3 => arb_text().prop_map(|text| Reply::Text { text }),
        1 => arb_text().prop_map(|text| Reply::Reasoning { text }),
        8 => arb_calls().prop_map(|calls| Reply::ToolCalls { calls }),
        2 => (arb_text(), arb_calls()).prop_map(|(text, calls)| Reply::Mixed { text, calls }),
        1 => Just(Reply::Empty),
    ]
}

pub fn arb_op() -> impl Strategy<Value = Op> {
    // More arms than `prop_oneof!` takes, so a weighted `Union` of boxed strategies.
    prop::strategy::Union::new_weighted(vec![
        (6, Just(Op::Approve).boxed()),
        (3, Just(Op::Reject).boxed()),
        (1, arb_text().prop_map(|text| Op::Edit { text }).boxed()),
        (
            3,
            (0usize..3)
                .prop_map(|choice| Op::AnswerQuestion { choice })
                .boxed(),
        ),
        (1, Just(Op::SkipQuestion).boxed()),
        (2, Just(Op::CtrlC).boxed()),
        (1, Just(Op::Stop).boxed()),
        (2, arb_text().prop_map(|text| Op::FollowUp { text }).boxed()),
        (1, Just(Op::ModeSwitch).boxed()),
        (1, Just(Op::Escape).boxed()),
        (1, Just(Op::TakeOver).boxed()),
        (1, Just(Op::GiveBack).boxed()),
        (1, Just(Op::CloseTab).boxed()),
        (1, (0u64..50).prop_map(|ms| Op::Wait { ms }).boxed()),
    ])
}

pub fn arb_timing() -> impl Strategy<Value = Timing> {
    prop_oneof![
        4 => Just(Timing::OnCard),
        3 => Just(Timing::Fast),
        1 => Just(Timing::Human),
        2 => Just(Timing::DuringStream),
        2 => Just(Timing::AfterEnd),
        1 => Just(Timing::Never),
    ]
}

pub fn arb_actor() -> impl Strategy<Value = Vec<ActorStep>> {
    prop::collection::vec(
        (arb_op(), arb_timing()).prop_map(|(op, timing)| ActorStep { op, timing }),
        0..7,
    )
}

/// 1-4 turns answering requests 0.., then the implicit `END`.
pub fn arb_scenario() -> impl Strategy<Value = Scenario> {
    (
        any::<u64>(),
        prop::collection::vec((arb_reply(), arb_chaos()), 1..5),
        prop::collection::vec(arb_cmd_behaviour(), CMD_KEYS.len()),
        arb_actor(),
    )
        .prop_map(|(seed, turns, behaviours, actor)| {
            let mut scenario = Scenario::new(seed);
            scenario.turns = turns
                .into_iter()
                .enumerate()
                .map(|(n, (reply, chaos))| Turn {
                    when: Match::Nth { n: n as u32 },
                    reply,
                    chaos,
                })
                .collect();
            scenario.env.commands = CMD_KEYS
                .iter()
                .map(|k| (*k).to_owned())
                .zip(behaviours)
                .collect();
            scenario.actor = actor;
            scenario
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    #[test]
    fn generated_scenarios_round_trip() {
        let mut runner = TestRunner::default();
        for _ in 0..64 {
            let scenario = arb_scenario().new_tree(&mut runner).unwrap().current();
            assert_eq!(Scenario::from_json(&scenario.to_json()).unwrap(), scenario);
        }
    }
}
