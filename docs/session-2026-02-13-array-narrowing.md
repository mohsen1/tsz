# Session 2026-02-13: Array Predicate Type Narrowing (WIP)

## Mission Context

**Goal**: Implement type relation/inference engine parity with TSC
**Priority**: Array predicate narrowing (Task #7) - highest impact fix identified

## Session Summary

### What Was Accomplished

1. **✅ Comprehensive Type System Analysis**
   - Analyzed conformance tests 0-499 (83% pass rate)
   - Identified TS2339 false positives (9 tests) as top priority
   - Created priority-ranked roadmap

2. **✅ Infrastructure Implementation**
   - Added `TypeGuard::ArrayElementPredicate` variant to solver
   - Implemented `check_array_every_predicate()` in checker
   - Implemented `narrow_array_element_type()` in solver
   - All 2394 unit tests still pass ✅

3. **📝 Documentation**
   - `docs/type-system-conformance-status.md` - Current state
   - `docs/priority-fixes-for-type-system.md` - Ranked priorities
   - Task #7 updated with implementation details

### What's Not Yet Working

**Issue**: Array narrowing not functioning in practice

**Test Case**:
```typescript
const foo: (number | string)[] = ['aaa'];
const isString = (x: unknown): x is string => typeof x === 'string';

if (foo.every(isString)) {
    foo[0].slice(0);  // ❌ Still error: Property 'slice' doesn't exist
}
```

**Expected**: `foo` narrowed to `string[]` in true branch
**Actual**: `foo` remains `(number | string)[]`

## Technical Implementation

### Changes Made

#### 1. New TypeGuard Variant

**File**: `crates/tsz-solver/src/narrowing.rs`

```rust
pub enum TypeGuard {
    // ... existing variants ...

    /// `array.every(predicate)` where predicate has type predicate
    ArrayElementPredicate {
        /// The type to narrow array elements to
        element_type: TypeId,
    },
}
```

#### 2. Detection Logic

**File**: `crates/tsz-checker/src/control_flow_narrowing.rs`

Added `check_array_every_predicate()` function:
- Detects `.every()` method calls
- Extracts callback parameter
- Checks if callback has type predicate via `predicate_signature_for_type()`
- Returns `ArrayElementPredicate` guard with predicate type
- Target is the array being called on

#### 3. Narrowing Logic

**File**: `crates/tsz-solver/src/narrowing.rs`

Added `narrow_array_element_type()` function:
- Checks if source type is `Array(elem_type)`
- Narrows element type using existing `narrow_to_type()`
- Reconstructs array with narrowed element
- Handles unions of arrays recursively

### Debugging Needed

**Hypothesis**: One of these is failing:

1. **Type Predicate Detection**
   - `predicate_signature_for_type(callback_type)` might return `None`
   - Callback type might not be properly analyzed
   - Arrow functions vs regular functions might be handled differently

2. **Guard Extraction**
   - `check_array_every_predicate()` might not be reached
   - Property access detection might fail
   - Callback argument extraction might fail

3. **Narrowing Application**
   - Guard might be extracted but not applied
   - Control flow analysis might not propagate narrowing
   - Element access might resolve type before narrowing applied

**Next Steps**:
1. Add tracing to `check_array_every_predicate()` to see if it's called
2. Add tracing to `narrow_array_element_type()` to see if narrowing happens
3. Check if `predicate_signature_for_type()` works for arrow functions
4. Verify control flow graph applies narrowing correctly

## Code Quality

- ✅ No regressions (all 2394 unit tests pass)
- ✅ Architecture principles maintained (Solver-First)
- ✅ Clean code structure with documentation
- ✅ Follows HOW_TO_CODE.md patterns

## References

- **Task**: #7 (in_progress)
- **Test Case**: `TypeScript/tests/cases/compiler/arrayEvery.ts`
- **Priority Doc**: `docs/priority-fixes-for-type-system.md`
- **Commit**: 5ab17531b (WIP implementation)

## Next Session Priorities

### Immediate (Continue Task #7)

1. **Debug type predicate detection**
   - Add `TSZ_LOG=trace` to see if functions are called
   - Check if `predicate_signature_for_type` works for arrows
   - Verify callback type is correctly inferred

2. **Debug narrowing application**
   - Verify guard is extracted (add tracing)
   - Verify guard is applied in control flow
   - Check element type resolution after narrowing

3. **Fix and test**
   - Identify the missing piece
   - Complete implementation
   - Run conformance tests to measure improvement

### If Blocked (Move to Backup Plans)

From priority doc:

1. **JSDoc arrow functions** (4 tests affected)
   - Fix TS7006 false positives
   - Medium complexity, clear path

2. **Unimplemented error codes** (instant wins)
   - TS2693: type vs value (3 tests)
   - TS2741: missing property (2 tests)
   - TS2488: not iterable (2 tests)

## Success Metrics

- Current: 414/499 (83.0%)
- Target after array narrowing: 420+/499 (84%+)
- All unit tests continue passing

## Key Learnings

1. **Impact Analysis Works**: Conformance analysis tool identified highest-value fixes
2. **Infrastructure Exists**: Type predicate and narrowing infrastructure already solid
3. **Incremental Progress**: Even partial implementation commits maintain quality
4. **Testing Essential**: Unit tests catch regressions immediately
5. **Documentation Matters**: Clear task tracking enables effective handoffs

## Conclusion

Solid progress on infrastructure, but narrowing not yet functional. Clear path forward with specific debugging steps identified. All code quality maintained.

**Status**: Ready for debugging session to complete Task #7.
