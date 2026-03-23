# Roadmap to 100% Conformance

**Current: 88.6% (11,143 / 12,581) — 1,438 tests failing**

> Last updated: March 2026. Phase 1 is largely complete.

## Failure Breakdown

| Category | Count | Description |
|----------|-------|-------------|
| Wrong codes | 597 | We emit errors but wrong codes (e.g., TS2345 instead of TS2322) |
| Fingerprint-only | 617 | Right codes but wrong line/column/message |
| All missing | 249 | tsc expects errors, we emit 0 |
| False positives | 121 | tsc expects 0 errors, we emit errors |
| Close (diff<=2) | 783 | One or two diagnostics away from passing |

## The "Big 3" Problem

TS2322/TS2339/TS2345 dominate **both** missing and extra lists:

| Code | Missing From | Falsely Emitted In | Quick-Win (add 1) | Quick-Win (remove 1) |
|------|-------------|-------------------|-------------------|---------------------|
| TS2322 | 59 tests | 83 tests | 22 tests | 30 tests |
| TS2339 | 48 tests | 64 tests | 19 tests | 27 tests |
| TS2345 | 38 tests | 63 tests | 22 tests | 21 tests |

This means the solver's assignability/property-resolution logic is both under-reporting
and over-reporting — indicating structural accuracy issues, not simply missing features.

---

## Phase 1: False Positive Elimination (~+120 tests → ~88.4%) — MOSTLY COMPLETE

**Target: Remove false emissions where tsc expects 0 errors but we emit errors.**
Every fix is a net gain with zero regression risk. Most false positives have been addressed
through fixes to Promise type preservation, ambient module resolution, private name access,
destructuring assignment flow narrowing, and union call parameter handling.

| Fix | Tests | Module | Difficulty |
|-----|-------|--------|-----------|
| Remove false TS2322 (assignability) | 30 | solver relations, compat checker | Medium |
| Remove false TS2339 (property) | 27 | solver property resolution, module augmentation | Medium |
| Remove false TS2345 (argument) | 21 | solver generic inference, contextual typing | Medium |
| Remove false TS7006 (implicit any) | 10 | checker contextual type propagation | Easy-Medium |
| Remove false TS2741/TS2349/TS1359 | ~12 | various | Easy |

**Root causes:**
- Over-strict relation checks in `CompatChecker`
- Missing `any` propagation in `AnyPropagationRules`
- Conditional type assignability gaps
- Module augmentation merging failures in binder
- Contextual type not reaching callback parameters

## Phase 2: Missing Diagnostic Additions (~+100 tests → ~89.2%)

**Target: Add single missing diagnostics where we currently emit nothing.**

| Fix | Tests | Module | Difficulty |
|-----|-------|--------|-----------|
| Add missing TS2322 | 22 | solver relation failure routing | Medium |
| Add missing TS2345 | 22 | solver argument checking | Medium |
| Add missing TS2339 | 19 | solver property lookup on unions | Medium |
| Add missing TS2307 | 10 | module resolution | Easy |
| Add missing TS7006 | 9 | contextual typing | Easy-Medium |
| Add missing TS2300 | 7 | duplicate identifier checking | Easy |
| Add missing TS2304 | 7 | name resolution / global scope | Easy-Medium |
| Add missing TS2344 | 6 | type constraint validation | Medium |

## Phase 3: Big 3 Diagnostic Selection Fix (~+150 tests → ~90.4%)

**Target: Fix the TS2322↔TS2345 swap pattern in the assignability gateway.**

The "swap" pattern — where we emit TS2345 instead of TS2322 or vice versa — accounts
for hundreds of wrong-code failures. Per CLAUDE.md, TS2322/TS2345/TS2416 paths must use
ONE compatibility gateway via `query_boundaries`.

| Fix | Tests | Module | Difficulty |
|-----|-------|--------|-----------|
| Fix TS2322/TS2345 diagnostic selection | ~80 | query_boundaries assignability gateway | Hard |
| Fix TS2339 on intersection/union members | ~30 | solver property visitors | Medium-Hard |
| Fix overload resolution TS2769 | ~16 | solver overload selection | Hard |

## Phase 4: Parser Recovery (~+70 tests → ~91.0%)

**Target: Fix parser error code selection and cascade prevention.**

| Fix | Tests | Module | Difficulty |
|-----|-------|--------|-----------|
| Fix TS1005/TS1109/TS1128 code selection | ~50 | tsz-parser error recovery | Easy |
| Fix import/export attribute recovery | ~15 | tsz-parser | Easy |
| Fix class member recovery cascade | ~10 | tsz-parser | Easy-Medium |

Parser fixes are self-contained and don't affect the type system.

## Phase 5: Unimplemented Diagnostics (~+60 tests → ~91.5%)

**Target: Implement diagnostic codes we never emit.**

| Code | Description | Tests | Difficulty |
|------|-------------|-------|-----------|
| TS7023 | Expression too complex | 9 | Medium |
| TS2318 | Cannot find global type | 7 | Easy |
| TS7017 | Implicit any (non-number index) | 6 | Medium |
| TS2394 | Overload not compatible with implementation | 6 | Hard |
| TS1181 | Labeled statement not valid target | 5 | Easy |
| TS2657 | JSX must have one parent | 5 | Easy |
| TS2565 | Property not initialized in constructor | 4 | Medium |
| TS2819 | Cannot use `in` on primitive | 4 | Easy |
| TS2417 | Class incorrectly implements interface | 3 | Medium |
| Others (20 codes) | Various | ~20 | Mixed |

## Phase 6: Narrowing & Control Flow (~+80 tests → ~92.1%)

**Target: Fix narrowing gaps across destructuring, optional chains, type predicates.**

| Fix | Tests | Module | Difficulty |
|-----|-------|--------|-----------|
| Narrowing through destructured aliases | ~20 | checker flow-node traversal | Hard |
| Type predicate across assignments | ~15 | solver predicate narrowing | Hard |
| Optional chain narrowing completeness | ~15 | checker CFA | Medium-Hard |
| Fix false TS18048 (possibly undefined) | ~12 | solver narrowing | Medium |
| Fix missing TS7022 (circular inference) | ~12 | checker inference | Hard |

## Phase 7: Contextual Typing & Inference (~+60 tests → ~92.6%)

**Target: Fix contextual type propagation failures.**

| Fix | Tests | Module | Difficulty |
|-----|-------|--------|-----------|
| Contextual typing to callback params | ~24 | solver contextual extraction | Medium-Hard |
| Contextual typing to JSX attributes | ~15 | checker JSX, solver | Medium |
| Generic inference at call sites | ~15 | solver constraint generation | Hard |
| Correlated union parameter checking | ~10 | solver relation engine | Hard |

## Phase 8: JSX / JSDoc / Salsa (~+100 tests → ~93.4%)

**Target: Feature-specific fixes for JS ecosystem.**

| Area | Failures | Key Issues | Difficulty |
|------|----------|-----------|-----------|
| JSX (64 failing) | ~30 fixable | Missing TS2786, TS2657 | Medium |
| JSDoc (68 failing) | ~30 fixable | Type-tag resolution, @returns, @template | Medium |
| Salsa (59 failing) | ~25 fixable | JS constructor property merging | Hard |
| Node modules (24 failing) | ~15 fixable | CommonJS/ESM resolution | Medium |

## Phase 9: Fingerprint-Only Failures (~+400 tests → ~96.6%)

617 tests have the right error codes but wrong line, column, or message text.
These require per-test investigation after code-level fixes stabilize.

- **Wrong line/column**: Diagnostic anchored to wrong AST node (~300 tests)
- **Wrong message text**: Different type formatting in error messages (~200 tests)
- **Wrong file**: Multi-file test error in wrong file (~50 tests)
- **Off-by-one**: Source position calculation issues (~67 tests)

## Phase 10: Long Tail (~+430 tests → 100%)

The final ~430 tests require deep investigation of edge cases, complex interactions
between features, and rare TypeScript patterns.

---

## Impact by Module

| Module | Fixes Needed | Impact |
|--------|-------------|--------|
| **tsz-solver** (relations, evaluation, property resolution) | Major | ~400 tests |
| **tsz-checker** (assignability gateway, contextual typing, flow) | Major | ~350 tests |
| **tsz-parser** (error recovery) | Moderate | ~70 tests |
| **tsz-binder** (module augmentation, symbol merging) | Moderate | ~40 tests |
| **tsz-checker** (diagnostic location/formatting) | Large volume | ~617 tests |

## Hot Files

1. `solver/src/relations/` — subtype, compat checker
2. `checker/src/query_boundaries/assignability.rs` — the central gateway
3. `solver/src/operations/property.rs` — property resolution
4. `solver/src/contextual/` — contextual type extraction
5. `parser/src/` — error recovery branches
6. `checker/src/state/type_environment/` — lazy resolution, cache management
7. `solver/src/narrowing/` — flow-sensitive type narrowing

## Worst Areas by Pass Rate

| Pass Rate | Failures | Area |
|-----------|----------|------|
| 54.5% | 5 | expressions/assignmentOperator |
| 65.9% | 15 | types/literal |
| 66.7% | 8 | types/intersection |
| 67.0% | 64 | jsx |
| 67.1% | 24 | node |
| 68.0% | 8 | types/union |
| 69.1% | 59 | salsa |
| 70.6% | 10 | types/tuple |
| 73.1% | 7 | types/mapped |

## Estimated Timeline

| Phase | Tests Fixed | Cumulative | Rate |
|-------|-----------|------------|------|
| Phase 1 | +120 | 88.4% | Quick wins |
| Phase 2 | +100 | 89.2% | Quick wins |
| Phase 3 | +150 | 90.4% | Core fixes |
| Phase 4 | +70 | 91.0% | Parser |
| Phase 5 | +60 | 91.5% | New diagnostics |
| Phase 6 | +80 | 92.1% | Narrowing |
| Phase 7 | +60 | 92.6% | Inference |
| Phase 8 | +100 | 93.4% | JSX/JSDoc/Salsa |
| Phase 9 | +400 | 96.6% | Fingerprints |
| Phase 10 | +430 | 100% | Long tail |
