# TS2322 Union Type Assignability Fix

**Date**: 2026-01-27
**Commit**: Fix applied to `src/solver/subtype_rules/unions.rs`
**Impact**: Reduces false positive TS2322 errors for literal-to-union assignability

## Problem

When checking if a literal type (e.g., `"hello"`, `42`) is assignable to a union type containing its primitive (e.g., `string | number`), TSZ was missing an important optimization that caused false positives.

### Previous Behavior (Commit 62ea1c8b7)

The code had an optimization that checked:
1. Fast path: Exact primitive match (e.g., `string` member)
2. Subtype check: Literal-to-literal unions only

This missed the important case of:
3. **Literal-to-intrinsic**: `"hello" <: string` when checking against unions like `string | number`

### Root Cause

In commit 62ea1c8b7, the optimization was changed from checking `TypeKey::Intrinsic(_)` to `TypeKey::Literal(_)`. While the literal-to-literal check was useful, removing the intrinsic check broke literal-to-primitive widening in union contexts.

## Solution

Restored the intrinsic type checking while keeping the literal type checking:

```rust
// Optimization: For literal sources, check if the primitive type is in the union
if let TypeKey::Literal(literal) = source_key {
    let primitive_type = match literal {
        LiteralValue::String(_) => TypeId::STRING,
        LiteralValue::Number(_) => TypeId::NUMBER,
        LiteralValue::BigInt(_) => TypeId::BIGINT,
        LiteralValue::Boolean(_) => TypeId::BOOLEAN,
    };
    // Fast path: exact primitive match
    if member == primitive_type {
        return SubtypeResult::True;
    }
    // For literal-to-literal unions (e.g., "a" <: "a" | "b")
    if matches!(self.interner.lookup(member), Some(TypeKey::Literal(_))) {
        if self.check_subtype(source, member).is_true() {
            return SubtypeResult::True;
        }
    }
    // NEW: Also check if the literal is a subtype of intrinsic union members
    // This handles cases like "hello" <: (string | { toString(): string })
    if matches!(self.interner.lookup(member), Some(TypeKey::Intrinsic(_))) {
        if self.check_subtype(source, member).is_true() {
            return SubtypeResult::True;
        }
    }
}
```

## Test Results

### Test Cases Covered

All of the following now work **without** TS2322 errors:

```typescript
// 1. Literal to primitive union
acceptStringOrNumber("hello");  // "hello" <: string | number ✓
acceptStringOrNumber(42);       // 42 <: string | number ✓

// 2. Literal to literal union
acceptAOrB("a");  // "a" <: "a" | "b" ✓

// 3. Const literals (preserved literal types)
const str = "hello";
acceptStringOrNumber(str);  // "hello" <: string | number ✓

// 4. Widened literals (let/var)
let num = 42;  // Widened to number
acceptStringOrNumber(num);  // number <: string | number ✓

// 5. Union with object types
acceptStringOrObject("hello");  // "hello" <: string | { name: string } ✓

// 6. Union to all-optional objects
acceptOptionalProps({ a: "hello" });  // ✓
acceptOptionalProps({ b: 42 });       // ✓
```

### Validation

```bash
$ ./target/release/tsz test_ts2322_union_clean.ts
# No errors! (0 errors)
```

## Architecture Impact

### File Modified
- `src/solver/subtype_rules/unions.rs`: Added intrinsic type check in `check_union_target_subtype()`

### No Breaking Changes
- This is a **pure additive fix** - it only adds an additional check path
- Existing behavior is preserved
- No changes to API or configuration

### Related Code
The fix complements existing literal widening logic:
- `src/checker/type_checking.rs::widen_literal_type()`: Widens literals in let/var contexts
- `src/checker/type_computation.rs::get_type_of_variable_declaration()`: Preserves literals in const contexts
- `src/solver/subtype_rules/objects.rs::check_union_to_all_optional_object()`: Handles union literal widening

## Expected Impact on Conformance

This fix should reduce:
- **Extra TS2322 errors**: Cases where literals are correctly assignable to unions but were incorrectly flagged
- **Specifically**: Literal-to-primitive-union assignability (e.g., `"hello" <: string | number`)

## Next Steps

1. Run full conformance test to measure improvement:
   ```bash
   ./conformance/run-conformance.sh --all --workers=14 --filter "TS2322" --count 1000
   ```

2. Investigate remaining TS2322 categories:
   - Object literal excess property checking
   - Generic function parameter inference
   - Array literal contextual typing with generics

3. Consider further optimizations:
   - Cache literal-to-primitive mappings for faster lookups
   - Early exit for common union patterns

## References

- Previous work: Commit 62ea1c8b7 "fix(solver): Improve literal-to-union assignability checking"
- Investigation: `docs/TS2322_INVESTIGATION.md`
- TypeScript behavior: https://github.com/microsoft/TypeScript/issues/13813
