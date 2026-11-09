use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use crate::proto::{Initialize, ServerMessage, session_scoped_request};
use crate::protocol::RequestId;

use super::*;
use crate::daemon::server::{ConnId, Server, next_conn_id};

fn write(home: &Path, rel: &str, body: &str) {
    let path = home.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Atomic, so a scan between the create and the write never reads an empty file.
    let tmp = path.with_extension("tmp-fixture");
    std::fs::write(&tmp, body).unwrap();
    std::fs::rename(tmp, path).unwrap();
}

#[test]
fn scan_finds_skills_one_level_down_in_every_provider_dir_and_the_rule_file() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    write(h, ".agents/skills/b/SKILL.md", "B");
    write(h, ".agents/skills/a/SKILL.md", "A");
    write(h, ".claude/skills/c/SKILL.md", "C");
    write(h, ".agents/AGENTS.md", "rules");
    // Not skills: a hidden directory, a file at the wrong depth, a directory without SKILL.md.
    write(h, ".agents/skills/.hidden/SKILL.md", "no");
    write(h, ".agents/skills/x/y/SKILL.md", "no");
    write(h, ".agents/skills/empty/README.md", "no");
    write(h, ".agents/skills/loose.md", "no");
    let found = scan(h);
    let paths: Vec<_> = found.skills.iter().map(|f| f.path.clone()).collect();
    assert_eq!(
        paths,
        vec![
            lossy(&h.join(".agents/skills/a/SKILL.md")),
            lossy(&h.join(".agents/skills/b/SKILL.md")),
            lossy(&h.join(".claude/skills/c/SKILL.md")),
        ]
    );
    assert_eq!(found.rules.len(), 1);
    assert_eq!(found.rules[0].bytes, b"rules");
    // Watched: home, the provider dirs that exist, their skill roots and each skill dir.
    for dir in [
        h.to_path_buf(),
        h.join(".agents"),
        h.join(".claude"),
        h.join(".agents/skills"),
        h.join(".agents/skills/a"),
        h.join(".claude/skills/c"),
    ] {
        assert!(found.watch_dirs.contains(&dir), "{dir:?}");
    }
    assert!(!found.watch_dirs.contains(&h.join(".codex")));
}

#[test]
fn scan_of_an_empty_home_is_empty_but_watches_the_home() {
    let home = tempfile::tempdir().unwrap();
    let found = scan(home.path());
    assert!(found.skills.is_empty() && found.rules.is_empty());
    assert_eq!(found.watch_dirs.len(), 1);
}

struct Rig {
    server: Server,
    events: Receiver<Event>,
}

fn rig(home: &Path) -> Rig {
    let (tx, events) = channel();
    let server = Server::new(Duration::from_secs(600))
        .with_home(home.to_path_buf())
        .with_events(tx);
    Rig { server, events }
}

impl Rig {
    fn connect(&mut self) -> (ConnId, async_channel::Receiver<ServerMessage>) {
        let (tx, rx) = async_channel::unbounded();
        let conn = next_conn_id();
        self.server.handle_event(Event::Connected {
            conn,
            tx,
            chunking: Arc::new(AtomicBool::new(false)),
        });
        (conn, rx)
    }

    fn initialize(&mut self, conn: ConnId) {
        self.server.handle_message(
            conn,
            crate::proto::ClientMessage::session_scoped(
                RequestId::new().to_string(),
                session_scoped_request::Message::Initialize(Initialize::default()),
            ),
        );
    }

    /// Runs the core until `rx` has an agent-context snapshot with revision >= `min`.
    fn snapshot(
        &mut self,
        rx: &async_channel::Receiver<ServerMessage>,
        min: u64,
    ) -> RemoteAgentContextSnapshot {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            while let Ok(msg) = rx.try_recv() {
                if let Some(server_message::Message::RemoteAgentContextSnapshot(s)) = msg.message
                    && s.revision >= min
                {
                    return s;
                }
            }
            if let Ok(event) = self.events.recv_timeout(Duration::from_millis(50)) {
                self.server.handle_event(event);
            }
        }
        panic!("no agent context snapshot with revision >= {min}");
    }
}

#[test]
fn initialize_pushes_the_snapshot_after_the_response_and_a_new_skill_bumps_the_revision() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path().canonicalize().unwrap();
    write(&h, ".agents/skills/a/SKILL.md", "A");
    write(&h, ".claude/skills/b/SKILL.md", "B");
    write(&h, ".agents/AGENTS.md", "rules");
    let mut rig = rig(&h);
    let (conn, rx) = rig.connect();
    rig.initialize(conn);
    // The response comes first.
    let first = async_io::block_on(rx.recv()).unwrap();
    assert!(matches!(
        first.message,
        Some(server_message::Message::InitializeResponse(_))
    ));
    let one = rig.snapshot(&rx, 1);
    assert_eq!(one.revision, 1);
    assert_eq!(one.home_dir, lossy(&h));
    assert_eq!(one.skills.len(), 2);
    assert_eq!(one.global_rules.len(), 1);

    // A skill appears on disk: the watcher drives a new, higher revision with all three.
    write(&h, ".agents/skills/c/SKILL.md", "C");
    let two = rig.snapshot(&rx, 2);
    assert!(two.revision > one.revision);
    assert_eq!(two.skills.len(), 3);

    // A second connection is handed the current snapshot at once, no new revision.
    let (conn2, rx2) = rig.connect();
    rig.initialize(conn2);
    let again = rig.snapshot(&rx2, 1);
    assert_eq!(again.revision, two.revision);
    assert_eq!(rig.server_agent_revision(), two.revision);
}

impl Rig {
    fn server_agent_revision(&self) -> u64 {
        self.server
            .agent_current_for_tests()
            .map_or(0, |s| s.revision)
    }
}
