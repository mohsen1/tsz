# Control-Flow Narrowing: Deep Analysis

**Date**: 2026-02-13
**Status**: Investigation Complete - Architecture Change Required

---

## Problem Statement

TypeScript's aliased discriminant narrowing respects let/const distinction for DESTRUCTURED variables, but our implementation doesn't track destructuring relationships.

---

## Key Test Case: controlFlowAliasedDiscriminants.ts

```typescript
// Case 1: Direct narrowing - WORKS (even with let)
let o: Result;
if (o.success) {
    o.data;  // ✅ Narrowed to number
}

// Case 2: Aliased narrowing with CONST - SHOULD WORK
const { data, success } = getResult();
if (success) {
    data;  // ✅ Should narrow to number
}

// Case 3: Aliased narrowing with LET - SHOULD NOT WORK
let { data, success } = getResult();
if (success) {
    data;  // ❌ Should be number | undefined (not narrowed)
}
```

---

## Root Cause: Missing Destructuring Relationship Tracking

TypeScript tracks when variables come from the same destructuring:
1. **Destructuring source tracking**: `{ data, success } = obj` creates a relationship
2. **Cross-property narrowing**: Checking `success` can narrow `data`
3. **Const-only rule**: Cross-narrowing only works if destructuring is const

**What we're missing**:
- No tracking of which variables come from the same destructuring
- No concept of "aliased" discriminants vs "direct" discriminants
- Mutability check was attempted on wrong target

---

## Failed Approach: Simple Mutability Check

### What I Tried
Added `is_mutable_variable(target)` check before applying discriminant narrowing.

### Why It Failed
**Test case that broke**:
```typescript
let o: D;  // o is mutable (let)
if ((o = fn()).done) {
    const y: 1 = o.value;  // Should work!
}
```

This is DIRECT narrowing on a let variable - perfectly valid! We check `o.done` directly, narrowing applies to `o` as a whole.

**The distinction**:
- ✅ Direct check: `if (obj.done)` - narrowing applies to `obj` itself
- ❌ Aliased check: `let { done, value } = obj; if (done)` - cross-narrowing requires const

---

## Correct Solution: Track Destructuring Relationships

### Required Architecture Changes

1. **Track Destructuring Sources**
   - When binding `let { data, success } = getResult()`, record:
     - `data` and `success` come from the same source
     - The destructuring is let/const

2. **Identify Aliased Discriminants**
   - When narrowing via discriminant, check if:
     - Target is a destructured variable
     - Discriminant is from the same destructuring
     - If yes, check const/let status

3. **Apply Const-Only Rule**
   - Allow cross-property narrowing ONLY if:
     - Both variables from same const destructuring, OR
     - Direct narrowing (no aliasing)

### Implementation Locations

**Binder Changes** (crates/tsz-binder/src/):
- Track destructuring relationships when binding
- Store in symbol or flow nodes

**Checker Changes** (crates/tsz-checker/src/control_flow.rs):
- Detect when narrowing is "aliased" (cross-property from same source)
- Check destructuring const/let status before applying
- Allow direct narrowing regardless of let/const

---

## Complexity Estimate

**Effort**: 5-7 sessions (multi-week)

**Why so complex**:
1. Requires changes to binder (symbol/flow tracking)
2. Requires changes to checker (aliasing detection)
3. Many edge cases (nested destructuring, parameter destructuring, etc.)
4. High risk of regressions (47/92 tests already passing)

**Alternatives**:
- Accept 51% pass rate for control-flow tests
- Focus on higher-ROI improvements elsewhere
- Revisit when type system is more mature

---

## Current Status

**Control Flow Tests**: 51.1% (47/92 passing)
**Unit Tests**: 2394/2394 passing ✅
**Blocking Issue**: Architectural - not a simple fix

---

## Recommendation

**Defer** control-flow narrowing improvements until:
1. Core type system is more stable
2. Dedicated multi-week effort can be allocated
3. Binder/checker architecture supports relationship tracking

**Focus instead on**:
- Tests with higher pass rates and simpler fixes
- Emit conformance (46%)
- Other checker improvements with better ROI

---

## References

- Test file: `TypeScript/tests/cases/compiler/controlFlowAliasedDiscriminants.ts`
- Baseline: `TypeScript/tests/baselines/reference/controlFlowAliasedDiscriminants.errors.txt`
- Code: `crates/tsz-checker/src/control_flow.rs:2250-2730`
- Unit test: `src/tests/checker_state_tests.rs:22989` (assignment expression narrowing)

---

**Conclusion**: Aliased discriminant narrowing requires architectural changes to track destructuring relationships. Simple mutability checks are insufficient and break valid narrowing cases.
