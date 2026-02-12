# Next Session Action Plan - Conformance Testing

## Current State Summary
- **Slice 4/4 Pass Rate**: 53.6% (1678/3134 passing)
- **Unit Tests**: ✅ 2396/2396 passing
- **Implementations Added**: 2 (both need fixes)
  - TS2428: Disabled due to binder scope bug
  - TS2630: Implemented but not emitting errors

## Immediate Actions (Highest Priority)

### Action 1: Fix TS2630 Implementation (Impact: 12 tests)
**Status**: Code exists but doesn't emit errors in manual testing

**Debug Steps**:
1. Test with a real conformance test case:
   ```bash
   # Find a test that should emit TS2630
   grep -r "2630" TypeScript/tests/baselines/reference/*.errors.txt | head -1
   # Run that specific test
   ./scripts/conformance.sh run --filter "<test-name>"
   ```

2. Add tracing to `check_function_assignment()`:
   ```rust
   eprintln!("DEBUG: checking function assignment for {:?}", name);
   eprintln!("DEBUG: symbol flags: {:?}", symbol.flags);
   eprintln!("DEBUG: FUNCTION flag: {}", symbol_flags::FUNCTION);
   ```

3. Check if the function is being called at all:
   - Add `eprintln!("DEBUG: check_function_assignment called");` at start
   - Run test and verify it prints

4. If not called, check `check_assignment_expression` is invoked:
   - The call site is in `type_computation.rs:886`
   - Verify binary expressions with `EqualsToken` trigger it

**Location**: `crates/tsz-checker/src/assignment_checker.rs:176`

### Action 2: Create Minimal Test Case for TS2630
**Why**: Verify implementation works before conformance testing

```typescript
// test-ts2630.ts
function foo() { return 42; }
foo = null;  // Should emit TS2630
```

**Run**:
```bash
cargo build --profile dist-fast
.target/dist-fast/tsz test-ts2630.ts
```

**Expected**: `error TS2630: Cannot assign to 'foo' because it is a function.`
**Actual**: No error (bug to fix)

### Action 3: Fix Binder Scope Bug (Blocks TS2428)
**Impact**: Enables interface type parameter validation

**Problem**: Binder merges symbols from different scopes incorrectly.

**Investigation Steps**:
1. Read `crates/tsz-binder/src/state.rs` - find `declare_symbol()`
2. Understand how symbols are stored:
   - `file_locals: FxHashMap<String, SymbolId>`
   - `scopes: Vec<Scope>`
3. Check if namespace symbols go into a separate scope
4. Test case to debug:
   ```typescript
   namespace M {
       interface A<T> { x: T; }
   }
   namespace M2 {
       interface A<T> { x: T; }  // Different scope!
   }
   ```

**Expected**: These should be separate symbols
**Actual**: They merge into one symbol

**Fix Direction**: Likely need to check scope when merging symbols

## Quick Wins (If Above is Too Hard)

### Quick Win 1: Test TS2630 with Conformance Runner
**Why**: Maybe it works for conformance tests but not manual tests

```bash
# Find tests expecting TS2630
grep -l "2630" TypeScript/tests/baselines/reference/*.errors.txt | \
  sed 's/.*\///' | sed 's/\.errors\.txt//'

# Run one of those tests
./scripts/conformance.sh run --filter "<test-name>" --verbose
```

If TS2630 appears in actual output, the implementation works!

### Quick Win 2: Document Exact Test Count Impact
```bash
# Before any changes
./scripts/conformance.sh run --offset 9411 --max 3134 2>&1 | \
  grep "FINAL RESULTS"

# Save this as baseline
# Then after each fix, re-run and compare
```

### Quick Win 3: Focus on Single-Code Quick Wins
From analysis, these have only ONE missing error:
- TS2322: 36 tests (type not assignable)
- TS2339: 21 tests (property doesn't exist)
- TS2304: 16 tests (cannot find name)

These are already implemented, just need broader coverage.

**Strategy**: Pick one failing test, debug why error isn't emitted, fix that case.

## Implementation Recipes

### Recipe: Implementing a New Error Code

1. **Find the diagnostic**:
   ```bash
   grep "CODEXXXX\|code: XXXX" crates/tsz-common/src/diagnostics.rs
   ```

2. **Find where it should be emitted**:
   - Search TypeScript compiler source: `github.com/microsoft/TypeScript`
   - Look for the error code in `src/compiler/diagnosticMessages.json`
   - Find emitting code in TypeScript `src/compiler/checker.ts`

3. **Find similar code in tsz**:
   ```bash
   # If TS2630, look for TS2588 (both "cannot assign" errors)
   grep -r "2588\|CANNOT_ASSIGN" crates/tsz-checker/src/
   ```

4. **Implement in appropriate file**:
   - Assignment errors → `assignment_checker.rs`
   - Type errors → `type_checking.rs`
   - Name resolution → `symbol_resolver.rs`
   - Module errors → `import_checker.rs`

5. **Test**:
   ```bash
   cargo nextest run  # Unit tests
   ./scripts/conformance.sh run --error-code XXXX  # Conformance
   ```

### Recipe: Debugging Missing Errors

1. **Find a failing test**:
   ```bash
   ./scripts/conformance.sh run --offset 9411 --max 3134 --verbose 2>&1 | \
     grep -A20 "FAIL.*enumTag.ts"
   ```

2. **Read the test file**:
   ```bash
   cat TypeScript/tests/cases/conformance/jsdoc/enumTag.ts
   ```

3. **See expected errors**:
   ```bash
   cat TypeScript/tests/baselines/reference/enumTag.errors.txt
   ```

4. **Run tsz with tracing**:
   ```bash
   TSZ_LOG=debug TSZ_LOG_FORMAT=tree .target/dist-fast/tsz \
     TypeScript/tests/cases/conformance/jsdoc/enumTag.ts 2>&1 | \
     grep -i "enum\|error" | head -50
   ```

5. **Compare**: What path does TSC take vs tsz?

## Measuring Success

### Before Making Changes
```bash
./scripts/conformance.sh run --offset 9411 --max 3134 2>&1 | tee baseline.txt
grep "FINAL RESULTS" baseline.txt
# Note the pass rate
```

### After Each Fix
```bash
./scripts/conformance.sh run --offset 9411 --max 3134 2>&1 | tee after-fix.txt
grep "FINAL RESULTS" after-fix.txt
# Compare: did pass rate improve?
```

### Diff to See Which Tests Now Pass
```bash
diff <(grep "PASS\|FAIL" baseline.txt | sort) \
     <(grep "PASS\|FAIL" after-fix.txt | sort)
```

## Common Pitfalls to Avoid

1. **Don't implement without testing**: 
   - TS2630 was implemented but never verified to work
   - Always test with a minimal case first

2. **Don't commit broken code**:
   - Run `cargo nextest run` before every commit
   - Verify no regressions

3. **Don't work without debugging**:
   - Use `TSZ_LOG=debug` to understand code flow
   - Add `eprintln!` liberally during development
   - Remove debug code before committing

4. **Don't guess at implementations**:
   - Look at TypeScript source code
   - Find similar existing code in tsz
   - Follow established patterns

## Files to Know

### Error Emission
- `crates/tsz-checker/src/error_reporter.rs` - Helper methods
- `crates/tsz-checker/src/error_handler.rs` - Error handling trait
- `crates/tsz-common/src/diagnostics.rs` - All error codes and messages

### Checking Logic
- `crates/tsz-checker/src/type_checking.rs` - Main type checking
- `crates/tsz-checker/src/assignment_checker.rs` - Assignment validation
- `crates/tsz-checker/src/import_checker.rs` - Import/module errors
- `crates/tsz-checker/src/symbol_resolver.rs` - Name resolution

### Binder
- `crates/tsz-binder/src/state.rs` - Symbol table management
- `crates/tsz-binder/src/lib.rs` - Binder entry point

## Success Criteria

- ✅ At least one error code implementation that works (verified by manual test)
- ✅ Pass rate improvement (even 1-2 tests is progress)
- ✅ All 2396 unit tests still passing
- ✅ Code committed with clear explanation
- ✅ Documented what works, what doesn't, and why

## If Stuck

1. Focus on debugging TS2630 - it's already mostly done
2. Use the tsz-gemini skill if available
3. Read similar error code implementations in the codebase
4. Start with the simplest possible test case
5. Don't be afraid to ask for clarification on architecture

## Time Estimates

- Debugging TS2630: 1-2 hours
- Fixing binder scope bug: 3-4 hours (complex)
- Implementing new error code: 2-3 hours
- Running full conformance suite: 5-10 minutes
