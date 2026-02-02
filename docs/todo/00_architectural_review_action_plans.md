# Architectural Review Action Plans Index

**Reference**: Critical Architectural Review Summary (February 2, 2026)
**Status**: Plans #08, #09, #11, #13 completed (archived), remaining plans ready for implementation

This document indexes all action plans from the architectural review findings. Completed items have been moved to `archive/`.

---

## 🔴 Critical Issues (Must Fix)

### [07_memory_leak_type_interner.md](07_memory_leak_type_interner.md)
**Issue**: Append-only TypeInterner with no GC causes OOM in LSP  
**Priority**: Critical - LSP viability depends on this  
**Solution**: Implement ScopedTypeInterner with generational GC

---

## 🟠 High Severity Issues

### [10_narrowing_to_solver.md](10_narrowing_to_solver.md)
**Issue**: Control flow narrowing math in Checker instead of Solver
**Priority**: High - Architecture violation
**Solution**: Move narrowing calculation to Solver, Checker builds CFG

### [12_type_identity_migration.md](12_type_identity_migration.md)
**Issue**: Split-brain type identity (SymbolRef vs DefId)
**Priority**: High - Correctness and consistency
**Solution**: Complete migration to DefId, remove SymbolRef

---

## Implementation Priority

Based on severity and dependencies:

1. **🔴 Critical Path** (LSP viability):
   - [07_memory_leak_type_interner.md](07_memory_leak_type_interner.md) - Must fix for LSP

2. **🟠 High Priority** (Architecture):
   - [10_narrowing_to_solver.md](10_narrowing_to_solver.md) - Move narrowing to Solver

3. **🟠 High Priority** (Cleanup):
   - [12_type_identity_migration.md](12_type_identity_migration.md) - Consistency

**Completed & Archived:**
- ✅ [archive/08_solver_first_architecture.md](archive/08_solver_first_architecture.md) - Core architecture (completed 2026-02-02)
- ✅ [archive/09_cycle_detection_fix.md](archive/09_cycle_detection_fix.md) - Correctness fix (completed 2026-02-02)
- ✅ [archive/11_visitor_pattern_enforcement.md](archive/11_visitor_pattern_enforcement.md) - Code quality (completed 2026-02-02)
- ✅ [archive/13_binder_checker_overlap.md](archive/13_binder_checker_overlap.md) - Architecture clarity (completed 2026-02-02)

---

## Notes

- All plans include detailed implementation phases, testing strategies, and acceptance criteria
- Plans reference specific file locations and code patterns
- Each plan is designed to be implemented incrementally with verification at each phase
- Conformance tests should be run after each major phase to ensure no regressions

---

## Related Documents

- [Critical Architectural Review Summary](../../ARCHITECTURAL_REVIEW_SUMMARY.md) - Original findings
- [AGENTS.md](../../../AGENTS.md) - Project rules and architecture guidelines
- [NORTH_STAR.md](../../architecture/NORTH_STAR.md) - Target architecture
