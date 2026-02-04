# Session tsz-3: Generic Inference & Contextual Typing Integration

**Started**: 2026-02-04
**Status**: 🟢 ACTIVE (Phase 5: Contextual Typing)
**Latest Update**: 2026-02-04 - Continuing with Contextual Typing (Phase 5)
**Focus**: Bidirectional Type Inference for Function Expressions

---

## Current Phase: Contextual Typing & Bidirectional Inference (ACTIVE)

**Started**: 2026-02-04
**Status**: Foundation Complete (Task 1.1 ✅, working on Task 1.2)

### Problem Statement

Currently, tsz only implements "upward" inference (from arguments to return type). This fails to handle common TypeScript patterns requiring "downward" inference:

```typescript
// Without contextual typing, 'x' is inferred as 'any' or 'unknown'
// With contextual typing, 'x' should be inferred as 'string' from the target type
const f: (x: string) => void = (x) => { console.log(x); };

// Array methods should use contextual typing
const nums = [1, 2, 3];
const doubled = nums.map(x => x * 2); // 'x' should be 'number' from array type
```

### Foundation ✅ COMPLETE

**Implemented:**
- `get_contextual_signature()` in `src/solver/operations.rs` - Uses TypeVisitor pattern
- Handles `FunctionShape`, `CallableShape`, and `Application` types
- `visit_ref` - Critical fix for named types
- `Contextual` priority added to `InferencePriority` enum

**Commits:**
- `35cb275b0` - Redefined focus to Generic Contextual Inference
- `f390ccbf2` - Implemented visit_application for generic types
- `2d2050763` - Fixed visit_ref (CRITICAL BUG)
- `f669259f5` - Implemented priority-based contextual inference constraints
- `9b629b157` - Wired up check_with_context API in ExpressionChecker

### Current Tasks: In Progress

**Task 1.2: Seed InferenceContext with Contextual Constraints (HIGH)** ✅ **COMPLETE**
- **File**: `src/solver/operations.rs`
- **Goal**: Allow `InferenceContext` to accept external contextual hints
- **Implementation**:
  - Added `InferencePriority` parameter to `constrain_types` and all helper functions
  - Moved contextual seeding to BEFORE argument processing (step 2.5)
  - Use `InferencePriority::Contextual` for contextual hints (lower priority)
  - Use `InferencePriority::Argument` for argument constraints (higher priority)

**Task 1.3: Wire up `check_with_context` in ExpressionChecker (MEDIUM)** ✅ **COMPLETE**
- **File**: `src/checker/expr.rs`
- **Goal**: Propagate contextual types down to function expressions
- **Implementation**:
  - Added `check_with_context(idx, context_type)` method
  - Updated `check()` to delegate to `check_with_context(context_type: None)`
  - Updated `compute_type_impl()` to accept context_type parameter
  - Bypass cache when contextual type is provided (prevents incorrect results)
  - Pass context through to parenthesized expressions

### Next Tasks:
- **File**: `src/checker/expr.rs`
- **Goal**: Propagate contextual types down to function expressions
- **Implementation**:
  - Add `check_with_context(idx, context_type)` method
  - Detect when expressions have contextual types
  - Call `get_contextual_signature()` to infer parameters

**Known Limitations:**
- Union contextual typing not yet implemented
- Intersection contextual typing not yet implemented
- Both can be added in future tasks

---

## Session History: Phase 4 - Generic Inference COMPLETE ✅

**Completed**: 2026-02-04

### Prioritized Tasks (Phase 4)

#### Task 1: Nominal BCT Bridge (Binder-Solver Link) (HIGH) ✅ COMPLETE
**Commits**: `bfcf9a683`, `d5d951612`
**Status**: Complete with deferred limitation
**Limitation**: Uses `is_subtype_of` without resolver. Nominal inheritance checks may fail for class hierarchies without structural similarity.

#### Task 2: Homomorphic Mapped Type Preservation (HIGH) ✅ COMPLETE
**Commit**: `5cc8b37e0`
**File**: `src/solver/evaluate_rules/mapped.rs`
**Description**: Implemented preservation of Array/Tuple/ReadonlyArray structure in mapped types.

#### Task 3: Inter-Parameter Constraint Propagation (MEDIUM) ✅ COMPLETE
**File**: `src/solver/infer.rs`

**Goal**: Implement `strengthen_constraints` for fixed-point iteration over type parameter bounds.

**Implementation Plan**:
1. Identify type parameters that reference other type parameters in their bounds (e.g., `T extends U`).
2. Propagate constraints: If `T` is constrained to `S`, and `T extends U`, then `S` should contribute to `U`'s constraints.
3. Implement iterative strengthening until a fixed point is reached or recursion limit is hit.

#### Task 1.1: Fix Nominal BCT Resolver (Refactor SubtypeChecker) (MEDIUM) ✅ COMPLETE
**Commits**: `52060cf9b`
**File**: `src/solver/subtype.rs`
**Goal**: Allow `SubtypeChecker` to accept `dyn TypeResolver` (unsized) to support nominal hierarchy checks in BCT.

#### Task 4: Contextual Return Inference (LOW) ⏸️ DEFERRED
**File**: `src/solver/operations.rs`
**Goal**: Refine `resolve_generic_call` to collect constraints from `contextual_type` before resolving.

**Status**: Deferred and continued in Phase 5 (Contextual Typing) below.

---

## Session History: Previous Phases COMPLETE ✅

### Phase 1: Solver-First Narrowing & Discriminant Hardening (COMPLETE)

**Completed**: 2026-02-04
- Task 1: Discriminant Subtype Direction ✅
- Task 2-3: Lazy/Intersection Type Resolution ✅ (commit `5b0d2ee52`)
- Task 4: Harden `in` Operator Narrowing ✅ (commit `bc80dd0fa`)
- Task 5: Truthiness Narrowing for Literals ✅ (commit `97753bfef`)
- Priority 1: instanceof Narrowing ✅ (commit `0aec78d51`)

**Achievement**: Implemented comprehensive narrowing hardening for the Solver.

### Phase 2: User-Defined Type Guards (COMPLETE)

**Completed**: 2026-02-04

#### Priority 2a: Assertion Guard CFA Integration ✅
**Commit**: `58061e588`

Implemented assertion guards (`asserts x is T` and `asserts x`) with:
- Truthiness narrowing via TypeGuard::Truthy
- Intersection type support
- Union type safety (all members must have compatible predicates)
- Optional chaining checks

#### Priority 2b: is Type Predicate Narrowing ✅
**Commit**: `619c3f279`

Implemented boolean predicates (`x is T`) with:
- Optional chaining fix (true branch only)
- Overload handling (heuristic with safety TODO)
- this target extraction (skip parens/assertions)
- Negation narrowing (exclude predicate type)

**Achievement**: User-defined type guards fully implemented, matching tsc behavior for assertion and is predicates.

---

## Phase 3: CFA Hardening & Loop Refinement (COMPLETE ✅)

**Started**: 2026-02-04
**Completed**: 2026-02-04
**Status**: ALL TASKS COMPLETE ✅

### Problem Statement

The current CFA implementation was **too conservative** regarding loops and closures compared to tsc:

1. **Loop Widening**: Currently resets ALL let/var variables to declared type at loop headers, even if they're never mutated in the loop body.
2. **Switch Fallthrough**: May not correctly union narrowed types from multiple case antecedents.
3. **Falsy Completeness**: Need to verify NaN, 0n, and other edge cases match tsc exactly.
4. **Cache Safety**: Flow analysis cache may return stale results across different generic instantiations.

### Completed Tasks

#### Task 4.1: Loop Mutation Analysis (HIGH) ✅ COMPLETE
**Commit**: `027d55f1a`

**Goal**: Only widen let/var at LOOP_LABEL if the variable is mutated in the loop body.

**Implementation**:
- Created `is_symbol_mutated_in_loop()` with backward traversal from back-edges
- Created `node_mutates_symbol()` to check assignment nodes
- Created `assignment_targets_symbol()` for SymbolId-aware assignment checking
- Added `loop_mutation_cache` to prevent O(N^2) complexity
- Modified LOOP_LABEL handling to check mutations before widening

**Critical Fix (Gemini Pro Review)**:
- REMOVED array mutation check - Array methods like push() don't reassign variable
- CFA tracks variable reassignments, not object content mutations

**Impact**: Significant improvement in narrowing precision for patterns where variables are narrowed before a loop but never reassigned inside.

#### Task 4.2: Switch Union Aggregation (MEDIUM) ✅ COMPLETE
**Commit**: `c6c9af77f`

**Goal**: Fix check_flow to correctly union types from multiple SWITCH_CLAUSE antecedents during fallthrough.

**Implementation**:
- Fixed `antecedents_to_check` to include ALL antecedents (switch header + fallthrough clauses)
- Removed redundant worklist code from `handle_switch_clause_iterative`
- Fixed critical regression: added antecedent scheduling for non-fallthrough cases

**Critical Fix (Gemini Pro Review)**:
- Fixed regression where worklist scheduling was removed but not replaced
- Added antecedent scheduling to prevent flow analysis from stopping prematurely

**Impact**: Correct type narrowing for switch fallthrough patterns with multiple case clauses.

#### Task 4.3: Falsy Value Completeness (MEDIUM) ✅ COMPLETE
**Commit**: `0950e7031`

**Goal**: Ensure NaN, 0n (BigInt), and all falsy primitives are correctly narrowed.

**Implementation**:
- Added `narrow_to_falsy` to Solver (src/solver/narrowing.rs:1696)
- Updated Checker to delegate to Solver (3 call sites updated)
- Handles NaN correctly (typeof 'number' but falsy)

**Critical Finding (Gemini Pro Review)**:
- TypeScript does NOT narrow primitive types in falsy branches
- `boolean` stays as `boolean`, NOT narrowed to `false`
- `number` stays as `number`, NOT narrowed to `0 | NaN`
- `string` stays as `string`, NOT narrowed to `""`
- `unknown` stays as `unknown`, NOT narrowed to falsy union
- Only literal types are narrowed (e.g., `true | false` -> `false`)

**Impact**: Matches tsc behavior exactly for falsy narrowing.

#### Task 4.4: CFA Cache Safety (LOW) ✅ COMPLETE
**Commit**: `2e2b253be`

**Goal**: Audit flow_analysis_cache to ensure no stale results across generic instantiations.

**Implementation**:
- Identified cache safety issue: key was (FlowNodeId, SymbolId, InitialTypeId) without TypeEnvironment
- Disabled caching for types containing type parameters
- Check `initial_type` for type parameters ONCE outside loop (performance)
- Check BOTH `initial_type` and `final_type` before cache write (soundness)

**Critical Insights (Gemini Flash Review)**:
- Performance: Move check outside loop (O(1) instead of O(N))
- Soundness: Must check both initial and final types ("Generic Result" bug)
- Example: `any` narrowed to `T` via type guard

**Impact**: Prevents stale cached results across different generic instantiations.

---

## Summary

**Phase 3 (CFA Hardening & Loop Refinement) is COMPLETE ✅**

All 4 tasks completed successfully:
- Task 4.1: Loop Mutation Analysis ✅
- Task 4.2: Switch Union Aggregation ✅
- Task 4.3: Falsy Value Completeness ✅
- Task 4.4: CFA Cache Safety ✅

**Key Achievements**:
1. Selective loop widening based on actual mutations (not conservative widening)
2. Correct switch fallthrough with union aggregation
3. Falsy narrowing that matches TypeScript exactly
4. Cache safety for generic functions

**Total Commits**: 4
**Lines Changed**: ~200 lines across 3 files

---

## Coordination Notes

### Avoid (tsz-1 domain):
- **Intersection Reduction** in `src/solver/intern.rs` (tsz-1 is working on this)
- Focus on **filtering logic** in `narrowing.rs`, not **reduction logic**

### Leverage:
- **tsz-2** (Checker-Solver Bridge): Use the `TypeResolver` to resolve `Lazy` types
- **tsz-3 previous work**: TypeEnvironment infrastructure is already in place

### North Star Rule:
- **NO AST dependencies** in `src/solver/narrowing.rs`
- Use `TypeGuard` enum to pass information from Checker to Solver
- Keep narrowing logic in the Solver (pure type algebra)

---

## Complexity: MEDIUM-HIGH

**Why Medium-High**: Loop mutation analysis requires careful flow graph traversal:
- Must handle nested scopes, closures, and continue statements
- Cache invalidation is subtle (generic instantiations)
- Switch fallthrough requires aggregating multiple antecedents correctly
- Falsy narrowing requires matching TypeScript exactly

**Risk**: Incorrect loop analysis could either:
1. Be too conservative (no improvement over current state)
2. Be too permissive (incorrect narrowing leading to runtime errors)

**Mitigation**: Follow Two-Question Rule strictly. All changes must be reviewed by Gemini Pro before commit.
