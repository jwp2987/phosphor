-- Reverses `up.sql`. Nothing references these tables by foreign key, so both
-- drop cleanly; the index is rebuilt from the filesystem on the next sync.
DROP INDEX IF EXISTS ux_codebase_index_embeddings_space_hash;
DROP TABLE IF EXISTS codebase_index_embeddings;
DROP INDEX IF EXISTS ux_codebase_index_nodes_space_hash;
DROP TABLE IF EXISTS codebase_index_nodes;
