pub use virtual_fs::{Stub, VirtualFS};

// `WarpDirs::git_repository_fixture` / `Zap::executable` / `Zap::fixtures` used to be
// defined here, duplicating `Dirs::git_repository_fixture` / `Zap::executable` /
// `Zap::fixtures` in `crates/virtual_fs/src/lib.rs`. Both copies had zero callers
// (this one doubly so: `mod virtual_fs` is private and `test_util::mod` only
// re-exports `Stub`/`VirtualFS` from it, never `Dirs`/`Zap`/`WarpDirs`, so this copy
// was unreachable even in principle). Removed as dead duplication (issue #549); the
// `crates/virtual_fs` copy is left as-is since that crate is a real, actively used
// dev-dependency (`VirtualFS`/`Stub`/`Dirs` have real callers there) and matches the
// pin, which carries the same dead helper with `#[allow(dead_code)]`.
