# Conformance Test Analysis - Slice 4 of 4

**Date:** 2026-02-08
**Slice:** Offset 4092, Max 1364 tests
**Pass Rate:** 789/1324 (59.6%)
**Skipped:** 40

## Top Error Code Mismatches

| Error Code | Missing | Extra | Net Impact | Description |
|------------|---------|-------|------------|-------------|
| TS2322 | 17 | 52 | +35 false positives | Type not assignable |
| TS2339 | 17 | 48 | +31 false positives | Property doesn't exist |
| TS2345 | 9 | 48 | +39 false positives | Argument not assignable |
| TS2304 | 22 | 19 | -3 missing | Cannot find name |
| TS1005 | 15 | 23 | +8 false positives | Expected token (parser) |
| TS2307 | 13 | 22 | +9 false positives | Cannot find module |
| TS1128 | 6 | 22 | +16 false positives | Declaration or statement expected |
| TS2305 | 13 | 6 | -7 missing | Module has no exported member |
| TS7006 | 2 | 15 | +13 false positives | Parameter implicitly has 'any' type |
| TS2792 | 14 | 2 | -12 missing | Cannot find module (different context) |

## Failure Categories

### 1. False Positives (127 tests)
We emit errors that TSC doesn't. Indicates we're being too strict.

**Impact:** These are high-priority because they directly reduce pass rate and user experience.

**Common patterns:**
- Over-reporting TS2322/TS2339/TS2345 in union type contexts
- Emitting errors for valid module/namespace access patterns
- Stricter checks on type compatibility than TSC

### 2. All Missing (158 tests)
We don't emit any errors where TSC expects them.

**Common patterns:**
- Missing parser errors (TS2796 - missing commas in arrays)
- Missing function implementation checks in module augmentation contexts (TS2391)
- Missing discriminated union specific errors (TS2353)
- Missing DOM intrinsic checks (TS2812)
- Missing module augmentation validation (TS2666, TS2667, TS2669)

### 3. Close to Passing (76 tests)
Differ by 1-2 error codes only.

**Examples:**
- `moduleProperty2.ts`: Missing TS2339 for accessing non-exported namespace members
- `moduleAugmentationImportsAndExports3.ts`: Missing TS2667 (only 1 error diff)
- `recursiveBaseCheck5.ts`: Missing TS2310 (only 1 error diff)

These are good candidates for targeted fixes.

## Specific Issues Identified

### Issue 1: Discriminated Union Handling
**Test:** `missingDiscriminants.ts`, `missingDiscriminants2.ts`
**Expected:** TS2353 (Object literal may only specify known properties)
**Actual:** Multiple TS2322 (Type not assignable)

**Root cause:** When checking object literal assignments to union types, we're doing general assignability checks instead of:
1. Detecting discriminant properties
2. Narrowing to the appropriate union member
3. Reporting TS2353 for unknown properties

**Files involved:**
- `crates/tsz-solver/` - discriminant detection and narrowing
- `crates/tsz-checker/` - object literal checking logic

### Issue 2: Parser - Missing Comma Detection
**Test:** `missingCommaInTemplateStringsArray.ts`
**Expected:** TS2796
**Actual:** No error

**Code:**
```typescript
var array = [
    `template string 1`
    `template string 2`  // Missing comma
];
```

**Root cause:** Parser accepts consecutive template strings in array literals without commas. Likely an ASI (Automatic Semicolon Insertion) issue or parser bug.

**Files involved:**
- `crates/tsz-parser/` - array literal parsing

### Issue 3: Function Implementation Checks
**Test:** `missingFunctionImplementation2.ts`
**Expected:** TS2391
**Actual:** No error

**Context:** Multi-file test with module augmentation.

**Current implementation:** `crates/tsz-checker/src/type_checking_queries.rs:1624` has `check_function_implementations` but it only checks for overload signatures without implementations within the same file.

**Root cause:** Missing check for functions without bodies AND without return types, especially in module augmentation contexts.

### Issue 4: DOM Intrinsic Types
**Test:** `missingDomElements.ts`
**Expected:** TS2812 (specific DOM error)
**Actual:** TS2339 (generic property error)

**Root cause:** We're not detecting when an interface like `HTMLElement` or `Node` is user-defined vs DOM intrinsic, so we emit generic property errors instead of specific DOM-related errors.

**Files involved:**
- `crates/tsz-solver/` - type classification
- `crates/tsz-checker/` - error reporting

### Issue 5: Module Property Visibility
**Test:** `moduleProperty2.ts`
**Expected:** TS2339 when accessing `M.y` (non-exported member)
**Actual:** Only TS2304 for `x`

**Code:**
```typescript
namespace M {
    var y;      // Not exported
    export var z;
}

namespace N {
    var test3=M.y; // Should error: y is private to M
}
```

**Root cause:** Not checking visibility/export status when accessing namespace members.

**Files involved:**
- `crates/tsz-binder/` - symbol visibility tracking
- `crates/tsz-checker/` - member access checking

## Recommended Fix Priority

### High Impact, Low Risk
1. **Module property visibility** (Issue 5) - Clear spec, isolated change
2. **Close to passing tests** - Only 1-2 error differences, easier to fix

### High Impact, Medium Risk
3. **Parser comma detection** (Issue 2) - Parser changes can be tricky but this is isolated
4. **Function implementation checks** (Issue 3) - Existing infrastructure, needs extension

### High Impact, High Risk
5. **Discriminated union handling** (Issue 1) - Core type system logic, affects many tests
6. **Reducing false positives** - Requires careful analysis to not introduce regressions

### Lower Priority
7. **DOM intrinsic detection** (Issue 4) - Specialized, affects fewer tests

## Unit Test Status

- **Passed:** 2303
- **Failed:** 1 (pre-existing: `isolated_test_runner::tests::test_isolated_runner_panic`)
- **Ignored:** 106

## Next Steps

1. Start with "close to passing" tests - target the 76 tests that differ by only 1-2 errors
2. Fix module property visibility (Issue 5) - clear, isolated, helps multiple tests
3. Add missing comma detection in parser (Issue 2) - specific, testable
4. Gradually tackle false positives by analyzing specific patterns

## Notes

- **DO NOT** make broad changes to type checking without extensive testing
- **ALWAYS** run `cargo nextest run` before committing
- **VERIFY** conformance test impact with `./scripts/conformance.sh run --offset 4092 --max 1364`
- Consider adding targeted unit tests for each fix to prevent regressions
