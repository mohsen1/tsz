# Session tsz-3: Generic Inference & Nominal Hierarchy Integration

**Started**: 2026-02-04
**Status**: 🟢 ACTIVE (Phase 4 starting)
**Latest Update**: 2026-02-04 - Redefined focus to Generic Inference & Nominal Hierarchy
**Focus**: Best Common Type, Homomorphic Mapped Types, Inter-Parameter Constraints

---

## Current Phase: Generic Inference & Nominal Hierarchy Integration (IN PROGRESS)

**Started**: 2026-02-04
**Status**: Ready to begin Task 1

### Problem Statement

The current generic inference and type system has several gaps that cause `any` leakage and imprecision:

1. **Nominal BCT**: `compute_best_common_type` cannot find common base classes (e.g., `[Dog, Cat] -> Animal`) because the Solver can't see the inheritance graph
2. **Homomorphic Mapped Types**: `Partial<T[]>` turns arrays into plain objects, losing methods like `.push()`
3. **Inter-Parameter Constraints**: Constraints don't flow between type parameters (e.g., `T extends U` doesn't propagate constraints from `U` to `T`)
4. **Contextual Return Inference**: Generic calls don't fully utilize expected return types to constrain inference

### Prioritized Tasks

#### Task 1: Nominal BCT Bridge (Binder-Solver Link) (HIGH)
**Files**: `src/solver/expression_ops.rs`, `src/solver/infer.rs`, `src/solver/subtype.rs`, `src/checker/context.rs`

**Goal**: Enable `compute_best_common_type` to find common base classes using the inheritance graph.

**Implementation**:
1. Implement `get_base_type` method in `TypeResolver` trait (defined in `src/solver/subtype.rs`)
2. Update implementation in `src/checker/context.rs` to query `InheritanceGraph`
3. Update `compute_best_common_type` to use resolver for most specific common supertype

**Example**: `[Dog, Cat]` should infer `Animal` (common base), not `Dog | Cat` (union)

**Coordination**: Follow Two-Question Rule - touches sensitive Binder/Solver boundary

#### Task 2: Homomorphic Mapped Type Preservation (MEDIUM)
**File**: `src/solver/evaluate_rules/mapped.rs`

**Goal**: Ensure mapped types preserve array/tuple structure instead of degrading to plain objects.

**Implementation**:
1. Modify `evaluate_mapped` to detect if source type is `Array` or `Tuple`
2. Return new `Array`/`Tuple` with template applied to elements
3. Ensure modifiers (readonly/optional) map correctly

**Example**: `Partial<number[]>` should be `number[]` with optional elements, not `{ [n: number]: number }`

#### Task 3: Inter-Parameter Constraint Propagation (MEDIUM)
**File**: `src/solver/infer.rs`

**Goal**: Implement `strengthen_constraints` for fixed-point iteration over type parameter bounds.

**Implementation**:
1. If `T` has lower bound `L` and `T <: U`, then `L` becomes lower bound for `U`
2. Critical for signatures like `function pipe<T, U>(val: T, fn: (x: T) => U): U`

#### Task 4: Contextual Return Inference (LOW)
**File**: `src/solver/operations.rs`

**Goal**: Refine `resolve_generic_call` to collect constraints from `contextual_type` before resolving.

**Implementation**:
1. Use expected return type to constrain inference variables
2. Example: `let x: string = identity(42)` should fail

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

## Gemini Consultation Plan

Following the mandatory Two-Question Rule from `AGENTS.md`:

### Question 1: Approach Validation (BEFORE implementation)
**Task 4.1 - Loop Mutation Analysis**:
```bash
./scripts/ask-gemini.mjs --include=src/checker/control_flow.rs "I need to implement loop mutation analysis for selective widening.

Current situation:
- Lines 335-365 in control_flow.rs conservatively widen ALL let/var at LOOP_LABEL
- tsc only widens if variable is mutated in loop body

Planned approach:
1. Create mutation_scanner function that walks loop body's flow nodes
2. Check for ASSIGNMENT flags targeting the SymbolId
3. If no mutations, skip widening in check_flow's LOOP_LABEL handling

Before I implement:
1) Is this the right approach? What functions should I modify?
2) How do I handle nested loops and closures?
3) What about continue statements that re-enter loop?
4) Are there edge cases I'm missing?"
```

### Question 2: Implementation Review (AFTER implementation)
```bash
./scripts/ask-gemini.mjs --pro --include=src/checker/control_flow.rs "I implemented loop mutation analysis.

Changes: [PASTE CODE OR DIFF]

Please review: 1) Is this correct for TypeScript? 2) Did I miss any edge cases?
3) Are there type system bugs? Be specific if wrong."
```

---

## Session History

- 2026-02-04: Session started - CFA infrastructure work (TypeEnvironment, Loop Narrowing)
- 2026-02-04: CFA Phase COMPLETE - all 74 control_flow_tests pass
- 2026-02-04: **REDEFINED** to "Solver-First Narrowing & Discriminant Hardening"
- 2026-02-04: Solver-First Phase COMPLETE - all 5 tasks done
- 2026-02-04: **REDEFINED** to "User-Defined Type Guards"
- 2026-02-04: User-Defined Type Guards COMPLETE - Priority 2a & 2b done
- 2026-02-04: **REDEFINED** to "CFA Hardening & Loop Refinement"

---

## Complexity: MEDIUM-HIGH

**Why Medium-High**: Loop mutation analysis requires careful flow graph traversal:
- Must handle nested scopes, closures, and continue statements
- Cache invalidation is subtle (generic instantiations)
- Switch fallthrough requires aggregating multiple antecedents correctly

**Risk**: Incorrect loop analysis could either:
1. Be too conservative (no improvement over current state)
2. Be too permissive (incorrect narrowing leading to runtime errors)

**Mitigation**: Follow Two-Question Rule strictly. All changes must be reviewed by Gemini Pro before commit.

