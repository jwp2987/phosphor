use crate::{
    ai::agent::conversation::AIConversationId,
    terminal::{
        History,
        model::{
            block::{AgentInteractionMetadata, SerializedAIMetadata, SerializedBlock},
            bootstrap::BootstrapStage,
            session::command_executor::testing::TestCommandExecutor,
            session::{BootstrapSessionType, Session, SessionId, SessionInfo},
            test_utils::TestBlockBuilder,
        },
        shell::ShellType,
    },
    test_util::{Stub, VirtualFS},
};
use chrono::Local;
use futures::Future;
use futures::future::join_all;
use futures_lite::StreamExt;
use itertools::Itertools;
use warp_core::command::ExitCode;
use warpui::{App, ModelHandle};

use std::collections::HashSet;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::Arc;

use super::{
    HistoryEntry, HistoryEvent, HostCommandIndex, LoadedHistoryFile, PersistedCommand,
    ReadHistoryFileState, ShellHost,
};

impl History {
    /// Returns a Future that completes when `History` is initialized for all sessions with IDs in
    /// the given `session_ids` vector.
    pub async fn initialized_sessions(
        history_handle: &mut ModelHandle<History>,
        app: &mut App,
        session_ids: Vec<SessionId>,
    ) {
        let mut history_initialization_receivers = vec![];
        for session_id in session_ids {
            let is_session_initialized = history_handle.read(app, |history, _| {
                history.is_session_initialized(&session_id)
            });
            if !is_session_initialized {
                let (tx, rx) = async_channel::unbounded();
                let history_handle_clone = history_handle.clone();
                history_handle.update(app, move |_, ctx| {
                    ctx.subscribe_to_model(&history_handle_clone, move |_, event, _| {
                        let HistoryEvent::Initialized(event_id) = event;
                        if session_id == *event_id {
                            let _ = tx.try_send(());
                        }
                    });
                });
                history_initialization_receivers.push(rx);
            }
        }

        join_all(
            history_initialization_receivers
                .into_iter()
                .map(|rx| async move { rx.recv().await }),
        )
        .await;
    }
}

impl HistoryEntry {
    fn with_session_id<S: Into<String>>(session_id: SessionId, command: S) -> Self {
        Self {
            session_id: Some(session_id),
            exit_code: Some(ExitCode::from(0)),
            command: command.into(),
            workflow_id: None,
            workflow_command: None,
            pwd: None,
            start_ts: None,
            completed_ts: None,
            git_head: None,
            shell_host: None,
            is_agent_executed: false,
            is_for_restored_block: false,
        }
    }

    pub fn with_pwd_and_exit_code<S: Into<String>>(
        command: S,
        pwd: S,
        exit_code: impl Into<ExitCode>,
    ) -> Self {
        Self {
            session_id: None,
            exit_code: Some(exit_code.into()),
            command: command.into(),
            workflow_id: None,
            workflow_command: None,
            pwd: Some(pwd.into()),
            start_ts: None,
            completed_ts: None,
            git_head: None,
            shell_host: None,
            is_agent_executed: false,
            is_for_restored_block: false,
        }
    }
}

#[test]
fn history_entry_for_restored_block_preserves_agent_execution() {
    let mut block = TestBlockBuilder::new()
        .with_bootstrap_stage(BootstrapStage::RestoreBlocks)
        .build();
    block.set_agent_interaction_mode_for_requested_command(
        String::from("action-id").into(),
        None,
        AIConversationId::new(),
    );

    let entry = HistoryEntry::for_restored_block("ls".to_string(), &block);

    assert!(entry.is_agent_executed);
}

#[test]
fn history_entry_for_restored_block_does_not_treat_all_agent_interactions_as_agent_execution() {
    let mut block = TestBlockBuilder::new()
        .with_bootstrap_stage(BootstrapStage::RestoreBlocks)
        .build();
    block.set_agent_interaction_mode(AgentInteractionMetadata::new(
        None,
        AIConversationId::new(),
        None,
        None,
        false,
        false,
    ));

    let entry = HistoryEntry::for_restored_block("ls".to_string(), &block);

    assert!(!entry.is_agent_executed);
}

#[test]
fn history_entry_for_completed_block_preserves_agent_execution() {
    let ai_metadata = serde_json::to_string(&SerializedAIMetadata::from(
        AgentInteractionMetadata::new_hidden(
            String::from("action-id").into(),
            AIConversationId::new(),
        ),
    ))
    .unwrap();
    let block = SerializedBlock {
        ai_metadata: Some(ai_metadata),
        ..SerializedBlock::new_for_test("ls".as_bytes().to_vec(), vec![])
    };

    let entry = HistoryEntry::for_completed_block("ls".to_string(), &block);

    assert!(entry.is_agent_executed);
}

#[test]
fn history_entry_for_completed_block_does_not_treat_all_agent_interactions_as_agent_execution() {
    let ai_metadata = serde_json::to_string(&SerializedAIMetadata::from(
        AgentInteractionMetadata::new(None, AIConversationId::new(), None, None, false, false),
    ))
    .unwrap();
    let block = SerializedBlock {
        ai_metadata: Some(ai_metadata),
        ..SerializedBlock::new_for_test("ls".as_bytes().to_vec(), vec![])
    };

    let entry = HistoryEntry::for_completed_block("ls".to_string(), &block);

    assert!(!entry.is_agent_executed);
}

/// Initializes history for testing
/// initialization is complete.
///
/// `history_commands_fn` is an async function that should emulate the async operation of reading
/// command from a session's history file.
///
/// `session_commands_to_append` are commands that are appended to the in-memory session history
/// after commands have been read from the history file.
async fn initialize_history_for_testing<F>(
    history_handle: &mut ModelHandle<History>,
    session: Arc<Session>,
    history_commands_fn: F,
    session_commands_to_append: Vec<String>,
    app: &mut App,
) where
    F: 'static + Future<Output = Vec<String>> + Send,
{
    let session_id = session.id();
    history_handle.update(app, move |history, ctx| {
        history.init_session_with(session, history_commands_fn, ctx);
    });
    History::initialized_sessions(history_handle, app, vec![session_id]).await;
    history_handle.update(app, move |history, _| {
        history.append_commands(
            session_id,
            session_commands_to_append
                .into_iter()
                .map(|command| HistoryEntry::with_session_id(session_id, command))
                .collect_vec(),
        );
    });
}

/// `History::visible_commands` is the lazy, double-ended source behind `commands()` and the
/// per-keystroke autosuggestion scan (#811), which walks it from the back. Build the
/// concatenation `histfile ++ session_commands` by hand with every visibility rule in play --
/// a skip index in each half (the session-half one exercises the `idx + histfile_len`
/// offset), a restored-block entry before the session start index, and an entry from another
/// session after it -- and check the reverse walk is exactly `commands()` reversed.
#[test]
fn visible_commands_reverse_walk_matches_commands() {
    let session_a = SessionId::from(1u64);
    let session_b = SessionId::from(2u64);
    let shell_host = ShellHost {
        shell_type: ShellType::Bash,
        user: "user".to_string(),
        hostname: "host".to_string(),
    };

    // Indices into the concatenation, oldest first.
    let histfile_commands = vec![
        Arc::new(HistoryEntry::command_only("cd ~")), // 0
        Arc::new(HistoryEntry::command_only("ls")),   // 1
        Arc::new(HistoryEntry::command_only("ls")),   // 2: deduped -> skipped
    ];
    let histfile_len = histfile_commands.len();
    let mut restored = HistoryEntry::with_session_id(session_a, "vim notes");
    restored.is_for_restored_block = true;
    let session_commands = vec![
        Arc::new(restored), // 3: restored, before the start index, still visible to A
        Arc::new(HistoryEntry::with_session_id(session_b, "make")), // 4: other session
        Arc::new(HistoryEntry::with_session_id(session_a, "git status")), // 5
        Arc::new(HistoryEntry::with_session_id(session_a, "git status")), // 6: deduped
    ];

    let mut history = History::default();
    history
        .history_file_commands
        .insert(shell_host.clone(), Arc::new(histfile_commands));
    history
        .session_commands
        .insert(shell_host.clone(), session_commands);
    history
        .session_id_to_shell_host
        .insert(session_a, shell_host);
    history
        .session_start_indices
        .insert(session_a, histfile_len + 1);
    history
        .session_skip_indices
        .insert(session_a, HashSet::from([2, histfile_len + 3]));

    let forward = history.commands(session_a).unwrap();
    assert_eq!(
        forward
            .iter()
            .map(|entry| entry.command.as_str())
            .collect_vec(),
        vec!["cd ~", "ls", "vim notes", "git status"]
    );

    let reverse = history
        .visible_commands(session_a)
        .unwrap()
        .rev()
        .map(Arc::as_ref)
        .collect_vec();
    assert_eq!(reverse, forward.iter().rev().copied().collect_vec());
    assert_eq!(history.len(session_a), forward.len());

    // A session the history has never heard of yields nothing from either accessor.
    assert!(history.visible_commands(session_b).is_none());
    assert!(history.commands(session_b).is_none());
}

#[test]
fn test_append_commands() {
    VirtualFS::test("history_append_command", |dirs, mut sandbox| {
        App::test((), |mut app| async move {
            sandbox.with_files(vec![Stub::FileWithContentToBeTrimmed(
                ".bash_history",
                r#"
                    ls
                    pwd
                    warp --listen --ports=8080,8081
                "#,
            )]);

            let mut history_handle = app.add_model(|_| History::default());
            let file = Some(dirs.tests().join(".bash_history").display().to_string());
            let session = Arc::new(Session::new(
                SessionInfo::new_for_test()
                    .with_histfile(file)
                    .with_shell_type(ShellType::Bash),
                Arc::new(TestCommandExecutor::default()),
            ));

            let session_clone = session.clone();
            initialize_history_for_testing(
                &mut history_handle,
                session.clone(),
                async move { session_clone.read_history(false).await },
                vec![
                    "pwd".to_owned(),
                    "ls".to_owned(),
                    "pwd".to_owned(),
                    "git status".to_owned(),
                ],
                &mut app,
            )
            .await;

            history_handle.read(&app, |history, _ctx| {
                assert_eq!(
                    history.commands(session.id()).unwrap_or_default(),
                    vec![
                        &HistoryEntry::command_only("warp --listen --ports=8080,8081"),
                        &HistoryEntry::with_session_id(session.id(), "ls"),
                        &HistoryEntry::with_session_id(session.id(), "pwd"),
                        &HistoryEntry::with_session_id(session.id(), "git status"),
                    ]
                );
            });
        });
    });
}

#[cfg_attr(windows, ignore = "TODO(CORE-3626)")]
#[test]
fn test_append_multiple_sessions() {
    VirtualFS::test("append_multiple_sessions", |dirs, mut sandbox| {
        App::test((), |mut app| async move {
            sandbox.with_files(vec![Stub::FileWithContentToBeTrimmed(
                ".bash_history",
                r#"
                    cd warp
                    cargo run --bin dev
                "#,
            )]);

            let mut history_handle = app.add_model(|_| History::default());
            let file = Some(dirs.tests().join(".bash_history").display().to_string());
            let session = Arc::new(Session::new(
                SessionInfo::new_for_test()
                    .with_histfile(file)
                    .with_shell_type(ShellType::Bash),
                Arc::new(TestCommandExecutor::default()),
            ));

            let session_clone = session.clone();
            initialize_history_for_testing(
                &mut history_handle,
                session.clone(),
                async move { session_clone.read_history(false).await },
                vec![
                    "ls target/".to_owned(),
                    "cargo clean".to_owned(),
                    "ls target/".to_owned(),
                ],
                &mut app,
            )
            .await;

            history_handle.read(&app, |history, _ctx| {
                assert_eq!(
                    history.commands(session.id()).unwrap_or_default(),
                    vec![
                        &HistoryEntry::command_only("cd warp"),
                        &HistoryEntry::command_only("cargo run --bin dev"),
                        &HistoryEntry::with_session_id(session.id(), "cargo clean"),
                        &HistoryEntry::with_session_id(session.id(), "ls target/"),
                    ]
                );
            });

            let second_session = Arc::new(Session::new(
                SessionInfo::new_for_test().with_id(1),
                Arc::new(TestCommandExecutor::default()),
            ));

            initialize_history_for_testing(
                &mut history_handle,
                second_session.clone(),
                async move { Vec::new() },
                vec!["ls target/".to_owned()],
                &mut app,
            )
            .await;

            history_handle.read(&app, |history, _ctx| {
                assert_eq!(
                    history.commands(second_session.id()).unwrap_or_default(),
                    vec![
                        &HistoryEntry::command_only("cd warp"),
                        &HistoryEntry::command_only("cargo run --bin dev"),
                        &HistoryEntry::with_session_id(session.id(), "cargo clean"),
                        &HistoryEntry::with_session_id(second_session.id(), "ls target/"),
                    ]
                );
            });
        });
    });
}

#[test]
fn test_len() {
    VirtualFS::test("history_len", |dirs, mut sandbox| {
        App::test((), |mut app| async move {
            sandbox.with_files(vec![Stub::FileWithContentToBeTrimmed(
                ".bash_history",
                r#"
                    cd warp
                    cargo run --bin dev
                    touch
                "#,
            )]);

            let mut history_handle = app.add_model(|_| History::default());
            let file = Some(dirs.tests().join(".bash_history").display().to_string());
            let session = Arc::new(Session::new(
                SessionInfo::new_for_test()
                    .with_histfile(file)
                    .with_shell_type(ShellType::Bash),
                Arc::new(TestCommandExecutor::default()),
            ));

            let session_clone = session.clone();
            initialize_history_for_testing(
                &mut history_handle,
                session.clone(),
                async move { session_clone.read_history(false).await },
                vec!["ls".to_owned(), "echo 'hello'".to_owned()],
                &mut app,
            )
            .await;

            history_handle.read(&app, |history, _ctx| {
                assert_eq!(history.len(session.id()), 5);
            });

            history_handle.update(&mut app, |history, _ctx| {
                history.append_commands(
                    session.id(),
                    vec![HistoryEntry::with_session_id(
                        session.id(),
                        "cargo run --bin dev".to_string(),
                    )],
                );
            });

            history_handle.read(&app, |history, _ctx| {
                assert_eq!(history.len(session.id()), 5);

                assert_eq!(
                    history.commands(session.id()).unwrap_or_default(),
                    vec![
                        &HistoryEntry::command_only("cd warp"),
                        &HistoryEntry::command_only("touch"),
                        &HistoryEntry::with_session_id(session.id(), "ls"),
                        &HistoryEntry::with_session_id(session.id(), "echo 'hello'"),
                        &HistoryEntry::with_session_id(session.id(), "cargo run --bin dev")
                    ]
                );
            });
        });
    });
}

#[test]
fn test_multiple_shells() {
    VirtualFS::test("multiple_shells", |dirs, mut sandbox| {
        App::test((), |mut app| async move {
            sandbox.with_files(vec![
                Stub::FileWithContentToBeTrimmed(
                    ".bash_history",
                    r#"
                    bash-cmd1
                    bash-cmd2
                "#,
                ),
                Stub::FileWithContentToBeTrimmed(
                    ".zsh_history",
                    r#"
                    zsh-cmd1
                    zsh-cmd2
                "#,
                ),
            ]);

            let mut history_handle = app.add_model(|_| History::default());

            let root = dirs.tests();
            let (bash_file, zsh_file) = (
                Some(root.join(".bash_history").display().to_string()),
                Some(root.join(".zsh_history").display().to_string()),
            );

            let (bash_session, zsh_session) = (
                Arc::new(Session::new(
                    SessionInfo::new_for_test()
                        .with_id(0)
                        .with_histfile(bash_file)
                        .with_shell_type(ShellType::Bash),
                    Arc::new(TestCommandExecutor::default()),
                )),
                Arc::new(Session::new(
                    SessionInfo::new_for_test()
                        .with_id(1)
                        .with_histfile(zsh_file)
                        .with_shell_type(ShellType::Zsh),
                    Arc::new(TestCommandExecutor::default()),
                )),
            );

            let bash_session_clone = bash_session.clone();
            initialize_history_for_testing(
                &mut history_handle,
                bash_session.clone(),
                async move { bash_session_clone.read_history(false).await },
                vec!["bash-cmd3".to_owned()],
                &mut app,
            )
            .await;

            let zsh_session_clone = zsh_session.clone();
            initialize_history_for_testing(
                &mut history_handle,
                zsh_session.clone(),
                async move { zsh_session_clone.read_history(false).await },
                vec!["zsh-cmd3".to_owned()],
                &mut app,
            )
            .await;

            history_handle.read(&app, |history, _ctx| {
                assert_eq!(history.len(bash_session.id()), 3);
                assert_eq!(history.len(zsh_session.id()), 3);
            });

            history_handle.update(&mut app, |history, _ctx| {
                history.append_commands(
                    bash_session.id(),
                    vec![HistoryEntry::with_session_id(
                        bash_session.id(),
                        "bash-cmd4".to_string(),
                    )],
                );
            });
            history_handle.read(&app, |history, _ctx| {
                assert_eq!(history.len(bash_session.id()), 4);
                assert_eq!(history.len(zsh_session.id()), 3);
            });

            history_handle.update(&mut app, |history, _ctx| {
                history.append_commands(
                    zsh_session.id(),
                    vec![HistoryEntry::with_session_id(
                        zsh_session.id(),
                        "zsh-cmd4".to_string(),
                    )],
                );
            });
            history_handle.read(&app, |history, _ctx| {
                assert_eq!(history.len(bash_session.id()), 4);
                assert_eq!(history.len(zsh_session.id()), 4);
            });
        });
    });
}

#[test]
fn test_multiple_machines() {
    App::test((), |mut app| async move {
        let session_a = Arc::new(Session::new(
            SessionInfo::new_for_test()
                .with_id(0)
                .with_shell_type(ShellType::Zsh)
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_hostname("prod".to_string())
                .with_user("user".to_string())
                .with_ssh_socket_path(PathBuf::from("~/.ssh/12345"))
                .with_home_dir("/users/warpuser".to_owned()),
            Arc::new(TestCommandExecutor::default()),
        ));

        let session_b = Arc::new(Session::new(
            SessionInfo::new_for_test()
                .with_id(1)
                .with_shell_type(ShellType::Zsh)
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_hostname("dev".to_string())
                .with_user("user2".to_string())
                .with_ssh_socket_path(PathBuf::from("~/.ssh/12345"))
                .with_home_dir("/users/warpuser".to_owned()),
            Arc::new(TestCommandExecutor::default()),
        ));

        let mut history_handle = app.add_model(|_| History::default());
        initialize_history_for_testing(
            &mut history_handle,
            session_a.clone(),
            async { vec!["p1".to_string(), "p2".to_string()] },
            vec!["p3".to_string()],
            &mut app,
        )
        .await;
        initialize_history_for_testing(
            &mut history_handle,
            session_b.clone(),
            async { vec!["d1".to_string(), "d2".to_string()] },
            vec!["d3".to_string()],
            &mut app,
        )
        .await;

        history_handle.read(&app, |history, _ctx| {
            assert_eq!(history.len(session_a.id()), 3);
            assert_eq!(history.len(session_b.id()), 3);
        });

        history_handle.update(&mut app, |history, _ctx| {
            history.append_commands(
                session_a.id(),
                vec![HistoryEntry::with_session_id(
                    session_a.id(),
                    "p4".to_string(),
                )],
            );
        });
        history_handle.read(&app, |history, _ctx| {
            assert_eq!(history.len(session_a.id()), 4);
            assert_eq!(history.len(session_b.id()), 3);
        });

        history_handle.update(&mut app, |history, _ctx| {
            history.append_commands(
                session_b.id(),
                vec![HistoryEntry::with_session_id(
                    session_b.id(),
                    "d4".to_string(),
                )],
            );
        });
        history_handle.read(&app, |history, _ctx| {
            assert_eq!(history.len(session_a.id()), 4);
            assert_eq!(history.len(session_b.id()), 4);
        });
    });
}

#[test]
fn test_sessions_same_shell_same_machine() {
    App::test((), |mut app| async move {
        let session_a = Arc::new(Session::new(
            SessionInfo::new_for_test()
                .with_id(0)
                .with_shell_type(ShellType::Zsh),
            Arc::new(TestCommandExecutor::default()),
        ));
        let session_b = Arc::new(Session::new(
            SessionInfo::new_for_test()
                .with_id(1)
                .with_shell_type(ShellType::Zsh),
            Arc::new(TestCommandExecutor::default()),
        ));

        let mut history_handle = app.add_model(|_| History::default());
        initialize_history_for_testing(
            &mut history_handle,
            session_a.clone(),
            async { vec!["z1".to_string(), "z2".to_string()] },
            vec!["z3".to_string()],
            &mut app,
        )
        .await;
        initialize_history_for_testing(
            &mut history_handle,
            session_b.clone(),
            async { vec![] },
            vec!["z4".to_string()],
            &mut app,
        )
        .await;

        history_handle.read(&app, |history, _ctx| {
            assert_eq!(history.len(session_a.id()), 3);
            assert_eq!(history.len(session_b.id()), 4);
        });

        history_handle.update(&mut app, |history, _ctx| {
            history.append_commands(
                session_a.id(),
                vec![HistoryEntry::with_session_id(session_a.id(), "z4")],
            );
        });

        history_handle.read(&app, |history, _ctx| {
            assert_eq!(history.len(session_a.id()), 4);
            assert_eq!(history.len(session_b.id()), 4);
        });

        history_handle.update(&mut app, |history, _ctx| {
            history.append_commands(
                session_b.id(),
                vec![HistoryEntry::with_session_id(session_b.id(), "z5")],
            );
        });
        history_handle.read(&app, |history, _ctx| {
            assert_eq!(history.len(session_a.id()), 4);
            assert_eq!(history.len(session_b.id()), 5);
        });

        history_handle.update(&mut app, |history, _ctx| {
            history.append_commands(
                session_b.id(),
                vec![HistoryEntry::with_session_id(session_b.id(), "z5")],
            );
        });
        history_handle.read(&app, |history, _ctx| {
            assert_eq!(history.len(session_a.id()), 4);
            assert_eq!(history.len(session_b.id()), 5);
        });
    });
}

#[test]
fn test_sessions_no_dupes_new_session() {
    App::test((), |mut app| async move {
        let session_a = Arc::new(Session::new(
            SessionInfo::new_for_test()
                .with_id(0)
                .with_shell_type(ShellType::Zsh),
            Arc::new(TestCommandExecutor::default()),
        ));
        let session_b = Arc::new(Session::new(
            SessionInfo::new_for_test()
                .with_id(1)
                .with_shell_type(ShellType::Zsh),
            Arc::new(TestCommandExecutor::default()),
        ));
        let session_c = Arc::new(Session::new(
            SessionInfo::new_for_test()
                .with_id(2)
                .with_shell_type(ShellType::Zsh),
            Arc::new(TestCommandExecutor::default()),
        ));

        let mut history_handle = app.add_model(|_| History::default());

        initialize_history_for_testing(
            &mut history_handle,
            session_a.clone(),
            async { vec!["z1".to_string(), "z2".to_string()] },
            vec!["z3".to_string()],
            &mut app,
        )
        .await;
        initialize_history_for_testing(
            &mut history_handle,
            session_b.clone(),
            async { vec![] },
            vec!["z2".to_string()],
            &mut app,
        )
        .await;

        history_handle.read(&app, |history, _ctx| {
            assert_eq!(
                history.commands(session_a.id()).unwrap_or_default(),
                vec![
                    &HistoryEntry::command_only("z1"),
                    &HistoryEntry::command_only("z2"),
                    &HistoryEntry::with_session_id(session_a.id(), "z3"),
                ]
            );
            assert_eq!(
                history.commands(session_b.id()).unwrap_or_default(),
                vec![
                    &HistoryEntry::command_only("z1"),
                    &HistoryEntry::with_session_id(session_a.id(), "z3"),
                    &HistoryEntry::with_session_id(session_b.id(), "z2"),
                ]
            );
        });

        initialize_history_for_testing(
            &mut history_handle,
            session_c.clone(),
            async { vec![] },
            vec!["z4".to_string()],
            &mut app,
        )
        .await;

        history_handle.read(&app, |history, _ctx| {
            assert_eq!(
                history.commands(session_c.id()).unwrap_or_default(),
                vec![
                    &HistoryEntry::command_only("z1"),
                    &HistoryEntry::with_session_id(session_a.id(), "z3"),
                    &HistoryEntry::with_session_id(session_b.id(), "z2"),
                    &HistoryEntry::with_session_id(session_c.id(), "z4")
                ]
            );
        });
    });
}

#[cfg_attr(windows, ignore = "TODO(CORE-3626)")]
#[test]
fn append_command_with_rich_history_data() {
    App::test((), |mut app| async move {
        let session = Arc::new(Session::new(
            SessionInfo::new_for_test().with_id(0),
            Arc::new(TestCommandExecutor::default()),
        ));

        let start_ts_1 = Local::now();
        let end_ts_1 = Local::now();
        let start_ts_2 = Local::now();
        let end_ts_2 = Local::now();
        let start_ts_3 = Local::now();
        let end_ts_3 = Local::now();
        let start_ts_4 = Local::now();
        let end_ts_4 = Local::now();

        let shell_host = ShellHost {
            shell_type: ShellType::Bash,
            user: String::from("local:user"),
            hostname: String::from("local:host"),
        };
        let persisted_commands = vec![
            PersistedCommand {
                id: 0,
                command: String::from("ls"),
                exit_code: Some(ExitCode::from(0)),
                start_ts: Some(start_ts_1),
                completed_ts: Some(end_ts_1),
                pwd: Some(String::from("/")),
                shell_host: Some(shell_host.clone()),
                session_id: None,
                git_branch: None,
                workflow_id: None,
                workflow_command: None,
                is_agent_executed: false,
            },
            PersistedCommand {
                id: 0,
                command: String::from("date"),
                exit_code: Some(ExitCode::from(0)),
                start_ts: Some(start_ts_2),
                completed_ts: Some(end_ts_2),
                pwd: Some(String::from("/usr/bin")),
                shell_host: Some(shell_host.clone()),
                session_id: None,
                git_branch: Some(String::from("foobar")),
                workflow_id: None,
                workflow_command: None,
                is_agent_executed: false,
            },
        ];

        let mut history_handle = app.add_model(|_| History::new(persisted_commands));
        initialize_history_for_testing(
            &mut history_handle,
            session.clone(),
            async {
                vec![
                    "cd ~/Desktop".to_string(),
                    "ls".to_string(),
                    "date".to_string(),
                ]
            },
            Vec::new(),
            &mut app,
        )
        .await;

        history_handle.update(&mut app, |history, _ctx| {
            history.append_commands(
                session.id(),
                vec![
                    HistoryEntry {
                        session_id: Some(session.id()),
                        command: String::from("touch foobar"),
                        pwd: Some(String::from("/Users/andy/")),
                        start_ts: Some(start_ts_3),
                        completed_ts: Some(end_ts_3),
                        workflow_id: None,
                        workflow_command: None,
                        exit_code: Some(ExitCode::from(0)),
                        git_head: None,
                        shell_host: None,
                        is_agent_executed: false,
                        is_for_restored_block: false,
                    },
                    HistoryEntry {
                        session_id: Some(session.id()),
                        command: String::from("ls"),
                        pwd: Some(String::from("/Users/andy/")),
                        start_ts: Some(start_ts_4),
                        completed_ts: Some(end_ts_4),
                        workflow_id: None,
                        workflow_command: None,
                        exit_code: Some(ExitCode::from(0)),
                        git_head: None,
                        shell_host: None,
                        is_agent_executed: false,
                        is_for_restored_block: false,
                    },
                ],
            );
        });

        history_handle.read(&app, |history, _ctx| {
            assert_eq!(
                history.commands(session.id()).unwrap_or_default(),
                vec![
                    &HistoryEntry::command_only("cd ~/Desktop"),
                    &HistoryEntry {
                        session_id: None,
                        command: String::from("date"),
                        pwd: Some(String::from("/usr/bin")),
                        start_ts: Some(start_ts_2),
                        completed_ts: Some(end_ts_2),
                        workflow_id: None,
                        workflow_command: None,
                        exit_code: Some(ExitCode::from(0)),
                        git_head: Some(String::from("foobar")),
                        shell_host: Some(shell_host.clone()),
                        is_agent_executed: false,
                        is_for_restored_block: false,
                    },
                    &HistoryEntry {
                        session_id: Some(session.id()),
                        command: String::from("touch foobar"),
                        pwd: Some(String::from("/Users/andy/")),
                        start_ts: Some(start_ts_3),
                        completed_ts: Some(end_ts_3),
                        workflow_id: None,
                        workflow_command: None,
                        exit_code: Some(ExitCode::from(0)),
                        git_head: None,
                        shell_host: None,
                        is_agent_executed: false,
                        is_for_restored_block: false,
                    },
                    &HistoryEntry {
                        session_id: Some(session.id()),
                        command: String::from("ls"),
                        pwd: Some(String::from("/Users/andy/")),
                        start_ts: Some(start_ts_4),
                        completed_ts: Some(end_ts_4),
                        workflow_id: None,
                        workflow_command: None,
                        exit_code: Some(ExitCode::from(0)),
                        git_head: None,
                        shell_host: None,
                        is_agent_executed: false,
                        is_for_restored_block: false,
                    },
                ]
            );
        });
    });
}

#[test]
fn append_restored_command_doesnt_overwrite_rich_history() {
    App::test((), |mut app| async move {
        let session = Arc::new(Session::new(
            SessionInfo::new_for_test().with_id(0),
            Arc::new(TestCommandExecutor::default()),
        ));
        let start_ts = Local::now();
        let end_ts = Local::now();

        let shell_host = ShellHost {
            shell_type: ShellType::Bash,
            user: String::from("local:user"),
            hostname: String::from("local:host"),
        };
        let persisted_commands = vec![PersistedCommand {
            id: 0,
            command: String::from("ls"),
            exit_code: Some(ExitCode::from(0)),
            start_ts: Some(start_ts),
            completed_ts: Some(end_ts),
            pwd: Some(String::from("/tmp")),
            shell_host: Some(shell_host),
            session_id: None,
            git_branch: None,
            workflow_id: None,
            workflow_command: None,
            is_agent_executed: false,
        }];
        let mut history_handle = app.add_model(|_| History::new(persisted_commands));
        initialize_history_for_testing(
            &mut history_handle,
            session.clone(),
            async { vec!["cd ~/Desktop".to_string(), "ls".to_string()] },
            Vec::new(),
            &mut app,
        )
        .await;

        history_handle.update(&mut app, |history, _ctx| {
            history.append_restored_commands(
                session.id(),
                vec![HistoryEntry {
                    session_id: Some(session.id()),
                    command: "ls".to_string(),
                    pwd: Some("/tmp".to_string()),
                    start_ts: Some(start_ts),
                    completed_ts: Some(end_ts),
                    workflow_id: None,
                    workflow_command: None,
                    exit_code: Some(ExitCode::from(0)),
                    git_head: None,
                    shell_host: None,
                    is_agent_executed: false,
                    is_for_restored_block: true,
                }],
            );
        });

        history_handle.read(&app, |history, _ctx| {
            assert_eq!(
                history.commands(session.id()).unwrap_or_default(),
                vec![
                    &HistoryEntry::command_only("cd ~/Desktop"),
                    &HistoryEntry {
                        session_id: Some(session.id()),
                        command: String::from("ls"),
                        pwd: Some(String::from("/tmp")),
                        start_ts: Some(start_ts),
                        completed_ts: Some(end_ts),
                        workflow_id: None,
                        workflow_command: None,
                        exit_code: Some(ExitCode::from(0)),
                        git_head: None,
                        shell_host: None,
                        is_agent_executed: false,
                        is_for_restored_block: true,
                    },
                ]
            )
        });
    });
}

#[test]
fn is_appendable_vs_is_queryable() {
    App::test((), |mut app| async move {
        let mut history_handle = app.add_model(|_| History::new(vec![]));
        let (tx, rx) = async_channel::bounded(1);

        let session = Arc::new(Session::new(
            SessionInfo::new_for_test().with_id(0),
            Arc::new(TestCommandExecutor::default()),
        ));
        let session_id = session.id();
        history_handle.update(&mut app, move |history, ctx| {
            history.init_session_with(
                session,
                async move {
                    pin!(rx).next().await;
                    vec![]
                },
                ctx,
            );
        });

        // When the session has just been registered with the history model,
        // its history will be appendable but not queryable.
        history_handle.read(&app, |history, _| {
            assert!(history.is_appendable(&session_id));
            assert!(!history.is_queryable(&session_id));
        });

        // Simulate the asynchronous reading of the histfile.
        tx.send(()).await.unwrap();
        History::initialized_sessions(&mut history_handle, &mut app, vec![session_id]).await;

        // Once we've read the histfile, the model should be appendable and queryable.
        history_handle.read(&app, |history, _| {
            assert!(history.is_appendable(&session_id));
            assert!(history.is_queryable(&session_id));
        });
    });
}

#[test]
fn forget_session_drops_per_session_bookkeeping() {
    // #843: nothing used to remove a closed session's entries from these maps.
    App::test((), |mut app| async move {
        let mut history_handle = app.add_model(|_| History::new(vec![]));
        let session = Arc::new(Session::new(
            SessionInfo::new_for_test().with_id(0),
            Arc::new(TestCommandExecutor::default()),
        ));
        let session_id = session.id();
        history_handle.update(&mut app, move |history, ctx| {
            history.init_session_with(session, async { vec![] }, ctx);
        });
        History::initialized_sessions(&mut history_handle, &mut app, vec![session_id]).await;
        history_handle.read(&app, |history, _| {
            assert!(history.is_appendable(&session_id));
            assert!(history.is_queryable(&session_id));
            assert!(history.all_live_session_ids().contains(&session_id));
        });

        history_handle.update(&mut app, |history, _| history.forget_session(session_id));

        history_handle.read(&app, |history, _| {
            assert!(!history.is_appendable(&session_id));
            assert!(!history.is_queryable(&session_id));
            assert!(!history.all_live_session_ids().contains(&session_id));
        });
    });
}

#[test]
fn new_keeps_newest_metadata_and_counts_duplicates() {
    // Persisted commands arrive newest-first (`ORDER BY id DESC`), so the first-seen entry for a
    // command is the most recent run and must be the one kept.
    let shell_host = ShellHost {
        shell_type: ShellType::Bash,
        user: String::from("local:user"),
        hostname: String::from("local:host"),
    };
    let persisted = |command: &str, pwd: &str| PersistedCommand {
        id: 0,
        command: command.to_owned(),
        exit_code: Some(ExitCode::from(0)),
        start_ts: Some(Local::now()),
        completed_ts: Some(Local::now()),
        pwd: Some(pwd.to_owned()),
        shell_host: Some(shell_host.clone()),
        session_id: None,
        git_branch: None,
        workflow_id: None,
        workflow_command: None,
        is_agent_executed: false,
    };
    let history = History::new(vec![
        persisted("ls", "/newest"),
        persisted("date", "/only"),
        persisted("ls", "/middle"),
        persisted("ls", "/oldest"),
    ]);

    let mut summaries = history.command_summaries(String::from("local:host"));
    summaries.sort_by(|a, b| a.1.command.cmp(&b.1.command));
    assert_eq!(summaries.len(), 2);

    let (date_count, date_entry) = summaries[0];
    assert_eq!(date_entry.command, "date");
    assert_eq!(date_count, 1);

    let (ls_count, ls_entry) = summaries[1];
    assert_eq!(ls_entry.command, "ls");
    assert_eq!(ls_count, 3);
    assert_eq!(ls_entry.pwd.as_deref(), Some("/newest"));
}

/// Deterministic pseudo-random command names from a small alphabet, so duplicates (within a
/// batch, against the history file and against earlier session commands) are common. About one
/// in eight is empty, which `append_commands` filters out.
fn generated_commands(seed: u64, count: usize) -> Vec<String> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            match (state >> 33) % 24 {
                0..=2 => String::new(),
                n => format!("cmd{n}"),
            }
        })
        .collect()
}

fn generated_history_host() -> ShellHost {
    ShellHost {
        shell_type: ShellType::Bash,
        user: "user".to_string(),
        hostname: "host".to_string(),
    }
}

/// The pre-#1140 `append_commands` skip computation: hash every entry of the history for every
/// append. Returns the indices it would mark as skipped for the appending session.
fn reference_append_skip_indices(
    histfile: &[Arc<HistoryEntry>],
    session_commands: &[Arc<HistoryEntry>],
    commands: &[HistoryEntry],
) -> HashSet<usize> {
    let mut skip = HashSet::new();
    let mut commands_set = HashSet::new();
    let mut commands_set_skip_indices = Vec::new();
    for (idx, command) in commands.iter().enumerate().rev() {
        if commands_set.contains(command.command.as_str()) || command.command.is_empty() {
            commands_set_skip_indices.push(idx);
        } else {
            commands_set.insert(command.command.as_str());
        }
    }
    let mut last_index = 0;
    for (idx, h) in histfile.iter().chain(session_commands.iter()).enumerate() {
        if commands_set.contains(h.command.as_str()) {
            skip.insert(idx);
        }
        last_index = idx;
    }
    for idx in commands_set_skip_indices {
        skip.insert(last_index + idx + 1);
    }
    skip
}

/// The pre-#1140 per-session init dedupe: materialise `commands()` and hash all of it.
fn reference_initial_skip_indices(history: &History, session_id: SessionId) -> HashSet<usize> {
    let mut seen_commands: HashSet<&str> = HashSet::new();
    let mut skip_index_set = HashSet::new();
    for (idx, &entry) in history
        .commands(session_id)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .rev()
    {
        if seen_commands.contains(entry.command.as_str()) {
            skip_index_set.insert(idx);
        } else {
            seen_commands.insert(entry.command.as_str());
        }
    }
    skip_index_set
}

/// The incremental command index must yield exactly the skip set the old full rescan did, on
/// every append of a long run of generated batches (duplicates, empty commands, a populated
/// history file and pre-existing session commands included), and also when the History is
/// built by hand with no index at all.
#[test]
fn append_commands_incremental_index_matches_full_rescan() {
    for seed in 0..8u64 {
        for prebuilt_index in [true, false] {
            let session_id = SessionId::from(1u64);
            let host = generated_history_host();
            let loaded = LoadedHistoryFile::build(
                crate::util::dedupe_from_last(generated_commands(seed, 40 * (seed as usize))),
                None,
            );
            let mut history = History::default();
            if prebuilt_index {
                history.command_indices.insert(
                    host.clone(),
                    HostCommandIndex::with_histfile(loaded.index, loaded.entries.len()),
                );
            }
            history
                .history_file_commands
                .insert(host.clone(), Arc::new(loaded.entries.clone()));
            history.session_commands.insert(host.clone(), vec![]);
            history
                .session_id_to_shell_host
                .insert(session_id, host.clone());
            history
                .read_history_file_state
                .insert(host.clone(), ReadHistoryFileState::Done);

            let mut reference_skip: HashSet<usize> = HashSet::new();
            let mut reference_session: Vec<Arc<HistoryEntry>> = vec![];
            for batch in 0..30u64 {
                let size = 1 + ((seed + batch) % 4) as usize;
                let commands: Vec<HistoryEntry> = generated_commands(seed * 1000 + batch, size)
                    .into_iter()
                    .map(|command| HistoryEntry::with_session_id(session_id, command))
                    .collect();
                reference_skip.extend(reference_append_skip_indices(
                    &loaded.entries,
                    &reference_session,
                    &commands,
                ));
                reference_session.extend(commands.iter().cloned().map(Arc::new));

                history.append_commands(session_id, commands);

                assert_eq!(
                    history
                        .session_skip_indices
                        .get(&session_id)
                        .cloned()
                        .unwrap_or_default(),
                    reference_skip,
                    "seed {seed} batch {batch} prebuilt_index {prebuilt_index}"
                );
            }
            assert_eq!(
                history.session_commands[&host]
                    .iter()
                    .map(|entry| entry.command.clone())
                    .collect_vec(),
                reference_session
                    .iter()
                    .map(|entry| entry.command.clone())
                    .collect_vec()
            );
        }
    }
}

/// The per-session init dedupe, which now consults the index instead of hashing the history
/// file, must mark exactly the indices the old scan over `commands()` did -- including the case
/// where restored blocks and another session's commands are filtered out of the visible list
/// (which shifts the positions the old scan reported).
#[test]
fn initial_skip_indices_match_full_scan() {
    for seed in 0..12u64 {
        let session_id = SessionId::from(1u64);
        let other_session = SessionId::from(2u64);
        let host = generated_history_host();
        let loaded = LoadedHistoryFile::build(
            crate::util::dedupe_from_last(generated_commands(seed, 60)),
            None,
        );
        let histfile_len = loaded.entries.len();
        let session_commands: Vec<Arc<HistoryEntry>> = generated_commands(seed + 100, 30)
            .into_iter()
            .enumerate()
            .map(|(i, command)| {
                let owner = if i % 3 == 0 {
                    other_session
                } else {
                    session_id
                };
                let mut entry = HistoryEntry::with_session_id(owner, command);
                entry.is_for_restored_block = i % 5 == 0;
                Arc::new(entry)
            })
            .collect();
        // Sessions start after the history file and some of the session commands.
        let start_index = histfile_len + (seed as usize % 7);

        let mut history = History::default();
        history.command_indices.insert(
            host.clone(),
            HostCommandIndex::with_histfile(loaded.index, histfile_len),
        );
        history
            .history_file_commands
            .insert(host.clone(), Arc::new(loaded.entries));
        history
            .session_commands
            .insert(host.clone(), session_commands);
        history
            .session_id_to_shell_host
            .insert(session_id, host.clone());
        history
            .session_start_indices
            .insert(session_id, start_index);
        history
            .session_skip_indices
            .insert(session_id, HashSet::new());

        let reference = reference_initial_skip_indices(&history, session_id);
        let actual = history.initial_skip_indices(session_id, &host, start_index);
        assert_eq!(actual, reference, "seed {seed}");
    }
}

/// The owned snapshot the deferred autosuggestion scan walks off-thread yields exactly what
/// `visible_commands` does, both ways round, with every visibility rule in play.
#[test]
fn visible_commands_snapshot_matches_visible_commands() {
    let session_id = SessionId::from(1u64);
    let other_session = SessionId::from(2u64);
    let host = generated_history_host();
    let loaded = LoadedHistoryFile::build(
        crate::util::dedupe_from_last(generated_commands(7, 80)),
        None,
    );
    let histfile_len = loaded.entries.len();
    let session_commands: Vec<Arc<HistoryEntry>> = generated_commands(8, 40)
        .into_iter()
        .enumerate()
        .map(|(i, command)| {
            let owner = if i % 3 == 0 {
                other_session
            } else {
                session_id
            };
            let mut entry = HistoryEntry::with_session_id(owner, command);
            entry.is_for_restored_block = i % 5 == 0;
            Arc::new(entry)
        })
        .collect();

    let mut history = History::default();
    history
        .history_file_commands
        .insert(host.clone(), Arc::new(loaded.entries));
    history
        .session_commands
        .insert(host.clone(), session_commands);
    history
        .session_id_to_shell_host
        .insert(session_id, host.clone());
    history
        .session_start_indices
        .insert(session_id, histfile_len + 3);
    history.session_skip_indices.insert(
        session_id,
        HashSet::from([1, 4, histfile_len + 2, histfile_len + 9]),
    );

    let snapshot = history.visible_commands_snapshot(session_id).unwrap();
    let expected = history
        .visible_commands(session_id)
        .unwrap()
        .cloned()
        .collect_vec();
    assert!(expected.len() > 10);
    assert_eq!(snapshot.iter().cloned().collect_vec(), expected);
    assert_eq!(
        snapshot.iter().rev().collect_vec(),
        history
            .visible_commands(session_id)
            .unwrap()
            .rev()
            .collect_vec()
    );

    // The snapshot is a copy: later appends do not change it.
    history
        .read_history_file_state
        .insert(host.clone(), ReadHistoryFileState::Done);
    history.append_commands(
        session_id,
        vec![HistoryEntry::with_session_id(session_id, "cmd5")],
    );
    assert_eq!(snapshot.iter().cloned().collect_vec(), expected);

    assert!(history.visible_commands_snapshot(other_session).is_none());
}

/// #1203: what the history says about a line run verbatim. The shell's own history file counts
/// without an exit code; a Phosphor-recorded entry needs exit 0; the newest entry with a recorded
/// exit code decides, so a known failure after an older success does not count and a later success
/// does again.
#[test]
fn ran_before_follows_the_newest_recorded_exit_code() {
    App::test((), |mut app| async move {
        let mut history_handle = app.add_model(|_| History::default());
        let session = Arc::new(Session::new(
            SessionInfo::new_for_test().with_shell_type(ShellType::Bash),
            Arc::new(TestCommandExecutor::default()),
        ));
        let session_id = session.id();
        initialize_history_for_testing(
            &mut history_handle,
            session.clone(),
            async move {
                vec![
                    "git log --since yesterday".to_owned(),
                    "say hello".to_owned(),
                ]
            },
            vec![],
            &mut app,
        )
        .await;
        history_handle.read(&app, |history, _| {
            assert_eq!(
                history.ran_before(session_id, "git log --since yesterday"),
                super::RanBefore::Succeeded,
                "a history-file line counts without an exit code"
            );
            assert_eq!(
                history.ran_before(session_id, "never typed"),
                super::RanBefore::Never
            );
        });

        // Phosphor records the line again; until it exits nothing changes, a failure counts
        // against it, a later success for it.
        let run = |app: &mut App, command: &str, at_ms: i64, exit: Option<i32>| {
            let start_ts = Local::now() + chrono::Duration::milliseconds(at_ms);
            history_handle.update(app, |history, _| {
                history.append_commands(
                    session_id,
                    vec![HistoryEntry::command_at_time(
                        command.to_owned(),
                        start_ts,
                        Some(session_id),
                        false,
                    )],
                );
                if let Some(exit) = exit {
                    history.mark_command_as_finished(
                        session_id,
                        start_ts,
                        start_ts,
                        ExitCode::from(exit),
                    );
                }
            });
        };
        run(&mut app, "git log --since yesterday", 1000, None);
        history_handle.read(&app, |history, _| {
            assert_eq!(
                history.ran_before(session_id, "git log --since yesterday"),
                super::RanBefore::Succeeded,
                "a running Phosphor entry is neutral; the history file still counts"
            );
        });
        run(&mut app, "git log --since yesterday", 2000, Some(128));
        history_handle.read(&app, |history, _| {
            assert_eq!(
                history.ran_before(session_id, "git log --since yesterday"),
                super::RanBefore::Failed
            );
        });
        run(&mut app, "git log --since yesterday", 3000, Some(0));
        history_handle.read(&app, |history, _| {
            assert_eq!(
                history.ran_before(session_id, "git log --since yesterday"),
                super::RanBefore::Succeeded
            );
        });
        // A line only Phosphor recorded, without an exit code, does not count.
        run(&mut app, "cargo build", 4000, None);
        history_handle.read(&app, |history, _| {
            assert_eq!(
                history.ran_before(session_id, "cargo build"),
                super::RanBefore::Never
            );
        });
    });
}
