# Control Flow Narrowing Investigation

**Date**: 2026-02-13
**Status**: Investigation Complete - Fix Required
**Current Pass Rate**: 47/92 (51.1%) for control flow tests

## Problem Summary

Control flow narrowing in tsz does not match tsc behavior in several critical areas:
1. **Aliased discriminant narrowing** - Destructured discriminants don't narrow related properties
2. **Assertion function narrowing** - `asserts x is T` functions don't narrow properly
3. **Destructuring-aware flow** - Destructured variables not tracked through CFA
4. **Various CFA edge cases** - let vs const, nested destructuring, etc.

## Test Results

### Overall Control Flow Tests
- **Pass Rate**: 47/92 (51.1%)
- **Major Error Mismatches**:
  - TS2339 (extra): 17 instances
  - TS2322 (extra): 13 instances
  - TS18048 (missing): 5 instances
  - TS2454 (missing): 5 instances

### Key Failing Tests

#### 1. controlFlowAliasedDiscriminants.ts
**TSC Expected**: 6 errors (TS18048, TS1360)
**TSZ Actual**: 1 error (TS2322 - false positive)

**Problem**: Aliased discriminants don't narrow related destructured variables

Example that fails:
```typescript
const { data: data1, isSuccess: isSuccess1 } = useQuery();
const areSuccess = isSuccess1 && isSuccess2 && isSuccess3;
if (areSuccess) {
    data1.toExponential();  // TSZ: doesn't narrow data1 based on areSuccess alias
}
```

#### 2. assertionFunctionsCanNarrowByDiscriminant.ts
**TSC Expected**: 0 errors
**TSZ Actual**: 4 errors (TS2352, TS2339)

**Problem**: Assertion functions don't narrow by discriminant property

Example that fails:
```typescript
function assertEqual<T>(value: any, type: T): asserts value is T;
const animal = { type: 'cat', canMeow: true } as Animal;
assertEqual(animal.type, 'cat' as const);
animal.canMeow; // TSZ: Property 'canMeow' does not exist - didn't narrow animal
```

## Root Causes

### 1. Missing Alias Tracking
When destructuring creates multiple variables from the same source:
```typescript
const { discriminant, data } = obj;
```
TSZ doesn't track that these variables are related. When `discriminant` is checked, it should narrow `data` as well.

**Location**: `crates/tsz-checker/src/control_flow_narrowing.rs`

### 2. Assertion Function Narrowing Not Implemented
Functions with `asserts x is T` signature should:
- Narrow the asserted expression in subsequent code
- Support discriminant-based narrowing

**Location**: Check for "asserts" handling in checker

### 3. Let vs Const Destructuring
TSZ doesn't differentiate between `let` and `const` destructured variables for CFA purposes.
TypeScript only narrows const-destructured variables in certain contexts.

## Architecture Review

**Existing Code**:
- `crates/tsz-checker/src/control_flow_narrowing.rs` - Core narrowing logic
- `crates/tsz-checker/src/control_flow.rs` - Flow analysis framework
- `crates/tsz-solver/src/narrowing.rs` - Type narrowing operations

**Key Functions** (from control_flow_narrowing.rs):
- `assignment_affects_reference()` - Tracks assignments through references
- `narrow_type_by_predicate()` - Type guard narrowing
- `narrow_type_by_discriminant()` - Discriminant property narrowing
- `narrow_type_by_instanceof()` - Instanceof narrowing

## Implementation Strategy

### Phase 1: Aliased Discriminant Narrowing (High Impact)
1. Track destructured variables from same source
2. When narrowing one destructured variable, propagate to related variables
3. Handle both object and array destructuring
4. Distinguish let vs const for proper narrowing rules

**Estimated Effort**: 4-6 hours
**Impact**: ~20 tests (major feature)

### Phase 2: Assertion Function Narrowing (Medium Impact)
1. Detect assertion function signatures (`asserts x is T`)
2. Implement post-call narrowing for assertion functions
3. Support discriminant-based assertion narrowing
4. Handle optional chaining in assertions

**Estimated Effort**: 3-5 hours
**Impact**: ~5-10 tests

### Phase 3: CFA Edge Cases (Lower Impact)
1. Fix truthiness narrowing for optional properties
2. Handle narrowing in complex switch/case scenarios
3. Improve narrowing across closures

**Estimated Effort**: 2-4 hours per category
**Impact**: ~10-15 tests total

## Next Steps

1. **Start with Phase 1** (aliased discriminants) as it has highest impact
2. Add tracing to understand current CFA behavior:
   ```bash
   TSZ_LOG="tsz_checker::control_flow=trace" TSZ_LOG_FORMAT=tree \
     cargo run -p tsz-cli --bin tsz -- tmp/test.ts 2>&1 | head -200
   ```
3. Write failing unit tests for each category before implementing fixes
4. Verify with existing control flow tests that pass to avoid regressions

## Files to Modify

Primary:
- `crates/tsz-checker/src/control_flow_narrowing.rs` - Add alias tracking
- `crates/tsz-checker/src/control_flow.rs` - Flow node tracking
- `crates/tsz-checker/src/state_checking.rs` - Assertion function detection

Secondary:
- `crates/tsz-solver/src/narrowing.rs` - New narrowing operations if needed
- Test files to add coverage

## Complexity Assessment

This is **not a quick fix**. Control flow analysis is one of TypeScript's most complex features:
- Requires tracking variable relationships across scope
- Must handle destructuring, aliasing, and re-assignments
- Needs to integrate with existing narrowing infrastructure
- Edge cases are numerous (let vs const, nested destructuring, etc.)

**Recommendation**: Tackle in multiple focused sessions, one phase at a time.

## References

- TypeScript CFA: https://github.com/microsoft/TypeScript/wiki/FAQ#narrowing
- Relevant test files:
  - `TypeScript/tests/cases/compiler/controlFlowAliasedDiscriminants.ts`
  - `TypeScript/tests/cases/compiler/assertionFunctionsCanNarrowByDiscriminant.ts`
  - `TypeScript/tests/cases/compiler/destructuringTypeGuardFlow.ts`
  - `TypeScript/tests/cases/conformance/controlFlow/*.ts`
