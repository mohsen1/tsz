# fix(solver): keep constraint display fallback call-scoped

- **Date**: 2026-05-10
- **Branch**: `fix/maxconstraints-display-review-cleanup-2026-05-10`
- **PR**: #4978
- **Status**: ready
- **Workstream**: diagnostic-conformance

## Intent

Follow up on review feedback from the literal constraint display fix. The
fallback display for rejected literal candidates is now kept on the specific
argument mismatch result instead of mutating the global interner display alias
for the evaluated constraint type.

This also avoids cloning the literal candidate list and avoids cloning the
entire substitution map just to override one type parameter for display.

## Files Touched

- `crates/tsz-solver/src/instantiation/instantiate.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`

## Verification

- `cargo fmt --all --check`
- `cargo test -p tsz-checker --test generic_call_inference_tests self_referential_constraint_fallback -- --nocapture`
- `cargo build --profile dist-fast -p tsz-cli --bin tsz -p tsz-conformance`
- `tsz-conformance --filter maxConstraints` (1/1 passed)
