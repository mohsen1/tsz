# fix(checker): restore conformance after #6070

- **Date**: 2026-05-13
- **Branch**: `codex/fix-main-conformance-after-6070`
- **PR**: #6079
- **Status**: ready
- **Workstream**: CI unblock

## Intent

Restore the seven conformance tests reported by the #6071 aggregate run so
performance documentation and follow-up T2.2 PRs can merge through the normal
required CI path.

Follow-up comparison showed the failures reproduced before #6070 as well; #6070
was the first full-CI PR to expose the current `main` aggregate drift, not the
root cause.

## Failing Tests

- `TypeScript/tests/cases/compiler/asyncYieldStarContextualType.ts`
- `TypeScript/tests/cases/compiler/coAndContraVariantInferences3.ts`
- `TypeScript/tests/cases/compiler/genericMethodOverspecialization.ts`
- `TypeScript/tests/cases/compiler/noImplicitReturnsExclusions.ts`
- `TypeScript/tests/cases/compiler/yieldStarContextualType.ts`
- `TypeScript/tests/cases/conformance/externalModules/typeOnly/mergedWithLocalValue.ts`
- `TypeScript/tests/cases/conformance/externalModules/valuesMergingAcrossModules.ts`

## Verification Plan

- Run focused conformance filters for each failing test.
- Run targeted checker tests around #6070's TS2852 behavior.
- Run the relevant checker unit suite if the fix touches shared checking paths.

## Result

Fixed the current aggregate drift by:

- Treating an `unknown` containing-generator `TNext` as insufficient evidence for
  TS2766 in `yield*` delegation.
- Restoring TS7030 behavior for annotated returns whose union contains
  `undefined` but not `void`/`any`.
- Narrowing imported companion value lookup so type-only/local value merges still
  work, while conflicted re-exports with local namespaces do not resolve through
  the imported function value.

Verification:

- `./scripts/conformance/conformance.sh run --filter asyncYieldStarContextualType --verbose --workers 1` -> 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter yieldStarContextualType --verbose --workers 1` -> 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter noImplicitReturnsExclusions --verbose --workers 1` -> 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter mergedWithLocalValue --verbose --workers 1` -> 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter valuesMergingAcrossModules --verbose --workers 1` -> 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter coAndContraVariantInferences3 --verbose --workers 1` -> 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter genericMethodOverspecialization --verbose --workers 1` -> 1/1 passed.
- `cargo test -p tsz-checker --test imported_companion_value_type_tests -- --nocapture` -> 2 passed.
- `cargo test -p tsz-checker --lib yield_star_return_type_tests::yield_star_allows_unknown_containing_next_type -- --nocapture` -> passed.
- `cargo test -p tsz-checker --lib ts7030_undefined_union_return_tests -- --nocapture` -> 4 passed.
- `cargo fmt --all -- --check` -> passed.
