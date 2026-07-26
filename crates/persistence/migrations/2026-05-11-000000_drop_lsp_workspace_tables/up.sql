-- Drop the two tables for LSP persistence and "visited git repo history".
-- workspace_language_server references workspace_metadata via a workspace_id FK,
-- so the child table must be dropped first.
DROP TABLE IF EXISTS workspace_language_server;
DROP TABLE IF EXISTS workspace_metadata;
