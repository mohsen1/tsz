# Session Summary: Conformance Tests 100-199 Analysis

**Date**: 2026-02-12
**Slice**: Tests 100-199 (second 100 tests)
**Current Pass Rate**: 77/100 (77.0%)

## Summary

Analyzed failures in the second 100 conformance tests to identify high-impact fixes. Found two main categories of issues:
1. **TS2792 vs TS2307 confusion** (affects 3 tests)
2. **Symbol shadowing bug** (affects broader test suite but not specific to this slice)

## Test Results

### Baseline
- **Passed**: 77/100 (77.0%)
- **Failed**: 23/100
  - False positives: 7 tests (we emit errors TSC doesn't)
  - All missing: 4 tests (we don't emit expected errors)
  - Wrong codes: 12 tests (we emit different error codes)
  - Close (diff ≤2): 9 tests

### Top Error Code Issues

**False Positives (fix = instant wins)**:
- TS2322: 2 tests
- TS2351: 2 tests
- TS2345: 2 tests
- TS2449, TS2339, TS2488: 1 test each

**All Missing (implement = new passes)**:
- TS2458, TS7039, TS1210, TS2345, TS7006: 1 test each

**Wrong Codes (requires investigation)**:
- TS2792: 3 tests (emitting instead of TS2307)
- TS2339: 2 tests
- Others: 1 test each

## Issues Investigated

### 1. TS2792 vs TS2307 Module Resolution Error (3 tests)

**Tests Affected**:
- `amdDependencyComment1.ts`
- `ambientExternalModuleInAnotherExternalModule.ts`
- `amdDependencyCommentName1.ts`

**Expected**: TS2307 - "Cannot find module"
**Actual**: TS2792 - "Cannot find module... Did you mean to set moduleResolution?"

**Root Cause**:
Logic in both `import_checker.rs` and `state_type_resolution.rs` determines error code based on module kind:
```rust
let module_kind_prefers_2792 = matches!(
    module_kind,
    System | AMD | UMD | ES2015 | ES2020 | ES2022 | ESNext | Preserve
);
```

For CommonJS (used in these tests), `module_kind_prefers_2792` should be FALSE, so TS2307 should be emitted. But TS2792 is being emitted instead.

**Investigation Status**:
- Added debug tracing to `module_not_found_diagnostic()`
- Debug output never appeared, suggesting function not called
- Error likely coming from driver setting resolution error with code 2792
- OR logic inversion somewhere in the call chain

**Files Involved**:
- `crates/tsz-checker/src/import_checker.rs:33-72`
- `crates/tsz-checker/src/state_type_resolution.rs:1661-1726`
- `crates/tsz-cli/src/driver.rs` (module resolution)

**Next Steps**:
1. Add tracing to `emit_module_not_found_error` in state_type_resolution.rs
2. Check if driver is pre-setting resolution error with 2792
3. Verify CommonJS is correctly excluded from the module_kind match
4. Test with other module types to see if pattern holds

### 2. Symbol Shadowing Bug (documented in separate file)

Found during investigation - user-declared variables don't properly shadow lib symbols.
See: `docs/bugs/symbol-shadowing-lib-bug.md`

This is a fundamental binder/resolution issue affecting ~78-85 tests across the full test suite.

## Close Tests (9 tests, diff ≤2)

| Test | Expected | Actual | Missing | Extra |
|------|----------|--------|---------|-------|
| `allowSyntheticDefaultImports8.ts` | TS2305 | TS1192 | TS2305 | TS1192 |
| `ambientExternalModuleInAnotherExternalModule.ts` | TS2307, TS2664 | TS2664, TS2792 | TS2307 | TS2792 |
| `ambientExportDefaultErrors.ts` | TS2714 | TS2304 | TS2714 | TS2304 |
| `ambientPropertyDeclarationInJs.ts` | TS2322, TS2339, TS8009, TS8010 | TS2322, TS2339 | TS8009, TS8010 | - |
| `amdDependencyCommentName1.ts` | TS2307 | TS2792 | TS2307 | TS2792 |
| `ambiguousGenericAssertion1.ts` | TS1005, TS1109, TS2304 | TS1005, TS1109, TS1434 | TS2304 | TS1434 |
| `anonymousClassExpression2.ts` | TS2551 | TS2339 | TS2551 | TS2339 |
| `amdDependencyComment1.ts` | TS2307 | TS2792 | TS2307 | TS2792 |
| `argumentsBindsToFunctionScopeArgumentList.ts` | TS2322 | TS2739 | TS2322 | TS2739 |

## Quick Win Opportunities

### Single Missing Error Code (3 tests)

**TS2458** - 1 test (`amdModuleName2.ts`)
- "Expecting 1 argument but got..."
- Not currently implemented

**TS7039** - 1 test
- "Parameter is not used"
- Not currently implemented

**TS1210** - 1 test (`argumentsReferenceInConstructor4_Js.ts`)
- "Code in class evaluated in strict mode..."
- Class constructor strict mode violations
- Test: `const arguments` in class constructor (JS file with JSDoc)

### Paired Errors (1 test)

**TS2345 + TS7006** - 1 test
- Implementing both would pass the test

## Recommendations

### Priority 1: TS2792 vs TS2307 Fix (3 tests)
**Estimated Impact**: 3 tests → 80/100 (80.0%)
**Complexity**: Medium
**Action**: Debug why CommonJS modules emit TS2792 instead of TS2307

### Priority 2: Symbol Shadowing Fix (broader impact)
**Estimated Impact**: ~78-85 tests across full suite
**Complexity**: High (requires binder/resolution refactor)
**Action**: Implement resolution order change (see bug doc)

### Priority 3: Implement Missing Error Codes
**Estimated Impact**: 1-3 tests per code
**Complexity**: Low-Medium per code
**Candidates**:
- TS2458 (function arity)
- TS7039 (unused parameter)
- TS1210 (strict mode in classes)
- TS8009/TS8010 (ambient property declarations in JS)

### Priority 4: False Positives
**Estimated Impact**: 7 tests
**Complexity**: Varies
**Candidates**:
- TS2322, TS2351, TS2345 (2 tests each)
- TS2449, TS2339, TS2488 (1 test each)

## Files Modified

- `crates/tsz-checker/src/import_checker.rs` - Added debug tracing
- `docs/bugs/symbol-shadowing-lib-bug.md` - Documented separate bug
- `tmp/test-symbol-shadowing.ts` - Test case for symbol shadowing
- `tmp/test-module-error.ts` - Test case for TS2792 vs TS2307

## Current Status

- ✅ Baseline preserved: 77/100 (77.0%)
- ✅ TS2792 issue root cause partially identified
- ✅ Symbol shadowing bug fully documented
- ❌ TS2792 fix incomplete (debug tracing didn't reveal issue)
- ❌ No new tests passing this session

## Next Session Actions

1. **Complete TS2792 debugging**: Add more tracing, check driver
2. **Implement quick win**: TS2458 or TS7039 (1-test gains)
3. **Fix false positives**: Focus on TS2322/TS2351/TS2345 patterns
4. **Coordinate**: Symbol shadowing fix affects multiple slices

## Time Spent

- Analysis: ~20 minutes
- TS2792 debugging: ~40 minutes
- Symbol shadowing investigation: ~60 minutes (carried over from previous work)
- Documentation: ~15 minutes

**Total**: ~2 hours 15 minutes
