//! "Explain" on the requested-command approval card (issue #891).
//!
//! The command the agent wants to run is sent to one of the user's BYOP models through the
//! one-shot path, and the answer is shown on the card: what the command does piece by piece,
//! and a risk verdict. It is a side channel on purpose. The one-shot path never touches
//! `task.messages`, so the exchange never enters the agent conversation's history, and nothing
//! here can approve, edit or run the command.
//!
//! Fork addition with no Warp counterpart: Warp's approval card has no explain affordance, and
//! this is built only on BYOP, so there is no pinned behavior to diverge from.
//!
//! The model's verdict is backed by [`local_risk_flags`], a small syntactic check over the
//! command itself. It exists because the explaining model may be weak or wrong, and a
//! `reboot` that the model calls SAFE must still be shown as dangerous. It only ever raises
//! the verdict, never lowers it.

use crate::ai::agent_providers::oneshot::{OneshotConfig, OneshotOptions, byop_oneshot_completion};
use crate::ai::agent_providers::prompt_renderer;

/// Cap on the command text sent to the model, in chars. The card itself always shows and runs
/// the whole command; this only bounds the request, and the prompt says when it was cut.
pub(crate) const MAX_COMMAND_CHARS: usize = 12_000;

/// Headroom over [`MAX_COMMAND_CHARS`] for the wrapper text, so the one-shot's own cap never
/// cuts the closing tag off a command that was already sized for this call.
///
/// Escaping `</command` (see [`escape_command_close_tag`]) adds one char per occurrence, and
/// every occurrence is at least 9 chars long, hence the extra eighth.
const MAX_USER_CHARS: usize = MAX_COMMAND_CHARS + MAX_COMMAND_CHARS / 8 + 1_000;

/// Low: the explanation must track the command, not free-associate.
const TEMPERATURE: f32 = 0.2;

/// How deep `ssh host '...'` / `sh -c '...'` nesting is followed by [`local_risk_flags`].
const MAX_NESTING: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RiskLevel {
    Safe,
    Caution,
    Destructive,
}

/// What a syntactic match in [`local_risk_flags`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalRiskCategory {
    DeletesData,
    OverwritesData,
    RewritesHistory,
    PrivilegeOrPermissions,
    RunsRemoteCode,
    StopsServices,
    KillsProcesses,
    RebootsOrShutsDown,
    /// Redirects, `tee`, `sed -i`, `cp`, `mv`: writes that may clobber an existing file.
    WritesFiles,
    /// `apt remove`, `npm uninstall` and the like.
    RemovesPackages,
}

impl LocalRiskCategory {
    pub(crate) fn level(self) -> RiskLevel {
        match self {
            LocalRiskCategory::DeletesData
            | LocalRiskCategory::OverwritesData
            | LocalRiskCategory::RewritesHistory
            | LocalRiskCategory::RunsRemoteCode
            | LocalRiskCategory::StopsServices
            | LocalRiskCategory::RebootsOrShutsDown => RiskLevel::Destructive,
            LocalRiskCategory::PrivilegeOrPermissions
            | LocalRiskCategory::KillsProcesses
            | LocalRiskCategory::WritesFiles
            | LocalRiskCategory::RemovesPackages => RiskLevel::Caution,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalRiskFlag {
    pub category: LocalRiskCategory,
    /// The words that matched, e.g. `systemctl restart`.
    pub trigger: String,
    /// The host an `ssh` invocation runs the matched words on; `None` for this machine.
    pub remote_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandExplanation {
    /// The model's verdict; `None` when its reply carried no recognisable `RISK:` line.
    pub model_risk: Option<RiskLevel>,
    /// The model's list of risks, without the "None" placeholder.
    pub risks: Vec<String>,
    /// The model's piece-by-piece explanation, as plain text.
    pub explanation: String,
    pub local_flags: Vec<LocalRiskFlag>,
}

impl CommandExplanation {
    /// The verdict to show: the model's, raised to the most severe local flag. `None` only when
    /// the model stated no verdict and nothing was flagged locally.
    pub(crate) fn effective_risk(&self) -> Option<RiskLevel> {
        let local = self
            .local_flags
            .iter()
            .map(|flag| flag.category.level())
            .max();
        match (self.model_risk, local) {
            (Some(model), Some(local)) => Some(model.max(local)),
            (Some(model), None) => Some(model),
            (None, local) => local,
        }
    }
}

/// The card's explanation state. Every non-idle state records the exact command text it was
/// produced for, so an edit or a streamed update can never leave an explanation of different
/// text on screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ExplanationState {
    #[default]
    Idle,
    /// Explain was pressed with no usable BYOP model configured.
    NoProvider,
    Loading {
        command: String,
        model_id: String,
    },
    Ready {
        command: String,
        model_id: String,
        explanation: CommandExplanation,
    },
    Failed {
        command: String,
        model_id: String,
        error: String,
    },
}

/// What a press of Explain should do, given the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplainPress {
    /// Resolve a model and send a request.
    Start,
    /// Hide the explanation already shown for this command.
    Dismiss,
    /// A request for this command is already in flight.
    Ignore,
}

impl ExplanationState {
    fn command(&self) -> Option<&str> {
        match self {
            ExplanationState::Idle | ExplanationState::NoProvider => None,
            ExplanationState::Loading { command, .. }
            | ExplanationState::Ready { command, .. }
            | ExplanationState::Failed { command, .. } => Some(command),
        }
    }

    pub(crate) fn is_loading(&self) -> bool {
        matches!(self, ExplanationState::Loading { .. })
    }

    /// Whether this state should be drawn on a card currently showing `current_command`.
    pub(crate) fn is_visible_for(&self, current_command: &str) -> bool {
        match self {
            ExplanationState::Idle => false,
            ExplanationState::NoProvider => true,
            ExplanationState::Loading { .. }
            | ExplanationState::Ready { .. }
            | ExplanationState::Failed { .. } => self.command() == Some(current_command),
        }
    }

    /// Whether this state describes text other than `current_command` and should be dropped
    /// (aborting its request, if any).
    pub(crate) fn is_stale_for(&self, current_command: &str) -> bool {
        self.command()
            .is_some_and(|command| command != current_command)
    }

    pub(crate) fn on_press(&self, current_command: &str) -> ExplainPress {
        match self {
            ExplanationState::Loading { command, .. } if command == current_command => {
                ExplainPress::Ignore
            }
            ExplanationState::Ready { command, .. } if command == current_command => {
                ExplainPress::Dismiss
            }
            ExplanationState::Idle
            | ExplanationState::NoProvider
            | ExplanationState::Loading { .. }
            | ExplanationState::Ready { .. }
            | ExplanationState::Failed { .. } => ExplainPress::Start,
        }
    }

    /// Applies a finished request for `command`. Returns `false`, leaving the state untouched,
    /// when that request is no longer the one in flight -- the card was approved, rejected,
    /// dismissed or edited while it ran.
    pub(crate) fn finish(&mut self, command: &str, reply: Result<String, String>) -> bool {
        let ExplanationState::Loading {
            command: loading_command,
            model_id,
        } = self
        else {
            return false;
        };
        if loading_command.as_str() != command {
            return false;
        }
        let command = std::mem::take(loading_command);
        let model_id = std::mem::take(model_id);
        *self = match reply {
            Ok(raw) => match parse_explanation(&raw, &command) {
                Some(explanation) => ExplanationState::Ready {
                    command,
                    model_id,
                    explanation,
                },
                None => ExplanationState::Failed {
                    command,
                    model_id,
                    error: crate::t!("ai-command-explanation-empty-reply"),
                },
            },
            Err(error) => ExplanationState::Failed {
                command,
                model_id,
                error,
            },
        };
        true
    }
}

/// Sends `command` to the model in `cfg` and returns its raw reply.
pub(crate) async fn request_explanation(
    cfg: &OneshotConfig,
    command: &str,
) -> anyhow::Result<String> {
    let system = prompt_renderer::explain_command_system_prompt();
    let user = build_user_prompt(command);
    let opts = OneshotOptions {
        max_chars: Some(MAX_USER_CHARS),
        temperature: Some(TEMPERATURE),
        ..Default::default()
    };
    byop_oneshot_completion(cfg, &system, &user, &opts).await
}

/// Wraps the command for the model. The command is fenced in a tag so text inside it can't
/// read as an instruction, and a cut is stated rather than left for the model to miss.
pub(crate) fn build_user_prompt(command: &str) -> String {
    let total_chars = command.chars().count();
    if total_chars <= MAX_COMMAND_CHARS {
        let command = escape_command_close_tag(command);
        return format!(
            "Explain this command the agent wants to run.\n<command>\n{command}\n</command>"
        );
    }
    let shown: String = command.chars().take(MAX_COMMAND_CHARS).collect();
    let shown = escape_command_close_tag(&shown);
    format!(
        "Explain this command the agent wants to run. It is {total_chars} characters long; only \
         the first {MAX_COMMAND_CHARS} are shown, and the rest is hidden from you.\n\
         <command truncated=\"true\">\n{shown}\n</command>"
    )
}

/// Rewrites every `</command` (any case) to `<\/command`, so the command text can't close the
/// fence early and have the rest read as instructions outside it.
fn escape_command_close_tag(command: &str) -> String {
    const NEEDLE: &str = "</command";
    let lower = command.to_ascii_lowercase();
    let mut out = String::with_capacity(command.len());
    let mut last = 0;
    for (start, _) in lower.match_indices(NEEDLE) {
        out.push_str(&command[last..start]);
        out.push_str("<\\/command");
        last = start + NEEDLE.len();
    }
    out.push_str(&command[last..]);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplySection {
    Preamble,
    Risks,
    Explanation,
}

/// Parses a model reply in the prompt's `RISK:` / `RISKS:` / `EXPLANATION:` format, and adds
/// the local flags for `command`. Tolerant of markdown emphasis on the headers, reasoning
/// blocks and a wrapping code fence; a reply that ignores the format entirely is kept whole as
/// the explanation, with no model verdict. `None` when nothing usable is left.
pub(crate) fn parse_explanation(raw: &str, command: &str) -> Option<CommandExplanation> {
    let without_reasoning = strip_reasoning_blocks(raw);
    let cleaned = strip_code_fence(&without_reasoning);
    if cleaned.is_empty() {
        return None;
    }

    let mut model_risk = None;
    let mut risks = Vec::new();
    let mut preamble = Vec::new();
    let mut explanation = Vec::new();
    let mut section = ReplySection::Preamble;
    for line in cleaned.lines() {
        let header = line.replace(['*', '#', '_'], "");
        let header = header.trim();
        if let Some(value) = strip_prefix_ignore_ascii_case(header, "RISKS:") {
            section = ReplySection::Risks;
            push_risk(&mut risks, value);
            continue;
        }
        if let Some(value) = strip_prefix_ignore_ascii_case(header, "RISK:") {
            if model_risk.is_none() {
                model_risk = parse_risk_level(value);
            }
            continue;
        }
        if let Some(value) = strip_prefix_ignore_ascii_case(header, "EXPLANATION:") {
            section = ReplySection::Explanation;
            if !value.is_empty() {
                explanation.push(value.to_owned());
            }
            continue;
        }
        match section {
            ReplySection::Preamble => preamble.push(line.trim_end().to_owned()),
            ReplySection::Risks => push_risk(&mut risks, line),
            ReplySection::Explanation => explanation.push(line.trim_end().to_owned()),
        }
    }

    let body = if explanation.iter().any(|line| !line.trim().is_empty()) {
        explanation
    } else {
        preamble
    };
    let explanation = body.join("\n").trim().to_owned();
    if model_risk.is_none() && risks.is_empty() && explanation.is_empty() {
        return None;
    }
    // The model only saw the head of an over-long command, so "safe" (or no verdict) says
    // nothing about the tail.
    if command.chars().count() > MAX_COMMAND_CHARS {
        if model_risk.is_none_or(|level| level == RiskLevel::Safe) {
            model_risk = Some(RiskLevel::Caution);
        }
        risks.push(crate::t!(
            "ai-command-explanation-risk-truncated",
            limit = MAX_COMMAND_CHARS.to_string()
        ));
    }
    Some(CommandExplanation {
        model_risk,
        risks,
        explanation,
        local_flags: local_risk_flags(command),
    })
}

fn parse_risk_level(value: &str) -> Option<RiskLevel> {
    let value = value.trim_matches(|c: char| c.is_whitespace() || "*_#.:[]".contains(c));
    let mut words = value
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_uppercase);
    // Only a leading SAFE / CAUTION / DESTRUCTIVE is a verdict. "UNSAFE", "NOT SAFE" and the
    // like lead with something else and read as no verdict, never as Safe.
    let level_of = |word: &str| match word {
        "SAFE" => Some(RiskLevel::Safe),
        "CAUTION" => Some(RiskLevel::Caution),
        "DESTRUCTIVE" => Some(RiskLevel::Destructive),
        _ => None,
    };
    let first = level_of(&words.next()?)?;
    // Most severe wins, so a hedged "SAFE, but DESTRUCTIVE if ..." reads as destructive.
    Some(
        words
            .filter_map(|word| level_of(&word))
            .fold(first, RiskLevel::max),
    )
}

fn push_risk(risks: &mut Vec<String>, line: &str) {
    let item = line
        .trim()
        .trim_start_matches(['-', '*', '\u{2022}'])
        .trim();
    let is_placeholder = item.trim_end_matches('.').eq_ignore_ascii_case("none");
    if !item.is_empty() && !is_placeholder {
        risks.push(item.to_owned());
    }
}

/// `str::strip_prefix`, ASCII-case-insensitively. Uses `get` so a non-ASCII leading character
/// can't produce an out-of-bounds slice.
fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| text[prefix.len()..].trim())
}

/// Removes `<think>…</think>`-style blocks, which reasoning models emit before the answer.
/// Case-insensitive. An unterminated open tag drops everything from it on, and a lone close tag
/// (the open was cut off upstream) drops everything up to the last one.
fn strip_reasoning_blocks(raw: &str) -> String {
    let mut out = raw.to_owned();
    for tag in ["think", "reasoning", "thought", "scratchpad"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        // ASCII lowercasing keeps byte offsets, so indices into `lower` index `out`.
        loop {
            let lower = out.to_ascii_lowercase();
            let Some(start) = lower.find(&open) else {
                break;
            };
            match lower[start + open.len()..].find(&close) {
                Some(rel) => out.replace_range(start..start + open.len() + rel + close.len(), ""),
                None => {
                    out.truncate(start);
                    break;
                }
            }
        }
        let lower = out.to_ascii_lowercase();
        if let Some(idx) = lower.rfind(&close) {
            out.replace_range(..idx + close.len(), "");
        }
    }
    out
}

/// Unwraps a reply the model wrapped in a ``` fence, even one never closed.
fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }
    let Some(first_newline) = trimmed.find('\n') else {
        return "";
    };
    let body = &trimmed[first_newline + 1..];
    match body.rfind("```") {
        Some(close) => body[..close].trim(),
        None => body.trim(),
    }
}

/// One simple command of a command line: its words, and whether its stdin is piped from the
/// command before it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SimpleCommand {
    words: Vec<String>,
    piped: bool,
    /// Targets of the output redirections (`>`, `>>`, `&>`, `>&file`) that are not
    /// `/dev/null` or a file-descriptor duplication like `2>&1`.
    redirects: Vec<String>,
}

/// Splits a command line into simple commands at `;`, `&`, `|`, `&&`, `||`, newlines,
/// parentheses and backticks, unquoting words, and collects output redirection targets.
/// Deliberately approximate -- no expansions or heredocs -- because it only feeds [`local_risk_flags`], which errs towards
/// flagging.
fn split_simple_commands(line: &str) -> Vec<SimpleCommand> {
    /// An output redirection operator was just read; the next word is its target.
    #[derive(Clone, Copy)]
    struct PendingRedirect {
        /// `>&`, whose target may be a file descriptor.
        dup: bool,
    }

    #[derive(Default)]
    struct Builder {
        commands: Vec<SimpleCommand>,
        words: Vec<String>,
        redirects: Vec<String>,
        word: String,
        in_word: bool,
        piped: bool,
        pending: Option<PendingRedirect>,
    }

    impl Builder {
        fn end_word(&mut self) {
            if !self.in_word {
                return;
            }
            let word = std::mem::take(&mut self.word);
            self.in_word = false;
            match self.pending.take() {
                None => self.words.push(word),
                Some(PendingRedirect { dup })
                    if word == "/dev/null"
                        || (dup && (word == "-" || word.chars().all(|c| c.is_ascii_digit()))) => {}
                Some(_) => self.redirects.push(word),
            }
        }

        fn end_command(&mut self) {
            self.end_word();
            self.pending = None;
            if !self.words.is_empty() || !self.redirects.is_empty() {
                self.commands.push(SimpleCommand {
                    words: std::mem::take(&mut self.words),
                    piped: self.piped,
                    redirects: std::mem::take(&mut self.redirects),
                });
            }
        }
    }

    let mut b = Builder::default();
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                b.in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    b.word.push(c);
                }
            }
            '"' => {
                b.in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                b.word.push(escaped);
                            }
                        }
                        c => b.word.push(c),
                    }
                }
            }
            '\\' => {
                b.in_word = true;
                if let Some(escaped) = chars.next() && escaped != '\n' {
                    b.word.push(escaped);
                }
            }
            '>' | '&' if c == '>' || chars.peek() == Some(&'>') => {
                // `&>file` / `&>>file` / `>file` / `>>file` / `>|file` / `>&file`; a file
                // descriptor prefix like the `2` of `2>` belongs to the operator, not a word.
                if c == '&' {
                    chars.next();
                }
                if b.in_word && !b.word.is_empty() && b.word.chars().all(|d| d.is_ascii_digit()) {
                    b.word.clear();
                    b.in_word = false;
                } else {
                    b.end_word();
                }
                if matches!(chars.peek(), Some('>' | '|')) {
                    chars.next();
                }
                let dup = c == '>' && chars.peek() == Some(&'&');
                if dup {
                    chars.next();
                }
                b.pending = Some(PendingRedirect { dup });
            }
            ';' | '&' | '|' | '\n' | '(' | ')' | '`' => {
                b.end_command();
                let doubled = (c == '&' || c == '|') && chars.peek() == Some(&c);
                if doubled {
                    chars.next();
                }
                b.piped = c == '|' && !doubled;
                if b.piped && chars.peek() == Some(&'&') {
                    chars.next();
                }
            }
            c if c.is_whitespace() => b.end_word(),
            c => {
                b.in_word = true;
                b.word.push(c);
            }
        }
    }
    b.end_command();
    b.commands
}

/// Flags well-known destructive, disruptive or privileged operations in `command` by syntax
/// alone, including inside `sudo`, `xargs`, `sh -c '...'` and `ssh host '...'`. Approximate by
/// design: see the module docs for why it exists, and [`split_simple_commands`] for its limits.
pub(crate) fn local_risk_flags(command: &str) -> Vec<LocalRiskFlag> {
    let mut flags = Vec::new();
    collect_flags(command, None, 0, &mut flags);
    flags
}

fn collect_flags(
    line: &str,
    remote_host: Option<&str>,
    depth: usize,
    flags: &mut Vec<LocalRiskFlag>,
) {
    for simple in split_simple_commands(line) {
        collect_simple_command_flags(&simple, remote_host, depth, flags);
    }
}

fn push_flag(
    flags: &mut Vec<LocalRiskFlag>,
    category: LocalRiskCategory,
    trigger: String,
    remote_host: Option<&str>,
) {
    let flag = LocalRiskFlag {
        category,
        trigger,
        remote_host: remote_host.map(str::to_owned),
    };
    if !flags.contains(&flag) {
        flags.push(flag);
    }
}

/// Wrappers that run the rest of their arguments as a command.
const PREFIX_COMMANDS: &[&str] = &[
    "sudo", "doas", "pkexec", "env", "nohup", "time", "exec", "command", "nice", "ionice", "xargs",
    "timeout", "stdbuf", "builtin",
];
const PRIVILEGE_PREFIXES: &[&str] = &["sudo", "doas", "pkexec"];
const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "fish"];
/// `ssh` options that take a value, so the value is not mistaken for the host.
const SSH_OPTIONS_WITH_VALUE: &str = "BbcDEeFIiJLlmOoPpQRSWw";

fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// Whether `args` contains a short flag cluster with `letter` (`-rf` has `r`) or `long`.
fn has_flag(args: &[String], letter: char, long: &str) -> bool {
    args.iter()
        .any(|arg| arg == long || short_cluster_has(arg, letter))
}

/// Whether `arg` is a short flag cluster (`-rf`) containing `letter`.
fn short_cluster_has(arg: &str, letter: char) -> bool {
    arg.starts_with('-') && !arg.starts_with("--") && arg[1..].contains(letter)
}

/// Global options of `tool` that take their value as a separate argument, so the value is not
/// mistaken for a subcommand (`kubectl -n prod delete`, `git -C repo reset`).
fn value_options(tool: &str) -> &'static [&'static str] {
    match tool {
        "git" => &["-C", "-c", "--git-dir", "--work-tree", "--namespace"],
        "kubectl" | "oc" | "helm" => &[
            "-n",
            "--namespace",
            "--context",
            "--kube-context",
            "--cluster",
            "--user",
            "--kubeconfig",
            "-s",
            "--server",
            "--as",
            "--token",
        ],
        "docker" | "podman" | "nerdctl" | "docker-compose" => &[
            "-H",
            "--host",
            "--context",
            "-f",
            "--file",
            "-p",
            "--project-name",
            "--profile",
            "--env-file",
        ],
        _ => &[],
    }
}

/// Indices into `args` of the arguments that are neither options nor the value of one of
/// `tool`'s [`value_options`].
fn positional_indices(tool: &str, args: &[String]) -> Vec<usize> {
    let with_value = value_options(tool);
    let mut indices = Vec::new();
    let mut skip_next = false;
    for (index, arg) in args.iter().enumerate() {
        if std::mem::take(&mut skip_next) {
            continue;
        }
        if arg.starts_with('-') {
            skip_next = with_value.contains(&arg.as_str());
        } else {
            indices.push(index);
        }
    }
    indices
}

/// Options of `docker exec` / `kubectl exec` that take a separate value.
const EXEC_OPTIONS_WITH_VALUE: &[&str] = &[
    "-e",
    "--env",
    "-u",
    "--user",
    "-w",
    "--workdir",
    "--env-file",
    "--detach-keys",
    "-c",
    "--container",
];

/// The command `docker exec [opts] <container> <cmd...>` runs, given the args after `exec`.
fn docker_exec_command(after_exec: &[String]) -> &[String] {
    let mut rest = after_exec;
    let mut container_seen = false;
    while let Some((arg, tail)) = rest.split_first() {
        if arg == "--" {
            rest = tail;
            continue;
        }
        if arg.starts_with('-') {
            let consumed = if EXEC_OPTIONS_WITH_VALUE.contains(&arg.as_str()) {
                2
            } else {
                1
            };
            rest = &rest[consumed.min(rest.len())..];
        } else if !container_seen {
            container_seen = true;
            rest = tail;
        } else {
            return rest;
        }
    }
    &[]
}

/// The script argument of `sh -c '...'`, with `-c` anywhere in a short flag cluster
/// (`bash -lc`, `-ec`).
fn shell_c_script(args: &[String]) -> Option<&str> {
    let index = args.iter().position(|arg| short_cluster_has(arg, 'c'))?;
    args.get(index + 1).map(String::as_str)
}

const PACKAGE_MANAGERS: &[&str] = &[
    "apt", "apt-get", "dnf", "yum", "pacman", "brew", "pip", "pip3", "npm", "pnpm", "yarn", "cargo",
];

/// Whether `args` has an operand that is neither an option nor `/dev/null`.
fn has_non_null_operand(args: &[String]) -> bool {
    args.iter()
        .any(|arg| !arg.starts_with('-') && arg != "/dev/null")
}

fn collect_simple_command_flags(
    simple: &SimpleCommand,
    remote_host: Option<&str>,
    depth: usize,
    flags: &mut Vec<LocalRiskFlag>,
) {
    for target in &simple.redirects {
        push_flag(
            flags,
            LocalRiskCategory::WritesFiles,
            format!("> {target}"),
            remote_host,
        );
    }
    let mut words = simple.words.as_slice();
    // Skip assignments and wrappers to reach the command that actually runs.
    loop {
        let Some(first) = words.first() else {
            return;
        };
        if is_assignment(first) {
            words = &words[1..];
            continue;
        }
        let name = basename(first);
        if !PREFIX_COMMANDS.contains(&name) {
            break;
        }
        if PRIVILEGE_PREFIXES.contains(&name) {
            push_flag(
                flags,
                LocalRiskCategory::PrivilegeOrPermissions,
                name.to_owned(),
                remote_host,
            );
        }
        words = &words[1..];
        while let Some(option) = words.first().filter(|word| word.starts_with('-')) {
            let takes_value = match name {
                "sudo" | "doas" => matches!(
                    option.as_str(),
                    "-u" | "-g" | "-U" | "-C" | "-D" | "-h" | "-p"
                ),
                "nice" => option == "-n",
                "ionice" => matches!(option.as_str(), "-c" | "-n" | "-p"),
                "xargs" => matches!(
                    option.as_str(),
                    "-n" | "-I" | "-P" | "-L" | "-d" | "-E" | "-s" | "-a"
                ),
                "env" => matches!(option.as_str(), "-u" | "-C" | "-S"),
                "timeout" => matches!(option.as_str(), "-s" | "-k"),
                _ => false,
            };
            let consumed = if takes_value { 2 } else { 1 };
            words = &words[consumed.min(words.len())..];
        }
        if name == "timeout" && !words.is_empty() {
            words = &words[1..];
        }
    }

    let Some((first, args)) = words.split_first() else {
        return;
    };
    let name = basename(first);
    let positions = positional_indices(name, args);
    let sub = positions.first().map(|&index| args[index].as_str());
    let sub_trigger = |sub: &str| format!("{name} {sub}");

    match name {
        "rm" | "shred" | "unlink" => push_flag(
            flags,
            LocalRiskCategory::DeletesData,
            name.to_owned(),
            remote_host,
        ),
        "find"
            if args
                .iter()
                .any(|arg| arg == "-delete" || basename(arg) == "rm") =>
        {
            push_flag(
                flags,
                LocalRiskCategory::DeletesData,
                "find -delete".to_owned(),
                remote_host,
            )
        }
        "dd" | "wipefs" | "truncate" | "fdisk" | "sfdisk" | "parted" | "sgdisk" => push_flag(
            flags,
            LocalRiskCategory::OverwritesData,
            name.to_owned(),
            remote_host,
        ),
        name if name.starts_with("mkfs") => push_flag(
            flags,
            LocalRiskCategory::OverwritesData,
            name.to_owned(),
            remote_host,
        ),
        "git" => match sub {
            Some("push")
                if args.iter().any(|arg| {
                    matches!(
                        arg.as_str(),
                        "-f" | "--force" | "--delete" | "-d" | "--mirror" | "--prune"
                    ) || arg.starts_with("--force-with-lease")
                        || (arg.starts_with('+') && arg.len() > 1)
                }) =>
            {
                push_flag(
                    flags,
                    LocalRiskCategory::RewritesHistory,
                    "git push --force".to_owned(),
                    remote_host,
                )
            }
            Some("reset") if args.iter().any(|arg| arg == "--hard") => push_flag(
                flags,
                LocalRiskCategory::RewritesHistory,
                "git reset --hard".to_owned(),
                remote_host,
            ),
            Some("push") if args.iter().any(|arg| arg.starts_with(':') && arg.len() > 1) => {
                push_flag(
                    flags,
                    LocalRiskCategory::RewritesHistory,
                    "git push :branch".to_owned(),
                    remote_host,
                )
            }
            Some("checkout") if args.iter().any(|arg| arg == ".") => push_flag(
                flags,
                LocalRiskCategory::OverwritesData,
                "git checkout .".to_owned(),
                remote_host,
            ),
            Some("stash")
                if args
                    .iter()
                    .any(|arg| matches!(arg.as_str(), "drop" | "clear")) =>
            {
                push_flag(
                    flags,
                    LocalRiskCategory::DeletesData,
                    "git stash drop".to_owned(),
                    remote_host,
                )
            }
            Some("clean") => push_flag(
                flags,
                LocalRiskCategory::DeletesData,
                "git clean".to_owned(),
                remote_host,
            ),
            Some("branch")
                if has_flag(args, 'D', "--delete") || has_flag(args, 'd', "--delete") =>
            {
                push_flag(
                    flags,
                    LocalRiskCategory::RewritesHistory,
                    "git branch --delete".to_owned(),
                    remote_host,
                )
            }
            _ => {}
        },
        "chmod" | "chown" | "chgrp"
            if has_flag(args, 'R', "--recursive") || args.iter().any(|arg| arg == "777") =>
        {
            push_flag(
                flags,
                LocalRiskCategory::PrivilegeOrPermissions,
                format!("{name} -R"),
                remote_host,
            )
        }
        "su" | "passwd" | "visudo" | "useradd" | "usermod" | "userdel" => {
            push_flag(
                flags,
                LocalRiskCategory::PrivilegeOrPermissions,
                name.to_owned(),
                remote_host,
            );
            if name == "su" && let Some(inner) = shell_c_script(args) && depth < MAX_NESTING {
                collect_flags(inner, remote_host, depth + 1, flags);
            }
        }
        "eval" => {
            if depth < MAX_NESTING && !args.is_empty() {
                collect_flags(&args.join(" "), remote_host, depth + 1, flags);
            }
        }
        "tee" if has_non_null_operand(args) && !has_flag(args, 'a', "--append") => push_flag(
            flags,
            LocalRiskCategory::WritesFiles,
            "tee".to_owned(),
            remote_host,
        ),
        "sed"
            if args
                .iter()
                .any(|arg| arg.starts_with("--in-place") || short_cluster_has(arg, 'i')) =>
        {
            push_flag(
                flags,
                LocalRiskCategory::WritesFiles,
                "sed -i".to_owned(),
                remote_host,
            )
        }
        "cp" | "mv" => push_flag(
            flags,
            LocalRiskCategory::WritesFiles,
            name.to_owned(),
            remote_host,
        ),
        "rsync" if args.iter().any(|arg| arg.starts_with("--delete")) => push_flag(
            flags,
            LocalRiskCategory::DeletesData,
            "rsync --delete".to_owned(),
            remote_host,
        ),
        "crontab" if args.iter().any(|arg| short_cluster_has(arg, 'r')) => push_flag(
            flags,
            LocalRiskCategory::DeletesData,
            "crontab -r".to_owned(),
            remote_host,
        ),
        name if PACKAGE_MANAGERS.contains(&name) => {
            let removal = positions
                .iter()
                .take(2)
                .map(|&i| args[i].as_str())
                .any(|word| matches!(word, "remove" | "purge" | "uninstall" | "autoremove" | "rm"))
                || (name == "pacman" && args.iter().any(|arg| arg.starts_with("-R")));
            if removal {
                push_flag(
                    flags,
                    LocalRiskCategory::RemovesPackages,
                    format!("{name} remove"),
                    remote_host,
                );
            }
        }
        name if SHELLS.contains(&name) => {
            if let Some(inner) = shell_c_script(args) {
                if depth < MAX_NESTING {
                    collect_flags(inner, remote_host, depth + 1, flags);
                }
            } else if simple.piped {
                push_flag(
                    flags,
                    LocalRiskCategory::RunsRemoteCode,
                    format!("| {name}"),
                    remote_host,
                );
            }
        }
        "systemctl" => match sub {
            Some(
                sub @ ("stop" | "disable" | "restart" | "try-restart" | "reload-or-restart"
                | "mask" | "kill" | "isolate"),
            ) => push_flag(
                flags,
                LocalRiskCategory::StopsServices,
                sub_trigger(sub),
                remote_host,
            ),
            Some(
                sub @ ("reboot" | "poweroff" | "halt" | "kexec" | "suspend" | "hibernate"
                | "rescue" | "emergency" | "soft-reboot"),
            ) => push_flag(
                flags,
                LocalRiskCategory::RebootsOrShutsDown,
                sub_trigger(sub),
                remote_host,
            ),
            _ => {}
        },
        "service" | "rc-service" => {
            if let Some(action) = args
                .iter()
                .map(String::as_str)
                .find(|arg| matches!(*arg, "stop" | "restart" | "force-reload" | "zap"))
            {
                push_flag(
                    flags,
                    LocalRiskCategory::StopsServices,
                    format!("{name} {action}"),
                    remote_host,
                )
            }
        }
        "launchctl" => {
            if let Some(sub @ ("stop" | "unload" | "bootout" | "kill" | "disable")) = sub {
                push_flag(
                    flags,
                    LocalRiskCategory::StopsServices,
                    sub_trigger(sub),
                    remote_host,
                )
            }
        }
        "kill" | "pkill" | "killall" | "killall5" => push_flag(
            flags,
            LocalRiskCategory::KillsProcesses,
            name.to_owned(),
            remote_host,
        ),
        "docker" | "podman" | "docker-compose" | "nerdctl" => {
            let positional: Vec<&str> = positions
                .iter()
                .take(2)
                .map(|&index| args[index].as_str())
                .collect();
            if depth < MAX_NESTING {
                // `docker exec c cmd` / `docker compose exec svc cmd` run `cmd`.
                if let Some(exec) = positions.iter().take(2).find(|&&i| args[i] == "exec") {
                    let command = docker_exec_command(&args[exec + 1..]);
                    if !command.is_empty() {
                        collect_simple_command_flags(
                            &SimpleCommand {
                                words: command.to_vec(),
                                piped: false,
                                redirects: Vec::new(),
                            },
                            remote_host,
                            depth + 1,
                            flags,
                        );
                    }
                }
            }
            let category = positional.iter().find_map(|word| match *word {
                "stop" | "kill" | "restart" | "pause" | "down" => {
                    Some(LocalRiskCategory::StopsServices)
                }
                "rm" | "rmi" | "prune" => Some(LocalRiskCategory::DeletesData),
                _ => None,
            });
            if let Some(category) = category {
                push_flag(
                    flags,
                    category,
                    format!("{name} {}", positional.join(" ")),
                    remote_host,
                );
            }
        }
        "kubectl" | "oc" => match sub {
            Some("exec") if depth < MAX_NESTING => {
                if let Some(dashes) = args.iter().position(|arg| arg == "--") {
                    let command = &args[dashes + 1..];
                    if !command.is_empty() {
                        collect_simple_command_flags(
                            &SimpleCommand {
                                words: command.to_vec(),
                                piped: false,
                                redirects: Vec::new(),
                            },
                            remote_host,
                            depth + 1,
                            flags,
                        );
                    }
                }
            }
            Some(sub @ ("delete" | "drain" | "cordon")) => push_flag(
                flags,
                LocalRiskCategory::StopsServices,
                sub_trigger(sub),
                remote_host,
            ),
            Some("scale")
                if args.iter().any(|arg| arg == "--replicas=0")
                    || args
                        .windows(2)
                        .any(|pair| pair[0] == "--replicas" && pair[1] == "0") =>
            {
                push_flag(
                    flags,
                    LocalRiskCategory::StopsServices,
                    format!("{name} scale --replicas=0"),
                    remote_host,
                )
            }
            Some("rollout") if args.iter().any(|arg| arg == "restart") => push_flag(
                flags,
                LocalRiskCategory::StopsServices,
                format!("{name} rollout restart"),
                remote_host,
            ),
            _ => {}
        },
        "helm" => {
            if let Some(sub @ ("uninstall" | "delete")) = sub {
                push_flag(
                    flags,
                    LocalRiskCategory::StopsServices,
                    sub_trigger(sub),
                    remote_host,
                )
            }
        }
        "reboot" | "shutdown" | "poweroff" | "halt" => push_flag(
            flags,
            LocalRiskCategory::RebootsOrShutsDown,
            name.to_owned(),
            remote_host,
        ),
        "init" | "telinit" => {
            if let Some(level @ ("0" | "6")) = sub {
                push_flag(
                    flags,
                    LocalRiskCategory::RebootsOrShutsDown,
                    format!("{name} {level}"),
                    remote_host,
                )
            }
        }
        "ssh" => {
            let mut rest = args;
            let mut host = None;
            while let Some((arg, tail)) = rest.split_first() {
                rest = tail;
                if arg == "--" {
                    continue;
                }
                if let Some(option) = arg.strip_prefix('-') {
                    let takes_value = option.len() == 1
                        && option
                            .chars()
                            .next()
                            .is_some_and(|c| SSH_OPTIONS_WITH_VALUE.contains(c));
                    if takes_value {
                        rest = rest.get(1..).unwrap_or_default();
                    }
                    continue;
                }
                host = Some(arg.as_str());
                break;
            }
            if let Some(host) = host && !rest.is_empty() && depth < MAX_NESTING {
                collect_flags(&rest.join(" "), Some(host), depth + 1, flags);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "command_explanation_tests.rs"]
mod tests;
