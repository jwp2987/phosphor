-- Reverses `up.sql` by dropping the restored table again.
--
-- Safe to run in isolation: `workspace_language_server` is a leaf — it
-- references `workspace_metadata` but nothing references it — so dropping it
-- orphans nothing. The parent table is left in place; it is owned by
-- `2026-08-09-000100_restore_workspace_metadata`, which has its own down.sql.
DROP TABLE IF EXISTS workspace_language_server;
