# Session tsz-2: Stabilization & North Star Architecture Alignment

**Started**: 2026-02-05
**Status**: Active - Merge Conflict to Resolve
**Focus**: Stabilize Ref -> Lazy/DefId migration, then enforce Checker/Solver separation

**Previous Session**: Coinductive Subtyping (COMPLETE)
**Next Session**: TBD

## Progress Summary

**Completed (Phase 4.3 Migration):**
- ✅ Fixed PromiseTypeKind::SymbolRef (removed)
- ✅ Fixed NewExpressionTypeKind::SymbolRef (removed)
- ✅ Fixed ConstructorCheckKind::SymbolRef (replaced with Lazy)
- ✅ Fixed PropertyAccessResolutionKind::Ref (removed)
- ✅ Fixed ContextualLiteralAllowKind::Ref (removed)
- ✅ Fixed SymbolResolutionTraversalKind::Ref (removed)
- ✅ Fixed NamespaceMemberKind::SymbolRef (replaced with Lazy)
- ✅ Replaced all get_ref_symbol() calls with resolve_type_to_symbol_id()
- ✅ Replaced all get_symbol_ref() calls with resolve_type_to_symbol_id()
- ✅ Replaced get_ref_if_symbol() with get_lazy_if_def()
- ✅ Removed deprecated functions from type_queries.rs
- **Eliminated ALL 46 deprecation warnings** (70 → 0)

**Current Status:**
- Local commit made but merge conflict with remote (tsz-4 touched similar files)
- Need to rebase on main and update incoming tsz-4 code to use new APIs

## Current Plan

### 1. Resolve Merge Conflicts (IMMEDIATE BLOCKER)
The conflict stems from tsz-4 adding new logic that uses deprecated functions.

**Action**: Rebase on main and fix tsz-4 code to use new APIs
- Update any new calls to `get_ref_symbol`/`get_symbol_ref` → `resolve_type_to_symbol_id`
- Update any new calls to `get_ref_if_symbol` → `get_lazy_if_def`
- Consult Gemini if translation isn't 1:1

### 2. Verify Conformance (CRITICAL)
Major refactors like "Ref -> Lazy" often introduce subtle regressions.

```bash
./scripts/conformance/run.sh --server --max=500
```

**Goal**: Ensure 0 regressions. Use `tsz-tracing` to debug if Lazy type resolution behaves differently than old Ref logic.

### 3. Architecture Refactor: Remove Direct TypeKey Matching in Checker
Per NORTH_STAR.md Anti-Pattern 8.1: Checker should NEVER inspect TypeKey internals directly.

**Task:**
- Search `src/checker/` for `match .*lookup\(.*\)` or `TypeKey::`
- Identify places where Checker manually unwraps types (checking Union, Object, etc.)
- Refactor to use `solver.is_...` methods or Visitor Pattern (`src/solver/visitor.rs`)

## Success Criteria

- [x] All 46 deprecation warnings resolved
- [x] Deprecated functions removed
- [ ] Merge conflicts resolved and pushed to main
- [ ] Conformance tests pass with 0 regressions
- [ ] No direct TypeKey matching in Checker (Anti-Pattern 8.1 eliminated)

## Notes

**Why This Priority:**
1. **Stability**: Merge conflict blocks other sessions
2. **Correctness**: Ref → Lazy migration may have subtle bugs
3. **Architecture**: Enforces North Star (Checker/Solver separation)

**Key Files:**
- src/checker/state_type_environment.rs (auto-merged)
- src/checker/type_checking_queries.rs (auto-merged)
- src/checker/assignment_checker.rs (needs re-apply)
- src/checker/enum_checker.rs (needs re-apply)
- src/checker/state_type_analysis.rs (needs re-apply)
- src/checker/type_checking.rs (needs re-apply)
- src/checker/type_computation_complex.rs (needs re-apply)
- src/checker/context.rs (needs re-apply)
- src/solver/type_queries.rs (needs re-apply)
