# Known Issues - Conformance Tests

## Critical Issues

### Stack Overflow in keyofAndIndexedAccess2.ts

**File**: `TypeScript/tests/cases/conformance/types/keyof/keyofAndIndexedAccess2.ts`
**Error**: Stack overflow during type checking
**Status**: 🔴 Blocks test execution

**Symptom**:
```
thread 'main' (4003) has overflowed its stack
fatal runtime error: stack overflow, aborting
```

**Root Cause**: Infinite recursion in type resolution for complex mapped types and indexed access types.

**Problematic Patterns**:
```typescript
// Mapped type with generic key
function f3<K extends string>(a: { [P in K]: number }, ...) {
    // Type resolution enters infinite loop
}

// Complex indexed access
function f4<K extends string>(a: { [key: string]: number }[K], b: number) {
    // Recursive type expansion
}
```

**Analysis**:
- Likely occurs in type evaluation or mapped type resolution
- May involve circular type dependencies
- Could be in indexed access evaluation (`evaluate_index_access`)
- Needs recursion depth limiting or cycle detection

**Recommended Fix**:
1. Add recursion depth counter to type evaluation
2. Implement cycle detection in mapped type resolution
3. Add early bailout for complex generic types
4. Consider memoization of evaluated types

**Priority**: Medium (only affects 1 test, but blocks it completely)

**Files to Investigate**:
- `crates/tsz-solver/src/evaluate.rs`
- `crates/tsz-solver/src/evaluate_rules/index_access.rs`
- `crates/tsz-solver/src/evaluate_rules/mapped.rs`

---

## High Priority Issues

### TS2345 - Argument Type False Positives (56 extra)

**Pattern**: Generic function calls with union arguments
**Status**: 🟡 Needs investigation
**Priority**: High

Similar to the TS2322 conditional expression issue. Type argument inference may be checking individual types instead of unions.

**Example Pattern**:
```typescript
function foo<T>(x: T) { }
foo(cond ? 1 : "hello");  // May incorrectly report TS2345
```

**Recommended Approach**:
- Check argument type computation in generic calls
- Ensure union types are properly formed before checking
- May need fixes in type parameter instantiation

---

### TS2339 - Property Access False Positives (85 extra overall, 10 in slice 2)

**Pattern**: Property access on union types or after narrowing
**Status**: 🟡 Improved but not complete
**Priority**: High

We've reduced this from 85 to 10 in some test slices, but it's still present overall.

**Possible Causes**:
1. Narrowing not applied correctly
2. Object type resolution issues
3. Intersection type property resolution

**Recommended Approach**:
- Review property access on narrowed types
- Check union type property resolution
- Verify intersection type handling

---

### TS1005 - Syntax Errors (51 extra)

**Pattern**: Parser reporting extra syntax errors
**Status**: 🟡 Parser issues
**Priority**: Medium

Likely parser edge cases or AST construction issues.

**Recommended Approach**:
- Review failing tests to find patterns
- May need parser fixes for specific constructs
- Could be related to error recovery

---

## Medium Priority Issues

### TS2304 - Cannot Find Name (58 missing, 15 extra)

**Pattern**: Symbol resolution issues
**Status**: 🟡 Mixed (some missing, some extra)
**Priority**: Medium

Both missing and extra errors suggest scope/binding issues.

**Recommended Approach**:
- Check symbol binding in different scopes
- Verify namespace and module resolution
- Review import/export handling

---

### TS2315 - Type Not Generic (24 extra)

**Pattern**: Type aliases or utility types incorrectly flagged
**Status**: 🟡 Type resolution issue
**Priority**: Medium

**Example**:
```typescript
type Partial<T> = { ... };  // May be flagged as non-generic
```

**Recommended Approach**:
- Check type alias instantiation
- Verify generic type parameter handling
- Review built-in utility type resolution

---

### TS2769 - No Overload Matches (23 extra)

**Pattern**: Function overload resolution
**Status**: 🟡 Signature matching issue
**Priority**: Medium

**Recommended Approach**:
- Review overload resolution order
- Check signature compatibility checking
- Verify rest parameter handling

---

## Fixed Issues ✅

### TS2322 - Type Not Assignable (Fixed: 85 → 23)

**Status**: ✅ Major improvement (-73%)
**Fix**: Commit `6283f81` - Conditional expression type checking
**Details**: Removed premature assignability checks in ternary expressions

### TS18050 - Value Cannot Be Used Here (Fixed)

**Status**: ✅ Eliminated for indexed access types
**Fix**: Commit `2ea3baa` - Typeof narrowing for indexed access types
**Details**: Create intersection `T[K] & Function` instead of narrowing to `never`

---

## Low Priority Issues

### JSX-related TS2874 Errors

**Pattern**: JSX factory not in scope
**Status**: 🟢 Low priority (JSX-specific)
**Priority**: Low

Most TS2874 errors are related to JSX, not general TypeScript.

**Recommended Approach**:
- Focus on non-JSX issues first
- JSX support can be improved later

---

## Testing Notes

### Test Statistics
- **Total tests**: 12,639
- **Run**: 2,117
- **Skipped**: 10,527
- **Passing**: 1,253 (59.2%)
- **Crashed**: 1 (keyofAndIndexedAccess2.ts)

### Test Slice Strategy
For focused work, use test slices:
```bash
# Slice 2: tests 3,101-6,201
./.target/dist-fast/tsz-conformance --offset 3101 --max 3101 \
  --cache-file tsc-cache-full.json --tsz-binary ./.target/release/tsz
```

### Error Code Filtering
```bash
# Focus on specific error code
./.target/dist-fast/tsz-conformance --error-code 2345 --max 100 \
  --cache-file tsc-cache-full.json --tsz-binary ./.target/release/tsz
```

---

## Debug Workflow

1. **Identify Pattern**: Find failing tests with same error
2. **Create Minimal Test**: Reproduce with minimal TypeScript
3. **Compare with TSC**: Check expected behavior
4. **Trace Execution**: Use `TSZ_LOG=debug` if needed
5. **Locate Bug**: Find responsible function
6. **Write Unit Test**: Add failing test first
7. **Implement Fix**: Make minimal, targeted changes
8. **Verify**: Run all tests, check for regressions

---

**Last Updated**: 2026-02-09
**Status**: Active - ready for next improvements
