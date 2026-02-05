# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2025-02-05
**Focus**: Core Type Relations & Structural Diagnostics (The "Judge" Layer)

## Session Redefined (2025-02-05)

**Strategic Position**: While tsz-2 refactors the interface (Solver-First) and tsz-4 handles the Lawyer (nominality/quirks), **tsz-1 focuses on the Judge** (Structural Soundness).

**Core Responsibility**: Ensure core set-theoretic operations (Intersections, Overlap, Subtyping) are mathematically correct.

**Why This Matters**: If the Judge is wrong, the Lawyer (tsz-4) cannot make correct decisions. This is foundational work.

### Coordination Map

| Session | Layer | Responsibility | Interaction with tsz-1 |
|:---|:---|:---|:---|
| **tsz-2** | **Interface** | Removing TypeKey from Checker | **Constraint**: Must use Solver APIs, not TypeKey inspection |
| **tsz-3** | **LSP** | DX Features | No overlap |
| **tsz-4** | **Lawyer** | Nominality & Quirks | **Dependency**: tsz-4 relies on tsz-1's PropertyCollector |
| **tsz-1** | **Judge** | **Structural Soundness** | **Foundation**: Provides core logic everyone uses |

## New Focus: Diagnostic Gap Analysis (2025-02-05)

**Strategic Shift**: After consulting with Gemini, shifting focus to implementing critical missing TypeScript diagnostic codes that would most improve conformance.

## Task Breakdown (Priority Order per Gemini Redefinition - 2025-02-05)

### Priority 0: Task #16.0 - Verification of Task #16 ⚡ CRITICAL
**Status**: 📋 Pending (Next Immediate Action)
**Why First**: tsz-4 (Lawyer) and tsz-2 (Interface) rely on Task #16 being correct. Any bugs here will cause "ghost failures" in their sessions.

**Actions**:
1. Run existing solver tests: `cargo test --lib solver`
2. Run intersection conformance tests if they exist
3. Create unit test for recursive intersections in `src/solver/objects.rs`
   - Test case: `type T = { a: T } & { a: T }` (verifies cycle_stack logic)
   - Test case: `(obj & any) == (any & obj)` (verifies commutativity)
4. Verify no regressions in object/intersection handling

**Estimated Impact**: Confidence in foundation before building more features

---

### Priority 1: Task #16 - Robust Intersection & Property Infrastructure ✅ COMPLETE
**Status**: ✅ Completed (2025-02-05)
**Why First**: Foundation for all object-based checks. tsz-4's nominality checks depend on this.

**Completed Subtasks**:
1. **Task 16.1**: ✅ Low-level Intersection Infrastructure
   - Implemented `intersect_types_raw()` and `intersect_types_raw2()` in `src/solver/intern.rs`
   - Preserves callable order (overloads must stay ordered)
   - Lazy type guard (no simplification if unresolved types present)
   - Does NOT call normalize_intersection or is_subtype_of
   - Commit: 4f0aa612a

2. **Task 16.2**: ✅ Property Collection Visitor
   - Created `src/solver/objects.rs` module with `PropertyCollector`
   - Handles Lazy, Ref, and Intersection types systematically
   - Commutative Any handling (found_any flag)
   - Visibility merging (Private > Protected > Public)
   - Fixed all bugs identified by Gemini Pro review
   - Commit: 4945939bb

3. **Task 16.3**: ✅ Judge Integration
   - Replaced manual property loop in `src/solver/subtype.rs` with `collect_properties()` call
   - North Star Rule: Judge asks Lawyer for effective property set
   - Handles Any, NonObject, and Properties result cases
   - Commit: 7b9b81f7e

**Impact**: Breaks infinite recursion cycle in intersection property merging. Foundation for tsz-4's nominality checks.

---

### Priority 1: Task #17 - TS2367 Comparison Overlap Detection
**Status**: 📋 Planned (After Task #16.0 Verification)
**Why**: Pure set-theory/structural logic - "Can these two sets ever have a non-empty intersection?"

**Gemini Redefinition** (Flash 2025-02-05):
> "This is the perfect next step. It is a pure 'Judge' operation:
> 'Can these two sets ever have a non-empty intersection?'"

**Subtask 17.1 (Solver)**: Implement `are_types_overlapping(a, b)` in `src/solver/subtype.rs`
- If `is_subtype_of(a, b)` or `is_subtype_of(b, a)` → Overlap
- If both are Objects: Use `PropertyCollector` to find common properties
- If one is Literal and other is different Literal of same base type → No overlap
- If one is `string` and other is `number` → No overlap

**Subtask 17.2 (Checker)**: Update `src/checker/expr.rs`
- Check equality comparisons (`===`, `!==`, `==`, `!=`)
- Report TS2367 if types have no overlap

**Constraint**: Follow Two-Question Rule for solver logic
**Must NOT inspect TypeKey in Checker** (tsz-2's rule)

**Example**:
```typescript
// Should emit TS2367
if (1 === "one") { }  // number & string have no overlap
if (true === 1) { }   // boolean & number have no overlap

// Should NOT emit TS2367
if (1 === 2) { }       // both number, overlap possible
```

---

### Priority 2: Task #18 - Structural Intersection Normalization (NEW)
**Status**: 📝 Planned
**Why**: High-impact. Prevents confusing errors on types that should have been simplified to `never`.

**Description**: Ensure the Solver simplifies impossible intersections to `never`.
- Simplify `string & number` to `never`
- Simplify `{ a: string } & { a: number }` to `{ a: never }`
- Core "Judge" responsibility to clean up structural types

---

### Priority 3: TS2416 - Signature Override Mismatch
**Status**: 📝 Planned
**Why**: Structural validation of class/interface hierarchies

**Description**: When a class extends another, verify new signatures are valid subtypes of old ones.
- Interaction: tsz-4 handles *accessibility* (private/protected)
- tsz-1 handles *type* compatibility (structural subtyping)

---

### Priority 3: TS2416 - Signature Override Mismatch
**Status**: 📝 Planned
**Why**: Critical for class hierarchy and interface implementation tests

**Implementation**:
1. Implement `is_signature_assignable_to` in Solver
2. Add check to `src/checker/declarations.rs` for class heritage

---

### Priority 4: TS2366 - Not All Code Paths Return
**Status**: 📝 Planned
**Why**: Essential for function conformance

**Implementation**:
1. Leverage existing `reachability_checker.rs`
2. Check if end-of-function is reachable when return value required

---

## Active Tasks

### Task #16.0: Verify Task #16 Implementation
**Status**: 📋 NEXT IMMEDIATE ACTION
**Priority**: Critical (Foundation Validation)

**Description**:
Verify that Task #16 (Robust Intersection Infrastructure) doesn't regress core behavior.

**Actions**:
1. Run solver tests: `cargo test --lib solver`
2. Create unit tests for:
   - Recursive intersections: `type T = { a: T } & { a: T }`
   - Commutative Any handling: `(obj & any) == (any & obj)`
   - Property merging with intersections
3. Check for regressions in existing intersection/object tests

**Why**: tsz-4 (Lawyer) and tsz-2 (Interface) rely on this being correct.

---

### Task #17: TS2367 - Comparison Overlap Detection
**Status**: 📋 Planned (After Task #16.0)
**Priority**: High
**Estimated Impact**: +1-2% conformance

**Description**:
Implement TS2367: "This condition will always return 'false' since the types 'X' and 'Y' have no overlap."

**Why**:
- Pure "Judge" operation: set-theory overlap detection
- Essential for control flow and equality conformance
- High-impact, self-contained implementation

**Gemini Guidance** (Flash 2025-02-05):
> "This is a pure 'Judge' operation: Can these two sets ever have a non-empty intersection?"

**Implementation Plan** (Two-Question Rule):
1. **Ask Gemini Question 1**: What's the right approach for `are_types_overlapping`?
2. **Subtask 17.1**: Implement in `src/solver/subtype.rs`
3. **Ask Gemini Question 2**: Review the implementation
4. **Subtask 17.2**: Integrate into `src/checker/expr.rs`

---

## Previously Identified Missing Diagnostics (For Reference)

| Priority | Code | Description | Status |
|:---|:---|:---|:---|
| **1** | **TS2367** | Comparison overlap check | ✅ Task #17 created |
| **2** | TS2300 | Duplicate Identifier | 📝 Lower priority |
| **3** | TS2352 | Invalid Type Assertion | 📝 Lower priority |
| **4** | TS2416 | Signature Override Mismatch | ✅ Priority 3 |
| **5** | TS2366 | Not all code paths return | ✅ Priority 4 |

### Already Implemented Diagnostics

Based on Gemini's analysis of `src/checker/error_reporter.rs`:
- **Assignability**: TS2322, TS2741, TS2326, TS2353, TS2559
- **Name Resolution**: TS2304, TS2552, TS2583, TS2584, TS2662
- **Properties**: TS2339, TS2540, TS2803, TS7053
- **Functions/Calls**: TS2345, TS2348, TS2349, TS2554, TS2555, TS2556, TS2769
- **Classes/Inheritance**: TS2506, TS2507, TS2351, TS2715, TS2420, TS2415
- **Operators**: TS18050, TS2469, TS2362, TS2363, TS2365
- **Variables**: TS2403, TS2454
- **Types**: TS2314, TS2344, TS2693, TS2585, TS2749

### Next Task: TS2367 - Comparison Overlap Detection

**Why First**: TS2367 is critical for control flow and equality conformance tests.

**Implementation Plan** (pending Gemini consultation):
1. Add `are_types_overlapping` query to `src/solver/`
2. Update `src/checker/expr.rs` to check comparison expressions (`==`, `===`, `!=`, `!==`)
3. Add reporting logic to `src/checker/error_reporter.rs`

**Example**:
```typescript
if (1 === "one") {  // TS2367: This condition will always return false
    // ...
}
```

## Active Tasks

### Task #17: TS2367 - Comparison Overlap Detection
**Status**: 📋 Planned
**Priority**: High (NEW FOCUS)
**Estimated Impact**: +1-2% conformance

**Description**:
Implement TS2367 diagnostic: "This condition will always return 'false' since the types 'X' and 'Y' have no overlap."

**Why This First**:
- Essential for control flow and equality conformance tests
- Affects `if` statements, `switch` cases, and conditional expressions
- High-impact, relatively self-contained implementation

**Gemini Guidance** (Flash 2025-02-05):
> "Requires: 1) Modifying `src/solver/` to add `are_types_overlapping` query
> 2) Updating `src/checker/expr.rs` to check comparison expressions
> 3) Adding reporting logic to `src/checker/error_reporter.rs`"

**Example Cases**:
```typescript
// Should emit TS2367
if (1 === "one") { }
if (true === 1) { }

// Should NOT emit TS2367 (types overlap)
if (1 === 2) { }
if (x === y) { }  // where x and y could be same type
```

**Implementation Steps**:
1. ✅ Ask Gemini Question 1: What's the right approach for type overlap detection?
2. ⏭️ Implement `are_types_overlapping` in solver
3. ⏭️ Ask Gemini Question 2: Review the implementation
4. ⏭️ Integrate into checker's comparison expression handling
5. ⏭️ Add tests

---

### Task #16: Robust Optional Property Subtyping & Narrowing
**Status**: 🔄 In Progress (Implementation Phase)
**Priority**: High
**Estimated Impact**: +2-3% conformance
**Gemini Pro Question 2**: COMPLETED - Received implementation guidance

**Investigation Complete** ✅:
1. `narrow_by_discriminant` (line 491): ✅ CORRECT
2. `narrow_by_excluding_discriminant` (line 642): ✅ CORRECT
3. `resolve_type`: ✅ Handles Lazy and Application types
4. `optional_property_type` (objects.rs:662): ✅ CORRECT
5. `lookup_property` (objects.rs:21-34): ✅ CORRECT

**🚨 CRITICAL BUG**: Intersection property merging overwrites instead of intersects
**Location**: `src/solver/subtype.rs` lines 1064-1071
**Root Cause**: Calling `interner.intersection2()` creates infinite recursion
**Solution**: Use low-level `intersect_types_raw()` that bypasses normalization

---

## IMPLEMENTATION PLAN (Gemini Flash Redefined Session)

### Task 16.1: Low-level Intersection Infrastructure ⚡ CRITICAL
**File**: `src/solver/intern.rs`
**Estimate**: 30 minutes
**Action**: Implement `intersect_types_raw()` and `intersect_types_raw2()`
**Guidance**: `/tmp/intersect_types_raw_implementation.md` (complete code from Gemini Pro)
**Risk**: Low - straightforward implementation with exact specification

### Task 16.2: Property Collection Visitor
**File**: `src/solver/objects.rs`
**Estimate**: 1 hour
**Action**: Create `PropertyCollector` struct/visitor
**Logic**:
- Use `resolve_type` before inspecting TypeKey (fixes Lazy/Ref bug)
- Recursively walk Intersection members
- Collisions: `interner.intersect_types_raw2(type_a, type_b)`
- Flags: Required if ANY member required, Readonly if ANY member readonly
**Risk**: Medium - must handle recursive types carefully using cycle_stack

### Task 16.3: Judge (Subtype) Integration
**File**: `src/solver/subtype.rs`
**Estimate**: 1 hour
**Action**: Replace manual property loop (line 1064) with PropertyCollector call
**North Star Rule**: Judge asks Lawyer for effective property set
**Risk**: Low - direct replacement

### Task 16.4: Verification
**Files**: `tests/conformance/intersections/`
**Estimate**: 30 minutes
**Test Cases**:
1. Basic intersection merging → `never` type
2. Optionality merging → required wins
3. Discriminant narrowing with intersections
4. Deep intersection (stack overflow guard)
**Risk**: Low - tests already defined

---

## DEPENDENCIES
- Task 16.2 DEPENDS ON 16.1 (must have `intersect_types_raw` first)
- Task 16.3 DEPENDS ON 16.2 (must have PropertyCollector first)
- Follow Two-Question Rule: Ask Gemini Question 2 after Tasks 16.1 and 16.2

---

## NEXT IMMEDIATE ACTIONS (Per Gemini Redefinition)

1. ✅ Update session file with new priorities (DONE)
2. ⏭️ **Execute Task 16.1**: Implement `intersect_types_raw()` in `src/solver/intern.rs`
3. ⏭️ **Ask Gemini Question 2**: Review the intersection infrastructure implementation
4. ⏭️ **Execute Task 16.2**: Create PropertyCollector in `src/solver/objects.rs`
5. ⏭️ **Ask Gemini Question 2**: Review PropertyCollector implementation
6. ⏭️ **Execute Task 16.3**: Integrate into SubtypeChecker (the Judge)
7. ⏭️ **Move to Task #17** (TS2367) after Task #16 completion

**Critical Constraint**: Follow Two-Question Rule for ALL solver/checker changes
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
