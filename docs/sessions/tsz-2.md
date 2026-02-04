# Session tsz-2: Type Narrowing & Control Flow Analysis

**Started**: 2026-02-04
**Status**: 🟢 Phase 3 Active - Task 9: typeof Narrowing (COMPLETE)
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
- 2026-02-04: COMPLETED Phase 1 (Tasks 1-4): Discriminant, Solver::narrow, Checker Integration, Truthiness
- 2026-02-04: COMPLETED Phase 2 Task 5: User-Defined Type Guards
- 2026-02-04: COMPLETED Phase 2 Task 6: Equality & Identity Narrowing
- 2026-02-04: **PHASE 2 COMPLETE**

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

### Task 3 Complete ✅

**Commits**:
- `9bf03b6b8` - "feat(tsz-2): integrate Solver::narrow with Checker using TypeGuard pattern"
- `b2691032a` - "fix(tsz-2): correct Instanceof TypeGuard handling per Gemini review"

**Changes Made**:

1. **src/checker/control_flow_narrowing.rs** - Expanded `extract_type_guard()`:
   - Added handling for `InstanceOfKeyword` operator
   - Extracts instance type from right side using `instance_type_from_constructor()`
   - Returns `TypeGuard::Instanceof(instance_type)`
   - Added handling for `InKeyword` operator
   - Extracts property name from left side using `in_property_name()`
   - Returns `TypeGuard::InProperty(prop_name)`
   - Fixed operator token comparison (is `u16`, not `NodeIndex`)

2. **src/checker/control_flow.rs** - Refactored `narrow_type_by_condition_inner()`:
   - Implemented "Extract then Delegate" pattern for binary expressions
   - Logical operators (`&&`, `||`) still use `narrow_by_logical_expr` (special recursion needed)
   - For other binary expressions:
     a. Call `extract_type_guard(condition_idx)` to get `(TypeGuard, guard_target)`
     b. Check if `is_matching_reference(guard_target, target)`
     c. If match: return `narrowing.narrow_type(type_id, &guard, is_true_branch)`
     d. If no match: return `type_id` unchanged

3. **src/solver/narrowing.rs** - Fixed `TypeGuard::Instanceof` handler:
   - **Critical Bug Fix**: Treat payload as Instance Type, not Constructor Type
   - Use `narrow_to_type()` directly instead of `narrow_by_instanceof()`
   - Added intersection fallback for interface vs class cases

**Solver-First Architecture Achieved**:
- **Checker**: Identifies WHERE narrowing happens (AST node) and WHAT the condition is (TypeGuard)
- **Solver**: Calculates the RESULT (narrowed type) via `NarrowingContext::narrow_type()`

**Critical Bug Fixed** (via Gemini Code Review):
- **Instanceof Type Mismatch**: Checker was passing Instance Type, but Solver expected Constructor Type
- Fixed by changing Solver to treat `TypeGuard::Instanceof` payload as Instance Type
- Use `narrow_to_type()` directly instead of trying to extract instance type again

**Gemini Guidance**:
- Question 1 (PRE): Approach validation - extract TypeGuard, then delegate to Solver
- Question 2 (POST): Implementation review - found Instanceof type mismatch bug, fixed

**Next**: Task 4 - Truthiness Narrowing

---

### Task 4 Complete ✅

**Commits**:
- `5b7d3b159` - "feat(tsz-2): implement Task 4 - Truthiness Narrowing with TypeGuard"
- `79a812ccb` - "fix(tsz-2): add NaN and void to falsy narrowing per Gemini review"

**Changes Made**:

1. **src/solver/narrowing.rs** - Added falsy narrowing support:
   - Added `narrow_to_falsy()` method to `NarrowingContext`
   - Narrows types to falsy components (string → "", number → 0 | NaN, boolean → false)
   - Handles intrinsics: null, undefined, void, boolean, string, number, bigint

2. **src/solver/narrowing.rs** - Added helper methods:
   - `falsy_component()`: Returns falsy representation of a type
   - `literal_is_falsy()`: Checks if a literal value is falsy

3. **src/solver/narrowing.rs** - Updated `TypeGuard::Truthy` handler:
   - True branch: uses `narrow_by_truthiness()` (existing)
   - False branch: uses `narrow_to_falsy()` (NEW) - fixes critical bug

4. **src/checker/control_flow.rs** - Refactored truthiness handling:
   - Added `TypeGuard` to imports
   - Property access case: now uses `TypeGuard::Truthy`
   - Default/fallback case: now uses `TypeGuard::Truthy`
   - Removed manual narrowing logic
   - Delegates to Solver via `narrowing.narrow_type()`

**Critical Bugs Fixed** (via Gemini Code Review):
1. **Missing NaN for number narrowing**
   - TypeScript treats NaN as falsy
   - Number type must narrow to `0 | NaN` in false branch
   - Updated `falsy_component()` for NUMBER to return union of 0 and NaN
   - Updated `narrow_to_falsy()` for UNKNOWN to include NaN

2. **Missing void handling**
   - void is a falsy type (effectively undefined at runtime)
   - `falsy_component()` now handles VOID alongside NULL and UNDEFINED

3. **False branch was returning source_type unchanged**
   - Was: `sense = false` → return `source_type`
   - Fixed: `sense = false` → return `narrow_to_falsy(source_type)`

**Solver-First Architecture**:
- Checker identifies truthiness context (WHERE + WHAT)
- Solver calculates narrowed type (RESULT)

**Gemini Guidance**:
- Question 1 (PRE): Approach validation - use TypeGuard::Truthy, implement narrow_to_falsy
- Question 2 (POST): Implementation review - found 3 critical issues, all fixed

**Session Status**: 🟡 PHASE 1 COMPLETE

---

## PHASE 2: Advanced Narrowing & Definite Assignment Analysis

**Status**: 🔄 PENDING (Proposed by Gemini consultation)

### Gemini Assessment (2026-02-04)

Phase 1 has successfully completed the core infrastructure for narrowing. The Solver-First architecture is in place and the most common cases are handled (Discriminants, Truthiness, instanceof, in).

However, to achieve the goal of matching `tsc` behavior exactly, additional narrowing features are needed.

### Remaining Narrowing Features (The "Gaps")

1. **User-Defined Type Guards (`is` predicates)**:
   - Functions with `x is T` return types don't currently narrow in caller's scope
   - Need to extract `TypePredicate` from function symbols and apply at call sites

2. **Equality Narrowing**:
   - `if (x === "foo")` or `if (x === y)` need special handling
   - Requires identity narrowing between variables and literals

3. **Assignment Narrowing**:
   - `let x: string | number; x = "hello"; x;` should narrow to `string`
   - CFA needs to track assignments and update flow types

4. **Array.isArray Narrowing**:
   - Special-casing the built-in `Array.isArray` call

5. **Literal/Template Literal Narrowing**:
   - Narrowing `string` to specific template literals

### Proposed Phase 2 Tasks

#### Task 5: User-Defined Type Guards (HIGH PRIORITY)

**Goal**: Implement support for `x is T` and `asserts x is T` predicates.

**Implementation**:
- Recognize calls to functions with `TypePredicate` returns
- Extract `TypeGuard::Predicate(TypeId)` from function symbol
- Apply narrowing at call site in caller's scope

**Why High Priority**: Essential for user-defined type guards, a common TypeScript pattern

**Files to modify**:
- `src/checker/control_flow.rs`: Extract type predicates from call expressions
- `src/solver/narrowing.rs`: Add `TypeGuard::Predicate` variant
- `src/binder/`: Track type predicate information

#### Task 6: Equality & Identity Narrowing (HIGH PRIORITY) - 🔄 IN PROGRESS

**Goal**: Handle `===` and `!==` between variables and literals.

**Status**: 🔄 IN PROGRESS (Started 2026-02-04)

**Implementation Plan (per Gemini Question 1 validation):**

1. **Loose Equality Support**:
   - Add handling for `==` and `!=` operators
   - `x == null` and `x == undefined` are treated identically
   - Use existing `TypeGuard::NullishEquality`

2. **Bidirectional Narrowing**:
   - If both sides are references (`x === y`): narrow both directions
   - Use caller-side iteration in `FlowAnalyzer`
   - Keep `extract_type_guard` as a query for a single node

3. **Verify Strict Equality**:
   - Ensure `===` and `!==` with literals work correctly
   - Check proper NaN handling
   - Handle `any`, enum members, object identity

**Edge Cases**:
- `any`: Narrowing `any === string` → `string` (true), `any` (false)
- NaN: `x === NaN` is valid type-level operation, always false at runtime
- Enums: Use specific enum member type, not just base number
- Objects: `x !== y` does NOT narrow (different objects can have same type)

**Files to modify**:
- `src/checker/control_flow_narrowing.rs`: Add loose equality support
- `src/checker/control_flow.rs`: Implement bidirectional narrowing
- `src/solver/narrowing.rs`: Verify `LiteralEquality` handling

**Gemini Guidance**:
- Question 1 (PRE): Approach validated - use caller-side iteration for bidirectional
- Question 2 (POST): Pending - after implementation

#### Task 7: Assignment Narrowing (MEDIUM PRIORITY)

**Goal**: Update flow types on variable reassignment.

**Implementation**:
- Track assignments in CFA graph as flow nodes
- Update current flow type based on assigned expression type
- Handle block-scoped variables correctly

**Why Medium Priority**: Important for completeness but more complex

**Files to modify**:
- `src/binder/`: Track assignments in flow graph
- `src/checker/control_flow.rs`: Query assignment flow nodes

#### Task 8: Definite Assignment Analysis (MEDIUM PRIORITY)

**Goal**: Use CFA graph to detect "Variable used before being assigned" (TS2454).

**Implementation**:
- Analyze flow graph to detect uninitialized variable usage
- Report definite assignment errors
- Handle type guards that imply definite assignment

**Why Medium Priority**: Important for type safety but can be added incrementally

**Files to modify**:
- `src/checker/`: Add definite assignment checker
- `src/binder/`: Track initialization state

#### Task 9: typeof Narrowing (HIGH PRIORITY - Added 2026-02-04)

**Goal**: Implement `typeof` narrowing (e.g., `if (typeof x === "string")` narrows to `string`).

**Implementation**:
- Recognize `typeof` expressions in binary comparisons
- Add `TypeGuard::Typeof(String)` to Solver
- Implement filtering logic to handle typeof strings ("string", "number", "object", etc.)
- Handle special case: "object" includes objects, arrays, null, etc.

**Why High Priority**: Fundamental building block used frequently in TypeScript. Without it, CFA feels "broken" to users.

**Files to modify**:
- `src/checker/control_flow_narrowing.rs`: Recognize typeof in `extract_type_guard`
- `src/solver/narrowing.rs`: Add TypeGuard::Typeof and filtering logic

**Edge Cases**:
- `typeof null === "object"` (TypeScript quirk)
- `typeof` on functions returns "function"
- Handle `any` (narrows to specific type) and `unknown`

#### Task 10: Array.isArray Narrowing (MEDIUM PRIORITY)

**Goal**: Implement `Array.isArray()` narrowing (e.g., `if (Array.isArray(x))` narrows to `any[]`).

**Implementation**:
- Recognize `Array.isArray()` call pattern
- Add type guard for array check
- Narrow to array type in true branch

**Why Medium Priority**: Important pattern but less common than typeof

**Files to modify**:
- `src/checker/control_flow_narrowing.rs`: Recognize Array.isArray pattern
- `src/solver/narrowing.rs`: Add array type guard handling

### Coordination Notes

**With tsz-1 (Core Solver)**:
- tsz-1 works on core solver correctness
- tsz-2 Phase 2 will continue working on narrowing features
- Coordination: Ensure type predicates integrate correctly with solver

**Avoid Conflicts**:
- tsz-3 (CFA) - COMPLETE, no conflicts
- tsz-4 (Declaration Emit) - COMPLETE, no conflicts
- tsz-5 (Import/Export Elision) - ACTIVE, minimal overlap

### Session History

- 2026-02-04: Started as "Intersection Reduction and Advanced Type Operations"
- 2026-02-04: COMPLETED BCT, Intersection Reduction, Literal Widening
- 2026-02-04: COMPLETED Phase 1: Nominal Subtyping (all 4 tasks)
- 2026-02-04: INVESTIGATED Cache Isolation Bug
- 2026-02-04: COMPLETE Cache Isolation Bug investigation
- 2026-02-04: REDEFINED to Lawyer-Layer Cache Partitioning & Type Cache Unification
- 2026-02-04: COMPLETED Task 1-2: Cache Partitioning & CheckerState Refactoring
- 2026-02-04: **REDEFINED** to Type Narrowing & Control Flow Analysis
- 2026-02-04: **PHASE 1 COMPLETE** - Tasks 1-4: Discriminant, Solver::narrow, Integration, Truthiness
- 2026-02-04: **PHASE 2 COMPLETE** - Tasks 5-6: User-Defined Type Guards, Equality & Identity Narrowing
- 2026-02-04: ALL HIGH-PRIORITY NARROWING FEATURES IMPLEMENTED

### Task 5 Complete ✅

**Commit**: `58f9bbec9` - "feat(tsz-2): refactor extract_type_guard for CallExpression"

**Changes Made**:
1. Refactored `extract_type_guard` to handle CallExpression nodes
   - Updated signature to return `Option<(TypeGuard, NodeIndex, bool)>` where bool is `is_optional`
   - Added CALL_EXPRESSION check before binary expression check

2. Implemented `extract_call_type_guard` helper method:
   - Checks for optional chaining via `node_flags::OPTIONAL_CHAIN`
   - Handles both `obj?.method()` and `func?.()` patterns
   - Resolves callee type using `skip_parens_and_assertions`
   - Returns `TypeGuard::Predicate` for "x is T" or `TypeGuard::Truthy` for "asserts x"

3. Added CALL_EXPRESSION case to `narrow_type_by_condition_inner`:
   - Skips narrowing on false branch when `is_optional=true` (TypeScript behavior)
   - Delegates to Solver via `narrow_type`

4. Fixed discriminant comparison to propagate `is_optional` flag
5. Fixed asserts flag dereference bug in narrowing.rs

**Gemini Guidance**: Followed Two-Question Rule (Question 1: Approach Validation, Question 2: Implementation Review)

**Bugs Found & Fixed**:
1. **Missing optional call detection**: Added `node_flags::OPTIONAL_CHAIN` check to catch `func?.()` pattern
2. **Hardcoded is_optional in discriminant**: Changed to propagate actual `is_optional` value from `discriminant_comparison`

### Task 6 Complete ✅

**Commit**: `f2d4ae5d5` - "feat(tsz-2): implement equality & identity narrowing"

**Changes Made**:
1. Added loose equality support for `==` and `!=` operators
2. Implemented bidirectional narrowing for `x === y` where both are references
3. Added `is_unit_type()` helper that recursively checks unions

**Critical Bug Fixed** (via Gemini Code Review):
- **is_unit_type too restrictive**: Made it recursively check union members - all members must be unit types
- Before: Returned false for unions, preventing narrowing like `x !== y` where y: "A" | "B"
- After: Correctly handles unions of unit types

### Task 9 Complete ✅

**Commit**: `36a76bc0d` - "fix(tsz-2): fix typeof narrowing for 'object' case"

**Discovery**: typeof narrowing infrastructure was already partially implemented, but had critical bugs in the "object" case.

**Changes Made**:
1. Fixed "object" case to include `null`
   - Before: `"object" => TypeId::OBJECT` (comment claimed it included null, but didn't)
   - After: `"object" => self.db.union2(TypeId::OBJECT, TypeId::NULL)`
   - Reason: `typeof null === "object"` in JavaScript

2. Fixed "object" case to exclude functions
   - Functions are a subtype of Object, so `narrow_to_type` kept them
   - But `typeof function === "function"`, not "object"
   - Added explicit `self.narrow_excluding_function(narrowed)` call

3. Removed duplicate function definitions
   - Removed duplicate `narrow_to_falsy`, `falsy_component`, `literal_is_falsy`
   - Lines 1901-2015 were duplicates of the correct implementations at lines 1751+

**Gemini Guidance**: Followed Two-Question Rule
- Question 1: Asked about approach - discovered typeof was already implemented
- Question 2: Gemini found both bugs (missing null, missing function exclusion)

**Critical Bug Fixed**:
- The comment on line 642 said "// includes null" but the code was `TypeId::OBJECT`
- For `UNKNOWN` it was correct, but for regular types it was wrong
- This would have caused incorrect narrowing in real-world TypeScript code


---
