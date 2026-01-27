# TS2304 Name Resolution Investigation

## Current State

Based on conformance testing (Jan 27, 2026):
- **Missing TS2304 errors**: 34x (we should emit but don't)
- **Extra TS2304 errors**: 149x (we emit but shouldn't)
- **Total**: 183 errors to fix

## Error Emission Points

TS2304 "Cannot find name" is emitted in:
1. `/Users/mohsenazimi/code/tsz/src/checker/type_computation.rs` - Main identifier resolution
2. `/Users/mohsenazimi/code/tsz/src/checker/error_reporter.rs` - Error formatting
3. `/Users/mohsenazimi/code/tsz/src/checker/state.rs` - Various type checking contexts

## Symbol Resolution Flow

The `resolve_identifier_symbol()` in `/Users/mohsenazimi/code/tsz/src/checker/symbol_resolver.rs` (lines 275-694):
1. **Phase 0**: Check type parameter scope
2. **Phase 1**: Log debug info
3. **Phase 2**: Walk scope chain (local -> parent -> module)
4. **Phase 3**: Check file_locals (global scope from lib.d.ts)
5. **Phase 4**: Check lib binders' file_locals
6. **Phase 5**: Not found - return None

## Known Issues

### Extra Errors (149x)
We emit TS2304 when we shouldn't for:
1. Global values when lib is loaded (console, Math, etc.)
2. Names in shadowed scopes
3. Namespace members
4. Import aliases

### Missing Errors (34x)
We don't emit TS2304 when we should for:
1. Names in certain contexts (typeof, heritage clauses)
2. Static class members referenced incorrectly
3. Type-only imports used as values

## Test Cases to Investigate

### Extra Error Cases
- `/Users/mohsenazimi/code/tsz/TypeScript/tests/cases/compiler/unknownSymbols1.ts` - 13 TS2304 errors
- `/Users/mohsenazimi/code/tsz/TypeScript/tests/cases/compiler/unknownSymbols2.ts` - 10 TS2304 errors

### Missing Error Cases
- Need to find specific test cases where TSC emits TS2304 but we don't

## Next Steps

1. Investigate why we're emitting extra errors for globals
2. Check scope chain traversal for edge cases
3. Verify lib loading and symbol merging
4. Add targeted tests for each fix
