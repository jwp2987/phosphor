//! The scenario schema (`"v": 1`), shared by every layer of the agent fuzzer.
//!
//! L0 compiles a scenario's turns into llmposter fixtures or a scripted wire; L1 feeds them to the
//! in-process seam in `app` (`agent_providers::fake_stream_seam`); the later L2 serves them from
//! llmposter to the real app. One JSON shape, so a failing case from any layer is replayable in
//! any other, and a corpus entry is just a file.
//!
//! Everything here is plain data (serde). The only behaviour is rendering a [`Call`]'s arguments
//! to the raw string a provider would put on the wire ([`Call::render_args`]), because the
//! malformed shapes (`Malformed`, a missing id) cannot be expressed as a JSON value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Current schema version. Bump on any breaking change; the corpus loader rejects others.
pub const SCHEMA_VERSION: u32 = 1;

/// Tool names the app offers (see `agent_providers/tools`). A call to any other name is
/// `Unknown`; a call to one of these that `tools_offered` leaves out is `NotOffered`.
pub const KNOWN_TOOLS: &[&str] = &[
    "run_shell_command",
    "read_files",
    "grep",
    "file_glob",
    "ask_user_question",
    "transfer_shell_command_control_to_user",
    "write_to_long_running_shell_command",
    "read_shell_command_output",
    "websearch",
    "webfetch",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub v: u32,
    /// Optional human name (the corpus file stem, or a shrunk case's origin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub seed: u64,
    /// The tools the request advertises. `None` = every tool in [`KNOWN_TOOLS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_offered: Option<Vec<String>>,
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub env: Env,
    #[serde(default)]
    pub actor: Vec<ActorStep>,
}

impl Scenario {
    pub fn new(seed: u64) -> Self {
        Self {
            v: SCHEMA_VERSION,
            name: None,
            seed,
            tools_offered: None,
            turns: Vec::new(),
            env: Env::default(),
            actor: Vec::new(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a Scenario always serializes")
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let scenario: Scenario =
            serde_json::from_str(s).map_err(|e| format!("bad scenario: {e}"))?;
        if scenario.v != SCHEMA_VERSION {
            return Err(format!(
                "scenario schema v{} is not supported (this build reads v{SCHEMA_VERSION})",
                scenario.v
            ));
        }
        Ok(scenario)
    }

    pub fn offers(&self, tool: &str) -> bool {
        match &self.tools_offered {
            Some(list) => list.iter().any(|t| t == tool),
            None => KNOWN_TOOLS.contains(&tool),
        }
    }
}

// ---- turns ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub when: Match,
    pub reply: Reply,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chaos: Option<Chaos>,
}

/// Which request a turn answers. The first turn (in order) that matches wins, so a `Default` turn
/// listed last is the fallback. A request that matches no turn gets the text `END`, so the fake
/// never hangs the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Match {
    /// The nth request of the run (0-based).
    Nth {
        n: u32,
    },
    /// The last message of the request is the tool result for this emitted call id.
    LastToolResult {
        id: String,
    },
    /// The last user message contains this token.
    UserContains {
        token: String,
    },
    Default,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Text { text: String },
    Reasoning { text: String },
    ToolCalls { calls: Vec<Call> },
    Mixed { text: String, calls: Vec<Call> },
    Empty,
}

impl Reply {
    pub fn calls(&self) -> &[Call] {
        match self {
            Reply::ToolCalls { calls } | Reply::Mixed { calls, .. } => calls,
            _ => &[],
        }
    }
}

// ---- calls ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub id: CallId,
    pub name: String,
    pub args: Args,
}

impl Call {
    pub fn new(id: CallId, name: &str, args: Args) -> Self {
        Self {
            id,
            name: name.to_owned(),
            args,
        }
    }

    /// The raw argument string as it goes on the wire. `cmd_ids` maps a scenario command key
    /// (`env.commands`) to the id the app assigned its block, for `RefersTo`.
    pub fn render_args(&self, cmd_ids: &BTreeMap<String, String>) -> String {
        render_args(&self.name, &self.args, cmd_ids)
    }
}

/// How a call's id is produced. The harness resolves these to concrete strings; `Missing` and
/// `DuplicateOf` are the malformed-provider shapes (empty id / repeated id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallId {
    Fresh,
    Literal {
        id: String,
    },
    Missing,
    /// The id of the kth call of this reply (0-based; clamped to a preceding call).
    DuplicateOf {
        k: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Args {
    /// Well-formed arguments for the tool (see [`render_args`]).
    Valid,
    /// An empty string (a provider that sends no arguments at all).
    Empty,
    /// `{}`.
    EmptyObject,
    /// Truncated / unbalanced JSON.
    Malformed,
    /// Right keys, wrong types.
    WrongTypes,
    /// Valid arguments plus keys the schema does not know.
    ExtraFields,
    /// A command-targeting call (`write_to_*`, `transfer_*`, `read_shell_command_output`) naming
    /// a command that never ran.
    RefersTo { cmd: String },
    /// `ask_user_question` with `questions: []`.
    AskQuestionsEmpty,
    /// `ask_user_question` with bare-string questions.
    AskBareStrings,
    /// `ask_user_question` whose question has `options: []`.
    AskOptionsEmpty,
    /// `ask_user_question` whose question object lacks `question`.
    AskFieldMissing,
    /// The command a `run_shell_command` should run is the scenario command of this key.
    Command { cmd: String },
}

pub fn render_args(name: &str, args: &Args, cmd_ids: &BTreeMap<String, String>) -> String {
    use serde_json::json;
    let valid = |name: &str| -> serde_json::Value {
        match name {
            "run_shell_command" => json!({"command": "echo hi", "is_read_only": false}),
            "read_files" => json!({"files": [{"path": "Cargo.toml"}]}),
            "grep" => json!({"queries": ["fn main"], "path": "."}),
            "file_glob" => json!({"patterns": ["*.rs"], "path": "."}),
            "ask_user_question" => json!({"questions": [{
                "question": "Which one?", "options": ["a", "b"], "recommended_index": 0,
                "multi_select": false, "supports_other": false}]}),
            "transfer_shell_command_control_to_user" => json!({"reason": "needs your input"}),
            "write_to_long_running_shell_command" => {
                json!({"command_id": "none", "input": "y", "mode": "line"})
            }
            "read_shell_command_output" => json!({"command_id": "none"}),
            "websearch" => json!({"query": "rust"}),
            "webfetch" => json!({"url": "http://127.0.0.1:9/"}),
            _ => json!({}),
        }
    };
    match args {
        Args::Valid => valid(name).to_string(),
        Args::Empty => String::new(),
        Args::EmptyObject => "{}".to_owned(),
        Args::Malformed => {
            let s = valid(name).to_string();
            s[..s.len().saturating_sub(3).max(1)].to_owned()
        }
        Args::WrongTypes => {
            let mut v = valid(name);
            if let Some(obj) = v.as_object_mut() {
                for value in obj.values_mut() {
                    *value = match value {
                        serde_json::Value::String(_) => json!(7),
                        serde_json::Value::Array(_) => json!("not an array"),
                        serde_json::Value::Bool(_) => json!("maybe"),
                        _ => json!([]),
                    };
                }
            }
            v.to_string()
        }
        Args::ExtraFields => {
            let mut v = valid(name);
            if let Some(obj) = v.as_object_mut() {
                obj.insert("zz_unknown".into(), json!({"nested": [1, 2, 3]}));
            }
            v.to_string()
        }
        Args::RefersTo { cmd } => {
            let id = cmd_ids
                .get(cmd)
                .cloned()
                .unwrap_or_else(|| format!("never-ran-{cmd}"));
            let mut v = valid(name);
            if let Some(obj) = v.as_object_mut() {
                if obj.contains_key("command_id") {
                    obj.insert("command_id".into(), json!(id));
                }
            }
            v.to_string()
        }
        Args::AskQuestionsEmpty => json!({"questions": []}).to_string(),
        Args::AskBareStrings => {
            json!({"questions": ["Delete the branches?", "Really?"]}).to_string()
        }
        Args::AskOptionsEmpty => {
            json!({"questions": [{"question": "Which one?", "options": []}]}).to_string()
        }
        Args::AskFieldMissing => {
            json!({"questions": [{"required": true, "text": "Which one?"}]}).to_string()
        }
        Args::Command { cmd } => {
            json!({"command": format!("fake-cmd {cmd}"), "is_read_only": false}).to_string()
        }
    }
}

// ---- commands (L1/L2 environment) ---------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Env {
    /// Scenario command key -> behaviour. A `run_shell_command` with `Args::Command { cmd }` runs
    /// the behaviour under that key; any other command is `Quick("")`.
    #[serde(default)]
    pub commands: BTreeMap<String, CmdBehaviour>,
}

/// What a command does once the harness runs it. `RealPrompt` is the only behaviour that reads
/// the tty; the three `Trailing*` / `Silent` shapes only LOOK like a prompt (#1796) and must never
/// be handed to the user (invariant I5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CmdBehaviour {
    Quick {
        out: String,
    },
    Silent {
        s: String,
    },
    TrailingColon {
        s: String,
    },
    TrailingQuestion {
        s: String,
    },
    /// `read -p` (or `read -s` when `secret`).
    RealPrompt {
        prompt: String,
        secret: bool,
    },
    NeverEnds,
    SetE,
    AltScreen,
    Pager,
}

impl CmdBehaviour {
    /// Whether a hand-over to the user is correct for this command (I5).
    pub fn reads_tty(&self) -> bool {
        matches!(self, CmdBehaviour::RealPrompt { .. })
    }
}

// ---- chaos --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Chaos {
    /// Cut every wire write into pieces of at most this many bytes (L0 / L2; L1 splits events).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<Split>,
    #[serde(default)]
    pub delay_ms: u64,
    /// Stop sending after this many events/frames and then go silent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stall_after: Option<u32>,
    /// Drop the connection after this many events/frames.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disconnect_after: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusKind>,
    #[serde(default)]
    pub context_overflow: bool,
    /// Send everything except the terminal frame (`[DONE]` / End).
    #[serde(default)]
    pub never_end: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Split {
    /// Fixed piece size in bytes.
    Bytes { n: usize },
    /// Pieces cut at these offsets (cycled and clamped), so a test can cut inside a UTF-8
    /// sequence, a JSON string, or between `data:` and `\n\n`.
    Offsets { cuts: Vec<usize> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatusKind {
    /// 429, 529, 503, 500, ... with a JSON error body.
    Http { code: u16 },
    /// A 502 with an HTML body, as a proxy in front of the provider sends.
    Html,
}

impl Chaos {
    /// The chaos kinds this turn exercises, for the per-kind ERROR-log allowlist (I6) and the
    /// coverage table.
    pub fn kinds(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.split.is_some() {
            out.push("split");
        }
        if self.delay_ms > 0 {
            out.push("delay");
        }
        if self.stall_after.is_some() {
            out.push("stall");
        }
        if self.disconnect_after.is_some() {
            out.push("disconnect");
        }
        if self.status.is_some() {
            out.push("status");
        }
        if self.context_overflow {
            out.push("context_overflow");
        }
        if self.never_end {
            out.push("never_end");
        }
        out
    }
}

// ---- actor --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorStep {
    pub op: Op,
    pub timing: Timing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    Approve,
    Reject,
    Edit { text: String },
    AnswerQuestion { choice: usize },
    SkipQuestion,
    CtrlC,
    Stop,
    FollowUp { text: String },
    ModeSwitch,
    Escape,
    TakeOver,
    GiveBack,
    CloseTab,
    Wait { ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Timing {
    /// 10-25 ms after the trigger.
    Fast,
    /// 400-2500 ms after the trigger.
    Human,
    /// While the response stream is still in flight.
    DuringStream,
    /// After the stream ended.
    AfterEnd,
    /// As soon as a card is on screen.
    OnCard,
    /// The step is skipped (the user never acts).
    Never,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_round_trips_through_json() {
        let mut s = Scenario::new(7);
        s.turns.push(Turn {
            when: Match::Nth { n: 0 },
            reply: Reply::ToolCalls {
                calls: vec![Call::new(
                    CallId::Fresh,
                    "ask_user_question",
                    Args::AskQuestionsEmpty,
                )],
            },
            chaos: Some(Chaos {
                split: Some(Split::Bytes { n: 3 }),
                ..Chaos::default()
            }),
        });
        s.actor.push(ActorStep {
            op: Op::AnswerQuestion { choice: 1 },
            timing: Timing::OnCard,
        });
        let back = Scenario::from_json(&s.to_json()).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn other_schema_versions_are_rejected() {
        let mut s = Scenario::new(1);
        s.v = 2;
        assert!(Scenario::from_json(&s.to_json()).is_err());
    }

    #[test]
    fn malformed_args_do_not_parse_but_valid_ones_do() {
        let ids = BTreeMap::new();
        for name in KNOWN_TOOLS {
            let valid = render_args(name, &Args::Valid, &ids);
            serde_json::from_str::<serde_json::Value>(&valid).unwrap();
            let bad = render_args(name, &Args::Malformed, &ids);
            assert!(
                serde_json::from_str::<serde_json::Value>(&bad).is_err(),
                "{name}: {bad}"
            );
        }
    }
}
