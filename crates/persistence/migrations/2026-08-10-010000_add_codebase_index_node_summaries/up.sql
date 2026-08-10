-- The codebase index's search index: one summary per merkle node.
--
-- WHY THIS TABLE EXISTS
--
-- Retrieval used to score every embedded fragment reachable from the queried
-- root, which is exact but O(fragments) per query -- on a large repository,
-- hundreds of megabytes of vectors read and decoded before the first result
-- appears. The pin hid that behind a server-side approximate index; this fork
-- has none, so it builds one out of the tree it already has.
--
-- Each row describes what one subtree looks like in embedding space: the mean
-- of every embedded leaf beneath the node (`mean`), how many leaves that was
-- (`leaf_count`), and an angle in radians within which all of them lie
-- (`radius`). Together those bound the best score any fragment in the subtree
-- could achieve, so a query can skip the subtree without opening it. See
-- `crates/ai/src/index/full_source_code_embedding/vector_index.rs`.
--
-- WHY IT CANNOT GO STALE
--
-- A merkle node's hash covers its entire subtree, so a summary keyed by
-- (embedding_space, node_hash) describes the same set of fragments forever. It
-- can be absent -- which the search reads as "open this subtree", slow but
-- correct -- and it can be rebuilt, but it can never be silently wrong. That is
-- also why editing a file invalidates nothing: it produces new hashes along one
-- path, and every other summary is reused as-is.
--
-- `mean` is the raw little-endian f32 sequence, the same encoding as
-- `codebase_index_embeddings.vector`, with `dimensions` stored alongside so a
-- truncated blob is detectable rather than silently scored at the wrong width.
-- It is deliberately the UNNORMALIZED mean: a parent's mean is the
-- leaf-count-weighted mean of its children's, which is only true before
-- normalization.
CREATE TABLE codebase_index_node_summaries (
    id INTEGER PRIMARY KEY NOT NULL,
    embedding_space TEXT NOT NULL,
    node_hash TEXT NOT NULL,
    leaf_count INTEGER NOT NULL,
    radius FLOAT NOT NULL,
    dimensions INTEGER NOT NULL,
    mean BLOB NOT NULL,
    last_modified_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Required by the `ON CONFLICT (embedding_space, node_hash) DO UPDATE` upsert,
-- and it is the lookup the search descent makes once per level.
CREATE UNIQUE INDEX ux_codebase_index_node_summaries_space_hash
    ON codebase_index_node_summaries (embedding_space, node_hash);
