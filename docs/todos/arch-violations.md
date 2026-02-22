# Architecture Audit Report

**Date**: 2026-02-22 (13th audit)
**Branch**: main (commit e38a95d3f)
**Status**: REFACTOR — grouped domain-specific checker files into checkers/ subdirectory

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

### 3. Checker File Sizes — COMPLIANT (5 near-threshold files)

All checker files are under the 2000-line limit. Many previously near-threshold files have been successfully split. Current files to monitor:

| File | Lines | Headroom |
|------|-------|----------|
| `state_checking_members/ambient_signature_checks.rs` | 1,767 | 233 lines |
| `types/type_node.rs` | 1,765 | 235 lines |
| `flow/control_flow.rs` | 1,734 | 266 lines |
| `types/type_checking_utilities_jsdoc.rs` | 1,728 | 272 lines |
| `types/type_checking_queries_class.rs` | 1,713 | 287 lines |

Previously near-threshold files (all successfully split): `state_class_checking.rs` (919), `type_computation_call.rs` (796), `member_declaration_checks.rs` (1,695), `type_computation_access.rs` (889), `type_checking_queries_lib.rs` (1,313), `control_flow_narrowing.rs` (1,204), `type_computation.rs` (1,119), `control_flow_assignment.rs` (878), `class_type.rs` (1,026).

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

CI was red for ~15 runs due to emit JS baseline mismatch. Commit 117acf1a4 manually set README baseline to 67.9% (9,254) based on local results, but CI consistently measured 67.8% (9,242-9,243). Fixed by updating README baseline to 67.8% (9,243) and DTS to 35.1% (762) in commit b9db67920.

**Lesson**: Always let CI/automated scripts set baselines. Manual baseline overrides from local results can diverge from CI environment behavior (timeout sensitivity, parallelism differences).

---

## Recommendations

1. **Monitor near-limit checker files** for growth. Top files by line count:
   - ~~`type_checking_queries_lib.rs` (1,901 lines)~~ ✅ Split — extracted type-only detection methods (~590 LOC) into `type_checking_queries_type_only.rs`, reducing to 1,313 LOC
   - ~~`control_flow_narrowing.rs` (1,883 lines)~~ ✅ Split — extracted reference matching, literal parsing, and symbol resolution (~680 LOC) into `control_flow_references.rs`, reducing to 1,204 LOC
   - ~~`control_flow_assignment.rs` (1,837 lines)~~ ✅ Split — extracted condition-based narrowing (switch, binary, logical, typeof/instanceof) (~970 LOC) into `control_flow_condition_narrowing.rs`, reducing to 878 LOC
   - ~~`class_type.rs` (1,818 lines)~~ ✅ Split — extracted constructor type resolution (~811 LOC) into `class_type_constructor.rs`, reducing to 1,025 LOC
   - ~~`type_checking_utilities.rs` (1,778 lines)~~ ✅ Split — extracted return type inference (~776 LOC) into `type_checking_utilities_return.rs`, reducing to 1,002 LOC
   - ~~`assignability_checker.rs` (1,447 lines)~~ ✅ Split — extracted subtype/identity/compat methods (~273 LOC) into `subtype_identity_checker.rs`, reducing to 1,176 LOC
2. **Monitor near-threshold files** in the 1,700-1,900 range for growth.
3. **Solver top-level file sprawl**: Remaining file families to organize into subdirectories (following the pattern now established by `narrowing/`, `relations/`, `evaluation/`, `inference/`, `instantiation/`, `visitors/`, `caches/`, `operations/`):
   - ~~`operations_*.rs` (10 files, 7,863 LOC) → `operations/` subdirectory~~ ✅ Done (c3365ed0d)
   - ~~`binary_ops.rs` (970 LOC) → `operations/binary_ops.rs`~~ ✅ Done (1a0a1f886) — was at top level but logically part of operations; `operations/mod.rs` was already re-exporting from it.
   - ~~`type_queries_*.rs` (5 files) → `type_queries/` subdirectory~~ ✅ Done
   - ~~`intern_*.rs` (4 files: `intern.rs`, `intern_normalize.rs`, `intern_intersection.rs`, `intern_template.rs`) → `intern/` subdirectory~~ ✅ Done
   - ~~Re-export shim files (7 files: `compat.rs`, `db.rs`, `evaluate.rs`, `infer.rs`, `instantiate.rs`, `query_trace.rs`, `subtype.rs`) removed~~ ✅ Done (a727b3b8b) — internal imports updated to direct module paths. Two externally-used shims (`judge.rs`, `visitor.rs`) retained.
   - ~~`contextual.rs` (1,693 LOC)~~ ✅ Split and grouped — first extracted 8 TypeVisitor implementations (~909 LOC) into `contextual_extractors.rs`, then moved both files into `contextual/` subdirectory (`contextual.rs` → `contextual/mod.rs`, `contextual_extractors.rs` → `contextual/extractors.rs`). The extractors are `pub(crate)` implementation details used only by `ContextualTypeContext`.
   - ~~`freshness.rs` (37 LOC) + `variance.rs` (558 LOC) → `relations/` subdirectory~~ ✅ Done (fd881b926) — both are relation-adjacent concepts (excess property freshness, type parameter variance for assignability).
   - ~~`apparent.rs`/`objects.rs`/`object_literal.rs`/`index_signatures.rs`/`element_access.rs` (object/property-adjacent, ~1,450 LOC) → `objects/` subdirectory~~ ✅ Done (943fe7f50) — apparent→objects/apparent, objects→objects/collect, object_literal→objects/literal, index_signatures and element_access kept names.
   - ~~`class_hierarchy.rs`/`inheritance.rs` (class-adjacent, ~511 LOC total) → `classes/` subdirectory~~ ✅ Done — class type construction and nominal inheritance graph grouped together.
   - ~~`diagnostics.rs` (1,690 LOC)~~ ✅ Split — extracted eagerly-rendered builders (`DiagnosticBuilder`, `SpannedDiagnosticBuilder`, `SourceLocation`, `DiagnosticCollector`, ~664 LOC) into `diagnostics_builders.rs`, reducing `diagnostics.rs` to ~1,002 LOC. Core file now contains tracer pattern, failure reasons, lazy diagnostics, codes, and PendingDiagnosticBuilder.
   - ~~`diagnostics.rs` (1,002 LOC) + `diagnostics_builders.rs` (664 LOC) → `diagnostics/` subdirectory~~ ✅ Done (dc40a5cfd) — `diagnostics.rs` → `diagnostics/mod.rs`, `diagnostics_builders.rs` → `diagnostics/builders.rs`.
   - Remaining top-level candidates: `tracer.rs` (735 LOC); `unsoundness_audit.rs` (835 LOC — not runtime code, could move to docs).
4. ~~**Checker `context*.rs` files**: organized into `context/` subdirectory~~ ✅ Done
5. ~~**Solver `type_queries/extended.rs`** (1,915 LOC): approaching 2000-line limit~~ ✅ Done — extracted constructor/class/instance classifiers (~482 LOC) into `extended_constructors.rs`, reducing `extended.rs` to ~1,442 LOC.
6. **Solver `type_queries/mod.rs`** reduced from 1,947 → 1,744 → 1,395 LOC by extracting iterable classifications into `iterable.rs` and then traversal/property-access classifications (~355 LOC) into `traversal.rs`. Remaining sections: core type queries, intrinsic queries, composite queries, constructor/static collection, construct signatures, constraint classification, signature classification, property lookup classification, evaluation-needed classification.
7. **Solver `visitors/visitor.rs`** reduced from 1,945 → ~1,130 LOC by extracting type predicates (`is_*`, `contains_*`, `classify_*`, `ObjectTypeKind`) and their internal helper structs into `visitor_predicates.rs` (~585 LOC). The `ConstAssertionVisitor` (~178 LOC) remains in `visitor.rs` but could be extracted if the file grows again.
8. **Solver `relations/subtype.rs`** reduced from 1,899 → 1,568 LOC by extracting the caching/cycle-detection layer (`check_subtype` method, ~331 LOC) into `subtype_cache.rs`. The main file now focuses on structural dispatch (`check_subtype_inner`) while the cache file handles fast paths, memoization, coinductive cycle detection (TypeId, DefId, SymbolId levels), and pre-evaluation intrinsic checks.
9. **Binder `state_binding.rs`** reduced from 1,992 → 1,679 LOC by extracting post-binding validation, lib symbol diagnostics, and resolution statistics (~313 LOC) into `state_binding_validation.rs`.
10. **Binder `state_node_binding.rs`** reduced from 1,950 → 1,397 LOC by extracting name collection utilities and modifier helpers (~553 LOC) into `state_node_binding_names.rs`. The extracted file consolidates six identical `has_*_modifier` functions into a shared `has_modifier` helper.
   - ~~**Checker `ambient_signature_checks.rs`** (1,767 LOC)~~ ✅ Split (b0cf24ed4) — extracted overload compatibility, implicit-any return checks, modifier combinations, and signature utilities (~466 LOC) into `overload_compatibility.rs`, reducing to 1,301 LOC.
   - ~~**Checker `type_node.rs`** (1,765 LOC)~~ ✅ Deduplicated (9ca67eac4) — extracted 4 identical type/value/def_id resolver closure blocks into shared helper methods (`resolve_type_symbol`, `resolve_value_symbol`, `resolve_value_symbol_with_libs`, `resolve_def_id`, `resolve_def_id_with_qualified_names`, `lower_with_resolvers`), reducing to 1,522 LOC.
11. ~~**Solver `caches/db.rs`** (1,896 LOC): approaching 2000-line limit~~ ✅ Split — extracted `QueryCache` struct and all its impl blocks (~1,006 LOC) into `caches/query_cache.rs`, reducing `db.rs` to ~882 LOC. The trait file now contains only `TypeDatabase`/`QueryDatabase` trait definitions and their `TypeInterner` implementations; the cache file contains the concrete `QueryCache` implementation.
12. ~~**DRY violation: `emit_exported_variable` vs `emit_variable_declaration_statement`**~~ ✅ Done (8eb4e84af) — extracted shared `emit_variable_decl_type_or_initializer` helper in `helpers.rs`. Also fixed latent bug where export path incorrectly included `NullKeyword` in literal initializer check (`const x = null` emitted `= null` instead of `: any`).
13. ~~**Emit JS pass rate regression**: Commit 92ba86966 ("Fix auto-accessor emit ordering") caused JS emit pass rate to drop from 67.9% → 67.7% (23 fewer passing tests).~~ ✅ Fixed — root cause was unconditional `mark_class_helpers` call for auto-accessor classes at ALL targets. The `__classPrivateFieldGet`/`Set` helpers were emitted even for ES2022+/ESNext where native syntax should be used. Fix gates both helper marking (`lowering_pass.rs`) and WeakMap emission (`declarations.rs`) behind `needs_es2022_lowering`. Pass rate restored to 67.9% (9,254 tests, +8 net improvement).
14. ~~**Misplaced non-source files in checker `src/`**: `keyof-type-checking/` (scaffolded skill template with placeholders) and `state_orchestration_docs.md` (orphaned architectural doc) were sitting inside `crates/tsz-checker/src/`.~~ ✅ Removed (c0cc2fb4c).
15. ~~**Checker top-level file sprawl**: assignability/assignment/subtype_identity checkers~~ ✅ Done (d45a73578) — moved `assignability_checker.rs`, `assignment_checker.rs`, `subtype_identity_checker.rs` into `assignability/` subdirectory (24 → 21 loose files). ~~Remaining grouping candidates: call/param/signature checkers, type-related checkers (generic, iterable, promise).~~ ✅ Done (e38a95d3f) — moved 10 `*_checker.rs` + `signature_builder.rs` files (3,913 LOC) into `checkers/` subdirectory (21 → 11 loose files). ~~Remaining grouping candidates: `error_handler.rs`/`error_reporter.rs` into an `errors/` directory~~ ✅ Done — `error_handler.rs` (668 LOC) removed entirely: the `ErrorHandler` trait was dead abstraction (20+ unused trait methods, unused `DiagnosticBuilder`, unused `&mut CheckerState` blanket impl). Only `emit_error_at` was used (2 call sites); promoted to inherent method on `CheckerState`. Remaining grouping candidates: `decorators.rs`/`optional_chain.rs`/`triple_slash_validator.rs` if more related files accumulate.
16. **Solver re-export shims**: `judge.rs` and `visitor.rs` in solver `src/` are 1-line `pub use` shims (`pub use crate::relations::judge::*` / `pub use crate::visitors::visitor::*`). Used from ~20 call sites in checker/emitter. Low priority — they provide stable convenience paths.
17. **Solver `inference/infer.rs`** reduced from 1,878 → 1,106 LOC by extracting the structural type matching algorithm (~770 LOC) into `infer_matching.rs`. The extracted file contains `infer_from_types` and all its type-shape dispatchers (objects, functions, tuples, callables, unions, intersections, applications, template literals, mapped types). The remaining `infer.rs` now focuses on the inference engine core: union-find, variable management, constraint tracking, and cycle detection.
18. **Solver `relations/compat.rs`** reduced from 1,876 → 1,313 LOC by extracting nominal typing overrides (~564 LOC) into `compat_overrides.rs`. The extracted file contains `is_assignable_with_overrides`, `private_brand_assignability_override`, `enum_assignability_override`, `are_types_identical_for_redeclaration`, and related enum/brand helpers. The remaining `compat.rs` now focuses on core compatibility checking (normalization, weak types, excess properties, structural dispatch).
19. **Solver `narrowing/mod.rs`** reduced from 1,839 → 1,421 LOC by extracting instanceof narrowing (~418 LOC) into `narrowing/instanceof.rs`. The extracted file contains `narrow_by_instanceof`, `narrow_by_instance_type`, and `narrow_by_instanceof_false`.
20. **Solver `operations/mod.rs`** reduced from 1,811 → 1,085 LOC by extracting argument checking, parameter analysis, tuple rest handling, placeholder detection, and contextual sensitivity helpers (~726 LOC) into `operations/call_args.rs`. Next solver files to monitor: `diagnostics.rs` (1,690 LOC), `operations/constraints.rs` (1,530 LOC).
21. **Solver `contextual.rs`** reduced from 1,693 → 805 LOC by extracting 8 `TypeVisitor` extractor implementations and `collect_single_or_union` helper (~909 LOC) into `contextual_extractors.rs`. Extractors: `ThisTypeExtractor`, `ReturnTypeExtractor`, `ThisTypeMarkerExtractor`, `ArrayElementExtractor`, `TupleElementExtractor`, `PropertyExtractor`, `ParameterExtractor`, `ParameterForCallExtractor`, `ApplicationArgExtractor`, plus `extract_param_type_at`.
22. ~~**DRY violation: `LiteralValue` → primitive `TypeId` match blocks**~~ ✅ Done (a5d257efa) — five identical 4-arm match blocks (`String→STRING`, `Number→NUMBER`, `Boolean→BOOLEAN`, `BigInt→BIGINT`) across `expression_ops.rs`, `widening.rs`, and `type_queries/extended.rs` consolidated into `LiteralValue::primitive_type_id()` const method on the enum itself.
23. **Dead modules in solver**: `sound.rs` (364 LOC) and `flow_analysis.rs` (250 LOC) have zero production callers — all public API is only exercised by their own test files. Consider `pub(crate)` or `#[cfg(test)]` gating.
24. **Dead function**: `get_required_es_version_for_global` in `checker/context/lib_queries.rs:190` has zero callers anywhere.
25. **Additional near-threshold checker files** not in the watchlist above: `member_declaration_checks.rs` (1,695 LOC), `flow_graph_builder.rs` (1,690 LOC), `state_type_analysis.rs` (1,619 LOC), `type_checking_global.rs` (1,599 LOC), `symbol_resolver.rs` (1,581 LOC), `context/mod.rs` (1,572 LOC).
26. **Binder lacks subdirectory organization**: 14 files at top level with no grouping. Logical candidates: `binding/` (state_binding + state_binding_validation), `nodes/` (state_node_binding + state_node_binding_names), `modules/` (state_module_binding + state_import_export + module_resolution_debug).
