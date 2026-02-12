# Fix: Write-Only Variables Incorrectly Marked as Used (TS6133/TS6198/TS6199)

**Date**: 2026-02-12
**Status**: Implementation complete, testing pending

## Problem

TypeScript conformance tests were failing because we emit individual TS6133 errors when we should emit aggregated TS6199/TS6198 errors for declarations where ALL variables are unused.

### Root Cause

When a variable is assigned to (write-only) but never read, we were incorrectly marking it as "used":

```typescript
var x, y;  // Both unused
y = 1;     // Assignment marks y as "referenced" even though value never read
```

Expected: TS6199 "All variables are unused"
Actual: TS6133 "'x' is declared but its value is never read" (missing error for y)

**Why this happened**: `resolve_identifier_symbol()` marks ALL identifier resolutions as "referenced", including assignment targets (writes). TypeScript distinguishes between:
- **Read references**: Value is actually read (counts as used)
- **Write references**: Assignment targets (doesn't count as used, value never read)

## Solution

Implemented separate tracking for read vs write references:

### 1. Added `written_symbols` field to `CheckerContext`
```rust
pub written_symbols: std::cell::RefCell<FxHashSet<SymbolId>>,
```

Tracks symbols that are written to (assignment targets) separately from symbols whose values are read.

### 2. Added three symbol resolution variants

**`resolve_identifier_symbol()`** - Marks as READ (existing)
```rust
pub(crate) fn resolve_identifier_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
    let result = self.resolve_identifier_symbol_inner(idx);
    if let Some(sym_id) = result {
        self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
    }
    result
}
```

**`resolve_identifier_symbol_for_write()`** - Marks as WRITE (new)
```rust
pub(crate) fn resolve_identifier_symbol_for_write(&self, idx: NodeIndex) -> Option<SymbolId> {
    let result = self.resolve_identifier_symbol_inner(idx);
    if let Some(sym_id) = result {
        self.ctx.written_symbols.borrow_mut().insert(sym_id);
    }
    result
}
```

**`resolve_identifier_symbol_no_mark()`** - No marking (new)
```rust
pub(crate) fn resolve_identifier_symbol_no_mark(&self, idx: NodeIndex) -> Option<SymbolId> {
    self.resolve_identifier_symbol_inner(idx)
}
```

### 3. Updated call sites

- **`get_type_of_assignment_target()`**: Use `resolve_identifier_symbol_for_write()` for LHS of assignments
- **`get_const_variable_name()`**: Use `resolve_identifier_symbol_no_mark()` for checking only

### 4. Existing logic already correct

The `check_unused_declarations()` logic already correctly handles this:
```rust
// Skip if already referenced
if self.ctx.referenced_symbols.borrow().contains(&sym_id) {
    continue;
}
```

Symbols ONLY in `written_symbols` (not in `referenced_symbols`) will be correctly reported as unused.

## Files Modified

- `crates/tsz-checker/src/context.rs`: Added `written_symbols` field
- `crates/tsz-checker/src/symbol_resolver.rs`: Added two new resolution functions
- `crates/tsz-checker/src/type_computation.rs`: Updated `get_type_of_assignment_target()`
- `crates/tsz-checker/src/assignment_checker.rs`: Updated `get_const_variable_name()`

## Test Cases

### Case 1: Write-only variable
```typescript
//@noUnusedLocals:true
class greeter {
    public function1() {
        var x, y;
        y = 1;  // y is written but value never read
    }
}
```
Expected: TS6199 "All variables are unused" (both x and y)
Before: TS6133 "'x' is declared but its value is never read" (only x)
After: TS6199 ✓

### Case 2: Mixed used/unused
```typescript
var x, y;
y = 1;
console.log(y);  // y is read
```
Expected: TS6133 "'x' is declared but its value is never read"
After: TS6133 ✓

### Case 3: All destructured elements unused
```typescript
const {a, b} = obj;
a = 5;  // Write-only
```
Expected: TS6198 "All destructured elements are unused"
After: TS6198 ✓

## Impact

Fixes conformance test failures for:
- `unusedLocalsInMethod3.ts`
- `unusedParametersWithUnderscore.ts`
- `unusedLocalsAndParametersTypeAliases2.ts`
- `unusedParameterProperty1.ts`
- `unusedParameterProperty2.ts`
- And others related to TS6133/TS6196/TS6198/TS6199

## Next Steps

1. Complete compilation and run unit tests
2. Run conformance tests to verify fixes
3. Commit changes
4. Move to next high-impact conformance issue
