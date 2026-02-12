# Investigation: TS2708 False Positive in Failed Imports

**Date**: 2026-02-12
**Issue**: Conformance tests `aliasesInSystemModule1.ts` and `aliasesInSystemModule2.ts` failing
**Error Pattern**: Emitting TS2708 (Cannot use namespace as value) in addition to TS2792 (Cannot find module)

---

## Problem Description

When an import statement fails to resolve (TS2792), we're emitting a cascade error TS2708 when the import alias is used.

### Test Case

```typescript
// @module: system
// @isolatedModules: true

import alias = require('foo');  // TS2792: Cannot find module 'foo'
import cls = alias.Class;       // Cascade error starts here
export import cls2 = alias.Class;

let x = new alias.Class();      // TS2708: Cannot use namespace 'alias' as a value
let y = new cls();
let z = new cls2();
```

**Expected**: Only TS2792
**Actual**: TS2708 + TS2792

---

## Root Cause Analysis

### Code Location

`crates/tsz-checker/src/import_checker.rs` lines 957 and 985:

```rust
// Line 957: In qualified name checking
if is_type_only {
    // Emit TS2708: Cannot use namespace as a value
    self.error_namespace_used_as_value_at(&name, qn.left);
}

// Line 985: In namespace value export checking
if !has_value_exports {
    self.error_namespace_used_as_value_at(&name, qn.left);
}
```

### Issue

When `import alias = require('foo')` fails (module 'foo' not found), the symbol `alias` is still created but the import fails. Later, when we check uses of `alias.Class`, we detect that `alias` is being used as a value and emit TS2708.

However, TypeScript's behavior is to suppress this cascade error when the original import failed.

---

## Solution Approach

### Option 1: Check Symbol Error State

Add a check before emitting TS2708 to see if the symbol has an associated error (failed import):

```rust
// Before emitting TS2708, check if this is a failed import
if is_failed_import(symbol_id) {
    return; // Don't emit cascade error
}
self.error_namespace_used_as_value_at(&name, qn.left);
```

**Challenge**: Need to identify how failed imports are tracked. Possible approaches:
- Check if the symbol's target (for alias symbols) is missing/unresolved
- Check if the parent module resolution failed
- Look for existing error suppression mechanisms in the checker

### Option 2: Track Failed Imports

Maintain a set of failed import symbols during module resolution:

```rust
pub struct CheckerState {
    // ... existing fields
    failed_imports: FxHashSet<SymbolId>,
}
```

When a module fails to resolve, add the import alias symbol to `failed_imports`. Then check this set before emitting cascade errors.

### Option 3: Error Suppression Flag

Add an `has_error` or `suppress_cascading` flag to symbols that failed to resolve properly. Check this flag before emitting related errors.

---

## Investigation Findings

### Symbol Flags

Checked `crates/tsz-binder/src/lib.rs` lines 30-100. Symbol flags include:
- ALIAS (1 << 21)
- MODULE (VALUE_MODULE | NAMESPACE_MODULE)
- VALUE, TYPE, NAMESPACE composites

**No ERROR flag found** - error tracking is likely done elsewhere.

### Error Emission Sites

TS2708 is emitted in 5 locations:
1. `type_computation.rs:1376` - Property access on namespace
2. `error_reporter.rs:2228` - Helper function definition
3. `state_checking.rs:1815` - Heritage clause checking
4. `function_type.rs:991` - Function call on namespace member
5. `import_checker.rs:957, 985` - Import alias usage (our issue)

---

## Testing

### Reproduce

```bash
./scripts/conformance.sh run --filter "aliasesInSystemModule1"
```

**Current Output**:
```
expected: [TS2792]
actual:   [TS2708, TS2792]
```

### Affected Tests

- `aliasesInSystemModule1.ts`
- `aliasesInSystemModule2.ts`

Both tests have the same issue pattern.

---

## Impact

**Priority**: Medium
**Effort**: 2-4 hours (investigation + fix + testing)
**Tests Affected**: 2 conformance tests
**Pattern**: Similar cascade error suppression may be needed elsewhere

---

## Next Steps

1. **Find Error Tracking Mechanism**
   - Search for how TypeScript tracks failed module resolutions
   - Look for existing cascade error suppression
   - Check if symbols have error markers

2. **Implement Fix**
   - Add check in `import_checker.rs` before emitting TS2708
   - Verify failed import can be detected
   - Test with both failing test cases

3. **Verify No Regressions**
   - Run full test suite
   - Check other TS2708 emission sites aren't affected
   - Verify we still emit TS2708 for actual namespace-as-value errors

4. **Consider Similar Patterns**
   - Are there other cascade errors that should be suppressed?
   - Document the error suppression pattern for future use

---

## References

- Conformance test: `TypeScript/tests/cases/compiler/aliasesInSystemModule1.ts`
- Error code TS2708: "Cannot use namespace '{0}' as a value"
- Error code TS2792: "Cannot find module '{0}'"
- Implementation: `crates/tsz-checker/src/import_checker.rs`

---

## Status

**Status**: Investigation complete, fix not implemented
**Reason**: Requires deeper understanding of error tracking mechanism
**Recommendation**: Implement as part of larger error suppression refactoring
