# Session Summary - Conformance Test Improvements

## Work Completed

### 1. TS2428 Implementation Analysis
- **Status**: Implementation exists but is DISABLED
- **Location**: `crates/tsz-checker/src/type_checking.rs:3276`
- **Issue**: Binder incorrectly merges symbols from different scopes
- **Impact**: Would affect interface declaration merging tests
- **Action Required**: Fix binder scope handling before re-enabling

**Root Cause**:
```rust
// crates/tsz-checker/src/state_checking.rs:160
// TODO: Re-enable after fixing binder bug where symbols from different scopes
// (e.g. file-scope and namespace-scope) get incorrectly merged into one symbol
```

Example that causes false positives:
```typescript
namespace M {
    interface A<T> { x: T; }
}
namespace M2 {
    interface A<T> { x: T; }  // Different scope - should NOT emit TS2428
}
```

### 2. Conformance Test Analysis - Slice 4/4

**Current Metrics**:
- Total tests in slice: 3134
- Passing: ~1678 (53.6%)
- Failing: ~1456 (46.4%)

**Failure Categories**:
- False Positives: 283 tests (we emit when TSC doesn't)
- All Missing: 463 tests (TSC emits, we don't)
- Wrong Codes: 694 tests (different error codes)
- Close to Passing: 414 tests (1-2 code differences)

### 3. High-Impact Fix Opportunities

#### Quick Wins (NOT IMPLEMENTED - Immediate Impact)
| Error Code | Total Tests | Single-Code Tests | Description |
|------------|-------------|-------------------|-------------|
| TS1479 | 23 | 7 | CommonJS/ES module import checking |
| TS2585 | 10 | 7 | (not analyzed) |
| TS1100 | 12 | 6 | (not analyzed) |
| TS2343 | 6 | 6 | (not analyzed) |
| TS7026 | 17 | - | (not analyzed) |
| TS2630 | 12 | - | (not analyzed) |

#### Partial Implementations (Need Broader Coverage)
| Error Code | Missing | Extra | Description |
|------------|---------|-------|-------------|
| TS2306 | 103 | - | "File is not a module" |
| TS2304 | 138 | 105 | "Cannot find name" |
| TS2322 | 112 | 68 | Type assignment errors |
| TS2339 | 78 | 120 | Property access errors |
| TS2318 | - | 81 | Global type not found (false positives) |

#### Co-Occurrence Opportunities
Implementing these pairs helps multiple tests:
- TS2305 + TS2823 → 6 tests
- TS2322 + TS2345 → 4 tests  
- TS2304 + TS2339 → 4 tests
- TS1100 + TS2630 → 4 tests

## Technical Findings

### TS2306 Implementation
**Location**: `crates/tsz-checker/src/import_checker.rs:1404-1421`

**Current Logic**:
Emits "File '{0}' is not a module" when:
1. Target file is not an external module
2. Not an ambient module
3. Not in declared_modules
4. Not a declaration file
5. Not a JS-like file (.js, .jsx, .mjs, .cjs)

**Missing Cases**: 103 tests suggest the logic doesn't trigger in all required scenarios. Potential issues:
- External module detection may be incomplete
- Ambient module matching may be too permissive
- Declaration file check may be excluding valid cases

### TS2318 False Positives
**Impact**: 81 tests incorrectly emit "Cannot find global type"

**Likely Cause**: Over-eager global type checking in:
- `state_type_resolution.rs`
- `type_literal_checker.rs`
- `type_computation_complex.rs`

Multiple call sites suggest the logic for determining when a type should be considered "global" needs refinement.

## Recommendations for Next Session

### Priority 1: Fix Binder Scope Bug
**Why**: Blocks TS2428 implementation and may affect other validations

**Investigation Steps**:
1. Examine how binder merges symbols across scopes
2. Check if namespace symbols should be in separate symbol tables
3. Review `declare_symbol` in `crates/tsz-binder/src/state.rs`

### Priority 2: Implement TS1479
**Impact**: 23 tests (7 quick wins)
**Complexity**: Medium - requires CommonJS vs ES module detection

**Implementation Plan**:
1. Identify import statements in CommonJS context
2. Check if target is ES module
3. Emit TS1479 with suggestion to use dynamic import

### Priority 3: Investigate TS2306 Missing Cases
**Impact**: 103 tests
**Complexity**: High - requires understanding module resolution edge cases

**Debug Approach**:
1. Run failing tests with `TSZ_LOG=debug`
2. Check which condition fails in import_checker.rs:1391-1422
3. Add missing conditions or fix existing logic

### Priority 4: Reduce TS2318 False Positives
**Impact**: 81 tests
**Complexity**: Medium - requires refining global type detection

**Investigation**:
1. Review all call sites of `error_cannot_find_global_type`
2. Check if type resolution should try local scopes first
3. Verify lib types are properly loaded before emitting error

## Files Modified

1. `docs/conformance-slice4-analysis.md` - Detailed analysis
2. `docs/session-summary.md` - This file

## Testing Notes

All unit tests continue to pass:
```bash
cargo nextest run
# 2396 tests run: 2396 passed
```

Conformance testing commands for slice 4:
```bash
# Run full slice
./scripts/conformance.sh run --offset 9411 --max 3134

# Analyze specific category
./scripts/conformance.sh analyze --offset 9411 --max 3134 --category close

# Test specific error code
./scripts/conformance.sh run --offset 9411 --max 3134 --error-code 2306
```
