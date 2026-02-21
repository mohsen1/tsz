# Architecture Audit Report

**Date**: 2026-02-21 (3rd audit)
**Branch**: main (commit 7ecbb5b4a)
**Status**: ONE VIOLATION FIXED — `TypeData` traversal moved from tsz-lowering to solver

---

## Audit Scope

Checked all architecture rules from CLAUDE.md and NORTH_STAR.md:

1. TypeKey/TypeData leakage outside solver crate
2. Solver imports in binder
3. Checker files exceeding 2000 LOC
4. Forbidden cross-layer imports (emitter->checker, binder->solver, checker->solver internals, CLI->checker internals)

---

## Findings

### 1. TypeKey Leakage Outside Solver — CLEAN

No `TypeKey` type exists in the codebase (the actual internal type is `TypeData`, which is properly encapsulated). All references to "TypeKey" in scanner/parser are `SyntaxKind::TypeKeyword` (the `type` keyword token), which is unrelated. One stale comment in `state_type_analysis_cross_file.rs:311` mentions "TypeKeys" conceptually but does not use the type.

### 2. Solver Imports in Binder — CLEAN

The binder crate (`tsz-binder`) depends only on `tsz-common`, `tsz-scanner`, and `tsz-parser`. Zero imports of solver or checker types found. No `TypeId`, `TypeData`, `TypeInterner`, or solver module references in any binder source file.

### 3. Checker File Sizes — COMPLIANT (3 files near limit)

All checker files are under the 2000-line limit. Three files are within 6 lines of the threshold and need monitoring:

| File | Lines | Headroom |
|------|-------|----------|
| `state/state_class_checking.rs` | 1,995 | 5 lines |
| `types/type_computation_call.rs` | 1,994 | 6 lines |
| `state_checking_members/member_declaration_checks.rs` | 1,994 | 6 lines |
| `types/type_computation_access.rs` | 1,972 | 28 lines |
| `state/state_type_resolution_module.rs` | 1,908 | 92 lines |

Total checker codebase: ~106,388 lines across ~143 files (avg ~744 lines/file).

Solver files are well within limits — largest is `visitors/visitor.rs` at 1,945 lines (no file over 2,000).

### 4. Cross-Layer Imports — CLEAN

- **Emitter -> Checker**: No `tsz_checker` imports in emitter. Emitter depends on parser, binder, solver only.
- **Binder -> Solver**: No solver dependencies in binder (see finding #2).
- **Checker -> Solver internals**: No raw `TypeData::` constructions or direct `intern()` calls in checker code. Checker uses public solver API constructors and query boundary helpers.
- **CLI -> Checker internals**: CLI and LSP crates import only public checker exports.
- **Solver -> Parser/Checker**: No upward imports. Solver is a pure type system layer.

### 5. TypeData Pattern Matching in tsz-lowering — FIXED

**Prior state**: `tsz-lowering/src/lower_advanced.rs` contained a 170-line `collect_infer_bindings` method that manually pattern-matched on 20+ `TypeData` variants for deep type-graph traversal. This violated the architecture rule: "Use visitor helpers for type traversal; avoid repeated TypeKey matching."

**Fix applied**: Moved `collect_infer_bindings` into `tsz-solver/src/visitors/visitor_extract.rs` as a solver-owned utility function. The lowering crate now calls `tsz_solver::collect_infer_bindings(interner, type_id)` and processes the returned `Vec<(Atom, TypeId)>`. This:
- Removes `TypeData` import from tsz-lowering entirely
- Consolidates type-graph traversal in the solver (WHAT)
- Keeps tsz-lowering as a thin AST→type bridge (WHO/WHERE)

### 6. TS2322 Routing — COMPLIANT

- `CompatChecker` is only instantiated from `query_boundaries/call_checker.rs` and `query_boundaries/assignability.rs`
- No direct `CompatChecker` calls in checker feature modules
- Assignability checks route through the centralized gateway

---

## CI Health

| Run | Status | Description |
|-----|--------|-------------|
| 22263919551 | in_progress | fix(checker): stop appending elaboration to TS2345 |
| 22263843318 | in_progress | fix(checker): stop appending elaboration to TS2322 |
| 22263649318 | success | fix: 6 bugs found in self-review |
| 22263618029 | success | harden run-session.sh |
| 22263607291 | in_progress | Fix node_modules package exports |

No red/failed CI runs. Latest completed runs are green.

---

## Pre-existing Test Failures (4)

These 4 tests fail on main before and after the refactoring (not related to architecture):

1. `conformance_issues::test_narrowing_after_never_returning_function`
2. `control_flow_tests::test_loop_label_returns_declared_type`
3. `spread_rest_tests::test_spread_in_function_call_with_wrong_types`
4. `ts2322_tests::test_ts2322_type_query_in_type_assertion_uses_flow_narrowed_property_type`

---

## Recommendations

1. **Split near-limit checker files**: The 3 files at 1,994-1,995 lines will breach the 2,000-line limit on the next feature addition. Proactively split them before adding new code.
2. **Stale comment**: Consider removing the "TypeKeys" reference in `state_type_analysis_cross_file.rs:311`.
3. **Investigate pre-existing test failures**: The 4 failing tests may indicate real conformance gaps worth prioritizing.
