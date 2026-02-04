# Session tsz-2: Intersection Reduction and Advanced Type Operations

**Started**: 2026-02-04
**Current Focus**: Implement Intersection Reduction as highest priority

## Completed Work

✅ **BCT Implementation**: get_base_type for ObjectWithIndex class instances
✅ **Nominal Subtyping Fix**: check_nominal_inheritance for class instances
✅ **Conformance Baseline**: 5357/12847 passed (41.7%)
✅ **Application Expansion**: Verified working correctly

## Current Session Plan (Redefined 2026-02-04)

### ✅ Priority 1: Intersection Reduction (COMPLETED 2026-02-04)
**Status**: Implemented and committed (commit 7bf0f0fc6)

**What was implemented**:
- Added recursive evaluation for `Intersection` and `Union` types
- Implemented `evaluate_intersection()` and `evaluate_union()` methods
- This enables "deferred reduction" where meta-types inside intersections/unions
  are evaluated first, then re-interned to trigger normalization

**Key changes in src/solver/evaluate.rs**:
1. Added `TypeKey::Intersection` case to main `evaluate()` match statement
2. Added `TypeKey::Union` case to main `evaluate()` match statement
3. Implemented recursive evaluation methods that:
   - Evaluate each member using `self.evaluate(member)`
   - Re-intern the result to trigger normalization
   - Properly handle recursion depth limits and cycle detection

**Tests added** (all passing):
- `test_intersection_reduction_disjoint_primitives`: string & number -> never ✅
- `test_intersection_reduction_any`: string & any -> any ✅
- `test_union_reduction_duplicates`: string | string -> string ✅
- `test_union_reduction_literal_into_base`: "hello" | string -> string ✅

**Gemini Pro Review**: Implementation confirmed correct ✅

### ✅ Priority 2 & 3: BCT for Intersections + Lazy Support (COMPLETED 2026-02-04)
**Status**: Implemented and committed (commit 7dfee5155)

**What was implemented**:
Enhanced `collect_class_hierarchy` in `src/solver/infer.rs` to handle:
1. **Intersection types** - Recurse into all members to extract commonality
2. **Lazy types** - Follow extends chain via resolver

**Key changes in src/solver/infer.rs**:
- Added `TypeKey::Intersection(members_id)` case to `collect_class_hierarchy`
- Added `TypeKey::Lazy(_)` case to `collect_class_hierarchy`
- Added test `test_best_common_type_with_intersections` to verify functionality

**How it works**:
- For intersections: Recursively collects hierarchy from each member
  - Example: `[A & B, A & C]` will collect A, B, and C as candidates
  - `find_common_base_class` then filters by subtype relationship
- For Lazy: Calls `get_extends_clause` to traverse the inheritance chain
- Uses existing cycle detection (`hierarchy.contains(&ty)`)

**Gemini Review**:
- ✅ Lazy type handling is correct
- ⚠️ Noted: `normalize_intersection` in `intern.rs` sorts by TypeId
  - This destroys source order for call signatures (overload resolution)
  - **Not a blocker for BCT** since BCT uses `is_subtype` checks
  - Documented as separate issue for future work

**Tests**:
- All 16 existing BCT tests pass ✅
- New test `test_best_common_type_with_intersections` passes ✅

### ✅ Priority 4: Literal Widening for BCT (COMPLETED 2026-02-04)
**Status**: Implemented and committed (commit c3d5d36d0)

**What was implemented**:
Added literal widening to `compute_best_common_type` in `src/solver/expression_ops.rs`
to match TypeScript's behavior for array literal inference.

**Key changes in src/solver/expression_ops.rs**:
- Added `widen_literals()`: Widens each literal to its primitive type
  - Example: `[1, 2]` -> `[number, number]`
  - Example: `["a", "b"]` -> `[string, string]`
  - Example: `[1, "a"]` -> `[number, string]` (mixed types supported)
- Added `find_common_base_type()`: Finds common base for literals
- Added `get_base_type()`: Extracts primitive type from literals
- Added `all_types_are_narrower_than_base()`: Validates subtype relationships
- Updated `compute_best_common_type()` to use widening in BCT algorithm

**Bugs fixed during implementation**:
- Initial implementation aborted widening on mixed types (e.g., `[1, "a"]`)
- Gemini Pro caught this: TypeScript widens each literal individually
- Fixed to unconditionally widen literals, preserving non-literals

**Impact**:
- Before: `[1, 2]` inferred as `(1 | 2)[]`
- After: `[1, 2]` inferred as `number[]` ✅
- Matches TypeScript's Rule #10 (Literal Widening)

**Tests**:
- All 18 BCT tests pass ✅
- Including `test_best_common_type_literal_widening` ✅

**Gemini Pro Review**: Confirmed correct after fixing mixed-type widening bug ✅

---

## Summary

**Session Status**: Continue current session (momentum built in Solver)

**Next Step**: Ask Gemini Question 1 for Intersection Reduction approach

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

### Priority 4: BCT for Intersections (NEXT)
**File**: `src/solver/expression_ops.rs`

**Problem**: BCT doesn't find commonality between intersection types
- `(A & B)` and `(A & C)` should result in `A` (or `A & (B | C)`)
- Currently likely returns flat union `(A & B) | (A & C)`

**Task**:
- Implement common member extraction for intersections
- Modify `compute_best_common_type` to handle intersection types
- Example: `(Dog & Serializable) | (Cat & Serializable)` → `Animal & Serializable`

**⚠️ MANDATORY**: Follow Two-Question Rule before implementing:
```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to implement BCT for Intersections.
What is the best way to extract common members from intersections without infinite loops?"
```

### Priority 5: Intersection Reduction
**File**: `src/solver/intern.rs`

**Problem**: Intersections with disjoint types should reduce to `never`
- `string & number` should consistently reduce to `never`
- Need to resolve `Lazy(DefId)` types before checking disjointness

**Task**:
- Enhance `intersection_has_disjoint_primitives`
- Ensure `Lazy` types are resolved before disjointness check

### Priority 6: Refine get_base_type for Lazy
**File**: `src/checker/context.rs`

**Task**:
- Review `TypeResolver::get_base_type` implementation
- Ensure correct handling of `Lazy` -> `DefId` -> `SymbolId` -> `InheritanceGraph` path
- Test with class-like structures

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
- 2026-02-04: Ready for implementation following Gemini guidance
- 2026-02-04: **Intersection Reduction COMPLETED**
  - Followed Two-Question Rule (asked Gemini Flash for approach, Gemini Pro for review)
  - Implemented recursive evaluation for Intersection and Union types
  - Added 4 unit tests (all passing)
  - Commit 7bf0f0fc6 pushed to origin/main
  - Gemini Pro confirmed implementation correct
- 2026-02-04: **BCT for Intersections + Lazy Support COMPLETED**
  - Followed Two-Question Rule (asked Gemini Flash for approach, Gemini Pro for review)
  - Enhanced collect_class_hierarchy to handle Intersection and Lazy types
  - Added test_best_common_type_with_intersections
  - All 16 BCT tests pass
  - Commit 7dfee5155 pushed to origin/main
  - Gemini Pro confirmed implementation correct for BCT use case
  - Documented intersection sorting issue in intern.rs as separate concern
- 2026-02-04: **Literal Widening for BCT COMPLETED**
  - Followed Two-Question Rule (asked Gemini Flash for approach, Gemini Pro for review)
  - Implemented widen_literals in compute_best_common_type
  - Fixed critical bug: widened each literal individually (not just homogeneous sets)
  - All 18 BCT tests pass
  - Commit c3d5d36d0 pushed to origin/main
  - Gemini Pro confirmed correct after bug fix
  - Impact: [1, 2] now correctly infers as number[] instead of (1 | 2)[]
