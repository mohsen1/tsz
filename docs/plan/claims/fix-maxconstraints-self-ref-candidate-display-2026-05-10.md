# fix(solver): preserve literal constraint display candidates

- **Date**: 2026-05-10
- **Branch**: `fix/maxconstraints-self-ref-candidate-display-2026-05-10`
- **PR**: #4967
- **Status**: ready
- **Workstream**: diagnostic-conformance

## Intent

`maxConstraints.ts` still failed after the generic-call display cleanup when
the generic function came from a contextual assignment. Constraint fallback
correctly used the widened primitive for assignability, but it also erased
the literal candidate union that TypeScript keeps for the instantiated
constraint display.

This preserves that literal candidate union as display provenance at the
constraint-fallback boundary without rewriting user-declared parameter
types or changing assignability.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `cargo test -p tsz-checker --test generic_call_inference_tests self_referential_constraint_fallback -- --nocapture`
- `cargo build --profile dist-fast -p tsz-cli --bin tsz -p tsz-conformance`
- `tsz-conformance --filter maxConstraints` (1/1 passed, fingerprint-only 0)
- pre-commit hook: clippy, wasm warning gate, architecture guardrails, and 22,390 affected tests
