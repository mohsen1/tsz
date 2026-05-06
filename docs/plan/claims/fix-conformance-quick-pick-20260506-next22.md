# fix(checker): suppress recursive typeof redeclaration cascade

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next22`
- **PR**: #3701
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claimed `TypeScript/tests/cases/conformance/types/specifyingTypes/typeQueries/recursiveTypesWithTypeof.ts`.

Current `origin/main` emits the expected `TS2454` and `TS2502`, but also emits
an extra `TS2403` for:

```ts
var f: Array<typeof f>;
var f: any;
```

This slice aligns the recursive `typeof` redeclaration diagnostics with tsc by
carrying the TS2502 circular annotation result into TS2403's raw declared type
selection. Circular annotations establish `any` for redeclaration comparison,
without weakening ordinary incompatible variable redeclaration reporting.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/src/state/variable_checking/core_tests.rs`
- `docs/plan/claims/fix-conformance-quick-pick-20260506-next22.md`

## Verification

- `cargo fmt --all`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker recursive_types_with_typeof_no_false_ts2403`
- `./scripts/conformance/conformance.sh run --filter "recursiveTypesWithTypeof" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `git diff --check`
- `scripts/architecture-check.sh --quick`
- `CARGO_TARGET_DIR=.target/nextest-local cargo clippy -p tsz-checker --lib -- -D warnings`
