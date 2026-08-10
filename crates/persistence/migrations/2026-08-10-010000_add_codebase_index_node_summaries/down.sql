-- Reverses `up.sql`. Nothing references this table by foreign key, and it holds
-- no information that is not derivable from `codebase_index_nodes` and
-- `codebase_index_embeddings`: dropping it makes search slow, never wrong, and
-- the next sync rebuilds it.
DROP INDEX IF EXISTS ux_codebase_index_node_summaries_space_hash;
DROP TABLE IF EXISTS codebase_index_node_summaries;
