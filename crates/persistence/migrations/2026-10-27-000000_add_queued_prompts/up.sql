-- Queued agent prompts and shell commands (#1027).
--
-- The in-memory `QueuedQueryModel` lost every queued row when the app quit or
-- crashed. Each row is mirrored here so the next launch can put it back, paused,
-- or return its text to an input box. A row exists only while it is queued:
-- sending, removing or restoring it to the input deletes it.
--
-- `conversation_id` is deliberately NOT a foreign key. A row must outlive a
-- conversation that was never saved (persistence off, crash before the first
-- write): the restore path then returns the text to an input instead of the
-- row vanishing with its parent.
--
-- `position` is the row's index in its conversation's queue. The app rewrites a
-- conversation's whole queue in one transaction, so positions are always dense.
CREATE TABLE queued_prompts (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    -- 'prompt' or 'command'.
    kind TEXT NOT NULL,
    text TEXT NOT NULL,
    -- The QueuedQueryOrigin variant name, kept for diagnosis; locked origins
    -- are restored as their unlocked counterparts.
    origin TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX ix_queued_prompts_conversation ON queued_prompts (conversation_id, position);
