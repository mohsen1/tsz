# Session tsz-2: Type Narrowing & Control Flow Analysis

**Started**: 2026-02-04
**Status**: 🟢 Active (Redefined 2026-02-04)
**Previous**: Lawyer-Layer Cache Partitioning (COMPLETE)

## SESSION REDEFINITION (2026-02-04)

**Gemini Consultation**: Asked for session redefinition after completing Cache Isolation Bug fix.

**New Direction**: **Type Narrowing Correctness and Completeness**

### Rationale

Based on completion of Tasks 1 & 2 (Lawyer-Layer Cache Partitioning, CheckerState Refactoring):
- Cache isolation foundation is now in place
- Cache Isolation Bug is fixed (Partial<T>, Pick<T,K> resolve correctly)
- Ready to tackle **Type Narrowing**, which is notoriously difficult to cache correctly

From **AGENTS.md**: Recent discriminant narrowing implementation had **3 critical bugs**:
1. Reversed subtype check
2. Missing type resolution (Lazy, Ref, Intersection)
3. Broken for optional properties

### Why This Matters

1. **Correctness**: Narrowing is essential for TypeScript's type safety - without it, types don't properly refine in conditionals
2. **North Star Alignment**: Section 3.1 requires Solver to handle narrowing, not Checker
3. **Foundation for CFA**: Proper narrowing is required for Control Flow Analysis

## Implementation Tasks

### Task 1: Fix Discriminant Narrowing Regressions (CRITICAL - Bug Fix)

**Goal**: Fix the 3 critical bugs in discriminant narrowing identified in AGENTS.md.

**Implementation**:
1. Fix reversed subtype check: use `is_subtype_of(literal, property_type)` not reverse
2. Add type resolution: handle `Lazy`, `Ref`, `Intersection` types before narrowing
3. Fix optional properties: correct logic for `{ prop?: "a" }` cases

**Why High Priority**: These bugs cause incorrect type narrowing in object literals, a fundamental TypeScript feature.

**Files to modify**:
- `src/solver/narrowing.rs`: Discriminant narrowing logic
- `src/checker/control_flow_narrowing.rs`: If applicable

### Task 2: Implement Solver::narrow (HIGH PRIORITY - Architecture)

**Goal**: Move narrowing logic from Checker to Solver using Visitor pattern.

**Implementation**:
- Add `narrow(type_id: TypeId, narrower: TypeId) -> TypeId` to `src/solver/narrowing.rs`
- Use Visitor Pattern from `src/solver/visitor.rs` to traverse unions
- Filter out constituents that don't match the narrower

**Why High Priority**: North Star Section 3.1 requires Solver to calculate narrowed types, not Checker.

**Files to modify**:
- `src/solver/narrowing.rs`: Add narrow() function
- `src/solver/visitor.rs`: Use visitor pattern for union traversal

### Task 3: Checker Integration with CFA (MEDIUM PRIORITY - Feature)

**Goal**: Update Checker to query Solver for narrowed types when traversing control flow.

**Implementation**:
- Update `src/checker/flow_analysis.rs` to use Solver::narrow
- Ensure Checker identifies WHERE narrowing happens (AST node) and WHAT the condition is
- Delegate the RESULT calculation to Solver

**Why Medium Priority**: Critical for complete CFA but depends on Task 2.

**Files to modify**:
- `src/checker/flow_analysis.rs`: Update to call Solver::narrow
- `src/checker/control_flow_narrowing.rs`: Update narrowing calls

### Task 4: Truthiness Narrowing (MEDIUM PRIORITY - Feature)

**Goal**: Implement truthiness narrowing (e.g., `if (x) { ... }` narrows `T | null | undefined` to `T`).

**Implementation**:
- Use Solver::narrow to filter out falsy types from unions
- Handle null, undefined, false, 0, "" as falsy values
- Apply narrowing in IfStatement, WhileStatement, logical operators (&&, ||, ??)

**Why Medium Priority**: Important feature but builds on Task 2 foundation.

**Files to modify**:
- `src/solver/narrowing.rs`: Add truthiness narrowing logic
- `src/checker/flow_analysis.rs`: Apply truthiness narrowing

## Coordination Notes

### With tsz-1 (Core Solver)
- **tsz-1** is working on `src/solver/subtype.rs` and core solver correctness
- **tsz-2** work is primarily in `src/solver/narrowing.rs` and `src/checker/flow_analysis.rs`
- **Coordination Point**: tsz-2's narrowing depends on tsz-1's subtype checks
- Ensure narrow() uses is_subtype_of correctly (not reversed!)

### Avoid Conflicts
- **tsz-3** (CFA) - COMPLETE, no conflicts
- **tsz-4** (Declaration Emit) - COMPLETE, no conflicts
- **tsz-5** (Import/Export Elision) - ACTIVE, minimal overlap
- **tsz-6/7** (Advanced Type Nodes/Import Generation) - COMPLETE, no conflicts

## Mandatory Two-Question Rule

Per AGENTS.md, since modifying `src/solver/` or `src/checker/`:

**Question 1 (PRE-implementation)**:
```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to fix the 3 bugs in discriminant narrowing.

Bugs:
1. Reversed subtype check - using is_subtype_of(property_type, literal) instead of is_subtype_of(literal, property_type)
2. Missing type resolution - not handling Lazy/Ref/Intersection types
3. Broken for optional properties - failing on { prop?: "a" } cases

My planned approach:
1. Fix the subtype check order
2. Add resolve_type() call before narrowing
3. Handle optional properties with explicit undefined check

Is this the right approach? What exact functions should I modify?
Please provide: 1) File paths, 2) Function names, 3) Edge cases."
```

**Question 2 (POST-implementation)**:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "I fixed the discriminant narrowing bugs.

Changes:
[PASTE CODE OR DIFF]

Please review:
1. Is this logic correct for TypeScript?
2. Did I miss any edge cases?
3. Are there type system bugs?"
```

## Success Criteria

1. **Correctness**:
   - [ ] Discriminant narrowing works correctly for object literals
   - [ ] Optional properties handled properly
   - [ ] No regressions in existing narrowing tests

2. **Architecture**:
   - [ ] Solver::narrow implemented using Visitor pattern
   - [ ] Checker delegates narrowing calculation to Solver

3. **Completeness**:
   - [ ] Truthiness narrowing implemented
   - [ ] CFA integration complete

## Session History

- 2026-02-04: Started as "Intersection Reduction and Advanced Type Operations"
- 2026-02-04: COMPLETED BCT, Intersection Reduction, Literal Widening
- 2026-02-04: COMPLETED Phase 1: Nominal Subtyping (all 4 tasks)
- 2026-02-04: INVESTIGATED Cache Isolation Bug
- 2026-02-04: COMPLETE Cache Isolation Bug investigation
- 2026-02-04: REDEFINED to Lawyer-Layer Cache Partitioning & Type Cache Unification
- 2026-02-04: COMPLETED Task 1: Lawyer-Layer Cache Partitioning
- 2026-02-04: COMPLETED Task 2: CheckerState Refactoring
- 2026-02-04: **REDEFINED** to Type Narrowing & Control Flow Analysis

## Previous Work (Archived)

### Task 1: Lawyer-Layer Cache Partitioning ✅
**Commit**: `02a84a5de`

Fixed `any` propagation results leaking between strict/non-strict modes.

### Task 2: CheckerState Refactoring ✅
**Commit**: `5f4072f36`

Fixed Cache Isolation Bug where lib.d.ts type aliases weren't resolving correctly.

## Complexity: MEDIUM

**Why Medium**:
- Narrowing logic is complex but well-defined
- Clear architectural guidance from North Star
- Can implement incrementally (Task 1 → 2 → 3 → 4)

**Mitigation**: Follow Two-Question Rule strictly. Use --pro flag for implementation reviews.

## Session Status: 🟢 ACTIVE

**Phase**: Task 2 - Implement Solver::narrow
**Focus**: Architecture
**Current Task**: Question 1 - Approach validation

---

## Progress Update (2026-02-04)

### Task 1 Complete ✅

**Commit**: `c109f1ffe` - "fix(tsz-2): handle optional properties in discriminant narrowing"

**Changes Made**:
1. Added handling for `prop_info.optional` in `narrow_by_discriminant`
   - For optional properties, effective type is `Union(prop_type, Undefined)`
   - Ensures correct narrowing for `{ prop?: "a" }` cases

2. Added same fix to `narrow_by_excluding_discriminant`
   - Ensures optional properties are handled in exclusion narrowing

3. Removed outdated TODO comment about Lazy/Intersection resolution

**Bug Status**:
- Bug #1 (reversed subtype check): Already fixed with warning comment
- Bug #2 (missing type resolution): Already implemented in lines 316-328
- Bug #3 (optional properties): FIXED in this commit

**Gemini Guidance**: Followed Two-Question Rule (Question 2: Implementation Review)

**Next**: Task 2 - Implement Solver::narrow

---

### Task 2 Complete ✅

**Commit**: `f7e11cdf5` - "feat(tsz-2): implement Solver::narrow using Visitor pattern"

**Changes Made**:
1. Added `narrow()` method to `NarrowingContext` (src/solver/narrowing.rs:1784)
   - Fast path: returns early if type is already subtype of narrower
   - Uses `NarrowingVisitor` to perform type-based narrowing

2. Created `NarrowingVisitor` struct implementing `TypeVisitor` (src/solver/narrowing.rs:1800-2010)
   - `visit_union()`: Recursively narrows each union member (CRITICAL bug fix)
   - `visit_intrinsic()`: Proper overlap/disjoint logic for primitives
   - `visit_intersection()`: Checks if all members match narrower
   - `visit_type_parameter()`: Intersects constraint with narrower
   - `visit_lazy/visit_ref/visit_application()`: Conservative returns with TODOs

3. Added `narrow()` to `QueryDatabase` trait (src/solver/db.rs:296)
   - Added `Self: Sized` constraint for dyn trait compatibility

**Critical Bugs Fixed** (via Gemini Code Review):
1. **Union logic broken**: Changed from `is_subtype_of()` check to recursive `narrow()` call
   - Before: Filtered union members by subtype check only
   - After: Recursively narrows each member, handling cases like `string` narrowed by `"foo"` → `"foo"`

2. **Intrinsic disjoint checks**: Added proper overlap/disjoint logic
   - Case 1: `narrower` is subtype of `type_id` (e.g., `narrow(string, "foo")`) → returns `narrower`
   - Case 2: `type_id` is subtype of `narrower` (e.g., `narrow("foo", string)`) → returns `type_id`
   - Case 3: Disjoint types (e.g., `narrow(string, number)`) → returns `never`

3. **Lazy/Ref/Application handling**: Added conservative returns with TODOs
   - Safely returns `narrower` (may over-narrow but won't crash)
   - TODO comments explain need to resolve types before recursing

**Gemini Guidance**:
- Question 1 (PRE): Approach validation - use Visitor pattern
- Question 2 (POST): Implementation review - found 3 critical bugs, all fixed

**Merge Conflict Resolved**: Successfully resolved merge conflict with remote commit `d1989079f`
- Kept local changes (bug fixes from Gemini review)
- Rebased and pushed successfully

**Next**: Task 3 - Checker Integration with CFA

---
