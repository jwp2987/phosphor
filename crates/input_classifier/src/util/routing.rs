//! The one decision procedure for command-vs-agent routing (#1234).
//!
//! [`route`] takes the buffer, the completer's parse of it and the facts the app has at the moment
//! it asks (which surface the line is typed on, the shell it is typed into, what the session knows
//! about the first word, whether the exact line ran before, typeahead provenance, the user's
//! denylist) and returns the route with the rule that decided it. It is synchronous, allocates
//! nothing beyond the word split the predicates it reuses already make, and runs at the keystroke
//! and at Enter alike, so an Enter never waits for an asynchronous verdict when the answer is
//! knowable from what is already there. `Route::Undecided` is the one case the classifier is
//! consulted for: a line whose head is no command the session knows, is not English and carries
//! no prose signal, typed outside a conversation; inside an agent conversation nothing is left
//! undecided, because there the safe default is the agent (#1183).
//!
//! The rules are ordered so that each one only sees lines the earlier ones left: shell structure
//! and command shape first (#808), then the request shapes (#834, #897, #964, #965), then what is
//! known about the head. Precision runs both ways: a `Shell` route executes the line, an `Agent`
//! route sends it off the machine, and both are one click away from the other through the
//! misroute recovery (#835), so the rule set prefers the agent when a line reads as English and
//! the shell when its first word is a command the session knows and nothing in it reads as a
//! sentence (owner decision #964). `PHOSPHOR_ROUTING_V1=1` keeps the previous rule set
//! (`BlocklistAIInputModel` reads it; this module does not).

use natural_language_detection::is_ordinary_english_word;
use warp_completer::ParsedTokensSnapshot;

use super::{
    AMBIGUOUS_LONE_HEADS, FREE_TEXT_ARG_HEADS, MAX_PREFILTER_BYTES, QUESTION_HEADS,
    REQUEST_OPENER_HEADS, SHELL_WORD_TERMINATORS, STRONG_PROSE_WORDS, ShellWord, WRAPPER_HEADS,
    buffer_has_prose_colon_shape, effective_head_index, ends_in_sentence_punctuation,
    has_no_prose_evidence, has_prose_veto_word, is_agent_follow_up_input, is_data_shaped_word,
    is_env_assignment_token, is_explicit_path_token, is_function_word, is_known_shell_command_word,
    is_misplaced_re_command_word, is_non_english_shell_name, is_one_off_natural_language_word,
    is_one_off_shell_command_keyword, is_prose_article, is_prose_word,
    is_unambiguous_shell_command, lacks_command_evidence_in_agent_conversation,
    lone_word_belongs_to_agent, reads_as_prose_with_unknown_head, reads_as_request_to_agent,
    split_shell_words, starts_with_slash_command_not_path, strip_sentence_punctuation,
    strip_trailing_comment, takes_free_text, word_basename,
};

/// The syntax family of the shell the line is typed into. bash and zsh are one family here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShellFamily {
    #[default]
    Unknown,
    Posix,
    Fish,
    PowerShell,
    Nu,
}

/// Where the line is typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The terminal with no agent conversation open.
    PlainTerminal,
    /// The input of an active agent conversation: the conversation is the context, so a shell
    /// route needs positive evidence that the line is a command (#1183).
    AgentConversation,
}

/// What the app knows about where the buffer's text came from (#886, #933).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Typeahead {
    /// Typed in the editor.
    #[default]
    None,
    /// Typeahead that began right after a command, before the shell was known to be at its
    /// prompt: classified like anything typed (#933).
    Young,
    /// Typeahead for a running command, handed back by the shell: the shell's unless it reads as
    /// prose, and never sent to the agent by these rules (#886).
    Running,
}

/// Everything [`route`] knows besides the buffer and its parse.
#[derive(Clone, Copy, Debug)]
pub struct RoutingContext {
    pub surface: Surface,
    pub shell: ShellFamily,
    /// The session's own answer to "is the first word a command here" (an alias, function,
    /// builtin, keyword or `$PATH` executable): `None` when it cannot say (#833), and the caller
    /// must pass `None` rather than guess.
    pub head_known_to_shell: Option<bool>,
    /// Whether the completion context the parse ran against had a complete view of top-level
    /// commands, so a first word it does not describe is really absent (#696).
    pub commands_fully_loaded: bool,
    /// Whether no command that could have changed the command set has run since it was captured
    /// (#833, #935).
    pub command_data_current: bool,
    /// Whether the line follows an agent block in the conversation.
    pub is_agent_follow_up: bool,
    /// Whether this exact line ran in the session's shell before, as its history records it
    /// (#1203; the caller applies the history's own exclusions).
    pub ran_before_exactly: bool,
    /// Whether the first word is in the user's autodetection denylist.
    pub denylisted: bool,
    pub typeahead: Typeahead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Shell,
    Agent,
    /// No rule decides; the classifier does, and until its verdict lands the live input type
    /// stands.
    Undecided,
}

/// The rule that decided a route, in the order [`route`] tries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Nothing to route.
    Empty,
    /// Over the size cap: a paste, never classified, never moved (#968, #1036, #1174).
    Oversized,
    /// The first word is in the user's autodetection denylist.
    Denylist,
    /// Typeahead for a running command with no prose evidence anywhere (#886).
    RunningTypeahead,
    /// `/word` is a slash-command spelling, never a path to run (#1184).
    SlashSpelling,
    /// One bare word the classifier reserves for the agent: a one-off natural-language word, a
    /// request opener or an ambiguous head such as `help`, or English with no command evidence
    /// (#696, #935, #1185, #1203).
    LoneWord,
    /// `yes`, `continue`, `do it`, `approve` after an agent block.
    FollowUpWord,
    /// Unambiguous shell syntax: an operator used as a shell uses it, a command-shaped body, or a
    /// known CLI with its own subcommand (#808, #1203).
    CommandShape,
    /// A wrapper (`nice`, `nohup`, `sudo`, `time`...) in front of a known command with a
    /// command-shaped body.
    WrapperChain,
    /// Syntax of the shell the line is typed into: a PowerShell cmdlet or `$variable` assignment,
    /// a nushell keyword or pipeline, a fish builtin with its subcommand.
    ShellSyntax,
    /// The line reads as a request to the agent (#834, #897, #964, #965).
    RequestShape,
    /// A command-headed line that reads as a sentence: a word addressed to someone (`me`, `my`,
    /// `please`, `when`...) plus another prose signal, with no shell data among the arguments.
    SentenceShape,
    /// A one-off natural-language word opening the line (`yes please`, `nice work`) (#833).
    OneOffWord,
    /// The exact line ran in this shell before (#1203).
    RanBefore,
    /// In a conversation, no positive evidence that the line is a command (#1183).
    NoEvidenceInConversation,
    /// In a conversation, a head the session does not know and no shell data around it (#935).
    UnknownHeadInConversation,
    /// In a conversation, evidence of a command and nothing that reads as English (#1183).
    EvidenceInConversation,
    /// A question or request opener heads a line whose first word is no command here.
    QuestionHead,
    /// The first word is a command the session knows and nothing reads as a request (#906, owner
    /// decision #964).
    InstalledHead,
    /// An English word (or a shell's name) with no command evidence opens the line (#696, #965,
    /// #1018).
    EnglishHeadNotInstalled,
    /// Clear prose headed by a word the session's shell does not know (#833).
    ProseWithUnknownHead,
    /// A head the session knows is absent, with a sentence word among the arguments (#1019).
    AbsentHeadWithProse,
    /// Nothing above applies: the classifier decides.
    Model,
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Rule::Empty => "empty",
            Rule::Oversized => "oversized (#1036)",
            Rule::Denylist => "denylist",
            Rule::RunningTypeahead => "running-typeahead (#886)",
            Rule::SlashSpelling => "slash-spelling (#1184)",
            Rule::LoneWord => "lone-word (#935/#1185)",
            Rule::FollowUpWord => "follow-up-word",
            Rule::CommandShape => "command-shape (#808/#1203)",
            Rule::WrapperChain => "wrapper-chain",
            Rule::ShellSyntax => "shell-syntax (#1234)",
            Rule::RequestShape => "request-shape (#834/#965)",
            Rule::SentenceShape => "sentence-shape (#1234)",
            Rule::OneOffWord => "one-off-word (#833)",
            Rule::RanBefore => "ran-before (#1203)",
            Rule::NoEvidenceInConversation => "no-evidence-in-conversation (#1183)",
            Rule::UnknownHeadInConversation => "unknown-head-in-conversation (#935)",
            Rule::EvidenceInConversation => "evidence-in-conversation (#1183)",
            Rule::QuestionHead => "question-head (#1234)",
            Rule::InstalledHead => "installed-head (#906/#964)",
            Rule::EnglishHeadNotInstalled => "english-head-not-installed (#696)",
            Rule::ProseWithUnknownHead => "prose-with-unknown-head (#833)",
            Rule::AbsentHeadWithProse => "absent-head-with-prose (#1019)",
            Rule::Model => "model",
        };
        f.write_str(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub route: Route,
    pub rule: Rule,
}

impl Decision {
    const fn shell(rule: Rule) -> Self {
        Self {
            route: Route::Shell,
            rule,
        }
    }

    const fn agent(rule: Rule) -> Self {
        Self {
            route: Route::Agent,
            rule,
        }
    }

    const fn undecided(rule: Rule) -> Self {
        Self {
            route: Route::Undecided,
            rule,
        }
    }
}

/// Decides where `input` goes. See the module documentation for the order and the reasoning.
pub fn route(input: &ParsedTokensSnapshot, ctx: &RoutingContext) -> Decision {
    let raw = input.buffer_text.trim();
    if raw.is_empty() {
        return Decision::undecided(Rule::Empty);
    }
    if raw.len() > MAX_PREFILTER_BYTES {
        return Decision::undecided(Rule::Oversized);
    }
    if ctx.denylisted {
        return Decision::shell(Rule::Denylist);
    }
    // #1185: a trailing comment is not part of the command.
    let stripped = strip_trailing_comment(input);
    let input: &ParsedTokensSnapshot = &stripped;
    let text = input.buffer_text.trim();

    let running_typeahead = ctx.typeahead == Typeahead::Running;
    if running_typeahead && has_no_prose_evidence(text) {
        return Decision::shell(Rule::RunningTypeahead);
    }
    // Typeahead for a running command is never routed to the agent by a rule: it was typed at the
    // shell, and only the classifier may read it as a question (#886).
    let agentward = !running_typeahead;
    if agentward && starts_with_slash_command_not_path(text) {
        return Decision::agent(Rule::SlashSpelling);
    }

    let in_conversation = ctx.surface == Surface::AgentConversation;
    let words = split_shell_words(text);
    let first_index = words
        .iter()
        .take_while(|word| !word.quoted && is_env_assignment_token(&word.text))
        .count();
    let Some(first) = words.get(first_index) else {
        // Only assignments (`FOO=bar`): valid shell, but also what "x=5" in a question looks like.
        return Decision::undecided(Rule::Model);
    };
    if first.quoted {
        return Decision::undecided(Rule::Model);
    }
    let head = first
        .text
        .split(|c: char| SHELL_WORD_TERMINATORS.contains(&c))
        .next()
        .unwrap_or("");
    if head.is_empty() {
        return Decision::undecided(Rule::Model);
    }
    let head_lower = head.to_lowercase();
    let args = &words[first_index + 1..];
    let lone = words.len() == 1;
    let multi_line = text.contains(['\n', '\r']);

    let head_described = head_token_described(input);
    // The session's statement counts as a description: at Enter there is often no parse yet, and
    // the session is asked instead (#1185: `write` lacked evidence only because no parse had
    // described it).
    let installed = head_described || ctx.head_known_to_shell == Some(true);
    let head_is_curated = is_known_shell_command_word(head);
    let head_is_keyword = is_one_off_shell_command_keyword(head);
    let head_is_path = is_explicit_path_token(head);
    // Known absent: the session says so, or a complete and current command index does not
    // describe it (#696). `None` from the session with no index to fall back on is "cannot say".
    let known_absent = !head_described
        && (ctx.head_known_to_shell == Some(false)
            || (ctx.head_known_to_shell.is_none()
                && ctx.commands_fully_loaded
                && ctx.command_data_current));

    if agentward
        && in_conversation
        && ctx.is_agent_follow_up
        && is_agent_follow_up_input(&text.to_lowercase())
    {
        return Decision::agent(Rule::FollowUpWord);
    }
    // One bare word that reads as English rather than as a command (`help`, `where`, `bar`): the
    // agent's unless the session knows it as a command and it is not an everyday request opener.
    if agentward
        && lone
        && lone_word_belongs_to_agent(text)
        && (known_absent || is_ambiguous_lone_head(&head_lower))
    {
        return Decision::agent(Rule::LoneWord);
    }

    let no_evidence = in_conversation
        && conversation_lacks_evidence(
            input,
            &head_lower,
            lone,
            installed,
            head_is_curated || head_is_keyword || head_is_path,
        );

    // Shell shape: structure, a command-shaped body, the shell's own syntax.
    if !no_evidence && is_unambiguous_shell_command(input) {
        return Decision::shell(Rule::CommandShape);
    }
    if !no_evidence && wrapper_chain_is_command(&words, first_index) {
        return Decision::shell(Rule::WrapperChain);
    }
    if !no_evidence && has_shell_specific_command_shape(&words, first_index, ctx.shell) {
        return Decision::shell(Rule::ShellSyntax);
    }

    // Request shape: the line reads as English addressed to the agent.
    if agentward && reads_as_request_to_agent(text) {
        return Decision::agent(Rule::RequestShape);
    }
    if agentward && reads_as_sentence_with_command_head(text, &words, first_index) {
        return Decision::agent(Rule::SentenceShape);
    }
    if agentward && !lone && opens_with_one_off_natural_language_word(&head_lower, args, text) {
        return Decision::agent(Rule::OneOffWord);
    }

    if ctx.ran_before_exactly && !no_evidence {
        return Decision::shell(Rule::RanBefore);
    }

    if in_conversation {
        if !agentward {
            return Decision::undecided(Rule::Model);
        }
        if no_evidence {
            return Decision::agent(Rule::NoEvidenceInConversation);
        }
        if !(installed || head_is_curated || head_is_keyword || head_is_path) {
            // Shell data around an unknown head (`mytool --json`) is the classifier's call, as
            // before; bare words around it are a message to the agent.
            if args.iter().any(is_data_shaped_word) || has_shell_structure(text) {
                return Decision::undecided(Rule::Model);
            }
            return Decision::agent(Rule::UnknownHeadInConversation);
        }
        if multi_line {
            return Decision::undecided(Rule::Model);
        }
        return Decision::shell(Rule::EvidenceInConversation);
    }

    // The plain terminal.
    if multi_line || running_typeahead {
        return Decision::undecided(Rule::Model);
    }
    // "Do not guess" (#833): a head the session cannot place keeps the live type until the
    // classifier answers; only a head known to be absent opens a question here.
    if first_index == 0
        && known_absent
        && !head_is_curated
        && words.len() >= 2
        && is_question_or_opener_head(&head_lower)
        && !has_shell_structure(text)
    {
        return Decision::agent(Rule::QuestionHead);
    }
    if installed || head_is_keyword || head_is_path {
        return Decision::shell(Rule::InstalledHead);
    }
    // An assignment prefix is a command by construction (#833), so with one only the #696 colon
    // shape reads the English word after it as prose.
    let english_head = is_ordinary_english_word(head) || is_non_english_shell_name(head);
    if english_head
        && ((known_absent && first_index == 0)
            || (!head_described && buffer_has_prose_colon_shape(text)))
    {
        return Decision::agent(Rule::EnglishHeadNotInstalled);
    }
    if ctx.head_known_to_shell == Some(false) && reads_as_prose_with_unknown_head(text) {
        return Decision::agent(Rule::ProseWithUnknownHead);
    }
    if known_absent
        && first_index == 0
        && !has_shell_structure(text)
        && !args.iter().any(is_data_shaped_word)
        && has_prose_veto_word(head, args)
    {
        return Decision::agent(Rule::AbsentHeadWithProse);
    }
    Decision::undecided(Rule::Model)
}

/// Whether the completer described the effective first token (leading assignments skipped).
fn head_token_described(input: &ParsedTokensSnapshot) -> bool {
    input
        .parsed_tokens
        .iter()
        .find(|token| !is_env_assignment_token(token.token.as_str()))
        .is_some_and(|token| token.token_description.is_some())
}

/// A lone head that is a request as often as a command, whatever the session knows about it
/// (`help`, `do`, `what`, `show`): the one-off natural-language words, the request openers and the
/// #1183 ambiguous heads, minus the words that are commands first (`make`, `test`, `find`).
fn is_ambiguous_lone_head(head_lower: &str) -> bool {
    (is_one_off_natural_language_word(head_lower)
        || REQUEST_OPENER_HEADS.contains(&head_lower)
        || AMBIGUOUS_LONE_HEADS.contains(&head_lower))
        && !is_known_shell_command_word(head_lower)
}

fn is_question_or_opener_head(head_lower: &str) -> bool {
    head_lower.chars().all(|c| c.is_ascii_alphabetic())
        && (QUESTION_HEADS.contains(&head_lower)
            || REQUEST_OPENER_HEADS.contains(&head_lower)
            || head_lower == "please")
}

fn has_shell_structure(text: &str) -> bool {
    text.contains(['|', '&', ';', '<', '>', '(', ')', '`']) || text.contains("$(")
}

/// #1183's evidence test, with the lone-word case decided from what the session knows rather
/// than from the parse alone: a known command word, a path or a described word that is not an
/// everyday request opener is evidence; an unknown or English bare word is none.
fn conversation_lacks_evidence(
    input: &ParsedTokensSnapshot,
    head_lower: &str,
    lone: bool,
    installed: bool,
    head_is_command_shaped: bool,
) -> bool {
    if lone {
        return !(head_is_command_shaped || (installed && !is_ambiguous_lone_head(head_lower)));
    }
    lacks_command_evidence_in_agent_conversation(input)
}

/// `nice true`, `sudo -n true`, `time ls -la`, `xargs echo`: a wrapper ([`WRAPPER_HEADS`]) in
/// front of a known command word or a path, with nothing after that reads as prose.
fn wrapper_chain_is_command(words: &[ShellWord], first_index: usize) -> bool {
    let Some(wrapper) = words.get(first_index) else {
        return false;
    };
    if wrapper.quoted || !WRAPPER_HEADS.contains(&word_basename(wrapper)) {
        return false;
    }
    let Some(index) = effective_head_index(words, first_index) else {
        return false;
    };
    if index == first_index {
        return false;
    }
    let command = &words[index];
    if command.quoted {
        return false;
    }
    let command_text = command
        .text
        .split(|c: char| SHELL_WORD_TERMINATORS.contains(&c))
        .next()
        .unwrap_or("");
    let rest = &words[index + 1..];
    (is_known_shell_command_word(command_text) || is_explicit_path_token(command_text))
        && !has_prose_veto_word(command_text, rest)
        && !rest.iter().any(is_prose_word)
}

/// PowerShell's approved verbs: a `Verb-Noun` head with one of these is a cmdlet, function or
/// alias spelling that no English sentence opens with.
const POWERSHELL_VERBS: &[&str] = &[
    "add",
    "approve",
    "assert",
    "block",
    "clear",
    "close",
    "compare",
    "complete",
    "compress",
    "confirm",
    "connect",
    "convert",
    "convertfrom",
    "convertto",
    "copy",
    "debug",
    "deny",
    "disable",
    "disconnect",
    "dismount",
    "edit",
    "enable",
    "enter",
    "exit",
    "expand",
    "export",
    "find",
    "format",
    "get",
    "grant",
    "group",
    "hide",
    "import",
    "initialize",
    "install",
    "invoke",
    "join",
    "limit",
    "lock",
    "measure",
    "merge",
    "mount",
    "move",
    "new",
    "open",
    "optimize",
    "out",
    "ping",
    "pop",
    "protect",
    "publish",
    "push",
    "read",
    "receive",
    "redo",
    "register",
    "remove",
    "rename",
    "repair",
    "request",
    "reset",
    "resize",
    "resolve",
    "restart",
    "restore",
    "resume",
    "revoke",
    "save",
    "search",
    "select",
    "send",
    "set",
    "show",
    "skip",
    "sort",
    "split",
    "start",
    "step",
    "stop",
    "submit",
    "suspend",
    "switch",
    "sync",
    "test",
    "trace",
    "unblock",
    "undo",
    "uninstall",
    "unlock",
    "unprotect",
    "unpublish",
    "unregister",
    "update",
    "use",
    "wait",
    "watch",
    "where",
    "write",
];

fn is_powershell_cmdlet(word: &str) -> bool {
    let Some((verb, noun)) = word.split_once('-') else {
        return false;
    };
    !noun.is_empty()
        && noun.chars().all(|c| c.is_ascii_alphanumeric())
        && verb.chars().all(|c| c.is_ascii_alphabetic())
        && POWERSHELL_VERBS.contains(&verb.to_ascii_lowercase().as_str())
}

/// A `$name` or `$env:NAME` / `$env.NAME` variable reference (PowerShell, nushell).
fn is_variable_head(word: &str) -> bool {
    word.strip_prefix('$').is_some_and(|name| {
        !name.is_empty()
            && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.'))
    })
}

/// nushell commands that follow a pipe in a data pipeline (`ls | where size > 1kb`).
const NU_PIPELINE_COMMANDS: &[&str] = &[
    "where",
    "get",
    "select",
    "reject",
    "sort-by",
    "group-by",
    "first",
    "last",
    "skip",
    "take",
    "each",
    "par-each",
    "filter",
    "lines",
    "str",
    "into",
    "to",
    "from",
    "length",
    "reverse",
    "uniq",
    "flatten",
    "save",
    "table",
    "print",
    "rename",
    "update",
    "insert",
    "upsert",
    "compact",
    "default",
    "headers",
    "describe",
    "explore",
    "split",
    "parse",
    "append",
    "prepend",
    "math",
    "ls",
    "open",
    "http",
    "find",
    "any",
    "all",
    "columns",
    "values",
    "transpose",
    "wrap",
    "merge",
    "join",
    "zip",
    "enumerate",
    "items",
    "window",
    "chunks",
    "collect",
    "tee",
    "inspect",
    "debug",
];

/// fish's `string` builtin subcommands.
const FISH_STRING_SUBCOMMANDS: &[&str] = &[
    "collect", "escape", "join", "join0", "length", "lower", "match", "pad", "repeat", "replace",
    "shorten", "split", "split0", "sub", "trim", "unescape", "upper",
];

/// fish builtins whose arguments are data (`math 2 + 2`, `count *.rs`, `set -gx FOO bar`).
const FISH_DATA_BUILTINS: &[&str] = &[
    "set",
    "abbr",
    "funced",
    "funcsave",
    "fish_add_path",
    "count",
    "math",
    "argparse",
    "commandline",
    "bind",
    "psub",
    "random",
    "contains",
    "isatty",
    "status",
    "functions",
    "set_color",
    "ulimit",
];

/// Syntax of the shell the line is typed into that the POSIX pre-filter does not know. A
/// PowerShell cmdlet spelling and a `$variable` assignment are recognised for any shell: no
/// English sentence has that shape.
fn has_shell_specific_command_shape(
    words: &[ShellWord],
    first_index: usize,
    shell: ShellFamily,
) -> bool {
    let Some(head) = words.get(first_index) else {
        return false;
    };
    if head.quoted {
        return false;
    }
    let head_text = head.text.as_str();
    let args = &words[first_index + 1..];
    let second = args
        .first()
        .filter(|word| !word.quoted)
        .map(|word| word.text.as_str());
    if is_powershell_cmdlet(head_text) && !has_prose_veto_word(head_text, args) {
        return true;
    }
    if is_variable_head(head_text)
        && second.is_some_and(|second| {
            matches!(second, "=" | "+=" | "-=" | "|" | "??=")
                || second
                    .strip_prefix('-')
                    .is_some_and(|op| !op.is_empty() && op.chars().all(|c| c.is_ascii_alphabetic()))
        })
    {
        return true;
    }
    match shell {
        ShellFamily::Nu => {
            let head_lower = head_text.to_ascii_lowercase();
            let third = args.get(1).map(|word| word.text.as_str());
            if matches!(head_lower.as_str(), "let" | "mut" | "const")
                && third == Some("=")
                && second.is_some_and(|name| {
                    name.chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
                })
            {
                return true;
            }
            if head_lower == "def" && third.is_some_and(|third| third.starts_with('[')) {
                return true;
            }
            if head_lower == "http"
                && second.is_some_and(|verb| {
                    matches!(
                        verb,
                        "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
                    )
                })
            {
                return true;
            }
            words.windows(2).any(|pair| {
                !pair[0].quoted
                    && pair[0].text == "|"
                    && !pair[1].quoted
                    && NU_PIPELINE_COMMANDS.contains(&pair[1].text.as_str())
            })
        }
        ShellFamily::Fish => {
            if matches!(head_text, "and" | "or" | "not")
                && second.is_some_and(|next| {
                    is_known_shell_command_word(next) || is_explicit_path_token(next)
                })
            {
                return true;
            }
            if head_text == "string"
                && second.is_some_and(|sub| FISH_STRING_SUBCOMMANDS.contains(&sub))
            {
                return true;
            }
            FISH_DATA_BUILTINS.contains(&head_text)
                && !args.is_empty()
                && !args.iter().any(is_prose_word)
                && !has_prose_veto_word(head_text, args)
        }
        ShellFamily::PowerShell | ShellFamily::Posix | ShellFamily::Unknown => false,
    }
}

/// The one-off natural-language words that open a message rather than a command when more words
/// follow (`yes please`, `nice work`, `hello there`). `what` is left to the #965 collision rules
/// and `nice` to [`wrapper_chain_is_command`], which runs first for `nice <command>`.
fn opens_with_one_off_natural_language_word(
    head_lower: &str,
    args: &[ShellWord],
    text: &str,
) -> bool {
    is_one_off_natural_language_word(head_lower)
        && head_lower != "what"
        && !args.is_empty()
        && !has_shell_structure(text)
        && !args.iter().any(is_data_shaped_word)
        && !args
            .first()
            .is_some_and(|next| is_known_shell_command_word(&next.text))
}

/// A command-headed line that reads as a sentence (`make me a backup script`, `kill Chrome
/// please`, `tell Josh when my build finished`): at least one word that addresses someone
/// ([`STRONG_PROSE_WORDS`], or a command-like `re-` verb out of subcommand position) and one more
/// prose signal (a function word, the article `a`, sentence punctuation, a Capitalised word, a
/// trailing `?`), with no shell structure, no shell data among the arguments, and a head that
/// takes neither free text nor a nested command. Pronouns alone (`ls this that it`, #964) and a
/// single `me`/`my` (`ls me`, `cd my`) are not a sentence.
fn reads_as_sentence_with_command_head(
    text: &str,
    words: &[ShellWord],
    first_index: usize,
) -> bool {
    if first_index > 0 || words.len() < first_index + 3 || has_shell_structure(text) {
        return false;
    }
    let Some(head_index) = effective_head_index(words, first_index) else {
        return false;
    };
    let head = &words[head_index];
    let basename = word_basename(head);
    if head.quoted
        || is_explicit_path_token(&head.text)
        || is_one_off_shell_command_keyword(basename)
        || FREE_TEXT_ARG_HEADS.contains(&basename)
        || takes_free_text(words, head_index)
    {
        return false;
    }
    let args = &words[head_index + 1..];
    if args.len() < 2 || args.iter().any(is_data_shaped_word) {
        return false;
    }
    let mut strong: Vec<&str> = Vec::new();
    let mut weak = 0usize;
    for (index, word) in args.iter().enumerate() {
        if word.quoted {
            continue;
        }
        let bare = strip_sentence_punctuation(&word.text);
        let lower = bare.to_lowercase();
        if let Some(strong_word) = STRONG_PROSE_WORDS.iter().find(|word| **word == lower) {
            if !strong.contains(strong_word) {
                strong.push(*strong_word);
            }
            continue;
        }
        if is_misplaced_re_command_word(&head.text, args, index) {
            strong.push("re-");
            continue;
        }
        if is_function_word(word)
            || is_prose_article(args, index)
            || ends_in_sentence_punctuation(word)
            || is_capitalised_word(bare)
        {
            weak += 1;
        }
    }
    if text.ends_with('?') {
        weak += 1;
    }
    !strong.is_empty() && strong.len() + weak >= 2
}

/// `Chrome`, `Josh`: an uppercase first letter followed by lowercase letters only.
fn is_capitalised_word(bare: &str) -> bool {
    let mut chars = bare.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && bare.len() >= 2
        && chars.all(|c| c.is_ascii_lowercase())
}
