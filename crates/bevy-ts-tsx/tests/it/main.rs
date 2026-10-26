//! bevy-ts-tsx's integration tests, one binary: each file here is a module (docs/build-cache.md, Test binaries).

// A release build of these tests is a release bench: it links Bevy statically, as what ships does,
// so it is built by `cargo static test --release` (docs/build-cache.md, Bevy as a dylib for tests).
#[cfg(all(not(debug_assertions), not(bevy_ts_static)))]
compile_error!("a release build of the tests links the Bevy dylib: build it with `cargo static test --release`");

mod bsn_spike;
mod from_world;
mod newtype;
mod primitives;
mod reconcile;
mod restore;
mod standard_material;
mod slot;
mod vertex_mesh;
mod visibility_range;
