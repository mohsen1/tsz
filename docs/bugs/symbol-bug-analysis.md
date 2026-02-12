# Symbol() Returns DecoratorMetadata - Detailed Analysis

## Confirmed Facts

### Works ✓
1. `--lib es2015` alone - Symbol() returns symbol
2. `--lib es2015,decorators` - Symbol() returns symbol
3. User-defined merged interface in code - works correctly
4. Explicit MySymbolConstructor variable - works correctly

### Fails ✗
1. `--lib esnext` (includes esnext.decorators) - Symbol() returns DecoratorMetadata
2. `--lib es2015,esnext.decorators` - Symbol() returns DecoratorMetadata

## Root Cause

**The bug is specific to `esnext.decorators.d.ts`, NOT `decorators.d.ts`.**

### Key Difference

`decorators.d.ts`:
- Defines DecoratorMetadata type
- Does NOT modify SymbolConstructor

`esnext.decorators.d.ts`:
- References `/// <reference lib="es2015.symbol" />`
- References `/// <reference lib="decorators" />`
- Adds `readonly metadata: unique symbol` to SymbolConstructor
- Adds `[Symbol.metadata]: DecoratorMetadata | null` to Function

## Investigation Findings

### Trace Output
```
name="Symbol", value_type=TypeId(1115)  # SymbolConstructor type
callee_type=TypeId(1115)
classification=Callable(CallableShapeId(360))  # Has call signatures ✓
```

- Symbol resolves to correct SymbolId (195)
- Type resolution finds TypeId(1115) = SymbolConstructor
- CallableShape(360) is found
- **But CallableShape(360) has wrong return_type in call signature**

### The Bug Location

The bug is NOT in:
- ✓ Symbol merging (works correctly)
- ✓ Identifier resolution (finds correct symbol)
- ✓ Type resolution (finds SymbolConstructor)
- ✓ Call signature classification (finds Callable)

The bug IS in:
- ✗ **CallableShape building** - call signature has wrong return_type

## Hypothesis

When `lower_merged_interface_declarations` processes SymbolConstructor with declarations from:
1. `es2015.symbol.d.ts` - call signature `(description?: string | number): symbol`
2. `esnext.decorators.d.ts` - property `readonly metadata: unique symbol`

The call signature's return type annotation `symbol` is somehow being resolved to `DecoratorMetadata` instead of the primitive `symbol` type.

## Possible Causes

1. **Type reference resolution bug**: When lowering the call signature from es2015.symbol.d.ts in the context of esnext.decorators being loaded, the `symbol` type reference might be incorrectly resolving to DecoratorMetadata

2. **Cross-arena type resolution**: The es2015.symbol.d.ts arena and esnext.decorators.d.ts arena might have conflicting type resolutions

3. **Conditional type evaluation**: The DecoratorMetadata type definition uses a conditional type that checks `Symbol.metadata`, which might be affecting type resolution

## Next Steps

1. Add instrumentation to `lower_call_signature` in tsz-solver/src/lower.rs
2. Check what type is returned by `lower_return_type` for the Symbol() call signature
3. Add instrumentation to `lower_type` when resolving "symbol" type reference
4. Create unit test that reproduces the bug with minimal lib file setup

## Files to Investigate
- `crates/tsz-solver/src/lower.rs:lower_call_signature`
- `crates/tsz-solver/src/lower.rs:lower_return_type`
- `crates/tsz-solver/src/lower.rs:lower_type`
- Type reference resolution for "symbol" in lib context
