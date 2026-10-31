use super::*;

#[test]
fn concurrent_loads_coalesce_to_one_owner() {
    let c = LoadCoalescer::new();
    assert!(c.try_begin(), "first caller owns the load");
    assert!(!c.try_begin(), "second dispatch while in flight is dropped");
    assert!(!c.try_begin());
    c.finish();
    assert!(
        c.try_begin(),
        "a new load may start after the first finishes"
    );
}
