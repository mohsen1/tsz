# Literal Property Subtype Bug

## Issue

TSZ incorrectly accepts assignments between object types with incompatible literal property types.

## Reproduction

```typescript
type A = { x: 1 };
type B = { x: 3 };

declare let a: A;
declare let b: B;

b = a;  // TSZ accepts, TSC rejects with TS2322
```

**Expected:** TS2322 error - Type '1' is not assignable to type '3'
**Actual:** No error

## Impact

This bug affects:
- Discriminated union checking (causes false positives)
- Object property compatibility
- Any code that relies on literal type checking in object properties

## Root Cause (TBD)

The literal-to-literal subtype check in `subtype.rs` lines 787-793 looks correct:

```rust
if let Some(t_lit) = literal_value(self.checker.interner, self.target) {
    return if value == &t_lit {
        SubtypeResult::True
    } else {
        SubtypeResult::False
    };
}
```

But object property checks in `subtype_rules/objects.rs` lines 374-379 may not be reaching this code path correctly:

```rust
if source_read != target_read
    && !self
        .check_subtype_with_method_variance(source_read, target_read, allow_bivariant)
        .is_true()
{
    return SubtypeResult::False;
}
```

The early-exit `source_read != target_read` check is correct (TypeId equality implies compatibility), so the issue must be elsewhere.

## Investigation Needed

1. Add tracing to see if `check_subtype` is being called for literal property types
2. Check if there's type widening happening when properties are read
3. Verify that property types are being constructed with literal TypeIds, not widened types
4. Check if there's any caching or memoization causing incorrect results

## Workaround

None currently. This is a fundamental soundness issue.

## Related Tests

- `assignmentCompatWithDiscriminatedUnion.ts` - expects 3 TS2322 errors, TSZ reports 0
- Many other conformance tests likely affected

## Status

Discovered during boolean discriminated union fix (2026-02-15). Pre-existing bug, not introduced by recent changes.
