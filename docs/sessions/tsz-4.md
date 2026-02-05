# Session tsz-4: Solver Lawyer Layer & strictNullChecks

**Status**: Active
**Priority**: 4 (High - Type System)
**Focus**: Verifying and tuning the Lawyer Layer implementation

## Problem Statement

TSZ is TOO STRICT in type checking, producing errors when `tsc` doesn't:
- **TS18050** (172 extra): "The value X cannot be used here" (null/undefined)
- **TS2322** (554 extra): "Type is not assignable to type"
- **TS2345** (387 extra): "Argument of type X is not assignable to parameter of type Y"

## Current Status (2026-02-05)

### Completed ✅

1. **TS18050 strictNullChecks Gating** (Partial)
   - Fixed `emit_binary_operator_error` to gate TS18050 for null/undefined operands
   - Fixed `report_nullish_object` to skip possibly-nullish errors in non-strict mode
   - Fixed `get_type_of_property_access_expression` similarly
   - Known issue: Error code selection for literal null in property access

2. **Lawyer Layer Verification** (Complete)
   - **Step 2 (Any propagation)**: Already implemented in `src/solver/subtype.rs` lines 734-759
   - **Step 3 (Method bivariance)**: Already implemented in `src/solver/subtype_rules/functions.rs`
   - **Step 4 (Void return exception)**: Already implemented in `src/solver/subtype_rules/functions.rs`
   - All features tested and confirmed working correctly

3. **Wiring Verification** (Complete)
   - Checker correctly calls `is_assignable_to` (CompatChecker)
   - CompatChecker uses `AnyPropagationRules` with legacy mode enabled by default
   - Fast path returns `true` if either side is `any`

4. **Testing** (Complete)
   - Created and tested multiple `any` propagation scenarios
   - All tests match `tsc` behavior
   - Lawyer Layer is functioning as expected

### In Progress 🔲

1. **Conformance Baseline** (Next Priority)
   - Need to run actual conformance tests to establish real error delta
   - The "~900 extra errors" from original plan may be outdated assumptions

2. **Configuration Verification**
   - Verify `AnyPropagationRules` correctly toggles based on `CompilerOptions`
   - Test with `strictNullChecks: true` vs `false`

3. **Error Code Selection** (Lower Priority)
   - Fix TS18050 vs TS2531 selection for literal null in property access

## Files Modified

- `src/checker/type_checking_queries.rs` - `report_nullish_object()`
- `src/checker/error_reporter.rs` - `emit_binary_operator_error()`
- `src/checker/function_type.rs` - `get_type_of_property_access_expression()`

## Coordination Points

- **TSZ-3 (Contextual Typing)**: Changes to `any` propagation may affect type inference from context
- **TSZ-1 (Modules)**, **TSZ-2 (Parser)**: No direct conflicts

## Test Results

### Working Correctly ✅
```typescript
// any propagation
const x: any = {};
const y: number = x;  // OK - matches tsc

// method bivariance
interface Derived { method(x: string | number): void; }

// void return
const returnsString: () => string = () => "hello";
const expectsVoid: () => void = returnsString;  // OK - matches tsc
```

### Known Issues ⚠️
- `null.toString()` without strictNullChecks emits TS2531 (tsc emits TS18050)
- Type inference for `const x = null` without strictNullChecks needs work

## Next Steps

1. **Run conformance tests** - Establish real baseline (not assumptions)
2. **Verify configuration propagation** - Ensure flags reach CompatChecker correctly
3. **Trace specific failing cases** - If errors exist, understand root cause
4. **Compare with tsc** - Determine if tsz is actually wrong

## History

- **2026-02-05**: Implemented TS18050 gating; verified Lawyer Layer features working
- **2026-02-05**: Verified wiring; tested `any` propagation scenarios
- **2026-02-05**: Discovered Steps 2-4 already implemented (any, bivariance, void return)
