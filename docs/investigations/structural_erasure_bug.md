# Structural Erasure Bug Investigation

**Date:** 2026-02-03
**Investigator:** Claude Sonnet
**Status:** Ongoing - Architectural issue requiring deeper work

## Problem Description

The "Structural Erasure Bug" causes generic type applications (like `Array<T>`) to lose their nominal identity and be converted to structural types, breaking proper type checking.

### Failing Test
```typescript
var autoToken: number[] = new Array<number[]>(1);
```

**Expected:** TS2322 error - `Type 'number[][]' is not assignable to type 'number[]'`
**Actual:** No error emitted (zang incorrectly accepts this)

## Root Cause Analysis

### Issue Location 1: `src/solver/lower.rs` (Lines 2211-2226)

**BEFORE (Special Case Handling):**
```rust
"Array" | "ReadonlyArray" => {
    let elem_type = ... // lower type argument
    let array_type = self.interner.array(elem_type);
    return array_type;  // BUG: Creates structural type directly
}
```

**Problem:**
- When `Array<T>` is referenced in type position, it creates a structural `TypeKey::Array(T)` directly
- This erases the nominal identity of `Array`
- Assignability checking then compares structural types instead of nominal types

**AFTER (Fix Applied):**
- Removed the special case entirely
- Code now falls through to standard type reference lowering
- Should create `Application(Array, [T])` to preserve nominal identity

### Issue Location 2: New Expression Type Computation

**File:** `src/checker/type_computation_complex.rs`
**Function:** `get_type_of_new_expression`

The `new Array<T>()` expression path:
1. Looks up the `Array` symbol
2. Applies type arguments to the construct signature
3. Returns the instance type from the construct signature

**Potential Issues:**
- The instance type returned might be a structural `Array<T>` instead of `Application(Array, [T])`
- Or the Application type is being resolved/evaluated to a structural type later in the pipeline

## Investigation Steps Taken

### 1. Confirmed the Bug
```bash
./scripts/conformance.sh run --filter "parserObjectCreation1"
# Expected: [TS2322]
# Actual: [] (missing error)
```

### 2. Removed Special Case in lower.rs
- Deleted lines 2211-2226 which created structural array types
- Allowed standard type reference lowering to create Application types

### 3. Test Still Fails
- After fix, the test still doesn't emit TS2322
- Indicates the problem is deeper than just type lowering

### 4. Identified Multiple Components Involved

The Application type pipeline involves:
1. **lower.rs** - Creates `Application(Array, [T])` types from AST
2. **type_computation_complex.rs** - Handles `new` expressions
3. **evaluate_application_type** - Resolves Application types
4. **compat.rs** / **subtype.rs** - Assignability checking

## Architectural Complexity

This is a **P0 architectural issue** that spans multiple files:

### Type Creation Pipeline
```
AST → lower.rs → Application(Array, [T]) → type_computation_complex.rs → evaluate_application_type → ?????
```

**Question:** Does `evaluate_application_type` preserve the Application type, or does it resolve it to a structural type?

### Assignability Pipeline
```
Source: Application(Array, [number[]])
Target: Application(Array, [number])
```

**Question:** Does the compat checker properly compare two Application types of the same generic type with different arguments?

## Recommended Next Steps

### Option 1: Comprehensive Fix (High Effort, High Impact)
1. Ensure `evaluate_application_type` preserves Application types
2. Update compat/subtype checkers to properly compare Application types
3. Add fast-path rules: `Application(A, [T1])` != `Application(A, [T2])` when T1 != T2
4. Test with full conformance suite

### Option 2: Document and Defer (Low Effort)
1. Create GitHub issue with detailed analysis
2. Add TODO comments in relevant files
3. Document expected behavior in design docs
4. Move to other improvements that can be completed faster

### Option 3: Hybrid Approach (Medium Effort)
1. Add specific handling for `Array` Application types in compat checker
2. This fixes the immediate test case
3. Document that this is a workaround for the larger architectural issue
4. Create tech debt ticket for comprehensive fix

## Related Code Sections

### Files Involved
- `src/solver/lower.rs` - Type lowering from AST
- `src/solver/instantiate.rs` - Type instantiation and substitution
- `src/solver/evaluate.rs` - Application type evaluation
- `src/solver/compat.rs` - Assignability checking
- `src/checker/type_computation_complex.rs` - New expression handling

### Key Functions
- `lower_type_reference` - Creates types from type references
- `get_type_of_new_expression` - Determines result type of `new` expressions
- `apply_type_arguments_to_constructor_type` - Instantiates generic constructors
- `evaluate_application_type` - Resolves Application types
- `is_assignable_to` - Main assignability checker

## Test Cases

### Passing Test
```typescript
type MyPartial<T> = { [K in keyof T]?: T[K] };
interface Cfg { host: string; port: number }
let a: MyPartial<Cfg> = {};           // ✓ PASS
let b: MyPartial<Cfg> = { host: "x" }; // ✓ PASS
```

### Failing Test
```typescript
var x: number[] = new Array<number[]>(1); // Should emit TS2322
```

## Notes

- This investigation revealed that mapped types now work correctly (28/28 TS2322 tests passing)
- The Array structural erasure is a separate, deeper architectural issue
- Fixing it properly requires careful coordination across multiple solver components
- Risk of breaking other tests if not done comprehensively
