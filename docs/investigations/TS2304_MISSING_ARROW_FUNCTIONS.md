# Investigation: TS2304 Not Emitted for Undefined Types in Arrow Function Signatures

**Date**: 2026-02-12
**Issue**: Missing TS2304 errors for undefined type names in arrow function type signatures
**Conformance Tests Affected**: `ParameterList5.ts`, `parserParameterList5.ts`

---

## Problem Description

Type references inside arrow function type signatures are not being validated. Undefined type names do not trigger TS2304 ("Cannot find name").

### Test Cases

**Case 1: Simple undefined type (WORKS)**
```typescript
let x: UndefinedType;
// ✅ Emits: TS2304: Cannot find name 'UndefinedType'
```

**Case 2: Arrow function parameter type (BROKEN)**
```typescript
let f: (x: UndefinedType) => ReturnType;
// ❌ No errors emitted - should emit TS2304 for both types
```

**Case 3: Function return type with arrow (BROKEN)**
```typescript
function foo(): (x: Foo) => Bar {
    return null as any;
}
// ❌ No errors emitted - should emit TS2304 for Foo and Bar
```

**Case 4: ParameterList5 (BROKEN)**
```typescript
function A(): (public B) => C {
}
// ❌ Emits: TS2355, TS2369
// ❌ Missing: TS2304 for B and C
```

---

## Root Cause

Type checking for arrow function type signatures appears to be incomplete. When processing arrow function types, we're not visiting/checking the type references for existence.

### Expected Behavior

All type references, regardless of location, should be checked:
1. ✅ Variable type annotations
2. ✅ Function parameter types (in implementation)
3. ✅ Function return types (simple)
4. ❌ Arrow function parameter types (in type position)
5. ❌ Arrow function return types (in type position)

---

## Investigation

### Error Code Details

**TS2304**: "Cannot find name '{0}'"
- Category: Error
- Should be emitted when: A type reference cannot be resolved

**TS2355**: "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value"
- Category: Error
- Correctly emitted in ParameterList5 test

**TS2369**: "A parameter property is only allowed in a constructor implementation"
- Category: Error
- Correctly emitted for `public B` in non-constructor context

### Conformance Test Requirements

```
Expected: [TS2304, TS2355, TS2369]
Actual:   [TS2355, TS2369]
Missing:  [TS2304]
```

The test expects TS2304 to be emitted for **both** `B` and `C` in the return type `(public B) => C`.

---

## Code Investigation Needed

### Likely Locations

1. **Type Reference Resolution**
   - Where we resolve type references to symbols
   - Likely in checker or binder

2. **Arrow Function Type Checking**
   - Where we process arrow function type annotations
   - Should visit parameter types and return type

3. **Type Annotation Traversal**
   - Visitor pattern for type nodes
   - May be skipping arrow function internals

### Search Starting Points

```bash
# Find where TS2304 is emitted
rg "2304|CANNOT_FIND_NAME" crates/tsz-checker/src/

# Find arrow function type processing
rg "FunctionType|ArrowFunction.*type" crates/tsz-checker/src/

# Find type reference checking
rg "check.*type.*reference|resolve.*type.*name" crates/tsz-checker/src/
```

---

## Reproduction

### Minimal Test Case

```typescript
// test.ts
let f: (x: UndefinedType) => AnotherUndefined;
```

**Expected Output**:
```
test.ts(1,12): error TS2304: Cannot find name 'UndefinedType'.
test.ts(1,30): error TS2304: Cannot find name 'AnotherUndefined'.
```

**Actual Output**:
```
(no errors)
```

### Run Conformance Test

```bash
./scripts/conformance.sh run --filter "ParameterList5"
```

---

## Impact

**Priority**: Medium-High
**Effort**: 4-8 hours (investigation + fix + testing)
**Tests Affected**: 2 conformance tests directly
**Broader Impact**: All arrow function type signatures may have undetected type errors

### Potential Issues

1. **Type Safety**: Undefined types in arrow function signatures not caught
2. **False Negatives**: Missing legitimate errors
3. **Test Coverage**: May affect many more tests than just ParameterList5

---

## Proposed Solution

### Step 1: Find Type Reference Checker

Locate where type references are validated (emit TS2304).

### Step 2: Identify Missing Path

Determine why arrow function type signatures don't go through type reference checking.

### Step 3: Add Type Checking

Ensure arrow function parameter types and return types are visited and checked:

```rust
fn check_arrow_function_type(&mut self, arrow_type: &ArrowFunctionType) {
    // Check parameter types
    for param in &arrow_type.parameters {
        if let Some(param_type) = &param.type_annotation {
            self.check_type_reference(param_type);  // ← Add this
        }
    }

    // Check return type
    if let Some(return_type) = &arrow_type.return_type {
        self.check_type_reference(return_type);  // ← Add this
    }
}
```

### Step 4: Test

- Verify ParameterList5 now passes
- Run full test suite for regressions
- Test various arrow function type patterns

---

## Testing Checklist

- [ ] Simple arrow function type: `(x: T) => U`
- [ ] Nested arrow types: `(f: (x: T) => U) => V`
- [ ] Optional parameters: `(x?: T) => U`
- [ ] Rest parameters: `(...args: T[]) => U`
- [ ] Generic arrow types: `<T>(x: T) => U`
- [ ] Constructor signatures: `new (x: T) => U`

---

## Related Issues

This may be part of a broader pattern where type annotations in certain contexts aren't being validated. After fixing this, audit other type positions:

- Method signatures in interfaces
- Index signatures
- Conditional types
- Mapped types
- Template literal types

---

## Status

**Status**: ✅ RESOLVED - Fix implemented and committed
**Commit**: 10c0698 "fix: emit TS2304 for undefined types in arrow function signatures"
**Branch**: claude/analyze-dry-violations-bRCVs
**Tests**: All unit tests passing (2372/2372), ParameterList5 conformance tests now pass

### Solution Implemented

Added explicit validation in `TypeNodeChecker::get_type_from_function_type` to check TYPE_REFERENCE nodes before delegating to TypeLowering:

1. Collects function type's own type parameters (e.g., `<T>` in `<T>(x: T) => T`)
2. Checks if type names are built-in types (void, number, string, etc.)
3. Checks if type names are local/global type parameters
4. Checks if type names exist in file or lib binders
5. Emits TS2304 for truly undefined types

Also added TYPE_REFERENCE routing in `state_type_environment.rs` to ensure top-level type references use CheckerState's diagnostic-emitting path.

---

## References

- Conformance tests: `TypeScript/tests/cases/compiler/ParameterList5.ts`
- Error code TS2304: "Cannot find name '{0}'"
- TypeScript behavior: All type references checked, regardless of context
