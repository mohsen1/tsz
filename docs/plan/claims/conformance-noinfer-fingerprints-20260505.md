# [WIP] fix(conformance): align NoInfer diagnostic fingerprints

- **Date**: 2026-05-05
- **Branch**: `conformance/noinfer-fingerprints-20260505`
- **PR**: #3342
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The random conformance picker selected
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/noInfer.ts`.
The code families match `tsc` (`TS2322`, `TS2345`, `TS2353`, `TS2741`), but
the diagnostic fingerprints differ. Direct CLI comparison shows at least one
missing property-context `TS2322` for `NoInfer<T>` nested inside an object
property and a stale-literal display drift in a `TS2345` missing-property
message. This PR will root-cause those NoInfer inference/display differences
through the owning checker/solver boundary instead of filtering diagnostics by
fixture.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/signatures.rs`
  - prevents mutable property write-type inference from bypassing nested
    `NoInfer<T>` markers, including markers reached through alias expansion.
- `crates/tsz-checker/src/types/type_literal_checker.rs`
  - treats compiler-managed intrinsics from cloned lib arenas as unshadowed, so
    CLI/lib `NoInfer<T>` references lower to the solver's real marker.
- `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`
  - widens anonymous object literal displays in missing-property related info
    for the `assertEqual(g, { x: 3 })` fingerprint.
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
  - regression for `NoInfer<T>` nested in object properties.
- `crates/tsz-checker/src/error_reporter/render_request_tests.rs`
  - regression for widened missing-property related info.

## Verification

- Passed: `cargo fmt --check`
- Passed: `CARGO_INCREMENTAL=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=.target cargo test -j 1 --package tsz-checker --test generic_call_inference_tests noinfer_blocks_candidates_nested_in_object_properties -- --nocapture`
- Passed: `CARGO_INCREMENTAL=0 RUSTFLAGS='-C debuginfo=0' CARGO_TARGET_DIR=.target cargo test -j 1 --package tsz-checker --lib call_missing_property_related_info_widens_fresh_inferred_target -- --nocapture`
- Passed before trimming a removed fallback recheck: `.target/debug/tsz --pretty false TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/noInfer.ts` reported 10 diagnostics with 4 `TS2322`, including line 42.
- Environment note: subsequent low-debug CLI rebuild attempts were interrupted by disk pressure/SIGTERM before a final post-trim CLI rerun; the focused solver/lib regression covers the final retained code path.
