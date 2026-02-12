# Slice 3, Session 3: ES5 Iterator Mode Destructuring

**Date**: 2026-02-12
**Goal**: Fix destructuring in for-of loops with --downlevelIteration
**Pass Rate**: 88.2% → 90.2% (45/51 → 46/51)

## Summary

Fixed destructuring patterns in for-of loops when using `--downlevelIteration`. Without pre-registering loop variables before emitting, destructuring patterns were emitted as raw ES6 syntax instead of being lowered to ES5 with `__read` helper.

## The Bug

```typescript
// @downlevelIteration: true
for (let [a = 0, b = 1] of [2, 3]) {
    a; b;
}
```

**Before** (incorrect - raw ES6 syntax):
```javascript
try {
    for (var _b = __values([2, 3]), _c = _b.next(); !_c.done; _c = _b.next()) {
        var [a = 0, b = 1] = _c.value;  // WRONG: Not lowered!
        a; b;
    }
}
```

**After** (correct - properly lowered to ES5):
```javascript
try {
    for (var _b = __values([2, 3]), _c = _b.next(); !_c.done; _c = _b.next()) {
        var _d = __read(_c.value, 2),
            _e = _d[0], a = _e === void 0 ? 0 : _e,
            _f = _d[1], b = _f === void 0 ? 1 : _f;
        a; b;
    }
}
```

## Root Cause

In Session 2, we added pre-registration for loop variables in array indexing mode (`emit_for_of_statement_es5_array_indexing`), but forgot to add it to iterator mode (`emit_for_of_statement_es5_iterator`).

Without pre-registration:
1. The scope is entered
2. Variable binding is emitted immediately
3. Destructuring patterns go through the normal emit path
4. The normal path doesn't trigger ES5 lowering because variables aren't registered yet

With pre-registration:
1. The scope is entered
2. Loop variables are pre-registered (triggers shadowing/renaming logic)
3. Variable binding emission now knows about the variables
4. Destructuring patterns are properly lowered via `emit_es5_destructuring_with_read`

## The Fix

Added pre-registration call in `emit_for_of_statement_es5_iterator`:

```rust
// Enter a new scope for the loop body to track variable shadowing
self.ctx.block_scope_state.enter_scope();

// Pre-register loop variables before emitting (needed for shadowing)
// Note: We only pre-register for VARIABLE_DECLARATION_LIST nodes, not assignment targets
self.pre_register_for_of_loop_variable(for_in_of.initializer);  // NEW

// Emit the value binding: var item = _c.value;
self.emit_for_of_value_binding_iterator_es5(for_in_of.initializer, &loop_result_name);
```

This mirrors the logic already present in `emit_for_of_statement_es5_array_indexing()` from Session 1.

## Changes Made

- `crates/tsz-emitter/src/emitter/es5_bindings.rs`:
  - Added `pre_register_for_of_loop_variable()` call in `emit_for_of_statement_es5_iterator()`
  - Ensures both array indexing and iterator protocol modes handle destructuring consistently

## Test Results

- **Fixed**: ES5For-of36 (destructuring with defaults in --downlevelIteration mode)
- **Pass rate**: 90.2% (46/51 ES5For-of tests)
- **Improvement**: +1 test (45 → 46), +2.0% pass rate
- **All unit tests pass**: 233/233 in tsz-emitter

## Overall Session Progress

### Session 1: Array Initializer Shadowing
- Pass rate: 82.0% (41/50)
- Fixed: ES5For-of17
- Key insight: Pre-register variables before emitting initialization expressions

### Session 2: Loop Body Shadowing & Temp Conflicts
- Pass rate: 88.2% (45/51)
- Fixed: ES5For-of11, ES5For-of24, ES5For-ofTypeCheck11, ES5For-ofTypeCheck8
- Key insight: Track temp variables to avoid rename collisions

### Session 3: Iterator Mode Destructuring (this session)
- Pass rate: 90.2% (46/51)
- Fixed: ES5For-of36
- Key insight: Apply pre-registration consistently across both lowering modes

### Total Improvement
- **Starting**: 82.0% (41/50)
- **Ending**: 90.2% (46/51)
- **Delta**: +8.2% pass rate, +5 tests fixed

## Remaining Issues

5 tests still failing (9.8% failure rate):

- **ES5For-of31** (+3/-3 lines): Temp variable naming differences
- **ES5For-of34** (+1/-1 lines): Variable hoisting (helper vars not at function top)
- **ES5For-of35** (+16/-0 lines): Large diff, likely complex destructuring or transform
- **ES5For-of37** (+8/-7 lines): Complex pattern, possibly nested control flow
- **ES5For-ofTypeCheck10** (+1/-4 lines): Formatting issue (object literal multiline)

These are either:
- Formatting issues (Slice 2 territory - multiline object literals)
- Variable hoisting issues (requires more complex analysis)
- Temp variable naming mismatches (cosmetic, no functional impact)

## Next Steps

The remaining failures are largely out of Slice 3 scope:

1. **ES5For-ofTypeCheck10**: Formatting issue (Slice 2 - multiline object literals)
2. **ES5For-of31, ES5For-of34**: Temp variable naming or hoisting (low priority)
3. **ES5For-of35, ES5For-of37**: Complex patterns requiring investigation

Slice 3 goals (ES5 destructuring/for-of lowering) are largely complete:
- ✅ Variable shadowing in array initializers
- ✅ Variable shadowing in loop bodies
- ✅ Temp variable conflict resolution
- ✅ Destructuring lowering in both modes (array & iterator)
- ✅ Default values in destructuring patterns

The 90.2% pass rate represents solid ES5 lowering behavior. Remaining issues are edge cases and formatting concerns.

## Commits

1. `fix(emit): ES5 for-of variable shadowing in array initializers` (Session 1)
2. `fix(emit): ES5 variable declarations with shadowing and temp variable conflicts` (Session 2)
3. `fix(emit): pre-register loop variables for ES5 iterator mode for-of` (Session 3)
