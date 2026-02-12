# Conformance Test Analysis - Slice 1

**Date:** 2026-02-12
**Slice:** 1 of 4 (tests 0-3,146 of 12,583 total)
**Baseline Pass Rate:** 68.2% (2,142/3,139 passing)

## Summary

Analyzed 997 failing tests in slice 1:
- **326 false positives** - we emit errors when TSC doesn't (HIGH PRIORITY)
- **280 all-missing** - TSC emits errors, we emit nothing
- **391 wrong-code** - both emit errors but different codes
- **244 close to passing** - differ by only 1-2 error codes

## High-Impact Issues

### 1. False Positives (326 tests)

These are the biggest problem - we're incorrectly rejecting valid TypeScript code.

**Top offenders:**
- **TS2345**: 118 false positives (argument type errors)
- **TS2322**: 110 false positives (assignment errors)
- **TS2339**: 94 false positives (property access errors)

**Common patterns:**
- Module import/export type compatibility issues
- Type alias usage in generic constraints
- Type guard predicate handling in Array.find and similar methods

### 2. Type Guard Predicate Bug (CRITICAL)

**Test:** `arrayFind.ts` and related
**Issue:** Array.find() with type guard predicates doesn't narrow return type

```typescript
function isNumber(x: any): x is number {
  return typeof x === "number";
}

const arr = ["string", false, 0];
const result: number | undefined = arr.find(isNumber);
// ❌ tsz: Type 'string | boolean | number | undefined' is not assignable to 'number | undefined'
// ✅ tsc: no error
```

**Root cause:** `crates/tsz-solver/src/operations.rs:990-991`
```rust
let return_type = instantiate_type(self.interner, func.return_type, &final_subst);
CallResult::Success(return_type)
```

The code ignores `func.type_predicate` when returning the call result. For type guard predicates, the semantic return type should be derived from the predicate's narrowed type, not just `boolean`.

**Impact:** Affects multiple tests with type guards in callbacks.

### 3. Missing Error Codes

**TS2740** (missing in 15+ tests): "Type is missing properties from type"
- We emit TS2322 but not the more specific TS2740
- Currently only emit TS2740 when 5+ properties are missing
- TSC emits both TS2322 AND TS2740 in some cases (needs investigation)

**Not implemented** (224 error codes never emitted):
- TS2792 (15 tests), TS2323 (9 tests), TS2301 (8 tests), etc.
- These require implementing entirely new error checks

### 4. Module Type Compatibility

**Pattern:** Tests with `import Backbone = require(...)` fail incorrectly

**Example:** `aliasUsageInArray.ts`, `aliasUsageInGenericFunction.ts`
- Module objects not being recognized as compatible with interfaces
- Likely issue with how module types are represented/compared

## Recommended Fixes (Priority Order)

### Priority 1: Type Guard Predicates
**Effort:** Medium | **Impact:** ~10-20 tests

Fix `CallEvaluator::resolve_generic_call` to:
1. Check if selected overload has `type_predicate`
2. If yes, substitute predicate type instead of return_type
3. Handle both function predicates and method predicates

### Priority 2: False Positive Investigation
**Effort:** High | **Impact:** 300+ tests

Requires systematic debugging of why we emit errors TSC doesn't:
- Add tracing to assignability checks
- Compare with TSC behavior on specific test cases
- Likely multiple distinct issues, not one root cause

### Priority 3: TS2740 Enhancement
**Effort:** Low | **Impact:** ~15 tests

Investigation needed: Why does TSC emit both TS2322 and TS2740?
- May need to emit both diagnostics in certain scenarios
- Check if it's related to specific kinds of type mismatches

### Priority 4: Module Compatibility
**Effort:** Medium-High | **Impact:** ~20-30 tests

Debug module type representation and comparison:
- How are `import = require()` modules typed?
- How should module types be checked against interfaces?
- Likely needs changes to module type handling in checker

## Quick Wins (234 tests needing just ONE error code)

- **TS2322 missing**: 20 tests
- **TS2304 missing**: 9 tests
- **TS2339 missing**: 8 tests
- **TS2353 missing**: 7 tests

These are cases where we're "close" - emitting most errors correctly but missing one specific check.

## Next Steps

1. **Start with type guard fix** - clear root cause, medium effort, good ROI
2. **Profile top false positives** - pick 5-10 examples, debug systematically
3. **Implement missing checks** - pick highest-impact error codes
4. **Iterate and measure** - re-run slice after each fix to track progress

## Tools for Investigation

```bash
# Analyze specific category
./scripts/conformance.sh analyze --offset 0 --max 3146 --category false-positive

# Run specific test with verbose output
./scripts/conformance.sh run --filter "arrayFind.ts" --verbose

# Debug with tracing
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run --bin tsz -- test.ts 2>&1 | head -200
```

## Baseline Metrics

```
Pass Rate: 68.2% (2,142/3,139)
False Positives: 326 tests
Missing Errors: 280 tests
Wrong Codes: 391 tests
Close (1-2 diff): 244 tests
```

**Target for next session:** 70%+ pass rate (60+ test improvement)
