# Conformance Test Improvement - Action Plan

## Current Status
- **Pass Rate**: 2,125/3,139 (67.7%) in slice 1/4
- **Main Issue**: Too many false positives (extra errors)
- **Strategy Needed**: Focus on easy wins, not deep architectural issues

## Lessons Learned from Investigation

### What NOT to Do
1. ❌ **Don't chase "NOT IMPLEMENTED" without validation**
   - TS2323 appeared "not implemented" but is actually a "wrong error code" issue
   - Always check if we emit a different error for the same issue

2. ❌ **Avoid deep architectural bugs initially**
   - Conditional type alias bug requires solver changes
   - These are research projects, not quick fixes

3. ❌ **Don't over-investigate**
   - Spent 3+ hours on investigation with 0 score improvement
   - Document findings quickly and move to implementation

### What TO Do
1. ✅ **Pick ONE specific test and fix it**
   - Don't aim for general fixes initially
   - One passing test > perfect understanding of all failures

2. ✅ **Write test FIRST, then fix**
   - Conformance test as acceptance criteria
   - Unit test for the specific issue
   - Then implement

3. ✅ **Commit small wins frequently**
   - Even fixing 1-2 tests is progress
   - Build momentum with visible improvements

## Immediate Next Actions

### Action 1: Fix One False Positive (30 min)
Pick the simplest false positive test and fix it.

**Example**: `acceptSymbolAsWeakType.ts`
- Expected: [] (no errors)
- Actual: [TS2349, TS2351, TS2769]
- Issue: WeakSet/WeakMap should accept symbols in ES2022+
- Fix: Check lib version before emitting these errors

**Steps**:
1. Write unit test that WeakSet accepts symbol
2. Find where TS2349/TS2351/TS2769 are emitted for WeakSet
3. Add version check
4. Run test
5. Commit

### Action 2: Fix One Missing Error (30 min)
Find a test expecting exactly 1 error we don't emit.

**Find candidates**:
```bash
./scripts/conformance.sh run --offset 0 --max 1000 --verbose 2>&1 | \
  grep -B 3 "actual:   \[\]$" | \
  grep "expected: \[TS[0-9]\+\]$"
```

Pick the simplest one and implement.

### Action 3: Run Analysis More Strategically (15 min)
Instead of analyzing all tests, find patterns in small batches:

```bash
# Find all tests with exactly 1 error difference
./scripts/conformance.sh run --offset 0 --max 500 2>&1 | \
  python3 -c "import sys, json; [print(line) for line in sys.stdin if 'expected: [TS' in line]"
```

## Practical Workflow

### Morning Session (2 hours)
1. Pick 1 false positive test (most common: TS2322, TS2345, TS2339)
2. Write failing unit test
3. Find emission location
4. Fix
5. Verify & commit
6. **Goal**: +1-5 passing tests

### Afternoon Session (2 hours)
1. Pick 1 missing error test
2. Implement error code if needed
3. Add check
4. Test & commit
5. **Goal**: +1-5 passing tests

### Daily Target
- **Minimum**: +2 passing tests
- **Good**: +5 passing tests
- **Excellent**: +10 passing tests

## Error Code Priority (Revised)

### Tier 1: Simple False Positives (Fix These First)
Look for false positives in:
- Type compatibility checks (TS2322, TS2345)
- Where we're too strict vs TSC
- Version/lib specific features

### Tier 2: Simple Missing Checks
- Single-location checks
- Clear error conditions
- No complex type system logic needed

### Tier 3: Complex Issues (Document & Defer)
- Solver architecture issues
- Parser rewrites
- Module resolution overhauls

## Success Metrics

### Week 1
- Pass rate: 67.7% → 70% (+2.3%)
- Tests: 2,125 → 2,200 (+75 tests)
- Daily commits: 2-3 per day
- Method: Fix specific tests, not general patterns

### Week 2
- Pass rate: 70% → 75% (+5%)
- Tests: 2,200 → 2,350 (+150 tests)
- Start finding patterns after individual fixes

### Month 1
- Pass rate: 75% → 85% (+10%)
- Tests: 2,350 → 2,670 (+320 tests)
- Can tackle some medium-complexity general fixes

## Tools & Commands

### Find simplest failing tests
```bash
./scripts/conformance.sh run --offset 0 --max 100 --verbose 2>&1 | \
  grep -A 10 "^FAIL" | less
```

### Test specific file
```bash
./scripts/conformance.sh run --filter "testname" --verbose
```

### Quick verification
```bash
cargo nextest run --filter specific_test
```

### Fast iteration
```bash
# Terminal 1: Watch mode
cargo watch -x 'nextest run test_name'

# Terminal 2: Edit code

# Terminal 3: Run conformance check
./scripts/conformance.sh run --filter pattern
```

## Anti-Patterns to Avoid

1. **Analysis Paralysis**: Don't spend >30min investigating without coding
2. **Perfect Solution Seeking**: Fix 1 test before trying to fix 100
3. **Deep Diving**: Document complex issues and move on
4. **No Commits**: Commit every working fix immediately
5. **Breaking Tests**: Always run `cargo nextest run` before committing

## Remember

> **Perfect is the enemy of good. Ship small wins daily.**

One test fixed and committed is worth more than understanding 100 tests deeply.
