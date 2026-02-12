# Investigation: Missing TS2362/TS2363/TS2365 for Arithmetic on Boxed Types

**Date**: 2026-02-12
**Issue**: Arithmetic operations on boxed types (`Number`, `String`, etc.) don't emit errors
**Conformance Tests Affected**: `arithmeticOnInvalidTypes.ts`

---

## Problem Description

TypeScript distinguishes between primitive types (`number`, `string`, `boolean`) and their boxed object equivalents (`Number`, `String`, `Boolean`). Arithmetic operations should only work on primitives, not boxed types.

### Test Case

```typescript
var x: Number;  // Boxed type (interface)
var y: Number;
var z = x + y;   // Should emit TS2365
var z2 = x - y;  // Should emit TS2362 + TS2363
```

**Expected Errors**:
- Line 3: TS2365 "Operator '+' cannot be applied to types 'Number' and 'Number'"
- Line 4: TS2362 "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type"
- Line 4: TS2363 "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type"

**Actual**: No errors emitted

---

## Root Cause

The `BinaryOpEvaluator` in `crates/tsz-solver/src/binary_ops.rs` is incorrectly allowing arithmetic operations on boxed types.

The evaluator uses `is_number_like()` to check if a type is valid for arithmetic:

```rust
pub fn is_arithmetic_operand(&self, type_id: TypeId) -> bool {
    self.is_number_like(type_id) || self.is_bigint_like(type_id)
}
```

The `is_number_like()` function uses `NumberLikeVisitor` which checks for `IntrinsicKind::Number`. However, boxed types like `Number` are represented as interface types (loaded from lib.d.ts), not intrinsic types.

### Investigation Needed

1. **How is `Number` type resolved?**
   - Is it being incorrectly mapped to the primitive `number` type?
   - Or is `is_number_like()` incorrectly returning true for interface types?

2. **Where are lib types loaded?**
   - Check how interface `Number` from lib.d.ts is handled
   - Verify it's represented as an interface type, not confused with primitive

3. **Check type visitor logic**
   - `NumberLikeVisitor` should only match primitives
   - Interface types should fail the check

---

## Error Codes

### TS2365
**Message**: "Operator '+' cannot be applied to types '{0}' and '{1}'"
**Emitted for**: Plus operator on incompatible types
**Special case**: Plus gets its own error because it can also do string concatenation

### TS2362
**Message**: "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type"
**Emitted for**: Left operand of -, *, /, %, ** operators

### TS2363
**Message**: "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type"
**Emitted for**: Right operand of -, *, /, %, ** operators

---

## Proposed Solution

### Option 1: Fix type resolution
If `Number` is being incorrectly resolved to primitive `number`, fix the type resolution to preserve the distinction.

### Option 2: Add explicit boxed type check
Add a check in `is_number_like()` to reject known boxed types:

```rust
fn is_number_like(&self, type_id: TypeId) -> bool {
    if type_id == TypeId::NUMBER || type_id == TypeId::ANY {
        return true;
    }

    // Reject boxed types explicitly
    if self.is_boxed_primitive_type(type_id) {
        return false;
    }

    let mut visitor = NumberLikeVisitor { db: self.interner };
    visitor.visit_type(self.interner, type_id)
}

fn is_boxed_primitive_type(&self, type_id: TypeId) -> bool {
    // Check if this is an interface type named "Number", "String", "Boolean", etc.
    // from lib.d.ts
    match self.interner.lookup(type_id) {
        Some(TypeKey::Interface(interface_id)) => {
            // Check interface name against known boxed types
            // "Number", "String", "Boolean", "BigInt", "Symbol"
        }
        _ => false
    }
}
```

### Option 3: Check at checker layer
Add validation in `get_type_of_binary_expression` before calling the evaluator:

```rust
// Before line 1110 (evaluator.evaluate)
if is_arithmetic_op && (is_boxed_type(left_type) || is_boxed_type(right_type)) {
    // Emit TS2362/TS2363/TS2365 directly
    self.emit_boxed_type_arithmetic_error(node_idx, left_idx, right_idx, left_type, right_type, op_str);
    type_stack.push(TypeId::UNKNOWN);
    continue;
}
```

---

## Testing Checklist

- [ ] `Number` + `Number` emits TS2365
- [ ] `Number` - `Number` emits TS2362 + TS2363
- [ ] `Number` * `Number` emits TS2362 + TS2363
- [ ] `Number` / `Number` emits TS2362 + TS2363
- [ ] `String` + `String` should work (string concatenation)
- [ ] `String` - `String` emits TS2362 + TS2363
- [ ] `Boolean` in arithmetic emits errors
- [ ] Mixed: `Number` + `number` emits error
- [ ] Enum types still work with arithmetic

---

## Files to Investigate

- `crates/tsz-solver/src/binary_ops.rs` - BinaryOpEvaluator
- `crates/tsz-solver/src/lower.rs` - TypeLowering (type name → TypeId)
- `crates/tsz-checker/src/type_computation.rs` - get_type_of_binary_expression
- `crates/tsz-checker/src/state_type_resolution.rs` - Type reference resolution
- `TypeScript/src/lib/es5.d.ts` - Interface definitions for Number, String, etc.

---

## Status

**Status**: Investigation started, root cause analysis in progress
**Priority**: Medium - affects type safety but less critical than TS2304 issues
**Complexity**: Medium - need to understand type resolution and lib loading
**Estimated Effort**: 4-6 hours

---

## References

- Conformance test: `TypeScript/tests/cases/compiler/arithmeticOnInvalidTypes.ts`
- Baseline: `TypeScript/tests/baselines/reference/arithmeticOnInvalidTypes.errors.txt`
- TypeScript behavior: Boxed types are not valid for arithmetic operations
