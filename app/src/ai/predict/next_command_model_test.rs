use crate::completer::SessionContext;
use crate::terminal::model::session::Session;
use crate::terminal::model::session::{
    command_executor::testing::TestCommandExecutor, BootstrapSessionType, SessionInfo,
};
use std::cell::Cell;
use std::collections::HashSet;
use typed_path::TypedPathBuf;
use warp_completer::signatures::CommandRegistry;
use warp_completer::{meta::SpannedItem, parsers::ParsedToken};
use warpui::App;

use super::*;

/// The pre-#811 shape of `find_potential_autosuggestions_from_history`: plain entries in and
/// out, no excluded commands, and a limit none of the small histories in these tests can
/// reach, so the ordering tests below keep asserting on the complete ranked list.
fn find_unbounded_autosuggestions_from_history<'a>(
    history_entries: impl DoubleEndedIterator<Item = &'a HistoryEntry>,
    buffer_text: &str,
    working_dir: Option<&str>,
) -> Vec<HistoryEntry> {
    let shared = history_entries.cloned().map(Arc::new).collect_vec();
    find_potential_autosuggestions_from_history(
        shared.iter(),
        buffer_text,
        working_dir,
        &HashSet::new(),
        MAX_HISTORY_AUTOSUGGESTION_CANDIDATES,
    )
    .into_iter()
    .map(|entry| (*entry).clone())
    .collect()
}

#[test]
fn test_find_autosuggestion_from_history_same_directory() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::with_pwd_and_exit_code("cd Documents", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    // We should return the most recent command from history that starts with the
    // buffer text and was executed in the current working directory.
    let autosuggestions = find_unbounded_autosuggestions_from_history(
        history_entries.iter(),
        "cd D",
        Some("/Users/tadej"),
    )
    .into_iter()
    .map(|history_entry| history_entry.command)
    .collect_vec();

    assert_eq!(
        autosuggestions,
        vec![
            "cd Documents".to_owned(),
            "cd Dotfiles".to_owned(),
            "cd Downloads".to_owned(),
        ]
    );
}

#[test]
fn test_find_autosuggestion_from_history_error_exit_code() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::with_pwd_and_exit_code("cd Documents", "/Users/tadej", 1),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    // We want to return failed commands in case the user wants to run it again.
    let autosuggestions = find_unbounded_autosuggestions_from_history(
        history_entries.iter(),
        "cd D",
        Some("/Users/tadej"),
    )
    .into_iter()
    .map(|history_entry| history_entry.command)
    .collect_vec();

    assert_eq!(
        autosuggestions,
        vec![
            "cd Documents".to_owned(),
            "cd Dotfiles".to_owned(),
            "cd Downloads".to_owned(),
        ]
    );
}

#[test]
fn test_find_autosuggestion_from_history_no_working_dir() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    // No working directory, so return the first successful command.
    let autosuggestions =
        find_unbounded_autosuggestions_from_history(history_entries.iter(), "cd D", None)
            .into_iter()
            .map(|history_entry| history_entry.command)
            .collect_vec();

    assert_eq!(
        autosuggestions,
        vec!["cd Downloads".to_owned(), "cd Dotfiles".to_owned(),]
    );
}

#[test]
fn test_find_autosuggestion_from_history_different_directory() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::with_pwd_and_exit_code("cd Documents", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    // There are no commands in history that were executed in the current directory,
    // So we return the most recent command that starts with the buffer text.
    let autosuggestions = find_unbounded_autosuggestions_from_history(
        history_entries.iter(),
        "cd D",
        Some("/Users/jonas"),
    )
    .into_iter()
    .map(|history_entry| history_entry.command)
    .collect_vec();

    assert_eq!(
        autosuggestions,
        vec![
            "cd Downloads".to_owned(),
            "cd Documents".to_owned(),
            "cd Dotfiles".to_owned(),
        ]
    );

    // There isn't a current working directory, so return the most recent command that
    // starts with the buffer text.
    let autosuggestions =
        find_unbounded_autosuggestions_from_history(history_entries.iter(), "cd D", None)
            .into_iter()
            .map(|history_entry| history_entry.command)
            .collect_vec();

    assert_eq!(
        autosuggestions,
        vec![
            "cd Downloads".to_owned(),
            "cd Documents".to_owned(),
            "cd Dotfiles".to_owned(),
        ]
    );
}

#[test]
fn test_find_autosuggestion_from_history_no_matching_commands() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::with_pwd_and_exit_code("cd Documents", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    let autosuggestions = find_unbounded_autosuggestions_from_history(
        history_entries.iter(),
        "cd Z",
        Some("/Users/jonas"),
    );

    assert_eq!(autosuggestions, vec![]);
}

#[test]
fn test_find_autosuggestion_from_history_matches_command_with_no_pwd() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::with_pwd_and_exit_code("cd Documents", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    let autosuggestions = find_unbounded_autosuggestions_from_history(
        history_entries.iter(),
        "cd P",
        Some("/Users/tadej"),
    )
    .into_iter()
    .map(|history_entry| history_entry.command)
    .collect_vec();

    assert_eq!(autosuggestions, vec!["cd Pictures".to_owned()]);
}

#[test]
fn test_find_autosuggestion_from_history_with_no_pwd_and_no_working_directory() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Documents"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej", 0),
    ];

    // When no working directory is passed, it shouldn't consider a command with
    // no pwd to be executed in the "same" directory and prioritize it.
    let autosuggestions =
        find_unbounded_autosuggestions_from_history(history_entries.iter(), "cd D", None)
            .into_iter()
            .map(|history_entry| history_entry.command)
            .collect_vec();

    assert_eq!(
        autosuggestions,
        vec![
            "cd Downloads".to_owned(),
            "cd Documents".to_owned(),
            "cd Dotfiles".to_owned()
        ]
    );
}

/// A `DoubleEndedIterator` adapter that counts how many entries the scan actually pulls
/// (yielded items only, not the terminating `None`), so a test can assert that the walk
/// stopped instead of visiting the whole history.
struct CountingIter<'c, I> {
    inner: I,
    visited: &'c Cell<usize>,
}

impl<I: Iterator> CountingIter<'_, I> {
    fn record(&self, item: Option<I::Item>) -> Option<I::Item> {
        if item.is_some() {
            self.visited.set(self.visited.get() + 1);
        }
        item
    }
}

impl<I: Iterator> Iterator for CountingIter<'_, I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next();
        self.record(item)
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for CountingIter<'_, I> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let item = self.inner.next_back();
        self.record(item)
    }
}

fn commands(entries: Vec<Arc<HistoryEntry>>) -> Vec<String> {
    entries.iter().map(|entry| entry.command.clone()).collect()
}

/// Pins the ranking the bounded scan must reproduce: newest-first within the current
/// directory, then newest-first everywhere else, with per-bucket dedup keeping the newest
/// occurrence and excluded commands never occupying a slot. The duplicate `git status` in the
/// other-dir bucket is deliberately kept: it is the newest occurrence *in that bucket*, and
/// dropping it in favour of the same-dir one would reorder what the consumer sees.
#[test]
fn test_find_autosuggestion_from_history_is_bounded_deduped_and_ranked() {
    // Oldest first.
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("git status", "/repo", 0),
        HistoryEntry::with_pwd_and_exit_code("git stash", "/repo", 0),
        HistoryEntry::with_pwd_and_exit_code("git log", "/elsewhere", 0),
        HistoryEntry::with_pwd_and_exit_code("git status", "/elsewhere", 0),
        HistoryEntry::with_pwd_and_exit_code("git push", "/repo", 0),
        HistoryEntry::with_pwd_and_exit_code("git status", "/repo", 0),
        HistoryEntry::with_pwd_and_exit_code("git log", "/elsewhere", 0),
        HistoryEntry::with_pwd_and_exit_code("git ignored-cmd", "/repo", 0),
        HistoryEntry::command_only("git diff"),
    ]
    .map(Arc::new);
    let excluded = HashSet::from(["git ignored-cmd".to_owned()]);

    // Unbounded (limit larger than history): full ranked list.
    let all = find_potential_autosuggestions_from_history(
        history_entries.iter(),
        "git ",
        Some("/repo"),
        &excluded,
        MAX_HISTORY_AUTOSUGGESTION_CANDIDATES,
    );
    assert_eq!(
        commands(all),
        vec![
            // same dir, newest first, one `git status`
            "git status".to_owned(),
            "git push".to_owned(),
            "git stash".to_owned(),
            // other dirs (and no-pwd), newest first, one `git log`
            "git diff".to_owned(),
            "git log".to_owned(),
            "git status".to_owned(),
        ]
    );

    // Bounded: the first `limit` of exactly that list, whichever buckets they fall in.
    for limit in 1..=6 {
        let bounded = find_potential_autosuggestions_from_history(
            history_entries.iter(),
            "git ",
            Some("/repo"),
            &excluded,
            limit,
        );
        let expected = commands(find_potential_autosuggestions_from_history(
            history_entries.iter(),
            "git ",
            Some("/repo"),
            &excluded,
            MAX_HISTORY_AUTOSUGGESTION_CANDIDATES,
        ))
        .into_iter()
        .take(limit)
        .collect_vec();
        assert_eq!(commands(bounded), expected, "limit {limit}");
    }

    // A ranked list that is all other-dir entries truncates the same way.
    let elsewhere = find_potential_autosuggestions_from_history(
        history_entries.iter(),
        "git ",
        Some("/nowhere"),
        &excluded,
        2,
    );
    assert_eq!(
        commands(elsewhere),
        vec!["git diff".to_owned(), "git log".to_owned()]
    );
}

/// The per-keystroke scan must stop once the same-directory bucket is full rather than walk
/// the whole history (#811): with a 50k-line histfile the unbounded walk cloned every prefix
/// match on the UI thread on every keystroke that landed with no suggestion showing.
#[test]
fn test_find_autosuggestion_from_history_stops_once_same_dir_bucket_is_full() {
    const OLD: usize = 990;
    const NEW: usize = 10;
    // Oldest first: a long tail run elsewhere, then a few recent commands run here.
    let history_entries = (0..OLD)
        .map(|i| {
            HistoryEntry::with_pwd_and_exit_code(
                format!("echo old {i}"),
                "/elsewhere".to_owned(),
                0,
            )
        })
        .chain((0..NEW).map(|i| {
            HistoryEntry::with_pwd_and_exit_code(format!("echo new {i}"), "/here".to_owned(), 0)
        }))
        .map(Arc::new)
        .collect_vec();

    let visited = Cell::new(0);
    let found = find_potential_autosuggestions_from_history(
        CountingIter {
            inner: history_entries.iter(),
            visited: &visited,
        },
        "echo",
        Some("/here"),
        &HashSet::new(),
        3,
    );
    assert_eq!(
        commands(found),
        vec![
            "echo new 9".to_owned(),
            "echo new 8".to_owned(),
            "echo new 7".to_owned()
        ]
    );
    assert_eq!(
        visited.get(),
        3,
        "scan should stop after filling the same-dir bucket, not visit all {} entries",
        OLD + NEW
    );

    // When the same-dir bucket cannot fill, the whole history is (correctly) consulted, but
    // the result is still capped and in ranked order: the three newest matches anywhere.
    let visited = Cell::new(0);
    let found = find_potential_autosuggestions_from_history(
        CountingIter {
            inner: history_entries.iter(),
            visited: &visited,
        },
        "echo",
        Some("/nowhere"),
        &HashSet::new(),
        3,
    );
    assert_eq!(
        commands(found),
        vec![
            "echo new 9".to_owned(),
            "echo new 8".to_owned(),
            "echo new 7".to_owned()
        ]
    );
    assert_eq!(visited.get(), OLD + NEW);
}

fn test_session_context(cwd: TypedPathBuf, app: &App) -> SessionContext {
    session_context_of_type(BootstrapSessionType::Local, cwd, app)
}

fn session_context_of_type(
    session_type: BootstrapSessionType,
    cwd: TypedPathBuf,
    app: &App,
) -> SessionContext {
    let session = Session::new(
        SessionInfo::new_for_test().with_session_type(session_type),
        Arc::new(TestCommandExecutor::default()),
    );
    app.read(|ctx| SessionContext::new(session, CommandRegistry::default().into(), cwd, ctx))
}

/// A `Folder` arg naming a path that does not exist on THIS machine, resolved against a
/// working directory that does not exist here either -- i.e. exactly what a warpified
/// remote session produces: the cwd is the remote host's.
fn nonexistent_relative_folder_arg() -> Spanned<ParsedExpression> {
    ParsedExpression::new(
        Expression::ValidatableArgument(vec![ArgType::Folder]),
        ParsedToken::new("Documents".to_string()),
    )
    .spanned((3, 12))
}

/// The remote cwd. Deliberately a macOS-shaped path that does not exist on the machine
/// running the test, so a local `is_dir` on `<cwd>/Documents` is guaranteed to be false.
const REMOTE_CWD: &str = "/Users/definitely-not-a-real-user-621/src";

#[test]
fn test_relative_path_arg_is_invalid_on_a_local_session_when_it_does_not_exist() {
    App::test((), |app| async move {
        let _flag = FeatureFlag::ValidateAutosuggestions.override_enabled(true);
        let ctx = session_context_of_type(
            BootstrapSessionType::Local,
            TypedPathBuf::from(REMOTE_CWD),
            &app,
        );

        // A local session's filesystem IS the one being stat'd, so a path that isn't there
        // really is evidence of a bad suggestion. This half must keep working.
        let is_valid = is_arg_valid(
            "cd Documents",
            &nonexistent_relative_folder_arg(),
            &ctx,
            None,
        )
        .await;
        assert!(!is_valid);
    });
}

#[test]
fn test_relative_path_arg_is_valid_on_a_warpified_remote_session() {
    App::test((), |app| async move {
        let _flag = FeatureFlag::ValidateAutosuggestions.override_enabled(true);
        let ctx = session_context_of_type(
            BootstrapSessionType::WarpifiedRemote,
            TypedPathBuf::from(REMOTE_CWD),
            &app,
        );

        // Same arg, same cwd, same (local) filesystem -- only the session type differs.
        // Before #621 this stat'd the client's disk, found nothing, and dropped the
        // suggestion, which is why no relative-path autosuggestion ever appeared over SSH.
        let is_valid = is_arg_valid(
            "cd Documents",
            &nonexistent_relative_folder_arg(),
            &ctx,
            None,
        )
        .await;
        assert!(is_valid);
    });
}

#[test]
fn test_feature_flag_arg_is_valid_with_no_whitespace_before_arg() {
    App::test((), |app| async move {
        let ctx = test_session_context(TypedPathBuf::from("/test/home/"), &app);

        let full_command = "cargo run --features=with_local_server,fast_dev";
        let with_local_server_arg = ParsedExpression::new(
            Expression::ValidatableArgument(vec![ArgType::Generator("feature_flags".into())]),
            ParsedToken::new("with_local_server".to_string()),
        )
        .spanned((21, 38));
        let fast_dev_arg = ParsedExpression::new(
            Expression::ValidatableArgument(vec![ArgType::Generator("feature_flags".into())]),
            ParsedToken::new("fast_dev".to_string()),
        )
        .spanned((39, 47));
        let is_valid = is_arg_valid(full_command, &with_local_server_arg, &ctx, None).await;
        assert!(is_valid);

        let is_valid = is_arg_valid(full_command, &fast_dev_arg, &ctx, None).await;
        assert!(is_valid);

        let is_valid = is_command_valid(full_command, Some(&ctx), None).await;
        assert!(is_valid);
    });
}

/// #1130: a generator arg on a warpified remote session fails open instead of running the
/// generator (a remote command) for every candidate on every keystroke.
#[test]
fn test_generator_arg_is_valid_on_a_warpified_remote_session() {
    App::test((), |app| async move {
        let _flag = FeatureFlag::ValidateAutosuggestions.override_enabled(true);
        let ctx = session_context_of_type(
            BootstrapSessionType::WarpifiedRemote,
            TypedPathBuf::from(REMOTE_CWD),
            &app,
        );
        let arg = ParsedExpression::new(
            Expression::ValidatableArgument(vec![ArgType::Generator("units".into())]),
            ParsedToken::new("nginx".to_string()),
        )
        .spanned((15, 20));
        assert!(is_arg_valid("systemctl status nginx", &arg, &ctx, None).await);
    });
}

/// Generated history (oldest first) with a handful of `rare ...` commands buried under many
/// that do not match, so a `rare` prefix has to walk far back to find them.
fn history_with_buried_matches(filler: usize) -> Vec<Arc<HistoryEntry>> {
    let mut entries = vec![
        HistoryEntry::with_pwd_and_exit_code("rare one", "/repo", 0),
        HistoryEntry::with_pwd_and_exit_code("rare two", "/elsewhere", 0),
        HistoryEntry::with_pwd_and_exit_code("rare one", "/elsewhere", 0),
    ];
    entries.extend((0..filler).map(|i| {
        HistoryEntry::with_pwd_and_exit_code(format!("filler {i}"), "/repo".to_owned(), 0)
    }));
    entries.into_iter().map(Arc::new).collect()
}

/// The UI-thread scan gives up (returning `None`, to be finished off-thread) exactly when the
/// walk needs more entries than the budget, and whenever it does finish its answer is the
/// unbudgeted one.
#[test]
fn budgeted_autosuggestion_scan_defers_long_walks_and_agrees_otherwise() {
    let history = history_with_buried_matches(500);
    let excluded = HashSet::new();
    let unbudgeted = find_potential_autosuggestions_from_history(
        history.iter(),
        "rare",
        Some("/repo"),
        &excluded,
        MAX_HISTORY_AUTOSUGGESTION_CANDIDATES,
    );
    assert_eq!(
        commands(unbudgeted.clone()),
        vec!["rare one", "rare one", "rare two"],
        "same-dir first, then the rest, each deduped to its newest"
    );

    // The matches sit 500+ entries back: a small budget must defer ...
    for budget in [1, 10, 499, 500] {
        assert!(
            scan_history_for_autosuggestions(
                history.iter(),
                "rare",
                Some("/repo"),
                &excluded,
                MAX_HISTORY_AUTOSUGGESTION_CANDIDATES,
                Some(budget),
            )
            .is_none(),
            "budget {budget}"
        );
    }
    // ... and one that covers the whole history finishes with the identical list.
    for budget in [503, 504, 10_000] {
        let within_budget = scan_history_for_autosuggestions(
            history.iter(),
            "rare",
            Some("/repo"),
            &excluded,
            MAX_HISTORY_AUTOSUGGESTION_CANDIDATES,
            Some(budget),
        )
        .expect("the budget covers the history");
        assert_eq!(commands(within_budget), commands(unbudgeted.clone()));
    }
}

#[cfg(feature = "local_fs")]
#[test]
fn similar_history_context_cache_reuses_result_per_block_until_ttl() {
    use std::cell::Cell;
    use std::time::Instant;

    let cache = SimilarHistoryContextCache::default();
    let calls = Cell::new(0);
    let compute = || {
        calls.set(calls.get() + 1);
        Vec::new()
    };
    let start = Instant::now();
    let pwd = Some("/repo".to_owned());
    let exit_ok = ExitCode::from(0);

    // Every keystroke after the same block: one query.
    for keystroke in 0..5 {
        let now = start + Duration::from_millis(keystroke * 100);
        cache.get_or_compute_at(now, "ls", &pwd, exit_ok, None, compute);
    }
    assert_eq!(calls.get(), 1);

    // A different block (command, pwd or exit code) re-queries ...
    cache.get_or_compute_at(start, "pwd", &pwd, exit_ok, None, compute);
    assert_eq!(calls.get(), 2);
    cache.get_or_compute_at(start, "pwd", &None, exit_ok, None, compute);
    assert_eq!(calls.get(), 3);
    cache.get_or_compute_at(start, "pwd", &None, ExitCode::from(1), None, compute);
    assert_eq!(calls.get(), 4);

    // ... and so does the same block once the entry has aged out.
    cache.get_or_compute_at(
        start + SIMILAR_HISTORY_CONTEXT_CACHE_TTL,
        "pwd",
        &None,
        ExitCode::from(1),
        None,
        compute,
    );
    assert_eq!(calls.get(), 5);
}
