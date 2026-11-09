//! The host-side vector store for indexing: a flat f32 file keyed by content hash and embedding
//! storage key, brute-force cosine `QueryVectors` (Wave H parity, family package P9). Reserved by
//! P0; no sqlite. The proto requests are added (additively) by P9.
