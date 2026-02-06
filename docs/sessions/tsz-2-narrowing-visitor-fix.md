# Session tsz-2: Fix NarrowingVisitor for Complex Types

**Started**: 2026-02-06
**Status**: Completed
**Focus**: Make the general `narrow()` function as robust as `narrow_by_discriminant()`

## Problem

The `NarrowingVisitor` (used by the `narrow()` function) was too conservative:
- **Lazy/Ref/Application types**: Just returned `narrower` without resolving
- **Intersection types**: Had TODO for proper narrowing
- **Object/Function types**: Just returned `narrower`

This made the general `narrow()` function less robust than specialized functions like `narrow_by_discriminant()`.

## Solution

### 1. Added `visit_type` Override

Created a custom `visit_type` implementation in `NarrowingVisitor` to:
- **Resolve Lazy/Ref/Application types**: Use `self.db.evaluate_type()` and recurse with the resolved type
- **Handle Object types**: Check subtype relationships and return appropriate result
- **Handle Function types**: Same subtype checking as Object

### 2. Fixed `visit_intersection`

Changed to recursively narrow each intersection member:
```rust
let narrowed_members: Vec<TypeId> = members
    .iter()
    .filter_map(|&member| {
        let narrowed = self.visit_type(self.db, member);
        if narrowed == TypeId::NEVER {
            None
        } else {
            Some(narrowed)
        }
    })
    .collect();
```

For `(A & B)` narrowed by `C`, the result is `(A narrowed by C) & (B narrowed by C)`.

### 3. Fixed Object/Function Narrowing (Critical Bug Fix)

**Initial Implementation Had Reversed Logic** (found by Gemini Pro review):

Wrong:
```rust
// Case 1: type_id is subtype of narrower -> return narrower (WRONG!)
if is_subtype_of(self.db, type_id, self.narrower) {
    return self.narrower;
}
```

This would widen `{ a: "foo" }` to `{ a: string }`, losing information.

Correct:
```rust
// Case 1: type_id is subtype of narrower -> keep type_id (more specific)
if is_subtype_of(self.db, type_id, self.narrower) {
    return type_id;
}
// Case 2: narrower is subtype of type_id -> use narrower (narrow down)
if is_subtype_of(self.db, self.narrower, type_id) {
    return self.narrower;
}
```

## Results

- ✅ All 3543 solver tests pass (up from 3527)
- ✅ NarrowingVisitor now properly handles Lazy/Ref/Application resolution
- ✅ Intersection types are narrowed correctly
- ✅ Object/Function types use correct subtype logic

## Commit

- `ec3a0f9ec`: feat(solver): fix NarrowingVisitor for complex types

## Success Criteria

- [x] Lazy/Ref/Application types are resolved before narrowing
- [x] Intersection types are properly narrowed
- [x] Object/Function types use correct subtype logic
- [x] All solver tests pass (3543 passing)
