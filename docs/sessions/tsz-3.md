# Session tsz-3: LSP Implementation - JSX Linked Editing

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE
**Previous Session**: CFA Stabilization (Blocked - architectural conflicts)

## Goal

Implement LSP features that improve developer experience without requiring deep Solver/Checker architecture expertise.

## Current Work: JSX Linked Editing

Implementing `textDocument/linkedEditingRange` for JSX/TSX files.

**Value**: When editing an opening JSX tag (e.g., `<div>`), the closing tag (`</div>`) automatically syncs.

**Implementation**: Creating `src/lsp/linked_editing.rs`
- Algorithm: Find JSX tag context, locate parent JsxElement, extract ranges for both opening and closing tag names
- Files to touch: `src/lsp/linked_editing.rs` (new), `src/lsp/mod.rs` (dispatch)
- Reference: `src/lsp/highlighting.rs` for parent-walking pattern

**Status**: Planning phase - reviewed implementation with Gemini

## Outcome: BLOCKED

After 6+ hours of investigation, encountered fundamental architectural conflict between **coinductive type inference** and **control flow narrowing**.

### What Was Attempted

1. **Assertion Predicate Fix** (commit c25830407 - REVERTED)
   - ✅ Logically correct TypeScript semantics
   - ✅ Fixes 1 test
   - ❌ Breaks 5 circular extends tests

2. **Assertion Predicate Fix v2** (commit 137b82c62 - SAFE)
   - ✅ Verified correct by Gemini review
   - ✅ Fixes test_asserts_type_predicate_narrows_true_branch
   - ✅ Does NOT break circular extends tests (isolated to narrow_type bridge)
   - ✅ Safe to keep (doesn't touch core type algebra)

2. **Truthiness Narrowing Fix** (commit 360c66e00 - REVERTED)
   - ✅ Logically correct TypeScript semantics
   - ✅ Fixes 1 test
   - ❌ Breaks SAME 5 circular extends tests

### Root Cause

Both fixes introduce literal types during type narrowing that interfere with type parameter resolution in circular contexts. This reveals:

- **Architectural Conflict**: Coinductive type inference vs. control flow narrowing
- **Solver Fragility**: `cycle_stack` in `subtype.rs` or `evaluate.rs` returns `ERROR` instead of handling cycles coinductively
- **Test Fragility**: Circular extends tests pass when certain narrowing doesn't happen, creating an "illusion of success"

### Required Investigation

To unblock, needs deep Solver architecture expertise:
1. Trace `cycle_stack` with `TSZ_LOG=trace`
2. Understand coinductive cycles (Greatest Fixed Point)
3. Modify cycle detection to distinguish valid vs invalid cycles
4. Make narrowing "lazier" to avoid forcing type resolution

**Estimated**: 20+ hours for someone unfamiliar with Solver's coinductive logic
**Risk**: HIGH - can destabilize entire compiler

## Deliverables

- ✅ **Documentation**: `docs/issues/CFA_CFA_CIRCULAR_DEPENDENCY.md`
  - Detailed analysis of both fixes
  - Root cause explanation
  - Required investigation steps
  - Complexity assessment

- ✅ **Session Notes**: All findings documented in this file

## Recommendation

**Status**: 🛑 PAUSED - HAND OFF REQUIRED

This requires expert-level Solver architecture knowledge. Continuing without this expertise risks:
- Spending 10+ more hours without guaranteed success
- Potentially destabilizing the compiler
- Blocking other sessions from making progress

## Previous Work (Archived)

From completed tsz-3 Phase 1:
- ✅ Bidirectional Narrowing
- ✅ Assertion Functions integration

## Next Steps for Future Work

1. **Solver Expert** investigates circular dependency (see issue doc)
2. Once circular extends resolved, re-apply:
   - Assertion predicate fix (code ready)
   - Truthiness narrowing fix (code ready)
3. Then continue with:
   - Array destructuring narrowing
   - Nested discriminants (original goal)

---

## See Also

- `docs/issues/CFA_CIRCULAR_DEPENDENCY.md` - Full technical analysis
- `docs/sessions/tsz-3.md` - This session
- `docs/sessions/history/tsz-3.md` - Completed Phase 1 work
