# JavaScript File Type Checking Issue

**Priority:** HIGH
**Impact:** Blocks TS1210 fix, likely affects multiple tests
**Discovery Date:** 2026-02-13

## Problem

JavaScript files (`.js`) are not being type-checked when using `--allowJs` flag, even though the checking infrastructure exists and works correctly for TypeScript files.

## Evidence

### Test Case 1: TypeScript File (WORKS)
```typescript
// tmp/args_test.ts
class A {
    constructor(foo = {}) {
        const arguments = this.arguments;  // ✅ Emits TS1210
    }
    get arguments() { return { bar: {} }; }
}
```

**Command:** `./.target/dist-fast/tsz tmp/args_test.ts`
**Result:** ✅ Correctly emits TS1210 error

### Test Case 2: JavaScript File (BROKEN)
```javascript
// tmp/args_test.js
class A {
    constructor(foo = {}) {
        const arguments = this.arguments;  // ❌ Should emit TS1210
    }
    get arguments() { return { bar: {} }; }
}
```

**Commands Tried:**
```bash
./.target/dist-fast/tsz --allowJs tmp/args_test.js
./.target/dist-fast/tsz --allowJs --noEmit tmp/args_test.js
./.target/dist-fast/tsz --allowJs --declaration --emitDeclarationOnly tmp/args_test.js
```

**Result:** ❌ No output at all (file not checked)

### Test Case 3: Comparison with TSC
```bash
tsc --allowJs --declaration --emitDeclarationOnly --noEmit tmp/args_test.js
```
**Result:** ✅ TSC correctly emits TS1210 for JS files

## Affected Tests

### Confirmed Affected
- `argumentsReferenceInConstructor4_Js.ts` - Expects TS1210 for .js file

### Likely Affected
- Any conformance test that uses `@allowJs: true` with `.js` files
- Tests expecting type errors in JavaScript files
- Declaration emit from JavaScript sources

## Root Cause Analysis

The TS1210 implementation is **complete and working**:
- ✅ Diagnostic defined in `diagnostics.rs` (code 1210)
- ✅ Check logic exists in `state_checking.rs` (lines 829-848)
- ✅ Correctly detects 'arguments' variable in class context
- ✅ Emits error for TypeScript files

The problem is **file selection/processing**:
- ❌ JS files are being filtered out or skipped
- ❌ Type checking not run for `--allowJs` files
- ❌ May be related to `emitDeclarationOnly` mode interaction

## Investigation Paths

### 1. Driver File Selection
**File:** `crates/tsz-cli/src/driver.rs`

Check where input files are selected and processed:
- How does `--allowJs` flag affect file selection?
- Are `.js` files added to the compilation list?
- Is there a filter that removes them?

Search for:
- `allow_js` or `allowJs` in driver
- File extension filtering logic
- Input file collection

### 2. Checker Entry Point
**File:** `crates/tsz-checker/src/checker/mod.rs`

Check if checker receives JS files:
- Are JS files passed to `check_source_file()`?
- Is there early return for JS files?
- Any special handling based on file extension?

### 3. Parser/Binder for JS
**Files:**
- `crates/tsz-parser/src/`
- `crates/tsz-binder/src/`

Check if JS files are parsed/bound correctly:
- Does parser handle `.js` files?
- Does binder process JS ASTs?
- Are symbols created for JS declarations?

### 4. Configuration Flow
**Files:**
- `crates/tsz-cli/src/args.rs` - CLI argument parsing
- `crates/tsz-common/src/checker_options.rs` - Checker options

Check if `allowJs` flag flows through:
- Is `allowJs` parsed from CLI?
- Is it stored in compiler options?
- Is it passed to checker/binder?

## Debugging Strategy

### Step 1: Add Tracing to Driver
```rust
// In driver.rs, where files are collected
#[tracing::instrument(level = "debug")]
fn collect_input_files(options: &CompilerOptions) -> Vec<PathBuf> {
    tracing::debug!(?options.allow_js, "Collecting input files");
    // ...
    for file in files {
        tracing::debug!(?file, ext = ?file.extension(), "Found input file");
    }
}
```

### Step 2: Verify File Reaches Checker
```rust
// In checker/mod.rs
pub fn check_source_file(&mut self, file: &SourceFile) {
    tracing::info!(
        path = %file.path.display(),
        is_js = file.path.extension() == Some("js"),
        "Checking source file"
    );
}
```

### Step 3: Check allowJs Flag
```bash
# Verify flag is recognized
./.target/dist-fast/tsz --showConfig tmp/args_test.js | grep allowJs
```

### Step 4: Compare with Working Case
- Find a passing conformance test that uses `.js` files
- Compare how it's configured vs failing test
- Identify what makes it work

## Expected Behavior

When `--allowJs` is enabled:
1. Compiler should accept `.js` files as input
2. Parser should parse JS files (with JS syntax rules)
3. Binder should bind symbols from JS
4. Checker should type-check JS with JSDoc comments
5. All error checks (including TS1210) should run

This is standard TypeScript behavior - `allowJs` enables full type checking of JavaScript files.

## Workarounds

None currently. JS file checking must be fixed.

## Next Steps

1. **Immediate:** Add tracing to driver to see if JS files are collected
2. **Debug:** Run with tracing to find where JS files are filtered
3. **Fix:** Remove filter or add JS files to processing pipeline
4. **Test:** Verify TS1210 works for JS files after fix
5. **Verify:** Run conformance tests to check improvements

## Impact Assessment

**High Impact** - This is a fundamental gap:
- Blocks TS1210 fix (1 test)
- Likely affects other JS file tests
- Missing core TypeScript feature
- May affect real-world usage

Fixing this could improve multiple conformance tests at once.

## Related Issues

- #4 - TS1210 implementation (blocked by this)
- Conformance test: argumentsReferenceInConstructor4_Js.ts
- Any test with `@allowJs: true` and `.js` extension

## Priority Justification

**HIGH** because:
1. Blocks a quick-win fix (TS1210)
2. Fundamental feature gap (allowJs)
3. Likely affects multiple tests
4. Core TypeScript compatibility issue

Should be prioritized over complex issues like Symbol.iterator or ambient declarations.
