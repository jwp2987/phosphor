-- Reverses `up.sql` by dropping the restored table again. Safe to run because
-- `workspace_language_server` (the only table that ever referenced
-- `workspace_metadata` by foreign key) remains dropped, so nothing is orphaned.
DROP TABLE IF EXISTS workspace_metadata;
