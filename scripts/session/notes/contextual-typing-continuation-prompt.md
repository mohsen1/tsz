# Contextual-Typing Campaign Continuation Prompt

## Current State (as of 2026-03-15, latest main)

**Conformance**: ~91.5% on 3000-test sample (2026-03-15, post fix #13). All contextual-typing commits merged.

**What was fixed** (13 commits, all merged):
1. Intra-expression inference for object literals with all-sensitive properties
2. Literal type preservation with primitive constraints (`<T extends string>`)
3. Contextual return type for zero-arg generic calls (`from<T>(): T[]`)
4. `this` type substitution in implements checking
5. Union inference for all-primitive conflicting literal bases
6. Intra-expression inference for tuple/array literals
7. Method declaration handling (top-level sensitivity, partial extraction via `get_type_of_function`, bare type param targets, nested object recursion)
8. TS2528 instead of TS2323 for duplicate default exports (+19 tests)
9. Bare type param targets + nested object recursion in intra-expression inference
10. Preserve callback return-type TS2322 through arg collection filter (circularResolvedSignature)
11. Fix `recover_property_from_implemented_interfaces` — was using `get_type_from_type_node` on ExpressionWithTypeArguments (returns ERROR → ANY via solver default), now uses `resolve_heritage_symbol` + `type_reference_symbol_type` for correct instance type resolution. Also added `TypeData::Error` handling in solver property access to return ERROR instead of ANY.
12. Fix TS7018 false positives — was emitting "implicitly has 'any' type" for object literal properties when initializer evaluates to explicit `any` (from `any` variable). tsc only emits TS7018 when `any` comes from null/undefined widening. Fixes ~15 tests (+6 net on 3000-sample).
13. Fix literal display properties leaking into assignability error messages — `format_type_for_assignability_message` was using `with_display_properties()` which showed pre-widened literal types (e.g., `{ two: 1; }` instead of `{ two: number; }`). Changed to regular formatter. Fixes ~13 fingerprint-only failures (+13 net on 3000-sample).

**Tests newly passing** (confirmed on latest main):
- `contextualPropertyOfGenericMappedType.ts` — now passes (fixed by recent main changes)
- `noCircularitySelfReferentialGetter1/2.ts` — now passes (TS7022 fix from another agent)
- `classImplementsClass6.ts` — now passes (fix #11: implements clause property recovery)
- TS7018 batch (fix #12): `arrayBestCommonTypes`, `declarationEmitOverloadedPrivateInference`, `esNextWeakRefs_IterableWeakMap`, `useBeforeDeclaration_propertyAssignment`, `superCallBeforeThisAccessing7`, `commonJSReexport`, `namespaceImportTypeQuery`, `namespaceImportTypeQuery4`, `YieldExpression20_es6`, `TwoInternalModules*` (4 tests), `exportEqualsProperty2`

## Remaining Failure Breakdown (updated 2026-03-15, post fix #13)

On 3000-test sample: 2745 pass (91.5%), ~255 fail. Includes 81 fingerprint-only failures.

**Fingerprint-only failures** (81 tests — correct error codes, wrong message/position):
- Diverse causes: union member ordering (`null | X | undefined` vs `X | null | undefined`), generic type parameter display (`A<T>` vs `A<number>`), optional parameter `| undefined` stripping, parser error positions, literal types in messages.
- No single pattern dominates. Fix #13 recovered 13 of these; remaining 81 each require individual investigation.

## Highest-Impact Next Steps

### 0. TS2323↔TS2528 Duplicate Default Export (DEAD END — do not re-investigate)
**Status**: Thoroughly investigated. tsc's choice between TS2323 and TS2528 depends on subtle per-test patterns that we can't replicate without deep understanding of tsc's symbol merge logic.
**Key finding**: `multipleExportDefault4.ts` (class + object literal → TS2528) vs `exportDefaultClassAndValue.ts` (const + class → TS2323). Both involve classes but expect different codes. The distinction is NOT based on whether a class is involved.
**Conflict detection bug**: interface+function is treated as OK but should be a conflict. However, fixing this AND changing TS2528→TS2323 breaks `multipleExportDefault1-4,6` (6 tests that correctly pass with TS2528). Fixing conflict detection ALONE (keeping TS2528) makes `exportDefaultInterfaceAndTwoFunctions` worse (adds TS2528 where TS2323 expected). All approaches are net-negative or zero.
**Dead end confirmed**: Any change to TS2323/TS2528 emission breaks as many tests as it fixes.

### 1. TS2322↔TS2345 Diagnostic Elaboration (3 remaining tests)
**Status**: FIXED for `circularResolvedSignature.ts` (commit `52b3a14f6`). The inner TS2322 is pruned by `collect_call_argument_types_with_context` (call_checker.rs:987-1022, filter at line 1009-1016). The fix stores the return-type TS2322 in `callback_return_type_errors` and restores it in `call_result.rs` when processing `ArgumentTypeMismatch`.
**Remaining tests**: `coAndContraVariantInferences6.ts`, `invariantGenericErrorElaboration.ts`, `intraExpressionInferences.ts` — these may have different patterns (generic inference, overloads) that need additional handling.
**Key dead end**: Modifying the callback body filter to keep ALL TS2322 causes 6 regressions (alias*.ts, asyncFunctionContextuallyTypedReturns). The flag-based approach (storing diagnostics separately) is the correct fix.

### 2. TS2741↔TS2322 Swap (6 tests)
**Pattern**: We emit TS2741 ("Property missing") where tsc emits TS2322 ("Type not assignable").
**Tests**: `excessPropertyCheckIntersectionWithIndexSignature.ts`, `fuzzy.ts`, `mappedTypeWithCombinedTypeMappers.ts`, `recursiveIntersectionTypes.ts`, etc.

### 3. TS7006 False Positives for Methods in Mapped Types (8 tests)
**Status**: `contextualPropertyOfGenericMappedType.ts` now passes, but other method contextual typing tests still fail.
**Tests**: `callbackTagNamespace.ts`, `contextualTypedSpecialAssignment.ts`, etc. (mostly JSDoc/Salsa)

### 4. `this` Type in Extends Checking (2-3 tests)
**Status**: `implements` path fixed, `extends` path needs nuanced handling (covariance preservation).
**Tests**: `performanceComparisonOfStructurallyIdenticalInterfacesWithGenericSignatures.ts`

### 5. Mapped Type Inference for Structured Types
**Status**: Primitives fixed (#5), objects/functions still fail. `isomorphicMappedTypeInference.ts` has only 1 lower bound for T in the solver despite multiple properties contributing.

## Root Cause Analysis: `recover_property_from_implemented_interfaces` Bug

**Bug chain**: `get_type_from_type_node(ExpressionWithTypeArguments)` → TypeNodeChecker doesn't handle → falls to `lower_type()` → `_ => TypeId::ERROR` → solver property access on ERROR → default `_ =>` branch returns `PropertyAccessResult::simple(TypeId::ANY)` → recovery function returns `Some(ANY)` → ALL property accesses silently accepted on classes with `implements`.

**Impact**: Every class with an `implements` clause had its property access recovery function returning ANY for any property name, suppressing TS2339 errors. This affected `classImplementsClass6.ts` and potentially many other tests where property access on classes with implements clauses should have errored.

**Fix applied**: Two changes:
1. `recover_property_from_implemented_interfaces` now uses `resolve_heritage_symbol` + `type_reference_symbol_type` to get the correct instance type (not ERROR)
2. Solver property access evaluator now handles `TypeData::Error` explicitly, returning ERROR instead of falling to the ANY-returning default

## Dead Ends (don't re-investigate)

- **Optional parameter `| undefined` in TS2345 messages**: Changing `CallResult::ArgumentTypeMismatch.expected` from `param_type` to `effective_param_type` (with `| undefined`) fixes some fingerprint tests (like `genericFunctionsWithOptionalParameters1`) but causes -9 net regression. tsc's behavior about when to show `| undefined` is context-dependent — not always for optional params.
- **Conditional expression literal widening** (`defaultParameterAddsUndefinedWithStrictNullChecks.ts`): `is_fresh_literal_expression` uses `&&` for conditionals (both branches must be fresh). Changing to `||` fixes parameter widening (`cond ? true : undefined` → `boolean | undefined`) but doesn't fix TS2367 — the comparison overlap check uses a DIFFERENT type resolution path that still sees the pre-widened `true`. Net -1 conformance. The real fix requires making TS2367's operand type resolution use widened types.
- **Bulk `Array<X>` → `X[]` formatting**: tsc uses BOTH formats (108 fingerprints contain `Array<`). Changing all breaks as many tests as it fixes. Evaluating all Application types before formatting also net-negative.
- **Method sensitivity inside object literals**: Changing methods from "always sensitive" to "check params" inside the OBJECT_LITERAL branch of `is_contextually_sensitive` breaks contextual typing for methods. Only the TOP-LEVEL can check params.
- **`has_conflicting_literal_bases` for objects**: Objects don't have primitive bases, so the heuristic doesn't apply. The mapped type inference issue for objects is about constraint COLLECTION, not resolution.
- **Evolving array types** (`controlFlowArrays.ts`): `let x; x = []; x.push(5)` — narrowing campaign territory.
- **Multi-file tests**: Tests with `// @Filename:` directives need cross-file compilation support.
- **Missing lib types** (AsyncGenerator, Disposable): Need lib definition additions, not type system fixes.
- **TS2741→TS2322 in explain.rs**: The `explain_failure_inner` function receives fully-evaluated types (Object shapes), NOT the original Application/Lazy types. So checking for Application same-base or intersection at that level doesn't work — the type structure is already lost. The fix needs to happen at a HIGHER level (checker's error reporting path) where the original type information is still available. `recursiveIntersectionTypes.ts` uses `type LinkedList<T> = T & { next: LinkedList<T> }` (intersection alias) which gets merged into a single Object during evaluation.

## Key Learnings / Gotchas

1. **Shared `CARGO_TARGET_DIR`**: Other agents overwrite binaries. Always use `CARGO_TARGET_DIR=.local-target`.
2. **TypeScript submodule**: Gets cleaned constantly. Run `git submodule update --force TypeScript` before EVERY conformance run.
3. **`is_contextually_sensitive` TWO levels**: Top-level (bare nodes) vs inside-object-literal (elements). Methods ALWAYS sensitive inside objects.
4. **`get_type_of_node` vs `get_type_of_function`**: METHOD_DECLARATION returns ERROR from `get_type_of_node`.
5. **Architecture guardrails**: No `lookup()` or `TypeData` in checker. Use `type_queries::*`.
6. **Pre-commit hooks**: 5+ minutes, may fail on pre-existing clippy errors from other crates.
7. **`.local-target` in git**: NEVER commit the local target directory (18GB). Add to `.gitignore`.

## How to Start

```bash
# 1. Setup
cd /Users/mohsen/code/tsz/.worktrees/contextual-typing
git fetch origin && git rebase origin/main

# 2. Restore TypeScript submodule
cd /Users/mohsen/code/tsz
rm -f .git/modules/TypeScript/index.lock
git submodule update --force TypeScript

# 3. Build with isolated target
cd /Users/mohsen/code/tsz/.worktrees/contextual-typing
CARGO_TARGET_DIR=.local-target cargo build --profile dist-fast -p tsz-cli -p tsz-conformance

# 4. Run conformance
cd /Users/mohsen/code/tsz
.worktrees/contextual-typing/.local-target/dist-fast/tsz-conformance \
  --test-dir "TypeScript/tests/cases" \
  --cache-file "scripts/conformance/tsc-cache-full.json" \
  --tsz-binary ".worktrees/contextual-typing/.local-target/dist-fast/tsz" \
  --max 2000
```
