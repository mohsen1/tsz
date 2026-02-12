# Slice 2 Investigation - Array Literal Type Inference

## Date
2026-02-12

## Problem
Tests failing because `["aa", "bb"]` infers as `string[]` instead of `("aa" | "bb")[]` when used for generic type parameter inference.

Example failing test:
```typescript
declare function func<T extends string>(arg: { keys: T[] }): { readonly keys: T[]; readonly firstKey: T; };
const { firstKey } = func({keys: ["aa", "bb"]})
const a: "aa" | "bb" = firstKey; // tsz: error TS2322, tsc: ok
```

## Root Cause Analysis

### Found in `crates/tsz-solver/src/expression_ops.rs:236-255`

The `widen_literals()` function ALWAYS widens string literals to `string`:

```rust
fn widen_literals(interner: &dyn TypeDatabase, types: &[TypeId]) -> Vec<TypeId> {
    types.iter().map(|&ty| {
        if let Some(key) = interner.lookup(ty) {
            if let crate::types::TypeKey::Literal(ref lit) = key {
                return match lit {
                    crate::types::LiteralValue::String(_) => TypeId::STRING,  // ← Problem!
                    ...
                };
            }
        }
        ty
    }).collect()
}
```

### Call Chain
1. `get_type_of_array_literal()` in `type_computation.rs:144`
2. Calls `compute_best_common_type()` at line 362
3. Which calls `widen_literals()` at line 148
4. Literals widened: `["aa", "bb"]` → `[string, string]` → `string[]`
5. Generic inference receives `string[]` and infers `T = string` (wrong!)

Should be:
- `["aa", "bb"]` → `["aa", "bb"]` → `("aa" | "bb")[]`  
- Generic inference receives `("aa" | "bb")[]` and infers `T = "aa" | "bb"` (correct!)

## TypeScript's Behavior

TypeScript preserves literal types in array literals when:
- The array is used for generic type parameter inference
- The array has a contextual type that expects literals

TypeScript widens literals when:
- The array is assigned to a variable without type annotation
- No contextual type is present

## Proposed Solution

### Option 1: Context-aware widening flag
Add a `widen_literals: bool` parameter to `compute_best_common_type()`:
- `true` for regular array literals (variable assignments)
- `false` for inference contexts (generic function arguments)

### Option 2: Defer widening
Don't widen in `compute_best_common_type`, let callers decide when to widen.

### Option 3: Check contextual type
Only widen when there's no contextual type expecting literals.

## Files to Modify

1. **`crates/tsz-solver/src/expression_ops.rs`**
   - Modify `compute_best_common_type()` and `widen_literals()`
   - Add parameter to control widening behavior

2. **`crates/tsz-checker/src/type_computation.rs`**
   - Modify `get_type_of_array_literal()`
   - Pass appropriate widening flag based on context

3. **`crates/tsz-solver/src/infer.rs`**
   - Ensure generic inference uses non-widened array types
   - May need to check how inference collects candidates from arrays

## Impact Estimate
- **50+ tests** per documentation
- High-priority fix identified in final status document

## Next Steps
1. Write failing unit test demonstrating the problem
2. Implement Option 1 (context-aware widening flag)
3. Verify with conformance tests
4. Commit with clear message

## References
- `docs/conformance/slice2-final-status.md` - identified as highest impact
- `docs/conformance/slice2-session-2026-02-12.md` - detailed analysis
