//! The geometry behind the codebase index's pruned search.
//!
//! # The problem this solves
//!
//! The first BYOP [`LocalStoreClient`][super::local_store_client::LocalStoreClient]
//! answered every query by walking the whole merkle tree, loading every
//! reachable leaf vector and scoring all of them. That is exact, but it is
//! `O(fragments)` per query: on a repository with a hundred thousand fragments
//! it means decoding ~200 MB of `f32` out of SQLite before the first result can
//! be shown. The pin's server hid this behind an approximate index; this fork
//! has to have one of its own.
//!
//! # The index, and why it is this one
//!
//! There is already a tree here — the merkle tree — and retrieval already
//! descends it from a root hash. So rather than build a second structure (HNSW,
//! whose graph has to be held in memory in proportion to the corpus, or IVF,
//! which has to be trained and retrained as the repository drifts), this treats
//! the merkle tree *as* a ball tree in cosine space:
//!
//! * every intermediate node carries a [`NodeSummary`] — the mean of the
//!   embedding vectors of every leaf beneath it, how many leaves that was, and
//!   an angular radius that covers all of them;
//! * a query then descends best-first, and a subtree whose *best possible*
//!   score is already worse than the current k-th best result is never opened.
//!
//! Two properties make this a good fit rather than merely a convenient one:
//!
//! 1. **It is exact.** [`NodeSummary::upper_bound`] is a true upper bound on the
//!    cosine similarity of *any* leaf in the subtree, so pruning can only ever
//!    discard fragments that could not have made the top k. The results are
//!    identical to the full scan's, ordering included. Nothing is traded away
//!    for the speed; what varies is only how much work it takes to get there.
//! 2. **It cannot go stale.** A merkle node's hash is a function of its entire
//!    subtree, so a summary keyed by node hash is either correct or absent — it
//!    can never be silently wrong. An edit changes the hashes on the path from
//!    the edited file to the root and nothing else, so a re-index recomputes
//!    that path and reuses every other summary.
//!
//! # Where the pruning does *not* help, stated plainly
//!
//! Pruning works when leaves that are near each other in embedding space are
//! also near each other in the tree — when code in one directory embeds to
//! similar vectors. That is usually true, and is most of the reason directory
//! structure exists, but it is a property of the repository rather than a
//! guarantee. If a repository's embeddings had no directory locality at all,
//! every node's radius would be wide, no subtree could be pruned, and the search
//! would visit every leaf: the old behaviour, plus the cost of reading the
//! summaries. The results would still be correct. There is no input for which
//! this returns a worse answer than the full scan; there are inputs for which it
//! is no faster.
//!
//! # Slack in the radius
//!
//! A node's radius is computed from its children rather than from its
//! descendants: `radius(parent) = max over children of (angle(parent_centroid,
//! child_centroid) + radius(child))`. That is the triangle inequality, so it is
//! an over-estimate — the true covering radius may be smaller. An over-estimated
//! radius makes [`NodeSummary::upper_bound`] larger, which prunes *less*. It can
//! never prune something it should have kept, so exactness is unaffected.
//!
//! The alternative — walking every descendant leaf of every node to get the
//! exact radius — costs `O(fragments * depth)` vector comparisons per index
//! build, to buy a tighter bound at the levels where it matters least. The level
//! where it matters most is already exact: a node whose children are all leaves
//! has children of radius zero, so its radius is the true covering radius.

use std::f32::consts::PI;

/// Cosine similarity of two vectors, in `[-1.0, 1.0]`.
///
/// Returns `0.0` for mismatched lengths or a zero-magnitude vector rather than
/// `NaN`, so a malformed row can never poison a sort.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += f64::from(*x) * f64::from(*y);
        norm_a += f64::from(*x) * f64::from(*x);
        norm_b += f64::from(*y) * f64::from(*y);
    }

    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }

    (dot / (norm_a.sqrt() * norm_b.sqrt())) as f32
}

/// Returns `vector` scaled to unit length, or `None` if it has no direction.
///
/// A zero-magnitude vector has no direction to compare against, and dividing by
/// its norm would produce `NaN`s that make every later comparison arbitrary.
pub fn unit(vector: &[f32]) -> Option<Vec<f32>> {
    let norm = vector
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return None;
    }
    Some(
        vector
            .iter()
            .map(|value| (f64::from(*value) / norm) as f32)
            .collect(),
    )
}

/// Dot product of two equal-length slices; `0.0` if the lengths disagree.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum::<f64>() as f32
}

/// What a subtree of the merkle tree looks like in embedding space: where it
/// sits, how many fragments it covers, and how far they spread.
///
/// This is the entirety of the index. One is stored per intermediate node, keyed
/// by node hash, and it is what makes a subtree skippable.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSummary {
    /// The mean of every descendant leaf's embedding vector. Deliberately the
    /// *unnormalized* mean, because that is what makes summaries combinable: a
    /// parent's mean is the leaf-count-weighted mean of its children's means,
    /// which is only true before normalization.
    pub mean: Vec<f32>,
    /// How many embedded leaves the mean was taken over — the weight in that
    /// combination.
    pub leaf_count: u32,
    /// The angle, in radians, within which every descendant leaf lies of
    /// `mean`'s direction. `PI` means "no information" — the whole sphere — and
    /// is what a degenerate summary reports so that it prunes nothing.
    pub radius: f32,
}

impl NodeSummary {
    /// The summary of a single embedded fragment: itself, exactly.
    pub fn leaf(vector: &[f32]) -> Self {
        Self {
            mean: vector.to_vec(),
            leaf_count: 1,
            radius: 0.0,
        }
    }

    /// A summary that covers everything and excludes nothing.
    ///
    /// Used where a subtree's true extent cannot be established — a width
    /// mismatch between children, or a mean with no direction. It is the safe
    /// answer because it prunes nothing: correctness survives, only speed
    /// suffers.
    pub fn opaque(dimensions: usize, leaf_count: u32) -> Self {
        Self {
            mean: vec![0.0; dimensions],
            leaf_count,
            radius: PI,
        }
    }

    /// Combines child summaries into their parent's.
    ///
    /// Returns `None` for an empty child list, or for children that cover no
    /// leaves: a node with nothing embedded beneath it has nothing to
    /// summarize, and recording a summary for it would claim coverage the index
    /// does not have.
    pub fn combine(children: &[NodeSummary]) -> Option<Self> {
        let first = children.first()?;
        let dimensions = first.mean.len();
        let total: u32 = children.iter().map(|child| child.leaf_count).sum();
        if total == 0 {
            return None;
        }

        // A width mismatch means two vector spaces got mixed, which should be
        // impossible (every row is scoped by `storage_key`) but must not be
        // allowed to produce a bound that silently excludes half the tree.
        if children.iter().any(|child| child.mean.len() != dimensions) {
            log::warn!("Codebase index node has children of differing vector widths; not pruning");
            return Some(Self::opaque(dimensions, total));
        }

        let mut mean = vec![0.0f64; dimensions];
        for child in children {
            let weight = f64::from(child.leaf_count);
            for (slot, value) in mean.iter_mut().zip(child.mean.iter()) {
                *slot += weight * f64::from(*value);
            }
        }
        let weight = f64::from(total);
        let mean: Vec<f32> = mean
            .into_iter()
            .map(|value| (value / weight) as f32)
            .collect();

        let Some(centroid) = unit(&mean) else {
            // The children cancel out: there is a mean, but it has no direction,
            // so no angle can be measured from it.
            return Some(Self::opaque(dimensions, total));
        };

        // Triangle inequality: every leaf under `child` is within
        // `radius(child)` of that child's centroid, which is itself
        // `angle(centroid, child_centroid)` away from ours.
        let mut radius = 0.0f32;
        for child in children {
            let separation = match unit(&child.mean) {
                Some(child_centroid) => dot(&centroid, &child_centroid).clamp(-1.0, 1.0).acos(),
                // A child with no direction could be pointing anywhere.
                None => PI,
            };
            radius = radius.max(separation + child.radius);
        }

        Some(Self {
            mean,
            leaf_count: total,
            radius: radius.min(PI),
        })
    }

    /// The largest cosine similarity any leaf under this node could have with
    /// `query_unit`, which must already be unit length.
    ///
    /// This is the whole pruning rule. Every descendant leaf lies within
    /// [`radius`](Self::radius) of the centroid, so the closest any of them can
    /// get to the query is at angle `angle(query, centroid) - radius`; a query
    /// already inside the ball could be matched exactly, which is `1.0`.
    pub fn upper_bound(&self, query_unit: &[f32]) -> f32 {
        if !self.radius.is_finite() || self.radius >= PI {
            return 1.0;
        }
        let Some(centroid) = unit(&self.mean) else {
            return 1.0;
        };
        if centroid.len() != query_unit.len() {
            return 1.0;
        }

        let angle = dot(query_unit, &centroid).clamp(-1.0, 1.0).acos();
        if angle <= self.radius {
            return 1.0;
        }
        (angle - self.radius).cos()
    }
}

/// Orders `f32` scores with a total order, so a search frontier and a result
/// heap can both be `BinaryHeap`s.
///
/// `f32` is only `PartialOrd`, and `partial_cmp().unwrap()` on a `NaN` would
/// panic inside a heap's sift. [`f32::total_cmp`] is total by construction.
#[derive(Clone, Debug)]
pub struct ByScore<T> {
    pub score: f32,
    pub item: T,
}

impl<T> PartialEq for ByScore<T> {
    fn eq(&self, other: &Self) -> bool {
        self.score.total_cmp(&other.score) == std::cmp::Ordering::Equal
    }
}

impl<T> Eq for ByScore<T> {}

impl<T> PartialOrd for ByScore<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for ByScore<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.total_cmp(&other.score)
    }
}

#[cfg(test)]
#[path = "vector_index_tests.rs"]
mod tests;
