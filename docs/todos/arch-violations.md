# Architecture Audit Report

**Date**: 2026-02-21 (4th audit)
**Branch**: main (commit f5aa685e7)
**Status**: ALL CLEAR — no new violations found

---

## Audit Scope

Checked all architecture rules from CLAUDE.md and NORTH_STAR.md:

1. TypeKey/TypeData leakage outside solver crate
2. Solver imports in binder
3. Checker files exceeding 2000 LOC
4. Forbidden cross-layer imports (emitter->checker, binder->solver, checker->solver internals, CLI->checker internals)

---

## Findings

### 1. TypeKey/TypeData Leakage Outside Solver — CLEAN

No `TypeKey` type exists in the codebase (the actual internal type is `TypeData`, which is properly encapsulated within `tsz-solver`). All references to "TypeKey" in scanner/parser are `SyntaxKind::TypeKeyword` (the `type` keyword token), which is unrelated.

- No `TypeData` imports in checker, binder, emitter, LSP, or CLI code
- No direct pattern matching on `TypeData` variants outside solver
- One stale comment in `state_type_analysis_cross_file.rs:311` mentions "TypeKeys" conceptually but does not use the type (cosmetic issue only)

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

Total checker codebase: ~106,388 lines across ~141 files (avg ~754 lines/file).

### 4. Cross-Layer Imports — CLEAN

- **Emitter -> Checker**: No `tsz_checker` imports in emitter. Emitter depends on parser, binder, solver only.
- **Binder -> Solver**: No solver dependencies in binder (see finding #2).
- **Checker -> Solver internals**: No raw `TypeData::` constructions or direct `intern()` calls in checker code. Checker uses public solver API constructors and query boundary helpers.
- **CLI -> Checker internals**: CLI and LSP crates import only public checker exports.
- **Solver -> Parser/Checker**: No upward imports. Solver is a pure type system layer.

**Note on TypeInterner usage**: LSP, CLI, and Emitter import `TypeInterner` from `tsz-solver`. This is **expected architecture** — `TypeInterner` is the public read-only type store. LSP owns the global type interner (per NORTH_STAR §14: "Global type interning across files"). Emitter needs read-only type access for `.d.ts` emission and type display. What's forbidden is importing `TypeData` variants or performing direct type construction, not read-only type store access.

### 5. Previously Fixed: TypeData Traversal in tsz-lowering — REMAINS FIXED

The `collect_infer_bindings` method was moved from `tsz-lowering` into `tsz-solver/src/visitors/visitor_extract.rs` in commit f5aa685e7. The lowering crate now calls the solver-owned utility. No regression.

### 6. TS2322 Routing — COMPLIANT

- `CompatChecker` is only instantiated from `query_boundaries/call_checker.rs` and `query_boundaries/assignability.rs`
- No direct `CompatChecker` calls in checker feature modules
- Assignability checks route through the centralized gateway

---

## CI Health

All 5 most recent CI runs are `in_progress` (none red/failed):

| Run | Status | Description |
|-----|--------|-------------|
| 22264427753 | in_progress | refactor(arch): move collect_infer_bindings from tsz-lowering to solver |
| 22264368508 | in_progress | docs: add emitter TODO with skipped issue patterns from analysis |
| 22264336895 | in_progress | docs(perf): note deferred profiling and type-env follow-ups |
| 22263919551 | in_progress | fix(checker): stop appending elaboration to TS2345 |
| 22263843318 | in_progress | fix(checker): stop appending elaboration to TS2322 |

---

## Recommendations

1. **Split near-limit checker files**: The 3 files at 1,994-1,995 lines will breach the 2,000-line limit on the next feature addition. Proactively split them before adding new code.
2. **Stale comment**: Consider updating the "TypeKeys" reference in `state_type_analysis_cross_file.rs:311` to use current terminology.
3. **Monitor CI**: All 5 runs are still in progress — verify they complete green.
