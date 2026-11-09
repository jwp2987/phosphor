//! Codebase indexing shared by both arms (Wave H parity, P9): the scan data model and pure
//! functions ([`types`]), and the flat-file store of fragments and vectors ([`store`]).
//!
//! The helper walks the host's tree and keeps the store on the host; the no-helper path gets the
//! same facts by running commands and keeps the store app-side. The app embeds the text through
//! `crates/network_gate` in both cases, so the user's embedding key never reaches a host.

pub mod store;
pub mod types;

pub use store::{IndexStore, RankedChunk, StoredScan, cosine};
pub use types::{ChunkEntry, ManifestEntry, Scan, ScanDiff};
