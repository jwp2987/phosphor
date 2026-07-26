-- SQLite doesn't support DROP COLUMN (in older versions), so we rebuild the
-- table to roll back the field. The backup table must strictly replicate the
-- ssh_servers column definitions from up.sql
-- (`2026-05-04-120000_add_ssh_manager_tables`), including all NOT NULL constraints.
CREATE TABLE ssh_servers_backup (
  node_id           TEXT PRIMARY KEY NOT NULL REFERENCES ssh_nodes(id) ON DELETE CASCADE,
  host              TEXT NOT NULL,
  port              INTEGER NOT NULL DEFAULT 22,
  username          TEXT NOT NULL DEFAULT '',
  auth_type         TEXT NOT NULL CHECK(auth_type IN ('password','key')) DEFAULT 'password',
  key_path          TEXT,
  last_connected_at TIMESTAMP
);
INSERT INTO ssh_servers_backup SELECT node_id, host, port, username, auth_type, key_path, last_connected_at FROM ssh_servers;
DROP TABLE ssh_servers;
ALTER TABLE ssh_servers_backup RENAME TO ssh_servers;
