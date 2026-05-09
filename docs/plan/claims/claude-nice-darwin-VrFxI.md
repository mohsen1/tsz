# fix(sourcemap): panic on u32 index overflow instead of substituting u32::MAX

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-VrFxI`
- **PR**: TBD
- **Status**: ready
- **Workstream**: bug fix (#4779)

## Intent

Closes #4779. `SourceMapGenerator::add_source`,
`add_source_with_content`, and `add_name` previously converted the
next index from `usize` to `u32` via
`u32::try_from(...).unwrap_or(u32::MAX)`. On overflow, that produced
a synthetic `u32::MAX` index that was committed to the generator and
later emitted into the VLQ-encoded `.map` output — silent corruption
of the mapping metadata. The fix replaces every fallback with
`expect(...)` carrying a clear overflow message, so an oversized
generator panics loudly instead of emitting an invalid map. Public
return types stay `u32`; no callers change.

## Files Touched

- `crates/tsz-common/src/source_map/mod.rs` (4 sites + doc panic
  notes, ~30 LOC change)
- `crates/tsz-common/tests/source_map.rs` (2 new regression tests,
  ~25 LOC)

## Verification

- `cargo test -p tsz-common` (413 lib tests + 1 doctest pass; the 30
  inlined `source_map` tests include the 2 new ones)
- `cargo test -p tsz-emitter source_writer` (68 tests pass)
- `cargo test -p tsz-core source_map` (931 tests pass)
- `cargo build --workspace` (clean across 30+ crates)
