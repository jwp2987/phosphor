-- Reverses `up.sql`. Dropping it loses only prompts still queued at the time.
DROP INDEX IF EXISTS ix_queued_prompts_conversation;
DROP TABLE IF EXISTS queued_prompts;
