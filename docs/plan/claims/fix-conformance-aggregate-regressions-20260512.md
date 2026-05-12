# Claim: fix conformance aggregate regressions blocking ready PRs

Status: claim
Owner: Codex
Branch: fix-conformance-aggregate-regressions-20260512
Started: 2026-05-12

## Scope

Investigate and fix the current conformance aggregate regressions that make ready PRs fail against the 12,580 snapshot, starting with the recurring regression list from PR #5911/#5916 aggregate CI.

## Current evidence

CI aggregate on PR #5911 at run 25746688287 reports 12,573/12,585, below snapshot floor 12,580, with these regressions:
- TypeScript/tests/cases/compiler/coAndContraVariantInferences3.ts
- TypeScript/tests/cases/compiler/correlatedUnions.ts
- TypeScript/tests/cases/compiler/enumLiteralAssignableToEnumInsideUnion.ts
- TypeScript/tests/cases/compiler/keyRemappingKeyofResult.ts
- TypeScript/tests/cases/conformance/moduleResolution/bundler/bundlerSyntaxRestrictions.ts
- TypeScript/tests/cases/conformance/types/literal/enumLiteralTypes3.ts
- TypeScript/tests/cases/conformance/types/literal/stringEnumLiteralTypes3.ts
- TypeScript/tests/cases/conformance/types/tuple/variadicTuples2.ts
- TypeScript/tests/cases/conformance/types/typeRelationships/recursiveTypes/recursiveTypeReferences1.ts

## Verification plan

Run targeted conformance filters for selected regressions, add focused checker tests where practical, then run broader conformance checks before marking ready.

## 2026-05-12 update: correlatedUnions

Fixed the extra TS2345 in `TypeScript/tests/cases/compiler/correlatedUnions.ts` for correlated indexed-access arguments passed to union callees whose synthetic parameter display reduces to `never`.

Verification:
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --lib correlated_index_access_argument_satisfies_union_callee_param_union -- --nocapture` -> passed
- `./scripts/conformance/conformance.sh run --filter "correlatedUnions" --verbose` -> 1/1 passed
