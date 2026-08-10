mod changed_files;
mod chunker;
mod codebase_index;
mod fragment_metadata;
pub mod local_store_client;
pub mod manager;
mod merkle_tree;
mod priority_queue;
pub mod search_shaping;
mod snapshot;
pub mod store_client;
mod sync_client;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use codebase_index::{CodebaseIndex, RetrievalID, SyncProgress};
pub use fragment_metadata::{FragmentLocation as FragmentMetadataLocation, FragmentMetadata};
pub use merkle_tree::{ContentHash, NodeHash};
use serde::{Deserialize, Serialize};
pub use snapshot::SnapshotStorage;
use string_offset::ByteOffset;
pub use sync_client::SyncTask;
use thiserror::Error;
use warp_core::errors::{AnyhowErrorExt, ErrorExt, register_error};

#[derive(Error, Debug)]
pub enum Error {
    #[error("File I/O error {0:#}")]
    Io(#[from] std::io::Error),
    #[error("Not a git repository")]
    NotAGitRepository,
    #[error("Build tree error {0:#}")]
    BuildTreeError(#[from] crate::index::BuildTreeError),
    #[error("Unsupported platform")]
    UnsupportedPlatform,
    #[error("Invalid hash: {0:#}")]
    InvalidHash(base16ct::Error),
    #[error("Empty node content")]
    EmptyNodeContent,
    #[error("Failed to get metadata")]
    FailedToGetMetadata(PathBuf),
    #[error("File size exceeds maximum limit")]
    FileSizeExceeded,
    #[error(transparent)]
    InconsistentState(#[from] InconsistentStateError),
    #[error("Failed to generate embeddings for some hashes")]
    FailedToGenerateEmbeddings(Vec<FragmentMetadata>),
    #[error("Failed to sync some intermediate nodes")]
    FailedToSyncIntermediateNodes(Vec<NodeHash>),
    #[error("Diff merkle tree {0:#}")]
    DiffMerkleTreeError(#[from] crate::index::full_source_code_embedding::DiffMerkleTreeError),
    #[error("File system changed since merkle tree construction")]
    FileSystemStateChanged,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error("Failed to parse snapshot")]
    SnapshotParsingFailed,
    /// No embedding provider is configured, so nothing can be embedded.
    ///
    /// Not present at the pin, where embeddings were produced by the server and
    /// the only way to have none was to be signed out. Here the user brings the
    /// provider, so "not configured yet" is an ordinary, expected state that has
    /// to be *said* rather than silently treated as an empty result — otherwise
    /// codebase search returns nothing and looks like a broken index.
    #[error(
        "no embedding provider is configured for {model}: add a provider under Settings > AI whose model list includes `{model}`, and give it an API key"
    )]
    NoEmbeddingProvider { model: &'static str },
    /// The vector store could not be read or written.
    #[error("codebase vector store error: {0:#}")]
    VectorStore(#[source] anyhow::Error),
}

impl ErrorExt for Error {
    fn is_actionable(&self) -> bool {
        match self {
            Self::Io(_)
            | Self::NotAGitRepository
            | Self::BuildTreeError(_)
            | Self::UnsupportedPlatform
            | Self::FailedToGetMetadata(_)
            | Self::FileSizeExceeded
            | Self::FileSystemStateChanged
            | Self::DiffMerkleTreeError(
                DiffMerkleTreeError::Ignored
                | DiffMerkleTreeError::Symlink
                | DiffMerkleTreeError::MaxDepthExceeded
                | DiffMerkleTreeError::ExceededMaxFileLimit,
            ) => false,
            Self::InvalidHash(_)
            | Self::EmptyNodeContent
            | Self::InconsistentState(_)
            | Self::FailedToGenerateEmbeddings(_)
            | Self::FailedToSyncIntermediateNodes(_)
            | Self::DiffMerkleTreeError(
                DiffMerkleTreeError::CurrentNodeMismatch(_) | DiffMerkleTreeError::Fragment(_),
            )
            | Self::SnapshotParsingFailed
            | Self::VectorStore(_) => true,
            // A missing provider is the user's configuration to fix, not a
            // defect for us to act on.
            Self::NoEmbeddingProvider { .. } => false,
            Self::Other(error) => error.is_actionable(),
        }
    }
}

register_error!(Error);

// Based off of BuildTreeError in entry.rs
#[derive(Debug, Error)]
pub enum DiffMerkleTreeError {
    #[error("Merkle tree node and file mismatch")]
    CurrentNodeMismatch(PathBuf),
    #[error("File is ignored")]
    Ignored,
    #[error("Symlink is not supported")]
    Symlink,
    #[error("Fragment node in diffing process")]
    Fragment(PathBuf),
    #[error("Max depth exceeded")]
    MaxDepthExceeded,
    #[error("Exceeded max file limit")]
    ExceededMaxFileLimit,
}

#[derive(Error, Debug)]
pub enum InconsistentStateError {
    #[error("Missing fragment metadata for {fragment_hash}")]
    MissingFragmentMetadata { fragment_hash: ContentHash },
    #[error("Can't find node index in merkle node")]
    NodeIndexNotFound,
}

/// Which embedding model a codebase index is built with.
///
/// The variants are the pin's, kept verbatim because the identity of the model
/// is part of the on-disk index: an index embedded with one model cannot be
/// queried with another, so the variant is stored alongside the vectors and
/// compared on load.
///
/// At the pin these values crossed the wire as `warp_graphql` enum members and
/// the embedding itself was produced server-side. Those `From`/`TryFrom` impls
/// are deliberately not ported — there is no server. The variants now name a
/// third-party model the user reaches directly with their own credentials; see
/// [`model_id`](Self::model_id) and [`dimensions`](Self::dimensions).
#[allow(non_camel_case_types)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmbeddingConfig {
    OpenAiTextSmall3_256,
    VoyageCode3_512,
    Voyage3_5_Lite_512,
    #[default]
    Voyage3_5_512,
    Voyage4_512,
}

impl EmbeddingConfig {
    /// The provider's own name for this model, as it appears in the request body.
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::OpenAiTextSmall3_256 => "text-embedding-3-small",
            Self::VoyageCode3_512 => "voyage-code-3",
            Self::Voyage3_5_Lite_512 => "voyage-3.5-lite",
            Self::Voyage3_5_512 => "voyage-3.5",
            Self::Voyage4_512 => "voyage-4",
        }
    }

    /// The vector width this configuration asks the provider for.
    ///
    /// Every variant the pin enumerates is a truncated/Matryoshka output, so the
    /// width is requested explicitly rather than taken as the model default.
    pub fn dimensions(&self) -> usize {
        match self {
            Self::OpenAiTextSmall3_256 => 256,
            Self::VoyageCode3_512
            | Self::Voyage3_5_Lite_512
            | Self::Voyage3_5_512
            | Self::Voyage4_512 => 512,
        }
    }

    /// A stable string used to key persisted vectors, so that changing the model
    /// invalidates the stored index instead of silently mixing vector spaces.
    pub fn storage_key(&self) -> &'static str {
        match self {
            Self::OpenAiTextSmall3_256 => "openai:text-embedding-3-small:256",
            Self::VoyageCode3_512 => "voyage:voyage-code-3:512",
            Self::Voyage3_5_Lite_512 => "voyage:voyage-3.5-lite:512",
            Self::Voyage3_5_512 => "voyage:voyage-3.5:512",
            Self::Voyage4_512 => "voyage:voyage-4:512",
        }
    }

    /// Parses the value written by [`storage_key`](Self::storage_key).
    pub fn from_storage_key(key: &str) -> Option<Self> {
        match key {
            "openai:text-embedding-3-small:256" => Some(Self::OpenAiTextSmall3_256),
            "voyage:voyage-code-3:512" => Some(Self::VoyageCode3_512),
            "voyage:voyage-3.5-lite:512" => Some(Self::Voyage3_5_Lite_512),
            "voyage:voyage-3.5:512" => Some(Self::Voyage3_5_512),
            "voyage:voyage-4:512" => Some(Self::Voyage4_512),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RepoMetadata {
    pub path: Option<String>,
}

#[derive(Clone, Copy)]
pub struct CodebaseContextConfig {
    pub embedding_config: EmbeddingConfig,
    pub embedding_cadence: Duration,
}

#[derive(Clone)]
pub struct FragmentLocation {
    absolute_path: PathBuf,
    byte_range: Range<ByteOffset>,
}

#[derive(Clone)]
pub struct Fragment {
    content: String,
    content_hash: ContentHash,
    location: FragmentLocation,
}

impl Fragment {
    pub fn from_byte_range(
        content: String,
        content_hash: ContentHash,
        absolute_path: PathBuf,
        byte_range: Range<ByteOffset>,
    ) -> Self {
        Self {
            content,
            content_hash,
            location: FragmentLocation {
                absolute_path,
                byte_range,
            },
        }
    }

    pub fn content_hash(&self) -> &ContentHash {
        &self.content_hash
    }

    pub fn absolute_path(&self) -> &Path {
        &self.location.absolute_path
    }

    pub fn byte_range(&self) -> Range<ByteOffset> {
        self.location.byte_range.clone()
    }

    /// The fragment's source text.
    ///
    /// At the pin nothing outside this module read the text directly: the
    /// content left the process through `From<Fragment> for
    /// warp_graphql::…::Fragment`, which moved the field out. Those conversions
    /// are not ported (there is no GraphQL server), so a `StoreClient`
    /// implementation living outside this module needs an accessor to embed the
    /// text.
    pub fn content(&self) -> &str {
        &self.content
    }
}
