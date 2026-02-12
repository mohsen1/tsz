# Type Guard Predicate Inference Bug

**Status**: Analyzed, needs fix verification
**Priority**: High (affects ~10-20 conformance tests)
**Test case**: `TypeScript/tests/cases/compiler/arrayFind.ts`

## Problem

When calling `Array.find()` with a type guard predicate, the type parameter is incorrectly inferred.

```typescript
function isNumber(x: any): x is number {
  return typeof x === "number";
}

const arr = ["string", false, 0];
const result: number | undefined = arr.find(isNumber);
// ❌ tsz: Type 'string | boolean | number | undefined' is not assignable to 'number | undefined'
// ✅ tsc: no error
```

## Expected Behavior

- S should be inferred as `number` (from the type guard `x is number`)
- Return type should be `S | undefined` = `number | undefined`

## Actual Behavior

- S is inferred as `string | boolean | number` (the constraint, not the type guard)
- Return type is `string | boolean | number | undefined`

## Analysis

### Type Inference Flow

1. `arr` has type `(string | boolean | number)[]`
2. `find` has signature: `find<S extends T>(predicate: (value: T) => value is S): S | undefined`
3. When calling `arr.find(isNumber)`:
   - T = `string | boolean | number`
   - S needs to be inferred
   - Parameter type is instantiated as: `(value: string|boolean|number) => value is __infer_0`
   - Argument type is: `(x: any) => x is number`

### Constraint Collection

In `crates/tsz-solver/src/operations.rs`:

**Line 2593-2597**: Type predicate constraints ARE being added:
```rust
if let (Some(s_pred), Some(t_pred)) = (&source.type_predicate, &target.type_predicate) {
    if let (Some(s_pred_type), Some(t_pred_type)) = (s_pred.type_id, t_pred.type_id) {
        self.constrain_types(ctx, var_map, s_pred_type, t_pred_type, priority);
    }
}
```

This should add: `number <: __infer_0` (lower bound for S)

**Line 1862-1864**: When target is a placeholder, source is added as lower bound:
```rust
if let Some(&var) = var_map.get(&target) {
    ctx.add_candidate(var, source, priority);
    return;
}
```

### Type Resolution

**Line 907-936**: Type parameters are resolved:
- If variable has constraints: use `resolve_with_constraints_by`
- If resolution fails OR no constraints: fall back to constraint/default/unknown
- **Line 931-932**: Fallback uses the constraint itself

## Root Cause Hypotheses

### Hypothesis 1: Type predicate constraint not being added
- Possible if one function doesn't have a type predicate in the instantiated form
- Would result in `has_constraints = false` → fallback to constraint

### Hypothesis 2: Constraint resolution failing
- `resolve_with_constraints_by` might be failing
- Would trigger fallback to constraint at line 922-923

### Hypothesis 3: Wrong constraint priority
- Type predicate constraints might have lower priority than other constraints
- Could be overridden during resolution

## Debugging Steps

1. **Add tracing** to `constrain_function_to_call_signature`:
   ```rust
   #[tracing::instrument(level = "trace", skip(self, ctx, var_map))]
   fn constrain_function_to_call_signature(...)
   ```

2. **Add tracing** to type predicate constraint collection (line 2593):
   ```rust
   if let (Some(s_pred), Some(t_pred)) = (&source.type_predicate, &target.type_predicate) {
       trace!("Adding type predicate constraint");
       if let (Some(s_pred_type), Some(t_pred_type)) = (s_pred.type_id, t_pred.type_id) {
           trace!(s_pred_type = ?s_pred_type, t_pred_type = ?t_pred_type, "Constraining type predicates");
           self.constrain_types(ctx, var_map, s_pred_type, t_pred_type, priority);
       }
   }
   ```

3. **Add tracing** to type parameter resolution (line 906):
   ```rust
   for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
       trace!(type_param = ?tp.name, var = ?var, "Resolving type parameter");
       let has_constraints = infer_ctx
           .get_constraints(var)
           .is_some_and(|c| !c.is_empty());
       trace!(has_constraints = has_constraints, "Constraint check");
       // ...
   }
   ```

4. **Run with tracing**:
   ```bash
   TSZ_LOG="wasm::solver::operations=trace" TSZ_LOG_FORMAT=tree \
     cargo run -- tmp/array_find_test.ts 2>&1 | head -300
   ```

## Potential Fixes

### Fix 1: Ensure type predicates are instantiated
Verify that when instantiating function types with placeholders, type predicates are properly preserved and instantiated.

### Fix 2: Adjust constraint priority
Type predicate constraints might need higher priority:
```rust
self.constrain_types(
    ctx, var_map, s_pred_type, t_pred_type,
    crate::types::InferencePriority::TypeGuardPredicate  // New higher priority
);
```

### Fix 3: Special handling for type guard predicates
When resolving type parameters, check if constraints come from type guard predicates and prioritize them:
```rust
// In resolve_generic_call, after constraint collection
if has_type_guard_constraint {
    // Use type guard constraint directly instead of falling back to parameter constraint
}
```

## Related Code Locations

- `crates/tsz-solver/src/operations.rs:2571-2598` - `constrain_function_to_call_signature`
- `crates/tsz-solver/src/operations.rs:1819-1871` - `constrain_types`
- `crates/tsz-solver/src/operations.rs:905-952` - Type parameter resolution
- `crates/tsz-solver/src/instantiate.rs:400-403` - Type predicate instantiation
- `crates/tsz-solver/src/types.rs:935-939` - `TypePredicate` definition

## Next Steps

1. Build tsz-cli to test the arrayFind.ts file directly
2. Add tracing as described above
3. Run test and analyze trace output
4. Identify which hypothesis is correct
5. Implement and test fix
6. Verify with full conformance test suite

## Impact

Fixing this issue will likely fix:
- arrayFind.ts
- Similar Array/ReadonlyArray method tests with type guards
- Potentially 10-20 other conformance tests that use type guard predicates in callbacks
