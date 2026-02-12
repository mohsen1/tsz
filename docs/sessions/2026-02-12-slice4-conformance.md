# Conformance Session 2026-02-12: Slice 4 Improvements

## Session Goal
Improve conformance test pass rate for slice 4 (tests 9438-12583 of 12583 total).

## Starting State
- Pass rate: 1669/3123 (53.4%)
- Slice: offset=9438, max=3145

## Analysis

### Top Issues Identified
1. **TS2318 false positives (83 tests)**: Cannot find global type errors in JSX tests
   - All JSX-related tests failing to resolve `JSX.Element` and similar qualified names
   - Root cause: Qualified name resolution not looking up symbols across lib binders

2. **TS2339 false positives (121 tests)**: Property access errors when shouldn't emit

3. **TS2304 false positives (118 tests)**: Name not found errors when shouldn't emit

4. **TS1005 false positives (85 tests)**: Parse errors when shouldn't emit
   - Many JSX-related parse failures
   - Some semantic errors being reported as parse errors (e.g., `interface Foo extends Foo`)

## Changes Made

### Fix 1: Cross-binder lookup for qualified type names
**File**: `crates/tsz-checker/src/symbol_resolver.rs:696-700`

**Problem**: When resolving qualified type names like `JSX.Element`:
- We correctly resolved the left side (`JSX` namespace) using lib binders
- But then used only the current file's binder to look up the right side member (`Element`)
- This meant JSX namespace members from lib files (react.d.ts) were invisible

**Solution**: Changed `get_symbol(left_sym)` to `get_symbol_with_libs(left_sym, &lib_binders)`

```rust
// Before:
let Some(left_symbol) = self.ctx.binder.get_symbol(left_sym) else {
    return TypeSymbolResolution::NotFound;
};

// After:
let lib_binders = self.get_lib_binders();
let Some(left_symbol) = self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders) else {
    return TypeSymbolResolution::NotFound;
};
```

**Impact**: +2 tests passing (though TS2318 issue persists - may need deeper lib symbol merging investigation)

## Final State
- Pass rate: 1672/3123 (53.5%)
- Improvement: +3 tests (+0.1 percentage points)

## Error Code Trends (Final)
```
  TS2304: missing=138, extra=118
  TS2339: missing=70, extra=139
  TS2322: missing=112, extra=69
  TS1005: missing=44, extra=85
  TS6053: missing=103, extra=0
  TS2345: missing=20, extra=69
  TS2318: missing=6, extra=83
  TS1109: missing=18, extra=63
  TS1128: missing=9, extra=48
  TS2552: missing=20, extra=18
```

## Key Insights

### JSX Type Resolution Architecture
- JSX types are defined in a `JSX` namespace (usually from react.d.ts)
- References like `JSX.Element` are QUALIFIED_NAME nodes
- Resolution requires coordinating between:
  1. Identifier resolution (finding `JSX` namespace)
  2. Qualified name traversal (finding `Element` within JSX's exports)
  3. Cross-binder symbol lookup (JSX may be in lib, Element in its exports)

### Symbol ID Space Complexity
- After lib merge, symbols have IDs in the file binder's ID space
- But exports maps may still reference lib binder IDs
- `get_symbol_with_libs` handles this with fast path for merged symbols

## Next Steps

### High Priority
1. **Investigate TS2318 persistence**: Despite the fix, 83 JSX tests still fail with TS2318
   - May need to trace symbol export lookups during lib merge
   - Check if exports maps are properly updated with merged symbol IDs

2. **TS1005 parse errors**: 85 false positive parse errors
   - Many in JSX (type assertions vs JSX tags ambiguity)
   - Some semantic errors reported as parse errors (self-referencing interfaces)

3. **TS2339 false positives**: 139 property access errors (increased from 121)
   - May be related to namespace member lookups
   - Could be side effect of changes or test variance

### Medium Priority
4. **TS2304 false positives**: 118 name resolution errors
5. **TS2322 missing**: 112 tests need assignability errors we don't emit

## Commits
- `48068e4ff`: fix: use cross-binder lookup for qualified type name resolution
