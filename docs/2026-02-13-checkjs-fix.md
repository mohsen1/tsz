# Fix: JavaScript Type Checking Now Works

## The Bug
The `--checkJs` CLI flag was being parsed but never applied to compiler options, causing JavaScript files to never be type-checked even when explicitly requested.

## The Fix
Added 3 lines to `apply_cli_overrides()` in `crates/tsz-cli/src/driver.rs`:

```rust
if args.check_js {
    options.check_js = true;
}
```

This matches the existing pattern for `--allowJs`.

## Impact

### Positive
- ✅ JavaScript files are now type-checked when `--checkJs` is specified
- ✅ TS1210 (arguments shadowing in class constructors) now correctly detected
- ✅ Enables checking of 3+ JS-related conformance tests
- ✅ All 2394 unit tests still pass

### Negative  
- ❌ Pass rate went from 90/100 to 89/100 (lost 1 test)
- ❌ Exposed existing bugs in JavaScript type checking:
  - **argumentsReferenceInConstructor3_Js**: False positive TS2340
    - "Only public/protected methods accessible via super"
    - Error on `super.arguments.foo` where `arguments` is a public getter
  - **argumentsReferenceInConstructor4_Js**: Extra false positive TS2339
    - Expected: [TS1210]
    - Actual: [TS1210, TS2339]
    - TS2339: "Property does not exist"

## Root Cause Analysis

The test regression is NOT caused by the fix itself - the fix is correct. Rather, enabling JavaScript checking exposed pre-existing bugs in the type checker:

1. **TS2340 False Positive**: The checker incorrectly reports that `super.arguments` is not accessible, even though `arguments` is a public getter in the parent class.

2. **TS2339 False Positive**: The checker reports a non-existent property error on valid JavaScript code in class constructors.

## Test Results

**Before Fix:**
- 90/100 passing (90%)
- JavaScript files not being checked at all
- TS1210 never emitted

**After Fix:**
- 89/100 passing (89%)
- JavaScript files now checked when requested  
- TS1210 correctly emitted
- False positives exposed in 2 tests

## Next Steps

To restore and improve the pass rate:

1. **Fix TS2340 false positive** in super property access
   - Location: Property access type checking for super keyword
   - Issue: Not recognizing public getters as accessible

2. **Fix TS2339 false positive** in JavaScript constructors
   - Location: Property resolution in JavaScript files
   - Issue: Likely related to `this.arguments` vs built-in `arguments`

3. **Investigate type annotation handling** in JavaScript
   - The `/** @type object */` annotations may not be processed correctly
   - May be emitting errors that should be suppressed

These fixes should bring the pass rate to 91/100 or higher by:
- Removing 2 false positives (-2 failures)
- Correctly detecting TS1210 (+1 pass)
- Net: +1 to +2 tests passing

## Commit
`d9ebb5e5e` - fix(cli): apply --checkJs flag from CLI arguments
