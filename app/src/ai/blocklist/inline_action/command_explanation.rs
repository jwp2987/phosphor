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
const MAX_USER_CHARS: usize = MAX_COMMAND_CHARS + 1_000;

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
            LocalRiskCategory::PrivilegeOrPermissions | LocalRiskCategory::KillsProcesses => {
                RiskLevel::Caution
            }
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
        return format!(
            "Explain this command the agent wants to run.\n<command>\n{command}\n</command>"
        );
    }
    let shown: String = command.chars().take(MAX_COMMAND_CHARS).collect();
    format!(
        "Explain this command the agent wants to run. It is {total_chars} characters long; only \
         the first {MAX_COMMAND_CHARS} are shown, and the rest is hidden from you.\n\
         <command truncated=\"true\">\n{shown}\n</command>"
    )
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
    Some(CommandExplanation {
        model_risk,
        risks,
        explanation,
        local_flags: local_risk_flags(command),
    })
}

fn parse_risk_level(value: &str) -> Option<RiskLevel> {
    let value = value.to_ascii_uppercase();
    // Most severe first, so a hedged "SAFE, but DESTRUCTIVE if ..." reads as destructive.
    if value.contains("DESTRUCTIVE") {
        Some(RiskLevel::Destructive)
    } else if value.contains("CAUTION") {
        Some(RiskLevel::Caution)
    } else if value.contains("SAFE") {
        Some(RiskLevel::Safe)
    } else {
        None
    }
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
fn strip_reasoning_blocks(raw: &str) -> String {
    let mut out = raw.to_owned();
    for tag in ["think", "reasoning", "thought", "scratchpad"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let (Some(start), Some(end)) = (
            out.find(&open),
            out.find(&close).map(|idx| idx + close.len()),
        ) {
            if end <= start {
                break;
            }
            out.replace_range(start..end, "");
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
}

/// Splits a command line into simple commands at `;`, `&`, `|`, `&&`, `||`, newlines and
/// parentheses, unquoting words. Deliberately approximate -- no expansions, heredocs or
/// redirection parsing -- because it only feeds [`local_risk_flags`], which errs towards
/// flagging.
fn split_simple_commands(line: &str) -> Vec<SimpleCommand> {
    let mut commands = Vec::new();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut piped = false;
    let mut chars = line.chars().peekable();

    fn end_word(word: &mut String, in_word: &mut bool, words: &mut Vec<String>) {
        if *in_word {
            words.push(std::mem::take(word));
            *in_word = false;
        }
    }

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                word.push(escaped);
                            }
                        }
                        c => word.push(c),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(escaped) = chars.next() {
                    if escaped != '\n' {
                        word.push(escaped);
                    }
                }
            }
            ';' | '&' | '|' | '\n' | '(' | ')' => {
                end_word(&mut word, &mut in_word, &mut words);
                if !words.is_empty() {
                    commands.push(SimpleCommand {
                        words: std::mem::take(&mut words),
                        piped,
                    });
                }
                let doubled = (c == '&' || c == '|') && chars.peek() == Some(&c);
                if doubled {
                    chars.next();
                }
                piped = c == '|' && !doubled;
                if piped && chars.peek() == Some(&'&') {
                    chars.next();
                }
            }
            c if c.is_whitespace() => end_word(&mut word, &mut in_word, &mut words),
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    end_word(&mut word, &mut in_word, &mut words);
    if !words.is_empty() {
        commands.push(SimpleCommand { words, piped });
    }
    commands
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
    args.iter().any(|arg| {
        arg == long || (arg.starts_with('-') && !arg.starts_with("--") && arg[1..].contains(letter))
    })
}

/// The first argument that is not an option.
fn subcommand(args: &[String]) -> Option<&str> {
    args.iter()
        .map(String::as_str)
        .find(|arg| !arg.starts_with('-'))
}

fn collect_simple_command_flags(
    simple: &SimpleCommand,
    remote_host: Option<&str>,
    depth: usize,
    flags: &mut Vec<LocalRiskFlag>,
) {
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
            let takes_value = matches!(name, "sudo" | "doas")
                && matches!(
                    option.as_str(),
                    "-u" | "-g" | "-U" | "-C" | "-D" | "-h" | "-p"
                );
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
    let sub = subcommand(args);
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
            if name == "su" {
                if let Some(index) = args.iter().position(|arg| arg == "-c") {
                    if let Some(inner) = args.get(index + 1) {
                        if depth < MAX_NESTING {
                            collect_flags(inner, remote_host, depth + 1, flags);
                        }
                    }
                }
            }
        }
        name if SHELLS.contains(&name) => {
            if let Some(index) = args.iter().position(|arg| arg == "-c") {
                if let Some(inner) = args.get(index + 1) {
                    if depth < MAX_NESTING {
                        collect_flags(inner, remote_host, depth + 1, flags);
                    }
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
            let positional: Vec<&str> = args
                .iter()
                .map(String::as_str)
                .filter(|arg| !arg.starts_with('-'))
                .take(2)
                .collect();
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
            if let Some(host) = host {
                if !rest.is_empty() && depth < MAX_NESTING {
                    collect_flags(&rest.join(" "), Some(host), depth + 1, flags);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "command_explanation_tests.rs"]
mod tests;
