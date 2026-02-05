# Task #32 Graph Isomorphism - Work In Progress

## Status: DefKind Infrastructure Complete ✅

## Completed Work

### Phase 1: TypeKey::Recursive(u32) variant ✅ (Previously completed)
```rust
/// Recursive type reference using De Bruijn index.
///
/// Represents a back-reference to a type N levels up the nesting path.
/// This is used for canonicalizing recursive types to achieve O(1) equality.
Recursive(u32),
```

- Added to src/solver/types.rs
- Visitor pattern updated (visit_recursive, for_each_child, TypeKind)
- All pattern matches fixed for Recursive variant

### Phase 2: TypeKey::BoundParameter(u32) variant ✅ (Previously completed)
```rust
/// Bound type parameter using De Bruijn index for alpha-equivalence.
///
/// When canonicalizing generic types, we replace named type parameters
/// with positional indices to achieve structural identity.
BoundParameter(u32),
```

- Added to src/solver/types.rs
- All pattern matches fixed for BoundParameter variant (11 locations)

### Phase 3: DefKind Infrastructure ✅ (Just completed)

**Implementation Summary:**
Added def_kinds storage to TypeEnvironment to enable the Canonicalizer to distinguish between structural and nominal types.

**Changes Made:**
1. **TypeEnvironment struct** (src/solver/subtype.rs):
   - Added `def_kinds: HashMap<u32, DefKind>` field
   - Added `insert_def_kind(def_id, kind)` method
   - Added `get_def_kind(def_id) -> Option<DefKind>` method

2. **TypeResolver trait** (src/solver/subtype.rs):
   - Implemented `get_def_kind` for TypeEnvironment
   - Delegates to the def_kinds map

3. **BinderTypeDatabase** (src/solver/db.rs):
   - Implemented `get_def_kind` for BinderTypeDatabase
   - Delegates to `type_env.borrow().get_def_kind(def_id)`

**Commit**: `af9b82f68`

## Next Steps

### Step 1: MANDATORY Gemini Consultation (Question 2 - Pro)

Before implementing Canonicalizer, ask Gemini Pro for implementation review:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "
I've completed the get_def_kind() infrastructure for Task #32.

What I've done:
1. Added def_kinds: HashMap<u32, DefKind> to TypeEnvironment
2. Added insert_def_kind() and get_def_kind() methods
3. Implemented get_def_kind() in TypeResolver for TypeEnvironment and BinderTypeDatabase

Next: I'm ready to implement the Canonicalizer struct.

Planned approach:
1. Create Canonicalizer struct in src/solver/intern.rs with:
   - def_stack: Vec<DefId> for tracking recursion
   - param_stack: Vec<Vec<Atom>> for tracking nested type parameter scopes
2. Implement canonicalize() method that:
   - Only processes DefKind::TypeAlias (structural)
   - Preserves Lazy(DefId) for Interface/Class/Enum (nominal)
   - Converts Lazy(DefId) -> Recursive(n) for self-references
   - Converts TypeParameter -> BoundParameter(n) for alpha-equivalence

Please review:
1) Is this approach correct?
2) What edge cases should I handle?
3) Should I canonicalize during lowering (lower_type_alias_declaration)?
4) Any pitfalls I'm missing?
"
```

### Step 2: Implement Canonicalizer (after Gemini approval)
Based on Gemini's guidance, implement:
- `struct Canonicalizer<'a, R: TypeResolver>` in src/solver/intern.rs
- `canonicalize(&mut self, type_id: TypeId) -> TypeId` method
- Handle mutual recursion, generic shadowing, mapped types, conditional types

### Step 3: Integrate into TypeLowering
- Modify `lower_type_alias_declaration` in src/solver/lower.rs
- Call Canonicalizer before returning the final TypeId

### Step 4: Add canonical_cache to TypeInterner
```rust
canonical_cache: DashMap<TypeId, TypeId, FxBuildHasher>,
```

## Context from Gemini

**Key Insight**: Only canonicalize TypeAlias (structural), not Classes/Interfaces (nominal).

**Architecture**:
- Use De Bruijn indices: `Recursive(0)` = immediate self-reference
- Use De Bruijn indices: `BoundParameter(0)` = most recent type parameter
- Stack-based cycle detection during unrolling
- Cache canonical forms to avoid O(N) re-traversal
- Critical: Check stack BEFORE resolve_lazy to prevent infinite unroll

**Edge Cases from Gemini:**
- Mutual Recursion: `type A = B; type B = A;`
- Generic Shadowing: `type F<T> = { g: <T>(x: T) => T };`
- Mapped Types: `{ [K in keyof T]: ... }`
- Conditional Types: `T extends U ? (infer R) : Y`
- Constraints: `type F<T extends string>`

**Critical Pitfalls:**
- Only TypeAlias should be expanded (Interface/Class must remain Lazy)
- Infinite expansion if not checking def_stack before resolve_lazy
- De Bruijn index off-by-one errors
- Nominal vs Structural confusion

## Recent Commits

- `af9b82f68`: feat(tsz-1): add DefKind storage to TypeEnvironment
- `a0917e439`: feat(tsz-1): fix BoundParameter pattern matches
- Previous: Recursive variant and pattern matches completed
