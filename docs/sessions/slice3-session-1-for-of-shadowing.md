# Slice 3, Session 1: ES5 For-Of Variable Shadowing

**Date**: 2026-02-12
**Goal**: Fix ES5 for-of destructuring/lowering failures
**Pass Rate**: 41/50 → 43/51 (82.0% → 84.3%)

## Summary

Fixed variable shadowing bug in ES5 for-of array initialization expressions. When a for-of loop variable shadows an outer variable with the same name, references to the outer variable in the array initializer must be renamed to avoid conflicts with the hoisted loop variable declaration.

## The Bug

When emitting ES5 for-of loops with nested shadowing:
```typescript
for (let v of []) {
    for (let v of [v]) {  // Inner v shadows outer v
        var x = v;
    }
}
```

**Before** (incorrect):
```javascript
for (var _b = 0, _c = [v]; _b < _c.length; _b++) {  // Wrong: uses 'v'
    var v_1 = _c[_b];
}
```

**After** (correct):
```javascript
for (var _b = 0, _c = [v_1]; _b < _c.length; _b++) {  // Correct: uses 'v_1'
    var v_1 = _c[_b];
}
```

## Root Cause

The emitter was:
1. Emitting the array initialization `_c = [v]` first
2. Then entering the loop body scope
3. Then registering the loop variable `v` as `v_1`

By the time we registered `v_1`, we'd already emitted `[v]`, so the identifier didn't get renamed.

## The Fix

Pre-register the loop variable **before** emitting the for loop header:

```rust
// BEFORE emitting `for (var _i = 0, _c = [v]; ...)`
self.ctx.block_scope_state.enter_scope();
self.pre_register_for_of_loop_variable(for_in_of.initializer);

// NOW emit the for loop header - identifiers will resolve to renamed variables
self.write("for (var ");
self.write(&index_name);
self.write(" = 0, ");
self.write(&array_name);
self.write(" = ");
self.emit_expression(for_in_of.expression);  // <-- [v] becomes [v_1]
```

The helper `pre_register_for_of_loop_variable()` walks the loop variable binding (simple identifier or destructuring pattern) and registers all names in the current scope. This ensures that when we emit the array expression `[v]`, the identifier resolution sees the renamed variable.

## Changes Made

- `crates/tsz-emitter/src/emitter/es5_bindings.rs`:
  - Added `pre_register_for_of_loop_variable()` to scan loop bindings before emission
  - Added `pre_register_binding_name()` to recursively register identifiers in patterns
  - Modified `emit_for_of_statement_es5_array_indexing()` to enter scope and pre-register before emitting loop header
  - Changed loop variable emission to use `get_emitted_name()` instead of `register_variable()` (prevents double-registration)

## Test Results

- **Fixed**: ES5For-of17 (variable shadowing in array initializer)
- **Pass rate**: 84.3% (43/51 ES5For-of tests)
- **All unit tests pass**: 233/233 in tsz-emitter

## Known Remaining Issues

The fix doesn't address all ES5 shadowing scenarios:

### 1. Multiple shadowing levels (ES5For-of20)
```typescript
for (let v of []) {
    let v;              // Creates v_1
    for (let v of [v]) {  // Should use v_2, but we emit v_1
        const v;        // Should be v_3, but we emit v_1 (duplicate!)
    }
}
```

**Problem**: When there are multiple shadowing levels before the for-of loop, our incremental registration doesn't match TypeScript's upfront analysis. TypeScript knows all bindings before emission; we discover them incrementally.

### 2. Loop body variable declarations (ES5For-of24, ES5For-of31, ES5For-of34, ES5For-of35)
```typescript
var a = [1, 2, 3];
for (var v of a) {
    let a = 0;  // Should be renamed to a_2 (a_1 is the array temp)
}
```

**Problem**: Regular `let`/`const` declarations inside the loop body aren't being registered with the block scope state at all. The ES5 variable declaration emitter (`emit_variable_declaration_list_es5`) doesn't call `register_variable()`, so shadowing isn't detected.

**Potential Fix**: Make `emit_variable_declaration_list_es5()` register variables as it emits them, similar to the for-of loop variable fix. This is a larger change affecting all ES5 variable declarations, not just for-of loops.

## Commit

```
fix(emit): ES5 for-of variable shadowing in array initializers

Fixes ES5For-of17 emit test (improves pass rate from 41/50 to 42/50).
```

## Next Steps

For future sessions:

1. **Fix loop body variable shadowing** (ES5For-of24, ES5For-of31, ES5For-of34, ES5For-of35):
   - Modify `emit_variable_declaration_list_es5()` to register variables in block scope state
   - Ensure identifier emission checks scope state for all ES5 variable declarations

2. **Fix multiple shadowing levels** (ES5For-of20):
   - Requires more sophisticated scope analysis or upfront binding pass
   - May need to scan entire block before emitting to match TypeScript's behavior

3. **Other ES5For-of failures** (ES5For-of37, ES5For-of36, ES5For-ofTypeCheck10):
   - Investigate specific failure patterns
   - May involve destructuring, async/await, or other ES5 lowering issues
