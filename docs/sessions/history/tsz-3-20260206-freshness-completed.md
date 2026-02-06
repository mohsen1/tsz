# Session tsz-3: Object Literal Freshness - COMPLETED

**Started**: 2026-02-06
**Completed**: 2026-02-06
**Status**: ✅ DONE
**Predecessor**: Discriminant Narrowing (Already Implemented)

## Task

Fix object literal freshness stripping - 6 tests failing.

## Problem

When `let x = { a: 1, b: 2 }` is declared, the freshness should be stripped so later uses of `x` don't trigger excess property checks.

Example that should pass but was failing:
```typescript
let x = { a: 1, b: 2 };  // Freshness should be stripped
let y: { a: number } = x;  // Should PASS (x is non-fresh, excess prop allowed)
```

## Investigation Process

1. Initial attempts focused on caching the widened type in `node_types` - but this didn't work
2. Added extensive debug output to trace the exact flow of types
3. Discovered that `get_type_of_identifier` was returning the correct widened type (8316), but the final result was fresh (8528)
4. Consulted Gemini Pro to understand the architecture

## Root Cause (Discovered with Gemini)

The issue was in the Control Flow Analysis (CFA) layer:

1. Variable declaration correctly widens fresh type (8528 → 8316) and caches it on symbol
2. When `x` is referenced later, `get_type_of_identifier` calls `check_flow_usage`
3. **Flow analysis returns the ORIGINAL fresh type (8528) from the initializer expression**, bypassing the widened type from the symbol cache
4. This causes EPC to trigger incorrectly

The code path was:
- `get_type_of_node(x_identifier)` → `get_type_of_identifier` → `get_type_of_symbol` (returns widened 8316) → `check_flow_usage` (returns fresh 8528!)

## Solution

Modified `src/checker/type_computation_complex.rs` in `get_type_of_identifier`:

After getting the flow type from CFA, apply `widen_freshness` if the flow type is fresh. This ensures CFA respects the widening that was applied during variable declaration.

```rust
let flow_type = self.check_flow_usage(idx, declared_type, sym_id);

// FIX: Flow analysis may return the original fresh type from the initializer expression.
// For variable references, we must respect the widening that was applied during variable
// declaration. If the symbol was widened (non-fresh), the flow result should also be widened.
if !self.ctx.compiler_options.sound_mode {
    use crate::solver::freshness::{is_fresh_object_type, widen_freshness};
    if is_fresh_object_type(self.ctx.types, flow_type) {
        return widen_freshness(self.ctx.types, flow_type);
    }
}

return flow_type;
```

Also updated `src/checker/state_checking.rs` to always cache the widened type in both `symbol_types` and `node_types` caches (removing the conditional check that prevented overwriting).

## Tests Fixed

All 6 failing tests now pass:
- `test_fresh_variable_can_be_reassigned_with_non_fresh_source` ✅
- `test_freshness_preserved_for_const_with_no_type_annotation` ✅
- `test_freshness_stripped_allows_passing_to_stricter_type` ✅
- `test_freshness_stripped_in_function_argument` ✅
- `test_freshness_stripped_in_let_declaration` ✅
- `test_freshness_stripped_variable_can_be_used_as_source` ✅

## Test Results

- Before fix: 191 test failures
- After fix: 185 test failures
- **Net improvement: 6 tests fixed**

## Commit

Commit: `ee07097a1` - "fix(freshness): fix object literal freshness stripping via flow analysis"

## Key Learnings

1. **Control Flow Analysis can bypass symbol caches**: When tracking variable types through flow analysis, CFA may return the type of the initializer expression directly, bypassing the widened type cached on the symbol.

2. **Freshness must be respected at all layers**: The fix needed to be applied at the CFA layer (`check_flow_usage` result handling), not just at the symbol cache layer.

3. **Gemini Pro was essential**: The architectural insight about CFA returning the original fresh type came from Gemini Pro. This would have been very difficult to discover through debugging alone.

## Next Steps

This session is complete. All object literal freshness tests are now passing. The remaining 185 failing tests are unrelated to freshness stripping.
