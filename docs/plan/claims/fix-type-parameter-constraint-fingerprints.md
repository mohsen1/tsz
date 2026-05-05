# fix(checker): align type parameter constraint diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/type-parameter-constraint-fingerprints`
- **PR**: https://github.com/mohsen1/tsz/pull/3426
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/typeParameters/typeArgumentLists/typeParameterAsTypeParameterConstraint2.ts`.
The current code set already matches TypeScript (`TS2322`, `TS2345`, and
`TS2454`), so the fix will inspect diagnostic anchors, messages, and displayed
types for the remaining fingerprint divergence.

## Summary

The generic-call resolver already inferred `T = number` and
`U = NumberVariant` for `<T, U extends T>(1, n)`, but the post-resolution
constraint check allowed lazy interface results from direct argument inference
to bypass `U extends T`. The fix keeps the existing lazy-type escape hatch for
non-direct/contextual inference while validating direct argument substitutions,
so `U` falls back to the instantiated `T` constraint and the final argument
check emits the missing `TS2345` at `n`.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target CARGO_INCREMENTAL=0 cargo nextest run --target-dir .target -p tsz-checker --test generic_call_inference_tests dependent_type_parameter_constraint_checks_second_argument_against_first_inference`
- `CARGO_TARGET_DIR=.target CARGO_INCREMENTAL=0 cargo nextest run --target-dir .target -p tsz-checker --test generic_call_inference_tests`
- `CARGO_TARGET_DIR=.target CARGO_INCREMENTAL=0 cargo build --target-dir .target --profile dist-fast -j 4 -p tsz-cli -p tsz-conformance`
- `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --filter 'typeParameterAsTypeParameterConstraint2' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/type-parameter-constraint-fingerprints --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
