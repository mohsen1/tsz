# Contextual Typing for Generic Function Parameters (TS7006)

## Status: Not implemented

## Problem
223 extra TS7006 "Parameter implicitly has 'any' type" errors in conformance tests. When a function expression/arrow is assigned to a generic function type like `<T>(x: T) => void`, the parameter should get its type from contextual typing. Currently, the parameter type is not inferred from the contextual type, so it falls back to implicit `any`.

## Examples
```typescript
// TS7006 should NOT be emitted here (t should be contextually typed as T)
const fn2: <T>(x: T) => void = function test(t) { };

// Also affects callback parameters with complex contextual types
declare function f(fun: <T>(t: T) => void): void;
f(t => { /* t should be T, not any */ });
```

## Impact
~223 false positive TS7006 errors across 247 conformance test files.

## Fix Approach
In the checker's function expression/arrow type resolution, when noImplicitAny is enabled and a parameter has no type annotation, check for a contextual type. If the contextual type is a generic function type, propagate the type parameter to the parameter.

## Files
- `crates/tsz-checker/src/state_type_analysis.rs` — `compute_type_of_symbol` for function parameters
- `crates/tsz-checker/src/state_checking.rs` — parameter type checking
