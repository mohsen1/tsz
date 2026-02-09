# Bug: TS2315 False Positives - Generic Interfaces Incorrectly Marked as Non-Generic

**Severity:** High
**Impact:** 34+ conformance tests failing
**Error Code:** TS2315 - "Type '{0}' is not generic"

## Description

When referencing a generic interface with type arguments, tsz incorrectly emits TS2315 saying the type is not generic, even though it clearly has type parameters.

## Minimal Reproduction

```typescript
interface Instance<Data> {
    get<K extends keyof Data>(name: K): unknown;
}

type Test = Instance<string>;
//          ^^^^^^^^ ERROR: TS2315 - Type 'Instance' is not generic
```

**Expected:** No error (Interface Instance IS generic with type parameter Data)
**Actual:** TS2315 error saying Instance is not generic

## Analysis

### Location
- File: `crates/tsz-checker/src/generic_checker.rs:131-151`
- Function: `validate_type_reference_type_arguments`

### Root Cause
```rust
let type_params = self.get_type_params_for_symbol(sym_id);
if type_params.is_empty() {
    // Emits TS2315 - Type is not generic
}
```

The function `get_type_params_for_symbol` returns an empty vector for generic interfaces, when it should return the interface's type parameters.

### Possible Issues

1. **Symbol Resolution:** May be resolving to wrong symbol
   - Looking up symbol in wrong scope/context
   - Finding a different symbol with same name that isn't generic

2. **Type Parameter Extraction:** `get_type_params_for_symbol` implementation
   - Located in `state_type_environment.rs:1210`
   - May not properly handle interface declarations
   - May not traverse to declaration to find type parameters

3. **Cross-File Symbols:** Symbol arena handling
   - Code has special handling for symbols from different arenas (line 1263+)
   - May not be delegating correctly for interface symbols

## Impact

**Conformance Tests Affected:** 34 tests with extra TS2315 errors

Example failing tests:
- `multipleInferenceContexts.ts` - Expected no error, got TS2315
- Many tests with generic interfaces being used with type arguments

## Recommended Fix

1. **Debug Symbol Resolution:**
   - Add tracing to `get_type_params_for_symbol` to see what symbol it finds
   - Verify it's finding the right declaration
   - Check if it's correctly extracting type parameters from interface declarations

2. **Test Type Parameter Flags:**
   - Verify interface symbol has correct flags (symbol_flags::INTERFACE)
   - Check if declarations are properly linked

3. **Check Fast Path Logic:**
   - Line 1253: Fast path only checks TYPE_ALIAS | CLASS | INTERFACE
   - Verify INTERFACE flag is set on interface symbols

## Workaround

None available - this is a compiler bug that blocks valid TypeScript code.

## Test Case

```typescript
// Should compile without errors
interface Box<T> {
    value: T;
}

type StringBox = Box<string>;  // TS2315: Type 'Box' is not generic (WRONG!)
```

## Related

- Error code: TS2315
- File: `generic_checker.rs:149`
- Diagnostic: TYPE_IS_NOT_GENERIC
