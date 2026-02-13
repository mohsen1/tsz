# Conformance Tests 100-199 - Assignment

**Date**: 2026-02-12
**Assignment**: Maximize pass rate for conformance tests 100-199 (offset 100, max 100)

## Mission

Pass the second 100 conformance tests with:
```bash
./scripts/conformance.sh run --max=100 --offset=100
```

## Strategy

### Priority Order
1. **"Close" tests** - Tests that differ by only 1-2 errors (easiest wins)
2. **False positives** - We emit errors TypeScript doesn't
3. **All-missing errors** - We miss entire error codes
4. **Wrong-code errors** - We emit wrong error codes

### Analysis Commands

```bash
# See all failures in my slice
./scripts/conformance.sh analyze --max=100 --offset=100

# Focus on easy wins
./scripts/conformance.sh analyze --max=100 --offset=100 --category close

# See false positives
./scripts/conformance.sh analyze --max=100 --offset=100 --category false-positive

# See missing error codes
./scripts/conformance.sh analyze --max=100 --offset=100 --category all-missing
```

### Debugging Workflow

1. Find a failing test
2. Run verbose to see exact differences:
   ```bash
   ./scripts/conformance.sh run --max=100 --offset=100 --verbose --filter="testname"
   ```
3. Create minimal reproduction in `tmp/test.ts`
4. Compare with TSC:
   ```bash
   ./target/dist-fast/tsz tmp/test.ts 2>&1
   cd TypeScript && npx tsc --noEmit tmp/test.ts
   ```
5. Fix the code
6. Verify: `cargo nextest run`
7. Re-run conformance tests
8. Commit and sync

## Key Principles

- **General fixes over one-offs** - Fix root causes that help many tests
- **No regressions** - Always run `cargo nextest run` before committing
- **Sync after every commit** - `git pull --rebase origin main && git push`
- **Focus on impact** - Use analyze command to find high-value fixes

## Current Status

**Build**: In progress (cargo clean + rebuild)
**Pass Rate**: Unknown (need to run tests)
**Tests Analyzed**: 0
**Fixes Implemented**: 0

## Next Steps

1. Wait for build to complete
2. Run: `./scripts/conformance.sh run --max=100 --offset=100`
3. Analyze failures: `./scripts/conformance.sh analyze --max=100 --offset=100 --category close`
4. Pick highest-impact fix
5. Implement and test
6. Iterate

## Files to Know

### Checker
- `crates/tsz-checker/src/checker/mod.rs` - Entry point
- `crates/tsz-checker/src/checker/state.rs` - State management
- `crates/tsz-checker/src/checker/type_checking.rs` - Type checking logic
- `crates/tsz-checker/src/checker/declaration_checker.rs` - Declarations

### Solver
- `crates/tsz-solver/src/subtype.rs` - Subtype checking
- `crates/tsz-solver/src/subtype_rules/` - Specific subtype rules

### Diagnostics
- `crates/tsz-common/src/diagnostics.rs` - All error codes and messages

### Other
- `crates/tsz-parser/src/` - Parser
- `crates/tsz-binder/src/` - Symbol binding
- `crates/tsz-cli/src/driver.rs` - CLI driver

## Notes

- This is the **second 100 tests** (100-199), not the first slice
- Different from slice 1 (offset 0, max 3146) which was 68.5% passing
- Different from emit tests which I was working on earlier
