// Zap: reduced warp_search_core. Upstream also ships a Tantivy full-text
// `searcher` + its `macros`/field-mapping modules; the warp_tui front-end only
// uses `inline_menu` (which pulls `mixer` -> `data_source` -> `item`/
// `result_renderer`), so the search-engine half is dropped to avoid the Tantivy
// dependency. See specs/warp-oss-sync/SCOPE.md.
// Vendored from warp_core::async::debounce (Zap's warp_core lacks the async
// module); a self-contained stream combinator used by the mixer.
pub mod debounce;
pub mod data_source;
pub mod inline_menu;
pub mod item;
pub mod mixer;
pub mod result_renderer;
