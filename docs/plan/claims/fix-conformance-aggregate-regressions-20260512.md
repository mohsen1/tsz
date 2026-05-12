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

## 2026-05-12 update: bundlerSyntaxRestrictions and co/contra check

Fixed the extra TS2309 in `TypeScript/tests/cases/conformance/moduleResolution/bundler/bundlerSyntaxRestrictions.ts`: an empty `export {}` module marker no longer counts as another exported element for export-assignment conflict checking.

Also checked `TypeScript/tests/cases/compiler/coAndContraVariantInferences3.ts`; it now passes on this branch.

Verification:
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts1203_node_esm_tests export_equals_with_empty_export_marker_does_not_emit_ts2309 -- --nocapture` -> passed
- `./scripts/conformance/conformance.sh run --filter "bundlerSyntaxRestrictions" --verbose` -> 1/1 passed
- `./scripts/conformance/conformance.sh run --filter "coAndContraVariantInferences3" --verbose` -> 1/1 passed

## 2026-05-12 update: variadicTuples2

Fixed the remaining fingerprint-only drift in `TypeScript/tests/cases/conformance/types/tuple/variadicTuples2.ts`: tuple source display now widens a trailing boolean literal to `boolean` when the mapped variadic tuple suffix slot is non-boolean, while preserving boolean literals for boolean rest segments.

Verification:
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --lib variadic_rest_tuple_trailing_mismatch_reports_single_error -- --nocapture` -> passed
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts2322_tests tuple_source_display_widens_boolean_literals_past_fixed_target_slots -- --nocapture` -> passed
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts2322_tests exact_optional_tuple_source_display_preserves_boolean_literal_elements -- --nocapture` -> passed
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts2322_tests variadic_tuple_source_display_maps_middle_positions_to_rest_before_suffix -- --nocapture` -> passed
- `./scripts/conformance/conformance.sh run --filter "variadicTuples2" --verbose` -> 1/1 passed

## 2026-05-12 remaining sampled regressions

Additional targeted checks:
- `./scripts/conformance/conformance.sh run --filter "variadicTuples2" --verbose` -> 1/1 passed on this branch
- `./scripts/conformance/conformance.sh run --filter "keyRemappingKeyofResult" --verbose` -> still fingerprint-only; TS2322 code/span match, target display expands `keyof Remapped` instead of preserving alias
- `./scripts/conformance/conformance.sh run --filter "enumLiteralAssignableToEnumInsideUnion" --verbose` -> still fingerprint-only; target displays expanded enum members instead of enum name
- `./scripts/conformance/conformance.sh run --filter "enumLiteralTypes3" --verbose` -> still fingerprint-only; target displays `Choice.No | Choice.Yes` instead of alias `YesNo`
- `./scripts/conformance/conformance.sh run --filter "stringEnumLiteralTypes3" --verbose` -> still fingerprint-only; target displays expanded enum-member union instead of alias `YesNo`
- `./scripts/conformance/conformance.sh run --filter "recursiveTypeReferences1" --verbose` -> still fingerprint-only; extra expanded recursive alias TS2322 display

Next likely slice: diagnostic display alias preservation for type-alias unions/keyof results, not semantic checking.
