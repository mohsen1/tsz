# Session TSZ-10: Flow Narrowing for Element Access

**Started**: 2026-02-06
**Status**: 🔄 ACTIVE
**Predecessor**: TSZ-9 (Enum Arithmetic - Complete)

## Task

Fix **Flow Narrowing for Element Access** - CFA should narrow types when accessing properties via bracket notation.

## Problem Statement

Tests failing for flow narrowing with element access:
- `test_flow_narrowing_applies_across_element_to_property_access`
- `test_flow_narrowing_applies_for_computed_element_access_*`

**Specific issue**: When a variable is narrowed via control flow, accessing properties via `obj["prop"]` or `obj[prop]` should use the narrowed type, but currently emits TS2339 errors.

## Expected Impact

- **Direct**: Fix ~5 flow narrowing tests
- **CFA**: Improve control flow analysis for element access
- **Type Safety**: Better narrowing with bracket notation

## Implementation Plan

### Phase 1: Investigate
1. Check `src/checker/control_flow_narrowing.rs` - element access handling
2. Review `src/checker/type_computation.rs` - get_type_of_element_access
3. Examine how narrowing is applied to property access

### Phase 2: Fix
1. Ensure element access uses narrowed type from CFA
2. Apply narrowing to computed property access
3. Handle literal keys and expression keys

### Phase 3: Test
1. Run all flow narrowing tests
2. Verify element access narrowing works
3. Check for regressions

## Files to Modify

- `src/checker/control_flow_narrowing.rs` - Element access narrowing
- `src/checker/type_computation.rs` - Type computation integration

## Test Status

**Start**: 8232 passing, 68 failing
**Target**: ~8237 passing (+5 tests)

## Next Steps

1. Investigate current implementation
2. Ask Gemini for approach validation (Question 1)
3. Implement based on guidance
4. Ask Gemini for implementation review (Question 2)
