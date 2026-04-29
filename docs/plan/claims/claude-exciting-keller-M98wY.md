# fix: cap function return-type inference priority at ReturnType

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-M98wY`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance — genericCallWithFunctionTypedArguments

## Intent

In the Function-Function arm of `constrain_types_impl`, the return type
was being constrained with the incoming priority unchanged. When a
contextually-retyped callback (no longer `is_contextually_sensitive`)
participated in Round 1 of generic inference, its return type was
inadvertently given `NakedTypeVariable` priority — the same tier as direct
value arguments. This caused U to receive `""` at NakedTypeVariable priority,
competing with the correct `1` candidate from `y: U`, and the wrong candidate
won, producing a diagnostic at the wrong argument position.

The fix clamps the return-type constraint priority at `ReturnType` (the
correct priority for function return positions per tsc). Direct-argument
inferences at `NakedTypeVariable` priority will always survive
`filter_candidates_by_priority` over any return-type inference, matching
tsc's two-round inference semantics.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/walker.rs` (7 LOC change)
- `crates/tsz-checker/tests/generic_inference_ordering_tests.rs` (new test file)
- `crates/tsz-checker/Cargo.toml` (register new test)

## Verification

- `cargo test -p tsz-checker --test generic_inference_ordering_tests` — 2 new tests pass
- `cargo test -p tsz-checker --test generic_call_inference_tests` — 71 existing tests pass
- `cargo test -p tsz-checker --test co_contra_inference_tests` — 4 pass
- `cargo test -p tsz-checker --test call_resolution_regression_tests` — 133 pass
- Conformance: `genericCallWithFunctionTypedArguments` error now at correct position (col 18, callback arg) not wrong position (col 46, y arg)
- Broader regression: 298/300 sample pass, no new failures
