# Conformance Roadmap (Jan 2026)

Last updated: 2026-01-27 (Post-SymbolId Collision Fix)

## Current State

**Pass Rate: 33.2% (4048/12198)**

### Critical Stability Issues
- **Worker crashes**: 123/137 (90% crash rate!)
- **OOM**: 20 tests
- **Timeouts**: 52 tests

### Top Error Analysis

| Category | Error Code | Count | Type | Root Cause |
|----------|------------|-------|------|------------|
| **Lib Resolution** | TS2318 | 3,360 | Missing | Lib symbol merge not reaching WASM path |
| **Readonly** | TS2540 | 10,520 | Extra | Over-aggressive readonly detection |
| **Type Assignability** | TS2322 | 12,448 extra / 805 missing | Both | Fundamental assignability divergence |
| **Control Flow** | TS2454 | 5,589 | Extra | Used before assignment false positives |
| **Symbol Resolution** | TS2304 | 3,837 extra / 1,987 missing | Both | Scope chain issues |
| **Duplicate** | TS2300 | 2,078 | Extra | False duplicate identifier detection |
| **Iteration** | TS2488 | 1,652 | Missing | Missing Symbol.iterator checks |
| **Parser** | TS1005 | 2,689 | Extra | Expected token errors |

---

## Phase 1: Stability (CRITICAL)

**Goal**: Fix crashes before correctness. Can't measure improvement with 90% crash rate.

### 1.1 Investigate Worker Crashes
```
Crashed Tests (sample):
- parserX_ArrowFunction4.ts
- genericCallWithArrayLiteralArgs.ts
- checkJsdocParamTag1.ts
```

**Action Items**:
- [ ] Run crashed tests individually with RUST_BACKTRACE=1
- [ ] Add panic hooks to capture stack traces in WASM
- [ ] Fix top 5 crash patterns (likely unwrap/expect on None)

### 1.2 Fix OOM Issues
```
OOM Tests:
- classInheritence.ts
- genericDefaultsErrors.ts
- recursiveBaseCheck3.ts
```

**Action Items**:
- [ ] Add recursion depth limits in type resolution
- [ ] Add visited set for recursive type expansion
- [ ] Profile memory usage on OOM test cases

---

## Phase 2: Lib Symbol Resolution (TS2318)

**Problem**: Our lib symbol merge fix isn't reaching the WASM conformance path.

**Current Flow**:
```
WASM: addLibFile() → lib_files.push()
      bindSourceFile() → bind_source_file_with_libs() → merge_lib_symbols()
      checkSourceFile() → checker resolves symbols
```

### 2.1 Verify WASM Path Uses New Merge
- [ ] Add debug logging to `merge_lib_contexts_into_binder`
- [ ] Verify `lib_symbols_merged` flag is true after binding
- [ ] Confirm `get_symbol()` uses fast path

### 2.2 Trace Symbol Resolution Failures
- [ ] Add tracing for TS2318 emission points
- [ ] Log which global types are failing to resolve
- [ ] Compare resolution path vs native (non-WASM) path

### 2.3 Potential Issues
1. **LibContext type mismatch** - Binder vs Checker LibContext
2. **Ordering issue** - Symbols merged after binding, not before
3. **Arena mismatch** - Symbol declarations point to wrong arena

---

## Phase 3: Readonly Over-Reporting (TS2540)

**Problem**: 10,520 extra TS2540 errors. P1 fix (property existence check) had no effect.

### 3.1 Root Cause Analysis
- [ ] Sample 20 failing tests with TS2540
- [ ] Compare tsz vs tsc readonly detection for each
- [ ] Identify patterns: index access? computed properties? mapped types?

### 3.2 Likely Causes
1. **Incorrect readonly flag propagation** from lib symbols
2. **Checking readonly on non-assignment contexts**
3. **Interface readonly vs object literal readonly** confusion
4. **`readonly` modifier on index signatures** incorrectly applied

### 3.3 Fix Strategy
- [ ] Audit `check_readonly_assignment` call sites
- [ ] Ensure only actual assignments trigger readonly check
- [ ] Verify mapped type readonly handling

---

## Phase 4: Type Assignability (TS2322)

**Problem**: 12,448 extra + 805 missing = fundamental assignability divergence

### 4.1 Analyze TS2322 Tracing Output
PR #180 added tracing. Use it:
- [ ] Enable tracing on failing tests
- [ ] Categorize failing assignability patterns
- [ ] Match against TS_UNSOUNDNESS_CATALOG.md

### 4.2 Common Patterns to Fix
1. **Generic instantiation** - Type parameters not matching
2. **Excess property checks** - Too strict on object literals
3. **Union/intersection** - Distribution rules
4. **Variance** - Covariance/contravariance in function params
5. **Bivariant functions** - TypeScript's intentional unsoundness

---

## Phase 5: Control Flow Analysis (TS2454)

**Problem**: 5,589 extra "variable used before assignment" errors

### 5.1 Root Cause
Our control flow analysis is too conservative - we think variables are unassigned when they're actually assigned.

### 5.2 Common Patterns
1. **Assignment in branches** - `if (cond) { x = 1; } else { x = 2; }`
2. **Loop assignments** - Variables assigned in loop body
3. **Function hoisting** - Functions accessing variables before textual assignment
4. **Destructuring** - Complex binding patterns

### 5.3 Fix Strategy
- [ ] Audit flow node propagation for assignments
- [ ] Verify narrowing preserves assignment state
- [ ] Check definite assignment analysis algorithm

---

## Phase 6: Symbol Resolution (TS2304)

**Problem**: Both missing (1,987) AND extra (3,837) "cannot find name" errors

### 6.1 Extra Errors (3,837)
We're failing to find symbols that exist:
- Scope chain not properly walked
- Module resolution failures
- Lib symbols not visible

### 6.2 Missing Errors (1,987)
We're NOT reporting errors for truly undefined symbols:
- Error suppression too aggressive
- Falling back to `any` silently
- noImplicitAny not enforced

---

## Execution Priority

```
Week 1: Phase 1 (Stability)
  - Fix crashes - can't measure anything with 90% crash rate
  - Goal: <5% crash rate

Week 2: Phase 2 (TS2318 - Lib Resolution)
  - Verify lib merge works in WASM
  - Goal: -3,000 missing errors

Week 3: Phase 3 (TS2540 - Readonly)
  - Root cause analysis + fix
  - Goal: -10,000 extra errors

Week 4: Phase 4-6 (TS2322, TS2454, TS2304)
  - Incremental improvements
  - Goal: -5,000 extra errors each
```

---

## Success Metrics

| Metric | Current | Target (4 weeks) |
|--------|---------|------------------|
| Pass Rate | 33.2% | 60%+ |
| Crash Rate | 90% | <5% |
| TS2318 missing | 3,360 | <500 |
| TS2540 extra | 10,520 | <2,000 |
| TS2322 extra | 12,448 | <5,000 |

---

## Appendix: Previous Work

### Completed (2026-01-27)
- ✅ P0 (TS2749): #179, #181 - Value-only symbol checks (WORKED - not in top errors)
- ✅ P1 (TS2540): #183 - Property existence before readonly (DID NOT HELP)
- ✅ P2 (TS2339): #182 - Union property access (WORKED - reduced 75%)
- ✅ P3 (TS2318): #178 - noLib global type resolution (PARTIAL - noLib specific)
- ✅ P4 (TS2322): #180 - Tracing added
- ✅ SymbolId Collision Fix: lib symbol remap (NEEDS VERIFICATION)
