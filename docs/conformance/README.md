# Conformance Test Documentation

This directory contains investigations, analyses, and session notes for improving tsz's conformance with the TypeScript compiler.

## Current Status

**Overall Conformance**: ~60% pass rate across 12,404 tests

**By Slice:**
- Slice 1 (tests 0-2,927): 62.5% (1,742/2,786 passed)
- Slice 2 (tests 3,101-6,201): 57.2% (1,734/3,030 passed)
- Slice 3: Not yet analyzed
- Slice 4: Not yet analyzed

## Documents

### Investigation Reports

- **[SLICE_2_INVESTIGATION.md](./SLICE_2_INVESTIGATION.md)** - Comprehensive analysis of slice 2 failures
  - 203 lines of detailed analysis
  - Categorizes 1,296 failures by type
  - Identifies high-impact issues
  - Provides prioritized recommendations
  - Includes debugging guides

- **[slice1-analysis.md](./slice1-analysis.md)** - Quick analysis of slice 1
  - Summary of pass/fail rates
  - Top error code mismatches
  - Key observations

### Session Notes

- **[SESSION_2026-02-09.md](./SESSION_2026-02-09.md)** - Detailed session summary
  - Investigation timeline
  - Attempts and lessons learned
  - Recommendations for next steps
  - Time tracking

## Key Findings

### High-Impact Issues (Quick Wins)

**False Positives** (we emit errors TSC doesn't):
- TS2339 (89 tests): Property access errors
- TS2322 (86 tests): Type assignment errors
- TS2345 (77 tests): Argument type errors

Fixing these error codes can flip many tests to PASS instantly.

### Missing Diagnostics

**Missing Errors** (TSC emits, we don't):
- TS2304 (46 tests in slice 1): Cannot find name
- TS2307 (42 tests in slice 2): Module not found
- TS2300 (23 tests in slice 1): Duplicate identifier

These require implementing new diagnostic checks.

### Common Patterns

1. **Type Narrowing Issues**
   - Control flow analysis not preserving types correctly
   - instanceof/typeof guards not working for complex types
   - Generic indexed access types narrowing to `never`

2. **Module/Namespace Issues**
   - Ambient module context not tracked
   - Namespace member exports incorrect
   - Import alias resolution fragile

3. **Declaration Merging**
   - Some valid merges not recognized
   - Duplicate detection incomplete

## Recommended Approach

### For Next Session

1. **Start with TS1206** (low complexity, ~13 tests)
   - Import helpers validation
   - Isolated fix, unlikely to break tests

2. **Fix TS18050** (medium complexity)
   - Generic type narrowing producing `never`
   - Clear root cause identified

3. **Tackle specific TS2339 patterns** (medium-high complexity)
   - Pick reproducible patterns
   - Don't try to fix all 89 tests at once

### Avoid For Now

1. **Namespace exports in ambient modules** (high complexity)
   - Requires architectural changes
   - Multiple systems involved

2. **Duplicate import detection** (high complexity)
   - Can't change binder without breaking resolution
   - Needs alternative approach

3. **Cross-closure narrowing** (high complexity)
   - Complex control flow analysis
   - High risk of regressions

## Investigation Lessons

### What Works

1. **Start with unit tests** - Write failing test first
2. **Use tracing early** - `TSZ_LOG=debug` helps identify issues quickly
3. **Test incrementally** - Small changes, frequent testing
4. **Reference TSC source** - Check TypeScript's implementation for ambiguous cases

### What Doesn't Work

1. **Quick fixes** - Average fix takes 1-2 days (per project docs)
2. **Binder changes** - Affects type resolution across the system
3. **Guessing** - Need deep understanding of affected systems

### Common Pitfalls

1. **Underestimating complexity** - Even "simple" issues touch multiple systems
2. **Skipping unit tests** - Integration tests catch issues too late
3. **Not using tracing** - Wastes time guessing instead of observing
4. **Breaking existing tests** - Core changes have wide-ranging effects

## Tools & Commands

### Running Tests

```bash
# Analyze failures (recommended first step)
./scripts/conformance.sh analyze --offset 3101 --max 3101

# Run specific slice
./scripts/conformance.sh run --offset 3101 --max 3101

# Filter by error code
./scripts/conformance.sh run --error-code 2339 --verbose

# Compare with TSC
./.target/dist-fast/tsz <test-file> 2>&1
tsc --noEmit <test-file> 2>&1
```

### Debugging

```bash
# Run with tracing
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts 2>&1 | head -200

# Filter to specific module
TSZ_LOG="tsz_binder::state_binding=trace" cargo run -- test.ts

# Run unit tests
cargo test --lib <test-name>
```

## Time Investment

Based on actual investigations:

- **Namespace exports**: 2 hours (reverted - needs architectural understanding)
- **Duplicate imports**: 1.5 hours (reverted - broke type resolution)
- **Documentation**: 1 hour
- **Total session**: 4.5 hours

**Reality check**: Each successful fix takes 1-2 days on average (from project docs).

## Related Documentation

- [../HOW_TO_IMPROVE_CONFORMANCE.md](../HOW_TO_IMPROVE_CONFORMANCE.md) - Step-by-step guide
- [../conformance-reality-check.md](../conformance-reality-check.md) - Honest complexity assessment
- [../conformance-fix-guide.md](../conformance-fix-guide.md) - Implementation workflow

## Contributing

When adding new analysis or session notes:

1. Create a descriptive filename (e.g., `SESSION_YYYY-MM-DD.md`)
2. Include pass/fail rates and test counts
3. Document specific issues investigated
4. Provide recommendations for next steps
5. Update this README with new findings

## Questions?

- Check the main conformance guide: `../HOW_TO_IMPROVE_CONFORMANCE.md`
- Review reality check: `../conformance-reality-check.md`
- Read slice 2 investigation: `./SLICE_2_INVESTIGATION.md`
