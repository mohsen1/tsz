# Bug: User Symbols Don't Shadow Lib Symbols Properly

## Status
**Identified but not fully fixed** - Needs comprehensive solution

## Symptom
When user code declares a variable that shadows a lib symbol (e.g., `var Symbol: SymbolConstructor`), the lib symbol is still resolved instead of the user's variable, causing false positive TS2339 errors.

## Reproduction
```typescript
// @target: ES5
interface SymbolConstructor {
    foo: string;
}
var Symbol: SymbolConstructor;

var obj = {
    [Symbol.foo]: 0  // Should work, but emits TS2339
}
obj[Symbol.foo];  // Should work, but emits TS2339
```

Expected: No errors (matches `tsc` behavior)
Actual: TS2339 - Property 'foo' does not exist on type '{ (description...: symbol...}'

## Root Cause

### Binding Flow
1. **Lib Merge** (`merge_lib_contexts_into_binder`): Lib symbols added to `file_locals`
2. **Binding Start** (`bind_source_file` lines 1560-1574):
   - Lib symbols copied from `file_locals` to **root persistent scope** (line 1563)
   - Lib symbols copied to `current_scope` (line 1571)
3. **User Declaration** (`declare_symbol`):
   - User's `var Symbol` creates new symbol (id=0)
   - Updates `current_scope` and `file_locals` to id=0
   - Path #3 now also tries to update root persistent scope
4. **Binding End** (`sync_current_scope_to_persistent` line 1605):
   - Copies `current_scope` to persistent scopes
   - May overwrite earlier updates if lib symbols re-added

### Resolution Flow
1. `resolve_identifier_with_filter` walks persistent scope chain (lines 792-827)
2. Finds lib Symbol (id=2477) in persistent scope before checking `file_locals`
3. Returns lib symbol instead of user symbol

## Impact
Affects ~78 tests with TS2339 false positives in conformance suite. Any user code that shadows built-in types/values will fail incorrectly.

## Attempted Fixes
1. ✅ Added function-scoped variables to `file_locals` (path #3, line 3605)
2. ✅ Added root persistent scope update in path #1 (shadowing, line 3507-3515)
3. ✅ Added root persistent scope update in path #3 (new symbol, line 3618-3626)
4. ❌ Still fails - persistent scopes not properly updated or overwritten later

## Proper Solution Options

### Option A: Change Resolution Order
Modify `resolve_identifier_with_filter` to check `file_locals` BEFORE walking persistent scopes for function-scoped symbols. This prioritizes user file-level declarations.

**Pros**: Matches TypeScript's resolution semantics
**Cons**: May affect other resolution scenarios, needs careful testing

### Option B: Prevent Lib Scope Pollution
Don't add lib symbols to persistent scopes at binding start. Keep them only in `file_locals` and handle resolution differently.

**Pros**: Cleaner separation of lib vs user symbols
**Cons**: Major refactor of binding/resolution logic

### Option C: Post-Binding Scope Cleanup
After user file binding completes, walk all persistent scopes and ensure user symbols from `file_locals` override any lib symbols with the same name.

**Pros**: Minimal disruption to existing logic
**Cons**: Extra pass needed, may be inefficient

### Option D: Shadow List
Maintain a separate "shadow list" of user symbols that should override lib symbols during resolution.

**Pros**: Surgical fix, doesn't change binding logic
**Cons**: Adds complexity to resolution path

## Recommended Solution
**Option A** - Change resolution order to prioritize `file_locals` for top-level lookups. This most closely matches TypeScript's behavior where file-level declarations shadow built-ins.

## Implementation Plan
1. Modify `resolve_identifier_symbol_inner` (line 303-397) to:
   - For file-level context (root scope), check `file_locals` first
   - Only fall back to scope chain if not found in `file_locals`
2. Ensure `file_locals` is properly populated for all file-level declarations
3. Test with ES5Symbol tests and full conformance suite
4. Add regression tests for common shadowing scenarios

## Related Files
- `crates/tsz-binder/src/state.rs` - `declare_symbol`, `resolve_identifier_with_filter`
- `crates/tsz-binder/src/state.rs` - `bind_source_file` (lib symbol initialization)
- `crates/tsz-checker/src/symbol_resolver.rs` - Checker's resolution wrappers

## Test Cases
- `TypeScript/tests/cases/conformance/Symbols/ES5SymbolProperty*.ts`
- `tmp/test-symbol-shadowing.ts` (minimal repro case)

## Date Identified
2026-02-12
