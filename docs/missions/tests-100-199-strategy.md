# Tests 100-199 Mission Strategy

**Mission**: Maximize pass rate for conformance tests 100-199
**Command**: `./scripts/conformance.sh run --max=100 --offset=100`
**Status**: READY FOR EXECUTION (pending build environment fix)

## Strategic Approach

### Phase 1: Baseline Assessment (15 min)

```bash
# Build binary
cargo build --profile dist-fast -p tsz-cli

# Run tests to get current pass rate
./scripts/conformance.sh run --max=100 --offset=100

# Analyze failure patterns
./scripts/conformance.sh analyze --max=100 --offset=100

# Focus on each category
./scripts/conformance.sh analyze --max=100 --offset=100 --category close
./scripts/conformance.sh analyze --max=100 --offset=100 --category false-positive
./scripts/conformance.sh analyze --max=100 --offset=100 --category all-missing
```

**Expected Output**:
- Current pass rate (X/100)
- Top failing error codes
- Number of "close" tests (1-2 errors away from passing)

### Phase 2: Target "Close" Tests (Highest ROI)

**Strategy**: Fix tests that are 1-2 errors away from passing

**Expected findings** (based on overall codebase patterns):
- Tests with single missing error codes (implement the check)
- Tests with single extra error codes (fix false positive)
- Tests with wrong error codes (fix error code mapping)

**Process**:
1. List all "close" tests
2. Group by error code pattern
3. Find common root cause
4. Implement fix that helps multiple tests
5. Verify with `cargo nextest run`
6. Re-run conformance to measure improvement

### Phase 3: False Positives (High Impact)

**Common patterns** (from Slice 3 analysis):
- TS2322 (type not assignable) - overly strict checks
- TS2339 (property doesn't exist) - over-reporting
- TS2345 (argument not assignable) - strict argument checks

**Root causes to investigate**:
1. Flow analysis narrowing incorrectly
2. Type widening issues
3. Union type handling
4. Intersection type checking

**Files to check**:
- `crates/tsz-solver/src/subtype.rs` - Assignability logic
- `crates/tsz-checker/src/type_checking_queries.rs` - Null checking
- `crates/tsz-checker/src/state_type_analysis.rs` - Property access

### Phase 4: Missing Error Codes

**Strategy**: Implement checks for error codes we never emit

**Common missing codes** (from overall analysis):
- Parser validation codes (TS1xxx series)
- Specific type checking codes
- Module resolution codes

**Process**:
1. Identify error codes with 0 emissions
2. Find what triggers those errors in TSC
3. Add validation/check in appropriate location
4. Write unit test for the check
5. Verify conformance improvement

### Phase 5: Wrong Error Codes

**Pattern**: We emit error X when TSC emits error Y

**Common causes**:
- Using generic error instead of specific
- Diagnostic code mapping incorrect
- Parser error recovery choosing wrong code

**Fix approach**:
- Map specific patterns to specific codes
- Check `crates/tsz-common/src/diagnostics.rs` for code definitions
- Update error emission sites

## Implementation Workflow

### For Each Fix

```bash
# 1. Identify failing test
./scripts/conformance.sh run --max=100 --offset=100 --verbose | grep FAIL

# 2. Examine specific test
cat TypeScript/tests/cases/compiler/<test-name>.ts

# 3. Create minimal reproduction
cat > tmp/test.ts << 'EOF'
// Minimal test case
EOF

# 4. Run with tsz to see our output
./.target/dist-fast/tsz tmp/test.ts 2>&1

# 5. Compare with TSC expected (in cache)
# Look at tsc-cache-full.json or expected errors

# 6. Fix the issue
# Edit relevant checker/solver/emitter files

# 7. Verify no regressions
cargo nextest run

# 8. Re-run conformance
./scripts/conformance.sh run --max=100 --offset=100

# 9. Commit if improved
git add -u
git commit -m "fix: <description>"
git pull --rebase origin main
git push origin main
```

## Expected Challenges

### Challenge 1: Type System Edge Cases
**Tests 100-199 likely include**:
- Generic type constraints
- Conditional types
- Mapped types
- Template literal types

**Strategy**:
- Focus on general improvements to type solver
- Avoid one-off special cases
- Fix root causes in subtype checking

### Challenge 2: Parser/AST Edge Cases
**Potential issues**:
- Syntax error recovery
- ASI (Automatic Semicolon Insertion)
- Error code selection

**Strategy**:
- Compare parser output with TSC AST
- Fix error code mappings
- Improve error recovery

### Challenge 3: Control Flow Analysis
**Known issue** (from Slice 3 investigation):
- Invalid assignments incorrectly narrow types
- Causes cascading false positives

**Strategy**:
- May need to skip if too complex for this mission
- Or implement targeted fix if it affects many tests

## Success Metrics

### Minimum Goal
- **+10 tests passing** (10% improvement)
- No regressions in unit tests
- Clean commits with clear messages

### Target Goal
- **+25 tests passing** (25% improvement)
- Fix at least 2 high-impact error patterns
- Document any remaining blockers

### Stretch Goal
- **+40 tests passing** (40% improvement)
- Fix all "close" tests
- Reduce false positives by 50%

## Code Locations Reference

### Type Checking
- `crates/tsz-checker/src/type_checking.rs` - Main type checking logic
- `crates/tsz-checker/src/type_checking_queries.rs` - Helper queries
- `crates/tsz-checker/src/state_type_analysis.rs` - Type analysis state

### Type Solver
- `crates/tsz-solver/src/subtype.rs` - Subtype checking (assignability)
- `crates/tsz-solver/src/type_ops.rs` - Type operations
- `crates/tsz-solver/src/intern.rs` - Type interning

### Declarations
- `crates/tsz-checker/src/declaration_checker.rs` - Declaration checking
- `crates/tsz-checker/src/interface_type.rs` - Interface checking
- `crates/tsz-checker/src/class_type.rs` - Class checking

### Diagnostics
- `crates/tsz-common/src/diagnostics.rs` - Error code definitions
- All checkers use `ctx.push_diagnostic()` to emit errors

## Notes

- **This document prepared**: 2026-02-12, during environment constraints
- **When executable**: Build environment must be functional
- **Pre-requisite**: `cargo build --profile dist-fast -p tsz-cli` must succeed
- **Estimated time**: 2-4 hours for 10-25 test improvements

## Environment Status (at time of writing)

```
Load Average: 66-128 (CRITICAL)
Build Status: All attempts SIGKILL
Disk Space: 113GB free (adequate)
Memory: 29GB/30GB used (97%)
```

**Resolution needed**: Free system resources or use different environment

---

**Next Action**: Once build environment is functional, execute Phase 1 to get baseline metrics.
