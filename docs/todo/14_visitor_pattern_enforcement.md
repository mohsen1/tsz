# Enforce Visitor Pattern

**Reference**: Architectural Review Summary - Issue #8  
**Severity**: 🟠 High  
**Status**: TODO  
**Priority**: High - Code quality and maintainability

---

## Problem

Codebase has manual `match` on `TypeKey` in `subtype.rs`, `evaluate_rules/index_access.rs`, `checker/flow_narrowing.rs`. `AGENTS.md` explicitly forbids this, but it's ignored in critical paths.

**Impact**: Adding new type variants requires hunting down every ad-hoc `match` statement. System is fragile.

**Locations**: 
- `src/solver/subtype.rs`
- `src/solver/evaluate_rules/index_access.rs`
- `src/checker/flow_narrowing.rs`
- Various other files

---

## Solution: Use Visitor Pattern Consistently

Replace all manual `TypeKey` matches with `TypeVisitor` implementations from `src/solver/visitor.rs`.

---

## Audit of Manual TypeKey Matches

| Module | Location | Violation |
|--------|----------|-----------|
| **Solver** | `src/solver/evaluate_rules/index_access.rs` | `evaluate_index_access` matches on `obj_key` (Readonly, Ref, Object, etc.) |
| **Solver** | `src/solver/narrowing.rs` | `find_discriminants` matches on `TypeKey::Union` and `TypeKey::Object` |
| **Solver** | `src/solver/subtype.rs` | `check_subtype_inner` matches on `(source_key, target_key)` tuple |
| **Solver** | `src/solver/subtype_rules/*.rs` | Various helpers (e.g., `is_object_keyword_type`) match on `TypeKey` |
| **Checker** | `src/checker/flow_narrowing.rs` | Uses `classify_for_union_members` which internally matches `TypeKey` (indirect violation) |

---

## Missing Visitor Infrastructure

To support the refactoring, we need to extend `src/solver/visitor.rs` with specialized visitors:

1. **`ControlFlowVisitor`**: For `ControlFlow::Break` support (needed for short-circuiting subtype checks).
2. **`PairVisitor`**: For visiting two types simultaneously (needed for `subtype.rs` double dispatch).
3. **`TransformationVisitor`**: For operations that map `TypeId -> TypeId` (needed for `evaluate_index_access`).

---

## Action Plan

### Phase 1: Refactor `evaluate_rules/index_access.rs`

The `evaluate_index_access` function currently dispatches logic based on the object type's structure.

**Task:** Implement `TypeVisitor` for `IndexAccessEvaluator`.

```rust
// src/solver/evaluate_rules/index_access.rs

impl<'a, R: TypeResolver> TypeVisitor for IndexAccessEvaluator<'a, R> {
    type Output = TypeId;

    fn visit_object(&mut self, shape_id: ObjectShapeId) -> Self::Output {
        let shape = self.interner().object_shape(shape_id);
        self.evaluate_object_index(&shape.properties, self.current_index_type)
    }

    fn visit_union(&mut self, list_id: TypeListId) -> Self::Output {
        // Distribute index access over union
        // ...
    }
    
    // ... implement other variants
}
```

### Phase 2: Refactor `narrowing.rs` (Flow Analysis)

`find_discriminants` manually inspects union members.

**Task:** Create `DiscriminantCollector` visitor.

```rust
// src/solver/narrowing.rs

struct DiscriminantCollector<'a> {
    interner: &'a dyn TypeDatabase,
    discriminants: Vec<DiscriminantInfo>,
}

impl<'a> TypeVisitor for DiscriminantCollector<'a> {
    type Output = ();
    
    fn visit_union(&mut self, list_id: TypeListId) {
        // Logic to find common literal properties
    }
    // ...
}
```

### Phase 3: Refactor `subtype.rs` (The Judge)

This is the most complex refactor due to double dispatch (source vs target).

**Task:** Implement `SubtypeSourceVisitor`.

```rust
// src/solver/subtype.rs

struct SubtypeSourceVisitor<'a, 'b, R> {
    checker: &'b mut SubtypeChecker<'a, R>,
    target: TypeId,
}

impl<'a, 'b, R: TypeResolver> TypeVisitor for SubtypeSourceVisitor<'a, 'b, R> {
    type Output = SubtypeResult;

    fn visit_union(&mut self, list_id: TypeListId) -> Self::Output {
        self.checker.check_union_source_subtype(list_id, self.target)
    }

    fn visit_object(&mut self, shape_id: ObjectShapeId) -> Self::Output {
        // Dispatch to target visitor or use classifier for target
        self.checker.check_object_source_subtype(shape_id, self.target)
    }
    
    // ...
}
```

---

## Implementation Steps

1. **Update `src/solver/visitor.rs`**:
   - Ensure `TypeVisitor` has a default implementation for all methods that returns `Self::Output` (or panics/errors if not implemented, forcing explicit handling).
   - Add `visit_type(type_id)` helper that performs the lookup and dispatch.

2. **Refactor `IndexAccessEvaluator`**:
   - Move logic from `evaluate_index_access` match arms into `TypeVisitor` implementation.
   - Replace the match in `evaluate_index_access` with `self.visit_type(object_type)`.

3. **Refactor `NarrowingContext`**:
   - Replace manual iteration in `find_discriminants` with a visitor that aggregates properties.

4. **Refactor `SubtypeChecker`**:
   - Create `SubtypeSourceVisitor`.
   - Replace `check_subtype_inner` match with `SubtypeSourceVisitor::new(self, target).visit_type(source)`.

---

## Testing Strategy

Since this is a refactor of core logic, we must ensure no regression in behavior.

1. **Unit Tests**:
   - Run `cargo nextest run -p tsz --lib solver::evaluate_rules::index_access`
   - Run `cargo nextest run -p tsz --lib solver::subtype`
   - Run `cargo nextest run -p tsz --lib solver::narrowing`

2. **Conformance Tests**:
   - Run `./scripts/conformance/run.sh --max=500` to verify no regression in pass rate.
   - Specific focus on tests involving:
     - `tests/cases/conformance/types/objectTypeLiteral/indexSignatures`
     - `tests/cases/conformance/types/union`
     - `tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes`

---

## Immediate Execution

Start by refactoring `src/solver/evaluate_rules/index_access.rs` as it is a self-contained violation of the visitor pattern.

```rust
// Example of the change for index_access.rs

// BEFORE
match obj_key {
    TypeKey::Object(shape_id) => { ... }
    TypeKey::Union(members) => { ... }
}

// AFTER
impl<'a, R> TypeVisitor for IndexAccessEvaluator<'a, R> {
    fn visit_object(&mut self, shape_id: ObjectShapeId) -> TypeId { ... }
    fn visit_union(&mut self, list_id: TypeListId) -> TypeId { ... }
}
// In evaluate_index_access:
self.visit_type(object_type)
```

---

## Acceptance Criteria

- [ ] All manual `TypeKey` matches replaced with visitor pattern
- [ ] `visitor.rs` extended with necessary visitor types
- [ ] Critical paths (`subtype.rs`, `index_access.rs`) use visitors
- [ ] No functionality lost
- [ ] Conformance tests pass with no regressions
- [ ] Adding new type variants only requires updating visitor trait
