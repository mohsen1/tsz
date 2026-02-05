# Task #32 Graph Isomorphism - Work In Progress

## Status: Pattern Match Fixes Complete ✅ (All compilation errors resolved)

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

### Phase 2: TypeKey::BoundParameter(u32) variant ✅ (Just completed)
```rust
/// Bound type parameter using De Bruijn index for alpha-equivalence.
///
/// When canonicalizing generic types, we replace named type parameters
/// with positional indices to achieve structural identity.
BoundParameter(u32),
```

### 3. Fixed Pattern Matches for BoundParameter ✅
Fixed 11 compilation errors across 2 files:

**src/solver/type_queries.rs** (9 locations):
- Line 1067: `classify_constructor_type` - NotConstructor case
- Line 1358: `classify_for_constraint` - NoConstraint case
- Line 1455: `classify_full_signature_type` - NoSignatures case
- Line 1586: `classify_full_iterable_type` - NotIterable case
- Line 1768: `classify_for_property_lookup` - NoProperties case
- Line 1867: `classify_for_evaluation` - Resolved case
- Line 1960: `classify_for_property_access` - Resolved case
- Line 2069: `classify_for_traversal` - Terminal case
- Line 2162: `classify_for_interface_merge` - Other case

**src/solver/visitor.rs** (2 locations):
- Line 1417: `visit_key` - Leaf types (nothing to traverse)
- Line 1699: `check_key` - Leaf types (returns false)

## Next Steps

### 1. Implement get_def_kind() in TypeResolver
**File**: `src/solver/db.rs`
**Action**: Add `get_def_kind(def_id: DefId) -> DefKind` to TypeResolver trait
**Why**: Distinguish TypeAlias (structural) from Interface/Class (nominal)

### 2. MANDATORY Gemini Consultation (Question 1)
Before implementing Canonicalizer, ask:
```bash
./scripts/ask-gemini.mjs --include=src/solver "I am ready to implement the Canonicalizer for Task #32.
Goal: Transform cyclic TypeAlias graphs into trees using TypeKey::Recursive(u32) and TypeKey::BoundParameter(u32).

My planned approach:
1. Create Canonicalizer struct in src/solver/intern.rs with stack: Vec<DefId>
2. Implement intern_canonical(type_id) with De Bruijn index transformation
3. Add canonical_cache: DashMap<TypeId, TypeId> to TypeInterner

Is this correct? How to handle TypeApplication (generics)?
Should I canonicalize TypeParameter names for alpha-equivalence?"
```

### 3. Implement Canonicalizer
- Based on Gemini's guidance from Question 1
- Stack-based cycle detection
- Transform Lazy -> Recursive for TypeAlias only
- Transform TypeParameter -> BoundParameter for alpha-equivalence

### 4. Gemini Question 2 (Pro)
- Implementation review before integration

## Context from Gemini

**Key Insight**: Only canonicalize TypeAlias (structural), not Classes/Interfaces (nominal).

**Architecture**:
- Use De Bruijn indices: `Recursive(0)` = immediate self-reference
- Use De Bruijn indices: `BoundParameter(0)` = most recent type parameter
- Stack-based cycle detection during unrolling
- Cache canonical forms to avoid O(N) re-traversal
- Critical: Check stack BEFORE resolve_lazy to prevent infinite unroll

## Recent Commits

- 2026-02-05: Fixed all 11 BoundParameter pattern match errors (tsz-1)
- Previous: Recursive variant and pattern matches completed (other session)
