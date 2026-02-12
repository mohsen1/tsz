# Conformance Tests 100-199: Session Progress

**Date**: 2026-02-12
**Starting Pass Rate**: 77/100 (77.0%)
**Current Pass Rate**: 81/100 (81.0%)
**Improvement**: +4 tests (+4 percentage points)

## Completed Work

### ✅ Implemented TS7039 - Mapped types with implicit any

**File**: `crates/tsz-checker/src/state_checking_members.rs`

**Change**: Added check in `check_type_for_missing_names` for mapped types that lack explicit value types when `noImplicitAny` is enabled.

```rust
} else if self.ctx.no_implicit_any() {
    // TS7039: Mapped object type implicitly has an 'any' template type
    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
    self.error_at_node(
        type_idx,
        diagnostic_messages::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
        diagnostic_codes::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
    );
}
```

**Test passing**: `anyMappedTypesError.ts`

**Example**:
```typescript
// @noImplicitAny: true
type Foo = {[P in "bar"]};  // Now correctly emits TS7039
```

### ✅ Fixed build error

**File**: `crates/tsz-checker/src/type_checking.rs`

**Change**: Made `destructuring_patterns` mutable to fix compilation error.

## Remaining Issues (19 failing tests)

### False Positives (7 tests) - We emit errors TSC doesn't

Priority: HIGH - These are easier wins, each test fixed = +1 pass rate

| Test | Extra Errors | Root Cause | Difficulty |
|------|--------------|------------|------------|
| `amdLikeInputDeclarationEmit.ts` | TS2339 | Module resolution with AMD/JSDoc types | Complex |
| `ambientClassDeclarationWithExtends.ts` | TS2322, TS2449 | Forward reference check shouldn't apply to ambient classes | Medium |
| `ambientExternalModuleWithInternalImportDeclaration.ts` | TS2351 | Module resolution with internal import aliases | Complex |
| `ambientExternalModuleWithoutInternalImportDeclaration.ts` | TS2351 | Module resolution with internal import aliases | Complex |
| `amdDeclarationEmitNoExtraDeclare.ts` | TS2322, TS2345 | Type checking in declaration emit context | Medium |
| `anonClassDeclarationEmitIsAnon.ts` | TS2345 | Argument type checking for anonymous classes | Medium |
| `argumentsObjectIterator02_ES6.ts` | TS2488 | Symbol.iterator resolution or iterability check | Complex |

**Recommended next steps**:
1. **TS2449 false positive**: Investigate forward reference checking in `crates/tsz-checker/src/` - ambient classes shouldn't trigger "used before declaration"
2. **TS2322/TS2345 false positives**: These might be fixable together if they share a root cause

### All Missing (3 tests) - We emit no errors when TSC does

Priority: MEDIUM - Requires implementing new error checks

| Test | Missing Errors | Root Cause | Difficulty |
|------|----------------|------------|------------|
| `amdModuleName2.ts` | TS2458 | Multiple AMD module name pragmas not detected | Hard |
| `argumentsReferenceInConstructor4_Js.ts` | TS1210 | `arguments` variable shadowing in class strict mode | Hard |
| `argumentsReferenceInFunction1_Js.ts` | TS2345, TS7006 | Multiple issues with arguments and type inference | Hard |

**Recommended next steps**:
- These require implementing new validation logic, skip for now

### Wrong Codes (9 tests) - Both TSC and tsz emit errors, but different ones

Priority: LOW-MEDIUM - Requires understanding why we chose different error

| Test | Expected → Actual | Notes |
|------|-------------------|-------|
| `allowSyntheticDefaultImports8.ts` | TS2305 → TS1192 | Import resolution |
| `ambientPropertyDeclarationInJs.ts` | +TS8009, TS8010 | Missing JSDoc-specific errors |
| `ambientExportDefaultErrors.ts` | TS2714 → TS2304 | Export default validation |
| `ambiguousGenericAssertion1.ts` | TS2304 → TS1434 | Type reference vs type assertion |
| `anonymousClassExpression2.ts` | TS2551 → TS2339 | Property access on private identifier |
| `argumentsBindsToFunctionScopeArgumentList.ts` | TS2322 → TS2739 | Error elaboration preference |

**Recommended next steps**:
- **argumentsBindsToFunctionScopeArgumentList.ts**: We emit TS2739 (detailed property mismatch) instead of TS2322 (simple "not assignable"). This might be an error elaboration preference issue in `error_reporter.rs`

### Close to Passing (6 tests) - Differ by only 1-2 error codes

All 6 "close" tests are actually in the "Wrong Codes" category above. No additional quick wins here.

## Analysis by Error Code

### Most Impactful Fixes

**False Positives to Fix** (highest ROI):
1. TS2339 (3 occurrences) - Property access errors
2. TS2322 (2 occurrences) - Type assignment errors
3. TS2351 (2 occurrences) - Constructor errors
4. TS2345 (2 occurrences) - Argument type errors

**Not Implemented** (would help if easy to add):
- TS2305, TS2439, TS2714, TS2551, TS2458, TS1210, TS1437, TS2580, TS2585, TS8009, TS8010, TS7006
- All appear in single tests, so low individual impact

## Recommendations for Next Session

### Quick Wins to Try First

1. **TS2449 Investigation** - `ambientClassDeclarationWithExtends.ts`
   - Search for "2449" or "CLASS_USED_BEFORE_ITS_DECLARATION" in checker
   - Add special handling to skip this check for ambient declarations
   - **Expected impact**: +1 test

2. **Error Elaboration** - `argumentsBindsToFunctionScopeArgumentList.ts`
   - We emit TS2739 (detailed) instead of TS2322 (simple)
   - Check `error_reporter.rs` for when we choose elaborate vs simple errors
   - May be controlled by compiler options or context
   - **Expected impact**: +1 test

3. **TS2322/TS2345 Pattern** - Multiple tests
   - `amdDeclarationEmitNoExtraDeclare.ts` and `anonClassDeclarationEmitIsAnon.ts` both have false positive type errors
   - May share a common root cause (declaration emit context?)
   - **Expected impact**: +2-3 tests if pattern found

### Longer-Term Work

- **Module resolution issues**: TS2351, TS2339 in AMD module tests - requires deeper module resolver work
- **Pragma parsing**: TS2458 for AMD module names - requires parser/scanner work
- **Strict mode checks**: TS1210 for arguments in classes - requires adding new validation

### Testing Strategy

Always verify fixes with:
```bash
# Run affected test
./scripts/conformance.sh run --max=100 --offset=100 --verbose 2>&1 | grep -A5 "test-name"

# Verify full suite
./scripts/conformance.sh run --max=100 --offset=100

# Check for regressions
cargo nextest run -p tsz-checker
```

## Notes

- Build was out of sync causing test failures in pre-commit hook
- Some false positives may be lib.d.ts issues (Symbol.iterator not found)
- The remaining issues are increasingly complex, requiring deeper understanding of module resolution, declaration emit, and error reporting strategies
