# Dev-only test tooling

Everything here is dev-only: nothing is compiled into, or shipped in, a normal or release build.
None of it has been run on the machine that added it (no cargo on the laptop); the first build box
run regenerates `Cargo.lock`.

## shuttle and loom (interleaving tests)

`warpui_core::sync_shim` re-exports `parking_lot` and `std` atomics in every normal build and
swaps in shuttle's or loom's primitives under a cfg. `clipboard.rs`, `key_facts.rs` and
`typeahead_handoff.rs` import their locks from it. A model build compiles `model_*` tests
instead of the modules' ordinary tests (their `static`s need `const` constructors).

```
# random schedules (broad, fast)
RUSTFLAGS="--cfg shuttle" cargo test -p warpui_core --lib --release -- model_
RUSTFLAGS="--cfg shuttle" cargo test -p warp --lib --features warp/gui --release -- model_
# every schedule (tiny bodies only)
RUSTFLAGS="--cfg loom" cargo test -p warpui_core --lib --release -- model_
RUSTFLAGS="--cfg loom" cargo test -p warp --lib --features warp/gui --release -- model_
```

Use a separate `CARGO_TARGET_DIR` (the cfg changes every crate's build). Model builds do not
model time: `Condvar::wait_until` waits for a notification and never times out.
