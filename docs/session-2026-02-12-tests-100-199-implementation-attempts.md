# Session: Implementation Attempts for Tests 100-199

**Date**: 2026-02-12 (continued)
**Baseline**: 77/100 (77.0%)

## Attempted Fixes

### 1. TS1210 Implementation Investigation

**Test**: `argumentsReferenceInConstructor4_Js.ts`
**Expected**: `[TS1210]`
**Actual**: `[]` (from conformance report)

**Investigation Result**: TS1210 **IS ALREADY IMPLEMENTED** ✅

Location: `crates/tsz-checker/src/state_checking.rs:819-831`

```rust
// TS1210: 'arguments'/'eval' as variable name inside class body (implicit strict mode)
if self.ctx.enclosing_class.is_some() {
    if let Some(ref name) = var_name {
        if name == "arguments" || name == "eval" {
            use crate::types::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                var_decl.name,
                diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                &[name],
            );
        }
    }
}
```

**Manual Test**:
```bash
$ ./.target/dist-fast/tsz tmp/test-simple-arguments.ts --no-emit
error TS1210: Code contained in a class is evaluated in JavaScript's strict mode...
```

✅ Works for `.ts` files
✅ Works for `.js` files with `--declaration --emitDeclarationOnly --allowJs`

**Actual Issue**: The test is in "wrong-code" category, not "all-missing"!

When running the actual test file:
```bash
$ ./.target/dist-fast/tsz TypeScript/tests/cases/compiler/argumentsReferenceInConstructor4_Js.ts \
  --declaration --emitDeclarationOnly --allowJs

TypeScript/tests/cases/compiler/argumentsReferenceInConstructor4_Js.ts(23,9): error TS1210: ...
TypeScript/tests/cases/compiler/argumentsReferenceInConstructor4_Js.ts(18,3): error TS2339: Property 'foo' does not exist on type 'A'.
TypeScript/tests/cases/compiler/argumentsReferenceInConstructor4_Js.ts(28,3): error TS2339: Property 'bar' does not exist on type 'A'.
...
```

**Root Cause**: We emit TS1210 ✅ **BUT** we also emit extra TS2339 errors that TypeScript doesn't emit. This is because we're strictly checking JSDoc-annotated properties in JavaScript files.

TypeScript allows `this.foo = ...` in constructors to implicitly declare properties even when they're not explicitly declared in the class. We're more strict and emit TS2339.

**Resolution**: This is not a TS1210 missing implementation issue. It's a JSDoc/JS property checking issue (TS2339 false positives).

### 2. TS2792 vs TS2307 Debugging (Incomplete)

**Tests Affected**: 3 tests
- `amdDependencyComment1.ts`
- `ambientExternalModuleInAnotherExternalModule.ts`
- `amdDependencyCommentName1.ts`

**Expected**: TS2307 - "Cannot find module"
**Actual**: TS2792 - "Cannot find module... Did you mean to set moduleResolution?"

**Investigation**:
Added debug tracing to `import_checker.rs:module_not_found_diagnostic()` but tracing never appeared, suggesting the function isn't called or error is set elsewhere.

**Hypothesis**:
1. Driver may be pre-setting resolution error with code 2792
2. OR resolution logic is inverted somewhere in call chain
3. OR module kind detection is wrong for CommonJS

**Status**: Incomplete - requires deeper driver investigation

**Next Steps**:
1. Add tracing to `emit_module_not_found_error` in state_type_resolution.rs
2. Check if driver sets resolution error before checker runs
3. Verify CommonJS is correctly excluded from `module_kind_prefers_2792` match
4. Trace full resolution chain from driver → checker

## Summary

- ✅ TS1210 already works - test failure is due to TS2339 false positives
- ❌ TS2792 vs TS2307 - requires driver investigation (complex)
- ❌ No new tests fixed this session

## Lessons Learned

1. **Check conformance test categories carefully** - "all-missing" vs "wrong-code" distinction is important
2. **Manual testing reveals true issues** - conformance runner shows "expected: [TS1210], actual: []" but manual test shows we DO emit TS1210 plus extras
3. **JavaScript checking is stricter than TypeScript** - we emit more property errors for JSDoc-annotated JS files
4. **Debugging complex issues requires time** - TS2792 issue needs systematic tracing through driver

## Files Modified

- Attempted: `crates/tsz-checker/src/state_checking.rs` (debug logging)
- Attempted: `crates/tsz-checker/src/import_checker.rs` (debug logging)
- Reverted: All changes to preserve baseline

## Baseline Preserved

- Tests 100-199: **77/100 (77.0%)** ✅
- No regressions introduced
