# Fix "Zombie Freshness" Architecture Violation

**Reference**: Architectural Review Summary - Issue #3  
**Severity**: 🔴 Critical  
**Status**: DONE  
**Priority**: Critical - Architecture violation and correctness divergence

---

## Problem

Freshness (excess property checking) is tracked both syntactically (`checker/state.rs`) and via `StickyFreshnessTracker` using `SymbolId` (`solver/sound.rs`). Solver shouldn't know about `SymbolId` (Binder concept). `tsc` tracks freshness on the type object itself (transiently). External tracking risks desynchronization.

**Impact**: Solver thinks type is fresh but Checker doesn't (or vice versa), causing divergence from `tsc`.

**Locations**: 
- `src/checker/state.rs` (syntactic tracking)
- `src/solver/sound.rs` (StickyFreshnessTracker with SymbolId)

---

## Solution: Transient Freshness on TypeKey

Freshness must be an intrinsic property of the `TypeId`, distinguishing fresh object literals from non-fresh objects structurally.

### Design: Freshness in ObjectFlags

```rust
// src/solver/types.rs

// Ensure ObjectShape includes flags in PartialEq/Hash
#[derive(Debug, Clone, PartialEq, Eq, Hash)] 
pub struct ObjectShape {
    pub flags: ObjectFlags, // MUST be part of the hash
    pub properties: Vec<PropertyInfo>,
    // ...
}

// Freshness is a bit in ObjectFlags
bitflags! {
    pub struct ObjectFlags: u32 {
        const FRESH_LITERAL = 1 << 0;
        // ...
    }
}
```

### Widening Operation

The Solver will provide a pure operation to "widen" a fresh type to its non-fresh equivalent.

```rust
// src/solver/ops.rs
fn widen_freshness(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) => {
            let shape = db.object_shape(shape_id);
            if shape.flags.contains(ObjectFlags::FRESH_LITERAL) {
                let mut new_shape = (*shape).clone();
                new_shape.flags.remove(ObjectFlags::FRESH_LITERAL);
                return db.intern_object_shape(new_shape);
            }
            type_id
        }
        _ => type_id
    }
}
```

---

## Refactor Phases

### Phase 1: Decouple Solver from SymbolId

Move the `StickyFreshnessTracker` logic out of the Solver entirely. It belongs in the Checker, which manages data flow and symbol lifetimes.

- **Move**: `src/solver/sound.rs` -> `src/checker/sound_checker.rs`
- **Refactor**: Remove `SymbolId` imports from `src/solver/mod.rs` and `src/solver/lawyer.rs`.
- **Update**: `SoundLawyer` in `solver/sound.rs` currently holds `StickyFreshnessTracker`. This struct should be split:
  - `SoundTypeRules` (Solver): Pure type logic (array covariance, etc.).
  - `SoundFlowAnalyzer` (Checker): Variable tracking (sticky freshness).

### Phase 2: Implement Freshness Interning

Ensure `TypeInterner` produces distinct IDs for fresh vs. non-fresh objects.

- **Modify**: `src/solver/types.rs` to ensure `ObjectShape` equality checks `flags`.
- **Modify**: `src/solver/intern.rs` to support creating fresh object types explicitly.

### Phase 3: Update Checker Logic

Update the Checker to manage the lifecycle of freshness (creation -> assignment -> widening).

- **Creation**: In `checker/type_computation.rs`, `get_type_of_object_literal` should request a **Fresh** object type from the interner.
- **Assignment**: In `checker/state.rs`, when assigning a type to a variable (e.g., `let x = { a: 1 }`), call `solver.widen_freshness(type)` *unless* Sticky Freshness (Sound Mode) is active.
- **Checking**: Update `check_object_literal_excess_properties` in `checker/flow_analysis.rs` (or `state.rs`) to check `type.is_fresh()` instead of `is_syntactically_fresh(node)`.

---

## Testing Strategy

### Zombie Freshness Test
```typescript
const a = { x: 1, y: 2 }; // Fresh
const b: { x: number } = a; // Error: 'y' is excess (if sticky) or OK (if standard)

const c = { x: 1, y: 2 }; // Fresh
const d = c; // Widen to non-fresh (Standard TS)
const e: { x: number } = d; // OK: 'y' is ignored (width subtyping)
```
*Verify that `a` and `d` have different TypeIds despite identical structure.*

### Interning Identity Test
```rust
let t1 = interner.object_fresh(vec![prop_x]);
let t2 = interner.object(vec![prop_x]);
assert_ne!(t1, t2); // Must be different IDs
```

---

## Migration Plan

| Step | File | Action |
|------|------|--------|
| 1 | `src/solver/types.rs` | Ensure `ObjectShape` derives `Hash`, `Eq` including `flags`. |
| 2 | `src/solver/intern.rs` | Add `intern_object_shape_with_flags` or ensure existing method respects flags. |
| 3 | `src/solver/ops.rs` | Implement `widen_freshness(TypeId) -> TypeId`. |
| 4 | `src/checker/sound_checker.rs` | Create new module. Move `StickyFreshnessTracker` here. |
| 5 | `src/solver/sound.rs` | Remove `StickyFreshnessTracker` and `SymbolId` usage. |
| 6 | `src/checker/state.rs` | Update `check_variable_declaration` to widen types (call `widen_freshness`) before caching symbol types. |
| 7 | `src/checker/flow_analysis.rs` | Update `check_object_literal_excess_properties` to use `type.is_fresh()` query. |

This plan aligns with the **Solver-First** architecture by keeping the Solver pure (types only) and moving variable/symbol lifecycle management to the Checker.

---

## Acceptance Criteria

- [x] `ObjectShape` includes `flags` in hash/equality
- [x] `StickyFreshnessTracker` moved to Checker
- [x] Solver has no `SymbolId` dependencies
- [x] Fresh and non-fresh objects intern to different `TypeId`s
- [x] Widening operation correctly strips freshness
- [ ] Conformance tests pass with no regressions
