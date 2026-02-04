# Session tsz-2: Type Correctness & Conformance

**Started**: 2026-02-04
**Current Focus**: Conformance validation and nominal subtyping fix

## Background

Completed BCT implementation for class instances and verified Application expansion is working. Now focused on core correctness issues and conformance validation.

## Current Session Plan (Redefined 2026-02-04)

### ✅ Priority 1: Conformance Testing (COMPLETE)
**Status**: Baseline established
- **Result**: 5357/12847 passed (41.7%)
- **Time**: 84.5s
- **Key issues**:
  - TS2322 (assignability): 544 extra errors (tsz stricter than tsc)
  - TS2345 (argument compatibility): 448 extra errors
- These extra strictness errors align with nominal subtyping bug

### ✅ Priority 2: Application Type Expansion (COMPLETE)
**Status**: Already implemented and working
- evaluate_application is wired up in evaluate function
- DefIds created for type aliases
- Type parameters stored in def_type_params map
- TODO in evaluate.rs was outdated

### ✅ Priority 3: Nominal Subtyping Fix (COMPLETE)
**Status**: Implemented and committed
**Commit**: `27f1d1a67`

**What was fixed**:
- Classes are now checked for nominal identity before structural comparison
- Added `check_nominal_inheritance` helper function (line 1560)
- Uses `get_base_type` to walk inheritance chain
- Returns False if classes have different symbols and no inheritance relationship

**Test verification**:
```typescript
class A { private x: string; }
class B { private x: string; }
const a: A = new B(); // Error: Type 'B' is not assignable to type 'A'
```

**Known issue**: Pre-existing test failure unrelated to this change
- `test_generic_parameter_without_constraint_fallback_to_unknown` was already failing

### Priority 4: BCT for Intersections
**File**: `src/solver/expression_ops.rs`

Extend `compute_best_common_type` to handle intersection types.
- Example: `(Dog & Serializable) | (Cat & Serializable)` → `Animal & Serializable`

---

**Steps**:
1. Implement `widen_literal_type` - widen `1 | 2` to `number`
2. Respect `const` contexts (preserve literals)
3. Update `resolve_from_candidates` to integrate widening

### Task 3: Tournament Reduction Refinement
**File**: `src/solver/infer.rs`

**Steps**:
1. Optimize `best_common_type` with tournament algorithm
2. Handle interface merging
3. Handle `any`/`unknown` per tsc rules

### Task 4: Array Literal Integration
**File**: `src/solver/array_literal.rs`

**Steps**:
1. Update `ArrayLiteralBuilder` to use refined BCT logic
2. Ensure `build_array_type` uses `best_common_type`

## Success Criteria

- [x] `get_class_base_type` added to TypeDatabase trait
- [ ] Common base class detection implemented
- [ ] Literal widening works: `[1, 2]` → `number[]`
- [ ] Nominal BCT works: `[dog, cat]` → `Animal[]`
- [ ] Union fallback preserved: `[1, "a"]` → `(string | number)[]`
- [ ] Homogeneous fast path preserved (performance)
- [ ] Conformance tests pass

## Complexity: HIGH

**Risk**: Changes to `is_subtype` or `best_common_type` can cause regressions

**Mandatory**: Follow **Two-Question Rule** in AGENTS.md before touching `src/solver/infer.rs`

---

## Implementation Progress

### Completed Infrastructure:
- ✅ **Step 1 Complete**: Added `get_base_type` to TypeResolver trait (src/solver/subtype.rs:197)
- ✅ **Step 2 Complete**: Implemented `get_base_type` in CheckerContext (src/checker/context.rs:1772)
- ✅ **Step 3 Complete**: Updated InferenceContext to hold resolver reference (src/solver/infer.rs:303-327)
- ✅ **Step 4 Complete**: Implemented `get_class_hierarchy` using TypeResolver (src/solver/infer.rs:1423-1431)
- ✅ **Step 5 Complete**: Wired BCT into array literals (src/solver/expression_ops.rs:114-184)
- ✅ **Fixed merge conflicts** in narrowing.rs
- ✅ **Step 6 Complete**: Implemented ObjectWithIndex handling for class instances (commit 0ff075ea3)
- ✅ **Step 7 Complete**: All 16 BCT unit tests pass
- ✅ **Gemini Review Complete**: Implementation confirmed correct

### Known Issues:

#### 1. Subtype Check Bug (Pre-existing)
❌ **BUG**: `is_subtype_of` treats ObjectWithIndex types structurally instead of nominally
- Impact: Causes BCT to return incorrect results when subtype check is wrong
- Example: `is_subtype_of(Cat, Dog)` returns `true` (WRONG!)
- Root cause: `src/solver/subtype.rs` performs structural check, ignores nominal identity
- Status: Pre-existing bug, not introduced by BCT work
- Fix needed: Separate issue to fix nominal type checking in subtype.rs

#### 2. BCT Behavior vs TypeScript
⚠️ **Note**: TypeScript infers `(Dog | Cat)[]` for `[new Dog(), new Cat()]`
- This is a union, not the common base `Animal[]`
- Current implementation would return `Animal[]` if subtype check worked correctly
- This may be correct TypeScript behavior (union vs base class preference)
- Needs investigation of TypeScript's actual BCT rules

### Implementation Details:

**get_base_type now handles three type representations:**
1. Lazy types (type aliases, some class references)
2. ObjectWithIndex types (class instances with nominal identity)
3. Callable types (legacy class constructor handling)

**Key changes in commit 0ff075ea3:**
```rust
// Extract symbol from ObjectShape for nominal identity
if let Some(shape_id) = object_shape_id(interner, type_id)
    .or_else(|| object_with_index_shape_id(interner, type_id))
{
    let shape = interner.object_shape(shape_id);
    if let Some(sym_id) = shape.symbol {
        let parents = self.inheritance_graph.get_parents(sym_id);
        if let Some(&parent_sym_id) = parents.first() {
            return self.symbol_instance_types.get(&parent_sym_id)
                .or_else(|| self.symbol_types.get(&parent_sym_id))
                .copied();
        }
    }
}
```

## Next Steps

1. ✅ Session redefined with this plan
2. ✅ Gemini Question 1 COMPLETE - got architectural guidance
3. ✅ Implement following Question 1 guidance (COMPLETE)
4. ✅ DEBUG and fix ObjectWithIndex handling (COMPLETE)
5. ✅ Gemini Question 2 COMPLETE - implementation reviewed and confirmed correct
6. ⏸️ **OPTIONAL**: Fix subtype check bug in src/solver/subtype.rs (separate issue)
7. ⏸️ **TODO**: Investigate TypeScript's actual BCT behavior (union vs base class preference)

### Key Architectural Decision (from Gemini Pro):

**Use TypeResolver trait, NOT TypeDatabase!**

The chain: `InferenceContext` → `TypeResolver` → `CheckerContext` → `Binder`

**Implementation Steps**:
1. Add `get_base_type` to `TypeResolver` trait (src/solver/subtype.rs)
2. Implement in `CheckerContext` (src/checker/context.rs)
3. Update `InferenceContext` to hold resolver reference
4. Implement `find_common_base_class` in src/solver/infer.rs
5. Implement `get_class_hierarchy` using the new TypeResolver hook

### Critical Edge Cases (from Gemini):
- Cycle protection in hierarchy traversal
- Use `TypeResolver` for semantic lookups (extends clause)
- `TypeDatabase` remains pure storage
- Interior mutability considerations when calling stateful methods

## Session History

- 2026-02-04: Started with TDZ focus (completed)
- 2026-02-04: Worked on class type resolution (completed)
- 2026-02-04: Redefined to BCT work
- 2026-02-04: Question 1 complete - got detailed architectural guidance
- 2026-04-04: Ready for implementation following Gemini guidance
