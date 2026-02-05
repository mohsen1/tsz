# Session tsz-3: CFA Refinement & Stabilization

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE - STABILIZING FOUNDATION

## Goal

Fix core CFA regressions to provide a stable foundation for advanced features.

## Context

Previous tsz-3 Phase 1 delivered:
- ✅ Bidirectional Narrowing (x === y where both are references)
- ✅ Assertion Functions (asserts x is T)

## Current Situation: The "Hydra Effect"

Every fix reveals more architectural issues:
- Assertion fix corrects 1 test but breaks 5 circular extends tests
- Array destructuring requires deep flow graph architecture knowledge
- Each task reveals deeper foundation issues

## Gemini's Recommendation (2026-02-05)

**"The Solver-First Stabilization"**

Do NOT build advanced features on broken foundation. Circular extends
failures signal Solver's resolution and cycle-detection are fragile.

**New Priority Order**:
1. **Truthiness** (Task 2) - LOW complexity, quick win
2. **Circular Extends** (Task 3) - CRITICAL, unblocks everything
3. **Array Destructuring** (Task 1) - MEDIUM-HIGH, infrastructure
4. **Assertions/Any** (Tasks 4-5) - BLOCKED by Task 3

---

## Task 2: Truthiness Narrowing (🔄 ACTIVE - QUICK WIN)

**Test**: `test_truthiness_false_branch_narrows_to_falsy`

**Why First**: Most isolated task, likely simple fix.

**Expected**: False branch of `if (x)` where `x: string | number | boolean | null | undefined` narrows to falsy values.

**File**: `src/solver/narrowing.rs` - `narrow_by_truthiness`

**Hypothesis**: Missing `resolve_type()` or `evaluate()` call.

**Estimated Complexity**: LOW (30 minutes - 1 hour)

---

## Task 3: Circular Extends Deep Dive (⏸️ PENDING - THE DRAGON)

**Status**: ⏸️ CRITICAL BLOCKER - HIGH COMPLEXITY

**Tests**: 5 circular extends tests broken by correct assertion logic

**Why Critical**: Unblocks assertion predicates, any narrowing, nested discriminants.

**Root Cause**: Solver's cycle detection is fragile. Correct narrowing forces
resolution of circular types that were previously "lazy."

**Investigation**:
1. Re-apply assertion fix
2. Run: `TSZ_LOG=trace TSZ_LOG_FORMAT=tree cargo test test_circular_extends_chain_with_endpoint_bound`
3. Find where Solver returns `TypeId::ERROR`
4. Hypothesis: `cycle_stack` in `subtype.rs` or `evaluate.rs` triggered by narrowing
5. Ask Gemini Pro with trace for speculative narrowing solution

**Files**: `src/solver/subtype.rs`, `src/solver/evaluate.rs`

**Estimated Complexity**: HIGH (4-6 hours, deep solver architecture)

---

## Task 1: Array Destructuring Narrowing (⏸️ DEPRIORITIZED)

**Status**: ⏸️ MOVED TO LAST - INFRASTRUCTURE ISSUE

**Tests**: Array destructuring clearing narrowing

**Why Moved**: Binder/Flow Graph issue, less critical than Solver stability.

**Files**: `src/binder/state.rs` - `bind_binary_expression_flow_iterative`

**When**: After Tasks 2 and 3 complete

---

## Task 4: Assertion Predicate Fix (✅ READY - BLOCKED)

**Status**: ✅ CODE READY - AWAITING TASK 3

**What**: Fix `TypeGuard::Predicate` to only narrow in true branch for assertions.

**Blocked**: Circular extends investigation must complete first.

---

## Task 5: Nested Discriminants (⏸️ BLOCKED)

**Status**: ⏸️ PAUSED - BLOCKED BY FOUNDATION STABILITY

**Implementation**: Code written and reviewed, awaiting stable test suite.

---

## Previous Work (Archived)

- Nested discriminants implementation (reverted due to test failures)
- Session tsz-12 merged into tsz-3
