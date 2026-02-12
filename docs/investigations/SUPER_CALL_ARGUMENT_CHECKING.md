# Investigation: Missing Argument Count Validation for super() Calls

**Date**: 2026-02-12
**Issue**: super() calls don't validate argument count against base class constructor
**Conformance Tests Affected**: `baseCheck.ts`
**Error Codes Missing**: TS2554, TS2345

---

## Problem Description

When calling `super()` in a derived class constructor, TSZ is not validating that the correct number of arguments is provided, nor that the argument types match the base class constructor signature.

### Test Cases

```typescript
class C { constructor(x: number, y: number) { } }

// Should emit TS2554: Expected 2 arguments, but got 1
class D extends C {
  constructor(z: number) {
    super(z);  // Missing second argument!
  }
}

// Should emit TS2345: Argument of type 'string' is not assignable to parameter of type 'number'
class F extends C {
  constructor(z: number) {
    super("hello", z);  // Wrong type for first argument!
  }
}
```

**Expected**: TS2554 and TS2345 errors
**Actual**: No errors emitted (except TS17009 if `this` is used)

---

## Root Cause Analysis

### How super() Calls Are Typed

In `crates/tsz-checker/src/type_computation.rs:1574`, `get_type_of_super_keyword()`:

1. Detects if this is a `super()` call (parent is CallExpression)
2. Returns the base class constructor type via `get_class_constructor_type()`

The constructor type SHOULD be callable with the base class constructor signatures.

### How Calls Are Evaluated

In `crates/tsz-checker/src/type_computation_complex.rs:1189-1196`, `get_type_of_call_expression()`:

1. Calls `CallEvaluator::resolve_call(callee_type, arg_types)`
2. Handles the `CallResult` in `handle_call_result()`

### The Bug

In `handle_call_result()` at line 1236-1239:

```rust
CallResult::NotCallable { .. } => {
    if is_super_call {
        return TypeId::VOID;  // ← BUG: Returns without validation!
    }
    // ... other error handling
}
```

When the result is `NotCallable` for a super() call, it immediately returns `VOID` without checking arguments.

**Why is it NotCallable?**
- The base class constructor type might not be properly recognized as callable
- OR the constructor type doesn't have call signatures extracted properly
- OR there's a special case where super() needs construct signatures not call signatures

---

## Investigation Needed

1. **What does `get_class_constructor_type()` return?**
   - Check if it returns a proper callable type
   - Verify it has constructor signatures

2. **Why does CallEvaluator return NotCallable?**
   - Does it check construct signatures or call signatures?
   - Should super() use construct signatures?

3. **Is there existing handling for constructor calls?**
   - `new C()` calls should use construct signatures
   - Does `super()` need the same logic?

---

## Proposed Solutions

### Option 1: Fix Constructor Type (Preferred)

Ensure `get_class_constructor_type()` returns a type that CallEvaluator recognizes as callable with the proper signatures.

### Option 2: Special Case super() Calls

Before calling `CallEvaluator::resolve_call()`, detect super() calls and manually validate arguments:

```rust
if is_super_call {
    // Get base class constructor signatures
    // Manually check argument count and types
    // Emit TS2554/TS2345 as needed
    // Return VOID
}
```

### Option 3: Fix in handle_call_result

When `NotCallable` result is received for super() call, still try to validate arguments:

```rust
CallResult::NotCallable { .. } => {
    if is_super_call {
        // Try to get constructor signatures from callee_type
        // Validate arguments if possible
        // Emit errors if validation fails
        return TypeId::VOID;
    }
    // ... other handling
}
```

---

## Related Code Locations

- `crates/tsz-checker/src/type_computation.rs:1574` - get_type_of_super_keyword()
- `crates/tsz-checker/src/type_computation_complex.rs:878-1208` - get_type_of_call_expression()
- `crates/tsz-checker/src/type_computation_complex.rs:1212-1287` - handle_call_result()
- `crates/tsz-checker/src/super_checker.rs` - Super expression validation
- `crates/tsz-solver/src/operations.rs` - CallEvaluator and CallResult

---

## Testing Checklist

- [ ] super() with too few arguments emits TS2554
- [ ] super() with too many arguments emits TS2554
- [ ] super() with wrong argument types emits TS2345
- [ ] super() with correct arguments works
- [ ] super(this.x) still emits TS17009 (accessing this before super)
- [ ] super() in class with no base class still emits error
- [ ] Regular constructor calls (new C()) still work

---

## Status

**Status**: Root cause identified, needs implementation
**Priority**: Medium - affects correctness but less critical than missing errors
**Complexity**: Medium - needs understanding of constructor vs call signatures
**Estimated Effort**: 2-4 hours

---

## Next Steps

1. Add debug logging to see what type `get_class_constructor_type()` returns
2. Check if CallEvaluator properly handles constructor types
3. Implement fix (likely Option 1 or Option 2)
4. Add unit tests for super() argument validation
5. Verify baseCheck conformance test passes
