# Architectural Review Action Plans Index

**Reference**: Critical Architectural Review Summary (February 2, 2026)  
**Status**: All plans created, ready for implementation

This document indexes all action plans created from the architectural review findings.

---

## 🔴 Critical Issues (Must Fix)

### [07_memory_leak_type_interner.md](07_memory_leak_type_interner.md)
**Issue**: Append-only TypeInterner with no GC causes OOM in LSP  
**Priority**: Critical - LSP viability depends on this  
**Solution**: Implement ScopedTypeInterner with generational GC

### [08_inference_algorithm_rewrite.md](08_inference_algorithm_rewrite.md)
**Issue**: Uses standard unification instead of TypeScript's candidate-based inference  
**Priority**: Critical - Correctness divergence from tsc  
**Solution**: Rewrite to candidate collection + BCT/Widening system

### [09_freshness_tracking_refactor.md](09_freshness_tracking_refactor.md)
**Issue**: "Zombie Freshness" - freshness tracked in both Checker and Solver with SymbolId  
**Priority**: Critical - Architecture violation and correctness divergence  
**Solution**: Move freshness to transient property on TypeKey, remove SymbolId dependency

---

## 🟠 High Severity Issues

### [10_solver_first_architecture.md](10_solver_first_architecture.md)
**Issue**: Checker doing massive type computation, violating solver-first architecture  
**Priority**: High - Core architecture violation  
**Solution**: Move all type math to Solver, Checker calls semantic APIs

### [11_any_propagation_fix.md](11_any_propagation_fix.md)
**Issue**: `any` propagation uses heuristics instead of flag-passing  
**Priority**: High - Correctness issue  
**Solution**: Implement flag propagation system, remove heuristics

### [12_cycle_detection_fix.md](12_cycle_detection_fix.md)
**Issue**: Coinductive cycle detection too aggressive (universal GFP)  
**Priority**: High - Correctness issue  
**Solution**: Implement TypeScript's specific recursion rules

### [13_narrowing_to_solver.md](13_narrowing_to_solver.md)
**Issue**: Control flow narrowing math in Checker instead of Solver  
**Priority**: High - Architecture violation  
**Solution**: Move narrowing calculation to Solver, Checker builds CFG

### [14_visitor_pattern_enforcement.md](14_visitor_pattern_enforcement.md)
**Issue**: Manual `TypeKey` matches throughout codebase  
**Priority**: High - Code quality and maintainability  
**Solution**: Replace all matches with visitor pattern

### [15_type_identity_migration.md](15_type_identity_migration.md)
**Issue**: Split-brain type identity (SymbolRef vs DefId)  
**Priority**: High - Correctness and consistency  
**Solution**: Complete migration to DefId, remove SymbolRef

### [16_binder_checker_overlap.md](16_binder_checker_overlap.md)
**Issue**: Checker doing scope walking that Binder should handle  
**Priority**: High - Architecture clarity  
**Solution**: Remove scope walking from Checker, trust Binder

---

## Implementation Priority

Based on severity and dependencies:

1. **🔴 Critical Path** (LSP viability):
   - [07_memory_leak_type_interner.md](07_memory_leak_type_interner.md) - Must fix for LSP

2. **🔴 Critical Path** (Correctness):
   - [08_inference_algorithm_rewrite.md](08_inference_algorithm_rewrite.md) - High divergence risk
   - [09_freshness_tracking_refactor.md](09_freshness_tracking_refactor.md) - Architecture violation

3. **🟠 High Priority** (Architecture):
   - [10_solver_first_architecture.md](10_solver_first_architecture.md) - Core refactor
   - [13_narrowing_to_solver.md](13_narrowing_to_solver.md) - Related to #10
   - [14_visitor_pattern_enforcement.md](14_visitor_pattern_enforcement.md) - Code quality

4. **🟠 High Priority** (Correctness):
   - [11_any_propagation_fix.md](11_any_propagation_fix.md) - Correctness fix
   - [12_cycle_detection_fix.md](12_cycle_detection_fix.md) - Correctness fix

5. **🟠 High Priority** (Cleanup):
   - [15_type_identity_migration.md](15_type_identity_migration.md) - Consistency
   - [16_binder_checker_overlap.md](16_binder_checker_overlap.md) - Architecture clarity

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
