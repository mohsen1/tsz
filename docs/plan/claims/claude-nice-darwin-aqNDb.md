# fix(source_map): encode_segment substitutes i32::MAX on overflow (#4780)

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-aqNDb`
- **PR**: #4875
- **Status**: ready
- **Workstream**: emit/source-map robustness (issue #4780)

## Intent

`SourceMapGenerator::encode_segment` currently uses
`i32::try_from(...).unwrap_or(i32::MAX)` for every mapping field. When a
mapping value exceeds `i32::MAX`, the encoder silently substitutes `i32::MAX`
and emits a syntactically valid but semantically wrong VLQ segment, producing
silent source-map corruption. Replace the silent fallbacks with `expect`
panics that match the existing convention in `add_source` / `add_name`, so
overflow surfaces loudly instead of corrupting mapping output. Document the
invariant on the public methods and lock it in with a unit test.

## Files Touched

- `crates/tsz-common/src/source_map/mod.rs`
  (replace `unwrap_or(i32::MAX)` with `expect` in `encode_segment`; document
  the invariant on `generate` / `generate_json` / `generate_inline`)
- `crates/tsz-common/tests/source_map.rs`
  (add test that valid `i32`-fitting values roundtrip and that a value
  greater than `i32::MAX` panics rather than silently corrupting output)

## Verification

- `cargo test -p tsz-common --lib --tests` → 418 passed
- `cargo test -p tsz-emitter` → all green (no source-map regressions)
- `cargo test -p tsz-core --tests source_map` → 931 passed
