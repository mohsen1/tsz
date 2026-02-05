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

## Task Breakdown (Priority Order per Gemini Redefinition - 2025-02-05 POST-TS2416)

### Priority 1: Index Signature Structural Compatibility ✅ ALREADY IMPLEMENTED
**Status**: ✅ Complete (Discovered 2025-02-05)
**Why**: Core structural operation - discovered to be already fully implemented

**Discovery**:
Index signature checking logic is ALREADY FULLY IMPLEMENTED in `src/solver/subtype_rules/objects.rs`.

**Implemented Functions**:
- `check_string_index_compatibility` (lines 290-331)
- `check_number_index_compatibility` (lines 343-372)
- `check_object_with_index_subtype` (lines 383-431)
- `check_object_with_index_to_object` (lines 440-500)
- `check_object_to_indexed` (lines 635-655)
- `check_properties_against_index_signatures` (lines 575-627)
- `check_missing_property_against_index_signatures` (lines 508-568)

**All Features Present**:
- String indexer subtyping (covariant)
- Number indexer subtyping (covariant)
- Property-to-index compatibility checking
- Readonly constraint enforcement
- Numeric property name handling
- Method variance checking
- Property optionality handling

---

### Priority 1: Refined Object Overlap Detection (TS2367) ✅ COMPLETE
**Status**: ✅ Completed (2025-02-05)
**Why**: Most immediate "Judge" task - TS2367 was implemented as MVP with Known Gaps

**Description**:
Completed TS2367 overlap detection to properly handle object property overlap using PropertyCollector.

**Implementation Summary**:
Implemented `do_refined_object_overlap_check` in `src/solver/subtype.rs` (lines 895-987):
1. **Property-Based Overlap**: Uses PropertyCollector to extract all properties from both types
2. **Discriminant Detection**: Common property with disjoint literal types = no overlap
   - Example: `{ kind: "a" }` vs `{ kind: "b" }` = zero overlap
3. **Index Signature Handling**: Only checks REQUIRED properties against index signatures
   - Optional properties can be missing, so they don't conflict
   - Index signatures never cause disjointness (empty object satisfies all)
4. **Recursive Overlap**: Handles recursive types using cycle_stack

**Critical Bugs Fixed** (Found by Gemini Pro):
1. **Optional Properties vs Index Signatures**: Only check required properties
   - Bug: Was checking all properties (including optional) against index signatures
   - Fix: Added `!p_a.optional` check before comparing with index signature
   - Example: `{ a?: string }` and `{ [k: string]: number }` DO overlap

2. **Index Signature Overlap**: Removed index signature comparison checks
   - Bug: Was checking if index signatures overlap with each other
   - Fix: Removed all index signature overlap checks
   - Example: `{ [k: string]: string }` and `{ [k: string]: number }` DO overlap

**Files**: `src/solver/subtype.rs`

**Commit**: a496185e6

**Gemini Pro Review**: ✅ "Both fixes are correct and accurately reflect TypeScript's structural type system behavior."

---

### Priority 2: Structural Intersection/Union Simplification ⚠️ PARTIAL
**Status**: ⚠️ Partial Implementation (2025-02-05)
**Why**: Performance North Star requires O(1) type equality via canonical forms

**Implementation Summary**:
Implemented **literal-based** subtype reduction for union/intersection normalization.

**What Works**:
- Literal-to-literal checking (prevents "a" <: "b")
- Literal-to-primitive reduction (`string | "a"` → `string`)
- Reduction skipped for complex types (TypeParameters, Lazy, Applications)

**Known Limitations**:
- **Object reduction DISABLED** due to complexity with generic types
- 5 circular_extends tests failing - needs investigation
- Conservative approach prioritizes correctness over optimization

**Why Partial**:
Object reduction proved too complex for the interner layer:
- Shallow checks are either too restrictive (break valid cases) or too permissive (incorrect reductions)
- Generic types and type parameters require full resolution
- Architecture limitation: Interner cannot call evaluate/subtype without infinite recursion

**Design Decisions**:
1. Skip reduction when any member is TypeParameter, Lazy, or Application
2. Objects never marked as subtypes in shallow check
3. Literal reduction only (object reduction deferred to future work)

**Files**: `src/solver/intern.rs`

**Commit**: cae535d63

**Gemini Pro Review**: ✅ "solid, conservative implementation" (before object reduction attempts)

**Next Steps**:
- Object reduction needs SubtypeChecker layer integration (not Interner)
- ~~Investigate circular_extends test failures~~ **PRE-EXISTING ISSUE** (tests fail at commit a975a10bf before my work)
- Consider architectural changes to support deeper reduction

**Investigation Finding** (2025-02-05):
The 5 circular_extends test failures are **pre-existing issues**, NOT caused by Priority 2 implementation.
- Tests fail at commit `a975a10bf` (before structural simplification work)
- Tests fail at commit `cae535d63` (after structural simplification work)
- Conclusion: These tests were already broken; my literal-based reduction did NOT introduce regressions

---

### Priority 3: Weak Type Detection (TS2559) (The Lawyer)
**Status**: 📝 Planned
**Why**: High-ROI diagnostic that relies on PropertyCollector

**Description**:
Implement weak type detection - objects where all properties are optional.

**TypeScript Rule**:
You cannot assign a type to a weak type if they share no common properties.

**Implementation Goals**:
- Implement `is_weak_type` check
- Common property check for weak type assignment

**Files**: `src/solver/compat.rs`

---

## Legacy Task Breakdown (2025-02-05 PRE-TS2416)

### Priority 0: Task #16.0 - Verification of Task #16 ✅ COMPLETE
**Status**: ✅ Completed (2025-02-05)
**Why First**: tsz-4 (Lawyer) and tsz-2 (Interface) rely on Task #16 being correct. Any bugs here will cause "ghost failures" in their sessions.

**Completed Actions**:
1. ✅ Ran existing solver tests - all 24 object tests pass
2. ✅ Added comprehensive unit tests to `src/solver/objects.rs`:
   - `test_collect_properties_conflicting_property_types` - verifies type intersection
   - `test_collect_properties_optionality_merging` - verifies required wins
   - `test_collect_properties_readonly_cumulative` - verifies readonly is cumulative
   - `test_collect_properties_nested_intersections` - verifies flattening
3. ✅ Verified `merge_visibility` logic is correct (Private > Protected > Public)
4. ✅ Verified `found_any` commutative Any handling

**Known Issue Discovered**:
Objects that differ only in `visibility` are incorrectly interned as the same `ObjectShapeId`. This is a bug in the interning system, not in PropertyCollector. The PropertyCollector correctly handles visibility merging when given distinct objects.

**Commit**: a48b5f3eb

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
**Status**: 🚧 In Progress (Subtask 17.1 Complete, 17.2 Pending)
**Why**: Pure set-theory/structural logic - "Can these two sets ever have a non-empty intersection?"

**Gemini Redefinition** (Flash 2025-02-05):
> "This is the perfect next step. It is a pure 'Judge' operation:
> 'Can these two sets ever have a non-empty intersection?'"

**Completed Subtask 17.1 (Solver)**: ✅ Implemented `are_types_overlapping(a, b)` in `src/solver/subtype.rs`
- MVP approach: Catches OBVIOUS non-overlaps
  - Different primitives (string vs number)
  - Different literals of same primitive ("a" vs "b")
  - Object property type mismatches ({ a: string } vs { a: number })
- Handles special cases:
  - strictNullChecks configuration
  - void/undefined always overlap
  - object keyword vs primitives
- Conservative default: Returns true for complex types not yet handled
- Helper functions:
  - `are_types_in_subtype_relation()` - literal-to-primitive checks
  - `are_literals_overlapping()` - literal value comparison
  - `do_object_properties_overlap()` - property intersection checking
- Followed Two-Question Rule:
  - Question 1: Asked Gemini Flash for implementation approach
  - Question 2: Asked Gemini Pro to review implementation
  - Fixed 3 critical bugs identified by Gemini Pro:
    1. Missing strictNullChecks handling (null/undefined overlap in non-strict mode)
    2. Missing void/undefined overlap (they always overlap)
    3. Incorrect primitive type matching (needed object keyword case)
- Added comprehensive test suite: 12 tests covering all overlap scenarios
- Commit: 15d8c93d9

**Subtask 17.2 (Checker)**: ✅ Completed (2025-02-05)

**Implementation Summary**:
1. Added TS2367 diagnostic code and message to `src/checker/types/diagnostics.rs`
2. Implemented `error_comparison_no_overlap()` in `src/checker/error_reporter.rs`
   - Operator-aware messages (always 'false' for ===, 'true' for !==)
   - Suppresses errors for any/unknown/error types
3. Implemented `are_types_overlapping()` wrapper in `src/checker/assignability_checker.rs`
   - Calls solver with `ensure_refs_resolved` for correctness
   - Passes `strict_null_checks` flag to solver
4. Added overlap check in `src/checker/type_computation.rs`
   - Checks all equality operators (===, !==, ==, !=)
   - Emits TS2367 when types don't overlap

**Test Results**:
✅ Detects number vs string non-overlap
✅ Detects boolean vs number non-overlap
✅ Operator-aware messages (=== vs !==)
✅ Suppresses errors for any/unknown types

**Known Gaps** (Follow-up work):
- Literal type overlap (1 vs 2, "a" vs "b") - literals may be widened
- Object type property overlap - needs PropertyCollector integration

**Commit**: b0b4476ed

**Gemini Guidance (Flash 2025-02-05)**:
Gemini provided complete implementation plan for integrating TS2367 into checker:

**Changes Required**:

1. **`src/checker/assignability_checker.rs`** - Add wrapper method:
```rust
pub fn are_types_overlapping(&mut self, source: TypeId, target: TypeId) -> bool {
    // Fast path: identity
    if source == target { return true; }

    // Ensure Refs are resolved (Critical for correct overlap check)
    self.ensure_refs_resolved(source);
    self.ensure_refs_resolved(target);

    let env = self.ctx.type_env.borrow();
    let mut checker = crate::solver::SubtypeChecker::with_resolver(self.ctx.types, &*env)
        .with_strict_null_checks(self.ctx.strict_null_checks());

    checker.are_types_overlapping(source, target)
}
```

2. **`src/checker/error_reporter.rs`** - Add diagnostic method:
```rust
pub fn error_comparison_no_overlap(&mut self, left: TypeId, right: TypeId, idx: NodeIndex) {
    // Suppress if either side is error/any/unknown to avoid noise
    if left.is_intrinsic_any_or_error() || right.is_intrinsic_any_or_error() { return; }

    let left_str = self.format_type(left);
    let right_str = self.format_type(right);

    let message = format_message(
        diagnostic_messages::TYPES_HAVE_NO_OVERLAP,
        &[&left_str, &right_str],
    );

    self.error_at_node(idx, &message, diagnostic_codes::TYPES_HAVE_NO_OVERLAP);
}
```

3. **`src/checker/type_computation.rs`** - Add check in `get_type_of_binary_expression`:
```rust
let is_equality = matches!(op_kind,
    k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 ||
         k == SyntaxKind::ExclamationEqualsEqualsToken as u16 ||
         k == SyntaxKind::EqualsEqualsToken as u16 ||
         k == SyntaxKind::ExclamationEqualsToken as u16
);

if is_equality && !self.are_types_overlapping(left_type, right_type) {
    self.error_comparison_no_overlap(left_type, right_type, node_idx);
}
```

**Edge Cases to Handle**:
- `any` / `unknown`: Should suppress error (handled by solver)
- Enums: Numeric enums overlap with `number`
- Null/Undefined: Respect `strict_null_checks` setting

**Must Follow**:
- Two-Question Rule for checker integration
- Must NOT inspect TypeKey in Checker (tsz-2 constraint)

**Example**:
```typescript
// Should emit TS2367
if (1 === "one") { }  // number & string have no overlap
```
if (true === 1) { }   // boolean & number have no overlap

// Should NOT emit TS2367
if (1 === 2) { }       // both number, overlap possible
```

---

### Priority 2: Task #18 - Structural Intersection Normalization ✅ COMPLETE
**Status**: ✅ Completed (2025-02-05)
**Why**: High-impact. Fixes foundational issues that affect TS2367 overlap detection.

**Gemini Recommendation**: Proceed with Task #18 instead of fixing Task #17 gaps directly.
- Foundational Fix: `are_types_overlapping` relies on proper intersection normalization
- Fixes the Interning Bug discovered in Task #16.0
- Completes the "What" (Solver defines what `A & B` is)

**Completed Implementation**:

1. **Fixed Visibility Merging** (`src/solver/intern.rs` lines 1334-1342):
   - Implements "most restrictive wins" rule: Private > Protected > Public
   - Pattern matching order is critical (Private must be first)
   - Verified by Gemini Pro ✅

2. **Verified Disjoint Literals**:
   - `intersection_has_disjoint_primitives` correctly handles 1 & 2
   - Type interning ensures different literals have unique TypeIds
   - Direct equality check identifies disjoint literals

3. **Added 4 Solver Unit Tests** (`src/solver/tests/intern_tests.rs`):
   - test_intersection_visibility_merging: private & public = private ✅
   - test_intersection_disjoint_literals: 1 & 2 = NEVER ✅
   - test_intersection_object_merging: {a:1} & {b:2} = {a:1, b:2} ✅
   - test_intersection_disjoint_property_types: {a:1} & {a:2} = NEVER ✅

**Commit**: `206acc76c`

**Gemini Pro Review**: "Verdict: ✅ Correct" - Implementation matches TypeScript behavior

**Impact**: Fixes foundational intersection normalization that affects TS2367 overlap detection and object interning.

---

### Priority 3: TS2416 - Signature Override Mismatch ✅ COMPLETE
**Status**: ✅ Completed (2025-02-05)
**Why**: Critical for class hierarchy and interface implementation tests

**Implementation Summary**:

TS2416 (Property '{0}' in type '{1}' is not assignable to the same property in base type '{2}')
now correctly distinguishes between methods (bivariant) and function properties (contravariant).

**Key Changes**:

1. **src/checker/assignability_checker.rs** - Added `is_assignable_to_bivariant`:
   - Calls `CompatChecker.is_assignable_to_bivariant_callback`
   - Disables strict_function_types for method override checking
   - Follows same pattern as `is_assignable_to` with ref resolution

2. **src/checker/class_checker.rs** - Updated `check_property_inheritance_compatibility`:
   - Handle METHOD_DECLARATION nodes (previously missing)
   - Create function types with `is_method: true` for methods
   - Track `is_method` flag for variance selection
   - Track `is_static` flag for static/instance compatibility
   - Handle SET_ACCESSOR for derived and base members
   - Skip private base members (they trigger different errors)
   - Use `is_assignable_to_bivariant` for methods (bivariant)
   - Use `is_assignable_to` for properties/accessors (contravariant with strictFunctionTypes)

**Bug Fixes from Gemini Pro Review**:

1. **Static members**: Fixed by tracking `is_static` in tuple and checking `is_static == base_is_static`
   - Static members are now checked (not skipped)
   - Ensures static only overrides static, instance only instance

2. **Private base members**: Fixed by adding `has_private_modifier` check
   - Private base members are skipped (they trigger different errors, not TS2416)
   - Protected members still participate in TS2416 checks

3. **SET_ACCESSOR**: Added complete handling in both derived and base members
   - Extracts type from first parameter in `accessor.parameters.nodes`
   - Uses parameter type (not return type) for setters

**Gemini Pro Review**: "The code is ready to be committed. It correctly implements the logic required to fix the identified bugs."

**Commit**: `cd01b467e`

**Impact**: TS2416 now correctly handles method vs property variance, static members, and accessors.

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

---

### Task #26: Union/Intersection Simplification Infrastructure ✅ COMPLETE (2025-02-05)
**Status**: ✅ Infrastructure Complete, Simplification Disabled (2025-02-05)
**Why**: Performance optimization to reduce type bloat through structural simplification

**Implementation Summary**:
Built infrastructure for union/intersection simplification in TypeEvaluator with SubtypeChecker integration.

**Changes Made**:
1. **`bypass_evaluation` flag** in `SubtypeChecker` (src/solver/subtype.rs)
   - Skips evaluate_type() calls to prevent mutual recursion
   - Safe for simplification (false negatives = no simplification, not incorrect types)

2. **`max_depth` field** in `SubtypeChecker` (src/solver/subtype.rs)
   - Configurable depth limit for subtype checking
   - Initialized to MAX_SUBTYPE_DEPTH (100) by default
   - Can be set lower (e.g., 10) for simplification to prevent stack overflow

3. **Disabled simplification methods** in `TypeEvaluator` (src/solver/evaluate.rs)
   - `simplify_union_members` and `simplify_intersection_members`
   - Currently DISABLED due to pre-existing stack overflow issue
   - Infrastructure is bug-free and ready for future use

**Pre-existing Issue Discovered**:
- `test_interface_extends_class_no_recursion_crash` overflows stack
- Verified this occurs WITHOUT my changes (reverted and tested)
- Root cause: recursive type structure in interface-extends-class scenario with **private properties**

**Further Investigation** (2025-02-05):
The test involves private properties (`#prop`) in a complex inheritance scenario:
```typescript
class C {
    #prop;  // private property
    func(x: I) { x.#prop = 123; }
}
interface I extends C {}  // interface extends class with private member
```

**Root Cause Hypothesis** (from Gemini Flash):
The issue is likely in `private_brand_assignability_override` in `src/solver/compat.rs` (lines 485–580).
- This function performs recursive calls for Union, Intersection, and Lazy types
- **Does NOT have a recursion guard** (unlike SubtypeChecker or ShapeExtractor)
- If the private property's type is recursive, this function will spin forever

**Resolution**:
This is a deep architectural issue requiring significant time to fix properly.
Recommendation: Defer and move to higher-ROI tasks (Weak Type Detection TS2559).

**Gemini Pro Review**:
- Infrastructure is **sound and bug-free**
- `bypass_evaluation`: Safe - false negatives just mean no simplification (semantically correct)
- `max_depth`: Safe - prevents stack overflow with configurable limit
- Ready for future use when non-recursive approach is implemented

**Files**: `src/solver/subtype.rs`, `src/solver/evaluate.rs`

**Commit**: a3533d7db

**Commit**: a3533d7db

---

### Task #27: Weak Type Detection (TS2559) ✅ ALREADY IMPLEMENTED (2025-02-05)
**Status**: ✅ Feature Already Complete (2025-02-05)
**Why**: High-ROI "Lawyer" rule for TypeScript assignability

**Investigation Findings**:
Weak type detection is ALREADY FULLY IMPLEMENTED in `src/solver/compat.rs`.

**Existing Implementation** (Lines 769-967):
- `violates_weak_type(source, target)`: Main entry point
- `violates_weak_union(source, target)`: Handles union targets
- `violates_weak_type_with_target_props`: Checks for common properties
- `has_common_property`: Property name overlap check (lines 940-962)
- `source_lacks_union_common_property`: Recursive union handling

**Coverage**:
✅ Index signatures preventing weakness
✅ Optional property checking
✅ Common property name checking
✅ Type parameter constraint resolution
✅ Union handling in source and target
✅ Empty object source handling
✅ ObjectWithIndex handling

**Test Status**: All 13 weak type tests PASS

**Additional Fix**:
Fixed compilation error in `src/solver/db.rs` where `PropertyAccessEvaluator::new_no_resolver` was called but the method is `::new`. This was from commit `5188f4d01` (another session).

**Commit**: 68f4c1fbb (db.rs fix)

---

**Next Session**:
- Ask Gemini for next high-priority task (skip Task #15)
- Focus on tasks with better ROI and clearer architectural path
- Continue following Two-Question Rule
