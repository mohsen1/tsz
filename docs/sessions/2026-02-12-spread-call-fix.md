# Session 2026-02-12: Fix ES5 Spread in Function Calls

## Problem

The ES5 spread transformation for function call arguments was implemented but not working. Spread syntax like `foo(...arr, 3)` was not being transformed to the ES5-compatible `.apply()` pattern.

## Root Cause

The implementation was complete except for one critical piece: `CALL_EXPRESSION` was not added to the `kind_may_have_transform()` gate function in the emitter.

### Why This Matters

The emitter uses a two-stage pipeline for transforms:

1. **Gate Check** - `kind_may_have_transform()` quickly filters which node types should check for transforms (performance optimization to avoid HashMap lookups for every node)
2. **Transform Lookup** - Only if the gate passes does it look up and apply directives

Without `CALL_EXPRESSION` in the gate list, the transform directives were being created by the lowering pass but never checked by the emitter.

## Solution

**File: `crates/tsz-emitter/src/emitter/mod.rs`**

Added `CALL_EXPRESSION` to the `kind_may_have_transform()` function (line ~1428):

```rust
fn kind_may_have_transform(kind: u16) -> bool {
    matches!(
        kind,
        k if k == syntax_kind_ext::SOURCE_FILE
            // ... other kinds ...
            || k == syntax_kind_ext::CALL_EXPRESSION  // ← Added this line
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
    )
}
```

**File: `crates/tsz-emitter/src/lowering_pass.rs`**

Removed debug `eprintln!` statements from the spread detection code (lines 1427-1445). The project uses the `tracing` crate for debugging, not `eprintln!`.

**File: `crates/tsz-emitter/src/emitter/mod.rs`**

Fixed dereference issues in the `ES5CallSpread` handlers:
- In `apply_transform()` (line 884): field is owned, no dereference needed
- In `emit_chained_directive()` (line 1198): field is a reference, dereference needed

## Verification

All spread patterns now work correctly:

```typescript
// Input
foo(...arr, 3);
```

```javascript
// Output (ES5)
foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [3], false));
```

### Test Cases

- ✅ Single spread: `foo(...arr)`
- ✅ Spread with trailing args: `foo(...arr, 3, 4)`
- ✅ Spread with leading args: `foo(0, ...arr)`
- ✅ Multiple spreads: `foo(...arr, 3, ...arr2)`
- ✅ Method calls: `obj.method(...arr, 7)`

## Files Modified

1. `crates/tsz-emitter/src/emitter/mod.rs` - Added CALL_EXPRESSION to gate function, fixed dereferences
2. `crates/tsz-emitter/src/lowering_pass.rs` - Removed debug eprintln statements

## Implementation Details

The complete implementation includes:

1. **Lowering Pass** (`lowering_pass.rs:1427-1445`)
   - Detects spread elements in call arguments
   - Creates `ES5CallSpread` directive
   - Marks `spread_array` helper as needed

2. **Transform Handlers** (`es5_helpers.rs:1207-1385`)
   - `emit_call_expression_es5_spread()` - Main transformation
   - `emit_spread_args_array()` - Builds nested `__spreadArray` calls
   - Handles all spread patterns (leading, trailing, multiple)

3. **Emitter Integration** (`emitter/mod.rs`)
   - `kind_may_have_transform()` - Gate function (now includes CALL_EXPRESSION)
   - `emit_directive_from_transform()` - Directive conversion
   - `apply_transform()` - Owned directive handler
   - `emit_chained_directive()` - Reference directive handler

## Status

✅ Complete and working. ES5 spread transformation for function calls now matches TypeScript's behavior exactly.
