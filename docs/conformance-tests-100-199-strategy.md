# Conformance Tests 100-199 Strategy

**Date**: 2026-02-12
**Mission**: Maximize pass rate for conformance tests 100-199 (offset 100, max 100)

## Test Range Overview

Tests 100-199 appear to cover alphabetically sorted TypeScript compiler test cases, starting around "ambient" related declarations based on file listing:
- Ambient module declarations
- Ambient context validation
- AMD module format
- Various declaration file scenarios

## Strategic Priorities

### 1. "Close" Tests (Highest Priority)
Tests that differ by only 1-2 error codes - easiest wins with highest impact.

**Action**: Once conformance analysis runs, focus here first:
```bash
./scripts/conformance.sh analyze --max=100 --offset=100 --category close
```

### 2. False Positive Elimination (High Priority)
Errors we emit that TSC doesn't - indicates overly strict checking.

**Common patterns**:
- Strict null checking in ambiguous contexts
- Property access restrictions
- Type compatibility edge cases

### 3. Missing Error Codes (Medium Priority)
Error codes we never emit that TSC does.

**Known gaps to investigate**:
- TS1039: Initializers not allowed in ambient contexts (check if comprehensive)
- TS1036: Statements not allowed in ambient contexts (check coverage)
- TS1035: Only ambient modules can use quoted names

### 4. Wrong Error Codes (Lower Priority)
Cases where we detect the issue but emit wrong diagnostic code.

## Implementation Checklist

### Before Any Code Changes
- [ ] Run: `./scripts/conformance.sh analyze --max=100 --offset=100`
- [ ] Document baseline pass rate
- [ ] Identify top 3 error codes by test impact
- [ ] Ensure unit tests pass: `cargo nextest run --release`

### For Each Fix
- [ ] Create minimal `.ts` reproduction in `tmp/`
- [ ] Run: `./target/dist-fast/tsz tmp/test.ts 2>&1`
- [ ] Compare with TSC expected output
- [ ] Implement fix following `docs/HOW_TO_CODE.md`
- [ ] Add unit test in appropriate `tests/` directory
- [ ] Verify: `cargo nextest run --release`
- [ ] Verify: `./scripts/conformance.sh run --max=100 --offset=100`
- [ ] Commit with clear message
- [ ] **MANDATORY**: `git pull --rebase origin main && git push origin main`

### Code Review Points
- **Architecture**: Follow `docs/HOW_TO_CODE.md` - Checker never matches TypeKey
- **Performance**: Check hot paths aren't slowed
- **Testing**: No regressions in existing tests
- **Tracing**: Use `tracing::trace!()`, never `eprintln!()`

## Known Areas of Improvement

### Ambient Context Validation

**Files**:
- `crates/tsz-checker/src/declarations.rs` - Main ambient validation
- `crates/tsz-checker/src/state_checking.rs` - State-based checks
- `crates/tsz-checker/src/state_checking_members.rs` - Member checks

**Existing checks**:
- TS1036: Statements in ambient contexts ✓
- TS1039: Initializers in ambient contexts ✓
- TS1040: Modifiers in ambient contexts ✓

**Potential gaps to investigate**:
- Completeness of statement type coverage
- Edge cases with nested declarations
- Module vs namespace ambient rules

### Error Code Accuracy

Check if we're emitting specific enough error codes vs generic ones:
- Generic TS1005 instead of specific syntax errors
- TS2339 instead of more specific property errors
- TS2322 instead of more specific assignability errors

## Test Execution Commands

### Full Suite
```bash
# Build
cargo build --profile dist-fast -p tsz-cli

# Run tests
./scripts/conformance.sh run --max=100 --offset=100

# Analyze failures
./scripts/conformance.sh analyze --max=100 --offset=100
```

### Targeted Testing
```bash
# Specific category
./scripts/conformance.sh analyze --max=100 --offset=100 --category close

# Specific error code
./scripts/conformance.sh run --max=100 --offset=100 --error-code 1039

# Verbose output
./scripts/conformance.sh run --max=100 --offset=100 --verbose
```

### Individual Test Debugging
```bash
# Find test case
ls TypeScript/tests/cases/compiler/*.ts | head -200 | tail -100 | grep "ambient"

# Run tsz on specific file
./target/dist-fast/tsz TypeScript/tests/cases/compiler/ambientModules.ts 2>&1

# Compare with TSC
tsc --noEmit TypeScript/tests/cases/compiler/ambientModules.ts 2>&1
```

## Success Metrics

- **Baseline**: [TBD - need conformance run]
- **Target**: +10% improvement (realistic for 1 session)
- **Stretch**: +20% improvement (if quick wins cluster)

## Time Estimates

Based on conformance documentation:
- Simple fixes (wrong error code): 1-2 hours
- Medium complexity (missing validation): 2-4 hours
- High complexity (architectural): 1-2 days (avoid in this session)

**Focus on low-hanging fruit** - fixes that help multiple tests.

## Next Actions

1. Wait for build to complete
2. Run conformance analysis for tests 100-199
3. Document baseline metrics
4. Pick highest-impact "close" test
5. Implement and verify fix
6. Iterate

## References

- Mission brief: session.sh
- Coding guidelines: `docs/HOW_TO_CODE.md`
- General conformance guide: `docs/conformance-README.md`
