# Contextual-Typing Campaign Continuation Prompt

## Current State (as of 2026-03-15, latest main)

**Conformance**: 10811/12581 (85.9%) on latest main. All contextual-typing commits merged.

**What was fixed** (9 commits, all merged):
1. Intra-expression inference for object literals with all-sensitive properties
2. Literal type preservation with primitive constraints (`<T extends string>`)
3. Contextual return type for zero-arg generic calls (`from<T>(): T[]`)
4. `this` type substitution in implements checking
5. Union inference for all-primitive conflicting literal bases
6. Intra-expression inference for tuple/array literals
7. Method declaration handling (top-level sensitivity, partial extraction via `get_type_of_function`, bare type param targets, nested object recursion)
8. TS2528 instead of TS2323 for duplicate default exports (+19 tests)

**Tests newly passing** (confirmed on latest main):
- `contextualPropertyOfGenericMappedType.ts` — now passes (fixed by recent main changes)
- `noCircularitySelfReferentialGetter1/2.ts` — now passes (TS7022 fix from another agent)

## Remaining Failure Breakdown (~1770 tests)

| Category | Count | Difficulty | Notes |
|----------|-------|-----------|-------|
| Fingerprint-only (right codes, wrong position/message) | ~620 | Medium | Error message formatting. Bulk changes break as many as they fix. |
| Pure false positives (expected=[], we emit errors) | ~150 | Medium-Hard | TS2322 (39), TS2339 (23), TS2345 (18), TS7006 (8), TS2741 (6) |
| False negatives (we emit 0, tsc expects errors) | ~280 | Hard | Multi-file, definite assignment, missing lib types |
| Code mismatches | ~100 | Medium | TS2322↔TS2345 (4), TS2741↔TS2322 (6), TS2834→TS2835 (5) |
| Deep type system issues | ~620 | Hard | Subtype checking, narrowing, mapped types, generics |

## Highest-Impact Next Steps

### 1. TS2322↔TS2345 Diagnostic Elaboration (4 tests)
**Status**: Investigated extensively. The issue is that for callback return type mismatches, we report TS2345 at the argument level while tsc reports TS2322 at the specific incompatible property inside the return expression. The inner `check_assignable_or_report` at `function_type.rs:1656` fires but gets overridden by the outer TS2345.
**Difficulty**: High. Requires changing diagnostic priority/suppression in the call resolution path.
**Tests**: `circularResolvedSignature.ts`, `coAndContraVariantInferences6.ts`, `invariantGenericErrorElaboration.ts`, `intraExpressionInferences.ts`

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

## Dead Ends (don't re-investigate)

- **Bulk `Array<X>` → `X[]` formatting**: tsc uses BOTH formats (108 fingerprints contain `Array<`). Changing all breaks as many tests as it fixes. Evaluating all Application types before formatting also net-negative.
- **Method sensitivity inside object literals**: Changing methods from "always sensitive" to "check params" inside the OBJECT_LITERAL branch of `is_contextually_sensitive` breaks contextual typing for methods. Only the TOP-LEVEL can check params.
- **`has_conflicting_literal_bases` for objects**: Objects don't have primitive bases, so the heuristic doesn't apply. The mapped type inference issue for objects is about constraint COLLECTION, not resolution.
- **Evolving array types** (`controlFlowArrays.ts`): `let x; x = []; x.push(5)` — narrowing campaign territory.
- **Multi-file tests**: Tests with `// @Filename:` directives need cross-file compilation support.
- **Missing lib types** (AsyncGenerator, Disposable): Need lib definition additions, not type system fixes.

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
