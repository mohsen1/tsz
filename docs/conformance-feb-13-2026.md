# Conformance Test Results - February 13, 2026

## Summary

**Pass Rate**: 429/499 tests (86.0%)
**Test Time**: 55.1s

## Recent Progress

### ✅ Array Predicate Narrowing (Completed)
- Implemented `arr.every(predicate)` type narrowing
- All 2394 unit tests pass
- Reduces TS2339 false positives

## Error Code Analysis

### Top Missing Errors (We Should Emit But Don't)

| Code | Count | Description | Impact |
|------|-------|-------------|--------|
| TS2322 | 11 | Type X is not assignable to Y | High - core type checking |
| TS2304 | 5 | Cannot find name | Medium - name resolution |
| TS2339 | 2 | Property does not exist | Low - reduced by array predicate work |

### Top Extra Errors (We Emit But Shouldn't)

| Code | Count | Description | Likely Cause |
|------|-------|-------------|--------------|
| TS2345 | 8 | Argument type not assignable | Generic inference / contextual typing |
| TS2769 | 6 | No overload matches | Overload resolution issues |
| TS2322 | 6 | Type not assignable | Type inference / widening |
| TS1109 | 5 | Expression expected | Parser issue |
| TS7006 | 4 | Parameter has implicit any | Contextual typing for lambdas |
| TS2339 | 4 | Property does not exist | Type narrowing edge cases |

## High-Impact Next Priorities

### 1. TS2304 - Cannot Find Name (5 missing)
**Why**: Clear signal of missing name resolution logic
**Effort**: Low-Medium
**Files**:
- `crates/tsz-checker/src/type_computation_complex.rs` - identifier resolution
- Check if we're missing global/ambient declarations

### 2. TS7006 - Implicit Any Parameters (4 extra)
**Why**: False positives hurt developer experience
**Effort**: Medium
**Cause**: Contextual typing not propagating to lambda parameters
**Files**:
- `crates/tsz-solver/src/contextual.rs` - contextual type inference
- Check overloaded function type handling

### 3. TS2345 False Positives (8 extra)
**Why**: Most common false positive
**Effort**: High - requires deep generic inference work
**Cause**: Generic type parameter inference differences vs TSC
**Files**:
- `crates/tsz-solver/src/infer.rs` - type parameter inference
- `crates/tsz-checker/src/call_checker.rs` - argument checking

### 4. TS1109 - Expression Expected (5 extra)
**Why**: Parser issue - should be easy to fix
**Effort**: Low
**Cause**: Likely parsing edge case (bigint properties, etc.)
**Files**:
- `crates/tsz-parser/` - parser logic
- Check syntax error reporting

## Recommendations

### Immediate (This Session)
1. **TS1109 Parser Issue** - Quick win, low effort
   - Investigate bigint property name parsing
   - Check for ASI edge cases

### Next Session
2. **TS7006 Contextual Typing** - Medium effort, high DX impact
   - Fix lambda parameter inference for overloaded targets
   - Should reduce 4+ false positives

3. **TS2304 Name Resolution** - Low-medium effort
   - Add missing global/ambient name handling
   - Should fix 5 missing errors

### Long-term
4. **Generic Inference** - High effort, high impact
   - TS2345 and TS2769 are complex generic inference issues
   - Requires systematic solver work
   - Defer until other priorities done

## Test Categories

### Well-Covered Areas ✅
- Basic type checking (2322, 2339 mostly working)
- Control flow narrowing (improved with array predicates)
- Function call checking (baseline functional)

### Needs Work ⚠️
- Generic inference edge cases (multi-signature, higher-order)
- Contextual typing for complex function types
- Some parser edge cases (bigint literals in certain positions)

### Known Gaps 🔴
- Conditional type evaluation (some complex cases)
- Mapped type recursive inference (some cases)
- Advanced generic constraints

## Next Steps

1. Run focused tests: `./scripts/conformance.sh run --error-code=1109` to debug parser issue
2. Create minimal reproductions for TS7006 cases
3. Check if TS2304 cases are all in same pattern (ambient globals, etc.)
4. After fixing 1-3, re-run full suite and update this doc
