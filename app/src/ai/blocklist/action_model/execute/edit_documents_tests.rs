use super::*;

fn delta(start: usize, end: usize, text: &str) -> DiffDelta {
    DiffDelta {
        replacement_line_range: start..end,
        insertion: text.to_string(),
    }
}

#[test]
fn out_of_order_deltas_are_sorted_into_document_order() {
    let mut deltas = vec![delta(8, 9, "b"), delta(2, 3, "a")];
    order_document_deltas(&mut deltas).unwrap();
    assert_eq!(deltas[0].insertion, "a");
    assert_eq!(deltas[1].insertion, "b");
}

#[test]
fn adjacent_deltas_are_allowed() {
    let mut deltas = vec![delta(3, 4, "b"), delta(2, 3, "a")];
    assert!(order_document_deltas(&mut deltas).is_ok());
}

#[test]
fn overlapping_deltas_are_rejected() {
    let mut deltas = vec![delta(2, 5, "a"), delta(4, 6, "b")];
    assert!(order_document_deltas(&mut deltas).is_err());
}
