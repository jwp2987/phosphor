//! One exec per refresh for the read-only git questions of the shell arm (#2156).
//!
//! Without the helper, one `cd` into a repository asked the host dozens of git questions, each a
//! fresh ssh exec channel: the git chip (main branch, branch, status, numstat, upstream, counts),
//! the PR and repository chips (work-tree and branch checks) and the diff state (all of those
//! again, plus the unpushed commits). They are the same few questions asked up to four times by
//! four independent producers.
//!
//! [`BatchedGitRunner`] keeps the shared algorithms (`shellcmd::git`) exactly as they are: they
//! still call `runner.run(args, ..)` one question at a time. What changes is where the answers
//! come from. A refresh first runs ONE framed [`Script`] (the `agent_context` envelope) whose
//! sections are the fixed set of read-only calls those algorithms are known to make; the answer
//! of a call is looked up by `(repo, args)` in what that script returned, and a call that is not
//! there (other arguments, any stdin, a write, a per-file diff) is one ordinary exec, exactly
//! today's behaviour.
//!
//! * Sections are independent. A section that is missing from the frame is a miss for ITS calls
//!   only (those calls take the per-call path); a section that ran and failed holds the real
//!   failing answer (no upstream, no HEAD yet), which is what the per-call path would have
//!   returned. A script that fails as a whole (exec error, truncated, no frame) means no
//!   prefetch for that refresh: every call takes the per-call path, never an empty chip.
//! * Producers of one precmd share a generation: a request for a repo that arrives within
//!   [`JOIN_WINDOW`] of the generation's start joins it instead of starting another script. A
//!   later request starts its own, so an answer is never reused for another prompt.
//! * `gh` is not run here (it is the network round trip and runs only when a lookup is due). The
//!   script only asks whether `gh` is on the host's `PATH`; when it is not, the `gh` calls are
//!   answered "not found" without an exec.
//! * The file tree of the same directory rides in the same script (#2156 lever B): the
//!   navigation probe (`canon`, `git`) and the two `ls-files` lists (`files`, `ignoredpaths`) are
//!   sections like the others, each capped well below the standalone call's cap. A list that
//!   reaches its cap is dropped from the answer and the tree runs that one section as its own
//!   exec, exactly as before; a section that is missing costs only itself.
//! * `PHOSPHOR_SHELL_BATCH_GIT=0` turns all of it off (read once per process).
//!
//! The sections that depend on another answer (the unpushed commits and the fork point chain)
//! compute that answer in the host shell too and print the value they used as the first line of
//! their output. [`assemble`] files the output under the key the Rust algorithm will ask for only
//! if its own computation of that value is identical; otherwise the section is a miss.
//!
//! No timers: the join window compares instants, nothing sleeps or polls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures::FutureExt as _;
use futures::future::{BoxFuture, Shared};

use super::super::shell_git_runner::ShellGitRunner;
use super::ShellBackend;
use crate::shellcmd::git::{
    GitOutput, GitRunner, MAIN_BRANCH_CANDIDATES, NUMSTAT_Z_HEAD_ARGS, STATUS_CHIP_ARGS,
    STATUS_FULL_ARGS, UNPUSHED_LOG_FORMAT,
};
use crate::shellcmd::tree::{
    canonical_dir_command, git_root_command, repo_files_command_capped, repo_ignored_command_capped,
};
use crate::shellcmd::{Frame, FrameError, Script, ShellKind, StderrMode, sh_quote, sh_quote_path};

/// A request for a repo whose generation started less than this ago joins it. Short enough that
/// a second prompt (a new command that may have changed the repo) never lands inside it, long
/// enough for the producers of one precmd, which are started together.
pub(super) const JOIN_WINDOW: Duration = Duration::from_millis(100);

/// A generation older than this is forgotten (it only holds memory by then).
const KEEP: Duration = Duration::from_secs(10);

/// With prompt keying on, a generation of the current prompt is joined by every producer for at
/// most this long after it opened (a producer later than this is a different question: an event
/// the manager never announced may have changed the repo). A generation still in flight is
/// always joined, however long its exec takes.
const PROMPT_MAX_AGE: Duration = Duration::from_secs(2);

/// Whether the generation is keyed on the prompt counter (see [`GitBatchStore::begin_prompt`]):
/// everything but `PHOSPHOR_SHELL_BATCH_PROMPT=0`, which brings back the pure 100 ms window.
/// Read once.
pub(super) fn prompt_keying_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("PHOSPHOR_SHELL_BATCH_PROMPT").map_or(true, |v| v.trim() != "0")
    })
}

/// The key two spellings of one directory share: runs of `/` collapse, `.` components and a
/// trailing `/` go. No filesystem access (a symlink is not resolved: a spelling that differs by
/// more than this is a different key and costs its own exec, as before).
pub(super) fn join_key(path: &str) -> String {
    if path.contains('\\') || path.contains("..") {
        return path.to_owned();
    }
    let absolute = path.starts_with('/');
    let parts: Vec<&str> = path
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .collect();
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

/// Whether batching is on: everything but `PHOSPHOR_SHELL_BATCH_GIT=0`. Read once.
pub(super) fn batching_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PHOSPHOR_SHELL_BATCH_GIT").map_or(true, |v| v.trim() != "0"))
}

/// How much of each tree list the batched script takes. A list that reaches its cap is left out
/// of the batch's answer and read by the tree's own (larger-capped) call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TreeCaps {
    pub(super) files: usize,
    pub(super) ignored: usize,
}

impl TreeCaps {
    pub(super) const DEFAULT: Self = Self {
        files: 1024 * 1024,
        ignored: 256 * 1024,
    };
}

/// What one batched script returned, ready to answer calls.
#[derive(Debug, Default)]
pub(in crate::backend) struct Prefetched {
    entries: HashMap<Vec<String>, GitOutput>,
    /// Whether `gh` is on the host's `PATH`; `None`: the script did not say.
    gh_available: Option<bool>,
    /// The tree sections (`canon`, `git`, `files`, `ignoredpaths`) the script returned, minus any
    /// list that reached its cap.
    tree: Option<Frame>,
}

impl Prefetched {
    /// The tree sections of the batch, for `shell/tree.rs`.
    pub(in crate::backend) fn tree(&self) -> Option<&Frame> {
        self.tree.as_ref()
    }
}

type Fetched = Shared<BoxFuture<'static, Option<Arc<Prefetched>>>>;

struct Generation {
    opened: Instant,
    /// The prompt counter value the generation was started under.
    prompt: u64,
    /// Its number, for the debug lines.
    id: u64,
    /// Who asked for it, in order (the starter first), for the debug lines.
    producers: Arc<Mutex<Vec<&'static str>>>,
    fut: Fetched,
}

/// The generations of one host, by repo path. Shared by every backend the router builds for the
/// host, so the producers of one precmd (each its own backend) find each other.
pub struct GitBatchStore {
    repos: Mutex<HashMap<String, Generation>>,
    join_window: Duration,
    prompt_max_age: Duration,
    /// Whether the prompt counter keys the generations (see [`prompt_keying_enabled`]).
    prompt_keyed: bool,
    /// Bumped once per prompt by the manager ([`Self::begin_prompt`]); 0 until the first one.
    prompt: AtomicU64,
    next_id: AtomicU64,
    tree_caps: TreeCaps,
}

impl Default for GitBatchStore {
    fn default() -> Self {
        Self::with_join_window(JOIN_WINDOW)
    }
}

impl GitBatchStore {
    pub(super) fn with_join_window(join_window: Duration) -> Self {
        Self {
            repos: Mutex::default(),
            join_window,
            prompt_max_age: PROMPT_MAX_AGE,
            prompt_keyed: prompt_keying_enabled(),
            prompt: AtomicU64::new(0),
            next_id: AtomicU64::new(1),
            tree_caps: TreeCaps::DEFAULT,
        }
    }

    /// The same store with other caps on the tree lists (tests).
    pub(super) fn with_tree_caps(mut self, tree_caps: TreeCaps) -> Self {
        self.tree_caps = tree_caps;
        self
    }

    /// The same store with prompt keying forced on or off (tests).
    #[cfg(test)]
    pub(super) fn with_prompt_keying(mut self, on: bool) -> Self {
        self.prompt_keyed = on;
        self
    }

    /// The same store with another prompt-generation age limit (tests).
    #[cfg(test)]
    pub(super) fn with_prompt_max_age(mut self, age: Duration) -> Self {
        self.prompt_max_age = age;
        self
    }

    /// A new prompt (or user refresh) started for this host: no producer of it may join an
    /// answer read for the previous one. Every producer of this prompt, however late it asks and
    /// however it spells the repo path, then joins the ONE exec the first of them starts.
    pub fn begin_prompt(&self) {
        self.prompt.fetch_add(1, Ordering::SeqCst);
    }

    /// Forgets every generation: a write just happened (or is about to), so no later request may
    /// join an answer read before it.
    pub(super) fn invalidate(&self) {
        self.repos.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// The answers for `repo_path`: those of the generation a producer of this refresh already
    /// started, else a new one (one exec). `None`: no prefetch, every call takes its own exec.
    /// `producer` names the caller in the debug lines.
    pub(in crate::backend) async fn get(
        &self,
        backend: &ShellBackend,
        runner: &ShellGitRunner,
        repo_path: &str,
        producer: &'static str,
    ) -> Option<Arc<Prefetched>> {
        let key = join_key(repo_path);
        let prompt = self.prompt.load(Ordering::SeqCst);
        let fut = {
            let mut repos = self.repos.lock().unwrap_or_else(|e| e.into_inner());
            repos.retain(|_, g| g.opened.elapsed() < KEEP);
            let joined = repos
                .get(&key)
                .filter(|g| self.joinable(g, prompt))
                .map(|g| {
                    g.producers
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(producer);
                    log::debug!(
                        "git batch: producer={producer} joins generation={} (prompt={prompt}) \
                         path={repo_path}",
                        g.id
                    );
                    g.fut.clone()
                });
            match joined {
                Some(fut) => fut,
                None => {
                    let id = self.next_id.fetch_add(1, Ordering::SeqCst);
                    let producers = Arc::new(Mutex::new(vec![producer]));
                    log::debug!(
                        "git batch: exec started generation={id} prompt={prompt} \
                         producer={producer} path={repo_path}"
                    );
                    let fut: Fetched = fetch(
                        backend.clone(),
                        runner.clone(),
                        repo_path.to_owned(),
                        self.tree_caps,
                        id,
                        Arc::clone(&producers),
                    )
                    .boxed()
                    .shared();
                    repos.insert(
                        key,
                        Generation {
                            opened: Instant::now(),
                            prompt,
                            id,
                            producers,
                            fut: fut.clone(),
                        },
                    );
                    fut
                }
            }
        };
        fut.await
    }

    /// Whether a request under prompt counter `prompt` may join `g`.
    fn joinable(&self, g: &Generation, prompt: u64) -> bool {
        let age = g.opened.elapsed();
        if age < self.join_window {
            // Started together (the pre-#2156 rule), unless the manager announced a new prompt
            // in between.
            return !self.prompt_keyed || g.prompt == prompt;
        }
        // Prompt keying: every producer of the same announced prompt, whenever it asks.
        self.prompt_keyed
            && prompt > 0
            && g.prompt == prompt
            && (age < self.prompt_max_age || g.fut.peek().is_none())
    }
}

async fn fetch(
    backend: ShellBackend,
    runner: ShellGitRunner,
    repo: String,
    caps: TreeCaps,
    id: u64,
    producers: Arc<Mutex<Vec<&'static str>>>,
) -> Option<Arc<Prefetched>> {
    let script = match build_script(&runner, &repo)
        .and_then(|script| add_tree_sections(script, backend.shell_kind(), &repo, caps))
    {
        Ok(script) => script,
        Err(e) => {
            log::debug!("git batch: cannot build the script, per-call fallback: {e}");
            return None;
        }
    };
    let result = backend.run_script(&script).await;
    log::debug!(
        "git batch: exec done generation={id} ok={} producers={:?} path={repo}",
        result.is_ok(),
        producers.lock().unwrap_or_else(|e| e.into_inner())
    );
    match result {
        Ok(frame) => {
            let mut prefetched = assemble(&frame);
            prefetched.tree = tree_frame(&frame, caps);
            Some(Arc::new(prefetched))
        }
        Err(e) => {
            log::debug!("git batch: the script failed, per-call fallback: {e}");
            None
        }
    }
}

/// A [`GitRunner`] that answers the calls one batched script already answered and execs the rest.
#[derive(Debug)]
pub(in crate::backend) struct BatchedGitRunner {
    inner: ShellGitRunner,
    repo: PathBuf,
    data: Option<Arc<Prefetched>>,
}

impl BatchedGitRunner {
    pub(in crate::backend) fn new(
        inner: ShellGitRunner,
        repo_path: &str,
        data: Option<Arc<Prefetched>>,
    ) -> Self {
        Self {
            inner,
            repo: PathBuf::from(repo_path),
            data,
        }
    }

    fn lookup(&self, args: &[&str], stdin: Option<&[u8]>, cwd: &Path) -> Option<GitOutput> {
        if stdin.is_some() || cwd != self.repo {
            return None;
        }
        let key: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        self.data.as_ref()?.entries.get(&key).cloned()
    }
}

impl GitRunner for BatchedGitRunner {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        match self.lookup(args, stdin, cwd) {
            Some(output) => std::future::ready(Ok(output)).boxed(),
            None => self.inner.run(args, stdin, cwd),
        }
    }

    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        let gh_missing = cwd == self.repo
            && self
                .data
                .as_ref()
                .is_some_and(|d| d.gh_available == Some(false));
        if gh_missing {
            // What the host's `sh` would have said, without opening a channel to hear it.
            return std::future::ready(Ok(GitOutput {
                stdout: Vec::new(),
                stderr: b"sh: gh: command not found\n".to_vec(),
                exit_code: Some(127),
            }))
            .boxed();
        }
        self.inner.run_gh(args, cwd)
    }
}

// ---------------------------------------------------------------------------------------------
// The script.

/// A call with fixed arguments: one section, filed under `args`.
struct Call {
    name: String,
    args: Vec<String>,
    /// stderr is folded into the output (the caller reads the message of a failure).
    merge_stderr: bool,
}

impl Call {
    fn new(name: impl Into<String>, args: &[&str]) -> Self {
        Self {
            name: name.into(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            merge_stderr: false,
        }
    }
}

const UPSTREAM_ARGS: [&str; 4] = ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"];
const ABBREV_ARGS: [&str; 3] = ["rev-parse", "--abbrev-ref", "HEAD"];
const SHOW_CURRENT_ARGS: [&str; 2] = ["branch", "--show-current"];

/// Every read-only call the status chip, the PR/repository checks and the diff state make with
/// arguments that do not depend on another answer.
fn fixed_calls() -> Vec<Call> {
    let mut calls = vec![
        // The diff state's directory probe reads the message of a failure.
        Call {
            merge_stderr: true,
            ..Call::new("gitdir", &["rev-parse", "--git-dir"])
        },
        Call::new("symref", &["symbolic-ref", "refs/remotes/origin/HEAD"]),
    ];
    for (i, candidate) in MAIN_BRANCH_CANDIDATES.iter().enumerate() {
        let arg = format!("{candidate}^{{}}");
        calls.push(Call::new(
            format!("verify{i}"),
            &["rev-parse", "--verify", &arg],
        ));
    }
    calls.extend([
        Call::new("showcur", &SHOW_CURRENT_ARGS),
        Call::new("abbrev", &ABBREV_ARGS),
        Call::new("shorthead", &["rev-parse", "--short", "HEAD"]),
        Call::new("head", &["rev-parse", "HEAD"]),
        Call::new("statuschip", &STATUS_CHIP_ARGS),
        Call::new("statusfull", &STATUS_FULL_ARGS),
        Call::new("numstatz", &NUMSTAT_Z_HEAD_ARGS),
        Call::new("numstathead", &["diff", "--numstat", "HEAD"]),
        Call::new("upstream", &UPSTREAM_ARGS),
        Call::new(
            "counts",
            &[
                "rev-list",
                "--left-right",
                "--cherry-mark",
                "--count",
                "HEAD...@{u}",
            ],
        ),
        Call::new("insidewt", &["rev-parse", "--is-inside-work-tree"]),
        Call::new("symhead", &["symbolic-ref", "--quiet", "--short", "HEAD"]),
    ]);
    calls
}

/// The host-shell computation of the current branch and of the fork point's `rev-list`, as
/// `detect_current_branch` and `detect_fork_point` do it.
const FORK_PREAMBLE: &str = "cur=$(g rev-parse --abbrev-ref HEAD) || cur=$(g branch --show-current)
rl() {
  if [ -n \"$cur\" ] && [ \"$cur\" != HEAD ]; then
    g rev-list HEAD --not \"--exclude=$cur\" --branches \"--exclude=origin/$cur\" --remotes
  else
    g rev-list HEAD --not --branches --remotes
  fi
}
";

/// From `$u` (the `rev-list` output) to `$t`, the revision whose parent is the fork point.
const FORK_TARGET: &str = "last=$(printf '%s\\n' \"$u\" | awk 'NF { l = $0 } END { print l }')
if [ -n \"$last\" ]; then t=\"$last^\"; else t=HEAD; fi
";

fn build_script(runner: &ShellGitRunner, repo: &str) -> Result<Script, FrameError> {
    // Every section is its own subshell: each gets the directory and the git function.
    let head = format!(
        "cd {} || exit 1\ng() {{ {} \"$@\" </dev/null; }}\n",
        sh_quote_path(repo),
        runner.git_command_prefix()
    );
    let log_format = sh_quote(UNPUSHED_LOG_FORMAT);
    let mut script = Script::new(ShellBackend::new_nonce())?;
    for call in fixed_calls() {
        let args: Vec<String> = call.args.iter().map(|a| sh_quote(a)).collect();
        let mode = if call.merge_stderr {
            StderrMode::Merge
        } else {
            StderrMode::Discard
        };
        script = script.section(call.name, format!("{head}g {}", args.join(" ")), mode)?;
    }
    // The unpushed commits against the upstream.
    script = script.section(
        "unpushed",
        format!(
            "{head}up=$(g rev-parse --abbrev-ref --symbolic-full-name '@{{u}}') || exit 1
[ -n \"$up\" ] || exit 1
printf '%s\\n' \"$up..HEAD\"
g log \"$up..HEAD\" {log_format} --numstat"
        ),
        StderrMode::Discard,
    )?;
    // With no upstream: the fork point chain (`rev-list`, the parent of its oldest commit, the
    // `log` since it). Each section recomputes what it needs and prints the key it used first.
    script = script.section(
        "forkunique",
        format!("{head}{FORK_PREAMBLE}printf '%s\\n' \"$cur\"\nrl"),
        StderrMode::Discard,
    )?;
    script = script.section(
        "forkparent",
        format!(
            "{head}{FORK_PREAMBLE}u=$(rl) || exit 1
{FORK_TARGET}printf '%s\\n' \"$t\"
g rev-parse \"$t\""
        ),
        StderrMode::Discard,
    )?;
    script = script.section(
        "forklog",
        format!(
            "{head}{FORK_PREAMBLE}if u=$(rl); then
{FORK_TARGET}  if fp=$(g rev-parse \"$t\"); then range=\"$fp..HEAD\"; else range=HEAD; fi
else
  range=HEAD
fi
printf '%s\\n' \"$range\"
g log \"$range\" {log_format} --numstat"
        ),
        StderrMode::Discard,
    )?;
    // Only whether `gh` exists; a lookup that is due runs `gh` itself.
    script = script.section(
        "ghpath",
        "command -v gh >/dev/null 2>&1",
        StderrMode::Discard,
    )?;
    Ok(script)
}

/// The tree's sections, named as `shell/tree.rs` reads them: the navigation probe of the
/// directory and the two `ls-files` lists (each capped at `caps`).
fn add_tree_sections(
    script: Script,
    kind: ShellKind,
    repo: &str,
    caps: TreeCaps,
) -> Result<Script, FrameError> {
    script
        .section(
            "canon",
            canonical_dir_command(kind, repo),
            StderrMode::Discard,
        )?
        .section("git", git_root_command(kind, repo), StderrMode::Discard)?
        .section(
            "files",
            repo_files_command_capped(kind, repo, caps.files),
            StderrMode::Discard,
        )?
        .section(
            "ignoredpaths",
            repo_ignored_command_capped(kind, repo, caps.ignored),
            StderrMode::Discard,
        )
}

/// The tree sections of `frame`. A list that reached its cap (`head -c` cut it, or it is one
/// byte short of the cap once the framing newline is gone) is left out so the tree reads it with
/// its own call; the others stay.
fn tree_frame(frame: &Frame, caps: TreeCaps) -> Option<Frame> {
    let mut sections = Vec::new();
    for (name, cap) in [
        ("canon", None),
        ("git", None),
        ("files", Some(caps.files)),
        ("ignoredpaths", Some(caps.ignored)),
    ] {
        let Some(section) = frame.section(name) else {
            continue;
        };
        if cap.is_some_and(|cap| section.output.len() + 1 >= cap) {
            continue;
        }
        sections.push(section.clone());
    }
    (!sections.is_empty()).then_some(Frame { sections })
}

// ---------------------------------------------------------------------------------------------
// Reading the frame back.

fn output_of(exit: i32, output: &[u8], merge_stderr: bool) -> GitOutput {
    if merge_stderr && exit != 0 {
        GitOutput {
            stdout: Vec::new(),
            stderr: output.to_vec(),
            exit_code: Some(exit),
        }
    } else {
        GitOutput {
            stdout: output.to_vec(),
            stderr: Vec::new(),
            exit_code: Some(exit),
        }
    }
}

/// `git_text`'s success rule: exit 0, or exit 1 with output.
fn text_ok(out: &GitOutput) -> Option<String> {
    let stdout = out.stdout_lossy();
    (out.success() || (out.exit_code == Some(1) && !stdout.is_empty())).then_some(stdout)
}

/// The first line of a section's output and the rest.
fn split_header(output: &[u8]) -> Option<(String, &[u8])> {
    let newline = output.iter().position(|&b| b == b'\n')?;
    let mut header = &output[..newline];
    if header.ends_with(b"\r") {
        header = &header[..header.len() - 1];
    }
    Some((
        String::from_utf8_lossy(header).into_owned(),
        &output[newline + 1..],
    ))
}

fn key(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// `detect_current_branch`, over the answers: `rev-parse --abbrev-ref HEAD`, else `branch
/// --show-current`, trimmed. `None` when neither worked (or was asked).
fn current_branch(entries: &HashMap<Vec<String>, GitOutput>) -> Option<String> {
    let abbrev = entries.get(&key(&ABBREV_ARGS))?;
    text_ok(abbrev)
        .or_else(|| entries.get(&key(&SHOW_CURRENT_ARGS)).and_then(text_ok))
        .map(|branch| branch.trim().to_owned())
}

fn assemble(frame: &Frame) -> Prefetched {
    let mut entries: HashMap<Vec<String>, GitOutput> = HashMap::new();
    for call in fixed_calls() {
        if let Some(section) = frame.section(&call.name) {
            entries.insert(
                call.args,
                output_of(section.exit, &section.output, call.merge_stderr),
            );
        }
    }
    derive_log_entries(&mut entries, frame);
    Prefetched {
        entries,
        gh_available: frame.section("ghpath").map(|s| s.exit == 0),
        tree: None,
    }
}

/// Files the sections whose arguments depend on an earlier answer under the arguments the Rust
/// algorithm (`compute_unpushed_state`) will use, when and only when it derives the same ones.
fn derive_log_entries(entries: &mut HashMap<Vec<String>, GitOutput>, frame: &Frame) {
    let upstream = entries
        .get(&key(&UPSTREAM_ARGS))
        .and_then(text_ok)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let current = current_branch(entries);
    let log_key = |range: String| {
        vec![
            "log".to_owned(),
            range,
            UNPUSHED_LOG_FORMAT.to_owned(),
            "--numstat".to_owned(),
        ]
    };
    // The section's output after its header line, when the header is the value Rust computed.
    let headed = |name: &str, expected: &str| -> Option<GitOutput> {
        let section = frame.section(name)?;
        let (header, rest) = split_header(&section.output)?;
        (header == expected).then(|| output_of(section.exit, rest, false))
    };

    if let Some(up) = upstream {
        let range = format!("{up}..HEAD");
        if let Some(out) = headed("unpushed", &range) {
            entries.insert(log_key(range), out);
        }
        return;
    }

    // No upstream: `detect_fork_point` over `current`.
    let wanted = current
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty() && *b != "HEAD")
        .map(str::to_owned);
    let mut rev_list: Vec<String> = ["rev-list", "HEAD", "--not"].map(str::to_owned).to_vec();
    if let Some(c) = &wanted {
        rev_list.push(format!("--exclude={c}"));
    }
    rev_list.push("--branches".to_owned());
    if let Some(c) = &wanted {
        rev_list.push(format!("--exclude=origin/{c}"));
    }
    rev_list.push("--remotes".to_owned());
    let Some(unique) = headed("forkunique", current.as_deref().unwrap_or_default()) else {
        return;
    };
    entries.insert(rev_list, unique.clone());

    let range = match text_ok(&unique) {
        // The fork point is unknown: the whole history.
        None => "HEAD".to_owned(),
        Some(unique) => {
            let target = match unique.lines().rfind(|l| !l.trim().is_empty()) {
                Some(sha) => format!("{}^", sha.trim()),
                None => "HEAD".to_owned(),
            };
            let Some(parent) = headed("forkparent", &target) else {
                return;
            };
            let range = match text_ok(&parent) {
                Some(sha) => format!("{}..HEAD", sha.trim()),
                None => "HEAD".to_owned(),
            };
            entries.insert(vec!["rev-parse".to_owned(), target], parent);
            range
        }
    };
    if let Some(out) = headed("forklog", &range) {
        entries.insert(log_key(range), out);
    }
}

#[cfg(test)]
#[path = "git_batch_tests.rs"]
mod tests;
