# Action Plan: Fix Array Literal Type Inference for Generic Functions

## Status
**Investigation Complete** - Root cause identified, ready for implementation

## Problem Summary  
When `["aa", "bb"]` is passed to a generic function `func<T extends string>(arg: {keys: T[]})`, tsz infers `T = string` instead of `T = "aa" | "bb"` because array element types are widened too early.

## Root Cause
File: `crates/tsz-solver/src/expression_ops.rs:236-255`

The `widen_literals()` function ALWAYS widens literals:
- `["aa", "bb"]` → `[string, string]` → `string[]`
- Should be: `["aa", "bb"]` → `("aa" | "bb")[]`

This breaks generic inference which needs the literal union type.

## Implementation Plan

### Step 1: Add Widening Control
**File**: `crates/tsz-solver/src/expression_ops.rs`

Modify signature:
```rust
pub fn compute_best_common_type<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: Option<&R>,
    widen_fresh_literals: bool,  // NEW PARAMETER
) -> TypeId
```

Modify `widen_literals()` to accept and respect the flag:
```rust
fn widen_literals(interner: &dyn TypeDatabase, types: &[TypeId], should_widen: bool) -> Vec<TypeId> {
    if !should_widen {
        return types.to_vec();  // Preserve literals
    }
    // ... existing widening logic ...
}
```

### Step 2: Update Caller
**File**: `crates/tsz-checker/src/type_computation.rs:362`

In `get_type_of_array_literal()`:
```rust
// Use Solver API for Best Common Type computation
let should_widen = self.ctx.contextual_type.is_none();  // Only widen if no context
let element_type = expression_ops::compute_best_common_type(
    self.ctx.types,
    &element_types,
    Some(&self.ctx),
    should_widen,  // Pass widening flag
);
```

### Step 3: Fix Other Callers
Search for all calls to `compute_best_common_type()` and add the parameter:
- Most callers should pass `true` (default widening behavior)
- Only array literals in inference contexts need `false`

### Step 4: Write Unit Test
**File**: `crates/tsz-solver/src/tests/inference_array_literals_test.rs` (new)

```rust
#[test]
fn test_array_literal_preserves_literals_for_inference() {
    let interner = TypeInterner::new();
    let aa = interner.literal_string("aa");
    let bb = interner.literal_string("bb");
    
    // Without widening (for inference)
    let result = compute_best_common_type(&interner, &[aa, bb], None, false);
    let union = interner.union(vec![aa, bb]);
    assert_eq!(result, union, "Should preserve literal types without widening");
    
    // With widening (for regular arrays)
    let result = compute_best_common_type(&interner, &[aa, bb], None, true);
    assert_eq!(result, TypeId::STRING, "Should widen to string with flag");
}
```

### Step 5: Run Conformance Tests
```bash
./scripts/conformance.sh run --offset 3146 --max 50 --verbose | grep -A 10 "inferStringLiteral"
```

Look for improvements in tests like:
- `inferStringLiteralUnionForBindingElement.ts`
- `inferObjectTypeFromStringLiteralToKeyof.ts`
- Similar array literal inference tests

### Step 6: Full Test Run
```bash
./scripts/conformance.sh run --offset 3146 --max 3146
```

Expected impact: **50+ tests** should now pass

## Verification Checklist
- [ ] Unit test passes for non-widened case
- [ ] Unit test passes for widened case  
- [ ] All existing unit tests still pass
- [ ] Conformance tests show improvement
- [ ] No regressions in other slices

## Alternative Approaches (if needed)

### Option B: Contextual Type Check
Instead of a flag, check if there's a contextual type expecting literals:
```rust
let should_widen = !has_literal_expecting_context(self.ctx);
```

### Option C: Separate Function
Create `compute_best_common_type_for_inference()` that never widens.

## Expected Results
- Pass rate improvement: 59.4% → ~61.0% (+50 tests)
- Fixes TS2322, TS2345, TS2339 false positives related to generic inference
- No impact on non-generic array literal handling

## Files to Modify
1. `crates/tsz-solver/src/expression_ops.rs` - Add widening parameter
2. `crates/tsz-checker/src/type_computation.rs` - Pass appropriate flag
3. (New) `crates/tsz-solver/src/tests/inference_array_literals_test.rs` - Unit tests

## Time Estimate
- Implementation: 30-45 minutes
- Testing: 15-20 minutes  
- Total: ~1 hour

## References
- Investigation: `docs/session-2026-02-12-investigation.md`
- Original issue: `docs/conformance/slice2-final-status.md`

---

**Ready to implement!** Follow steps 1-6 in order.
