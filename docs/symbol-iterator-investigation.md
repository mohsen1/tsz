# Symbol.iterator Investigation

**Status:** In Progress
**Issue:** `Symbol.iterator` property not recognized, causing TS2339 errors
**Affects:** argumentsObjectIterator02_ES6.ts (and likely argumentsObjectIterator02_ES5.ts)

## Problem

When accessing `Symbol.iterator`, we emit:
```
TS2339: Property 'iterator' does not exist on type '{ ... }'
```

TSC correctly recognizes `Symbol.iterator` as a valid property.

## Investigation Findings

### 1. Symbol Type is Loaded

The Symbol type IS being created and HAS properties, including:
- `metadata`
- `dispose`
- `asyncDispose`
- `hasInstance`
- `isConcatSpreadable`
- `match`, `replace`, `search`, `split`
- `toPrimitive`
- `toStringTag`
- `unscopables`

But **missing `iterator`**!

### 2. WellKnownSymbolKey Enum Exists

File: `crates/tsz-solver/src/types.rs`

The enum includes:
```rust
pub enum WellKnownSymbolKey {
    Iterator,           // ✅ Defined
    AsyncIterator,
    HasInstance,
    // ... etc
}
```

The infrastructure exists to handle Symbol.iterator, but the Symbol type itself is missing the property.

### 3. Lib File Loading

File: `crates/tsz-binder/src/lib_loader.rs`

Lib files are loaded from disk (or provided in WASM). The Symbol type comes from TypeScript's lib files:
- `lib.es5.d.ts` - Basic Symbol interface
- `lib.es2015.symbol.d.ts` - Symbol constructor
- `lib.es2015.iterable.d.ts` - **Symbol.iterator property**

### 4. Hypothesis

The issue is likely one of:

**A. Missing lib file** - `lib.es2015.iterable.d.ts` is not being loaded
- Target is ES6/ES2015, which should include iterable
- Need to check lib file selection logic

**B. Symbol property filtering** - `iterator` is being filtered out during lib loading
- Other well-known symbols ARE present
- Something specific to `iterator` is being excluded

**C. Wrong lib version** - Using older lib without Symbol.iterator
- But we have other ES2015 symbols, so this seems unlikely

## Code Locations

### Lib Loading
- `crates/tsz-binder/src/lib_loader.rs` - LibLoader struct, loads .d.ts files
- `crates/tsz-cli/src/driver.rs` - Determines which libs to load based on target

### Symbol Handling
- `crates/tsz-solver/src/types.rs` - WellKnownSymbolKey enum (lines 417-494)
- `crates/tsz-checker/src/iterable_checker.rs` - Looks for `"[Symbol.iterator]"` property
- `crates/tsz-checker/src/type_computation_complex.rs` - Symbol type resolution (line 1784)

### Property Resolution
- `crates/tsz-checker/src/state_type_analysis.rs` - Property access type computation
- `crates/tsz-checker/src/type_checking.rs` - Property access checking

## Next Steps

### 1. Debug Lib Loading
Add tracing to see what lib files are being loaded:
```rust
#[tracing::instrument(level = "debug")]
fn load_default_libs(target: ScriptTarget) -> Vec<String> {
    tracing::debug!(?target, "Loading default libs for target");
    // ...
}
```

### 2. Check Lib File Selection
Find where the code decides which .d.ts files to load based on target:
- Search for "lib.es2015" in driver.rs or lib_loader.rs
- Check if iterable.d.ts is in the list

### 3. Verify Lib File Contents
If we're loading the right lib file, check if Symbol.iterator is actually in it:
```bash
grep -r "iterator" path/to/lib/files/*.d.ts
```

### 4. Check Symbol Type Construction
Add tracing when Symbol type is being constructed to see what properties are added:
- When parsing lib files
- When creating the global Symbol type
- When merging interface declarations

### 5. Test Workaround
Try explicitly referencing the iterable lib:
```typescript
/// <reference lib="es2015.iterable" />
function test() {
    let it = arguments[Symbol.iterator];
}
```

If this works, it confirms the lib file loading issue.

## Expected Behavior

TSC with `--target ES6` automatically includes:
- lib.es5.d.ts
- lib.es2015.d.ts (which includes Symbol.iterator via lib.es2015.iterable.d.ts)

Our compiler should do the same.

## Test Case

**File:** `tmp/args_iterator.ts`
```typescript
//@target: ES6

function doubleAndReturnAsArray(x: number, y: number, z: number) {
    let blah = arguments[Symbol.iterator];  // ❌ TS2339

    let result = [];
    for (let arg of blah()) {
        result.push(arg + arg);
    }
    return result;
}
```

**Expected:** No errors (or only TS2488 about iterator protocol)
**Actual:** TS2339 - Property 'iterator' does not exist

## Related Issues

This likely affects any code using:
- `Symbol.iterator`
- `for...of` loops on custom iterables
- Spread operator with iterables
- Array destructuring of iterables

May also affect argumentsObjectIterator02_ES5.ts (ES5 target).

## Priority

**Medium-High** - Affects 1-2 conformance tests, common feature (iterables)

## Estimated Complexity

**Medium** - Requires understanding lib file loading system, but infrastructure exists
