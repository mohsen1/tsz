# Session TSZ-10: Advanced Control Flow Analysis (CFA) & Narrowing

**Started**: 2026-02-05
**Status**: ✅ COMPLETED (Pivoted to TSZ-11 for integration work)
**Ended**: 2026-02-05

## Session Scope

### Problem Statement

TypeScript's type system becomes significantly more powerful with control flow analysis. The compiler can narrow types based on conditionals, type guards, truthiness checks, assertion functions, and discriminant unions.

## Outcome

**Status**: Pivoted / Partially Complete - Infrastructure Implemented, Integration Deferred

### Achievements ✅

1. **Task 1: Truthiness & Typeof Narrowing** - SUBSTANTIAL PROGRESS
   - Fixed typeof inequality narrowing bug (!= and !== operators)
   - Verified typeof with any/unknown works correctly
   - Verified basic truthiness narrowing works (null/undefined/void removal)
   - Fixed "Missing Type Resolution" bug (enables type alias narrowing)

2. **Task 4: Assertion Functions Integration** - ✅ COMPLETED
   - Assertion functions are properly integrated into the narrowing infrastructure

3. **Task 5.1-5.2: Discriminant Union Refinement** - ✅ COMPLETED
   - Implemented TypeResolver injection into NarrowingContext
   - Fixed Lazy/Intersection resolution bugs in narrowing
   - Simple type alias narrowing now works
   - Added blanket impl for `&T` to fix `Sized` trait object error

4. **Task 2 Infrastructure** - ✅ COMPLETED
   - Wired TypeEnvironment resolver to `apply_type_predicate_narrowing()`
   - Wired TypeEnvironment resolver to `narrow_by_instanceof()`
   - Both functions use proper resolver pattern for type alias support

### Critical Discovery 🔍

**Blocker Identified**: FlowAnalyzer is not integrated into the main expression type checking path.

**Root Cause**:
- The narrowing CALCULATION logic is correct (in Solver)
- FlowAnalyzer correctly calculates narrowed types
- `narrow_to_type()` has proper subtype checking logic
- Flow graph correctly creates condition nodes
- **BUT**: Narrowed types are never CONSUMED by the Checker

**Technical Details**:
- `get_type_of_symbol()` (state_type_analysis.rs:751) uses a flow-insensitive cache
- Cache is keyed only by `SymbolId`, ignoring flow context
- FlowAnalyzer is created on-demand for specific checks only
- Main expression type checking path does not query FlowAnalyzer
- When checking `animal.bark()`, checker returns declared type `Animal` instead of narrowed type `Dog`

**Impact**: This is a SIGNIFICANT architectural change affecting how every identifier is resolved in the Checker.

### Commits

- `73e2ded5a` - fix(narrowing): wire TypeEnvironment resolver to instanceof narrowing
- `f1982b31d` - docs(tsz-10): update Task 2 status - narrowing infrastructure in place
- `2ebeefc93` - docs(tsz-10): document root cause of instanceof narrowing failure

## Next Steps

Integration of FlowAnalyzer moved to **Session TSZ-11: Control Flow Analysis Integration**.

This new session will:
1. Integrate FlowAnalyzer into the main Checker loop
2. Implement `get_flow_type_of_node` in flow_analysis module
3. Wire into `check_identifier` or equivalent in expr.rs
4. Verify instanceof and typeof narrowing end-to-end
5. Ensure no performance regression

## Rationale for Pivot

Mixing "completed narrowing logic" with "broken integration" in one session makes debugging difficult. The integration work is high-risk and high-impact, deserving its own isolated session to ensure it doesn't break existing type checking.

## Files Modified

- `src/checker/control_flow_narrowing.rs` - TypeResolver wiring
- `src/solver/subtype.rs` - Blanket impl for TypeResolver
- `src/solver/narrowing.rs` - TypeResolver field and with_resolver() method
- `src/checker/control_flow.rs` - TypeResolver wiring in narrow_type_by_condition_inner
- `src/solver/db.rs` - Fixed PropertyAccessEvaluator compilation error

## References

- North Star Architecture: docs/architecture/NORTH_STAR.md
- Previous Session: docs/sessions/tsz-09.md (if applicable)
- Next Session: docs/sessions/tsz-11.md (Control Flow Analysis Integration)
