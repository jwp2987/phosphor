-- Restore the `workspace_language_server` table.
--
-- It was dropped by `2026-05-11-000000_drop_lsp_workspace_tables` together with
-- `workspace_metadata`, because the whole `lsp` crate went out at the same time
-- (`efcaa42b8`). `2026-08-09-000100_restore_workspace_metadata` brought the
-- parent table back for `PersistedWorkspace`'s recent-repositories list and
-- explicitly deferred this child table until the `lsp` crate returned. It has:
-- `crates/lsp` is restored, so `LSPServerType` — the key this table stores in
-- `language_server_name` — exists again.
--
-- This is a NEW forward migration. The 2026-05-11 drop is left untouched: it has
-- already run on every existing user database, and editing or reverting it in
-- place would leave those databases inconsistent with the recorded migration
-- history.
--
-- Column definitions are copied verbatim from
-- `2025-11-11-230915_change_workspace_language_server_enabled_to_text/up.sql`
-- (the last upstream shape of this table, after `enabled` moved from BOOLEAN to
-- TEXT to hold the `EnablementState` enum), with one deliberate addition
-- described below. A database that predates the drop and one created after this
-- migration therefore agree on every column, type and nullability.
--
-- ON DELETE CASCADE — deliberate divergence from the pin's DDL.
-- ------------------------------------------------------------
-- Upstream declares this FK without a referential action. `PRAGMA foreign_keys`
-- is ON for every connection (`app/src/persistence/sqlite.rs`), so deleting a
-- `workspace_metadata` row upstream is simply *rejected* while children exist —
-- and the startup query `inner_join`s the two tables, so any child row that did
-- become orphaned would be silently dropped by that join and a user's *enabled*
-- language servers would read back as *disabled*, with no error anywhere.
--
-- Upstream defends against that in application code:
-- `PersistedWorkspace::clean_up_expired_metadata` refuses to delete a workspace
-- row that still holds Yes/No language-server entries. That guard is NOT yet
-- restored in this fork (see the `LSP SEAM` note on that function in
-- `app/src/ai/persisted_workspace.rs`), so the schema must not depend on it.
--
-- CASCADE makes the orphan state unrepresentable rather than merely unlikely: a
-- workspace row and its language-server rows now live and die together, and the
-- inner_join can never silently downgrade an enabled server. It is strictly
-- narrower than the failure it replaces, but it is NOT a substitute for the
-- upstream guard: CASCADE deletes the user's per-workspace LSP choice along with
-- the workspace, whereas upstream's guard preserves the choice by keeping the
-- workspace row alive. Both are needed for full parity. Restoring
-- `clean_up_expired_metadata`'s third arm is tracked with the rest of the
-- `PersistedWorkspace` LSP leg.
CREATE TABLE IF NOT EXISTS workspace_language_server (
    id integer NOT NULL PRIMARY KEY,
    workspace_id integer NOT NULL,
    language_server_name TEXT NOT NULL,
    enabled TEXT NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspace_metadata (id) ON DELETE CASCADE
);
