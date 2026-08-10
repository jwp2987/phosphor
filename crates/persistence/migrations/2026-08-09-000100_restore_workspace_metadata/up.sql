-- Restore the `workspace_metadata` table.
--
-- It was dropped by `2026-05-11-000000_drop_lsp_workspace_tables` together with
-- `workspace_language_server`, because `PersistedWorkspace` (the only writer)
-- was removed at the same time. `PersistedWorkspace` is back, so the table that
-- backs "recently used repositories" has to come back with it.
--
-- Only `workspace_metadata` is restored. `workspace_language_server` stays
-- dropped: it is keyed by `LSPServerType`, a type that lives in the `lsp` crate
-- which this fork deleted entirely. Restoring that table without the crate would
-- create a schema no code can read or write. See
-- `app/src/ai/persisted_workspace.rs` (LSP seam) for what a future port needs.
--
-- Column definitions are copied verbatim from
-- `2025-10-31-201353_add_workspace_language_server/up.sql`, so a database that
-- predates the drop and one created after this migration end up identical.
CREATE TABLE IF NOT EXISTS workspace_metadata (
    id integer NOT NULL PRIMARY KEY,
    repo_path TEXT NOT NULL UNIQUE,
    navigated_ts DATETIME,
    modified_ts DATETIME,
    queried_ts DATETIME
);
