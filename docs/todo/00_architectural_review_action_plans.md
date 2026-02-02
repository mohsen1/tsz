# Architectural Review Action Plans Index

**Reference**: Critical Architectural Review Summary (February 2, 2026)  
**Status**: Some plans completed (08, 09, 11 removed), remaining ready for implementation

This document indexes all remaining action plans from the architectural review findings.

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

### [09_cycle_detection_fix.md](09_cycle_detection_fix.md)
**Issue**: Coinductive cycle detection too aggressive (universal GFP)  
**Priority**: High - Correctness issue  
**Solution**: Implement TypeScript's specific recursion rules

### [10_narrowing_to_solver.md](10_narrowing_to_solver.md)
**Issue**: Control flow narrowing math in Checker instead of Solver  
**Priority**: High - Architecture violation  
**Solution**: Move narrowing calculation to Solver, Checker builds CFG

### [11_visitor_pattern_enforcement.md](11_visitor_pattern_enforcement.md)
**Issue**: Manual `TypeKey` matches throughout codebase  
**Priority**: High - Code quality and maintainability  
**Solution**: Replace all matches with visitor pattern

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

2. **🔴 Critical Path** (Correctness):
   - None remaining

3. **🟠 High Priority** (Architecture):
   - [08_solver_first_architecture.md](08_solver_first_architecture.md) - Core refactor
   - [10_narrowing_to_solver.md](10_narrowing_to_solver.md) - Related to #08
   - [11_visitor_pattern_enforcement.md](11_visitor_pattern_enforcement.md) - Code quality

4. **🟠 High Priority** (Correctness):
   - [09_cycle_detection_fix.md](09_cycle_detection_fix.md) - Correctness fix

5. **🟠 High Priority** (Cleanup):
   - [12_type_identity_migration.md](12_type_identity_migration.md) - Consistency
   - [13_binder_checker_overlap.md](13_binder_checker_overlap.md) - Architecture clarity

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
