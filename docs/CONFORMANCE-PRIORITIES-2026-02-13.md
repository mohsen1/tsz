# Conformance Test Priorities - 2026-02-13

## Current Status

**Overall Progress:**
- Tests 0-100: **97%** (96/99 passing)
- Tests 100-199: **96%** (improved from 95% after discriminant fix!)
- Tests 0-500: **86%** (429/499 passing)

**Recent Fix:**
- Fixed discriminant narrowing for let-bound variables in switch statements
- Impact: Improved tests 100-199 from 95% to 96%

## High-Impact Priorities (from Mission)

### 1. Conditional Type Evaluation ⚠️ BLOCKS ~200 TESTS
**Issue:** Conditional types are not being evaluated correctly when used with type parameters.

**Example:**
```typescript
type Test<T> = T extends { name: any } ? {} : { name: string };
function build<SO_FAR>(soFar: SO_FAR): Test<SO_FAR> {
    return "name" in soFar ? {} : { name: "test" };
}
// Error: Type 'SO_FAR' is not assignable to type 'object'
```

**Root Cause:** Type narrowing with `in` operator doesn't properly evaluate conditional types with the narrowed type parameter.

**Files to investigate:**
- `crates/tsz-solver/src/evaluate_rules/conditional.rs`
- `crates/tsz-solver/src/instantiate.ts`
- Integration between narrowing and conditional type evaluation

**Impact:** ~23 false positives are related to conditional types and complex type evaluation.

### 2. Contextual Typing for Function Expressions
**Issue:** TS7006 false positives for implicitly typed parameters that should be contextually typed.

**Example:**
```typescript
// @strict: false
var f: {
    (x: string): string;
    (x: number): string
};
f = (a) => { return a.asdf }  // Should NOT error in non-strict mode
```

**Root Cause:** Not properly applying contextual type from assignment target to lambda parameters, especially with overloads and in non-strict mode.

**Files to investigate:**
- `crates/tsz-solver/src/contextual.rs`
- `crates/tsz-checker/src/call_checker.rs`

**Impact:** Multiple tests with function expression parameters

### 3. Generic Inference Edge Cases
**Issue:** Generic function inference fails for complex cases like multi-signature overloads and higher-order functions.

**Test:** `TypeScript/tests/cases/compiler/genericFunctionInference1.ts`

**Files to investigate:**
- `crates/tsz-solver/src/infer.rs`
- `crates/tsz-checker/src/call_checker.rs`

### 4. Array Method Return Types
**Issue:** Array methods like `.sort()` are returning the full interface structure instead of the array type.

**Example:**
```typescript
this.options = this.options.sort(fn);
// Error: Type '{... entire array interface ...}' is not assignable to type 'IOptions[]'
```

**Root Cause:** Type resolution for array methods not properly simplifying to array type.

**Files to investigate:**
- `crates/tsz-solver/src/type_queries.rs`
- Property access resolution for array types

### 5. Object Literal Property-Level Errors
**Issue:** When checking object literals against types, we emit errors at the argument level instead of property level.

**Example:**
```typescript
foo({ id: 1234, name: false });
// We emit: TS2345 at argument level
// TSC emits: TS2322 at 'name' property level
```

**Root Cause:** Error reporting granularity in object literal checking.

**Files to investigate:**
- `crates/tsz-checker/src/call_checker.rs`
- Object literal checking and error emission

## Error Code Impact Analysis

### Not Implemented (would unlock tests immediately):
- **TS2693** → 3 tests (statement not accessible)
- **TS2741** → 2 tests (property missing in type)
- **TS2461** → 2 tests (type is not an array type)
- **TS2488** → 2 tests (must have Symbol.iterator)
- **TS2705** → 2 tests (async function must return Promise)

### Partially Implemented (needs broader coverage):
- **TS2322** → missing in 11 tests (type not assignable)
- **TS2304** → missing in 5 tests (cannot find name)
- **TS2339** → missing in 2 tests (property does not exist)

### Falsely Emitted (reducing these = quick wins):
- **TS2411** → 1 test (property not assignable to index type)
- **TS7010** → 1 test (function implicitly has any return)
- **TS2345** → extra in 9 tests (argument type not assignable)
- **TS2769** → extra in 6 tests (no overload matches)

## Recommended Next Steps

1. **Quick Win**: Fix false positive in `aliasUsageInIndexerOfClass.ts` (TS2411)
   - Only 1 test affected
   - Should be localized fix

2. **Medium Impact**: Fix array method return type resolution
   - Affects multiple array-related tests
   - Localized to type resolution

3. **High Impact**: Improve conditional type evaluation
   - Most impactful but most complex
   - Requires deep investigation of conditional type evaluation logic
   - Could unlock many tests

4. **High Impact**: Improve contextual typing
   - Multiple false positives
   - Need to handle strict/non-strict mode differences
   - Need to handle overload signatures

## Test Examples for Investigation

**Conditional Types:**
- `TypeScript/tests/cases/compiler/conditionalTypeDoesntSpinForever.ts`
- `tmp/test-conditional-simple.ts` (minimal repro)

**Contextual Typing:**
- `TypeScript/tests/cases/compiler/contextualTypingOfLambdaWithMultipleSignatures2.ts`

**Generic Inference:**
- `TypeScript/tests/cases/compiler/genericFunctionInference1.ts`

**Array Methods:**
- `TypeScript/tests/cases/compiler/arrayconcat.ts`
