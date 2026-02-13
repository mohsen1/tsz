# Control Flow Narrowing Analysis

**Date**: 2026-02-13
**Status**: 47/92 tests passing (51.1%)

## Executive Summary

Control flow narrowing tests have a 51% pass rate, but investigation reveals the root cause is **NOT** narrowing logic itself - it's **contextual typing of object literals** when the target type is a discriminated union.

## Key Findings

### 1. Narrowing Infrastructure is Solid

The narrowing architecture is well-designed:

- **Solver** (`crates/tsz-solver/src/narrowing.rs`): Implements pure type algebra for narrowing
  - `TypeGuard` enum: AST-agnostic representation of narrowing conditions
  - `narrow_type()`: Main entry point that handles all guard types
  - Support for: typeof, instanceof, discriminants, predicates, truthiness, arrays

- **Checker** (`crates/tsz-checker/src/control_flow_narrowing.rs`): Extracts TypeGuards from AST
  - `extract_type_guard()`: Converts AST nodes to TypeGuard
  - `apply_type_predicate_narrowing()`: Applies assertion function narrowing
  - `predicate_signature_for_type()`: Extracts type predicates from function types

### 2. Assertion Functions Work Correctly

Test results:
```typescript
// ✅ WORKS: Basic assertion narrowing
declare function assertIsString(x: unknown): asserts x is string;
const val: unknown = "hello";
assertIsString(val);
val.toUpperCase(); // No error

// ✅ WORKS: Assertion discriminant narrowing (with explicit type)
const animal: Animal = { type: 'cat', canMeow: true } as const;
assertEqual(animal.type, 'cat' as const);
animal.canMeow; // No error
```

### 3. The Real Bug: Object Literal Contextual Typing

The primary failure in `controlFlowAliasedDiscriminants.ts` is at line 12:

```typescript
type UseQueryResult<T> = {
    isSuccess: false;
    data: undefined;
} | {
    isSuccess: true;
    data: T
};

function useQuery(): UseQueryResult<number> {
    return {
        isSuccess: false,  // ❌ Inferred as `boolean` instead of literal `false`
        data: undefined,
    };
}
```

**Error**: `Type '{ data: undefined; isSuccess: boolean }' is not assignable to type 'UseQueryResult<number>'`

**Root Cause**: Object literal properties are not being contextually typed against the discriminated union return type. The literal `false` should be inferred as type `false` (not widened to `boolean`) when the target type is a discriminated union.

### 4. Destructuring Narrowing Works

```typescript
// ✅ WORKS: Destructuring with narrowing
const { data, isSuccess } = result;
if (isSuccess) {
    data.toExponential(); // No error
}

// ✅ WORKS: Renamed bindings
const { data: data1, isSuccess: isSuccess1 } = result;
if (isSuccess1) {
    data1.toExponential(); // No error
}
```

## Test Failure Breakdown

### controlFlowAliasedDiscriminants.ts
- **Expected**: [TS1360, TS18048] - Errors for `let` vs `const` narrowing behavior
- **Actual**: [TS2322] - Type mismatch on return statement
- **Root Cause**: Contextual typing bug (not narrowing)

### assertionFunctionsCanNarrowByDiscriminant.ts
- **Expected**: [] (no errors)
- **Actual**: [TS2339, TS2352] - Property errors and type conversion errors
- **Root Cause**: Contextual typing bug (assertion logic is correct)

### destructuringTypeGuardFlow.ts
- **Expected**: [] (no errors)
- **Actual**: [TS2322, TS18050] - Type mismatch and possibly null/undefined errors
- **Needs Investigation**: May be unrelated narrowing issues

### assertionTypePredicates1.ts
- **Expected**: [TS1228, TS2775, TS2776, TS7027] - Various specific errors
- **Actual**: [TS2339, TS7006, TS18048, TS18050] - Different error codes
- **Needs Investigation**: Complex test with many edge cases

## Priority Fixes

### High Priority: Object Literal Contextual Typing
**Impact**: Fixes 2+ tests immediately

When an object literal is contextually typed against a discriminated union:
1. Match the literal properties against each union member
2. If a unique match is found, use that member's property types (including literal types)
3. Don't widen literal values like `false` to `boolean`

**Files to investigate**:
- `crates/tsz-checker/src/expr.rs` - Expression type inference
- `crates/tsz-checker/src/type_computation.rs` - Type computation for literals
- `crates/tsz-checker/src/type_computation_complex.rs` - Contextual typing logic

### Medium Priority: Let vs Const Narrowing
**Impact**: Fixes remaining errors in controlFlowAliasedDiscriminants

When destructuring with `let`, narrowing should NOT propagate:
```typescript
let { data, isSuccess } = result;  // Mutable
if (isSuccess) {
    data.toExponential(); // Should error (isSuccess could be reassigned)
}
```

When destructuring with `const`, narrowing SHOULD propagate (current behavior is correct).

### Low Priority: Other Edge Cases
Investigate remaining test failures after fixing contextual typing.

## Recommendations

1. **Fix object literal contextual typing first** - This will likely fix 40-50% of failing tests
2. **Verify with conformance tests** after each fix
3. **Don't break existing narrowing logic** - It's working correctly
4. **Add unit tests** for contextual typing of discriminated unions

## Code Quality Notes

- ✅ Clean architecture with good separation of concerns
- ✅ Comprehensive TypeGuard enum covering all narrowing cases
- ✅ Good use of tracing for debugging
- ✅ Well-documented with examples in code comments
- ⚠️ Need to follow "Checker never inspects type internals" rule from HOW_TO_CODE.md

## Next Steps

1. Start with task #1: Investigate object literal contextual typing
2. Find where object literals are type-checked against their contextual type
3. Add special handling for discriminated unions
4. Run tests to verify the fix
5. Commit and move to next issue
