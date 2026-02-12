# Conformance Test Session - Final Report (2026-02-12)

## Summary

**Slice:** 1 of 4 (tests 0-3,145 out of 12,583 total)  
**Initial Pass Rate:** 68.3% (2,145/3,139)  
**Final Pass Rate:** 68.4% (2,147/3,139)  
**Improvement:** +2 tests (+0.1%)

**Major Achievement:** Fixed critical type inference bug where `Symbol('test')` returned `DecoratorMetadata` instead of `symbol`

## Work Completed

### 1. Critical Bug Fix: Symbol/DecoratorMetadata (Committed)

**File:** `crates/tsz-solver/src/lower.rs`  
**Function:** `lower_identifier_type`

**Problem:** Primitive type keywords like "symbol" were resolved as symbols BEFORE checking built-in types. When `esnext.decorators` was loaded, this caused type annotations like `: symbol` to resolve to `DecoratorMetadata` instead of the primitive `symbol` type.

**Solution:** Reordered checks to verify built-in primitive types (symbol, string, number, etc.) FIRST before attempting symbol resolution.

**Impact:**
- ✅ All 3,547 tsz-solver unit tests pass
- ✅ All 2,396 pre-commit tests pass
- ✅ Symbol('test') correctly returns symbol with all lib combinations
- ✅ Prevents primitive types from being shadowed

**Why Small Test Improvement:** The Symbol bug masked other issues. Many failures are due to:
- WeakKey type not including symbol in esnext  
- Interface augmentation not working for built-in types
- Missing error code implementations

### 2. Deep Investigation & Documentation

Created comprehensive analysis documents:
- `docs/bugs/symbol-decorator-metadata-bug.md` - Initial bug report
- `docs/bugs/symbol-bug-analysis.md` - Detailed root cause analysis  
- `docs/sessions/2026-02-12-slice1-investigation.md` - Investigation methodology
- `docs/sessions/2026-02-12-slice1-fix-summary.md` - Fix summary

## Current Error Distribution (After Fix)

**Top False Positives (we emit, TSC doesn't):**
- TS2345: 120 extra - Argument type not assignable
- TS2322: 106 extra - Type not assignable
- TS2339: 95 extra - Property does not exist

**Top Missing Errors (TSC emits, we don't):**
- TS2304: 44 missing - Cannot find name
- TS2792: 15 missing - Cannot find module (different message than TS2307)
- TS2671: 4 missing - Module augmentation not found

## Issues Identified

### High Priority

**1. WeakKey Type Incomplete**
- `WeakKey` defined as `WeakKeyTypes[keyof WeakKeyTypes]`
- `WeakKeyTypes` only has `{object: object}` in some libs
- Should also have `{symbol: symbol}` for esnext
- Causes TS2345 errors like "Argument of type 'symbol' is not assignable to parameter of type 'WeakKey'"
- Affects tests: acceptSymbolAsWeakType, and WeakSet/WeakMap/WeakRef tests

**2. Interface Augmentation for Built-in Types**
- User-defined interface augmentations don't apply to built-in types
- Example: `interface Array<T> { split: (parts: number) => T[][] }` should add split to all arrays
- Causes TS2339 "Property does not exist" errors
- Affects: arrayAugment.ts and similar tests

### Medium Priority

**3. TS2792 vs TS2307**
- We emit TS2307 (Cannot find module) when should emit TS2792
- TS2792 is more specific: suggests fixing moduleResolution config
- Only affects error message wording, not detection
- 15 tests affected

**4. Missing Error Codes**
- TS2671: Module augmentation for module not found (4 tests)
- TS2740: Type is missing properties (3 tests missing)
- TS2551: (specific property error variant)

## Pass Rate by Test Range

- First 100 tests: **87.9%** (87/99)
- First 500 tests: **73.5%** (367/499)
- Full slice (3,146): **68.4%** (2,147/3,139)

The pass rate decreases in later tests, suggesting early tests are simpler or certain patterns cause cascading failures in later tests.

## Lessons Learned

1. **Root cause ≠ Only cause** - The Symbol bug masked WeakKey and augmentation issues
2. **Primitive types must be non-shadowable** - Critical for type system stability
3. **Cross-arena lib file merging is complex** - Interface merging can introduce subtle bugs
4. **Test thoroughly at multiple levels** - Unit tests passed but integration revealed issues
5. **Document as you investigate** - Comprehensive docs help future debugging

## Next Steps (Priority Order)

### Immediate (High Impact)
1. Fix WeakKey to include symbol type
2. Fix interface augmentation for built-in types (Array, etc.)
3. Audit lib file type definitions

### Short Term
4. Implement missing error codes (TS2792 message, TS2671, TS2740)
5. Investigate TS2304 missing errors (cannot find name)
6. Review "close to passing" tests (diff ≤ 2 error codes)

### Medium Term  
7. Analyze why pass rate drops in later test ranges
8. Profile test execution to identify slow tests
9. Create targeted test suites for specific features

## Code Quality

- ✅ All changes properly tested
- ✅ Clear commit messages with detailed descriptions
- ✅ Comprehensive documentation added
- ✅ Pre-commit hooks verified
- ✅ No breaking changes to existing tests
- ✅ All changes synced to main branch

## Time Investment

- **Investigation:** ~60% - Deep root cause analysis, tracing, documentation
- **Implementation:** ~20% - Actual fix was simple once root cause found
- **Verification:** ~20% - Testing, running conformance suite, verifying fix

The significant investigation time was well spent - found root cause, identified multiple other issues, and created comprehensive documentation for future work.

## Conclusion

While the test count improvement was modest (+2 tests), this session accomplished:

1. **Fixed a critical type system bug** that would have caused widespread issues
2. **Identified and documented** multiple high-priority issues for future work
3. **Established investigation methodology** with detailed documentation
4. **Verified type system stability** through comprehensive unit testing

The Symbol/DecoratorMetadata fix was a foundational improvement that prevents primitive type shadowing, which is critical for type system correctness. The other issues identified provide a clear roadmap for future conformance improvements.
