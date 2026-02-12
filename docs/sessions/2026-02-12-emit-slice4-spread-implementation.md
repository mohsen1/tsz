# Session 2026-02-12: Emit Slice 4 - Spread in Function Calls Implementation

## Work Completed

### Implementation Added (Not Yet Working)

Added complete infrastructure for transforming spread arguments in function calls to ES5:

**Files Modified:**

1. **`crates/tsz-emitter/src/transform_context.rs`**
   - Added `TransformDirective::ES5CallSpread` variant with documentation

2. **`crates/tsz-emitter/src/lowering_pass.rs:1413-1441`**
   - Added detection logic in `visit_call_expression()` to check for spread arguments
   - Creates `ES5CallSpread` directive when spread detected
   - Marks `spread_array` helper as needed

3. **`crates/tsz-emitter/src/emitter/mod.rs`**
   - Added `CALL_EXPRESSION` to `kind_may_have_transform()` list (line 1418)
   - Added `EmitDirective::ES5CallSpread` variant (line 183)
   - Added conversion in `emit_directive_from_transform()` (line 573-577)
   - Added handler in `apply_transform()` (line 1191-1197)

4. **`crates/tsz-emitter/src/emitter/es5_helpers.rs:1207-1385`**
   - Implemented `emit_call_expression_es5_spread()`
   - Implemented `emit_function_call_with_spread()`
   - Implemented `emit_method_call_with_spread()`
   - Implemented `emit_spread_args_array()`
   - Implemented `emit_spread_segments()`

### Test Cases

Created test files:
- `tmp/test-call-spread.ts` - Spread arguments in function call
- `tmp/test-spread-only.ts` - Simple spread test

### Expected vs Actual

**Input:**
```typescript
function foo(...args: any[]) {
    return args;
}
const arr = [1, 2];
foo(...arr, 3);
```

**Expected (TypeScript):**
```javascript
foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [3], false));
```

**Actual (Our Compiler):**
```javascript
foo(...arr, 3);  // ❌ Not transformed
```

## Issue: Transform Not Being Applied

The implementation is complete but the transform is not being applied. The __spreadArray helper is also not being emitted, which indicates the directive is not being created in the lowering pass.

### Possible Causes

1. **Spread Element Detection**
   - `is_spread_element()` checks for `SPREAD_ASSIGNMENT` or `SPREAD_ELEMENT`
   - In function call arguments, spread might use a different AST node kind
   - Need to investigate what `...arr` actually parses to in call expression context

2. **Lowering Pass Not Visiting**
   - The lowering pass might not be visiting expression statements correctly
   - Or the call expression visitor might not be getting invoked

3. **Node Kind Mismatch**
   - Spread in array literals vs spread in call arguments might use different node types
   - TypeScript AST uses `SpreadElement` for both, but parser might differ

### Next Steps to Debug

1. **Add debug logging** to `visit_call_expression()`:
   ```rust
   eprintln!("DEBUG: Checking call expression at {:?}", idx);
   if let Some(ref args) = call.arguments {
       eprintln!("DEBUG: Call has {} args", args.nodes.len());
       for (i, &arg_idx) in args.nodes.iter().enumerate() {
           let node = self.arena.get(arg_idx);
           eprintln!("DEBUG: Arg {}: kind={:?}", i, node.map(|n| n.kind));
       }
   }
   ```

2. **Check AST node kinds** for spread in call context
   - Parse a simple test file
   - Print out the node kinds in call arguments
   - Compare with what `is_spread_element()` checks for

3. **Verify lowering pass execution**
   - Add debug print at start of `visit_call_expression()`
   - Confirm it's being called for our test case

4. **Check if directives are created**
   - Add debug print when inserting `ES5CallSpread` directive
   - Verify `self.transforms.helpers_mut().spread_array = true` executes

## Architecture Notes

### Transform Pipeline

1. **Lowering Pass** (`lowering_pass.rs`)
   - Walks the AST
   - Detects patterns that need transformation
   - Creates `TransformDirective` entries
   - Marks which helpers are needed

2. **Emit** (`emitter/mod.rs`)
   - Converts `TransformDirective` → `EmitDirective`
   - Checks `kind_may_have_transform()` before looking up directives
   - Calls `apply_transform()` which dispatches to specific handlers

3. **Transform Handlers** (`emitter/es5_helpers.rs`)
   - Implement the actual ES5 transformation logic
   - Generate the lowered code

### Key Functions

- `kind_may_have_transform()` - Gates whether to check for transforms
- `emit_directive_from_transform()` - Converts transform → emit directive
- `apply_transform()` - Dispatches to specific transform handlers
- `is_spread_element()` - Detects spread elements (might need fixing)

## Files to Review

When continuing this work:
1. Check how parser creates AST nodes for spread in call arguments
2. Look at `crates/tsz-parser/src/parser/expressions.rs` for call expression parsing
3. Compare with how array literal spread is parsed (that one works)
4. May need to add different node kind checks in `is_spread_element()`

## Commit Status

Code changes are complete and compilable but not yet working. Need debugging session to identify why spread detection fails in call argument context.
