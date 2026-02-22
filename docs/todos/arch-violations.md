# Architecture Audit Report

**Date**: 2026-02-21 (11th audit)
**Branch**: main (commit 84ed883ec)
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
| ~~`state/state_type_resolution_module.rs`~~ | ~~1,908~~ | ✅ Split — extracted constructor/class type ops (~767 LOC) into `state_type_resolution_constructors.rs`, reducing to ~1,140 LOC |
| `types/type_checking_queries_lib.rs` | 1,901 | 99 lines |
| `flow/control_flow_narrowing.rs` | 1,883 | 117 lines |
| `types/type_computation.rs` | 1,882 | 118 lines |
| `flow/control_flow_assignment.rs` | 1,837 | 163 lines |
| ~~`context.rs`~~ | ~~1,830~~ | ✅ Split — `context/mod.rs` now 1,546 LOC (extracted `compiler_options.rs` + `lib_queries.rs`) |
| `types/class_type.rs` | 1,803 | 197 lines |

Total checker codebase: ~148 files, ~106,525 LOC.

**Note**: The `type_checking_queries_lib.rs` file remains stable at 1,901 lines. The perf commit (b81760973) extracted new logic into a separate `type_checking_queries_lib_prime.rs` (113 lines), keeping the near-threshold file from growing. Good architectural practice.

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

All recent CI runs green. One perf commit (b81760973) run still in progress at time of audit.

| Run | Status | Description |
|-----|--------|-------------|
| 22264969121 | completed/success | docs(arch): 10th architecture audit |
| 22264898602 | completed/success | docs(arch): 9th architecture audit |
| 22264859539 | in_progress | perf(checker): avoid full lib lowering when priming generic params |
| 22264836885 | completed/success | docs(arch): 8th architecture audit |
| 22264779796 | completed/success | docs(arch): 7th architecture audit |

---

## Recommendations

1. **Monitor near-limit checker files** for growth. Top files by line count:
   - ~~`type_checking_queries_lib.rs` (1,901 lines)~~ ✅ Split — extracted type-only detection methods (~590 LOC) into `type_checking_queries_type_only.rs`, reducing to 1,313 LOC
   - ~~`control_flow_narrowing.rs` (1,883 lines)~~ ✅ Split — extracted reference matching, literal parsing, and symbol resolution (~680 LOC) into `control_flow_references.rs`, reducing to 1,204 LOC
   - `control_flow_assignment.rs` (1,837 lines)
   - `class_type.rs` (1,818 lines)
   - `type_checking_utilities.rs` (1,778 lines)
   - ~~`assignability_checker.rs` (1,447 lines)~~ ✅ Split — extracted subtype/identity/compat methods (~273 LOC) into `subtype_identity_checker.rs`, reducing to 1,176 LOC
2. **Monitor near-threshold files** in the 1,700-1,900 range for growth.
3. **Solver top-level file sprawl**: Remaining file families to organize into subdirectories (following the pattern now established by `narrowing/`, `relations/`, `evaluation/`, `inference/`, `instantiation/`, `visitors/`, `caches/`, `operations/`):
   - ~~`operations_*.rs` (10 files, 7,863 LOC) → `operations/` subdirectory~~ ✅ Done (c3365ed0d)
   - ~~`type_queries_*.rs` (5 files) → `type_queries/` subdirectory~~ ✅ Done
   - ~~`intern_*.rs` (4 files: `intern.rs`, `intern_normalize.rs`, `intern_intersection.rs`, `intern_template.rs`) → `intern/` subdirectory~~ ✅ Done
   - ~~Re-export shim files (7 files: `compat.rs`, `db.rs`, `evaluate.rs`, `infer.rs`, `instantiate.rs`, `query_trace.rs`, `subtype.rs`) removed~~ ✅ Done (a727b3b8b) — internal imports updated to direct module paths. Two externally-used shims (`judge.rs`, `visitor.rs`) retained.
4. ~~**Checker `context*.rs` files**: organized into `context/` subdirectory~~ ✅ Done
5. ~~**Solver `type_queries/extended.rs`** (1,915 LOC): approaching 2000-line limit~~ ✅ Done — extracted constructor/class/instance classifiers (~482 LOC) into `extended_constructors.rs`, reducing `extended.rs` to ~1,442 LOC.
6. **Solver `type_queries/mod.rs`** reduced from 1,947 → 1,744 LOC by extracting iterable classifications into `iterable.rs`. Still contains traversal, property lookup, evaluation, signature, and constraint sections that could be further split if growth continues.
7. **Solver `visitors/visitor.rs`** reduced from 1,945 → ~1,130 LOC by extracting type predicates (`is_*`, `contains_*`, `classify_*`, `ObjectTypeKind`) and their internal helper structs into `visitor_predicates.rs` (~585 LOC). The `ConstAssertionVisitor` (~178 LOC) remains in `visitor.rs` but could be extracted if the file grows again.
