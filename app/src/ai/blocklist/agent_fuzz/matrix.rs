//! The approvals matrix: a deterministic, exhaustive sweep of what a user can do to what an agent
//! asked for, next to the proptest in `fuzz_tests.rs` (which samples; this enumerates).
//!
//! Every case is a [`Scenario`] built in code (no randomness; seeds come from the case index), run
//! through the same rig and oracle as every other case: I1-I7 are checked after every step, so a
//! stuck card, a lost tool message, a card that still takes keys, a panic or an ERROR log in ANY
//! cell fails it and prints the scenario as schema JSON. Each sweep runs with the safe read-only
//! allowlist OFF (the shipped default, and the gate) and ON (a separate test, so an ON failure
//! never hides an OFF result).
//!
//! | sweep | cells |
//! |---|---|
//! | `kinds x responses` | every action kind x every user response (approve, reject, edit, Ctrl-C, stop, close the tab, switch mode, escape, type a follow-up, answer, skip, never) at the card |
//! | `timings` | a command / a never-ending command / a question x the main responses x early (during the stream), fast (10-25 ms), late, after the end, twice, a burst of three |
//! | `orders` | every ordered PAIR of six kinds in one turn, and a seeded sample of ordered triples, x seven user policies (approve all, reject first, reject last, Ctrl-C / stop / close the tab after the first, a follow-up) |
//! | `malformed` | every tool x every malformed-argument shape x approve / reject / Ctrl-C |
//! | `commands` | every command behaviour (quick, failing, slow, never ends, silent, pager, alt screen, prompt) x approve / reject / Ctrl-C / stop / follow-up |

use fake_llm::scenario::{
    ActorStep, Args, Call, CallId, CmdBehaviour, KNOWN_TOOLS, Match, Op, Reply, Scenario, Timing,
    Turn,
};

use super::runner::run_case_with;

fn tool(name: &str, args: Args) -> Call {
    Call::new(CallId::Fresh, name, args)
}

/// One kind of thing the agent can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A command the allowlist does not list (`fake-cmd cN`): always asks.
    Command,
    /// `echo hi`: the one line the ON runs list, so it asks OFF and runs by itself ON.
    ListedCommand,
    FailingCommand,
    SlowCommand,
    HungCommand,
    ReadFiles,
    Grep,
    FileGlob,
    Question,
    Transfer,
    WriteToCommand,
    ReadCommandOutput,
    WebSearch,
    WebFetch,
}

pub const KINDS: [Kind; 14] = [
    Kind::Command,
    Kind::ListedCommand,
    Kind::FailingCommand,
    Kind::SlowCommand,
    Kind::HungCommand,
    Kind::ReadFiles,
    Kind::Grep,
    Kind::FileGlob,
    Kind::Question,
    Kind::Transfer,
    Kind::WriteToCommand,
    Kind::ReadCommandOutput,
    Kind::WebSearch,
    Kind::WebFetch,
];

/// The six kinds the order sweeps permute: a command, a failing command, a never-ending one, a
/// read, a question and a grep.
const ORDER_KINDS: [Kind; 6] = [
    Kind::Command,
    Kind::FailingCommand,
    Kind::HungCommand,
    Kind::ReadFiles,
    Kind::Question,
    Kind::Grep,
];

#[derive(Default)]
struct Built {
    calls: Vec<Call>,
    commands: Vec<(String, CmdBehaviour)>,
}

/// Appends the call(s) for `kind` at position `pos` of the turn. Command keys are unique per
/// position so several commands in one turn do not share a behaviour.
fn push_kind(kind: Kind, pos: usize, out: &mut Built) {
    let key = format!("c{pos}");
    let command = |out: &mut Built, behaviour: CmdBehaviour| {
        out.calls.push(tool(
            "run_shell_command",
            Args::Command { cmd: key.clone() },
        ));
        out.commands.push((key.clone(), behaviour));
    };
    match kind {
        Kind::Command => command(
            out,
            CmdBehaviour::Quick {
                out: "ok".to_owned(),
            },
        ),
        Kind::ListedCommand => out.calls.push(tool("run_shell_command", Args::Valid)),
        Kind::FailingCommand => command(out, CmdBehaviour::SetE),
        Kind::SlowCommand => command(
            out,
            CmdBehaviour::Slow {
                ms: 400,
                out: "slow ok".to_owned(),
            },
        ),
        Kind::HungCommand => command(out, CmdBehaviour::NeverEnds),
        Kind::ReadFiles => out.calls.push(tool("read_files", Args::Valid)),
        Kind::Grep => out.calls.push(tool("grep", Args::Valid)),
        Kind::FileGlob => out.calls.push(tool("file_glob", Args::Valid)),
        Kind::Question => out.calls.push(tool("ask_user_question", Args::Valid)),
        Kind::WebSearch => out.calls.push(tool("websearch", Args::Valid)),
        Kind::WebFetch => out.calls.push(tool("webfetch", Args::Valid)),
        Kind::Transfer | Kind::WriteToCommand | Kind::ReadCommandOutput => {
            // These act on a command that is still alive: a never-ending one in the same turn.
            let target = format!("t{pos}");
            out.calls.push(tool(
                "run_shell_command",
                Args::Command {
                    cmd: target.clone(),
                },
            ));
            out.commands.push((target.clone(), CmdBehaviour::NeverEnds));
            let name = match kind {
                Kind::Transfer => "transfer_shell_command_control_to_user",
                Kind::WriteToCommand => "write_to_long_running_shell_command",
                _ => "read_shell_command_output",
            };
            out.calls.push(tool(name, Args::RefersTo { cmd: target }));
        }
    }
}

fn build(kinds: &[Kind]) -> Built {
    let mut built = Built::default();
    for (pos, kind) in kinds.iter().enumerate() {
        push_kind(*kind, pos, &mut built);
    }
    built
}

/// A two-request scenario: the model asks for `built.calls`, then says "done" once the results
/// come back.
fn scenario(seed: u64, built: Built, actor: Vec<ActorStep>) -> Scenario {
    let mut s = Scenario::new(seed);
    s.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls { calls: built.calls },
        chaos: None,
    });
    s.turns.push(Turn {
        when: Match::Nth { n: 1 },
        reply: Reply::Text {
            text: "done".to_owned(),
        },
        chaos: None,
    });
    s.env.commands = built.commands.into_iter().collect();
    s.actor = actor;
    s
}

/// What the user can do about a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    Approve,
    Reject,
    Edit,
    CtrlC,
    Stop,
    CloseTab,
    ModeSwitch,
    Escape,
    FollowUp,
    Answer,
    Skip,
    Never,
}

pub const RESPONSES: [Response; 12] = [
    Response::Approve,
    Response::Reject,
    Response::Edit,
    Response::CtrlC,
    Response::Stop,
    Response::CloseTab,
    Response::ModeSwitch,
    Response::Escape,
    Response::FollowUp,
    Response::Answer,
    Response::Skip,
    Response::Never,
];

fn op(response: Response) -> Op {
    match response {
        Response::Approve | Response::Never => Op::Approve,
        Response::Reject => Op::Reject,
        Response::Edit => Op::Edit {
            text: "echo edited".to_owned(),
        },
        Response::CtrlC => Op::CtrlC,
        Response::Stop => Op::Stop,
        Response::CloseTab => Op::CloseTab,
        Response::ModeSwitch => Op::ModeSwitch,
        Response::Escape => Op::Escape,
        Response::FollowUp => Op::FollowUp {
            text: "and then list the files".to_owned(),
        },
        Response::Answer => Op::AnswerQuestion { choice: 0 },
        Response::Skip => Op::SkipQuestion,
    }
}

fn step(response: Response, timing: Timing) -> ActorStep {
    ActorStep {
        op: op(response),
        timing: if response == Response::Never {
            Timing::Never
        } else {
            timing
        },
    }
}

/// Runs every case; one failing cell does not hide the others.
fn run_all(label: &str, cases: Vec<(String, Scenario)>, allowlist: bool) {
    let total = cases.len();
    let mut failures = Vec::new();
    for (name, mut scenario) in cases {
        scenario.name = Some(name.clone());
        if let Err(failure) = run_case_with(&scenario, allowlist) {
            failures.push(format!(
                "[{name}] (allowlist {}): {failure}",
                if allowlist { "on" } else { "off" }
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{label}: {} of {total} cells broke an invariant\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

// ---- sweep 1: kinds x responses ------------------------------------------------------------

pub fn kinds_x_responses(kind: Kind) -> Vec<(String, Scenario)> {
    RESPONSES
        .iter()
        .enumerate()
        .map(|(i, response)| {
            (
                format!("{kind:?}/{response:?}"),
                scenario(
                    1000 + i as u64,
                    build(&[kind]),
                    vec![step(*response, Timing::OnCard)],
                ),
            )
        })
        .collect()
}

// ---- sweep 2: timings ----------------------------------------------------------------------

/// When the answer arrives relative to the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// While the response that holds the call is still streaming.
    Early,
    /// 10-25 ms after the previous step.
    Fast,
    /// Human-slow.
    Late,
    /// After the stream ended.
    AfterEnd,
    /// The same answer twice, the second 10-25 ms later.
    Twice,
    /// Three in a burst.
    Burst,
}

pub const ARRIVALS: [Arrival; 6] = [
    Arrival::Early,
    Arrival::Fast,
    Arrival::Late,
    Arrival::AfterEnd,
    Arrival::Twice,
    Arrival::Burst,
];

const TIMING_KINDS: [Kind; 3] = [Kind::Command, Kind::HungCommand, Kind::Question];
const TIMING_RESPONSES: [Response; 6] = [
    Response::Approve,
    Response::Reject,
    Response::CtrlC,
    Response::Stop,
    Response::CloseTab,
    Response::FollowUp,
];

fn arrival_steps(response: Response, arrival: Arrival) -> Vec<ActorStep> {
    match arrival {
        Arrival::Early => vec![step(response, Timing::DuringStream)],
        Arrival::Fast => vec![step(response, Timing::Fast)],
        Arrival::Late => vec![step(response, Timing::Human)],
        Arrival::AfterEnd => vec![step(response, Timing::AfterEnd)],
        Arrival::Twice => vec![step(response, Timing::OnCard), step(response, Timing::Fast)],
        Arrival::Burst => vec![
            step(response, Timing::OnCard),
            step(response, Timing::Fast),
            step(response, Timing::Fast),
        ],
    }
}

pub fn timings(kind: Kind) -> Vec<(String, Scenario)> {
    let mut cases = Vec::new();
    let mut seed = 2000;
    for response in TIMING_RESPONSES {
        for arrival in ARRIVALS {
            seed += 1;
            // A question is answered, not approved.
            let response = match (kind, response) {
                (Kind::Question, Response::Approve) => Response::Answer,
                _ => response,
            };
            cases.push((
                format!("{kind:?}/{response:?}/{arrival:?}"),
                scenario(seed, build(&[kind]), arrival_steps(response, arrival)),
            ));
        }
    }
    cases
}

// ---- sweep 3: orders -----------------------------------------------------------------------

/// What the user does across a multi-action turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    ApproveAll,
    RejectFirst,
    RejectLast,
    CtrlCAfterFirst,
    StopAfterFirst,
    CloseTabAfterFirst,
    FollowUpAfterFirst,
}

pub const POLICIES: [Policy; 7] = [
    Policy::ApproveAll,
    Policy::RejectFirst,
    Policy::RejectLast,
    Policy::CtrlCAfterFirst,
    Policy::StopAfterFirst,
    Policy::CloseTabAfterFirst,
    Policy::FollowUpAfterFirst,
];

fn accept(kind: Kind) -> Response {
    if kind == Kind::Question {
        Response::Answer
    } else {
        Response::Approve
    }
}

fn policy_steps(kinds: &[Kind], policy: Policy) -> Vec<ActorStep> {
    let on_card = |r| step(r, Timing::OnCard);
    let first = kinds[0];
    match policy {
        Policy::ApproveAll => kinds.iter().map(|k| on_card(accept(*k))).collect(),
        Policy::RejectFirst => {
            let mut steps = vec![on_card(Response::Reject)];
            steps.extend(kinds[1..].iter().map(|k| on_card(accept(*k))));
            steps
        }
        Policy::RejectLast => {
            let mut steps: Vec<ActorStep> = kinds[..kinds.len() - 1]
                .iter()
                .map(|k| on_card(accept(*k)))
                .collect();
            steps.push(on_card(Response::Reject));
            steps
        }
        Policy::CtrlCAfterFirst => {
            vec![on_card(accept(first)), step(Response::CtrlC, Timing::Fast)]
        }
        Policy::StopAfterFirst => {
            vec![on_card(accept(first)), step(Response::Stop, Timing::Fast)]
        }
        Policy::CloseTabAfterFirst => {
            vec![
                on_card(accept(first)),
                step(Response::CloseTab, Timing::Fast),
            ]
        }
        Policy::FollowUpAfterFirst => {
            vec![
                on_card(accept(first)),
                step(Response::FollowUp, Timing::Fast),
            ]
        }
    }
}

/// Every ordered pair starting with `first`, x every policy.
pub fn pairs(first: Kind) -> Vec<(String, Scenario)> {
    let mut cases = Vec::new();
    let mut seed = 3000;
    for second in ORDER_KINDS {
        let kinds = [first, second];
        for policy in POLICIES {
            seed += 1;
            cases.push((
                format!("{first:?}+{second:?}/{policy:?}"),
                scenario(seed, build(&kinds), policy_steps(&kinds, policy)),
            ));
        }
    }
    cases
}

/// A seeded, deterministic sample of ordered triples starting with `first` (every one of the 36
/// continuations is reachable from some seed; the sample takes 12), x three policies.
pub fn triples(first: Kind) -> Vec<(String, Scenario)> {
    let mut cases = Vec::new();
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15 ^ (first as u64 + 1);
    let mut next = move || {
        // xorshift64*: deterministic, no dependency.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        (state.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize
    };
    let mut seed = 4000;
    for _ in 0..12 {
        let second = ORDER_KINDS[next() % ORDER_KINDS.len()];
        let third = ORDER_KINDS[next() % ORDER_KINDS.len()];
        let kinds = [first, second, third];
        for policy in [
            Policy::ApproveAll,
            Policy::RejectFirst,
            Policy::CtrlCAfterFirst,
        ] {
            seed += 1;
            cases.push((
                format!("{first:?}+{second:?}+{third:?}/{policy:?}"),
                scenario(seed, build(&kinds), policy_steps(&kinds, policy)),
            ));
        }
    }
    cases
}

// ---- sweep 4: malformed arguments ----------------------------------------------------------

const MALFORMED: [Args; 5] = [
    Args::Empty,
    Args::EmptyObject,
    Args::Malformed,
    Args::WrongTypes,
    Args::ExtraFields,
];

const MALFORMED_QUESTIONS: [Args; 4] = [
    Args::AskQuestionsEmpty,
    Args::AskBareStrings,
    Args::AskOptionsEmpty,
    Args::AskFieldMissing,
];

pub fn malformed(tool_name: &str) -> Vec<(String, Scenario)> {
    let mut shapes: Vec<Args> = MALFORMED.to_vec();
    if tool_name == "ask_user_question" {
        shapes.extend(MALFORMED_QUESTIONS.iter().cloned());
    }
    let mut cases = Vec::new();
    let mut seed = 5000;
    for args in shapes {
        for response in [Response::Approve, Response::Reject, Response::CtrlC] {
            seed += 1;
            let mut built = Built::default();
            built.calls.push(tool(tool_name, args.clone()));
            cases.push((
                format!("{tool_name}/{args:?}/{response:?}"),
                scenario(seed, built, vec![step(response, Timing::OnCard)]),
            ));
        }
    }
    cases
}

// ---- sweep 5: command behaviours -----------------------------------------------------------

pub fn command_behaviours() -> Vec<(&'static str, CmdBehaviour)> {
    vec![
        (
            "quick",
            CmdBehaviour::Quick {
                out: "ok".to_owned(),
            },
        ),
        ("failing", CmdBehaviour::SetE),
        (
            "slow",
            CmdBehaviour::Slow {
                ms: 600,
                out: "slow ok".to_owned(),
            },
        ),
        ("never_ends", CmdBehaviour::NeverEnds),
        (
            "silent",
            CmdBehaviour::Silent {
                s: "working".to_owned(),
            },
        ),
        ("pager", CmdBehaviour::Pager),
        ("alt_screen", CmdBehaviour::AltScreen),
        (
            "prompt",
            CmdBehaviour::RealPrompt {
                prompt: "Password:".to_owned(),
                secret: true,
            },
        ),
    ]
}

pub fn commands() -> Vec<(String, Scenario)> {
    let mut cases = Vec::new();
    let mut seed = 6000;
    for (name, behaviour) in command_behaviours() {
        for response in [
            Response::Approve,
            Response::Reject,
            Response::CtrlC,
            Response::Stop,
            Response::FollowUp,
        ] {
            seed += 1;
            let built = Built {
                calls: vec![tool(
                    "run_shell_command",
                    Args::Command {
                        cmd: "c0".to_owned(),
                    },
                )],
                commands: vec![("c0".to_owned(), behaviour.clone())],
            };
            // Approve the command, then (for the later responses) act on it while it runs.
            let mut steps = vec![step(Response::Approve, Timing::OnCard)];
            match response {
                Response::Approve => {}
                Response::Reject => steps = vec![step(Response::Reject, Timing::OnCard)],
                other => steps.push(step(other, Timing::Human)),
            }
            cases.push((format!("{name}/{response:?}"), scenario(seed, built, steps)));
        }
    }
    cases
}

// ---- the tests -----------------------------------------------------------------------------

/// One module per row so nextest runs rows in parallel processes (the ERROR-log tap is one per
/// process); `off` is the gate, `on` is the allowlist run.
macro_rules! rows {
    ($($module:ident => $cases:expr),* $(,)?) => {
        $(
            mod $module {
                use super::*;

                #[test]
                fn off() {
                    run_all(stringify!($module), $cases, false);
                }

                #[test]
                fn on() {
                    run_all(stringify!($module), $cases, true);
                }
            }
        )*
    };
}

rows! {
    kinds_command => kinds_x_responses(Kind::Command),
    kinds_listed_command => kinds_x_responses(Kind::ListedCommand),
    kinds_failing_command => kinds_x_responses(Kind::FailingCommand),
    kinds_slow_command => kinds_x_responses(Kind::SlowCommand),
    kinds_hung_command => kinds_x_responses(Kind::HungCommand),
    kinds_read_files => kinds_x_responses(Kind::ReadFiles),
    kinds_grep => kinds_x_responses(Kind::Grep),
    kinds_file_glob => kinds_x_responses(Kind::FileGlob),
    kinds_question => kinds_x_responses(Kind::Question),
    kinds_transfer => kinds_x_responses(Kind::Transfer),
    kinds_write_to_command => kinds_x_responses(Kind::WriteToCommand),
    kinds_read_command_output => kinds_x_responses(Kind::ReadCommandOutput),
    kinds_websearch => kinds_x_responses(Kind::WebSearch),
    kinds_webfetch => kinds_x_responses(Kind::WebFetch),
    timings_command => timings(Kind::Command),
    timings_hung_command => timings(Kind::HungCommand),
    timings_question => timings(Kind::Question),
    pairs_command => pairs(Kind::Command),
    pairs_failing_command => pairs(Kind::FailingCommand),
    pairs_hung_command => pairs(Kind::HungCommand),
    pairs_read_files => pairs(Kind::ReadFiles),
    pairs_question => pairs(Kind::Question),
    pairs_grep => pairs(Kind::Grep),
    triples_command => triples(Kind::Command),
    triples_hung_command => triples(Kind::HungCommand),
    triples_read_files => triples(Kind::ReadFiles),
    triples_question => triples(Kind::Question),
    malformed_run_shell_command => malformed("run_shell_command"),
    malformed_read_files => malformed("read_files"),
    malformed_grep => malformed("grep"),
    malformed_file_glob => malformed("file_glob"),
    malformed_ask_user_question => malformed("ask_user_question"),
    malformed_transfer => malformed("transfer_shell_command_control_to_user"),
    malformed_write_to_command => malformed("write_to_long_running_shell_command"),
    malformed_read_command_output => malformed("read_shell_command_output"),
    malformed_websearch => malformed("websearch"),
    malformed_webfetch => malformed("webfetch"),
    command_behaviours => commands(),
}

/// The sweeps cover every tool the app offers and every response; a new tool or response must be
/// added to the matrix (this fails until it is).
#[test]
fn the_matrix_names_every_tool_and_every_response() {
    let mut named: Vec<String> = Vec::new();
    for kind in KINDS {
        let mut built = Built::default();
        push_kind(kind, 0, &mut built);
        named.extend(built.calls.into_iter().map(|call| call.name));
    }
    for tool_name in KNOWN_TOOLS {
        assert!(
            named.iter().any(|name| name == tool_name),
            "KNOWN_TOOLS has {tool_name}, but no matrix Kind asks for it"
        );
    }
    assert_eq!(RESPONSES.len(), 12);
}

/// The sizes the docs quote.
#[test]
fn the_matrix_sizes_are_what_the_docs_say() {
    let kinds: usize = KINDS.iter().map(|k| kinds_x_responses(*k).len()).sum();
    let timing: usize = TIMING_KINDS.iter().map(|k| timings(*k).len()).sum();
    let pair: usize = [
        Kind::Command,
        Kind::FailingCommand,
        Kind::HungCommand,
        Kind::ReadFiles,
        Kind::Question,
        Kind::Grep,
    ]
    .iter()
    .map(|k| pairs(*k).len())
    .sum();
    let triple: usize = [
        Kind::Command,
        Kind::HungCommand,
        Kind::ReadFiles,
        Kind::Question,
    ]
    .iter()
    .map(|k| triples(*k).len())
    .sum();
    let bad: usize = KNOWN_TOOLS.iter().map(|t| malformed(t).len()).sum();
    let cmds = commands().len();
    assert_eq!(kinds, 14 * 12);
    assert_eq!(timing, 3 * 6 * 6);
    assert_eq!(pair, 6 * 6 * 7);
    assert_eq!(triple, 4 * 12 * 3);
    assert_eq!(bad, 10 * 5 * 3 + 4 * 3);
    assert_eq!(cmds, 8 * 5);
}
