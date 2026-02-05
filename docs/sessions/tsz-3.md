# Session tsz-3: Advanced CFA Features

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE
**Previous Session**: tsz-10 (CFA & Narrowing - Complete)

## Goal

Implement advanced Control Flow Analysis features to achieve 100% TypeScript parity.

## Progress

### Phase 1: Bidirectional Narrowing (✅ COMPLETE)

**Status**: ✅ IMPLEMENTED & TESTED

**Problem**: Implement narrowing for `x === y` where both are references.

**Solution Implemented**:
1. **Flow Context Architecture**: Modified `narrow_type_by_condition` and related functions to accept `antecedent_id` parameter, allowing access to flow-narrowed types of the "other" reference
2. **Bidirectional Narrowing Logic**: Enhanced `narrow_by_binary_expr` to handle `x === y` where both are references by:
   - Getting the flow type of the "other" reference using `get_flow_type`
   - Narrowing the target to the intersection of its type and the other's type
3. **Subtype Narrowing Fix**: Fixed `narrow_to_type` in `src/solver/narrowing.rs` to handle cases where target type is a subtype of a union member (e.g., narrowing `string | number` by `"hello"`)

**Files Modified**:
- `src/checker/control_flow.rs`:
  - Updated `check_flow` to pass `antecedent_id` to `narrow_type_by_condition`
  - Updated `narrow_type_by_condition` signature to accept `antecedent_id`
  - Updated `narrow_type_by_condition_inner` signature
  - Updated `narrow_by_logical_expr` signature
  - Updated `narrow_by_binary_expr` signature
  - Added call to `narrow_by_binary_expr` in binary expression handling path
  - Implemented bidirectional narrowing logic with flow type lookup
- `src/solver/narrowing.rs`:
  - Fixed `narrow_to_type` to check if target_type is a subtype of union member
  - Added `is_subtype_of_with_db` check for proper narrowing behavior

**Test Cases Verified**:
```typescript
// Test 1: Basic bidirectional narrowing (✅ WORKING)
function test1(x: string | number, y: string) {
    if (x === y) {
        x.toLowerCase(); // x correctly narrowed to string
    }
}

// Test 2: Error when incompatible types (✅ WORKING)
function test2(x: string | number, y: string) {
    if (x === y) {
        x.toFixed(); // Error: Property 'toFixed' does not exist on type 'string'
    }
}

// Test 3: Literal type narrowing (✅ WORKING)
function test4(x: string | number, y: string) {
    y = "hello";
    if (x === y) {
        x.toLowerCase(); // x correctly narrowed to "hello" (literal type)
    }
}
```

**Gemini Consultation**:
- Question 1: Asked about architectural approach for passing flow context
- Answer: Pass `antecedent_id` through call chain, use `get_flow_type` to query flow types
- Question 2: Asked about literal type narrowing edge case
- Answer: Fixed `narrow_to_type` to check `is_subtype_of(target_type, member)` for proper narrowing

---

### Phase 2: Assertion Functions (✅ COMPLETE)

**Status**: ✅ IMPLEMENTED & TESTED

**Problem**: Integration of `asserts x is T` with flow analysis for all subsequent code.

**Solution Implemented**:
1. **Treat CALL nodes as merge points**: Modified `check_flow` to include `flow_flags::CALL` in merge point detection
2. **Dependency tracking**: CALL nodes now wait for their antecedents to be processed before `handle_call_iterative` is called
3. **Existing logic reused**: `handle_call_iterative` already had the core logic to detect assertion functions and apply narrowing

**How It Works**:
1. When a CALL node is encountered in flow analysis, it's treated as a merge point
2. The worklist algorithm ensures the antecedent (state before the call) is processed first
3. `handle_call_iterative`:
   - Gets the pre-call type from the antecedent's results
   - Checks if the callee is an assertion function via `predicate_signature_for_type`
   - If `asserts` is true and it targets our reference, applies narrowing via `apply_type_predicate_narrowing`
   - Returns the narrowed type
4. The narrowed type is cached in `results` and propagated to all subsequent statements

**Files Modified**:
- `src/checker/control_flow.rs`:
  - Added `is_call` to merge point detection (line ~417)
  - CALL nodes now wait for antecedents before processing

**Test Cases Verified**:
```typescript
function assertIsString(x: unknown): asserts x is string {
    if (typeof x !== "string") throw new Error();
}

// Test 1: Basic assertion (✅ WORKING)
function test1(x: unknown) {
    assertIsString(x);
    x.toLowerCase(); // x correctly narrowed to string
}

// Test 2: Error on incompatible assertion (✅ WORKING)
function test2(x: unknown) {
    assertIsString(x);
    assertIsNumber(x); // Error: x is never (string & number = never)
    x.toFixed(); // Error on never type
}
```

**Gemini Consultation**:
- Question: Asked about architectural approach for assertion functions
- Answer: Treat CALL nodes as merge points, use existing `handle_call_iterative` logic

---

### Phase 3: Nested Discriminants (� ACTIVE)

**Status**: 🟡 IN PROGRESS - IMPLEMENTATION

**Problem**: Support narrowing for nested discriminant paths like `action.payload.kind`.

**Current Limitation**: `discriminant_property_info` only returns the immediate parent property. Real-world Redux/Flux code uses nested discriminants.

**Implementation Plan**:
1. Modify `discriminant_property_info` to return `Vec<Atom>` (full property path)
2. Update `narrow_by_binary_expr` to pass the path to the solver
3. Update `src/solver/narrowing.rs` to recursively descend into types based on path
4. Handle optional properties in the path (e.g., `action.payload?.kind`)

**Critical Bugs to Avoid** (from previous attempt):
1. Reversed subtype check - must check `is_subtype_of(literal, property_type)`
2. Missing type resolution - must handle `Lazy`, `Ref`, `Intersection` types
3. Optional properties - must handle `{ prop?: "a" }` correctly

**Gemini Consultation**: Pending - asking about architecture before implementation

---

### Phase 4: Edge Cases (PENDING)

**Status**: ⏸️ NOT STARTED

Freshness, `0`/`""`, `any` narrowing fixes.

---

## Context from tsz-10

Session tsz-10 completed:
- ✅ Type guards (typeof, instanceof, discriminants, truthiness)
- ✅ Property access & assignment narrowing
- ✅ Exhaustiveness checking (fixed discriminant comparison bug)

See `docs/sessions/history/tsz-10.md` for details.

---

## Session Notes

This session continues the CFA work started in tsz-10. The core infrastructure is complete; these are advanced features needed for real-world TypeScript code.
