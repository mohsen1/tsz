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
- Pass rate: 1670/3123 (53.5%)
- Improvement: +1 test (within test variance margin)

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

## Additional Investigations

### TS6053 (File not found) - 104 missing
- **Issue**: Triple-slash reference paths not emitting file-not-found errors
- **Code exists**: `check_triple_slash_references` in `state_checking.rs:2967-3073`
- **Called**: Line 164 in source file checking
- **Root cause**: Logic exists but may not properly match virtual files from multi-file tests
  - Test pattern: `// @filename: declaration.d.ts` creates virtual file
  - Reference: `///<reference path="declaration.d.ts" />` should find it
  - May be path matching issue between virtual file names and reference paths
- **Impact**: 104 tests, many JSX-related with react.d.ts references

### Type Parameter Scoping in Heritage Clauses
- **Issue**: `interface Foo2<T> extends Base2<T>` emits extra TS2304 for T
- **Tests affected**: interfaceWithPropertyThatIsPrivateInBaseType.ts (and variant)
- **Root cause**: Type parameter T not in scope when resolving Base2<T> in heritage clause
- **Pattern**: Close-to-passing tests (diff=1), need to suppress TS2304 for type params in heritage

### TS1005 Parse Errors
- **Issue**: 85 false positive parse errors
- **Example**: `interface Foo extends Foo` emits TS1005 instead of TS1176/TS2310
- **Root cause**: Parser rejecting valid (but erroneous) syntax instead of parsing and letting checker validate
- **Pattern**: Self-referencing interfaces treated as parse errors

## Next Steps

### High Priority
1. **TS6053 missing**: Investigate virtual file path matching in `check_triple_slash_references`
   - Check if all_arenas properly includes multi-file test virtual files
   - Trace path matching logic (absolute vs relative, stem matching)
   - 104 tests affected, relatively isolated fix

2. **Type parameter scoping**: Ensure heritage clause type arguments have access to interface's type params
   - Multiple close-to-passing tests (each diff=1)
   - Clear pattern to fix

3. **TS2318 persistence**: Despite qualified name fix, 83 JSX tests still fail
   - Deeper lib symbol merge investigation needed
   - May require tracing at runtime with tsz-tracing skill

### Medium Priority
4. **TS1005 parse errors**: 85 false positives
   - Parser architecture change to accept more syntax
   - Lower priority due to complexity

5. **TS2339/TS2304 false positives**: ~138 each
   - May improve with symbol resolution fixes
   - Need pattern analysis

6. **TS2322 missing**: 112 tests need assignability errors we don't emit
   - Requires assignability checker improvements

## Commits
- `48068e4ff`: fix: use cross-binder lookup for qualified type name resolution
