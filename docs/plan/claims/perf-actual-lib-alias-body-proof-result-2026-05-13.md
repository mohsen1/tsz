# Claim: Actual-lib alias body proof result

Date: 2026-05-13
Status: stacked behind `perf-actual-lib-alias-body-outcomes-2026-05-13`

## Claim

The actual-lib alias-body helper should return a typed proof result before any
generic alias widening happens. The proof result carries the proven alias body,
the `DefinitionStore` `DefId`, alias type parameters, and the proof outcome
while preserving the current caller behavior.

This is a behavior-neutral plumbing slice. It does not admit more aliases, does
not change lib delegation cache-hit behavior, and keeps generic aliases and
`PropertyKey` on fallback.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - adds `DirectActualLibAliasBodyProof`.
  - changes `direct_actual_lib_type_alias_body` to return the proof object.
  - carries `DirectActualLibAliasBodyOutcome::Success` on successful proof so
    later generic application can consume the typed proof and its measured
    outcome together.
  - keeps the current `direct_actual_lib_symbol_type` external return shape by
    destructuring the proof back into `(TypeId, Vec<TypeParamInfo>)`.
  - preserves the existing outcome-counter recording and conservative name
    gate.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `cargo test -p tsz-common perf_counters::json_tests -- --nocapture`
- `cargo check -p tsz-checker`
- `cargo fmt --all --check`
- `git diff --check`
