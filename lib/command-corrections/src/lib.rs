pub mod rules;

use itertools::Itertools;
use lazy_static::lazy_static;
pub use rules::RuleId;
use std::{borrow::Cow, collections::HashSet};
use version_compare::Version;

#[cfg(test)]
mod test_utils;

// TODO: move these structs/impls to dedicated files.

/// A RuleCorrection is created by a Rule. Itcould either be a
/// fully formed command or a series of command parts that need
/// to be combined to form a command.
///
/// Note about Cow<str>: we use a Cow here since the string will either be a reference
/// to one of the input parts, a static string, or a computed string over an input part.
/// In the first two cases, a borrowed string suffices, but in the last case, we need an owned string.
/// Using a Cow allows us to avoid `clone`s for the first two types of strings.
#[derive(Debug, PartialEq)]
pub enum RuleCorrection<'a> {
    Command(Cow<'a, str>),

    /// Example: if the corrected command is `git commit -m "best fix"`, then the correction is
    /// ["git", "commit", "-m", "best fix"]. Note that "best fix" is a single part. This is important
    /// as it guarantees the correct shell-escaped command is returned in `to_command_string`.
    /// Note: although translating into a RuleCorrection seems expensive (using Vecs), very few corrections
    /// are actually computed (only for matching top-level command and if the rule `matches`)
    CommandParts(Vec<Cow<'a, str>>),

    // TODO: This is a temporary solution to a better escaping strategy
    /// The `And` variant is used for corrections that involve two other
    /// Corrections, meant to be combined with the shell AND operator. For example,
    /// the command "mkdir -p dir && cd dir" can be expressed with an And variant like so:
    /// And(CommandParts(["mkdir", "-p", "dir"]), Command("cd dir"))
    And(Box<RuleCorrection<'a>>, Box<RuleCorrection<'a>>),

    /// An `And` variant specifically for PowerShell versions < 7.0. Old Powershell versions
    /// do not support the `&&` pipeline chaining operator. Instead we adopt the form:
    ///     a; if ($?) { b }
    IfAThenB(Box<RuleCorrection<'a>>, Box<RuleCorrection<'a>>),
}

impl<'a> RuleCorrection<'a> {
    fn to_command_string(&self, shell: &Shell) -> String {
        use RuleCorrection::*;
        match self {
            // base cases
            Command(str) => str.to_string(),
            #[allow(deprecated)]
            // TODO(dan): shlex::join is deprecated b/c it propagates null bytes.
            // However, using the newer versions would require a decent refactor to allow
            // this function to return a result. We are just ignoring the deprecation for
            // now, but may want to revisit in the future.
            CommandParts(parts) => shlex::join(parts.iter().map(|part| part.as_ref())),

            // recursive case
            And(first, second) => {
                let first_str = first.to_command_string(shell);
                let second_str = second.to_command_string(shell);
                [first_str.as_str(), shell.and(), second_str.as_str()].join(" ")
            }
            IfAThenB(first, second) => {
                let first_str = first.to_command_string(shell);
                let second_str = second.to_command_string(shell);
                format!("{first_str}; if ($?) {{ {second_str} }}")
            }
        }
    }
}

// Note: need two separate From impl's because we
// can't do From<T> where T: Cow<str> (compiler will complain
// about conflicting with the other From impl's)
impl<'a> From<String> for RuleCorrection<'a> {
    fn from(command: String) -> Self {
        RuleCorrection::Command(command.into())
    }
}
impl<'a> From<&'a str> for RuleCorrection<'a> {
    fn from(command: &'a str) -> Self {
        RuleCorrection::Command(command.into())
    }
}

impl<'a, T> From<Vec<T>> for RuleCorrection<'a>
where
    T: Into<Cow<'a, str>>,
{
    fn from(parts: Vec<T>) -> Self {
        RuleCorrection::CommandParts(parts.into_iter().map(Into::into).collect_vec())
    }
}
impl<'a, T> From<&'a [T]> for RuleCorrection<'a>
where
    T: AsRef<str>,
{
    fn from(parts: &'a [T]) -> Self {
        RuleCorrection::CommandParts(
            parts
                .iter()
                .map(AsRef::as_ref)
                .map(Into::into)
                .collect_vec(),
        )
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExitCode(i32);

impl ExitCode {
    /// We specifically consider exit codes 130 and 141 as success
    /// - 130 is the exit code for SIGINT (e.g. user terminates process with ctrl-c)
    /// - 141 is the exit code for SIGPIPE (e.g. user quits the git log pager)
    // TODO: we should support passing in exit codes so that a client
    // can configure this.
    pub fn is_success(&self) -> bool {
        self.0 == 0 || self.0 == 130 || self.0 == 141
    }

    pub fn is_error(&self) -> bool {
        !self.is_success()
    }

    pub fn raw(&self) -> i32 {
        self.0
    }
}

impl From<i32> for ExitCode {
    fn from(code: i32) -> Self {
        ExitCode(code)
    }
}

/// The shells supported by this crate.
#[derive(Default, Clone, PartialEq)]
pub enum Shell {
    #[default]
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

impl Shell {
    #[allow(dead_code)]
    fn and(&self) -> &'static str {
        use Shell::*;
        match self {
            Bash | Zsh | PowerShell => "&&",
            Fish => "and",
        }
    }

    #[cfg(test)]
    fn test_version(&self) -> &'static str {
        match self {
            Self::Bash => "5.2.26",
            Self::Zsh => "5.9",
            Self::Fish => "3.7.1",
            Self::PowerShell => "7.4.2",
        }
    }
}

#[cfg(test)]
impl std::fmt::Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "pwsh",
        };
        write!(f, "{name}")
    }
}

/// A Command represents a shell command that the user executed along
/// with its metadata. This is used to determine which corrections
/// make sense in the context of the command.
pub struct Command<'a> {
    // The following are required attributes of a Command
    input: &'a str,
    output: &'a str,
    /// TODO: by default, a rule should only be run if the command failed.
    /// Rules that should run if a command _succeeded_ should have to opt-in.
    #[allow(dead_code)]
    exit_code: ExitCode,

    // The following are optional attributes of a Command, following the builder pattern.
    /// The directory the command was executed in.
    /// The path is assumed to be absolute.
    // TODO: we should represent this as a typed Path
    //       once we support Rules that can read from a remote FS
    //       to compute a correction.
    working_dir: Option<&'a str>,

    // The following are internal, computed properties of a Command.
    lowercase_output: String,
    input_parts: Vec<String>,
}

impl<'a> Command<'a> {
    pub fn new(input: &'a str, output: &'a str, exit_code: ExitCode) -> Self {
        // TODO: We need to re-escape multiword parts in the input after splitting.
        let input = input.trim();
        let output = output.trim();
        let input_parts = shlex::split(input).unwrap_or_default();
        let lowercase_output = output.to_lowercase();

        Self {
            input,
            output,
            working_dir: None,
            lowercase_output,
            input_parts,
            exit_code,
        }
    }

    pub fn set_working_dir(mut self, working_dir: &'a str) -> Self {
        self.working_dir = Some(working_dir);
        self
    }

    pub fn input_parts(&self) -> &[String] {
        &self.input_parts
    }

    pub fn lowercase_output(&self) -> &str {
        self.lowercase_output.as_str()
    }
}

type AliasName<'a> = &'a str;
type BuiltinName<'a> = &'a str;
type ExecutableName<'a> = &'a str;
type FunctionName<'a> = &'a str;
type BranchName<'a> = &'a str;

#[derive(Copy, Clone)]
pub struct HistoryItem<'a> {
    command: &'a str,
    exit_code: ExitCode,

    /// The working directory the command was run in.
    /// The path is assumed to be an absolute path.
    // TODO: we should represent this as a typed Path
    //       once we support Rules that can read from a remote FS
    //       to compute a correction.
    pwd: &'a str,
}

impl<'a> HistoryItem<'a> {
    pub fn new(command: &'a str, exit_code: impl Into<ExitCode>, pwd: &'a str) -> Self {
        Self {
            command,
            exit_code: exit_code.into(),
            pwd,
        }
    }
}

#[derive(Default)]
pub enum SessionType {
    #[default]
    Local,
    Remote,
}

impl SessionType {
    fn is_local(&self) -> bool {
        matches!(self, SessionType::Local)
    }
}
lazy_static! {
    static ref POWERSHELL_PIPELINE_CHAINING_VERSION: Version<'static> =
        Version::from("7.0.0").expect("Version number should not fail to parse");
}

#[derive(Default, Clone, PartialEq, Debug)]
pub enum PlatformType {
    #[default]
    Posix,
    Windows,
}

#[derive(Debug)]
pub struct PlatformTypeParseError(pub String);

impl std::str::FromStr for PlatformType {
    type Err = PlatformTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Windows" => Ok(PlatformType::Windows),
            "Posix" => Ok(PlatformType::Posix),
            _ => Err(PlatformTypeParseError(s.to_owned())),
        }
    }
}

#[cfg(test)]
impl std::fmt::Display for PlatformType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self, f)
    }
}

#[derive(Default)]
pub struct SessionMetadata<'a> {
    shell: Shell,
    version: Option<Version<'a>>,
    session_type: SessionType,
    platform_type: PlatformType,

    aliases: HashSet<AliasName<'a>>,
    builtins: HashSet<BuiltinName<'a>>,
    executables: HashSet<ExecutableName<'a>>,
    functions: HashSet<FunctionName<'a>>,

    history: Vec<HistoryItem<'a>>,

    // TODO: deprecate this field once we support
    // arbitrary command execution in rules
    git_branches: HashSet<BranchName<'a>>,

    // NOTE: this is inverted b/c we want the pipeline
    // chaining to be the default behavior. Doing
    // SessionMetadata::default() should not disable pipeline chaining.
    disable_pipeline_chaining: bool,
}

impl<'a> SessionMetadata<'a> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn set_aliases(&mut self, aliases: impl IntoIterator<Item = AliasName<'a>>) {
        self.aliases = HashSet::from_iter(aliases);
    }

    pub fn set_builtins(&mut self, builtins: impl IntoIterator<Item = BuiltinName<'a>>) {
        self.builtins = HashSet::from_iter(builtins);
    }

    pub fn set_functions(&mut self, functions: impl IntoIterator<Item = FunctionName<'a>>) {
        self.functions = HashSet::from_iter(functions);
    }

    pub fn set_executables(&mut self, executables: impl IntoIterator<Item = ExecutableName<'a>>) {
        self.executables = HashSet::from_iter(executables);
    }

    pub fn set_history(&mut self, history: impl IntoIterator<Item = HistoryItem<'a>>) {
        self.history = Vec::from_iter(history);
    }

    pub fn set_git_branches(&mut self, git_branches: impl IntoIterator<Item = BranchName<'a>>) {
        self.git_branches = HashSet::from_iter(git_branches);
    }

    pub fn set_session_type(&mut self, session_type: SessionType) {
        self.session_type = session_type;
    }

    pub fn set_shell(&mut self, shell: Shell, version: Option<&'a str>) {
        self.shell = shell;
        self.version = match version {
            Some(x) => Version::from(x),
            None => None,
        };

        // Determine if we can support pipeline chaining.
        // Powershell versions above 7, and all SH shells can support it,
        // otherwise we disable it.
        self.disable_pipeline_chaining = match (&self.shell, &self.version) {
            (Shell::PowerShell, Some(version)) => version.lt(&POWERSHELL_PIPELINE_CHAINING_VERSION),
            (Shell::PowerShell, None) => true,
            (_, _) => false,
        }
    }

    pub fn check_supports_pipeline_chaining(&self) -> bool {
        !self.disable_pipeline_chaining
    }

    pub fn set_platform_type(&mut self, platform: PlatformType) {
        self.platform_type = platform;
    }

    fn is_top_level_command(&self, command: &str) -> bool {
        self.executables.contains(command)
            || self.aliases.contains(command)
            || self.functions.contains(command)
            || self.builtins.contains(command)
    }

    fn top_level_commands(&self) -> impl Iterator<Item = &str> {
        self.executables
            .iter()
            .chain(self.aliases.iter())
            .chain(self.functions.iter())
            .chain(self.builtins.iter())
            .copied()
    }

    /// Returns the command name for each history item
    /// e.g. `git checkout` => `git`
    fn top_level_commands_from_history(&self) -> impl Iterator<Item = &str> {
        self.history
            .iter()
            .filter_map(|s| s.command.split_whitespace().next())
    }

    /// Utility to create the And variant of RulesCorrection (without fussing with Box at callsites)
    /// If pipeline chaining is not supported, this will return None (and will panic if debug_asserts
    /// are enabled).
    #[allow(dead_code)]
    fn and(
        &self,
        first: impl Into<RuleCorrection<'a>>,
        second: impl Into<RuleCorrection<'a>>,
    ) -> Option<RuleCorrection<'a>> {
        if self.check_supports_pipeline_chaining() {
            Some(RuleCorrection::And(
                Box::new(first.into()),
                Box::new(second.into()),
            ))
        } else if self.shell == Shell::PowerShell {
            Some(RuleCorrection::IfAThenB(
                Box::new(first.into()),
                Box::new(second.into()),
            ))
        } else {
            None
        }
    }
}

/// A Correction is what's returned to the caller. It includes the corrected
/// command along with metadata about the correction itself.
#[derive(Clone, PartialEq)]
pub struct Correction {
    pub command: String,
    pub rule_applied: RuleId,
}

/// Returns a list of command corrections given a command. This is _heavily_ inspired
/// by The Fuck (https://github.com/nvbn/thefuck).
pub fn correct_command(
    command: Command,
    session_metadata: &SessionMetadata,
    ignored_rules: impl IntoIterator<Item = RuleId>,
) -> Vec<Correction> {
    let rules_by_command = &*rules::RULES_BY_COMMAND;
    let ignored_rules: HashSet<RuleId> = HashSet::from_iter(ignored_rules);

    let command_name: Cow<str> = Cow::from(match command.input_parts.first() {
        None => return vec![],
        Some(first) => first.to_lowercase(),
    });

    // Note: the current order that the rules are applied in is:
    // 1. command specific rules before generic rules
    // 2. within each rule group, the order of rules is the order in which they're evaluated
    // This is a stopgap until we support `Priority` on a RuleCorrection
    rules_by_command
        .get(command_name.as_ref())
        .into_iter()
        .flatten()
        .chain(rules::GENERIC_RULES.iter())
        .filter(|rule| {
            // Only check a rule if it should be considered by default.
            let should_be_considered =
                rule.should_be_considered_by_default(&command, session_metadata);

            // And finally, make sure the rule matches. Note: the order of these is important.
            // `matches` can be expensive so we check it last.
            should_be_considered
                && !ignored_rules.contains(&rule.id())
                && rule.matches(&command, session_metadata)
        })
        .flat_map(|rule| {
            // Generate the corrections for this rule.
            rule.generate_command_corrections(&command, session_metadata)
                .into_iter()
                .flatten()
                .filter_map(|rule_correction| {
                    // Don't consider corrections that look exactly like the original command input or are empty.
                    let cmd_string = rule_correction
                        .to_command_string(&session_metadata.shell)
                        .trim()
                        .to_owned();

                    (cmd_string != command.input && !cmd_string.is_empty()).then_some(Correction {
                        command: cmd_string,
                        rule_applied: rule.id(),
                    })
                })
        })
        .unique_by(|correction| correction.command.to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        rules::{generic::cd_parent::CdParent, Rule},
        test_utils::corrections,
        Command, SessionMetadata, Shell,
    };

    #[test]
    fn test_ignored_rules() {
        let command = Command::new("cd..", "", 127.into());
        let metadata = SessionMetadata::new();

        assert_eq!(
            corrections(command, &metadata, std::iter::empty()),
            vec!["cd .."]
        );

        let command = Command::new("cd..", "", 127.into());
        assert!(corrections(command, &metadata, [CdParent.id()].into_iter()).is_empty());
    }

    // NOTE: this is a little brittle b/c it depends on the rules at `src/rules/cat/cat_dir.rs`
    // However, I did not have a good way to add a testing rule to validate that command corrections
    // work for this case.
    #[test]
    fn test_case_insensitivity() {
        let mut metadata = SessionMetadata::new();

        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
            let version = shell.test_version();
            metadata.set_shell(shell.clone(), Some(version));
            for command_input in ["cat src", "Cat src", "CAT src"] {
                let command = Command::new(command_input, "cat: src: Is a directory", 127.into());
                let expected = vec!["ls src"];
                let actual = corrections(command, &metadata, std::iter::empty());
                assert_eq!(
                    actual,
                    expected,
                    "expected shell {shell} to correct '{command_input}' to '{expected:?}', but got {actual:?}"
                );
            }
        }
    }
}
