# Task #32 Graph Isomorphism - Work In Progress

## Status: Pattern Match Fixes Required (21 compilation errors)

## Completed Work

### 1. Added TypeKey::Recursive(u32) variant (src/solver/types.rs)
```rust
/// Recursive type reference using De Bruijn index.
///
/// Represents a back-reference to a type N levels up the nesting path.
/// This is used for canonicalizing recursive types to achieve O(1) equality.
Recursive(u32),
```

### 2. Updated Visitor Pattern (src/solver/visitor.rs)
- Added `visit_recursive(&mut self, _de_bruijn_index: u32)` to TypeVisitor trait
- Added Recursive case to accept() method
- Added Recursive as leaf type in for_each_child() (no children)
- Added Recursive to TypeKind classification (TypeKind::Reference)

### 3. Fixed Pattern Matches (2 of 21)
- src/emitter/type_printer.rs: Prints as `T{index}`
- src/solver/evaluate_rules/infer_pattern.rs: Added as leaf type (first of two locations)

## Remaining Work

### Fix 19 Pattern Match Errors

The pattern matches need to add `TypeKey::Recursive(_)` or `&TypeKey::Recursive(_)` to existing leaf type lists, usually next to `Lazy(_)`.

#### Files and Line Numbers:
1. **src/solver/evaluate_rules/infer_pattern.rs:342** (1 error)
2. **src/solver/format.rs:188** (1 error)
3. **src/solver/infer.rs:555** (1 error)
4. **src/solver/infer.rs:733** (1 error)
5. **src/solver/instantiate.rs:245** (1 error)
6. **src/solver/lower.rs:1496** (1 error)
7. **src/solver/lower.rs:1834** (1 error)
8. **src/solver/operations.rs:1454** (1 error)
9. **src/solver/operations.rs:1621** (1 error)
10. **src/solver/type_queries.rs:572** (1 error)
11. **src/solver/type_queries.rs:1043** (1 error)
12. **src/solver/type_queries.rs:1333** (1 error)
13. **src/solver/type_queries.rs:1414** (1 error)
14. **src/solver/type_queries.rs:1566** (1 error)
15. **src/solver/type_queries.rs:1832** (1 error)
16. **src/solver/type_queries.rs:1918** (1 error)
17. **src/solver/type_queries.rs:2023** (1 error)
18. **src/solver/type_queries.rs:2143** (1 error)
19. **src/solver/visitor.rs:1400** (1 error)
20. **src/solver/visitor.rs:1677** (1 error)

#### Pattern to Apply:
For most matches that look like:
```rust
TypeKey::Intrinsic(_)
| TypeKey::Literal(_)
| TypeKey::Lazy(_)
| TypeKey::TypeQuery(_)
// ...
```

Change to:
```rust
TypeKey::Intrinsic(_)
| TypeKey::Literal(_)
| TypeKey::Lazy(_)
| TypeKey::Recursive(_)  // <-- ADD THIS LINE
| TypeKey::TypeQuery(_)
// ...
```

For `&TypeKey` reference patterns, add `&TypeKey::Recursive(_)`.

### After Pattern Matches Fixed

1. **Implement Canonicalizer struct in src/solver/intern.rs**
   ```rust
   struct Canonicalizer<'a> {
       interner: &'a TypeInterner,
       resolver: &'a dyn TypeResolver,
       stack: Vec<DefId>,
   }
   ```

2. **Add intern_canonical() entry point**
   - DO NOT modify hot path intern() function
   - Checker/Solver calls intern_canonical() for recursive types

3. **Add canonical_cache to TypeInterner**
   ```rust
   canonical_cache: DashMap<TypeId, TypeId, FxBuildHasher>,
   ```

4. **Implement get_def_kind() in TypeResolver**
   - Need to distinguish TypeAlias (structural) vs Class/Interface (nominal)
   - Only canonicalize TypeAlias, preserve nominal types

## Context from Gemini

**Key Insight**: Only canonicalize TypeAlias (structural), not Classes/Interfaces (nominal).

**Architecture**:
- Use De Bruijn indices: `Recursive(0)` = immediate self-reference
- Stack-based cycle detection during unrolling
- Cache canonical forms to avoid O(N) re-traversal
- Critical: Check stack BEFORE resolve_lazy to prevent infinite unroll

## Next Session

1. Fix all 19 remaining pattern matches
2. Compile and test the Recursive variant
3. Implement Canonicalizer
4. Ask Gemini Question 2 (Pro) for implementation review
