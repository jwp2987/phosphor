use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, Local, TimeZone as _};
use futures::future::BoxFuture;
use warp_core::SessionId;
use warpui::r#async::Timer;
use warpui::{App, AppContext, EntityId, SingletonEntity};

use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::search::data_source::{Query, QueryFilter};
use crate::search::mixer::{AddAsyncSourceOptions, AsyncDataSource, SearchMixer};
use crate::terminal::history::{UpArrowHistoryConfig, UpArrowSnapshot};
use crate::terminal::model::session::command_executor::NoOpCommandExecutor;
use crate::terminal::model::session::{Session, SessionInfo};
use crate::terminal::{History, HistoryEntry};

use crate::input_suggestions::HistoryOrder;

use super::{
    AcceptHistoryItem, MenuEntry, MenuItem, MenuQuerySnapshot, build_results_in_background,
    interleave_conversations, newest_tail, prefix_filtered_newest_tail,
};

#[test]
fn interleave_conversations_only_inserts_into_current_session_segment() {
    let t10 = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, 10).unwrap();
    let t20 = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, 20).unwrap();
    let t50 = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, 50).unwrap();
    let t100 = Local.with_ymd_and_hms(2024, 1, 1, 0, 1, 40).unwrap();
    let t150 = Local.with_ymd_and_hms(2024, 1, 1, 0, 2, 30).unwrap();
    let t200 = Local.with_ymd_and_hms(2024, 1, 1, 0, 3, 20).unwrap();

    let base = vec![
        MenuEntry {
            order: HistoryOrder::DifferentSession,
            sort_timestamp: t10,
            item: MenuItem::Command {
                command: "d10".to_string(),
                display_timestamp: t10,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::DifferentSession,
            sort_timestamp: t20,
            item: MenuItem::Command {
                command: "d20".to_string(),
                display_timestamp: t20,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t100,
            item: MenuItem::Command {
                command: "c100".to_string(),
                display_timestamp: t100,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t200,
            item: MenuItem::Command {
                command: "c200".to_string(),
                display_timestamp: t200,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
    ];

    let conversations = vec![
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t150,
            item: MenuItem::Command {
                command: "conv150".to_string(),
                display_timestamp: t150,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t50,
            item: MenuItem::Command {
                command: "conv50".to_string(),
                display_timestamp: t50,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
    ];

    let merged = interleave_conversations(base, conversations);

    let commands = merged
        .iter()
        .map(|e| match &e.item {
            MenuItem::Command { command, .. } => command.as_str(),
            MenuItem::Conversation { title, .. } => title.as_str(),
        })
        .collect::<Vec<_>>();

    // DifferentSession entries stay at the top; conversations are inserted into the CurrentSession segment.
    assert_eq!(
        commands,
        vec!["d10", "d20", "conv50", "c100", "conv150", "c200"]
    );
}

fn command_entry(order: HistoryOrder, name: &str, secs: u32) -> MenuEntry {
    let ts = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, secs).unwrap();
    MenuEntry {
        order,
        sort_timestamp: ts,
        item: MenuItem::Command {
            command: name.to_string(),
            display_timestamp: ts,
            linked_workflow_data: None,
            prefix_match_len: 0,
        },
    }
}

fn names(entries: &[MenuEntry]) -> Vec<&str> {
    entries
        .iter()
        .map(|e| match &e.item {
            MenuItem::Command { command, .. } => command.as_str(),
            MenuItem::Conversation { title, .. } => title.as_str(),
        })
        .collect()
}

#[test]
fn interleave_conversations_edge_orderings() {
    use HistoryOrder::{CurrentSession as Cur, DifferentSession as Diff};

    // Conversations older than everything land first in the current segment, newer ones last,
    // and a conversation with an equal timestamp goes before the base entry (strict `<`).
    let base = vec![
        command_entry(Diff, "d1", 1),
        command_entry(Cur, "c10", 10),
        command_entry(Cur, "c20", 20),
    ];
    let conversations = vec![
        command_entry(Cur, "new", 30),
        command_entry(Cur, "tie", 20),
        command_entry(Cur, "old", 5),
    ];
    let merged = interleave_conversations(base, conversations);
    assert_eq!(
        names(&merged),
        vec!["d1", "old", "c10", "tie", "c20", "new"]
    );

    // No current-session entries: every conversation is appended after the base.
    let base = vec![command_entry(Diff, "d1", 1), command_entry(Diff, "d2", 2)];
    let merged = interleave_conversations(base, vec![command_entry(Cur, "conv", 0)]);
    assert_eq!(names(&merged), vec!["d1", "d2", "conv"]);

    // No conversations: base is unchanged.
    let base = vec![command_entry(Diff, "d1", 1), command_entry(Cur, "c2", 2)];
    let merged = interleave_conversations(base, vec![]);
    assert_eq!(names(&merged), vec!["d1", "c2"]);
}

#[test]
fn newest_tail_keeps_only_the_newest_in_order() {
    let items: Vec<u32> = (0..10).collect();
    assert_eq!(newest_tail(items, 3).collect::<Vec<_>>(), vec![7, 8, 9]);
    assert_eq!(newest_tail(vec![1, 2], 5).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(newest_tail(Vec::<u32>::new(), 0).count(), 0);
}

#[test]
fn prefix_filter_runs_before_the_cap_so_old_matches_stay_reachable() {
    // Oldest first: one old match, then 600 newer non-matching rows.
    let mut items = vec!["cargo build --old".to_string()];
    items.extend((0..600).map(|i| format!("ls {i}")));

    let got: Vec<String> =
        prefix_filtered_newest_tail(items.clone(), "cargo", 500, |s| s.as_str()).collect();
    assert_eq!(got, vec!["cargo build --old".to_string()]);

    // Empty prefix still caps to the newest 500.
    let all: Vec<String> = prefix_filtered_newest_tail(items, "", 500, |s| s.as_str()).collect();
    assert_eq!(all.len(), 500);
    assert_eq!(all.last().unwrap(), "ls 599");
}

// ---- #874: the up-arrow list is built off the main thread ----

const SESSION: u64 = 1;

/// Sets up a `History` with `histfile` shell-history-file lines and `session` commands that
/// belong to the current session, and the empty AI history model the snapshot reads.
async fn setup_history(app: &mut App, histfile: Vec<String>, session: Vec<HistoryEntry>) {
    let session_id = SessionId::from(SESSION);
    let mut info = SessionInfo::new_for_test();
    info.session_id = session_id;
    let session_obj = Arc::new(Session::new(info, Arc::new(NoOpCommandExecutor::default())));
    let (tx, rx) = async_channel::bounded(1);
    let history = app.add_singleton_model(|_| History::default());
    app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], vec![], &[]));
    app.update(|ctx| {
        ctx.subscribe_to_model(&history, move |_, event, _| {
            if matches!(event, crate::terminal::HistoryEvent::Initialized(id) if *id == session_id)
            {
                let _ = tx.try_send(());
            }
        });
        history.update(ctx, |history, ctx| {
            history.init_session_with(session_obj, async move { histfile }, ctx);
        });
    });
    rx.recv().await.expect("history initializes");
    history.update(app, |history, _| {
        history.append_commands(session_id, session)
    });
}

/// A synthetic history of `n` entries: a quarter of them repeat earlier text (so dedupe has work
/// to do), half come from the shell history file and half from the current session.
fn synthetic(n: usize) -> (Vec<String>, Vec<HistoryEntry>) {
    let session_id = SessionId::from(SESSION);
    let base = Local::now();
    let text = |i: usize| format!("cmd {} --flag {}", i % (n * 3 / 4), i % 7);
    let histfile = (0..n / 2).map(text).collect();
    let session = (n / 2..n)
        .map(|i| {
            let mut entry = HistoryEntry::command_only(text(i));
            entry.session_id = Some(session_id);
            entry.start_ts = Some(base + ChronoDuration::milliseconds(i as i64));
            entry
        })
        .collect();
    (histfile, session)
}

fn capture(app: &AppContext, include_prompts: bool) -> UpArrowSnapshot {
    UpArrowSnapshot::capture(
        History::handle(app).as_ref(app),
        EntityId::new(),
        Some(SessionId::from(SESSION)),
        UpArrowHistoryConfig {
            include_commands: true,
            include_prompts,
        },
        app,
    )
}

/// The snapshot path must produce the same suggestions, in the same order, as the synchronous
/// `History` API it replaced on the menu's path, for the empty prefix and a typed one.
#[test]
fn snapshot_suggestions_match_synchronous_path() {
    App::test((), |mut app| async move {
        let (histfile, session) = synthetic(2_000);
        setup_history(&mut app, histfile, session).await;
        app.read(|ctx| {
            let tv = EntityId::new();
            let config = UpArrowHistoryConfig {
                include_commands: true,
                include_prompts: true,
            };
            for prefix in [None, Some("cmd 1"), Some("cmd 17"), Some("nomatch")] {
                let snapshot = UpArrowSnapshot::capture(
                    History::handle(ctx).as_ref(ctx),
                    tv,
                    Some(SessionId::from(SESSION)),
                    config,
                    ctx,
                );
                let old = History::handle(ctx)
                    .as_ref(ctx)
                    .up_arrow_suggestions_with_command_prefix(
                        tv,
                        Some(SessionId::from(SESSION)),
                        config,
                        prefix,
                        ctx,
                    );
                let new = snapshot.suggestions(prefix);
                assert!(prefix == Some("nomatch") || !old.is_empty());
                assert_eq!(new, old, "prefix {prefix:?}");
            }
        });
    });
}

/// With a 200k-entry history the main-thread capture must only share the entries, never copy
/// them, and the background build must equal the inline one. (A wall-clock ratio was flaky on a
/// loaded box, so the "main thread stays small" claim is asserted structurally.)
#[test]
fn main_thread_capture_shares_entries_for_a_huge_history() {
    App::test((), |mut app| async move {
        let (histfile, session) = synthetic(200_000);
        setup_history(&mut app, histfile, session).await;

        let (snapshot, visible) = app.read(|ctx| {
            let history = History::handle(ctx).as_ref(ctx);
            let visible = history
                .visible_commands(SessionId::from(SESSION))
                .map_or(0, |commands| commands.count());
            (capture(ctx, true), visible)
        });
        let captured = snapshot.command_snapshot();
        assert_eq!(captured.len(), visible);
        assert!(visible >= 190_000, "visible {visible}");
        // The history still owns each entry, so a count above one means the snapshot holds a
        // shared handle, not a copy of the command text.
        assert!(
            captured.iter().all(|entry| Arc::strong_count(entry) > 1),
            "capture copied an entry"
        );

        let suggestions = snapshot.suggestions(None).len();
        assert!(suggestions > 100_000, "dedupe left {suggestions}");

        // Same rows from the background path as from running the build inline.
        let (inline, background) = {
            let (a, b) = app.read(|ctx| (menu_snapshot(ctx, ""), menu_snapshot(ctx, "")));
            let background = app.background_executor();
            (
                a.build_results(),
                build_results_in_background(&background, b).await,
            )
        };
        assert_eq!(rows(&inline), rows(&background));
        assert_eq!(inline.len(), super::MAX_MENU_HISTORY_ROWS);
    });
}

/// A build cancelled before it starts yields nothing and does not wedge the next one.
#[test]
fn cancelled_build_returns_none() {
    App::test((), |mut app| async move {
        let (histfile, session) = synthetic(2_000);
        setup_history(&mut app, histfile, session).await;
        let snapshot = app.read(|ctx| menu_snapshot(ctx, ""));
        assert!(snapshot.build_results_cancellable(&|| true).is_none());
        let snapshot = app.read(|ctx| menu_snapshot(ctx, ""));
        assert!(snapshot.build_results_cancellable(&|| false).is_some());
    });
}

fn menu_snapshot(app: &AppContext, query: &str) -> MenuQuerySnapshot {
    MenuQuerySnapshot {
        is_agent_view: false,
        include_commands: true,
        up_arrow: capture(app, false),
        trimmed_query: query.trim().to_string(),
        session_id: Some(SessionId::from(SESSION)),
        conversations: Vec::new(),
    }
}

fn rows(results: &[crate::search::data_source::QueryResult<AcceptHistoryItem>]) -> Vec<String> {
    results
        .iter()
        .map(|r| format!("{:?}", r.accept_result()))
        .collect()
}

/// Test source mirroring the real one: inline for small histories, otherwise snapshot on the
/// main thread and build in the background after an optional artificial delay.
struct SnapshotSource {
    delay: Duration,
}

type TestResults = Result<
    Vec<crate::search::data_source::QueryResult<AcceptHistoryItem>>,
    crate::search::mixer::DataSourceRunErrorWrapper,
>;

impl AsyncDataSource for SnapshotSource {
    type Action = AcceptHistoryItem;

    fn run_query_inline(&self, query: &Query, app: &AppContext) -> Option<TestResults> {
        let bound = History::handle(app)
            .as_ref(app)
            .visible_commands_upper_bound(SessionId::from(SESSION));
        (bound <= super::INLINE_BUILD_MAX_ENTRIES)
            .then(|| Ok(menu_snapshot(app, &query.text).build_results()))
    }

    fn run_query(&self, query: &Query, app: &AppContext) -> BoxFuture<'static, TestResults> {
        let snapshot = menu_snapshot(app, &query.text);
        let background = app.background_executor().clone();
        let delay = self.delay;
        Box::pin(async move {
            Timer::after(delay).await;
            Ok(build_results_in_background(&background, snapshot).await)
        })
    }
}

fn add_source(
    mixer: &warpui::ModelHandle<SearchMixer<AcceptHistoryItem>>,
    app: &mut App,
    delay: Duration,
) {
    mixer.update(app, |mixer, ctx| {
        mixer.set_initial_results_timeout(Duration::from_secs(60));
        mixer.add_async_source(
            SnapshotSource { delay },
            [QueryFilter::Commands],
            AddAsyncSourceOptions {
                debounce_interval: None,
                run_in_zero_state: true,
                run_when_unfiltered: true,
            },
            ctx,
        );
    });
}

fn commands_query(text: &str) -> Query {
    Query {
        text: text.to_string(),
        filters: HashSet::from([QueryFilter::Commands]),
    }
}

/// A small history is answered inline: the rows exist the moment `run_query` returns, with no
/// loading state, exactly like the old synchronous source.
#[test]
fn small_history_is_built_inline_with_rows_immediately() {
    App::test((), |mut app| async move {
        let (histfile, session) = synthetic(1_000);
        setup_history(&mut app, histfile, session).await;
        let mixer = app.add_model(|_| SearchMixer::<AcceptHistoryItem>::new());
        add_source(&mixer, &mut app, Duration::ZERO);
        mixer.update(&mut app, |mixer, ctx| {
            mixer.run_query(commands_query(""), ctx)
        });
        // No await between run_query and the assertions.
        mixer.read(&app, |m, _| {
            assert!(
                !m.is_loading(),
                "small history must not enter a loading state"
            );
            assert!(!m.results().is_empty());
        });
        mixer.update(&mut app, |mixer, ctx| {
            mixer.run_query(commands_query("cmd 1"), ctx)
        });
        mixer.read(&app, |m, _| {
            assert!(!m.is_loading());
            assert!(
                m.results()
                    .iter()
                    .all(|r| { format!("{:?}", r.accept_result()).contains("cmd 1") })
            );
        });
    });
}

/// A large history takes the async path: right after `run_query` the mixer is loading, the
/// previous rows are not what a key would act on, and the empty 500 ms commit does not fire.
#[test]
fn large_history_stays_loading_past_the_default_timeout() {
    App::test((), |mut app| async move {
        let (histfile, session) = synthetic(20_000);
        setup_history(&mut app, histfile, session).await;
        let mixer = app.add_model(|_| SearchMixer::<AcceptHistoryItem>::new());
        add_source(&mixer, &mut app, Duration::from_millis(900));
        mixer.update(&mut app, |mixer, ctx| {
            mixer.run_query(commands_query(""), ctx)
        });
        mixer.read(&app, |m, _| {
            assert!(m.is_loading());
            assert!(m.results().is_empty());
        });
        // Past the 500 ms default initial-results timeout, still loading, still no empty commit.
        Timer::after(Duration::from_millis(700)).await;
        mixer.read(&app, |m, _| {
            assert!(m.is_loading(), "an empty commit would read as No results");
            assert!(m.results().is_empty());
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        while mixer.read(&app, |m, _| m.is_loading()) && Instant::now() < deadline {
            Timer::after(Duration::from_millis(50)).await;
        }
        mixer.read(&app, |m, _| {
            assert_eq!(m.results().len(), super::MAX_MENU_HISTORY_ROWS)
        });
    });
}

/// Keys pressed while the list is pending are queued in order and never applied to the rows
/// on screen; once it commits they are handed back to be replayed on the fresh rows.
#[test]
fn keys_before_commit_are_queued_never_applied() {
    use super::super::view::{KeyGate, PendingKey};
    let mut gate = KeyGate::default();
    assert_eq!(gate.offer(PendingKey::Up, true), None);
    assert_eq!(gate.offer(PendingKey::Up, true), None);
    assert_eq!(gate.offer(PendingKey::Accept, true), None);
    assert_eq!(gate.offer(PendingKey::Down, true), None);
    assert_eq!(
        gate.take_queued(),
        vec![
            PendingKey::Up,
            PendingKey::Up,
            PendingKey::Accept,
            PendingKey::Down
        ]
    );
    assert!(gate.take_queued().is_empty());
    // Not pending (the small-history path): applied immediately, nothing queued.
    assert_eq!(gate.offer(PendingKey::Up, false), Some(PendingKey::Up));
    assert!(gate.take_queued().is_empty());
}

/// A slow older query (full 200k sort) superseded by a fast newer one must never overwrite the
/// newer results when it finally completes.
#[test]
fn stale_history_results_are_dropped() {
    App::test((), |mut app| async move {
        let (mut histfile, session) = synthetic(200_000);
        histfile.push("zzz only-here".to_string());
        setup_history(&mut app, histfile, session).await;

        let mixer = app.add_model(|_| SearchMixer::<AcceptHistoryItem>::new());
        let query = |text: &str| Query {
            text: text.to_string(),
            filters: HashSet::from([QueryFilter::Commands]),
        };
        mixer.update(&mut app, |mixer, ctx| {
            mixer.add_async_source(
                SnapshotSource {
                    delay: Duration::ZERO,
                },
                [QueryFilter::Commands],
                AddAsyncSourceOptions {
                    debounce_interval: None,
                    run_in_zero_state: true,
                    run_when_unfiltered: true,
                },
                ctx,
            );
            mixer.run_query(query(""), ctx);
            mixer.run_query(query("zzz"), ctx);
        });

        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            Timer::after(Duration::from_millis(50)).await;
            if !mixer.read(&app, |m, _| m.is_loading()) || Instant::now() > deadline {
                break;
            }
        }
        // Give the superseded full-history build time to finish and (wrongly) deliver.
        Timer::after(Duration::from_secs(2)).await;

        mixer.read(&app, |m, _| {
            let got = rows(m.results());
            assert_eq!(got.len(), 1, "{got:?}");
            assert!(got[0].contains("zzz only-here"), "{got:?}");
        });
    });
}
