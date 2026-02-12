# Session 2026-02-12: Emit Slice 4 - Spread in Function Calls Investigation

## Assignment
**Slice 4**: Helper functions + this capture (__values, __read, __spread, _this binding)

**Current Status**: ~62% JS emit pass rate
**Target**: 90%+ pass rate

## Bugs Identified

### Bug 1: Spread in Function Calls Not Transformed (CRITICAL)

**Issue**: Spread arguments in function calls are not transformed to ES5.

**Example**:
```typescript
// Input
const arr = [1, 2];
foo(...arr, 3);

// Expected (TypeScript output)
foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [3], false));

// Actual (our output)
foo(...arr, 3);  // ❌ Not transformed!
```

**Impact**: Code won't run in ES5 environments - spread syntax is ES6 only.

**Root Cause**:
- The lowering pass (`crates/tsz-emitter/src/lowering_pass.rs:1401`) `visit_call_expression()` only checks for super() calls
- It does NOT check for spread arguments in regular function calls
- The `__spreadArray` helper IS being emitted (helpers are marked correctly)
- But the actual call expression is not being transformed

**Files Involved**:
- `crates/tsz-emitter/src/lowering_pass.rs` - needs to detect spread in call args
- `crates/tsz-emitter/src/transform_context.rs` - needs ES5CallSpread directive
- `crates/tsz-emitter/src/emitter/mod.rs` - needs to handle the directive
- `crates/tsz-emitter/src/emitter/es5_helpers.rs` - needs transformation function

**Attempted Fix**:
1. Added detection in `visit_call_expression()` to check for spread arguments
2. Added `TransformDirective::ES5CallSpread` variant
3. Added `EmitDirective::ES5CallSpread` variant
4. Implemented `emit_call_expression_es5_spread()` function
5. Implemented helper functions for building __spreadArray calls

**Status**: Implementation added but not working. Changes may have been lost due to linter/hooks.

### Bug 2: Spread in Array Literals Missing First Array

**Issue**: Array literals with multiple spread elements only emit the last spread.

**Example**:
```typescript
// Input
const arr1 = [1, 2];
const arr2 = [3, 4];
return [...arr1, ...arr2];

// Expected (TypeScript)
return __spreadArray(__spreadArray([], arr1, true), arr2, true);

// Actual (our output)
return [].concat(arr2);  // ❌ arr1 is missing!
```

**Root Cause**:
The `build_concat_chain()` function in `crates/tsz-emitter/src/transforms/spread_es5.rs` at line 233 has a comment:
```rust
// Chain concat calls for remaining segments
// Note: We already consumed the first element, so we need to re-iterate
// This is a simplification - in real code we'd handle this more elegantly

Some(result)  // ❌ Returns only first segment!
```

**Fix Required**: Complete the implementation to actually chain all segments.

## Test Cases Created

Created test files in `tmp/`:
- `test-helpers.ts` - comprehensive test of all helper types
- `test-call-spread.ts` - simple spread in call test

## Key Learnings

1. **Helper Emission Works**: The `__spreadArray` helper IS correctly emitted when spread is detected
2. **Array Spread Partially Works**: Array literals with single spread work, multiple spreads fail
3. **Call Spread Completely Broken**: Function calls with spread not transformed at all
4. **Transform Pipeline**:
   - Lowering pass detects patterns → adds TransformDirective
   - Emitter converts TransformDirective → EmitDirective
   - Emitter matches EmitDirective in emit() → calls special ES5 function

## Next Steps

### Immediate (P0 - Code Won't Run)
1. **Fix spread in function calls**:
   - Re-implement detection in `visit_call_expression()`
   - Ensure directive is created and applied
   - Test with `tmp/test-call-spread.ts`
   - Verify transformation matches TypeScript exactly

2. **Fix array spread for multiple spreads**:
   - Complete `build_concat_chain()` implementation
   - Test with `[...arr1, ...arr2]` pattern
   - Verify all segments are included

### Testing
```bash
# Build
cargo build --release -p tsz-cli

# Test specific pattern
.target/release/tsz --noCheck --noLib --target es5 --module none tmp/test-call-spread.ts

# Compare with TypeScript
tsc --target es5 --module none tmp/test-call-spread.ts
diff tmp/test-call-spread.js <expected-output>

# Run emit test suite
./scripts/emit/run.sh --max=100 --js-only
```

## Relevant Code Locations

**Detection**:
- `crates/tsz-emitter/src/lowering_pass.rs:1401` - `visit_call_expression()`
- `crates/tsz-emitter/src/lowering_pass.rs:1877` - `is_spread_element()`

**Directives**:
- `crates/tsz-emitter/src/transform_context.rs:37` - `TransformDirective` enum
- `crates/tsz-emitter/src/emitter/mod.rs:140` - `EmitDirective` enum

**Transformation**:
- `crates/tsz-emitter/src/emitter/es5_helpers.rs` - ES5 transformations
- `crates/tsz-emitter/src/transforms/spread_es5.rs` - Spread transformer

**Emit**:
- `crates/tsz-emitter/src/emitter/expressions.rs:90` - `emit_call_expression()`
- `crates/tsz-emitter/src/emitter/mod.rs:1100+` - directive handling in `emit()`

## Reference: TypeScript's Transform

For `foo(...arr, 1, 2)`, TypeScript emits:
```javascript
foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [1, 2], false))
```

Pattern:
1. Convert to `.apply(void 0, args_array)` for function calls
2. Convert to `.apply(obj, args_array)` for method calls (need temp variable for obj)
3. Build args_array with nested `__spreadArray()` calls:
   - Start with `__spreadArray([], first_spread, false)`
   - For each additional segment: `__spreadArray(previous, next_segment, false)`

## Time Spent
- Investigation: 1.5 hours
- Implementation attempt: 1.5 hours
- Total: 3 hours

## Status
🔴 **INCOMPLETE** - Implementation added but not working. Needs debugging to understand why directives aren't being applied.
