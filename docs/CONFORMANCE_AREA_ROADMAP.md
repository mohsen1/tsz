# Conformance Area Roadmap

Tracking systematic work to close conformance gaps by feature area.
Baseline: **7883/12574 (62.7%)** as of 2026-02-15 (pre-TS6 defaults).
Current: **8300/12574 (66.0%)** as of 2026-02-16 (post-target default + TS2585 + TS2411 fixes).

## Phase 1: High-ROI Quick Wins

### 1. TS2454 — "Variable is used before being assigned" (target: ~241 tests)
- **Status**: MOSTLY DONE (51 missing, 42 extra remain)
- **Root causes fixed**:
  1. `should_check_definite_assignment()` only checked `BLOCK_SCOPED_VARIABLE` — fixed to use `VARIABLE` (includes `var`)
  2. Conformance runner defaulted `strict: false` but TS 6.0 defaults `strict: true` — fixed default
- **Remaining**: 51 missing (complex control flow), 42 extra (over-reporting)
- **Files**: `crates/tsz-checker/src/flow_analysis.rs`, `crates/conformance/src/tsz_wrapper.rs`

### 2. TS2580 — "Cannot find name 'require'" false positive (target: ~147 tests)
- **Status**: DONE (+82 tests)
- **Fixes**: Suppressed TS2580 in JS files (implicitly CommonJS), changed to TS2591 for tsconfig contexts
- **Files**: `crates/tsz-checker/src/type_computation_complex.rs`, `crates/tsz-checker/src/error_reporter.rs`

### 3. Target default + TS2585 + TS2411 (target: ~62 tests)
- **Status**: DONE (+62 tests)
- **Root causes fixed**:
  1. Conformance runner defaulted `target: es5` but tsc 6.0 defaults to ES2025 (our ES2022)
  2. TS2585 guards checked target version instead of checking lib value availability
  3. Index signature check skipped GET_ACCESSOR/SET_ACCESSOR members
- **Files**: `crates/conformance/src/tsz_wrapper.rs`, `crates/tsz-checker/src/type_computation_complex.rs`, `crates/tsz-checker/src/state_checking_members/member_access.rs`

## Phase 2: Targeted Feature Gaps

### 3. expressions/typeGuards (target: ~42 tests)
- **Status**: NOT STARTED
- **Pass rate**: 33.3% (21/63)
- **Scope**: `typeof`, `instanceof`, user-defined type predicates narrowing

### 4. expressions/binaryOperators (target: ~50 tests)
- **Status**: NOT STARTED
- **Pass rate**: 24.2% (16/66)
- **Scope**: Operator type checking (arithmetic, comparison, logical)

### 5. override keyword (target: ~22 tests)
- **Status**: NOT STARTED
- **Pass rate**: 29.0% (9/31)
- **Scope**: `override` modifier checking in class members

### 6. interfaces/declarationMerging (target: ~20 tests)
- **Status**: NOT STARTED
- **Pass rate**: 23.1% (6/26)
- **Scope**: Binder-level interface/declaration merging

## Phase 3: Broader Coverage

### 7. controlFlow (target: ~30 tests)
- **Pass rate**: 47.4% (27/57)
- **Scope**: Narrowing, exhaustiveness, reachability

### 8. types/tuple + types/union (target: ~40 tests)
- **Pass rates**: 35.3%, 28.0%
- **Scope**: Core type system operations

### 9. es6/destructuring (target: ~54 tests)
- **Pass rate**: 63.3% (93/147)
- **Scope**: Edge cases in nested destructuring, defaults, rest

---

## Progress Log

| Date | Change | Tests Flipped | New Total |
|------|--------|---------------|-----------|
| 2026-02-15 | Baseline (pre-TS6 defaults) | — | 7883/12574 (62.7%) |
| 2026-02-15 | TS2580 fix (JS files, TS2591) | +82 | 7528→7610 (60.5%*) |
| 2026-02-15 | TS2454 var + TS6 strict default | +676 | 8204/12574 (65.2%) |
| 2026-02-16 | Target default es5→es2022 (tsc 6.0 LatestStandard) | +37 | 8238→8275 |
| 2026-02-16 | TS2585 false positive removal (target-based → lib-based) | +14 | 8275→8289 |
| 2026-02-16 | TS2411 accessor index sig check (GET/SET_ACCESSOR) | +11 | 8289→8300 (66.0%) |
