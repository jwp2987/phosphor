//! Tests for the SQLite-backed vector store's encoding.
//!
//! New, not ported. The walk and the query paths need a database and are
//! covered by `LocalStoreClient`'s own tests against an in-memory store; what is
//! tested here is the one thing only this module does — turning a `Vec<f32>`
//! into bytes and back — because a mistake there corrupts every vector without
//! anything downstream being able to tell.

use super::*;

#[test]
fn a_vector_round_trips_through_the_blob_encoding() {
    let vector = vec![0.0f32, 1.5, -2.25, 1e-9, 12345.678];
    let bytes = encode_vector(&vector);

    assert_eq!(bytes.len(), vector.len() * 4, "four bytes per f32");

    let decoded =
        decode_vector(&bytes, vector.len() as i32).expect("a vector we just wrote must decode");
    assert_eq!(decoded, vector);
}

#[test]
fn an_empty_vector_round_trips() {
    let bytes = encode_vector(&[]);
    assert!(bytes.is_empty());
    assert_eq!(
        decode_vector(&bytes, 0).expect("decodes"),
        Vec::<f32>::new()
    );
}

#[test]
fn a_blob_that_disagrees_with_its_dimension_count_is_rejected() {
    // The whole point of storing `dimensions` separately: a truncated blob must
    // be detectable. Silently decoding a shorter vector would still score, so
    // the corruption would never surface.
    let bytes = encode_vector(&[1.0, 2.0, 3.0]);

    assert!(
        decode_vector(&bytes, 4).is_err(),
        "a blob shorter than its claimed width must be rejected"
    );
    assert!(
        decode_vector(&bytes, 2).is_err(),
        "a blob longer than its claimed width must be rejected"
    );
    assert!(decode_vector(&bytes, 3).is_ok(), "the honest width decodes");
}

#[test]
fn a_negative_dimension_count_is_rejected_rather_than_wrapping() {
    // `dimensions` is an i32 because SQLite has no unsigned integers. A negative
    // value can only come from a corrupt row, and must not be cast into a huge
    // usize.
    let bytes = encode_vector(&[1.0]);
    assert!(decode_vector(&bytes, -1).is_err());
}

#[test]
fn a_store_with_no_persistence_reports_errors_instead_of_pretending_to_work() {
    // A store that cannot reach the database must fail loudly. Answering "I know
    // nothing" from `known_hashes` would be worse than an error in one specific
    // way: it is indistinguishable from a genuinely empty store, so the index
    // would re-embed the entire repository on every sync, spending the user's
    // quota, and never notice.
    let store = SqliteVectorStore::new(None, None);

    assert!(
        store
            .known_hashes(
                EmbeddingConfig::default().storage_key(),
                &[NodeHash::from(ContentHash::from_content("x"))],
            )
            .is_err(),
        "a store with no read connection must error, not report an empty set"
    );
    assert!(
        store
            .record_embeddings(
                EmbeddingConfig::default().storage_key(),
                &[(ContentHash::from_content("x"), vec![1.0])],
            )
            .is_err(),
        "a store with no writer must error, not silently drop the write"
    );
}

#[test]
fn empty_inputs_are_no_ops_even_without_persistence() {
    // Nothing to do is not a failure -- these must not manufacture an error the
    // caller would report to the user.
    let store = SqliteVectorStore::new(None, None);
    let space = EmbeddingConfig::default().storage_key();

    assert!(store.record_nodes(space, &[]).is_ok());
    assert!(store.record_embeddings(space, &[]).is_ok());
    assert!(store.known_hashes(space, &[]).is_ok());
    assert!(store.vectors_for(space, &[]).is_ok());
}
