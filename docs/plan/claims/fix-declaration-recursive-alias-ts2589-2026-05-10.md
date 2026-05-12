# fix(checker): defer parameter-dependent recursive alias TS2589

- **Date**: 2026-05-10
- **Branch**: `fix/declaration-recursive-alias-ts2589-2026-05-10`
- **PR**: #4977
- **Status**: shipped
- **Workstream**: diagnostic-conformance

## Intent

Definition-site TS2589 should only fire for recursive conditional aliases
whose self-instantiation is stable at the alias declaration, or for the
existing unresolved computed recursive case. Recursive calls whose type
arguments still depend on the alias's scoped type parameters through helper
aliases are deferred until an actual instantiation site.

This fixes the extra TS2589 in
`declarationEmitRecursiveConditionalAliasPreserved.ts` without weakening the
`recursiveConditionalCrash4.ts` definition-site diagnostic.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `crates/tsz-checker/tests/ts2589_tests.rs`

## Verification

- `cargo fmt --all --check`
- `cargo test -p tsz-checker --lib ts2589_tests -- --nocapture`
- `cargo build --profile dist-fast -p tsz-cli --bin tsz -p tsz-conformance`
- `tsz-conformance --filter declarationEmitRecursiveConditionalAliasPreserved` (1/1 passed)
- `tsz-conformance --filter recursiveConditionalCrash4` (1/1 passed)
