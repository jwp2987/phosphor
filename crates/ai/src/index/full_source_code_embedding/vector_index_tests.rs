//! Tests for the pruning geometry.
//!
//! New, not ported — the pin had no client-side index at all. The only property
//! that really matters here is the one the search's exactness rests on:
//! [`NodeSummary::upper_bound`] must never under-estimate. Everything else about
//! the index can be slow or wasteful and the results are still right; a bound
//! that is too tight silently drops fragments, and nothing downstream can tell.

use super::*;

/// A deterministic generator, so a failure is reproducible.
struct Lcg(u64);

impl Lcg {
    fn next_signed(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32) * 2.0 - 1.0
    }

    fn next_vector(&mut self, dimensions: usize) -> Vec<f32> {
        (0..dimensions).map(|_| self.next_signed()).collect()
    }
}

#[test]
fn a_leaf_summary_is_the_leaf_itself() {
    let summary = NodeSummary::leaf(&[3.0, 4.0]);
    assert_eq!(summary.mean, vec![3.0, 4.0]);
    assert_eq!(summary.leaf_count, 1);
    assert_eq!(summary.radius, 0.0);
}

#[test]
fn a_leaf_bounds_itself_exactly() {
    // A leaf's "upper bound" is its own score, which is what lets the search
    // treat reaching a leaf and scoring it as the same event.
    let vector = [1.0f32, 2.0, 3.0];
    let summary = NodeSummary::leaf(&vector);
    let query = unit(&[1.0, 1.0, 1.0]).expect("has direction");

    let bound = summary.upper_bound(&query);
    let actual = cosine_similarity(&query, &vector);
    assert!(
        (bound - actual).abs() < 1e-5,
        "a leaf's bound {bound} should equal its score {actual}"
    );
}

#[test]
fn combining_children_averages_over_leaves_not_over_children() {
    // The weighting is what makes summaries composable up a tree of uneven
    // fan-out. A parent of a 100-leaf subtree and a 1-leaf subtree sits almost
    // on top of the first, not halfway between them.
    let heavy = NodeSummary {
        mean: vec![1.0, 0.0],
        leaf_count: 99,
        radius: 0.0,
    };
    let light = NodeSummary::leaf(&[0.0, 1.0]);

    let combined = NodeSummary::combine(&[heavy, light]).expect("combines");
    assert_eq!(combined.leaf_count, 100);
    assert!(
        combined.mean[0] > 0.9 && combined.mean[1] < 0.1,
        "the mean {:?} should sit near the heavy child",
        combined.mean
    );
}

#[test]
fn combining_nothing_produces_nothing() {
    assert!(
        NodeSummary::combine(&[]).is_none(),
        "a node with no embedded leaves beneath it must not claim a summary"
    );
    assert!(
        NodeSummary::combine(&[NodeSummary {
            mean: vec![1.0, 0.0],
            leaf_count: 0,
            radius: 0.0,
        }])
        .is_none(),
        "nor may one whose children cover no leaves"
    );
}

#[test]
fn a_degenerate_summary_prunes_nothing() {
    // Children that cancel out leave a mean with no direction, so no angle can be
    // measured from it. The only safe answer is "this subtree could contain
    // anything", which is a bound of 1.0.
    let opposed = NodeSummary::combine(&[
        NodeSummary::leaf(&[1.0, 0.0]),
        NodeSummary::leaf(&[-1.0, 0.0]),
    ])
    .expect("combines");

    let query = unit(&[0.0, 1.0]).expect("has direction");
    assert_eq!(opposed.upper_bound(&query), 1.0);

    let mismatched = NodeSummary::opaque(4, 7);
    assert_eq!(mismatched.upper_bound(&query), 1.0);
    assert_eq!(
        NodeSummary::leaf(&[1.0, 0.0, 0.0]).upper_bound(&query),
        1.0,
        "a summary of a different width cannot bound this query, so it must not try"
    );
}

#[test]
fn a_bound_never_under_estimates_a_leaf_beneath_it() {
    // The property the whole search rests on. A bound that came out below some
    // descendant's real score would let the search prune a fragment that should
    // have been returned -- an error nothing downstream could detect, because
    // the results would still look plausible.
    //
    // Checked over a randomised two-level tree rather than a hand-picked case,
    // because the failure would be in the arithmetic, not in the shape.
    let mut rng = Lcg(0x5eed);
    let dimensions = 12;

    for _ in 0..64 {
        // A cluster of leaves, then a parent over several such clusters, so both
        // the exact (leaf-children) and the triangle-inequality (node-children)
        // paths are exercised.
        let mut cluster_summaries = Vec::new();
        let mut every_leaf = Vec::new();

        for _ in 0..4 {
            let centre = rng.next_vector(dimensions);
            let leaves: Vec<Vec<f32>> = (0..6)
                .map(|_| {
                    let noise = rng.next_vector(dimensions);
                    centre
                        .iter()
                        .zip(noise.iter())
                        .map(|(base, offset)| base + 0.4 * offset)
                        .collect()
                })
                .collect();

            let summaries: Vec<NodeSummary> =
                leaves.iter().map(|leaf| NodeSummary::leaf(leaf)).collect();
            cluster_summaries.push(NodeSummary::combine(&summaries).expect("combines"));
            every_leaf.extend(leaves);
        }

        let root = NodeSummary::combine(&cluster_summaries).expect("combines");
        assert_eq!(root.leaf_count, 24);

        let query = unit(&rng.next_vector(dimensions)).expect("has direction");
        let bound = root.upper_bound(&query);
        for leaf in &every_leaf {
            let score = cosine_similarity(&query, leaf);
            assert!(
                bound >= score - 1e-4,
                "bound {bound} is below a descendant's score {score}"
            );
        }
    }
}

#[test]
fn a_tight_cluster_far_from_the_query_bounds_well_below_one() {
    // The other half of usefulness: a valid bound that is always 1.0 would prune
    // nothing. A cluster pointing one way must bound low against a query pointing
    // another.
    let cluster = NodeSummary::combine(&[
        NodeSummary::leaf(&[1.0, 0.02, 0.0]),
        NodeSummary::leaf(&[1.0, -0.02, 0.0]),
        NodeSummary::leaf(&[1.0, 0.0, 0.03]),
    ])
    .expect("combines");

    let query = unit(&[0.0, 0.0, 1.0]).expect("has direction");
    let bound = cluster.upper_bound(&query);
    assert!(
        bound < 0.2,
        "a tight cluster nearly orthogonal to the query should bound low, got {bound}"
    );
}

#[test]
fn unit_refuses_a_vector_with_no_direction() {
    assert!(unit(&[0.0, 0.0]).is_none());
    assert!(unit(&[]).is_none());
    let scaled = unit(&[3.0, 4.0]).expect("has direction");
    assert!((scaled[0] - 0.6).abs() < 1e-6 && (scaled[1] - 0.8).abs() < 1e-6);
}

#[test]
fn by_score_orders_highest_first_without_panicking_on_nan() {
    // A `NaN` reaching a `BinaryHeap`'s comparator would panic inside the sift,
    // taking down a background search task. `total_cmp` cannot.
    let mut heap = std::collections::BinaryHeap::new();
    for score in [0.5f32, f32::NAN, 0.9, -1.0] {
        heap.push(ByScore { score, item: () });
    }
    assert_eq!(heap.len(), 4);
    let top = heap.pop().expect("non-empty");
    assert!(
        top.score.is_nan() || (top.score - 0.9).abs() < 1e-6,
        "got {}",
        top.score
    );
}
