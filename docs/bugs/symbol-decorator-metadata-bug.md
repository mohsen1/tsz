# Symbol() Returns DecoratorMetadata Bug

## Summary
When `esnext.decorators` lib is loaded, `Symbol('test')` incorrectly returns type `DecoratorMetadata` instead of `symbol`.

## Reproduction
```typescript
const s: symbol = Symbol('test');
// ERROR: Type 'DecoratorMetadata' is not assignable to type 'symbol'
```

### Works:
- `--lib es2015` alone ✓
- All code in single file (no lib merging) ✓

### Fails:
- `--lib esnext` (includes decorators) ✗
- `--lib es2015,esnext.decorators` ✗

## Impact
High - affects 320+ conformance tests:
- TS2345: 122 tests (false positive "Argument not assignable")  
- TS2322: 107 tests (false positive "Type not assignable")
- TS2339: 95 tests (false positive "Property does not exist")

Many failures involve WeakSet, WeakMap, WeakRef, FinalizationRegistry which all accept `symbol` as keys.

## Root Cause Analysis

### Interface Definitions
**es2015.symbol.d.ts:**
```typescript
interface SymbolConstructor {
    (description?: string | number): symbol;  // Call signature
}
declare var Symbol: SymbolConstructor;
```

**esnext.decorators.d.ts:**
```typescript
interface SymbolConstructor {
    readonly metadata: unique symbol;  // Property
}
```

### Type Resolution Flow
1. Resolve `Symbol` identifier → gets `SymbolConstructor` type (merged interface)
2. Classify for call → finds `Callable(CallableShapeId(360))`  ✓
3. Extract return type from call signature → **Returns `DecoratorMetadata`** ✗

### Investigation Points

#### ✓ Confirmed Working:
- Binder merges interface declarations correctly (both call sig + property)
- `classify_for_call_signatures` correctly identifies it as `Callable`
- CallableShape contains call signatures

#### ❓ Suspected Bug Location:
The bug is likely in one of these areas:

1. **Building CallableShape from merged interfaces** (`crates/tsz-checker/src/interface_type.rs`)
   - When lowering SymbolConstructor interface to CallableShape
   - Call signature's `return_type` field may be set incorrectly
   - Hypothesis: Property type (`DecoratorMetadata` from `metadata` property) is being used instead of call signature return type

2. **Symbol merging in `merge_lib_contexts_into_binder`** (`crates/tsz-binder/src/state.rs:1312`)
   - Cross-lib declaration merging  
   - `value_declaration` field handling
   - Declaration order dependencies

3. **Type annotation resolution** (`crates/tsz-checker/src/type_computation_complex.rs`)
   - `type_of_value_declaration_for_symbol`
   - `type_of_value_declaration` 
   - How SymbolConstructor type reference is resolved

### Debug Traces
```bash
# Confirmed callee type is found and classified
TSZ_LOG="tsz_checker::type_computation_complex=trace" cargo run -p tsz-cli --bin tsz -- test.ts --lib esnext

# Output shows:
callee_type=TypeId(1115)  # SymbolConstructor
classification=Callable(CallableShapeId(360))  # Has call signatures ✓
```

## Next Steps

1. **Add instrumentation** to `lower_interface_declarations` to see what return_type is set for call signatures
2. **Check CallableShape** building - verify call signature return types are copied correctly  
3. **Test hypothesis**: Does `DecoratorMetadata` type appear anywhere in the call signature or is it from property lookup confusion?
4. **Unit test**: Create minimal test case that reproduces the bug without full lib loading

## Files to Investigate
- `crates/tsz-checker/src/interface_type.rs` - Interface lowering
- `crates/tsz-binder/src/state.rs:1312-1403` - Lib symbol merging
- `crates/tsz-solver/src/operations.rs:746-800` - Call signature resolution
- `crates/tsz-checker/src/type_computation_complex.rs:2080-2230` - Value declaration types

## Workaround
None currently. Tests must use `--lib es2015` without decorators to avoid this bug.
