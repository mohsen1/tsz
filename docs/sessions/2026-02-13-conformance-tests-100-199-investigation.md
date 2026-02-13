# Conformance Tests 100-199 Investigation

**Date**: 2026-02-13
**Goal**: Investigate and improve pass rate for conformance tests 100-199 (second batch of 100 tests)

## Current Status

### Pass Rate (Stable Segments)
- **Tests 100-150**: 48/50 passed (96%)
- **Tests 150-200**: 43/50 passed (86%)
- **Overall 100-200**: ~90% pass rate (reported at session start)

### Test Execution Stability
When running all 100 tests at once, results show variability:
- Some runs: 90/100 (90%)
- Other runs: 86/100 with 5 timeouts
- Other runs: 51/100 with 47 timeouts

**Issue**: Running full batch of 100 tests shows instability. Smaller batches (10-50 tests) are stable.

**Hypothesis**: Possible parallelization issues, resource contention, or specific test interactions causing timeouts.

## Failure Analysis

### Top 10 Failing Tests (from initial run)

1. **ambiguousGenericAssertion1.ts** (diff=2) ← CLOSE
   - Expected: [TS1005, TS1109, TS2304]
   - Actual: [TS1005, TS1109, TS1434]
   - Category: Wrong codes (parser ambiguity issue)

2. **amdDeclarationEmitNoExtraDeclare.ts**
   - Expected: []
   - Actual: [TS2322, TS2345]
   - Category: False positive (mixin pattern)

3. **amdModuleConstEnumUsage.ts**
   - Expected: []
   - Actual: [TS2339]
   - Category: False positive (const enum with baseUrl)

4. **anonClassDeclarationEmitIsAnon.ts**
   - Expected: []
   - Actual: [TS2345]
   - Category: False positive (anonymous class in mixin)

5. **amdLikeInputDeclarationEmit.ts**
   - Expected: []
   - Actual: [TS2339]
   - Category: False positive (JS module pattern)

6. **argumentsObjectIterator02_ES5.ts**
   - Expected: [TS2585]
   - Actual: [TS2495, TS2551]
   - Category: Wrong codes (arguments + Symbol.iterator in ES5)

7. **argumentsObjectIterator02_ES6.ts**
   - Expected: []
   - Actual: [TS2488]
   - Category: False positive (arguments + Symbol.iterator in ES6)

8. **argumentsReferenceInConstructor4_Js.ts** (diff=1) ← CLOSE
   - Expected: [TS1210]
   - Actual: [TS1210, TS2339]
   - Category: Extra error (JS + arguments getter)

9. **argumentsReferenceInConstructor3_Js.ts**
   - Expected: []
   - Actual: [TS2340]
   - Category: False positive (JS + super.arguments)

10. **argumentsReferenceInFunction1_Js.ts**
    - Expected: [TS2345, TS7006]
    - Actual: []
    - Category: All missing (JS strict mode implicit any)

### Error Code Patterns

**False Positives (we emit when we shouldn't):**
- **TS2339** (Property doesn't exist): 3 occurrences
- **TS2345** (Argument type mismatch): 2 occurrences
- **TS2322** (Type not assignable): 1 occurrence
- **TS2488** (Missing iterator): 1 occurrence
- **TS2340** (Only public/protected accessible via super): 1 occurrence

**Missing Implementations:**
- **TS2304** (Cannot find name): Not implemented
- **TS2585** (Iterator only in ES6+): Not implemented
- **TS7006** (Implicit any): Missing in some contexts

## Key Issues Discovered

### 1. Symbol.iterator Property Access

**Problem**: Accessing `Symbol.iterator` produces TS2339 "Property 'iterator' does not exist"

**Test Case**:
```typescript
function test() {
    let blah = arguments[Symbol.iterator];
    for (let arg of blah()) {
        console.log(arg);
    }
}
```

**Error**:
```
tmp/symbol-property.ts(2,14): error TS2339: Property 'iterator' does not exist on type 'Symbol'.
```

**Root Cause**: The `Symbol` global type is missing the `iterator` property in lib definitions. The Symbol interface shows many properties (hasInstance, isConcatSpreadable, etc.) but `iterator` is missing.

**Impact**: Affects multiple tests involving Symbol.iterator access.

**Fix Needed**: Add `iterator` property to Symbol interface in lib files.

### 2. Const Enum with Module Resolution

**Test**: amdModuleConstEnumUsage.ts
**Issue**: TS2339 when accessing const enum member through module import
**Config**: AMD module, preserveConstEnums: true, baseUrl: /proj

**Code**:
```typescript
// @filename: /proj/defs/cc.ts
export const enum CharCode { A, B }

// @filename: /proj/component/file.ts
import { CharCode } from 'defs/cc';
CharCode.A // ← TS2339
```

**Root Cause**: Likely module resolution or const enum handling with AMD + baseUrl.

### 3. JavaScript + Arguments Object

Multiple failures involve JavaScript files with the `arguments` object:
- argumentsReferenceInConstructor3_Js.ts (TS2340)
- argumentsReferenceInConstructor4_Js.ts (extra TS2339)
- argumentsReferenceInFunction1_Js.ts (missing TS7006, TS2345)

Pattern: Issues with `arguments` variable shadowing, getters returning objects with `arguments` property, or implicit any in JS files.

### 4. Mixin Patterns

Tests with class mixins are producing false TS2345 errors:
- amdDeclarationEmitNoExtraDeclare.ts
- anonClassDeclarationEmitIsAnon.ts

Pattern: `class X extends Configurable(Base)` where Configurable returns a class expression.

## Recommendations

### High-Priority Fixes

1. **Fix Symbol.iterator** (affects 2+ tests)
   - Add missing `iterator` property to Symbol interface
   - Location: lib file loading/definitions

2. **Const enum + module resolution** (affects 1+ tests)
   - Investigate why const enum members aren't resolved through imports
   - Check AMD module + baseUrl interaction

3. **Investigate test stability** (critical for CI/CD)
   - Determine why full 100-test runs show timeouts
   - Check for resource leaks or infinite loops
   - Consider reducing worker count or adding timeout handling

### Medium-Priority Fixes

4. **JavaScript strict mode checking** (affects 1 test)
   - Implement missing implicit any checks in JS files
   - Add TS7006 for parameters without type annotations

5. **Mixin type inference** (affects 2 tests)
   - Improve type inference for class expressions returned from functions
   - Handle `extends` with call expressions better

### Investigation Needed

- Why do timeouts occur only when running full batch?
- Are there specific tests that cause cascading slowdowns?
- Is there a memory leak or resource exhaustion issue?

## Next Steps

1. ✅ Document findings (this file)
2. ⏭️ Fix Symbol.iterator property access
3. ⏭️ Investigate test execution stability
4. ⏭️ Fix const enum module resolution
5. ⏭️ Run full test suite validation
6. ⏭️ Commit improvements

## Metrics

- **Starting Point**: 90/100 (90%) - reported baseline
- **Stable Segments**: 96% (tests 100-150), 86% (tests 150-200)
- **Target**: 95%+ pass rate with stable execution
