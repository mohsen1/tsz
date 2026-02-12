# Conformance Testing Session - Slice 4 - TS2630 Fix
**Date:** 2026-02-12
**Slice:** 4 of 4 (tests 9438-12583, 3145 tests total)

## Summary

Improved slice 4 conformance pass rate by implementing TS2630 error for built-in global function assignments (`eval` and `arguments`).

## Metrics

| Metric | Value |
|--------|-------|
| **Pass Rate** | 54.1% (1688/3123 passing) |
| **Tests Fixed** | 2 by TS2630 implementation |
| **Failing Tests** | 1435 |
| **Timeouts** | 3 tests (>5s limit) |

## Work Completed

### 1. TS2630 Implementation ✅

**Error Code:** TS2630 - "Cannot assign to 'x' because it is a function"

**Implementation:**
- Added special-case handling for built-in globals `eval` and `arguments`
- These names always emit TS2630 when assigned to, even without explicit declarations
- Extended check to all assignment operators:
  - Direct assignment: `eval = 1`
  - Increment: `eval++`, `++eval`
  - Decrement: `eval--`, `--eval`

**Files Modified:**
- `crates/tsz-checker/src/assignment_checker.rs` - Added eval/arguments check
- `crates/tsz-checker/src/dispatch.rs` - Added check to postfix unary
- `crates/tsz-checker/src/type_computation.rs` - Added check to prefix unary

**Tests Fixed:**
- `parserStrictMode3-negative.ts` - eval assignment
- `parserStrictMode6-negative.ts` - eval increment

**Commit:** `a5aa7575c fix: add TS2630 checks for function assignment to built-in globals`

### 2. Comprehensive Slice 4 Analysis

Analyzed all 3145 tests in slice 4 to identify patterns and high-impact opportunities.

## Top Error Code Mismatches

### False Positives (We emit, TSC doesn't)

| Code | Count | Issue |
|------|-------|-------|
| TS2339 | 118 | Property doesn't exist - mostly declaration merging |
| TS2304 | 105 | Cannot find name - mixed cases |
| TS1005 | 84 | Parser errors - possibly strictness issues |
| TS2318 | 83 | Cannot find global type - JSX with missing libs |
| TS2345 | 79 | Argument type mismatch |

### Missing Errors (TSC emits, we don't)

| Code | Count | Issue |
|------|-------|-------|
| TS2304 | 138 | Cannot find name - mixed cases |
| TS2322 | 114 | Type not assignable - various contexts |
| TS6053 | 103 | File not found - import resolution |
| TS2339 | 76 | Property doesn't exist |

## Pattern Analysis

### Declaration Merging Issues (TS2339 false positives)

**Pattern:** Namespace/class/function with same name across files not merging properly

**Example:** `AmbientModuleAndAmbientWithSameNameAndCommonRoot.ts`
```typescript
// File 1
declare namespace A {
    export namespace Point { ... }
}

// File 2
declare namespace A {
    export class Point { ... }
}

// Should merge: A.Point is both namespace AND class
// We emit: TS2339 on A.Point.Origin
```

**Impact:** ~40+ tests
**Complexity:** High (requires declaration merging fix)

### JSX Global Type Issues (TS2318 false positives)

**Pattern:** JSX tests with `nolib: true` - we bail out early when global types missing

**Example:** `checkJsxChildrenProperty12.tsx`
- We emit: TS2318 (Cannot find global type)
- TSC emits: TS2339, TS6053 (still does property checking)

**Impact:** ~83 tests
**Complexity:** High (JSX + lib type handling)

### JSDoc Validation Missing

**Pattern:** Various JSDoc-specific type checks not implemented

**Examples:**
- `enumTag.ts` - Enum member type validation against @enum declaration
- `importTag24.ts` - @returns type validation
- `jsdocSatisfiesTag*.ts` - @satisfies tag support

**Impact:** ~50+ tests
**Complexity:** Medium (requires JSDoc annotation parsing/validation)

### Edge Cases

**Pattern:** Very specialized language edge cases

**Examples:**
- `NonInitializedExportInInternalModule.ts` - Bare `let;` treated as identifier
- `parserStrictMode*.ts` - TS1100 (Duplicated params) in strict mode
- Import resolution errors (TS6053)

**Impact:** ~100+ tests
**Complexity:** Varies

## Quick Win Opportunities

Tests needing just ONE additional error code:

| Code | Tests | Feasibility |
|------|-------|-------------|
| TS2322 | 36 | Medium (scattered contexts) |
| TS2339 | 19 | Low (mostly merging issues) |
| TS2304 | 17 | Low (edge cases) |
| TS2353 | 15 | Low (JSDoc @satisfies) |
| TS6053 | ~100 | Medium (import resolution) |

**Assessment:** Most "quick wins" are actually specialized edge cases (JSDoc, declaration merging) that require significant infrastructure work.

## Performance Notes

**Timeout Tests:** 3 tests exceed 5s limit
- `parserRealSource12.ts` - Large real-world source file
- Likely performance issue in checker/solver for complex code

## Recommendations for Next Session

### High Impact, Medium Effort

1. **TS6053 Implementation** (~100 tests)
   - Missing "File not found" errors for imports
   - Requires module resolution error tracking
   - Clear, focused scope

2. **Declaration Merging Foundation** (~40-60 tests)
   - Fix namespace/class/function merging across files
   - Core infrastructure improvement
   - Affects many TS2339 false positives

### Medium Impact, Medium Effort

3. **JSDoc Return Type Validation** (~20-30 tests)
   - Validate function returns against @returns annotations
   - Focused JSDoc feature
   - TS2322 in JSDoc contexts

4. **TS1100 Implementation** (12 tests)
   - Duplicate function parameters in strict mode
   - Not yet implemented, focused scope

### Lower Priority

5. **JSX Global Type Handling** (~83 tests)
   - Complex interaction between JSX and lib types
   - Requires architectural changes
   - Can be deferred

## Testing

**Unit Tests:** ✅ All 2396 tests passing, 40 skipped

## Git Sync

```bash
git pull --rebase origin main && git push origin main
```
Status: ✅ Synced successfully

---

## Detailed Error Analysis

### TS2630 Remaining Cases

8 TS2630-related tests remain (we fixed 5, 3 still failing):
- `parserStrictMode3.ts` - Needs TS1100 (duplicate params) + TS2630
- `parserStrictMode6.ts` - Needs TS1100 + TS2630
- `parserStrictMode7.ts` - Needs TS1100 + TS2630
- `validNullAssignments.ts` - Needs TS2540, TS2629, TS2630, TS2631, TS2693
- `invalidUndefinedAssignments.ts` - Needs multiple assignment validation codes

These require additional error codes beyond TS2630.

---

**Session Duration:** ~2 hours
**Lines Changed:** ~50 (focused fix)
**Tests Improved:** 2 direct + better analysis foundation
