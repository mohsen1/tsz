# Conformance 100% Strategy

**Date**: 2026-03-03
**Baseline**: 9,886/12,570 (78.7%)
**Target**: 12,570/12,570 (100%)
**Gap**: 2,684 tests

## Executive Summary

Five-wave approach organized by difficulty and impact, with workers parallelized by error code within each wave. Each wave has clear exit criteria before advancing.

| Wave | Target | Tests to fix | Cumulative |
|------|--------|-------------|------------|
| 1 — Quick Wins | 83% | ~550 | 10,436 |
| 2 — Close-to-Passing | 90% | ~940 | 11,374 |
| 3 — False Positive Elimination | 92% | ~250 | 11,624 |
| 4 — Missing Diagnostics | 95% | ~320 | 11,944 |
| 5 — Hard Solver Gaps | 100% | ~626 | 12,570 |

## Wave 1: Quick Wins (78.7% → ~83%)

**Strategy**: Fix tests that are exactly 1 error code away from passing. Workers organized by error code.

### 1a. Remove one extra code (306 tests)

Tests that pass if we stop emitting one false diagnostic:

| Code | Tests | Root cause pattern |
|------|-------|--------------------|
| TS2322 | 72 | Over-eager assignability in contextual/generic inference |
| TS2339 | 59 | Property not found on narrowed types, intersection members |
| TS2345 | 46 | Argument mismatch from wrong inference or missing covariance |
| TS7006 | 7 | Implicit any where tsc infers from context |
| TS7053 | 6 | Element access on types with valid index signatures |
| TS2307 | 6 | Module resolution false negatives |
| TS2304 | 6 | Variable not found (scope resolution issues) |
| TS2741 | 6 | Missing property (over-strict structural checks) |
| TS2353 | 6 | Excess property on types with index signatures |
| Others | 92 | Various smaller codes (3-5 tests each) |

### 1b. Add one missing code (356 tests)

Tests that pass if we emit one additional diagnostic:

| Code | Tests | Implementation needed |
|------|-------|--------------------|
| TS2322 | 54 | Assignment checks on index sigs, generic variance, mapped types |
| TS2339 | 40 | Property access checks on merged/augmented types |
| TS2345 | 34 | Argument checks for iterators, template strings, rest params |
| TS2307 | 14 | Module not found for specific resolution patterns |
| TS2304 | 12 | Undeclared variable in specific scoping contexts |
| TS2300 | 11 | Duplicate identifier detection gaps |
| TS1005 | 10 | Expected token (parser recovery) |
| TS2741 | 8 | Missing property in structural assignment |
| TS2454 | 8 | Variable used before assigned |
| Others | 165 | Various smaller codes |

### 1c. TS5107 cross-cutting cleanup (59 tests)

**Root cause**: We emit TS5107 (deprecated option) alongside parser errors. In tsc, parser errors prevent TS5107 from being emitted. Most of these tests have parser error gaps — fixing the parser error gaps from 1b will automatically resolve many TS5107 false positives.

**Approach**: Fix parser error gaps first, then add suppression logic for remaining cases.

### Wave 1 Worker Assignment

| Worker | Codes | Est. tests | Focus area |
|--------|-------|-----------|------------|
| W1-TS2322 | TS2322 | 126 | checker/solver assignability, generic inference |
| W1-TS2339 | TS2339 | 99 | property checker, narrowing, intersections |
| W1-TS2345 | TS2345 | 80 | call checker, argument validation, covariance |
| W1-parser | TS1005,TS1109,TS1128 | 24 | parser recovery, expected tokens |
| W1-scope | TS2304,TS2307,TS2300,TS2305 | 43 | module resolution, scope, duplicates |
| W1-misc | TS2741,TS2454,TS2344,TS7053,etc | 80 | various checker features |
| W1-TS5107 | TS5107 | 59 | deprecation priority, parser gating |

## Wave 2: Close-to-Passing (83% → ~90%)

**Strategy**: 1,488 tests with diff <= 2. After Wave 1 resolves many, attack systematic patterns in remaining.

### Systematic categories after Wave 1

| Category | Est. remaining | Root cause |
|----------|---------------|------------|
| Parser errors (TS1005/1109/1128/1434) | ~120 | Parser recovery differs from tsc |
| Contextual inference (TS7006/7022/7005) | ~35 | Missing contextual type propagation |
| Strict null (TS18048/18046/18047) | ~20 | Possibly-null checks not emitted |
| Remaining big-3 edge cases | ~150 | Complex patterns in TS2322/2339/2345 |
| TS2416 class implements | ~10 | Method signature compatibility |
| TS2451 block-scoped redeclaration | ~8 | Scope analysis gaps |
| TS2352 type assertion | ~5 | Type assertion validity |
| TS6133 unused locals | ~4 | Unused variable detection |
| Other codes (2-3 each) | ~200 | Long tail of small fixes |

### Wave 2 Worker Assignment

| Worker | Focus | Est. tests |
|--------|-------|-----------|
| W2-parser | Parser recovery alignment with tsc | ~120 |
| W2-inference | Contextual typing, implicit any | ~35 |
| W2-nullsafety | Strict null/undefined checks | ~20 |
| W2-assignability | Remaining TS2322/2339/2345 patterns | ~150 |
| W2-classes | TS2416, TS2515, class member compat | ~20 |
| W2-scope | TS2451, TS2403, TS6133, block scoping | ~20 |
| W2-longtail | Remaining codes, 2-5 tests each | ~200 |

## Wave 3: False Positive Elimination (90% → ~92%)

**Strategy**: Fix 305 tests where we emit errors but tsc expects clean compilation.

### Root cause taxonomy

| Root cause | Tests | Fix location |
|-----------|-------|-------------|
| Inference producing wrong types → false TS2322/2345 | ~130 | Solver inference, contextual typing |
| Narrowing not applied → false TS2339 | ~56 | Flow analysis, type guards |
| Operator checks too strict → false TS2365 | ~22 | Binary expression checker |
| Namespace/module resolution → false TS2503/2693 | ~31 | Binder, module resolver |
| Excess property false alarms → false TS2353 | ~11 | Solver excess property checker |
| Implicit any false alarms → false TS7006 | ~20 | Contextual type propagation |
| Cascading from other false positives | ~35 | Resolves when primary is fixed |

### Key systemic fixes needed

1. **Contextual typing completeness**: Many false TS2322/2345/7006 stem from failing to propagate contextual types through:
   - Generic callback parameters in complex signatures
   - Conditional type branches
   - Mapped type templates
   - Application/indexed-access evaluations

2. **Control flow narrowing gaps**: False TS2339 from:
   - Discriminant narrowing through property chains
   - Type guards on union members with mixed predicates
   - Assignments in control flow paths

3. **Global/ambient type resolution**: False TS2503/2693 from:
   - `globalThis` declarations not merged
   - UMD module augmentations
   - Ambient namespace resolution

## Wave 4: Missing Diagnostics Infrastructure (92% → ~95%)

**Strategy**: Implement ~50 error codes that tsz never emits but tsc requires.

### High-impact unimplemented codes

| Code | Tests | Feature |
|------|-------|---------|
| TS2323 | 8 | Type not assignable (index sig variant) |
| TS7017 | 6 | Index sig implicitly has 'any' |
| TS2742 | 5 | Inferred type cannot be named |
| TS2550 | 5 | Property has no initializer (exactOptionalPropertyTypes) |
| TS17019/17020 | 9 | Resolution mode assertions |
| TS1181 | 5 | Invalid 'in' expression |
| TS2657 | 5 | JSX spread children |
| TS2833 | 4 | Relative import paths |
| TS9007 | 4 | Declaration emit isolation |
| TS2819 | 4 | Expression not callable (type from spread) |
| TS7014 | 4 | Construct sig return types |
| TS1138 | 4 | Parameter declaration expected |
| TS2343 | 4 | Type parameter constraint violation |
| TS2862 | 3 | Cannot write to (generic index) |
| ~36 more | ~80 | Various 1-3 test codes |

### Implementation approach
- Group by checker location (same file handles related codes)
- Prioritize codes that unblock other tests (cascading effect)
- Add unit tests for each new diagnostic

## Wave 5: Hard Solver/Type System Gaps (95% → 100%)

**Strategy**: Deep solver and type system work for the most complex TypeScript patterns.

### Major work items

| Area | Est. tests | Difficulty | Description |
|------|-----------|------------|-------------|
| types/mapped (53.9%) | 12 | Very hard | Homomorphic mapped types, key remapping, template inference |
| types/tuple (64.7%) | 12 | Hard | Variadic tuple inference, labeled elements, rest handling |
| salsa/JS inference (64.7%) | 67 | Hard | Constructor property inference, prototype chains, expando patterns |
| controlFlow (64.9%) | 20 | Hard | Complex discriminant narrowing, assertion functions, IIFE flow |
| jsdoc (65.1%) | 87 | Medium | @callback, @template complex forms, @overload, method predicates |
| Circular types | ~30 | Very hard | Coinductive resolution, circular constraints, infinite depth |
| Conditional types | ~20 | Hard | Deferred distribution, nested conditionals, infer in extends |
| Generic inference | ~100 | Very hard | Priority inference, reverse mapped types, contextual generics |
| Overload resolution | ~30 | Hard | Union callables, generic overloads, JSX overloads |
| Type display/formatting | ~50 | Medium | Alias preservation, union ordering, intersection display |

### Approach for Wave 5
- Each area gets dedicated investigation before implementation
- Use conformance tracing (`tsz-tracing` skill) to compare behavior with tsc
- Focus on solver-level fixes that cascade to multiple tests
- Accept that some tests may require fundamental algorithm changes

## Measurement & Tracking

### Snapshot cadence
- Run `./scripts/conformance/conformance.sh snapshot` after each batch of changes
- Track delta per wave in `docs/todos/conformance.md`

### Exit criteria per wave
- Wave 1: All one-missing and one-extra quick wins addressed (may not fix 100% due to entanglement)
- Wave 2: All diff<=2 tests addressed; pass rate >= 89%
- Wave 3: False positive count < 50 (from 305)
- Wave 4: All codes needed by >= 3 tests implemented
- Wave 5: Pass rate >= 98% (last 2% may require tsc-specific quirk matching)

### Risk factors
- **Entangled fixes**: Fixing one code may regress another (monitor regressions per snapshot)
- **Solver recursion**: Deep type system changes can cause stack overflows or infinite loops
- **Parser divergence**: Our parser may accept/reject different syntax than tsc's parser
- **Diminishing returns**: Last 5% will take disproportionate effort
