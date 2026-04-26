# fix(checker): stop typeof-chain TS2456 walker at structural-wrapper kinds

- **Date**: 2026-04-26
- **Branch**: `fix/typeof-chain-structural-wrapper-stop-walk`
- **PR**: #1355
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the spurious extra TS2456 on
`compiler/unionTypeWithRecursiveSubtypeReduction3.ts` and similar tests where
a `typeof <var>` alias's target variable has an annotation that only
references the alias from inside a structurally-wrapped position.

Per the TypeScript spec
(`conformance/types/typeAliases/directDependenceBetweenTypeAliases.ts`):

> A type query directly depends on the type of the referenced entity.
> A type literal does NOT directly depend on its property types.

`ast_finds_resolution_chain_alias` (used by
`typeof_target_annotation_refs_resolution_chain` in
`computed_helpers.rs`) walked through ALL child AST nodes of the target
variable's annotation, including children of `TYPE_LITERAL`,
`FUNCTION_TYPE`, `CONSTRUCTOR_TYPE`, and `MAPPED_TYPE`. That made
`type T27 = typeof a27; declare var a27: { prop: number } | { prop: T27 };`
look like a direct circular reference even though the only self-reference
hides inside `{ prop: T27 }`, where tsc treats it as a structurally-deferred
recursive type.

The fix stops the walker at those four wrapper kinds. Direct-dependency
kinds (`UNION_TYPE`, `INTERSECTION_TYPE`, `ARRAY_TYPE`, `TUPLE_TYPE`,
`TYPE_QUERY`, `TYPE_REFERENCE`) keep descending so the existing
`type T = typeof x; var x: T[]` cycle still emits TS2456.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs`
  (~15 LOC: `ast_finds_resolution_chain_alias` skips `TYPE_LITERAL`,
   `FUNCTION_TYPE`, `CONSTRUCTOR_TYPE`, `MAPPED_TYPE`)
- `crates/tsz-checker/tests/type_alias_typeof_circular_tests.rs`
  (+22 LOC, one new test pinning the structural-wrapper case)

## Verification

- `cargo nextest run -p tsz-checker --test type_alias_typeof_circular_tests`
  (5 PASS)
- `cargo nextest run -p tsz-checker -E 'test(circular)'` (25 PASS)
- `cargo nextest run -p tsz-checker -E 'test(/type_alias|recursive|union_type|TS2456/)'`
  (110 PASS)
- `cargo nextest run -p tsz-checker --lib` (2852 PASS, no regressions)
- `cargo nextest run -p tsz-core -E 'test(/circular|2456|recursive/)'`
  (18 PASS)
- Conformance impact: `unionTypeWithRecursiveSubtypeReduction3.ts` flips
  from FAIL (TS2322 + extra TS2456) to PASS (TS2322 only). Other
  candidates with the same shape may also flip.
