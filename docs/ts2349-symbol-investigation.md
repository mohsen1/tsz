# TS2349 False Positive Investigation: Symbol() Call

## Issue
14 tests fail with TS2349 false positives, primarily related to `Symbol()` calls in declaration emit mode.

## Example Test
`TypeScript/tests/cases/compiler/declarationEmitClassMemberWithComputedPropertyName.ts`

```typescript
const k1 = Symbol();  // ❌ TS2349: Type '{ readonly [Symbol.toStringTag]: string; ... }' has no call signatures.
```

## Root Cause Analysis

### Error Flow
1. **Checker Phase**: `type_computation_complex.rs:1245` calls `error_not_callable_at()`
2. **Diagnostic Creation**: `error_reporter.rs:1523` creates TS2349 diagnostic
3. **Solver Check**: `operations.rs:338` `resolve_call()` returns `CallResult::NotCallable`

### Problem
When resolving `Symbol` as a value, we get the wrong type:
- **What we get**: `{ readonly [Symbol.toStringTag]: string; toString: { (): Date }; ... }`
- **What we need**: `SymbolConstructor` interface with call signatures

The type string suggests we're resolving to:
- Symbol prototype/instance type (properties of symbol objects)
- NOT the SymbolConstructor interface (which has call signatures)

### Code Locations
1. **Error Emission**: `crates/tsz-checker/src/type_computation_complex.rs:1245`
   ```rust
   CallResult::NotCallable { .. } => {
       self.error_not_callable_at(callee_type, callee_expr);
   }
   ```

2. **Callable Check**: `crates/tsz-solver/src/operations.rs:338-398`
   ```rust
   pub fn resolve_call(&mut self, func_type: TypeId, arg_types: &[TypeId]) -> CallResult
   ```
   - Line 342: Returns `NotCallable` if type lookup fails
   - Line 383-388: Returns `NotCallable` if Lazy type doesn't resolve
   - Line 397: Default returns `NotCallable`

3. **Type Resolution**: Need to check how `Symbol` global is resolved
   - Symbol should resolve to `SymbolConstructor` from lib.d.ts
   - SymbolConstructor has call signatures: `(description?: string | number): symbol`

### Next Steps

1. **Find Symbol Resolution**: Check where global `Symbol` is resolved as a value
   - Search in: `crates/tsz-checker/src/type_computation.rs` or similar
   - Look for global value resolution

2. **Check SymbolConstructor Type**: Verify that SymbolConstructor interface has call signatures in our type representation
   - May need to check lib file loading
   - Verify callable shape is created correctly

3. **Fix Resolution**: Update Symbol resolution to:
   - Return SymbolConstructor type (which has call signatures)
   - Not the symbol primitive instance type

4. **Similar Issues**: This may affect other global constructors:
   - `Number()`, `String()`, `Boolean()`, `Object()`
   - Check if they have the same problem

## Impact
- 14 tests in slice 2 affected
- All related to declaration emit mode
- Pattern: computed property names, dynamic names

## Related Files
- `crates/tsz-checker/src/type_computation_complex.rs` - Call checking
- `crates/tsz-checker/src/error_reporter.rs` - Error emission
- `crates/tsz-solver/src/operations.rs` - resolve_call logic
- `crates/tsz-checker/src/type_checking.rs:2026` - is_global_function_type (similar handling)

## Observations
- There's already special handling for `Function` type (line 2026)
- Symbol may need similar special handling
- The issue only manifests with --declaration --emitDeclarationOnly
- Debug output shows type_interner is None during emit (separate issue)
