# Slice 4 Session: Object Destructuring __read Helper Fix

## Date
2026-02-12

## Summary
Fixed incorrect emission of `__read` helper for object destructuring in for-of loops with `--downlevelIteration`.

## Problem
Test ES5For-of35 was failing because we emitted the `__read` helper for object destructuring patterns, when it should only be emitted for array destructuring patterns.

### Root Cause
In `crates/tsz-emitter/src/lowering_pass.rs`, the function `for_of_initializer_has_binding_pattern` was returning `true` for both `ARRAY_BINDING_PATTERN` and `OBJECT_BINDING_PATTERN`:

```rust
if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
    || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
{
    return true;
}
```

This caused `helpers.read = true` to be set even for object destructuring, which doesn't need the `__read` helper.

### Why __read is Only for Arrays
- **Array destructuring**: `[a, b] = iter` needs `__read(iter, 2)` to convert iterator result to an array and extract elements by index
- **Object destructuring**: `{x: a} = obj` becomes `a = obj.x`, accessing properties by name, no helper needed

## Solution
Changed the check to only return `true` for `ARRAY_BINDING_PATTERN`:

```rust
// Check if name is an ARRAY binding pattern
// __read helper is only needed for array destructuring, not object destructuring
// Object destructuring accesses properties by name, not by iterator position
if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
    return true;
}
```

## Test Results

### Before Fix
```typescript
// Input: ES5For-of35.ts
for (const {x: a = 0, y: b = 1} of [2, 3]) { a; b; }

// Output: Incorrectly emitted __read helper (not used in code)
var __values = ...
var __read = ...  // ← Unnecessary!
for (var _b = __values([2, 3]), _c = _b.next(); !_c.done; _c = _b.next()) {
    var _d = _c.value, _e = _d.x, a = ..., _f = _d.y, b = ...;
}
```

### After Fix
```typescript
// Output: Only __values helper emitted
var __values = ...  // Only needed helper
for (var _b = __values([2, 3]), _c = _b.next(); !_c.done; _c = _b.next()) {
    var _d = _c.value, _e = _d.x, a = ..., _f = _d.y, b = ...;
}
```

### Pass Rate
- **300-test sample**: 79.9% (207/259)
- **Target**: 90%+
- **Gap**: 10.1 percentage points (26 tests)

## Tests Fixed
- ✅ ES5For-of35 (object destructuring in for-of)

## Tests Still Failing (Slice 4 Area)
- ES5For-of31, ES5For-of34, ES5For-of37 (likely Slice 3 - variable renaming)
- No obvious remaining Slice 4 issues found in 1000-test sample

## Files Modified
- `crates/tsz-emitter/src/lowering_pass.rs` (lines 1901-1906)

## Commits
- `7231994ce` - fix(emit): only emit __read helper for array destructuring, not object destructuring

## Remaining Work for Slice 4
Most remaining failures are in other slices:
- **Slice 1**: Comment preservation (line comments, inline comments)
- **Slice 2**: Object literal formatting (multiline vs single-line)
- **Slice 3**: Variable renaming (shadowed variables need suffixes)

### Potential Slice 4 Work
- Arrow function `_this` capture (documented in previous sessions, requires architectural changes)
- Any other helper function edge cases (none found in current test sample)

## Verification
All 233 emitter unit tests pass. No regressions detected.
