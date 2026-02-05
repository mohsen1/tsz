# Session tsz-3: Advanced CFA Features

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE
**Previous Session**: tsz-10 (CFA & Narrowing - Complete)

## Goal

Implement advanced Control Flow Analysis features to achieve 100% TypeScript parity.

## Progress

### Phase 1: Bidirectional Narrowing (PAUSED - Needs Architecture Decision)

**Status**: ⏸️ ANALYSIS COMPLETE, AWAITING ARCHITECTURE DECISION

**Problem**: Implement narrowing for `x === y` where both are references.

**Analysis Complete**:
- Current code in `narrow_by_binary_expr` (line ~2362) already has symmetric checks
- **CRITICAL BUG**: Uses `node_types` (declared types) instead of flow types
- Example: If `y` was narrowed to `string` in outer scope, `x === y` should narrow `x` to `string`
- Currently uses `y`'s declared type, not its flow-narrowed type

**Architectural Challenge**:
- `narrow_by_binary_expr` doesn't have access to flow context (no flow node ID)
- Can't query flow type of "other" reference without flow context
- Passing flow context through entire call chain would be significant refactor

**Gemini Guidance**:
- Need helper method like `get_type_of_reference_at_antecedent(other, flow_id)`
- Query type from antecedent flow node (state before this comparison)
- Use `results` cache to look up computed types

**Decision Needed**:
1. Refactor to pass flow context through call chain?
2. Create new API to query flow types from within `narrow_by_binary_expr`?
3. Different architectural approach?

**Next Steps**:
1. Ask Gemini: How to get flow type in `narrow_by_binary_expr` without major refactor?
2. Implement the recommended approach
3. Ask Gemini Question 2 for code review

---

### Phase 2: Assertion Functions (PENDING)

**Status**: ⏸️ NOT STARTED

Integration of `asserts x is T` with flow analysis for all subsequent code.

---

### Phase 3: Nested Discriminants (PENDING)

**Status**: ⏸️ NOT STARTED

Support for `action.payload.kind` style discriminants.

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
