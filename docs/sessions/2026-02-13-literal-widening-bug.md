# Literal Widening Bug in Object Literals

**Date**: 2026-02-13
**Status**: Root Cause Identified - Fix In Progress
**Severity**: High - Blocks Control Flow Narrowing Tests

## Problem

Boolean literals in object literals are being widened to `boolean` type even when the contextual type expects specific literals. This prevents discriminated unions from working correctly.

## Minimal Reproduction

```typescript
type Obj = { flag: false } | { flag: true };
const obj: Obj = { flag: false };  // Error: Type '{ flag: boolean }' not assignable
```

**TSC**: No error
**TSZ**: TS2322 error - `flag` is typed as `boolean` instead of `false`

## Root Cause Analysis

The issue is in object literal property typing in `crates/tsz-checker/src/type_computation.rs`:

1. **Contextual property type extraction** (lines 1976-1981):
   - Gets contextual type for property from union
   - Should return `false | true` for property `flag`
   - This part likely works correctly (from contextual.rs:953-973)

2. **Value typing** (line 1987):
   - `get_type_of_node(prop.initializer)` is called with contextual type set
   - Dispatches to `dispatch.rs` for boolean literal

3. **Boolean literal resolution** (dispatch.rs:224-227):
   ```rust
   k if k == SyntaxKind::FalseKeyword as u16 => {
       let literal_type = self.checker.ctx.types.literal_boolean(false);
       self.resolve_literal(Some(literal_type), TypeId::BOOLEAN)
   }
   ```
   - Creates literal type `false`
   - Calls `resolve_literal` which checks if contextual type allows it

4. **Literal resolution check** (dispatch.rs:30-36):
   ```rust
   fn resolve_literal(&mut self, literal_type: Option<TypeId>, widened: TypeId) -> TypeId {
       match literal_type {
           Some(lit)
               if self.checker.ctx.in_const_assertion
                   || self.checker.contextual_literal_type(lit).is_some() =>
           {
               lit
           }
           _ => widened,  // <-- Returns TypeId::BOOLEAN here
       }
   }
   ```

5. **Contextual literal check** (state_type_analysis.rs:2860-2867):
   - Checks if contextual type allows the literal
   - Should succeed for `false` when contextual type is `false | true`

## Investigation Needed

The chain seems correct, so the bug must be in one of these areas:

### Hypothesis 1: Property Context Not Set
The `property_context_type` is not being properly set when calling `get_type_of_node`.

**Test**: Add logging before line 1987 in type_computation.rs to see if `property_context_type` is correctly computed.

### Hypothesis 2: Union Simplification
The union `false | true` might be getting simplified to `boolean` somewhere before it reaches the literal check.

**Test**: Check if `contextual_property_type` returns simplified union.

### Hypothesis 3: Contextual Type Not Visible
The contextual type might not be visible in the dispatch layer when `resolve_literal` is called.

**Test**: Add logging in `resolve_literal` to see what `self.checker.ctx.contextual_type` is.

### Hypothesis 4: apply_contextual_type Widening
The `apply_contextual_type` call at lines 1993-1997 might be widening the type after it's been correctly preserved.

**Test**: Check what `apply_contextual_type` does with literal boolean types.

## Impact

This bug blocks:
- All discriminated union tests
- Control flow narrowing tests (can't create proper discriminated unions)
- ~40-50 conformance tests affected

**Must fix before**: Implementing control flow narrowing features

## Next Steps

1. Add strategic logging/tracing to identify which hypothesis is correct
2. Fix the specific issue
3. Verify with minimal test case
4. Run full conformance tests to see improvement
5. Commit with clear message

## Test Cases to Verify Fix

```typescript
// Test 1: Simple object literal
type Obj = { flag: false } | { flag: true };
const obj: Obj = { flag: false };  // Should: no error

// Test 2: Return statement
function getObj(): Obj {
    return { flag: false };  // Should: no error
}

// Test 3: Nested object
type Nested = {
    type: 'a';
    data: { value: false };
} | {
    type: 'b';
    data: { value: true };
};
const nested: Nested = { type: 'a', data: { value: false } };  // Should: no error
```

## Files to Modify

Primary suspects:
- `crates/tsz-checker/src/type_computation.rs` (object literal typing)
- `crates/tsz-checker/src/dispatch.rs` (literal resolution)
- `crates/tsz-solver/src/contextual.rs` (property type extraction)
- `crates/tsz-solver/src/bidirectional.rs` (apply_contextual_type)
