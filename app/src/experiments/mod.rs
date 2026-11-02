//! The A/B experiment framework was removed. Its only live outcomes were constant for every
//! user (the bucket derived from the nil anonymous id is fixed): `BlockOnboarding` was always
//! `VariantTwo` and `ImprovedPaletteSearch` was always `Experiment` (Tantivy search on); the
//! `LoginLayer` experiment never assigned anyone. Those results are now inlined at their call
//! sites.

/// Kept as a no-op only so `terminal/input_test.rs` (owned by another change set) still builds;
/// delete it together with that call.
#[cfg(test)]
pub fn init(_ctx: &mut warpui::AppContext) {}
