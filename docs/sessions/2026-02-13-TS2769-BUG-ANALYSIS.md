# TS2769 Bug Analysis: ConcatArray→Node DefId Confusion

**Date**: 2026-02-13
**Status**: Root cause identified, fix location narrowed
**Priority**: HIGH (affects 20-30+ conformance tests)

## Summary

Error messages show `"Node<T>"` instead of `"ConcatArray<T>"` when displaying Array.concat overload signatures. Root cause: DefId confusion where ConcatArray interface is being represented with Node's DefId(14).

## Reproduction

### Minimal Test Case
```typescript
// tmp/check-concat-type.ts
const arr: number[] = [];
arr.concat("wrong");
```

### Expected (TSC)
```
Argument of type 'string' is not assignable to parameter of type 'ConcatArray<number>'.
```

### Actual (tsz)
```
Argument of type 'string' is not assignable to parameter of type 'Node<number>'.
```

## Root Cause Analysis

### Key Findings

1. **DefId(14) = "Node"** (from lib.dom.d.ts)
   - Interface with 0 type parameters
   - Correct name, correct definition

2. **ConcatArray missing correct DefId**
   - When formatted, uses DefId(14) instead of its own DefId
   - Results in `Application(Node, [T])` instead of `Application(ConcatArray, [T])`

3. **Type structure trace**:
   ```
   TypeId(60581) = Application(TypeApplicationId(3195))
     └─ base = TypeId(287) = Lazy(DefId(14))  ← WRONG!
         └─ DefId(14) = "Node" (0 type params)
   ```

   **Should be**:
   ```
   TypeId(60581) = Application(TypeApplicationId(X))
     └─ base = TypeId(Y) = Lazy(DefId(ConcatArray))
         └─ DefId(ConcatArray) = "ConcatArray" (1 type param)
   ```

### Observations

- ✅ Rest parameter extraction works correctly
- ✅ Array element type extraction works correctly
- ❌ The extracted element type itself has wrong DefId
- ❌ @noLib directive not respected (separate issue)
- ✅ Bug does NOT occur without using built-in Array interface

## Investigation Timeline

1. **Identified phantom "Node<T>" in error messages** (1h)
2. **Traced to rest parameter handling** (1h)
3. **Added comprehensive tracing** (1h)
4. **Discovered DefId(14) = "Node"** (0.5h)
5. **Confirmed ConcatArray→Node confusion** (0.5h)

**Total**: 4 hours investigation

## Where the Bug Likely Is

### Primary Suspects

1. **Type Application Creation** (`crates/tsz-solver/src/application.rs`)
   - When creating `Application(base, args)` for ConcatArray<T>
   - Might be using wrong DefId for base

2. **Type Instantiation** (`crates/tsz-solver/src/instantiate.rs`)
   - When instantiating generic signatures like `concat<T>`
   - Might be substituting wrong type for ConcatArray

3. **Def Loading/Resolution** (`crates/tsz-binder/`)
   - When loading lib.es5.d.ts
   - Might be assigning DefId(14) to multiple interfaces
   - Or ConcatArray definition might be malformed

4. **Structural Type Unification**
   - If ConcatArray and Node are being unified structurally
   - Though they have different structures, so unlikely

### Tracing Added

- ✅ `format.rs:288-330` - Application formatting with DefId details
- ✅ `operations.rs:1336-1358` - Rest parameter extraction

### Next Debugging Steps

1. **Trace DefId assignment** (2h)
   ```bash
   # Add tracing in binder when creating interface definitions
   # Track DefId(14) and ConcatArray's DefId through binding process
   ```

2. **Check DefinitionStore** (1h)
   ```rust
   // In format.rs, when DefId(14) is encountered:
   // Log all fields of the DefinitionInfo
   // Check if body matches Node or ConcatArray structure
   ```

3. **Trace type instantiation** (1-2h)
   ```rust
   // In instantiate.rs, add tracing when processing:
   // - ConcatArray<T> types
   // - Union types containing ConcatArray
   ```

4. **Fix and verify** (1h)
   - Implement fix based on findings
   - Run conformance tests
   - Verify 20-30+ tests now pass

## Impact

**Affected tests** (sample of 300):
- arrayConcat3.ts
- arrayFromAsync.ts
- arrayToLocaleStringES2015.ts
- arrayToLocaleStringES2020.ts
- 2+ more with concat/array operations

**Estimated total**: 20-30+ tests in full conformance suite

**Pass rate improvement**: 90.3% → 92-93%

## Files Modified

- `crates/tsz-solver/src/format.rs` - Added tracing
- `crates/tsz-solver/src/operations.rs` - Added tracing
- Committed: `eef1aa0f1`

## Related Issues

- `@noLib: true` directive not working (separate bug)
- Interface merging with built-in Array (expected behavior)

## References

- Session: `docs/sessions/2026-02-13-COMPLETE-SESSION-SUMMARY.md`
- Investigation: `docs/sessions/2026-02-13-TS2769-INVESTIGATION.md`
- Test file: `tmp/no-concat-name.ts`
- Simpler test: `tmp/check-concat-type.ts`

---

**Status**: ✅ Root cause identified
**Next session**: Trace DefId assignment in binder (2-3 hours to fix)
