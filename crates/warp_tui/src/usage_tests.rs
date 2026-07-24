use super::*;

#[test]
fn context_usage_formats_fraction_as_whole_percent() {
    assert_eq!(format_context_usage(0.0), "0% context");
    assert_eq!(format_context_usage(0.183), "18% context");
    assert_eq!(format_context_usage(0.185), "19% context");
    assert_eq!(format_context_usage(0.5), "50% context");
    assert_eq!(format_context_usage(1.0), "100% context");
}

#[test]
fn context_usage_clamps_out_of_range_fractions() {
    assert_eq!(format_context_usage(-0.2), "0% context");
    assert_eq!(format_context_usage(1.7), "100% context");
}
