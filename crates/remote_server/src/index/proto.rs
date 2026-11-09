//! Conversions between the index data model and the P9b wire messages, shared by the helper and
//! the shell arm so both produce the same responses.

use super::store::RankedChunk;
use super::types::{ChunkEntry, ManifestEntry, Scan};
use crate::proto::{
    CodebaseIndexError, CodebaseManifestEntry, FragmentMetadata, GetChunksResponse,
    QueryVectorsResponse, QueryVectorsSuccess, RankedCodebaseFragment, ScanCodebaseResponse,
    ScanCodebaseSuccess, StoreVectorsResponse, get_chunks_response, query_vectors_response,
    scan_codebase_response, store_vectors_response,
};

pub fn scan_to_proto(scan: &Scan) -> ScanCodebaseSuccess {
    ScanCodebaseSuccess {
        manifest: scan
            .manifest
            .iter()
            .map(|m| CodebaseManifestEntry {
                path: m.path.clone(),
                file_hash: m.file_hash.clone(),
            })
            .collect(),
        chunks: scan
            .chunks
            .iter()
            .map(|c| FragmentMetadata {
                content_hash: c.content_hash.clone(),
                path: c.path.clone(),
                start_line: c.start_line as u32,
                end_line: c.end_line as u32,
                byte_start: c.byte_start as u64,
                byte_end: c.byte_end as u64,
            })
            .collect(),
        root_hash: scan.root_hash.clone(),
    }
}

pub fn scan_from_proto(success: &ScanCodebaseSuccess) -> Scan {
    Scan {
        manifest: success
            .manifest
            .iter()
            .map(|m| ManifestEntry {
                path: m.path.clone(),
                file_hash: m.file_hash.clone(),
            })
            .collect(),
        chunks: success
            .chunks
            .iter()
            .map(|c| ChunkEntry {
                content_hash: c.content_hash.clone(),
                path: c.path.clone(),
                start_line: c.start_line as usize,
                end_line: c.end_line as usize,
                byte_start: c.byte_start as usize,
                byte_end: c.byte_end as usize,
            })
            .collect(),
        root_hash: success.root_hash.clone(),
    }
}

pub fn ranked_to_proto(hits: Vec<RankedChunk>) -> QueryVectorsSuccess {
    QueryVectorsSuccess {
        fragments: hits
            .into_iter()
            .map(|h| RankedCodebaseFragment {
                content_hash: h.content_hash,
                path: h.path,
                start_line: h.start_line as u32,
                end_line: h.end_line as u32,
                score: h.score,
            })
            .collect(),
    }
}

pub fn index_error(message: impl Into<String>) -> CodebaseIndexError {
    CodebaseIndexError {
        message: message.into(),
    }
}

pub fn scan_error(message: impl Into<String>) -> ScanCodebaseResponse {
    ScanCodebaseResponse {
        result: Some(scan_codebase_response::Result::Error(index_error(message))),
    }
}

pub fn chunks_error(message: impl Into<String>) -> GetChunksResponse {
    GetChunksResponse {
        result: Some(get_chunks_response::Result::Error(index_error(message))),
    }
}

pub fn store_error(message: impl Into<String>) -> StoreVectorsResponse {
    StoreVectorsResponse {
        result: Some(store_vectors_response::Result::Error(index_error(message))),
    }
}

pub fn query_error(message: impl Into<String>) -> QueryVectorsResponse {
    QueryVectorsResponse {
        result: Some(query_vectors_response::Result::Error(index_error(message))),
    }
}

/// The most hashes one `GetChunks` may name, so a response stays far below the frame cap.
pub const MAX_CHUNKS_PER_REQUEST: usize = 4096;
/// The most results one `QueryVectors` returns.
pub const MAX_TOP_K: u32 = 200;
