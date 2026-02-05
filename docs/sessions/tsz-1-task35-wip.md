# Task #35 - Completing the Canonical Suite

## Status: 📋 NEXT IMMEDIATE TASK

**Priority**: CRITICAL (Completion of Graph Isomorphism)
**Estimated Impact**: Enables O(1) structural identity for objects with methods and proper intersection handling

## Context

Task #32 (Graph Isomorphism) implemented the core `Canonicalizer` with:
- ✅ TypeAlias expansion with De Bruijn indices
- ✅ Object property canonicalization
- ✅ Function type canonicalization
- ✅ Generic application canonicalization
- ✅ Integration API (`are_types_structurally_identical`)

However, two gaps remain for complete canonical coverage:

### Gap 1: Callable Type Canonicalization
**Why Critical**: TypeScript uses `Callable` types for function overloads. Objects with methods need proper callable canonicalization.

### Gap 2: Intersection Type Sorting
**Why Critical**: Without sorting, `{a: string} & {b: number}` and `{b: number} & {a: string}` have different canonical forms, violating O(1) equality.

**Challenge**: Must distinguish between:
- **Structural members** (commutative - can be sorted)
- **Call signatures/overloads** (order matters - preserve relative order)

## Implementation Plan

### Phase 1: Callable Canonicalization

**File**: `src/solver/canonicalize.rs`

**Task**: Implement `TypeKey::Callable` canonicalization similar to `Function`:

```rust
TypeKey::Callable(shape_id) => {
    let shape = self.interner.callable_shape(shape_id);
    // Canonicalize each signature in the overload list
    let c_signatures: Vec<FunctionShape> = shape.signatures.iter().map(|sig| {
        // Canonicalize params, return_type, this_type for each signature
        FunctionShape {
            type_params: sig.type_params.clone(),
            params: sig.params.iter().map(|p| ParamInfo {
                name: p.name,
                type_id: self.canonicalize(p.type_id),
                optional: p.optional,
                rest: p.rest,
            }).collect(),
            this_type: sig.this_type.map(|t| self.canonicalize(t)),
            return_type: self.canonicalize(sig.return_type),
            type_predicate: sig.type_predicate.clone(),
            is_constructor: sig.is_constructor,
            is_method: sig.is_method,
        }
    }).collect();
    self.interner.callable(c_signatures)
}
```

### Phase 2: Intersection Sorting with Overload Preservation

**File**: `src/solver/canonicalize.rs`

**Task**: Sort structural members while preserving callable order:

```rust
TypeKey::Intersection(members_id) => {
    let members = self.interner.type_list(members_id);

    // Separate callables (preserve order) from structural types (sort)
    let mut callables: Vec<TypeId> = Vec::new();
    let mut structural: Vec<TypeId> = Vec::new();

    for &member in members.iter() {
        let canon_member = self.canonicalize(member);
        // Check if this is a callable/function type
        if self.is_callable_type(canon_member) {
            callables.push(canon_member);
        } else {
            structural.push(canon_member);
        }
    }

    // Sort structural members by their canonical TypeId
    structural.sort_by_key(|t| t.0);
    structural.dedup();

    // Combine: structural first (sorted), then callables (preserved order)
    let c_members: Vec<TypeId> = structural.into_iter().chain(callables).collect();
    self.interner.intersection(c_members)
}
```

**Helper Needed**: `is_callable_type(type_id) -> bool`
- Checks if TypeKey is `Function` or `Callable`

## Gemini Consultation Required

### Question 1 (Approach Validation)

Before implementing, ask Gemini:

```bash
./scripts/ask-gemini.mjs --include=src/solver "I am implementing Task #35: Completing the Canonicalizer.

I need to handle Callable types and Intersection sorting.

For Intersections:
1. How should I separate 'structural' members from 'call signatures' during canonicalization?
2. Should I sort structural members by their raw TypeId or their canonical TypeId?
3. How do I ensure I don't break overload resolution order?

For Callables:
1. Is it sufficient to just map over the signatures, or do I need to handle alpha-equivalence of type parameters across the entire callable set?"
```

### Question 2 (Implementation Review)

After implementation, ask Gemini Pro:

```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/canonicalize.rs "I implemented Callable canonicalization and Intersection sorting in src/solver/canonicalize.rs.

Changes:
[PASTE CODE OR DIFF]

Please review: 1) Is this correct for TypeScript's structural typing? 2) Does overload order preservation work correctly? 3) Are there any edge cases I'm missing?"
```

## Testing Strategy

Add tests to `src/solver/tests/isomorphism_tests.rs`:

1. **Callable canonicalization**:
   - `type A = { (): void }` vs `type B = { (): void }` → identical
   - Overload ordering preserved

2. **Intersection order independence**:
   - `{a: string} & {b: number}` vs `{b: number} & {a: string}` → identical
   - Overload order not disrupted

3. **Mixed intersections**:
   - `(A & B) & C` with callables → correct canonical form

## Success Criteria

- All callable tests pass
- Intersection order independence works
- Overload order preserved
- No regression in existing 8 isomorphism tests

## Files to Modify

- `src/solver/canonicalize.rs` - Main implementation
- `src/solver/tests/isomorphism_tests.rs` - Test coverage

## Commits

- [Pending] feat(tsz-1): implement Callable canonicalization
- [Pending] feat(tsz-1): implement Intersection sorting with overload preservation
