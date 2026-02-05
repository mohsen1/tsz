# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2025-02-05
**Focus**: Solver (Type Relations & Narrowing)

## Active Tasks

### Task #16: Robust Optional Property Subtyping & Narrowing
**Status**: 🔄 In Progress (Root Cause Analysis Complete)
**Priority**: High
**Estimated Impact**: +2-3% conformance

**Investigation Complete**:
1. `narrow_by_discriminant` (line 491): ✅ CORRECT - `is_subtype_of(literal, prop_type)`
2. `narrow_by_excluding_discriminant` (line 642): ✅ CORRECT - `is_subtype_of(prop_type, excluded_value)`
3. `resolve_type`: Handles Lazy and Application types correctly
4. `optional_property_type` (objects.rs:662): ✅ CORRECT - Checks `exact_optional_property_types` flag
5. `lookup_property` (objects.rs:21-34): ✅ CORRECT - Simple name lookup

**🚨 CRITICAL BUG FOUND (Gemini Pro Review)**:
**Location**: `src/solver/subtype.rs` lines 1064-1071
**Issue**: Intersection property merging **overwrites** instead of **intersecting** types

**Root Cause Analysis** (from Gemini Flash):
- `intern.rs`'s `try_merge_objects_in_intersection` only works for fully collapsible intersections (all concrete objects)
- For intersections with TypeParameters, Unions, or Lazy types, the `SubtypeChecker` must manually merge
- Calling `interner.intersection2()` during subtype checking creates infinite recursion:
  - SubtypeChecker → interner.intersection2 → normalize_intersection → is_subtype_of → **LOOP**

**Architectural Issue**: "Judge vs. Lawyer" conflict
- SubtypeChecker (Judge) is trying to perform type construction (Solver's job) during a relationship check
- North Star Rule 1: Solver handles WHAT, Judge handles RELATIONS

**Implementation Plan** (from Gemini Flash):

**Step 1**: Create `PropertyCollector` visitor in `src/solver/objects.rs`
- Walk TypeIds recursively
- If Object: collect properties
- If Intersection: recursively collect and merge collisions
- Call `resolve_type` before inspecting any TypeKey (fixes Lazy/Ref resolution bug)

**Step 2**: Implement "Safe" merging in PropertyCollector
- **Type**: Do NOT call `interner.intersection2()`. Use new low-level `intersect_types_raw()` that creates TypeKey::Intersection WITHOUT normalization
- **Flags**: Required wins (AND logic), Readonly accumulates (OR logic)
- **Write Type**: Intersect contravariantly

**Step 3**: Update `src/solver/subtype.rs` to use PropertyCollector
- Judge asks "What are the effective properties?" and gets pre-merged map
- No direct calls to interner.intersection during checking

**Step 4**: MANDATORY - Ask Gemini BEFORE implementing:
> "I am implementing `intersect_types_raw` in `src/solver/intern.rs` to avoid stack overflows during subtyping. It should create a `TypeKey::Intersection` without calling `normalize_intersection` which triggers `is_subtype_of`. Is this the correct way to break the cycle, or should I use the `cycle_stack` from the `SubtypeChecker`?"

**Test Cases** (from Gemini):
```typescript
// 1. Basic Intersection Merging (The "Never" Case)
type A = { a: string };
type B = { a: number };
type C = A & B;
let c: C = { a: "any" as any };
// @ts-expect-error: Type 'string' is not assignable to type 'never'
c.a = "hello";

// 2. Optionality Merging (Required wins)
type O = { x?: string };
type R = { x: string };
type M = O & R;
let m: M = { x: "hi" };
// @ts-expect-error: Type 'undefined' is not assignable to type 'string'
m.x = undefined;

// 3. Discriminant Narrowing with Intersections
type Union = ({ kind: "a", val: string } & { extra: number }) | { kind: "b", val: number };
function check(v: Union) {
    if (v.kind === "a") {
        v.val.toUpperCase(); // Should work
        v.extra.toFixed();   // Should work
    }
}

// 4. Deep Intersection (Stack Overflow Guard)
type Deep<T> = { prop: T & { x: string } };
type Rec = Deep<Deep<string>>;
// Ensures getting properties doesn't crash
```
**Status**: Pending
**Priority**: High
**Estimated Impact**: +2-3% conformance

**Description**:
Fix critical bugs in optional property subtyping and narrowing logic identified in AGENTS.md investigation:
1. Reversed subtype checks in discriminant narrowing
2. Missing type resolution for Lazy/Ref/Intersection types
3. Incorrect logic for `{ prop?: "a" }` cases with undefined

**Gemini Guidance**:
> "This is a pure Solver task focusing on the 'WHAT' (the logic of the types themselves).
> Fixes systemic bugs that affect all object-based type operations."

**Implementation Focus**:
- `src/solver/subtype.rs`: Ensure property checks resolve Lazy/Ref/Intersection types
- `src/solver/narrowing.rs`: Fix reversed discriminant check
- Use Visitor pattern for systematic type resolution

**Prerequisites**:
- Follow Two-Question Rule (ask Gemini BEFORE implementing)
- Review AGENTS.md investigation findings
- Understand North Star Rule 2: Use visitor pattern for ALL type operations

### Task #15: Mapped Types Property Collection
**Status**: ⚠️ Blocked - Architecture Issue (Deferred)
**Priority**: Lowered (due to complexity)
**Estimated Impact**: +0.5-1% conformance
**Status**: ⚠️ Blocked - Architecture Issue Found
**Priority**: Medium (lowered due to complexity)
**Estimated Impact**: +0.5-1% conformance

**Description**:
Make excess property checking (TS2353) work for mapped types like `Partial<T>`.

**Investigation Findings**:
1. `Partial<User>` is a Type APPLICATION, not a Mapped type directly
2. The checker's `check_object_literal_excess_properties` uses `get_object_shape` which returns `None` for Application types
3. My solver-layer implementation in `explain_failure` only runs when assignments FAIL
4. For `Partial<User>` with optional properties, assignments often PASS, so `explain_failure` is never called
5. This is an ARCHITECTURE mismatch - excess property checks need to happen in CHECKER layer (before assignability), not SOLVER layer (after failure)

**Root Cause**:
- `check_object_literal_excess_properties` (checker) runs before assignability - correct layer, but doesn't handle Application types
- `find_excess_property` (solver) runs in `explain_failure` - wrong layer (only runs on failure), and doesn't help for passing assignments

**Possible Solutions**:
1. Update `get_object_shape` to evaluate Application types - high complexity
2. Update `check_object_literal_excess_properties` to use `evaluate_type` before `get_object_shape` - medium complexity
3. Make assignments with excess properties FAIL - would break many valid TypeScript patterns

**Recommendation**:
Defer this task. It requires significant refactoring of the checker-layer excess property checking logic.
Focus on higher-priority tasks with better ROI.

**Gemini Consultation**:
Asked Gemini for approach guidance - confirmed this is more complex than initially estimated.
Requires understanding Application type evaluation and checker architecture.

## Completed Tasks

### Task #14: Excess Property Checking (TS2353)
**Status**: ✅ Completed
**Date**: 2025-02-05
**Implementation**:
- Added `ExcessProperty` variant to `SubtypeFailureReason` in `src/solver/diagnostics.rs`
- Added `find_excess_property` function in `src/solver/compat.rs` to detect excess properties
- Updated `explain_failure` in `src/solver/compat.rs` to check for excess properties
- Added case in `render_failure_reason` in `src/checker/error_reporter.rs` to emit TS2353
- Handles Lazy type resolution, intersections, and unions

**Result**: TS2353 now works for basic cases:
```typescript
interface User { name: string; age: number; }
const bad: User = { name: "test", age: 25, extra: true }; // TS2353
```

**Known Limitations**:
- Does not yet handle mapped types (e.g., `Partial<User>`)
- Checker's existing `check_object_literal_excess_properties` has duplicate logic



### Task #11: Method/Constructor Overload Validation
**Status**: ✅ Completed
**Date**: 2025-02-05
**Implementation**: Added manual signature lowering infrastructure in `src/solver/lower.rs`
**Result**: TS2394 now works for methods and constructors

### Task #12: Reachability Analysis (TS7027)
**Status**: ✅ Completed
**Date**: 2025-02-05
**Finding**: Already implemented in `src/checker/reachability_checker.rs`
**Verification**: Tested with unreachable code scenarios - all working correctly

## Quick Wins (Backlog)

### Excess Property Checking (TS2353)
**Priority**: Medium (+1-2% conformance)
**Location**: `src/solver/lawyer.rs` or `src/solver/compat.rs`
**Description**: Implement check for extra properties in object literals

### Optional Property Subtyping Fixes
**Priority**: Medium
**Location**: `src/solver/subtype.rs`
**Description**: Fix logic for `{ prop?: "a" }` cases with optional properties and undefined

## Session Direction

**Current Focus**: Solver work (Type Relations & Narrowing)
- **Why**: Solver is the "WHAT" - defines type relationships and narrowing logic
- **Goal**: Build robust, complete type system operations

**Key Principles** (from AGENTS.md):
1. **Two-Question Rule**: Always ask Gemini BEFORE and AFTER implementing solver/checker changes
2. **Type Resolution**: Every relation check must handle Lazy, Ref, and Intersection types
3. **Directionality**: Ensure correct subtype check ordering (literal <: property_type, not reverse)

**Recent Learning** (from AGENTS.md investigation 2026-02-04):
- Even "working" features like discriminant narrowing had critical bugs
- 100% of unreviewed implementations had type system bugs
- Gemini Pro consultation is NON-NEGOTIABLE for solver/checker changes

## Recent Commits

- `f78fd2493`: docs(tsz-9): record Gemini Pro approval - plan validated
- `7353a8310`: docs(tsz-9): document investigation findings and bug report

## 2025-02-05 Session Summary

**Tasks Completed**:
- Task #11: Method/Constructor Overload Validation ✅
- Task #12: Reachability Analysis (TS7027) ✅
- Task #13: Type Narrowing Verification ✅
- Task #14: Excess Property Checking (TS2353) ✅
- Task #15: Mapped Types Investigation - Blocked ⚠️

**Task #14 Details**:
Implemented excess property checking for fresh object literals:
- Added `ExcessProperty` variant to `SubtypeFailureReason` in diagnostics.rs
- Added `find_excess_property` function in compat.rs
- Updated `explain_failure` to check for excess properties
- Added case in error_reporter.rs to emit TS2353
- Handles Lazy type resolution, intersections, and unions

**Task #15 Investigation**:
Investigated making excess property checking work for `Partial<T>` and other mapped types.

**Key Findings**:
1. `Partial<User>` is a Type APPLICATION, not a Mapped type directly
2. Checker's `check_object_literal_excess_properties` uses `get_object_shape` which returns `None` for Application types
3. My solver-layer implementation in `explain_failure` only runs when assignments FAIL
4. For `Partial<User>` with optional properties, assignments often PASS, so `explain_failure` is never called

**Root Cause**:
Architecture mismatch between checker and solver layers. Excess property checking needs to happen in CHECKER layer (before assignability), but the checker doesn't handle Application types. My solver-layer implementation only catches excess properties when assignments FAIL, which doesn't help for `Partial<T>`.

**Resolution**:
Task #15 is BLOCKED due to architectural complexity. Requires refactoring checker-layer excess property checking.
Recommendation: Defer and focus on higher-ROI tasks.

**Testing**:
✅ Basic case: `{ name: "test", age: 25, extra: true }` → TS2353 on 'extra'
✅ Valid case: `{ name: "test", age: 25 }` → No error
✅ Index signature: Target with [key: string] disables excess check
❌ Mapped types: `Partial<User>` - doesn't trigger TS2353 (blocked)

**Next Session**:
- Ask Gemini for next high-priority task (skip Task #15)
- Focus on tasks with better ROI and clearer architectural path
- Continue following Two-Question Rule
