# tsz Type System Status - February 13, 2026

## Current State

**Conformance**: 87-97% pass rate on TypeScript tests (varies by test range)
**Unit Tests**: 2,394 passing ✅
**Build**: Clean, no warnings

## Documented Issues Ready for Implementation

### 🎯 Priority 1: Generic Function Inference (HIGHEST IMPACT)

**Status**: ✅ **READY FOR IMPLEMENTATION**

**Impact**: Unblocks **~100+ conformance tests**

**Files**:
- Issue documentation: `docs/issues/generic-function-inference-pipe-pattern.md`
- Implementation guide: `docs/IMPLEMENTATION-GUIDE-generic-inference.md`
- Test case: `tmp/pipe_simple.ts`

**What's Ready**:
- ✅ Root cause identified and documented
- ✅ Minimal reproduction verified
- ✅ Step-by-step implementation guide
- ✅ Three solution approaches analyzed
- ✅ Testing strategy with specific commands
- ✅ Success criteria defined

**Estimated**: 3-5 hours for careful implementation

**Next Steps**:
1. Read `docs/IMPLEMENTATION-GUIDE-generic-inference.md`
2. Implement "defer instantiation" approach in `operations.rs:2271-2445`
3. Verify with `tmp/pipe_simple.ts`
4. Run full test suite

---

### 🔍 Priority 2: Contextual Typing in Non-Strict Mode

**Status**: Investigated, needs further analysis

**Files**:
- Issue documentation: `docs/issues/contextual-typing-non-strict.md`
- Test infrastructure: `crates/tsz-checker/src/tests/property_access_non_strict.rs`

**What's Known**:
- Issue is in lambda parameter typing, NOT property access
- TypeScript handles `noImplicitAny: false` differently for contextual types
- Previous fix attempt was too broad (broke 9 tests)

**Next Steps**:
1. Study TypeScript's `inferTypes` implementation
2. Determine how `noImplicitAny` affects parameter type inference
3. Write targeted test cases for different scenarios
4. Implement correct fix

---

### 📝 Priority 3: Recursive Mapped Type Inference

**Status**: Identified, not yet investigated

**Test**: `TypeScript/tests/cases/compiler/mappedTypeRecursiveInference.ts`

**Problem**: `Deep<T> = { [K in keyof T]: Deep<T[K]> }` not inferring correctly

**Next Steps**:
1. Study `crates/tsz-solver/src/evaluate_rules/mapped.rs`
2. Investigate coinductive inference for recursive structures
3. Create minimal reproduction
4. Implement fix

---

## Session Documentation

Comprehensive documentation in `docs/`:
- `docs/SESSION-2026-02-13.md` - Session notes
- `docs/FINAL-SESSION-SUMMARY-2026-02-13.md` - Complete summary
- `docs/IMPLEMENTATION-GUIDE-generic-inference.md` - Implementation guide

## Quick Commands

### Build
```bash
cargo build --profile dist-fast -p tsz-cli
```

### Test Minimal Case
```bash
.target/dist-fast/tsz tmp/pipe_simple.ts
```

### Unit Tests
```bash
cargo nextest run
```

### Conformance Tests
```bash
# First 100 tests
./scripts/conformance.sh run --max 100

# Specific test
.target/dist-fast/tsz TypeScript/tests/cases/compiler/TEST_NAME.ts
```

### Compare with TSC
```bash
cat TypeScript/tests/baselines/reference/TEST_NAME.errors.txt
```

## Key Metrics

- **Unit Tests**: 2,394 passing (100%)
- **Conformance**: 87-97% pass rate
- **Documented Issues**: 3
- **Ready for Implementation**: 1 (highest impact)
- **Test Infrastructure**: Complete

## Architecture Notes

From `docs/HOW_TO_CODE.md`:
- ✅ Checker never inspects TypeKey
- ✅ Solver owns all type logic
- ✅ Use tracing, never `eprintln!`
- ✅ Measure performance on changes to hot paths

## Recent Commits

1. `a7cf49a5c` - Implementation guide for generic inference
2. `e35fbb11f` - Comprehensive session summary
3. `033e1908a` - Verify generic function inference issue
4. `bbd4954c2` - Session documentation
5. `b1eb6c023` - Investigate contextual typing
6. `fcf2861a1` - Add property access tests (WIP)
7. `f2895823c` - Document pipe pattern issue

All commits synced with remote ✅

## For Next Session

**Recommended**: Start with Priority 1 (Generic Function Inference)
- Highest impact (~100+ tests)
- Best documented
- Clear implementation path
- 3-5 hour estimate

**Alternative**: If more investigation time available, Priority 2 (Contextual Typing)

## Contact

Issues, documentation, and guides are comprehensive. Any developer can pick up
and continue from here with confidence.
