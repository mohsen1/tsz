# Architectural Plan: Hard Conformance Problems

This document outlines the strategic approach to solving the remaining conformance gaps in the TSZ
compiler. Updated 2026-03-05 with current implementation status and data-driven prioritization.

**Current score**: 10,025 / 12,570 (79.8%)
**Remaining**: 2,541 failing tests

---

## Implementation Status of Original Plan

### 1. `TypeId::ERROR` — COMPLETE ✓

The error type is fully implemented as a first-class type primitive at index 1 in the solver.

- **Assignability**: bi-directionally assignable to/from everything (like `ANY`)
- **Property access**: returns `PropertyResult::IsError` silently
- **Diagnostic suppression**: checker checks for `ERROR` before emitting TS18046 and other diagnostics
- **Propagation**: contagious through evaluation, narrowing, and operations

**Key files**: `tsz-solver/src/types.rs`, `relations/judge.rs`, `relations/subtype/rules/intrinsics.rs`

### 2. Cross-File SymbolId — PARTIALLY IMPLEMENTED ⚠️

Addressed via runtime `cross_file_symbol_targets` map rather than the proposed `QualifiedSymbolId` struct.

- **`CheckerContext.cross_file_symbol_targets`**: `FxHashMap<SymbolId, usize>` mapping symbols to source
  file indices, populated during cross-file export resolution
- **`get_cross_file_symbol()`**: checks the map FIRST to find correct file, avoiding collision
- **`delegate_cross_arena_symbol_resolution()`**: creates child checkers with correct arena

**Remaining risk**: The runtime map approach handles known cross-file paths but may miss edge cases
where symbols cross file boundaries through unexpected paths. The original `QualifiedSymbolId` proposal
would provide compile-time guarantees. Consider adopting if cross-file symbol bugs persist.

**Key files**: `tsz-checker/src/context/mod.rs`, `tsz-checker/src/cross_file.rs`

### 3. Project References / `--build` — PARTIALLY IMPLEMENTED ⚠️

- **Phase 1 (Config Parsing)**: ✓ Complete — `TsConfigWithReferences`, `composite`, `references` supported
- **Phase 2 (Graph Construction)**: ✓ Complete — `ProjectReferenceGraph` with BFS loading, cycle detection
- **Phase 3 (Build Orchestration)**: ⚠️ Partial — up-to-date checking implemented (`is_project_up_to_date`),
  but full multi-project topological compilation may be incomplete

**Key files**: `tsz-cli/src/driver/project_refs.rs`, `tsz-cli/src/driver/build.rs`

---

## Current Conformance Landscape

### Failure Breakdown (2,541 tests)

| Category | Count | % | Description |
|----------|------:|--:|-------------|
| Type-only diff | 1,531 | 60% | Core type system — inference, assignability, narrowing |
| All missing | 439 | 17% | We emit 0 errors, tsc expects some |
| False positives | 253 | 10% | We emit errors, tsc expects none |
| Mixed parser+type | 185 | 7% | Both layers wrong |
| Parser-only | 133 | 5% | Scanner/parser TS1xxx differences |

### Proximity to Passing

| Diff | Tests | Cumulative |
|------|------:|-----------|
| 1 (single code off) | 911 (35.9%) | — |
| 2 (two codes off) | 477 (18.8%) | 1,388 (54.6% of failures) |

**602 tests are quick wins** (1-missing-0-extra or 0-missing-1-extra) → would reach **84.5%** if all fixed.

---

## Priority Problems (Ranked by Impact)

### P0: The Big 3 Error Codes (TS2322 / TS2339 / TS2345)

These three codes account for the majority of both false positives AND missing diagnostics:

| Code | Missing in | Extra in | Quick wins (add) | Quick wins (remove) |
|------|--------:|--------:|--------:|--------:|
| TS2322 | 100 | 145 | 48 | 59 |
| TS2339 | 87 | 127 | 43 | 47 |
| TS2345 | 56 | 98 | 37 | 40 |
| **Total** | **243** | **370** | **128** | **146** |

**Root causes** (shared across all three):
1. **Solver assignability too strict** — missing `any` propagation paths, variance mode gaps,
   intersection/union compatibility holes → false positives
2. **Narrowing gaps** — control flow analysis not narrowing through certain patterns (optional chains,
   type predicates, discriminated unions) → false negatives
3. **Generic inference failures** — contextual types not flowing through type parameters, constraint
   evaluation gaps → both directions
4. **Property resolution gaps** — module augmentation, namespace merging, computed properties
   not fully resolved → false TS2339

**Strategy**: These are NOT "implement error code X" fixes. They require solver-level root cause
analysis where a single fix affects 10-40+ tests simultaneously. Priority should be on finding
high-leverage solver changes, not chasing individual test failures.

### P1: TS5107 Deprecation Warning Misalignment (59 tests extra)

TS5107 ("Option X is deprecated") is falsely emitted in 59 tests. Root cause is in the conformance
wrapper and driver:

1. `@strict: false` expands to `alwaysStrict: false` which is deprecated in tsc 6.0
2. Our driver's TS5107 suppression logic (drop TS5107 when "reliable grammar errors" exist) is
   misaligned with tsc's behavior
3. Fix: align the TS5107 suppression heuristic in `driver/core.rs` with tsc 6.0's actual behavior

**Impact**: Single fix could recover up to 59 tests.

### P2: Parser Recovery Diagnostic Selection (133 parser-only failures)

| Code | Missing | Clean Quick Wins | Root Issue |
|------|---------|------------------|------------|
| TS1005 (`'X' expected`) | 80 | 33 | Over-eager catch-all; emitted instead of context-specific codes |
| TS1128 (`Declaration expected`) | 37 | 2 | TS1005 emitted instead at wrong recovery level |
| TS1109 (`Expression expected`) | 36 | 14 | Missing in async/arrow/default param contexts |
| TS1434 (`Duplicate modifier`) | 27 | 7 | Duplicate modifier detection not triggered |

**Key insight**: TS1005 is the root problem. It's used as a catch-all in import/export/class member
error recovery. Fixing TS1005's diagnostic selection would cascade-fix many TS1128/TS1434 cases.

### P3: Unimplemented Diagnostic Codes (batch implementation)

These codes are never emitted by tsz and affect multiple tests:

| Code | Tests | Description |
|------|------:|-------------|
| TS2323 | 8 | Cannot redeclare exported variable |
| TS7017 | 6 | Element implicitly has 'any' type (index signature) |
| TS2742 | 5 | Module augmentation inferred type |
| TS2550 | 5 | Property does not exist (did you mean?) |
| TS17019 | 5 | Rest tuple optional element |
| TS1181 | 5 | Nested mapped type |
| TS2657 | 5 | JSX member expression |
| TS17020 | 4 | Rest tuple labeled element |
| TS2833 | 4 | typeof import |
| TS7014 | 4 | Construct signature |

Implementing these from scratch is lower-risk than modifying Big 3 behavior and provides
guaranteed test gains.

### P4: False Positive Heavy Hitters

| Code | False Positive Tests | Description |
|------|--------------------:|-------------|
| TS2322 | 66 | Type not assignable (solver too strict) |
| TS2345 | 47 | Arg not assignable (same root) |
| TS2339 | 40 | Property doesn't exist (resolution gaps) |
| TS7006 | 20 | Implicit any (contextual type not flowing) |
| TS2769 | 8 | No overload matches |
| TS2307 | 7 | Module not found |
| TS7053 | 6 | Element has implicit any (index) |
| TS2693 | 6 | Type used as value |
| TS2365 | 6 | Operator cannot be applied |

### P5: Lowest Pass Rate Areas

| Pass Rate | Failed | Area | Key Issues |
|-----------|-------:|------|------------|
| 57.7% | 11 | types/mapped | Mapped type evaluation, template literal keys |
| 68.2% | 14 | types/literal | Literal narrowing, template literal inference |
| 68.2% | 20 | expressions/typeGuards | CFA narrowing through assignments |
| 68.3% | 79 | jsdoc | @constructor, @typedef scope, @callback |
| 68.4% | 18 | controlFlow | Narrowing, unreachable code detection |
| 68.7% | 61 | jsx | Class component props, children type |
| 69.5% | 58 | salsa | JS prototype patterns, constructor synthesis |
| 70.0% | 59 | classes/members | Override checks, heritage resolution |

---

## Recommended Order of Operations

### Phase 1: Low-Risk High-Return (target: 82-84%)

1. **Fix TS5107 suppression** (P1) — single conformance wrapper / driver fix, up to +59 tests
2. **Implement missing codes** (P3) — TS2323, TS7017, TS2742, etc., ~30-40 tests
3. **Parser TS1005 recovery** (P2) — fix diagnostic selection, ~30-50 cascading tests

### Phase 2: Solver Depth Work (target: 85-88%)

4. **Big 3 false positive reduction** (P0) — solver assignability strictness audit
5. **Contextual type flowing** (P4/TS7006) — generic inference, callback parameter typing
6. **Narrowing improvements** (P0) — discriminated unions, optional chains, type predicates

### Phase 3: Area-Focused Campaigns (target: 88-92%)

7. **JSDoc area** (68.3%, 79 failures) — @constructor prototype, @typedef scope
8. **JSX area** (68.7%, 61 failures) — class component validation, children
9. **Salsa/JS area** (69.5%, 58 failures) — constructor synthesis, expando types
10. **Control flow** (68.4%, 18 failures) — narrowing edge cases

### Phase 4: Long Tail (target: 92%+)

11. **Mapped types** (57.7%) — deep solver evaluation
12. **Cross-file patterns** — module augmentation, namespace merging
13. **Generic inference** — variadic tuples, conditional types, higher-order patterns
14. **Project references** — complete Phase 3 build orchestration

---

## Architectural Principles for Remaining Work

1. **Favor solver-level fixes over checker heuristics** — a single solver change that corrects
   assignability for a pattern class affects dozens of tests simultaneously
2. **Avoid ad-hoc suppressions** — each false positive should be traced to the root solver/binder
   cause rather than adding checker-level exception paths
3. **Measure before and after** — always run `./scripts/conformance.sh run --filter "pattern"` to
   verify fixes don't introduce regressions before running the full suite
4. **Use offline analysis first** — `python3 scripts/query-conformance.py` provides instant analysis
   without running the suite; reserve full runs for verification only
5. **Batch related fixes** — group fixes by root cause (e.g., "narrowing through optional chains"
   affects multiple error codes) rather than by individual error code
