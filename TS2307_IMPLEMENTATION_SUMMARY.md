# TS2307 Module Resolution Error Implementation

## Summary

Implemented TS2307 "Cannot find module" error emission for missing module resolution in the TypeScript checker.

## Changes Made

### 1. Added `emit_module_not_found_error()` function (src/checker/state.rs)

**Location**: Lines ~1764-1820 in `src/checker/state.rs`

This function:
- Emits TS2307 errors when a module specifier cannot be resolved
- Respects `report_unresolved_imports` flag (CLI driver handles multi-file mode)
- Extracts proper span information from various import declaration types:
  - `ImportEqualsDeclaration` (import x = require('...'))
  - `ImportDeclaration` (import { x } from '...')
  - `ImportSpecifier`, `ImportNamespaceSpecifier`, `ImportDefaultSpecifier`
- Uses existing `emitted_diagnostics` tracking for deduplication

### 2. Modified `compute_type_of_symbol()` function

**Two key locations where TS2307 is now emitted:**

#### A. require() imports (Line ~5105)
```rust
// Module not found - emit TS2307 error and return ANY to allow property access
self.emit_module_not_found_error(&module_specifier, value_decl);
return (TypeId::ANY, Vec::new());
```

#### B. ES6 imports (Line ~5148)
```rust
// Module not found in exports - emit TS2307 error and return ANY
// TSC emits TS2307 for missing module but allows property access on the result
self.emit_module_not_found_error(module_name, value_decl);
return (TypeId::ANY, Vec::new());
```

## Test Files Created

1. **test_missing_module.ts** - Basic missing module tests
2. **test_ts2307_various.ts** - Comprehensive test scenarios:
   - Relative imports (missing files)
   - Bare specifiers (missing packages)
   - Scoped packages
   - Default/namespace/type-only imports
   - Re-exports from missing modules
3. **test_ts2307_import_equals.ts** - Import equals declarations with require()

## How It Works

1. When `compute_type_of_symbol()` is called on an import symbol:
   - For `require()` calls: Checks if module exists in `module_exports`
   - For ES6 imports: Checks if module exists in `module_exports`

2. If module is not found:
   - Calls `emit_module_not_found_error()` with the module specifier
   - The function extracts the span from the import declaration node
   - Emits TS2307 error with proper location information
   - Returns `TypeId::ANY` to prevent cascading errors

3. Error tracking:
   - Uses existing `emitted_diagnostics: FxHashSet<(u32, u32)>`
   - Key is `(start, code)` to avoid duplicate errors at same location

## Expected Behavior

### Before:
```typescript
import { foo } from './missing';
foo.x;  // No error (foo was typed as ANY silently)
```

### After:
```typescript
import { foo } from './missing';
// TS2307: Cannot find module './missing'

foo.x;  // TS2304 suppressed because TS2307 already emitted
```

## Error Code

- **TS2307**: `CANNOT_FIND_MODULE` (error code 2307)
- Message: `"Cannot find module '{0}'"`

## Notes

- The implementation returns `TypeId::ANY` for unresolved modules to prevent cascading TS2571 errors
- The `is_unresolved_import_symbol()` check suppresses TS2304 errors when TS2307 was already emitted
- Dynamic imports and export declarations already had TS2307 emission (existing code in type_checking.rs)

## Future Enhancements

Potential improvements for additional coverage:
1. Better path mapping error messages (e.g., "@/utils not resolved from tsconfig paths")
2. Node.js module resolution details (package.json exports/imports field errors)
3. Type-only import vs value import distinction in error messages
4. Suggestions for similar module names (did you mean...?)

## Testing

To test:
1. Run the checker on the provided test files
2. Verify TS2307 errors are emitted for all missing module specifiers
3. Verify error messages include the module specifier string
4. Verify error locations point to the import specifier
