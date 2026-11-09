//! The store-side half of the index requests, shared by the helper and the shell arm: both keep
//! the same flat-file store and must answer `StoreVectors` / `QueryVectors` identically.

use super::proto::{MAX_TOP_K, query_error, ranked_to_proto, store_error};
use super::store::IndexStore;
use crate::proto::{
    QueryVectors, QueryVectorsResponse, StoreVectors, StoreVectorsResponse, StoreVectorsSuccess,
    query_vectors_response, store_vectors_response,
};

pub fn store_vectors(store: &IndexStore, msg: StoreVectors) -> StoreVectorsResponse {
    if msg.storage_key.is_empty() {
        return store_error("storage_key is empty");
    }
    let vectors: Vec<(String, Vec<f32>)> = msg
        .vectors
        .into_iter()
        .map(|v| (v.content_hash, v.values))
        .collect();
    match store.append_vectors(&msg.storage_key, &vectors) {
        Ok(stored) => {
            // A changed embedding model makes every older space unusable.
            store.drop_other_spaces(&msg.storage_key);
            StoreVectorsResponse {
                result: Some(store_vectors_response::Result::Success(
                    StoreVectorsSuccess {
                        stored: stored as u64,
                    },
                )),
            }
        }
        Err(e) => store_error(format!("{e:#}")),
    }
}

pub fn query_vectors(store: &IndexStore, msg: QueryVectors) -> QueryVectorsResponse {
    if msg.query_vector.is_empty() {
        return query_error("the query vector is empty");
    }
    let top_k = msg.top_k.clamp(1, MAX_TOP_K) as usize;
    match store.query(&msg.storage_key, &msg.query_vector, top_k) {
        Ok(hits) => QueryVectorsResponse {
            result: Some(query_vectors_response::Result::Success(ranked_to_proto(
                hits,
            ))),
        },
        Err(e) => query_error(format!("{e:#}")),
    }
}
