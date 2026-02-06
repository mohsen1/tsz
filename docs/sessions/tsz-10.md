# Session TSZ-10: Flow Narrowing Investigation - Complex

**Started**: 2026-02-06
**Status**: ⚠️ COMPLEX - DEFERRED
**Predecessor**: TSZ-9 (Enum Arithmetic - Complete)

## Investigation Summary

This session investigated **Flow Narrowing for Element Access** - ensuring that `obj["prop"]` and `obj.prop` share narrowing.

## Findings

### Current Implementation ✅ Already Exists

The `is_matching_property_reference` function in `src/checker/control_flow_narrowing.rs` (lines 1421-1472) already handles:
- **Property access**: `obj.prop` (line 1451)
- **Element access**: `obj["prop"]` (line 1462)
- **Name extraction**: Both paths extract the property name and return `(base, name)`
- **Matching**: `is_matching_property_reference` compares base and name for equality

### The Problem ❌ Complex

Both test directions fail:
1. `obj["prop"]` → `obj.prop` - FAIL
2. `obj.prop` → `obj["prop"]` - FAIL

The reference matching logic appears correct, but narrowing is not being applied across different access forms. This suggests the issue is in:
1. **How narrowing is stored**: Narrowing entries need to be keyed in a way that both access forms can find
2. **How narrowing is looked up**: `apply_flow_narrowing` needs to recognize equivalent references
3. **CFA graph structure**: The control flow graph may not be tracking cross-form access

### Root Cause Analysis

The narrowing infrastructure is complex and involves:
- `src/checker/flow_analysis.rs` - Control flow graph construction
- `src/checker/control_flow_narrowing.rs` - Narrowing logic
- `src/checker/flow_narrowing.rs` - Additional narrowing
- `src/checker/type_computation.rs` - Type resolution with narrowing

This is a **multi-file architectural issue**, not a simple bug fix.

## Recommendation

**DEFER this session** - Flow narrowing for element access requires:
1. Deep understanding of CFA architecture
2. Changes to how narrowing is keyed/stored
3. Changes to how narrowing is looked up across equivalent references
4. Extensive testing to avoid regressions

Estimated effort: 4-8 hours of focused work with multiple iterations.

## Alternative: High-Impact Quick Wins

Given current test status (8232 passing, 68 failing), consider:
1. **Readonly array tests** (~3 tests) - May be simpler fixes
2. **Overload resolution** (~3 tests) - Function overload selection
3. **Module resolution** (~4 tests) - Barrel files and reexports

These may be more straightforward fixes with clearer paths.

## Test Status

**Start**: 8232 passing, 68 failing
**End**: 8232 passing, 68 failing
**Result**: No change - issue is complex

## Conclusion

Flow narrowing for element access is **architecturally complex**. The reference matching logic exists but narrowing doesn't transfer between access forms. Fixing this requires deep CFA infrastructure work beyond the scope of a simple session.

Recommendation: Pivot to simpler, higher-impact fixes (readonly, overload, modules).
