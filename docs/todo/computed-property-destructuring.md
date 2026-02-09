# Computed Property Destructuring Type Checking

## Status: Not implemented

## Problem
159 extra TS2349 "This expression is not callable" errors in conformance. Many are from computed property destructuring patterns where the computed key type is not properly resolved.

## Example
```typescript
let foo = "bar";
let {[foo]: bar} = {bar: "bar"}; // Should work, we may incorrectly check foo's callability
```

## Impact
~159 false positive TS2349 errors.

## Files
- `crates/tsz-checker/src/` — destructuring pattern handling
