# Contextual Typing in Non-Strict Mode

## Status
**FIXED** - Implemented and verified ✓

## Problem Summary
The test `contextualTypingOfLambdaWithMultipleSignatures2.ts` expects no errors with `@strict: false`, but tsz reports TS2322 and TS2339.

## Test Case
```typescript
// @strict: false
var f: {
    (x: string): string;
    (x: number): string
};

f = (a) => { return a.asdf }
```

## Expected Behavior (TSC)
- No errors reported
- Parameter `a` gets type `any` (NOT `string | number`!)
- Accessing `.asdf` on `any` is allowed, so no TS2339 error

## Current Behavior (tsz)
- TS2322: Type mismatch
- TS2339: Property 'asdf' does not exist on type 'number | string'

## Investigation

### Initial Hypothesis (INCORRECT)
Thought the issue was that property access should be lenient in non-strict mode (when `noImplicitAny: false`). Attempted fix:
- Modified property access handlers to return `any` without error when `!ctx.no_implicit_any()`
- This fixed the contextual typing test ✓
- But broke 9 other tests that expect TS2339 in non-strict mode ✗

### Revised Hypothesis
The issue is likely in **contextual typing** for lambda parameters, not property access error reporting.

In non-strict mode (`noImplicitAny: false`), TypeScript may:
1. Treat unannotated lambda parameters as `any` when strict checks are disabled
2. Or suppress contextual typing for unannotated parameters in non-strict mode
3. Or have special handling for overload resolution in non-strict mode

The key is that TypeScript doesn't apply contextual typing the same way in strict vs non-strict mode.

## Code Locations
- Contextual typing: `crates/tsz-solver/src/contextual.rs`
- Lambda parameter typing: `crates/tsz-checker/src/function_type.rs`
- Property access: `crates/tsz-checker/src/type_computation.rs`

## Next Steps
1. Test what TSC actually infers for `a` in the test case (is it `any` or `string | number`?)
2. Check if the issue is in how we apply contextual types to unannotated lambda parameters
3. Look for TypeScript's handling of `noImplicitAny` in contextual typing logic
4. Consider if overload resolution is different in non-strict mode

## Root Cause Analysis (2026-02-13)

### Actual TSC Behavior (Verified)
Testing revealed the actual behavior:
```bash
# Without noImplicitAny: parameter gets `any`
$ tsc --noImplicitAny false test.ts  # No errors, `a` is `any`

# With noImplicitAny: parameter gets union type
$ tsc --noImplicitAny test.ts  # Error: Property 'charAt' does not exist on type 'string | number'
```

### Bug Location
`crates/tsz-solver/src/contextual.rs`, lines 545-560:
```rust
fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
    // ...
    if param_types.len() == 1 {
        Some(param_types[0])
    } else {
        Some(self.db.union(param_types))  // ← BUG: Always creates union
    }
}
```

### The Fix
`ParameterExtractor` should check `noImplicitAny`:
- If `noImplicitAny: false` and types differ → return `None` (falls back to `any`)
- If `noImplicitAny: true` and types differ → return union

**Challenge:** `ParameterExtractor` operates on `TypeDatabase` which doesn't have compiler options access.

**Solution:** Add `no_implicit_any` parameter to the contextual typing API, passed from checker.

## Fix Implementation

The fix was implemented by threading the `no_implicit_any` compiler option through the contextual typing system:

1. **Added `no_implicit_any` field to `ContextualTypeContext`** (contextual.rs:710)
   - New constructor `with_expected_and_options` accepts the flag
   - Backward compatible: existing constructors default to `false`

2. **Modified `ParameterExtractor` to respect the flag** (contextual.rs:517)
   - Added `no_implicit_any` parameter to constructor
   - Updated `visit_callable` to check the flag:
     - If `no_implicit_any: false` and types differ → return `None` (falls back to `any`)
     - If `no_implicit_any: true` and types differ → return union type

3. **Updated all checker call sites** to pass `compiler_options.no_implicit_any`:
   - `function_type.rs:172` - Lambda parameter typing (key fix)
   - `state_checking_members.rs:2632` - Object literal methods
   - `type_computation_complex.rs:368, 1184` - Constructor and function calls
   - `type_computation.rs:187, 1943` - General contextual typing
   - `call_checker.rs:323, 383, 397` - Call argument checking

## Result
Test `contextualTypingOfLambdaWithMultipleSignatures2.ts` now passes:
- Expected: No errors in non-strict mode
- Actual: No errors ✓

## Related TypeScript Behavior
- [TypeScript Handbook - Type Inference](https://www.typescriptlang.org/docs/handbook/type-inference.html)
- [noImplicitAny flag documentation](https://www.typescriptlang.org/tsconfig#noImplicitAny)
- Contextual typing for function expressions depends on strict mode settings
