# Type System Conformance Status

**Date**: 2026-02-13
**Session**: Type Relation / Inference Engine Parity

## Current Pass Rates

| Test Range | Pass Rate | Notes |
|------------|-----------|-------|
| Tests 0-49 | 49/49 (100%) | Perfect! No failures |
| Tests 0-199 | 192/199 (96.5%) | Only 7 failures |
| Tests 0-499 | 414/499 (83.0%) | Significant drop-off |

### Recent Achievements

✅ **Literal Type Preservation** (Previous session)
- Fixed discriminated union contextual typing
- Object literals now preserve literal types (`false` vs `boolean`)
- All 2394 unit tests pass

## Top Error Code Mismatches (Tests 0-499)

| Error Code | Missing | Extra | Priority |
|------------|---------|-------|----------|
| TS2322 | 12 | 6 | Medium |
| TS2339 | 2 | 9 | Medium |
| TS2345 | 1 | 8 | Medium |
| TS2304 | 5 | 2 | Low |
| TS2769 | 0 | 6 | Low |

## Remaining Failures in Tests 0-199

1. **accessorInferredReturnTypeErrorInReturnStatement.ts** - TS7023 missing
2. **aliasOnMergedModuleInterface.ts** - TS2339 extra
3. **aliasUsageInIndexerOfClass.ts** - TS2411 extra
4. **allowJscheckJsTypeParameterNoCrash.ts** - Wrong error (TS2345 vs TS2322)
5. **ambiguousGenericAssertion1.ts** - Parser recovery (TS1434 vs TS2304)
6. **amdLikeInputDeclarationEmit.ts** - TS2339 false positive
7. **argumentsReferenceInFunction1_Js.ts** - TS7011 vs TS2345

## Investigation Results

### Generic Function Inference (TS2769)

**Status**: Complex, low priority (only 6 tests affected)

**Issue**: Higher-order generic functions fail to infer correctly
```typescript
declare function compose<A, B, C>(ab: (a: A) => B, bc: (b: B) => C): (a: A) => C;
declare function list<T>(a: T): T[];
const f = compose(list, box);  // ❌ Fails
```

**Root Cause**: When a generic function is passed as an argument to another generic function, TSZ doesn't properly instantiate the argument function with fresh type variables before matching against the parameter type.

**Complexity**: High - requires changes to constraint collection in `crates/tsz-solver/src/operations.rs`

**Recommendation**: Defer - too complex for immediate impact

### Array Narrowing with Type Predicates

**Status**: Not yet investigated in detail

**Issue**: `.every()`, `.filter()`, `.some()` with type predicates don't narrow array types
```typescript
const foo: (number | string)[] = ['aaa'];
if (foo.every(isString)) {
    foo[0].slice(0);  // ❌ Property 'slice' doesn't exist
}
```

**Expected**: Array should be narrowed to `string[]` after the type predicate

**Complexity**: Medium - control flow narrowing enhancement

**Impact**: Affects multiple tests

## Architectural Observations

### Strengths

1. **Clean Separation**: Solver/Checker/Binder layers are well-separated
2. **Inference Infrastructure**: Multi-pass constraint collection works well
3. **Contextual Typing**: Recently fixed literal preservation shows the system is robust

### Gaps

1. **Higher-Order Generics**: Generic functions as arguments need better instantiation
2. **Control Flow Narrowing**: Array predicates and more complex narrowing patterns
3. **Module Resolution**: Some tests fail due to module loading issues (not type system)

## Recommended Priorities

### High Impact, Medium Complexity

1. **TS7006 False Positives** (Mentioned in mission as blocking)
   - "Contextual typing for function expressions"
   - Need to investigate which tests are affected

2. **Array Type Narrowing**
   - Straightforward control flow enhancement
   - Affects multiple tests
   - Clear implementation path

### Medium Impact, Lower Complexity

3. **Parser Error Recovery** (ambiguousGenericAssertion1.ts)
   - Wrong error code after parse error
   - May be simple diagnostic issue

4. **Module Export/Import Edge Cases**
   - Several tests fail on module resolution
   - May not be type system issues

### Lower Priority

5. **Higher-Order Generic Inference**
   - Complex, only 6 tests
   - Defer until simpler issues resolved

## Next Steps

### Immediate Actions

1. ✅ Document current state (this file)
2. ⏭️ Investigate TS7006 pattern to assess impact
3. ⏭️ Implement array type predicate narrowing
4. ⏭️ Run full conformance suite to measure improvement

### Long-term Goals

- Push test 0-199 pass rate from 96.5% → 99%+
- Push test 0-499 pass rate from 83% → 90%+
- Focus on general fixes over one-off patches

## Code Quality Notes

- All 2394 unit tests continue to pass ✅
- No regressions introduced
- Architecture principles maintained (Solver-First, no TypeKey matching in Checker)

## References

- **Mission**: Type Relation / Inference Engine Parity
- **Key Insight**: Prioritize by impact, not by interesting complexity
- **Success Metric**: Pass rate improvement across conformance suite
