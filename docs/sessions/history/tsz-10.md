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

## Later Session Work (2026-02-05)

### Task 2: instanceof Narrowing - COMPLETED ✅

**Implementation**: `narrow_by_instanceof` in `src/solver/narrowing.rs` (line ~751)

**Gemini Code Review Results**:
- Asked Gemini Pro to review the instanceof narrowing implementation
- Overall logic approved as correct
- **BUG FOUND**: `are_object_like` helper didn't handle TypeParameter
- Fixed by checking constraint recursively for type parameters

**Changes**:
- Added TypeParameter case to `are_object_like` function
- Added TODO comment for Symbol.hasInstance custom instanceof behavior
- Commit: `5736afc35` - fix(solver): handle TypeParameter in are_object_like

**Test Results**:
- Simple instanceof: Works correctly ✅
- Generic constrained types: Works correctly after fix ✅
- Compound conditions (`&&`): Known limitation in checker (separate issue)

**Known Limitations**:
- Compound conditions don't narrow both sides - checker issue, not solver
- Symbol.hasInstance not supported - added TODO for future work

### Task 6: Exhaustiveness Checking - ATTEMPTED, REVERTED ❌

**Initial Implementation**: Added diagnostic emission in `check_switch_exhaustiveness`

**Gemini Code Review Results**:
- Asked Gemini Pro to review the exhaustiveness checking implementation
- **CRITICAL BUGS FOUND** - Implementation was incorrect

**Issues Identified**:
1. **False positives**: Reported errors even when code after switch handles missing cases
   - Example: Fallthrough with return statement after switch
2. **Incorrect void check**: Used strict equality (`!=`) instead of assignability check
   - Failed for union return types like `number | undefined`
3. **Architectural error**: TS2366 should be reported at function level in CFA, not switch level
   - Cannot determine exhaustiveness by looking at individual switch in isolation
   - Must consider entire function body and code after switch

**Correct Approach** (per Gemini):
- `no_match_type` calculation is useful for narrowing infrastructure
- Error emission must happen in Control Flow Analysis at function level
- Must check if `undefined` is assignable to return type (using `is_assignable_to`)
- Must consider code after switch statement
- This requires deeper CFA integration than initially estimated

**Action**: Reverted diagnostic emission, kept narrowing infrastructure
**Commit**: `d63a638b0` - revert(checker): remove incorrect exhaustiveness diagnostic emission

**Conclusion**: Task 6 requires function-level flow analysis integration, not just switch-level checks. The narrowing infrastructure exists and works correctly, but the diagnostic emission is blocked on TSZ-11 CFA integration work.

## References

- North Star Architecture: docs/architecture/NORTH_STAR.md
- Previous Session: docs/sessions/tsz-09.md (if applicable)
- Next Session: docs/sessions/tsz-11.md (Control Flow Analysis Integration)
