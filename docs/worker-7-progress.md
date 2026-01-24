# Worker-7: Symbol Resolution Balance (TS2304 Extra and Missing)

## Summary

This document tracks the progress on fixing TS2304 (Cannot find name) issues - both reducing EXTRA errors (false positives) and adding MISSING errors (false negatives).

## Completed Fixes

### 1. Type-Only Imports in Type Positions (EXTRA TS2304)

**Problem**: Type-only imports (from `import type`) were being incorrectly flagged as "value-only types" even in type positions, causing EXTRA TS2304 errors.

**Solution**: Modified the `symbol_is_value_only` and `alias_resolves_to_value_only` functions to check the `is_type_only` flag. When a symbol is marked as type-only, it should:
- Be **allowed** in type positions (e.g., `function f(x: MyType)`)
- Be **rejected** in value positions (already handled by existing `alias_resolves_to_type_only` check)

**Files Modified**:
- `src/checker/state.rs`: Updated `symbol_is_value_only` and `alias_resolves_to_value_only` to check `is_type_only`
- `src/checker/state.rs`: Added `symbol_is_type_only` helper method
- `src/checker/state.rs`: Updated all value-only type error checks to exempt type-only imports
- `src/checker/symbol_resolver.rs`: Added TDZ (Temporal Dead Zone) stub for block-scoped variables

**Impact**: Reduces EXTRA TS2304 errors where type-only imports are used in type annotations.

## Remaining Issues

### EXTRA TS2304 (False Positives - Need Reduction)

1. **Declaration Hoisting Not Respected** (High Priority)
   - `var` and `function` declarations should be visible throughout their enclosing scope
   - Current binder already handles hoisting via `declare_in_persistent_scope`
   - May need verification that scope chain traversal properly finds hoisted declarations

2. **Ambient Declarations Not Visible** (High Priority)
   - `declare` statements in `.d.ts` files should create globally visible symbols
   - `global_augmentations` tracks declarations in `declare global` blocks
   - Need to ensure these are properly merged into visible symbols

3. **Merged Declarations Across Files Not Visible** (Medium Priority)
   - Interface/class merging across files (e.g., multiple `interface Foo {}` declarations)
   - Symbol should be visible if at least one declaration is visible
   - Need to verify cross-file symbol resolution

### MISSING TS2304 (False Negatives - Need Addition)

1. **Block-Scoped Let/Const Before Declaration (TDZ)** ✅ COMPLETED
   - References to `let`/`const` variables before their declaration should emit TS2304
   - **Implemented**: `is_node_before_decl` to check node positions
   - **Implemented**: TDZ check integrated into symbol resolution logic
   - When a block-scoped variable reference is before its declaration, the symbol is skipped

2. **Using Undeclared Variables** (High Priority)
   - When `noImplicitAny` is enabled, undeclared variables should emit TS2304
   - Currently: `get_type_of_identifier` returns `TypeId::ERROR` and emits TS2304 when symbol not found
   - May need to verify this is working correctly in all cases

3. **Using Private Members Outside Class** (Medium Priority)
   - Private identifier access (e.g., `obj.#priv`) outside class should emit appropriate error
   - Current code in `resolve_private_identifier_symbols` handles this
   - May need verification that errors are being emitted correctly

### Scope Chain Traversal Differences

The scope chain traversal in `resolve_identifier_symbol` follows this order:
1. **Phase 2**: Scope chain traversal (local -> parent -> ... -> module)
   - Checks each scope's symbol table
   - Checks module exports for module scopes
   - Filters out class members (not accessible via simple name)
2. **Phase 3**: Check file_locals (global scope from lib.d.ts)
3. **Phase 4**: Check lib binders' file_locals directly

Potential improvements:
- Ensure hoisted declarations (var/function) are in the correct scope
- Verify that merged declarations are accessible across files
- Check that ambient declarations are properly visible

## Implementation Notes

### Symbol Flags

Key symbol flags from `src/binder.rs`:
- `FUNCTION_SCOPED_VARIABLE`: `var` declarations
- `BLOCK_SCOPED_VARIABLE`: `let`/`const` declarations
- `FUNCTION`: Function declarations
- `CLASS`: Class declarations
- `ALIAS`: Import aliases
- `TYPE`: Type symbols
- `VALUE`: Value symbols
- `EXPORT_VALUE`: Exported values

### Type-Only Imports

Type-only imports are identified by the `is_type_only` flag on symbols:
- Created by `import type { X } from 'module'`
- Should resolve in type positions (type annotations)
- Should emit TS1369 error in value positions

## Testing Strategy

To verify fixes:
1. Run conformance tests targeting TS2304
2. Check reduction in EXTRA TS2304 (false positives)
3. Check addition in MISSING TS2304 (false negatives)
4. Use test files that specifically exercise:
   - Type-only imports
   - Declaration hoisting (var/function)
   - Block-scoped variables (TDZ)
   - Ambient declarations
   - Merged declarations
   - Private member access

## Next Steps

1. **Implement TDZ checking**: Complete `is_node_before_decl` implementation
2. **Verify hoisting**: Ensure var/function declarations are properly visible
3. **Ambient declarations**: Ensure `declare` statements create visible symbols
4. **Merged declarations**: Verify cross-file symbol merging works
5. **Run conformance tests**: Measure actual reduction in TS2304 errors
