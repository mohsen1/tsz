# Architectural Review Action Plans Index

**Reference**: Critical Architectural Review Summary (February 2, 2026)
**Status**: Plan #09 completed, Plan #11 88% complete, remaining plans ready for implementation

This document indexes all action plans from the architectural review findings.

---

## 🔴 Critical Issues (Must Fix)

### [07_memory_leak_type_interner.md](07_memory_leak_type_interner.md)
**Issue**: Append-only TypeInterner with no GC causes OOM in LSP  
**Priority**: Critical - LSP viability depends on this  
**Solution**: Implement ScopedTypeInterner with generational GC

---

## 🟠 High Severity Issues

### [08_solver_first_architecture.md](08_solver_first_architecture.md)
**Issue**: Checker doing massive type computation, violating solver-first architecture  
**Priority**: High - Core architecture violation  
**Solution**: Move all type math to Solver, Checker calls semantic APIs

### [09_cycle_detection_fix.md](09_cycle_detection_fix.md) ✅ COMPLETED
**Issue**: Coinductive cycle detection too aggressive (universal GFP)
**Priority**: High - Correctness issue
**Solution**: Implemented TypeScript's specific recursion rules
**Status**: Completed 2026-02-02, all tests passing

### [10_narrowing_to_solver.md](10_narrowing_to_solver.md)
**Issue**: Control flow narrowing math in Checker instead of Solver  
**Priority**: High - Architecture violation  
**Solution**: Move narrowing calculation to Solver, Checker builds CFG

### [11_visitor_pattern_enforcement.md](11_visitor_pattern_enforcement.md) 🔄 88% COMPLETE
**Issue**: Manual `TypeKey` matches throughout codebase
**Priority**: High - Code quality and maintainability
**Solution**: Replace all matches with visitor pattern
**Status**: 88% complete (~140 of ~159 refs eliminated)
- ✅ index_signatures.rs (4 visitors)
- ✅ binary_ops.rs (7 visitors)
- ✅ compat.rs (1 visitor)
- 🔄 contextual.rs (10 visitors, 91% of main methods)
- ⏳ Remaining: contextual.rs helpers, index_access.rs, narrowing.rs, subtype.rs, flow_narrowing.rs

### [12_type_identity_migration.md](12_type_identity_migration.md)
**Issue**: Split-brain type identity (SymbolRef vs DefId)  
**Priority**: High - Correctness and consistency  
**Solution**: Complete migration to DefId, remove SymbolRef

### [13_binder_checker_overlap.md](13_binder_checker_overlap.md)
**Issue**: Checker doing scope walking that Binder should handle  
**Priority**: High - Architecture clarity  
**Solution**: Remove scope walking from Checker, trust Binder

---

## Implementation Priority

Based on severity and dependencies:

1. **🔴 Critical Path** (LSP viability):
   - [07_memory_leak_type_interner.md](07_memory_leak_type_interner.md) - Must fix for LSP

2. **🟠 High Priority** (Code Quality - In Progress):
   - [11_visitor_pattern_enforcement.md](11_visitor_pattern_enforcement.md) - 88% complete, finish remaining files

3. **🟠 High Priority** (Architecture):
   - [08_solver_first_architecture.md](08_solver_first_architecture.md) - Core refactor
   - [10_narrowing_to_solver.md](10_narrowing_to_solver.md) - Related to #08

4. **🟠 High Priority** (Cleanup):
   - [12_type_identity_migration.md](12_type_identity_migration.md) - Consistency
   - [13_binder_checker_overlap.md](13_binder_checker_overlap.md) - Architecture clarity

**Completed:**
- ✅ [09_cycle_detection_fix.md](09_cycle_detection_fix.md) - Correctness fix (completed 2026-02-02)

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
