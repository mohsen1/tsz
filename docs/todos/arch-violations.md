# Architecture Audit Report

**Date**: 2026-02-21 (7th audit)
**Branch**: main (commit 9c7e23019)
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

### 2. Solver Imports in Binder — CLEAN

The binder crate (`tsz-binder`) depends only on `tsz-common`, `tsz-scanner`, and `tsz-parser`. Zero imports of solver or checker types found. No `TypeId`, `TypeData`, `TypeInterner`, or solver module references in any binder source file.

### 3. Checker File Sizes — COMPLIANT (11 near-threshold files)

All checker files are under the 2000-line limit. Eleven files are approaching the threshold and need monitoring:

| File | Lines | Headroom |
|------|-------|----------|
| `state/state_class_checking.rs` | 1,995 | 5 lines |
| `state_checking_members/member_declaration_checks.rs` | 1,994 | 6 lines |
| `types/type_computation_call.rs` | 1,994 | 6 lines |
| `types/type_computation_access.rs` | 1,972 | 28 lines |
| `state/state_type_resolution_module.rs` | 1,908 | 92 lines |
| `types/type_checking_queries_lib.rs` | 1,901 | 99 lines |
| `flow/control_flow_narrowing.rs` | 1,883 | 117 lines |
| `types/type_computation.rs` | 1,882 | 118 lines |
| `flow/control_flow_assignment.rs` | 1,837 | 163 lines |
| `context.rs` | 1,830 | 170 lines |
| `types/class_type.rs` | 1,803 | 197 lines |

Total checker codebase: ~127 files, ~106,405 LOC.

No change from 6th audit — no new code has been added since then.

### 4. Cross-Layer Imports — CLEAN

- **Emitter -> Checker**: No `tsz_checker` imports in emitter. Emitter depends on parser, binder, solver only.
- **Binder -> Solver**: No solver dependencies in binder (see finding #2).
- **Checker -> Solver internals**: No raw `TypeData::` constructions or direct `intern()` calls in checker code. Checker uses public solver API constructors and query boundary helpers.
- **CLI -> Checker internals**: CLI and LSP crates import only public checker exports.
- **Solver -> Parser/Checker**: No upward imports. Solver is a pure type system layer.
- **Lowering -> Checker**: No checker dependency. Lowering bridges AST and Solver only.

**Note on TypeInterner usage**: LSP, CLI, and Emitter import `TypeInterner` from `tsz-solver`. This is **expected architecture** — `TypeInterner` is the public read-only type store. What's forbidden is importing `TypeData` variants or performing direct type construction, not read-only type store access.

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

Latest CI run (9c7e23019) completed successfully. Three older runs remain in-progress (likely stuck/slow runners).

| Run | Status | Description |
|-----|--------|-------------|
| 22264708570 | completed/success | docs(arch): 6th architecture audit |
| 22264633879 | completed/success | docs(arch): 5th architecture audit |
| 22264560766 | completed/success | docs: automated README metrics update |
| 22264546925 | in_progress | docs(arch): update audit report and fix stale TypeKeys comment |
| 22264518667 | in_progress | perf(checker): cache lib type-name resolution results |
| 22264427753 | in_progress | refactor(arch): move collect_infer_bindings from tsz-lowering to solver |

---

## Recommendations

1. **Split near-limit checker files**: The top 3 files at 1,994-1,995 lines will breach the 2,000-line limit on the next feature addition. Proactively split them before adding new code:
   - `state_class_checking.rs` (1,995 lines) — consider extracting class heritage/implements checking
   - `member_declaration_checks.rs` (1,994 lines) — consider extracting method signature validation
   - `type_computation_call.rs` (1,994 lines) — consider extracting overload resolution logic
2. **Monitor 8 additional near-threshold files** in the 1,803-1,972 range for growth.
3. **Investigate stuck CI runs**: Three runs (22264546925, 22264518667, 22264427753) have been in-progress for 14-22+ minutes. Consider canceling if they remain stuck.
