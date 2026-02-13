# Contextual Typing in Non-Strict Mode

## Status
**INVESTIGATING** - Initial fix attempt was too broad

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
- Parameter `a` gets type from contextual typing (should be `string | number` based on the overloads)
- Accessing `.asdf` on `a` should not error

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

## Related TypeScript Behavior
- [TypeScript Handbook - Type Inference](https://www.typescriptlang.org/docs/handbook/type-inference.html)
- [noImplicitAny flag documentation](https://www.typescriptlang.org/tsconfig#noImplicitAny)
- Contextual typing for function expressions depends on strict mode settings
