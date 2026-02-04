# Session tsz-3: CFA Hardening & Loop Refinement

**Started**: 2026-02-04
**Status**: 🟢 ACTIVE (Priority 2 COMPLETE, starting Priority 4)
**Latest Update**: 2026-02-04 - Redefined focus to CFA Hardening
**Focus**: Loop narrowing refinement, switch fallthrough, falsy completeness

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

## Current Phase: CFA Hardening & Loop Refinement (IN PROGRESS)

**Started**: 2026-02-04
**Status**: Starting Priority 4 tasks

### Problem Statement

The current CFA implementation is **too conservative** regarding loops and closures compared to tsc:

1. **Loop Widening**: Currently resets ALL let/var variables to declared type at loop headers, even if they're never mutated in the loop body.
2. **Switch Fallthrough**: May not correctly union narrowed types from multiple case antecedents.
3. **Falsy Completeness**: Need to verify NaN, 0n, and other edge cases match tsc exactly.
4. **Cache Safety**: Flow analysis cache may return stale results across different generic instantiations.

### Prioritized Tasks

#### Task 4.1: Loop Mutation Analysis (HIGH)
**File**: `src/checker/control_flow.rs` (lines 335-365)

**Goal**: Only widen let/var at LOOP_LABEL if the variable is mutated in the loop body.

**Implementation**:
- Implement "Mutation Scanner" that checks loop body for assignments to target SymbolId
- If no mutations exist, skip the widening at LOOP_LABEL
- Handle nested loops, closures, and continue statements

**Expected Impact**: Significant improvement in narrowing precision for common patterns like:
```typescript
let x: string | number = getValue();
if (typeof x === "string") {
    while (condition) {
        console.log(x.length); // Should be string, not widened
    }
}
```

#### Task 4.2: Switch Union Aggregation (MEDIUM)
**File**: `src/checker/control_flow.rs` (lines 515-560)

**Goal**: Fix check_flow to correctly union types from multiple SWITCH_CLAUSE antecedents during fallthrough.

**Implementation**:
- Verify handle_switch_clause_iterative handles antecedent.len() > 1
- Ensure types from all preceding cases are unioned correctly
- Test complex fallthrough chains

#### Task 4.3: Falsy Value Completeness (MEDIUM)
**Files**: `src/solver/narrowing.rs`, `src/checker/control_flow_narrowing.rs`

**Goal**: Ensure NaN, 0n (BigInt), and all falsy primitives are correctly narrowed.

**Implementation**:
- Audit narrow_to_falsy and narrow_by_truthiness against tsc's getFalsyFlags
- Test edge cases: NaN, 0n, -0, empty string, false, null, undefined
- Verify union types of mixed primitives narrow correctly

#### Task 4.4: CFA Cache Safety (LOW)
**Files**: `src/checker/context.rs`, `src/checker/control_flow.rs`

**Goal**: Audit flow_analysis_cache to ensure no stale results across generic instantiations.

**Implementation**:
- Review cache key: (FlowNodeId, SymbolId, InitialTypeId)
- Determine if TypeEnvironment hash is needed
- Test with different generic instantiations

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

