use super::native::parse_thread_count;

#[test]
fn parse_pool_is_single_threaded_on_eight_cores_or_fewer() {
    for cores in [1, 2, 4, 8] {
        assert_eq!(parse_thread_count(cores), 1, "cores={cores}");
    }
}

#[test]
fn parse_pool_never_exceeds_two_threads() {
    for cores in [9, 12, 16, 64, 256] {
        assert!(parse_thread_count(cores) <= 2, "cores={cores}");
        assert!(parse_thread_count(cores) >= 1, "cores={cores}");
    }
}
