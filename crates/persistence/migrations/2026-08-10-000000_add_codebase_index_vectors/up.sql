-- Local storage for the codebase embedding index.
--
-- At the pin (`02b53fcd8`) neither of these tables existed, because neither
-- thing was stored on the machine: `StoreClient` was implemented only by
-- `ServerApi`, and the merkle-node registry and the embedding vectors both
-- lived on Warp's server. This fork has no server, so a local `StoreClient`
-- has to keep them, and this is where.
--
-- `embedding_space` is `EmbeddingConfig::storage_key()` -- e.g.
-- `voyage:voyage-3.5:512`. Every row is scoped by it so vectors produced by
-- different models can never be compared with each other; switching models
-- leaves the old rows unreachable rather than corrupting anything.

-- The merkle DAG's intermediate nodes. Without these there is no tree to walk,
-- and a query cannot be scoped to one repository: retrieval starts at a root
-- hash and descends through `child_hashes`.
--
-- `child_hashes` is a JSON array of hex hashes rather than a child table. The
-- list is only ever read or written whole, is bounded by the tree's fan-out,
-- and a join table would cost an extra index and a join on the hot walk.
CREATE TABLE codebase_index_nodes (
    id INTEGER PRIMARY KEY NOT NULL,
    embedding_space TEXT NOT NULL,
    node_hash TEXT NOT NULL,
    child_hashes TEXT NOT NULL,
    last_modified_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Required by the `ON CONFLICT (embedding_space, node_hash) DO UPDATE` upsert:
-- a node is re-recorded whenever its subtree changes.
CREATE UNIQUE INDEX ux_codebase_index_nodes_space_hash
    ON codebase_index_nodes (embedding_space, node_hash);

-- One embedding vector per code fragment.
--
-- `vector` is the raw little-endian `f32` sequence, and `dimensions` is its
-- length. Storing the length separately means a truncated or corrupt blob is
-- detectable on read instead of being silently scored as a shorter vector.
CREATE TABLE codebase_index_embeddings (
    id INTEGER PRIMARY KEY NOT NULL,
    embedding_space TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    dimensions INTEGER NOT NULL,
    vector BLOB NOT NULL,
    last_modified_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX ux_codebase_index_embeddings_space_hash
    ON codebase_index_embeddings (embedding_space, content_hash);
