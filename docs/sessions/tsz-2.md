# Session tsz-2: Best Common Type (BCT) - Common Base Class & Literal Widening

**Started**: 2026-02-04
**Focus**: Implement Rule #32 - Best Common Type algorithm with proper common base class detection

## Problem Statement

The Best Common Type (BCT) algorithm is the foundation for type inference in:
- Array literals: `[1, 2]` → `number[]`
- Conditional expressions: `cond ? a : b` → common type
- Function return types: inferred from return statements

**Current Gap**: While `UnsoundnessAudit` marks BCT as "Fully Implemented," the actual code in `src/solver/infer.rs` reveals that the **Common Base Class** logic is a placeholder (lines 1270-1300).

**Impact**: Without proper common base class detection:
```typescript
class Animal {}
class Dog extends Animal {}
class Cat extends Animal {}

const animals = [new Dog(), new Cat()];
// tsc infers: Animal[]
// tsz currently infers: Dog | Cat (union - WRONG)
```

This leads to "type not assignable" errors when the result is passed to functions expecting the base class.

## Scope

Implement proper BCT with:
1. **Common Base Class Detection**: Find the most specific common ancestor of class types
2. **Literal Widening**: Widen literals to primitives in non-const contexts
3. **Tournament Reduction**: Proper supertype selection with `any`/`unknown` handling

## Implementation Plan

### Phase 1: TypeDatabase Bridge
**File**: `src/solver/db.rs`, `src/checker/context.rs`
- Add `get_class_base_type(type_id: TypeId) -> Option<TypeId>` to `TypeDatabase` trait
- Implement in `CheckerContext` to bridge Solver → Binder
- Query symbol's `extends` clause via binder

### Phase 2: Hierarchy Traversal
**File**: `src/solver/infer.rs`
- Replace placeholder `get_class_hierarchy` with robust implementation
- Use `InheritanceGraph` or symbol resolution to find all ancestors
- Implement `find_common_base_class` to find most specific common ancestor

### Phase 3: Literal Widening
**File**: `src/solver/infer.rs`
- Ensure `[1, 2]` widens to `number[]` (not `1 | 2[]`)
- Respect `const` contexts (preserve literals)

### Phase 4: Tournament Reduction
**File**: `src/solver/infer.rs`
- Refine supertype selection logic
- Handle `any`, `unknown`, `null`/`undefined` per tsc rules

## Success Criteria

- [ ] `get_class_base_type` added to TypeDatabase trait
- [ ] Common base class detection implemented
- [ ] Literal widening in array literals works
- [ ] Test: `[new Dog(), new Cat()]` infers `Animal[]`
- [ ] Test: `[1, 2]` infers `number[]`
- [ ] No regressions in existing BCT tests
- [ ] Conformance tests pass

## Complexity

**Medium**
- Logic is well-defined by TypeScript
- Primary challenge: bridging Solver (TypeId) to Binder (symbols)
- Requires care with concurrent interner architecture

## Why tsz-2?

- **No Conflict**: Different space from tsz-1 (property access) and tsz-3 (narrowing)
- **Focused**: Targets specific Rule #32 gap
- **High-Impact**: Fixes fundamental type inference behavior

## Previous Work: TDZ (2026-02-04) ✅

TDZ checking for static blocks, computed properties, and heritage clauses is complete.
See: `docs/sessions/history/tsz-2-tdz-20260204.md`
