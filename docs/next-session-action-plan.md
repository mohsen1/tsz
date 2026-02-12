# Next Session Action Plan - Conformance Testing

## Current State Summary (2026-02-12 Update)
- **Overall Pass Rate**: 60.9% (7638/12545 passing)
- **Slice 3/4**: 61.5% (1934/3145 passing) - improved from 60.1%
- **Unit Tests**: ✅ 2396/2396 passing
- **Recent Fixes**:
  - TS2630: Function assignment check (completed)
  - TS2365: '+' operator with null/undefined (completed)

## Immediate Actions (Highest Priority)

### Action 1: Reduce TS2322 False Positives (Impact: 91 tests in slice 3)
**Status**: High-impact opportunity

**Problem**: We emit TS2322 (type not assignable) in 91 tests where TypeScript doesn't.

**Debug Steps**:
1. Analyze pattern in false positive tests:
   ```bash
   ./scripts/conformance.sh analyze --offset 6292 --max 3146 --category false-positive 2>&1 | grep "TS2322" -A5
   ```

2. Pick a specific test to investigate:
   ```bash
   ./scripts/conformance.sh run --offset 6292 --max 3146 --verbose --filter "variance" | less
   ```

3. Common patterns to investigate:
   - Variance-related types
   - Index signature relationships
   - Generic type inference

**Potential Root Causes**:
- Over-strict assignability checking in certain contexts
- Missing special cases in subtype checking
- Incorrect handling of contravariance/covariance

### Action 2: Reduce TS2339 False Positives (Impact: 76 tests)
**Status**: Pattern identified - ES5 Symbol properties

**Problem**: We emit "Property doesn't exist" for Symbol properties in ES5 target.

**Example Tests**:
- ES5SymbolProperty1.ts, ES5SymbolProperty3.ts, ES5SymbolProperty4.ts, ES5SymbolProperty5.ts, ES5SymbolProperty7.ts

**Investigation**:
1. Check how we handle Symbol properties for different targets
2. ES5 should allow Symbol as property key even though Symbol doesn't exist at runtime
3. Look at `crates/tsz-checker/src/type_checking.rs` - property access resolution

**Location**: Likely in property access checking or symbol resolution

### Action 3: Implement TS1362/TS1361 (Impact: 27 tests)
**Status**: Not implemented at all

**TS1362**: "'await' expressions are only allowed within async functions and at the top levels of modules."
**TS1361**: "'await' expressions are only allowed at the top level of a file when that file is a module..."

**Implementation Strategy**:
1. Track async function context in checker state
2. Add check when visiting await expressions
3. Verify file is a module for top-level await
4. Check module/target options for ES2022+ top-level await

**Files to Modify**:
- `crates/tsz-checker/src/context.rs` - Add `in_async_function` flag
- `crates/tsz-checker/src/type_computation.rs` or `dispatch.rs` - Check await expressions
- `crates/tsz-checker/src/statements.rs` - Track function context

**Example Code to Add**:
```rust
// In context.rs
pub in_async_function: bool,

// When checking await expression
if !self.ctx.in_async_function && !self.is_top_level_module() {
    self.error(node, "TS1362: 'await' expressions are only allowed within async functions...");
}
```

## Quick Wins (Easier to Implement)

### Quick Win 1: Improve TS2454 Coverage (Impact: 15+ tests)
**Why**: Already implemented, just needs broader coverage in for-in/for-of loops

**Tests Affected**:
- unusedLocalsInForInOrOf1.ts
- classStaticBlockUseBeforeDef3.ts

**Investigation**: Check why definite assignment analysis isn't running for these contexts.

### Quick Win 2: Fix Missing TS2538 (Impact: 2+ tests)
**TS2538**: "Type 'X' cannot be used as an index type."

**Tests**:
- asyncFunctionDeclarationParameterEvaluation.ts
- asyncGeneratorParameterEvaluation.ts

**Status**: Likely already partially implemented, needs specific case coverage.

### Quick Win 3: Implement TS2636/TS2637 Variance Errors (Impact: 2+ tests)
**TS2636**: "The 'in' modifier can only be used on a type parameter of a type."
**TS2637**: "The 'out' modifier can only be used on a type parameter of a type."

**Simple Implementation**: Add validation when binding/checking type parameters with variance annotations.

## Implementation Recipes

### Recipe: Reducing False Positives
When we emit errors TypeScript doesn't:

1. **Find the test and read it**:
   ```bash
   ./scripts/conformance.sh run --offset <offset> --max <max> --verbose --filter "<test-name>"
   ```

2. **Run both compilers side-by-side**:
   ```bash
   # TypeScript
   cd TypeScript && npx tsc --noEmit tests/cases/compiler/<test>.ts

   # tsz
   ./.target/dist-fast/tsz TypeScript/tests/cases/compiler/<test>.ts
   ```

3. **Add tracing to understand why we emit**:
   ```rust
   #[tracing::instrument(level = "debug")]
   fn check_that_emits_error(&mut self, node: NodeIndex) {
       debug!("Checking node that might emit false positive");
       // ... existing code
   }
   ```

4. **Run with tracing**:
   ```bash
   TSZ_LOG="tsz_checker=debug" TSZ_LOG_FORMAT=tree ./.target/dist-fast/tsz test.ts 2>&1 | less
   ```

5. **Identify the condition** that should suppress the error

6. **Add the check** and verify it doesn't break existing tests

### Recipe: Implementing New Error Codes

1. **Verify the diagnostic exists**:
   ```bash
   grep "<code>" crates/tsz-common/src/diagnostics.rs
   ```

2. **Find where TypeScript emits it**:
   - Search TypeScript compiler source on GitHub
   - Look in `src/compiler/checker.ts` for the error code

3. **Add the check in the appropriate file**:
   - Await expressions → `type_computation.rs` or `dispatch.rs`
   - Type parameters → `type_checking.rs` or `state_type_resolution.rs`
   - Declarations → `binder` or `statements.rs`

4. **Test with a minimal case first**:
   ```typescript
   // test.ts
   await 42; // Should emit TS1362 if not in async function
   ```

5. **Run conformance tests**:
   ```bash
   ./scripts/conformance.sh run --error-code <code>
   ```

## Measuring Success

### Before Making Changes
```bash
./scripts/conformance.sh run --offset 6292 --max 3146 2>&1 | tee baseline-slice3.txt
grep "FINAL RESULTS" baseline-slice3.txt
```

### After Each Fix
```bash
./scripts/conformance.sh run --offset 6292 --max 3146 2>&1 | tee after-fix.txt
diff <(grep "PASS\|FAIL" baseline-slice3.txt) <(grep "PASS\|FAIL" after-fix.txt) | wc -l
```

### Full Suite
```bash
./scripts/conformance.sh run 2>&1 | grep "FINAL RESULTS"
```

## Common Pitfalls to Avoid

1. **Don't implement without testing**: Always verify with minimal test case first
2. **Don't commit broken code**: Run `cargo nextest run` before every commit
3. **MANDATORY sync after EVERY commit**: `git pull --rebase origin main && git push origin main`
4. **Don't work without debugging**: Use tracing to understand code flow
5. **Don't guess at implementations**: Look at TypeScript source and existing patterns

## Files to Know

### Error Checking & Reporting
- `crates/tsz-checker/src/error_reporter.rs` - Error emission helpers
- `crates/tsz-checker/src/type_checking.rs` - Main type checking logic
- `crates/tsz-checker/src/assignment_checker.rs` - Assignment validation
- `crates/tsz-common/src/diagnostics.rs` - All error codes and messages

### Context & State
- `crates/tsz-checker/src/context.rs` - Checker context and state
- `crates/tsz-checker/src/state.rs` - Main checker state

### Type Computation
- `crates/tsz-checker/src/type_computation.rs` - Type inference
- `crates/tsz-checker/src/dispatch.rs` - AST node dispatch

### Binder
- `crates/tsz-binder/src/state.rs` - Symbol table management

## Success Criteria for Next Session

- ✅ Reduce false positives by at least 20 tests
- ✅ Implement at least one new error code (TS1362 recommended)
- ✅ Maintain 100% unit test pass rate (2396/2396)
- ✅ Improve slice 3 pass rate by at least 1-2%
- ✅ Sync with remote after every commit
- ✅ Document findings and create session summary

## If Stuck

1. Use tracing to understand code flow: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree`
2. Compare with TypeScript behavior side-by-side
3. Look at similar existing error code implementations
4. Start with the simplest possible test case
5. Check docs/HOW_TO_CODE.md for patterns and conventions

## Notes from This Session

**What Worked Well**:
- Using `binder.resolve_identifier()` instead of `node_symbols.get()` for reference lookup
- Systematic analysis with conformance.sh analyze command
- Focusing on high-impact opportunities first

**Lessons Learned**:
- False positives are just as important as missing errors
- Many tests are very close to passing (diff=1-2 errors)
- Pattern analysis reveals clusters of similar failures
- Symbol resolution context matters (declarations vs references)

**Next Time**:
- Start with false positive reduction - high impact, lower risk
- Consider implementing TS1362/TS1361 if time permits
- Look for patterns in ES5Symbol property tests
