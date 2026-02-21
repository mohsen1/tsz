# Architecture Audit Report

**Date**: 2026-02-21 (10th audit)
**Branch**: main (commit b81760973)
**Status**: ALL CLEAR — no violations found

---

## Audit Scope

Checked all architecture rules from CLAUDE.md and NORTH_STAR.md:

1. TypeKey/TypeData leakage outside solver crate
2. Solver imports in binder
3. Checker files exceeding 2000 LOC
4. Forbidden cross-layer imports (emitter->checker, binder->solver, checker->solver internals, CLI->checker internals)
5. Checker pattern-matching on low-level type internals
6. TS2322 routing compliance

---

## Findings

### 1. TypeKey/TypeData Leakage Outside Solver — CLEAN

No `TypeKey` or `TypeData` type imports found outside the solver crate. All references to "TypeKey" in scanner/parser are `SyntaxKind::TypeKeyword` (the `type` keyword token), which is unrelated.

- No `TypeData` imports in checker, binder, emitter, LSP, or CLI code
- No direct pattern matching on `TypeData` variants outside solver
- Checker uses only public solver API: `TypeId`, `DefId`, `TypeFormatter`, `QueryDatabase`, `TypeEnvironment`, `Judge`, etc.
- 25 `TypeData` mentions in checker are all in comments/documentation (architectural notes), not actual code usage

### 2. Solver Imports in Binder — CLEAN

The binder crate (`tsz-binder`) depends only on `tsz-common`, `tsz-scanner`, and `tsz-parser`. Zero imports of solver or checker types found. No `TypeId`, `TypeData`, `TypeInterner`, or solver module references in any binder source file.

### 3. Checker File Sizes — COMPLIANT (11 near-threshold files)

All checker files are under the 2000-line limit. Eleven files are approaching the threshold and need monitoring:

| File | Lines | Headroom |
|------|-------|----------|
| `state/state_class_checking.rs` | 1,995 | 5 lines |
| `types/type_computation_call.rs` | 1,994 | 6 lines |
| `state_checking_members/member_declaration_checks.rs` | 1,994 | 6 lines |
| `types/type_computation_access.rs` | 1,972 | 28 lines |
| `state/state_type_resolution_module.rs` | 1,908 | 92 lines |
| `types/type_checking_queries_lib.rs` | 1,901 | 99 lines |
| `flow/control_flow_narrowing.rs` | 1,883 | 117 lines |
| `types/type_computation.rs` | 1,882 | 118 lines |
| `flow/control_flow_assignment.rs` | 1,837 | 163 lines |
| `context.rs` | 1,830 | 170 lines |
| `types/class_type.rs` | 1,803 | 197 lines |

Total checker codebase: ~148 files, ~106,525 LOC.

**Note on perf commit (b81760973)**: The `type_checking_queries_lib.rs` file stayed at 1,901 lines — the perf commit extracted new logic into a separate `type_checking_queries_lib_prime.rs` (113 lines), keeping the near-threshold file from growing. Good architectural practice.

### 4. Cross-Layer Imports — CLEAN

- **Emitter -> Checker**: No `tsz_checker` imports in emitter. Emitter depends on parser, binder, solver only.
- **Binder -> Solver**: No solver dependencies in binder (see finding #2).
- **Checker -> Solver internals**: No raw `TypeData::` constructions or direct `intern()` calls in checker code. Checker uses public solver API constructors and query boundary helpers.
- **CLI -> Checker internals**: CLI and LSP crates import only public checker exports.
- **Solver -> Parser/Checker**: No upward imports. Solver is a pure type system layer.
- **Lowering -> Checker**: No checker dependency. Lowering bridges AST and Solver only.

**Note on TypeInterner usage**: LSP, CLI, and Emitter import `TypeInterner` from `tsz-solver`. This is **expected architecture** — `TypeInterner` is the public read-only type store. What's forbidden is importing `TypeData` variants or performing direct type construction, not read-only type store access.

**Note on Solver -> Binder (`SymbolId`)**: The solver crate depends on `tsz-binder` for the `SymbolId` identity handle. This is **by design** — `SymbolId(u32)` is a shared identity handle (CLAUDE.md §7) required for type variants like `TypeQuery(SymbolRef)` and `UniqueSymbol(SymbolRef)`. The forbidden pattern is binder importing solver for *semantic decisions*, not shared identity handles flowing between layers.

### 5. Checker Type Internals Pattern-Matching — CLEAN

- All type shape inspection delegated to solver queries via query boundaries
- Type traversal properly routed through `tsz_solver::visitor::` helpers (`collect_lazy_def_ids`, `collect_type_queries`, `collect_referenced_types`, `collect_enum_def_ids`, `is_template_literal_type`)
- Architecture contract tests in `architecture_contract_tests.rs` actively prevent direct `TypeData::Array`, `TypeData::ReadonlyType`, `TypeData::KeyOf`, `TypeData::IndexAccess`, `TypeData::Lazy(DefId)`, and `TypeData::TypeParameter` construction in checker

### 6. TS2322 Routing — COMPLIANT

- `CompatChecker` is only instantiated from `query_boundaries/call_checker.rs` and `query_boundaries/assignability.rs`
- No direct `CompatChecker` calls in checker feature modules
- Assignability checks route through the centralized gateway
- 26 query_boundaries modules properly gate all solver access

### 7. Previously Fixed: TypeData Traversal in tsz-lowering — REMAINS FIXED

The `collect_infer_bindings` method was moved from `tsz-lowering` into `tsz-solver/src/visitors/visitor_extract.rs` in commit f5aa685e7. The lowering crate now calls the solver-owned utility. No regression.

---

## CI Health

Latest CI run (b81760973) in progress. All completed runs green.

| Run | Status | Description |
|-----|--------|-------------|
| 22264898602 | completed/success | docs(arch): 9th architecture audit |
| 22264859539 | in_progress | perf(checker): avoid full lib lowering when priming generic params |
| 22264836885 | completed/success | docs(arch): 8th architecture audit |
| 22264779796 | completed/success | docs(arch): 7th architecture audit |
| 22264708570 | completed/success | docs(arch): 6th architecture audit |

---

## Recommendations

1. **Split near-limit checker files**: The top 3 files at 1,994-1,995 lines will breach the 2,000-line limit on the next feature addition. Proactively split them before adding new code:
   - `state_class_checking.rs` (1,995 lines) — consider extracting class heritage/implements checking
   - `member_declaration_checks.rs` (1,994 lines) — consider extracting method signature validation
   - `type_computation_call.rs` (1,994 lines) — consider extracting overload resolution logic
2. **Monitor 8 additional near-threshold files** in the 1,803-1,972 range for growth.
