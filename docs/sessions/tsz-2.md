# Session tsz-2: Phase 4.3 Migration & Anti-Pattern 8.1

**Started**: 2026-02-05
**Status**: Active - Completing Phase 4.3, then Anti-Pattern 8.1
**Focus:** Complete Ref → Lazy/DefId migration, then enforce Solver-First boundary

**Previous Session**: Coinductive Subtyping (COMPLETE)
**Next Session**: TBD

## Progress Summary

**Completed (Phase 4.3 Migration):**
- ✅ All 46 deprecation warnings eliminated (70 → 0) in earlier session
- ⚠️ **LOST**: Most Phase 4.3 changes were lost during merge conflict resolution
- ⚠️ **CURRENT**: 12 deprecated function warnings remain

**Issue:**
During merge conflict with tsz-4 (commit 1b467adb0), I reset to origin/main and lost
the Phase 4.3 fixes. Only assignment_checker.rs fix survived (commit 7d338791a).

**Remaining Work (Phase 4.3):**
- src/checker/enum_checker.rs (3 × get_ref_symbol)
- src/checker/state_type_analysis.rs (1 × get_symbol_ref)
- src/checker/state_type_environment.rs (import + 1 × get_symbol_ref)
- src/checker/type_computation_complex.rs (1 × get_ref_if_symbol)
- src/checker/context.rs (fallback code using get_symbol_ref)
- src/solver/type_queries.rs (remove deprecated functions)

**Next Session Plan:**
1. Complete Phase 4.3 migration (12 warnings remaining)
2. Then proceed with Anti-Pattern 8.1 refactoring

## Anti-Pattern 8.1 Planning (Post Phase 4.3)

**Completed Audit:**
- 109 TypeKey usages across 18 checker files
- assignability_checker.rs: 15 usages (TYPE TRAVERSAL)
- generators.rs: 10 usages
- iterators.rs: 9 usages
- state_type_environment.rs: 19 usages
- state_type_analysis.rs: 18 usages

**Refactoring Strategy (validated by Gemini):**
1. Add `walk_type_children()` helper to src/solver/visitor.rs
2. Create `RefResolverVisitor` in assignability_checker.rs
3. Replace 200-line match statement with visitor pattern
4. Extend to other files iteratively

**Gemini Guidance:**
- Traversal logic → Solver layer (walk_type_children)
- Resolution logic → Checker layer (visitor implementation)
- Must handle edge cases: cycles, TypeQuery, Application types
- Watch for double-borrow panics with type_env

## Two-Question Rule (MANDATORY)

For ANY changes to `src/solver/` or `src/checker/`:

**Question 1 (Before):**
```bash
./scripts/ask-gemini.mjs --include=src/solver "I'm doing X in Y file.
Plan: [YOUR PLAN]

Is this correct? What edge cases?"
```

**Question 2 (After):**
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "I implemented X.
Code: [PASTE CODE]

Does this handle all edge cases correctly?"
```

## Session History

- 2026-02-05: Started Phase 4.3 migration
- 2026-02-05: Completed Phase 4.3 (46 warnings fixed)
- 2026-02-05: Lost changes during merge conflict
- 2026-02-05: Redefined session to Anti-Pattern 8.1
- 2026-02-05: Discovered Phase 4.3 incomplete (12 warnings remain)
- **NEXT**: Complete Phase 4.3, then Anti-Pattern 8.1
