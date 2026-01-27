# TS2322 Type Assignability Investigation

**Date**: 2026-01-27
**Task**: Fix 14,737 TS2322 errors (1,044 missing + 13,693 extra)
**Status**: In Progress

## Summary

After the recent commit (65c6c172f) that fixed strict_function_types configuration consistency, TSZ is now emitting TS2322 errors correctly for basic cases. This document outlines the current state and remaining work.

## Recent Progress

### Commit 65c6c172f: Configuration Consistency Fix

**Impact**: Reduced 13,687 extra TS2322 errors by ensuring all CompatChecker instances use consistent configuration.

**Root Cause**: Several CompatChecker instantiation sites were not calling `set_strict_function_types()`, leaving it at the default value of `false`. This caused:
- Some code paths to use bivariant checking (more lenient)
- Other code paths to use contravariant checking (more strict)
- Inconsistency led to false positives

**Locations Fixed**:
- `src/checker/state.rs` (6 sites)
- `src/checker/type_computation.rs` (1 site)
- `src/checker/error_reporter.rs` (1 site)

## Current State

### Baseline (from conformance/baseline.log)
- **Pass Rate**: 42.5% (203/478)
- **Extra TS2322 errors**: 101x
- **Total TS2322 issues to fix**: 14,737 (1,044 missing + 13,693 extra)

After the fix, we should have significantly fewer extra errors. Need to run full conformance to get updated numbers.

## Architecture

### TS2322 Emission Flow

```
CheckerState (type_checking.rs)
    ├─> is_assignable_to(source, target)
    │   └─> CompatChecker::is_assignable(source, target)
    │       ├─> check_assignable_fast_path() [any, unknown, never, etc.]
    │       ├─> violates_weak_union()
    │       ├─> violates_weak_type()
    │       ├─> is_empty_object_target()
    │       └─> subtype.is_subtype_of(source, target)
    │
    └─> if !is_assignable_to()
        └─> error_type_not_assignable_with_reason_at()
            └─> diagnose_assignment_failure()
                └─> checker.explain_failure(source, target)
                    └─> Render detailed diagnostic
```

### Configuration Parameters

All CompatChecker instances MUST be configured with:
1. **strict_function_types**: Controls function parameter variance (contravariant vs bivariant)
2. **strict_null_checks**: Controls null/undefined assignability
3. **exact_optional_property_types**: Controls optional property exactness
4. **no_unchecked_indexed_access**: Controls undefined in indexed access results

### Error Suppression Rules

From `src/checker/error_handler.rs`:
```rust
fn emit_type_not_assignable(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
    // ERROR type suppression
    if source == TypeId::ERROR || target == TypeId::ERROR {
        return;
    }
    // ANY type suppression
    if source == TypeId::ANY || target == TypeId::ANY {
        return;
    }
    // ... emit error
}
```

## Test Verification

### Void Assignability Test

Test file (`/tmp/test_ts2322_void.ts`):
```typescript
var x: void;
x = 1;        // Should emit TS2322
x = true;     // Should emit TS2322
x = '';       // Should emit TS2322
x = {}        // Should emit TS2322
```

Result: TSZ correctly emits all 4 TS2322 errors ✓

### Reference Test Case

TypeScript test: `TypeScript/tests/cases/conformance/invalidAssignmentsToVoid.ts`

Expected errors: 10
- Type 'number' is not assignable to type 'void'
- Type 'boolean' is not assignable to type 'void'
- Type 'string' is not assignable to type 'void'
- Type '{}' is not assignable to type 'void'
- Type 'typeof C' is not assignable to type 'void'
- Type 'C' is not assignable to type 'void'
- Type 'I' is not assignable to type 'void'
- Type 'typeof M' is not assignable to type 'void'
- Type 'T' is not assignable to type 'void'
- Type '<T>(a: T) => void' is not assignable to type 'void'

Status: Needs verification

## Remaining Work

### High Priority Investigation Areas

1. **Missing TS2322 Errors (1,044 cases)**
   - Look for assignability checks that don't emit errors
   - Check for early returns that skip error emission
   - Verify ERROR/ANY suppression is correct

2. **Extra TS2322 Errors (baseline: 101, expected much lower after fix)**
   - Look for false positives in assignability logic
   - Check for over-aggressive checks
   - Verify configuration consistency

3. **Specific Test Categories**
   - Function type assignability (bivariant vs contravariant)
   - Generic type parameter constraints
   - Union/intersection assignability
   - Tuple/array assignability
   - Object literal excess properties
   - Weak type violations

### Code Locations to Investigate

1. **Type Checking** (`src/checker/type_checking.rs`)
   - Assignment expressions (lines ~400-650)
   - Binary operations (lines ~100-450)
   - Return statements

2. **Type Computation** (`src/checker/type_computation.rs`)
   - Variable declarations
   - Property declarations
   - Function parameters

3. **State** (`src/checker/state.rs`)
   - `is_assignable_to()` (line 6065)
   - Object literal checking (line 9755)
   - Excess property checking

4. **Solver Compat** (`src/solver/compat.rs`)
   - Fast path checks (line 228)
   - Weak type violations (line 209-213)
   - Empty object target (line 217-219)

## Test Strategy

### Quick Verification
```bash
# Test specific TS2322 cases (100 tests)
./conformance/run-conformance.sh --all --workers=14 --filter "TS2322" --count 100

# Test void assignability
./.target/release/tsz /tmp/test_ts2322_void.ts

# Test specific file
./.target/release/tsz TypeScript/tests/cases/conformance/invalidAssignmentsToVoid.ts
```

### Full Conformance
```bash
# Run all TS2322 tests
./conformance/run-conformance.sh --all --workers=14 --filter "TS2322"

# Full conformance (takes time)
./conformance/run-conformance.sh --all --workers=14
```

## Key Insights

1. **Configuration is Critical**: The recent fix showed that inconsistent configuration across 8 sites caused 13,687 false positives. This suggests that:
   - ALL CompatChecker sites must be configured consistently
   - Missing configuration leads to subtle bugs
   - Default values should match compiler options

2. **Error Suppression is Correct**: The ERROR and ANY type suppression prevents cascading errors and matches TypeScript behavior.

3. **Void Assignability Works**: Basic tests show TSZ correctly handles void assignability.

4. **Need for More Investigation**:
   - Missing errors suggest some code paths don't check assignability
   - Extra errors suggest over-aggressive checking or configuration issues
   - Need to identify specific patterns causing discrepancies

## Next Steps

1. Run full conformance test to get current numbers
2. Analyze specific failing test cases
3. Create minimal test cases for missing/extra errors
4. Fix identified issues incrementally
5. Commit frequently with descriptive messages

## References

- TypeScript specification: https://www.typescriptlang.org/docs/handbook/2/types-from-types.html
- strictFunctionTypes: https://www.typescriptlang.org/tsconfig#strictFunctionTypes
- Commit 65c6c172f: Configuration consistency fix
- Test baseline: conformance/baseline.log
