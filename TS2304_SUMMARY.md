# TS2304 Investigation Summary

## Task #7: Fix TS2304 Name Resolution

**Goal**: Fix 4739 total TS2304 errors (2169 missing + 2570 extra)

**Actual Current State** (Jan 27, 2026):
- Missing TS2304 errors: 34x
- Extra TS2304 errors: 149x
- **Total**: 183 errors to fix

## Root Cause Analysis

### Duplicate Error Emission

Investigated `unknownSymbols1.ts` test case where TSC expects 13 errors but we emit 18.

**Found**: We're emitting 4 duplicate TS2304 errors for class property type annotations:
```typescript
class C<T> {
    foo: asdf;  // TS2304 emitted 4 times instead of 1
    bar: C<asdf>;  // TS2304 emitted 4 times instead of 1
}
```

### Investigation Process

1. **Traced error emission flow** through:
   - `/Users/mohsenazimi/code/tsz/src/checker/type_computation.rs` - Type computation
   - `/Users/mohsenazimi/code/tsz/src/checker/symbol_resolver.rs` - Symbol resolution
   - `/Users/mohsenazimi/code/tsz/src/checker/error_reporter.rs` - Error emission
   - `/Users/mohsenazimi/code/tsz/src/checker/state.rs` - Type checking state

2. **Identified deduplication mechanism**:
   - `CheckerContext.emitted_diagnostics: HashSet<(u32, u32)>` tracks (start, code)
   - `push_diagnostic()` and `error()` methods check for duplicates before adding
   - Located in `/Users/mohsenazimi/code/tsz/src/checker/context.rs` lines 694-697, 711-714

3. **Tested deduplication**:
   - Added debug logging to trace diagnostic emission
   - Found that deduplication code exists and should work
   - BUT diagnostics may be added via multiple code paths

### Potential Duplicate Sources

**Hypothesis 1**: Multiple code paths for diagnostic emission
- Some diagnostics use `push_diagnostic()` (with deduplication)
- Others use `ctx.diagnostics.push()` directly (bypassing deduplication)

**Hypothesis 2**: Different start positions
- Same line/column but different character offsets
- Deduplication key is `(start, code)` where start is byte offset
- May need to normalize positions

**Hypothesis 3**: Type arguments processed multiple times
- `get_type_from_type_node()` calls `get_type_from_type_reference()`
- Type arguments processed in checking phase AND lowering phase
- Both emit TS2304 errors

## Key Files

- `/Users/mohsenazimi/code/tsz/src/checker/symbol_resolver.rs` - Symbol resolution logic (lines 275-694)
- `/Users/mohsenazimi/code/tsz/src/checker/type_computation.rs` - Type computation for identifiers
- `/Users/mohsenazimi/code/tsz/src/checker/error_reporter.rs` - Error emission (line 662: `error_cannot_find_name_at`)
- `/Users/mohsenazimi/code/tsz/src/checker/state.rs` - Type checking state
- `/Users/mohsenazimi/code/tsz/src/checker/context.rs` - Diagnostic deduplication (lines 694-714)

## Next Steps

### Immediate Actions

1. **Verify diagnostic path**: Add comprehensive logging to track:
   - Where TS2304 errors are emitted
   - Whether they go through deduplication
   - What the (start, code) keys are

2. **Test with simpler cases**:
   - Test simple identifier: `const x = asdf;`
   - Test property type: `class C { foo: asdf; }`
   - Test type argument: `const x: C<asdf>;`

3. **Check for multiple emission points**:
   - Search all `diagnostics.push` calls
   - Identify which bypass `push_diagnostic()`
   - Consolidate to use deduplication consistently

### Potential Fixes

**Fix Option 1**: Ensure all diagnostics use `push_diagnostic()`
- Replace direct `ctx.diagnostics.push()` with `push_diagnostic()`
- Guarantees deduplication works
- Risk: May have performance implications

**Fix Option 2**: Track errors at symbol level
- Add `HashSet<SymbolId>` for symbol-based deduplication
- More robust than position-based deduplication
- Requires symbol ID tracking

**Fix Option 3**: Prevent duplicate type checking
- Avoid processing type arguments multiple times
- More efficient overall
- Requires careful refactoring of type resolution

## Conformance Testing

Run tests with:
```bash
./conformance/run-conformance.sh --all --workers=14 --filter "TS2304" --count 100
```

Monitor:
- Missing errors (we should emit but don't)
- Extra errors (we emit but shouldn't)
- Both should decrease with fixes

## Commit Strategy

Commit frequently with descriptive messages:
- `docs: TS2304 investigation notes`
- `fix(checker): Fix TS2304 duplicate error emission`
- `fix(checker): Fix TS2304 missing errors for <case>`

## References

- Gap documentation: `/Users/mohsenazimi/code/tsz/docs/walkthrough/07-gaps-summary.md`
- Conformance baseline: `/Users/mohsenazimi/code/tsz/conformance/baseline.log`
- Test cases: `/Users/mohsenazimi/code/tsz/TypeScript/tests/cases/compiler/unknownSymbols*.ts`
