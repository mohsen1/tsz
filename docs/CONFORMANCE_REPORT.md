# TSZ Conformance Report

**Date**: 2026-02-12
**Test Suite**: TypeScript Official Conformance Tests (12,583 tests)
**TSZ Status**: Early development, ~39.8% passing

---

## Executive Summary

TSZ is a high-performance TypeScript compiler implementation that currently passes **7,700 out of 12,583 conformance tests (61.2%)**. The remaining 4,883 failing tests (38.8%) identify gaps in error checking, type narrowing, and module handling. This report categorizes the failures and provides implementation priorities.

### Key Metrics

| Metric | Count | Percentage |
|--------|-------|-----------|
| **Passing Tests** | 7,700 | 61.2% |
| **Failing Tests** | 4,883 | 38.8% |
| | | |
| **False Positives** | 1,280 | We emit errors when we shouldn't |
| **All Missing** | 1,441 | We emit nothing when we should |
| **Wrong Codes** | 2,162 | Both have errors but codes differ |
| **Close to Passing** | 1,297 | Differ by only 1-2 error codes |

---

## Problem Categories

### 1. False Positives (1,280 tests) - FIX = INSTANT WINS

These are tests where TSZ emits error codes that the official TypeScript compiler doesn't emit. **Fixing these yields immediate test passes.**

#### Top False Positive Error Codes

| Code | Tests Affected | Category |
|------|---------------|----------|
| **TS2339** | 284 | Property doesn't exist on type (overly strict) |
| **TS2345** | 262 | Argument type mismatch (overly strict) |
| **TS2322** | 241 | Type not assignable (overly strict) |
| **TS2304** | 66 | Name not found (spurious) |
| **TS1005** | 56 | Syntax error (parser issue) |
| **TS7006** | 52 | Parameter implicitly any (incomplete narrowing) |
| **TS2769** | 49 | Overload resolution (too strict) |
| **TS7011** | 34 | Binding error |
| **TS1109** | 28 | Expression expected (parser issue) |
| **TS2344** | 25 | Constraint violation (generics) |

#### Root Causes

1. **Property access checks too strict** - Not respecting `any` propagation
2. **Function argument checking too strict** - Not handling overloads correctly
3. **Type compatibility too strict** - Missing edge cases in assignability
4. **Parser emitting unneeded errors** - Some parse errors that tsc ignores

#### Impact

Fixing these alone would improve pass rate to ~71.5% (7,700 + 1,280 = 8,980 / 12,583).

---

### 2. All Missing (1,441 tests) - IMPLEMENT = NEW PASSES

These are tests where TSZ emits **no errors** but TypeScript expects specific error codes. These represent missing error checking features.

#### Top All-Missing Error Codes

| Code | Tests Affected | Category | Implementation Status |
|------|---------------|----------|---------------------|
| **TS2322** | 126 | Assignment compatibility | Partial (only catches some cases) |
| **TS2339** | 87 | Property doesn't exist | Partial (needs context awareness) |
| **TS2304** | 49 | Name not found | Partial (some edge cases) |
| **TS2345** | 48 | Argument type mismatch | Partial (overload resolution) |
| **TS2300** | 39 | Duplicate identifier | Missing (declaration merging validation) |
| **TS1005** | 28 | Syntax error (parse-time) | Parser incomplete |
| **TS2411** | 21 | Type extends type | Missing (extends validation) |
| **TS2353** | 18 | Property does not exist (object literal) | Missing |
| **TS2741** | 18 | Property missing in type | Missing (literal type precision) |
| **TS7006** | 17 | Parameter implicitly any | Missing (parameter inference) |

#### Root Causes

1. **Incomplete type checking** - Many check conditions not fully implemented
2. **Missing error codes** - Some codes not emitted at all (from "NOT IMPLEMENTED" list)
3. **Insufficient narrowing** - Type guards don't narrow in all cases
4. **Module system edge cases** - Import/export validation incomplete

#### Quick Win Opportunity

Implementing the top 10 missing codes would add ~**375 passing tests**.

---

### 3. Wrong Codes (2,162 tests) - REFINE = CLOSER TO PASSING

These tests have errors in both TSZ and TypeScript, but the **error codes don't match**. The logic is partially there but needs refinement.

#### Top Extra Codes in Wrong-Code Tests

| Code | Tests Affected | Issue |
|------|---------------|-------|
| **TS1005** | 196 | Syntax error (we emit too many) |
| **TS2792** | 173 | Union conversion (overly strict) |
| **TS2339** | 165 | Property access (false positives) |
| **TS2304** | 162 | Name resolution (overly strict) |
| **TS2345** | 122 | Function arguments (overly strict) |
| **TS1128** | 122 | Unreachable code (over-reporting) |
| **TS2322** | 121 | Assignment (overly strict) |
| **TS1109** | 120 | Expression parsing (too strict) |
| **TS2318** | 87 | Type not instantiable (missing cases) |
| **TS2305** | 80 | Module import (edge cases) |

#### Strategy

These tests are **close to passing** - we're emitting errors in the right places but with slightly different codes. Refinement needed in:
- Type compatibility logic
- Union type handling
- Name resolution rules
- Syntax validation

---

### 4. Close to Passing (1,297 tests)

Tests that differ by only **1-2 error codes**. These represent the most achievable quick wins.

#### Quick Win Opportunities

| Code | Tests Affected | Effort |
|------|--------|--------|
| **TS2322** (partial) | 85 | Fix assignment compatibility edge case |
| **TS2339** (partial) | 50 | Fix property access type narrowing |
| **TS2304** (partial) | 31 | Fix name resolution in specific contexts |
| **TS2345** (partial) | 28 | Fix overload resolution |
| **TS2300** (partial) | 19 | Implement duplicate identifier detection |
| **TS2411** (partial) | 18 | Implement extends validation |
| **TS2353** (partial) | 14 | Fix object literal property checking |
| **TS2403** (partial) | 12 | Fix async function return type |
| **TS2307** (partial) | 12 | Fix module not found detection |
| **TS2792** (partial) | 10 | Fix union type conversion |

**Total Potential**: Fixing these 10 categories would add ~**279 passing tests** → **75.4% pass rate**.

---

## Not Implemented Error Codes

These codes are **never emitted by TSZ** - implementing them will have immediate impact.

### Critical Missing Codes (high impact)

| Code | Tests | Category | Effort |
|------|-------|----------|--------|
| **TS7026** | 33 | Unused variable declaration | Low |
| **TS2551** | 25 | Property exists but is private | Medium |
| **TS2585** | 17 | Object class pattern | Medium |
| **TS1011** | 17 | Initializer not allowed | Low |
| **TS2528** | 17 | Can't assign to readonly | Low |
| **TS1100** | 17 | Invalid use of super | Medium |
| **TS2823** | 17 | Type is readonly | Low |
| **TS2874** | 16 | Type is incompatible | Low |
| **TS2371** | 16 | Declaration is private | Low |
| **TS1125** | 16 | Module only valid in files | Low |

### All Not-Implemented Codes (626 codes total)

These 626 error codes that TSZ never emits collectively affect **1,927 failing tests**. A prioritized implementation plan focusing on the highest-impact codes would be valuable.

---

## Partially Implemented Error Codes

These codes work in **some cases** but are missing in others. They represent incomplete implementations.

### Top Partially Implemented Codes

| Code | Missing in | Status |
|------|-----------|--------|
| **TS2322** | 272 tests | Assignment checking incomplete |
| **TS2304** | 270 tests | Name resolution incomplete |
| **TS2307** | 217 tests | Module resolution incomplete |
| **TS2339** | 161 tests | Property access checking incomplete |
| **TS6053** | 159 tests | File not found checking incomplete |
| **TS1005** | 114 tests | Syntax error checking incomplete |
| **TS2345** | 88 tests | Function argument checking incomplete |
| **TS2741** | 76 tests | Literal type checking incomplete |
| **TS2300** | 66 tests | Duplicate identifier detection partial |
| **TS2792** | 58 tests | Union conversion incomplete |

---

## Co-Occurrence Analysis

### Error Code Pairs That Appear Together

When multiple error codes appear in the same test, fixing them together gives more impact.

#### High-Impact Pairs

| Pair | Tests | Implementation Strategy |
|------|-------|------------------------|
| **TS2322 + TS2345** | 10 | Refine assignment + function argument checking |
| **TS2305 + TS2823** | 6 | Module export + readonly property |
| **TS2322 + TS2339** | 5 | Assignment + property access |
| **TS7005 + TS7034** | 4 | Focus on parsing rules |
| **TS1005 + TS2304** | 4 | Syntax + name resolution |
| **TS2304 + TS2339** | 4 | Name resolution + property access |

#### Implementation Principle

Rather than implementing codes in isolation, implement them in groups. This maximizes test pass rate per unit of work.

---

## Root Cause Analysis by Feature Area

### Type System

**Issues**:
- Assignment compatibility not catching all edge cases
- Function argument checking too strict or too lenient depending on context
- Union type handling incomplete (over-narrowing or under-narrowing)
- Generic type instantiation missing some edge cases

**Evidence**: TS2322 (assignment), TS2345 (function args), TS2769 (overloads) consistently high

**Fix Area**: `crates/tsz-checker/src/assignability_checker.rs` and related type checking modules

### Property Access

**Issues**:
- Property access checks are overly strict or incomplete depending on object type
- Optional chaining may not be handled consistently
- Index signatures not fully respected
- Readonly property violations not always caught

**Evidence**: TS2339 (property), TS2353 (object property) consistently high

**Fix Area**: `crates/tsz-checker/src/expr.rs` and property resolution logic

### Name Resolution

**Issues**:
- Some name lookups fail in specific scopes
- Module resolution edge cases not covered
- Declaration merging validation incomplete
- Private/protected checking inconsistent

**Evidence**: TS2304 (name), TS2307 (module), TS2551 (private) appearing frequently

**Fix Area**: `crates/tsz-checker/src/symbol_resolver.rs` and scope-related logic

### Control Flow & Narrowing

**Issues**:
- Type guards not narrowing in all cases
- Definite assignment analysis incomplete
- Reachability analysis may be too strict or too lenient
- Variable initialization not always tracked properly

**Evidence**: Tests with multiple narrowing checks fail

**Fix Area**: `crates/tsz-checker/src/flow_analyzer.rs` and narrowing modules

### Module System

**Issues**:
- Export validation incomplete
- Re-export handling buggy
- Namespace merging not fully implemented
- Module augmentation edge cases

**Evidence**: TS2305 (module), TS2503 (augmentation) appearing in many tests

**Fix Area**: `crates/tsz-checker/src/` module resolution and export checking

### Parser Errors

**Issues**:
- Some valid syntax flagged as errors
- Error recovery creating spurious errors
- Specific syntax constructs not fully supported

**Evidence**: TS1005, TS1109, TS1128 (syntax errors) high false positive count

**Fix Area**: `crates/tsz-parser/src/` - may need grammar refinements

---

## Recommended Implementation Priority

### Phase 1: Quick Wins (Immediate Impact)

**Goal**: Increase pass rate from 61.2% to 75%+ (add ~1,700 passing tests)

1. **Fix TS2322 false positives** (241 tests become false positives)
   - Review assignment checking logic
   - Identify over-strict cases
   - Add exception handling for edge cases
   - Estimated effort: 4-6 hours
   - Impact: +85 tests (partially), +241 (fix false positives)

2. **Fix TS2339 false positives** (284 tests become false positives)
   - Review property access narrowing
   - Improve `any` propagation in property lookups
   - Handle optional properties correctly
   - Estimated effort: 3-5 hours
   - Impact: +50 tests (partially), +284 (fix false positives)

3. **Fix TS2345 false positives** (262 tests become false positives)
   - Review function call argument checking
   - Improve overload resolution
   - Better error reporting for mismatches
   - Estimated effort: 4-6 hours
   - Impact: +28 tests (partially), +262 (fix false positives)

### Phase 2: Core Missing Features (Medium Effort)

**Goal**: Increase pass rate to 80%+

4. **Implement TS2300** - Duplicate identifier detection
   - Check for redeclaration violations
   - Validate declaration merging rules
   - Estimated effort: 2-3 hours
   - Impact: +19 tests (partially), +39 tests (all missing)

5. **Implement TS2411** - Type extends validation
   - Check generic type constraints
   - Validate extends clauses
   - Estimated effort: 3-4 hours
   - Impact: +18 tests (partially), +21 tests (all missing)

6. **Fix TS2304 incomplete** - Name resolution
   - Review symbol lookup in all scopes
   - Handle import aliases correctly
   - Validate binding timing
   - Estimated effort: 4-5 hours
   - Impact: +31 tests (partially), +49 tests (all missing)

### Phase 3: Advanced Features (Longer Term)

**Goal**: Continue toward 90%+ pass rate

7. **Implement the 626 not-implemented codes**
   - Start with highest-impact codes (TS7026, TS2551, TS2585)
   - Build infrastructure for common patterns
   - Estimated effort: 20-30 hours (prioritized subset)
   - Impact: +~500-800 tests

8. **Refine type narrowing**
   - Improve control flow analysis
   - Better type guard recognition
   - More precise readonly handling
   - Estimated effort: 10-15 hours
   - Impact: +~200-300 tests

9. **Complete module system**
   - Full export validation
   - Re-export handling
   - Namespace merging
   - Declaration augmentation
   - Estimated effort: 15-20 hours
   - Impact: +~150-200 tests

---

## File Organization for Fixes

### Error Checking Modules to Review

```
crates/tsz-checker/src/
├── assignability_checker.rs    ← TS2322, TS2345, TS2353
├── expr.rs                      ← TS2339, TS2353 (property access)
├── call_checker.rs              ← TS2345, TS2769 (function calls)
├── symbol_resolver.rs           ← TS2304, TS2307 (name resolution)
├── flow_analyzer.rs             ← Type narrowing, TS7006
├── declarations.rs              ← TS2300 (duplicate), TS2411 (extends)
├── interface_type.rs            ← Interface implementation
├── class_checker.rs             ← Class inheritance validation
├── operator_checker.rs          ← TS1005 (syntax)
└── error_reporter.rs            ← Error code emission
```

### Solver Modules (Already Solid)

The Solver layer is comprehensive. Most issues are in the Checker layer deciding WHICH error codes to emit, not in type computation itself.

```
crates/tsz-solver/src/
├── subtype.rs                   ← Type compatibility (likely correct)
├── infer.rs                     ← Type inference
├── narrowing.rs                 ← Type guard evaluation
├── judge.rs & lawyer.rs         ← Subtyping rules (likely correct)
```

---

## Data Summary

### Test Results by Category

| Category | Count | % of Total |
|----------|-------|-----------|
| PASS | 7,700 | 61.2% |
| False Positive | 1,280 | 10.2% |
| All Missing | 1,441 | 11.4% |
| Wrong Codes | 2,162 | 17.2% |

### Error Code Statistics

| Metric | Count |
|--------|-------|
| Total diagnostic codes defined | 2,129 |
| Codes never emitted (not implemented) | 626 |
| Codes partially implemented | 202 |
| Codes always correct | ~1,301 |

### Test Examples

#### Top False Positive Tests (to investigate)

```
acceptSymbolAsWeakType.ts
  Expected: []
  Actual: [TS2345, TS2769]

aliasOfGenericFunctionWithRestBehavedSameAsUnaliased.ts
  Expected: []
  Actual: [TS2322]

arrayFind.ts
  Expected: []
  Actual: [TS2322]
```

#### Top All-Missing Tests

```
aliasOnMergedModuleInterface.ts
  Expected: [TS2708]
  Actual: []

assignToEnum.ts
  Expected: [TS2540, TS2628]
  Actual: []

assignToExistingClass.ts
  Expected: [TS2629]
  Actual: []
```

---

## Running Conformance Tests Locally

### Generate Test Results

```bash
# Full analysis with rankings
./scripts/conformance.sh analyze

# Run specific tests
./scripts/conformance.sh run --max 100         # First 100 tests
./scripts/conformance.sh run --filter "strict" # Tests matching pattern
./scripts/conformance.sh run --error-code 2304 # TS2304 only

# Analyze failures by category
./scripts/conformance.sh analyze --category false-positive
./scripts/conformance.sh analyze --category all-missing
./scripts/conformance.sh analyze --category close     # 1-2 code diffs
```

### Interpreting Output

- **PASS**: Expected error codes match actual
- **FAIL**: Expected vs actual differ
- **SKIP**: Test marked as skipped
- **CRASH**: TSZ crashed on test
- **⏱️ TIMEOUT**: Test exceeded time limit

### Example Output

```
FAIL ./TypeScript/tests/cases/compiler/acceptSymbolAsWeakType.ts
  expected: []
  actual:   [TS2345, TS2769]
  options:  {strict: true, lib: esnext, target: esnext}

FINAL RESULTS: 48/50 passed (96.0%)
  Skipped: 1
  Crashed: 0
  Timeout: 0
  Time: 2.3s

Top Error Code Mismatches:
  TS2769: missing=0, extra=1
  TS2345: missing=0, extra=1
```

---

## Next Steps

### Immediate Actions

1. **Pick highest-impact false positive code** (TS2339 or TS2345)
2. **Find 5-10 test cases** exhibiting the issue
3. **Debug one test case** with tracing:
   ```bash
   TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts 2>&1 | head -200
   ```
4. **Identify where error is emitted** in checker code
5. **Write fix** with corresponding test
6. **Run conformance** to measure impact

### Long-term Roadmap

- **Month 1**: Phase 1 quick wins (75% pass rate)
- **Month 2**: Phase 2 core features (80% pass rate)
- **Month 3**: Phase 3 advanced features (85%+ pass rate)
- **Month 4+**: Iterative refinement toward 95%+ parity with tsc

---

## Conclusion

TSZ is at an excellent milestone with **61.2% of conformance tests passing**. The remaining gaps are well-characterized:

- **False positives** are the easiest to fix (instant wins)
- **Quick wins** (1-2 code diffs) are very achievable
- **Partially implemented** codes need refinement, not ground-up work
- **Not-implemented** codes are in the long tail but manageable with prioritization

The architecture is sound - most fixes will be in the **Checker layer** (deciding which errors to emit), not in the **Solver layer** (type computation).

**Recommended next step**: Focus on Phase 1 to reach 75% quickly, then reassess.
