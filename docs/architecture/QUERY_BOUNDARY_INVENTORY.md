# Query Boundary Inventory

`crates/tsz-checker/src/query_boundaries` is the checker-facing API over
solver semantics. This inventory classifies each module by intended ownership
so future checker fixes use a reusable boundary instead of growing local solver
knowledge.

Categories:

- Stable API: preferred checker entry point for a semantic question.
- Diagnostic adapter: converts semantic facts into diagnostic-oriented answers
  while leaving span/emission policy in the checker.
- Compatibility shim: behavior-preserving migration surface over older checker
  call sites; acceptable, but should shrink as stable APIs replace it.
- Quarantine helper: temporary wrapper around low-level solver representation
  access. New checker code should not add more of these without a migration
  plan.

## Module Inventory

| Module | Category | Main exports / role | Notes |
|---|---|---|---|
| [`mod.rs`](../../crates/tsz-checker/src/query_boundaries/mod.rs) | Stable API policy | boundary module tree and ownership rules | Root module documents allowed checker/solver ownership direction. |
| [`assignability.rs`](../../crates/tsz-checker/src/query_boundaries/assignability.rs) | Stable API plus quarantine helper | `RelationRequest`, `execute_relation`, assignability gates, property classification | One remaining direct `type_queries::data::get_intersection_members` call should move behind a solver query wrapper. |
| [`capabilities.rs`](../../crates/tsz-checker/src/query_boundaries/capabilities.rs) | Stable API | environment capability structs and feature gates | Checker-owned capability facts. |
| [`environment.rs`](../../crates/tsz-checker/src/query_boundaries/environment.rs) | Diagnostic adapter | `CapabilityDiagnostic` and environment diagnostic decisions | Produces diagnostic decisions; caller owns spans/emission. |
| [`class.rs`](../../crates/tsz-checker/src/query_boundaries/class.rs) | Stable API plus quarantine helper | class member compatibility and base-type predicates | Direct `is_valid_base_type` wrappers are quarantine helpers. |
| [`class_type.rs`](../../crates/tsz-checker/src/query_boundaries/class_type.rs) | Compatibility shim | class-type shape helpers | Thin wrappers over solver type queries. |
| [`common.rs`](../../crates/tsz-checker/src/query_boundaries/common.rs) | Compatibility shim plus quarantine helper | broad type-data predicates, constructors, display helpers, and data wrappers | Largest migration surface; prefer narrower stable modules for new calls. |
| [`construct_signatures.rs`](../../crates/tsz-checker/src/query_boundaries/construct_signatures.rs) | Stable API | construct-signature summarization | Checker-facing construct-signature query. |
| [`definite_assignment.rs`](../../crates/tsz-checker/src/query_boundaries/definite_assignment.rs) | Diagnostic adapter | constructor/property use-before-assignment decisions | Keeps TS2454/strict-property-initialization policy out of call sites. |
| [`diagnostics.rs`](../../crates/tsz-checker/src/query_boundaries/diagnostics.rs) | Diagnostic adapter | display-oriented type/property queries | Used by diagnostic formatting and spelling suggestions. |
| [`dispatch.rs`](../../crates/tsz-checker/src/query_boundaries/dispatch.rs) | Compatibility shim | expression-dispatch type constructors/classifiers | Migration surface for dispatch code. |
| [`flow.rs`](../../crates/tsz-checker/src/query_boundaries/flow.rs) | Compatibility shim | flow helper predicates and type facts | Prefer narrower flow-analysis APIs for new work. |
| [`flow_analysis.rs`](../../crates/tsz-checker/src/query_boundaries/flow_analysis.rs) | Stable API | flow construction, assignability, type evaluation helpers | Canonical boundary for control-flow code. |
| [`index_signature.rs`](../../crates/tsz-checker/src/query_boundaries/index_signature.rs) | Compatibility shim | index-signature helpers | Narrow adapter over solver classification. |
| [`inference.rs`](../../crates/tsz-checker/src/query_boundaries/inference.rs) | Compatibility shim | inference shape helpers | Keep until inference paths have stable request APIs. |
| [`intersection_display.rs`](../../crates/tsz-checker/src/query_boundaries/intersection_display.rs) | Diagnostic adapter | intersection display helpers | Presentation-oriented boundary. |
| [`js_exports.rs`](../../crates/tsz-checker/src/query_boundaries/js_exports.rs) | Stable API | JS/CommonJS export surface queries | Owns checker/source-specific JS export synthesis. |
| [`name_resolution.rs`](../../crates/tsz-checker/src/query_boundaries/name_resolution.rs) | Diagnostic adapter | name-resolution diagnostic helpers | Keeps name lookup diagnostics structured. |
| [`property_access.rs`](../../crates/tsz-checker/src/query_boundaries/property_access.rs) | Stable API | property/index access classification and lookup | Preferred property-access semantic boundary. |
| [`recursive_alias.rs`](../../crates/tsz-checker/src/query_boundaries/recursive_alias.rs) | Stable API | recursive alias detection helpers | DefId/type alias boundary. |
| [`relation_types.rs`](../../crates/tsz-checker/src/query_boundaries/relation_types.rs) | Stable API | relation failure/property classification data types | Shared request/result vocabulary. |
| [`spread.rs`](../../crates/tsz-checker/src/query_boundaries/spread.rs) | Compatibility shim | spread type construction helpers | Thin construction wrappers for spread handling. |
| [`type_checking.rs`](../../crates/tsz-checker/src/query_boundaries/type_checking.rs) | Compatibility shim | constructor/function classification | Older checker-facing wrapper surface. |
| [`type_checking_utilities.rs`](../../crates/tsz-checker/src/query_boundaries/type_checking_utilities.rs) | Compatibility shim | array/index/literal/intersection classifiers | Utility wrappers; prefer narrower stable modules when adding calls. |
| [`type_construction.rs`](../../crates/tsz-checker/src/query_boundaries/type_construction.rs) | Compatibility shim | construction boundary module | Currently no exported functions; keep as placeholder only if future construction APIs land. |
| [`type_defaults.rs`](../../crates/tsz-checker/src/query_boundaries/type_defaults.rs) | Stable API | type-parameter defaulting | Small stable semantic wrapper. |
| [`type_parameter_identity.rs`](../../crates/tsz-checker/src/query_boundaries/type_parameter_identity.rs) | Stable API | type-parameter identity comparisons | Stable type-parameter semantic helper. |
| [`type_predicates.rs`](../../crates/tsz-checker/src/query_boundaries/type_predicates.rs) | Stable API | type-predicate extraction | Stable predicate query wrapper. |
| [`type_rewrite.rs`](../../crates/tsz-checker/src/query_boundaries/type_rewrite.rs) | Stable API | type rewrite helper | Stable rewrite wrapper. |
| [`variance.rs`](../../crates/tsz-checker/src/query_boundaries/variance.rs) | Stable API | variance computation with resolver | Solver-backed variance boundary. |
| [`widening.rs`](../../crates/tsz-checker/src/query_boundaries/widening.rs) | Stable API | widening helpers | Stable wrapper for widening behavior. |
| [`checkers/mod.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/mod.rs) | Stable API policy | checker-specific boundary module tree | Groups call, constructor, generic, iterable, JSX, promise, and property boundaries. |
| [`checkers/call.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/call.rs) | Stable API plus quarantine helper | call-site classification and overload helpers | Direct overload data wrapper should move behind a solver query helper. |
| [`checkers/constructor.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/constructor.rs) | Stable API plus quarantine helper | constructor classification and display helpers | Direct constructor return data wrapper should move behind a solver query helper. |
| [`checkers/generic.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/generic.rs) | Compatibility shim plus quarantine helper | generic checker predicates and type-argument helpers | Contains direct type-param extraction/count wrappers. |
| [`checkers/iterable.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/iterable.rs) | Stable API | iterable/promise iteration helpers | Preferred iterable checker boundary. |
| [`checkers/jsx.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/jsx.rs) | Stable API | JSX shape helpers | Preferred JSX checker boundary. |
| [`checkers/promise.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/promise.rs) | Stable API | promise/thenable helpers | Preferred promise checker boundary. |
| [`checkers/property.rs`](../../crates/tsz-checker/src/query_boundaries/checkers/property.rs) | Stable API | property checker wrapper | Narrow checker-specific entry point. |
| [`state/mod.rs`](../../crates/tsz-checker/src/query_boundaries/state/mod.rs) | Stable API policy | checker-state boundary module tree | Groups state/checking, type-environment, and type-resolution boundaries. |
| [`state/checking.rs`](../../crates/tsz-checker/src/query_boundaries/state/checking.rs) | Stable API | checker-state semantic helpers | Preferred boundary for state/checking code. |
| [`state/type_analysis.rs`](../../crates/tsz-checker/src/query_boundaries/state/type_analysis.rs) | Compatibility shim | type-analysis module placeholder | No exported functions today. |
| [`state/type_environment.rs`](../../crates/tsz-checker/src/query_boundaries/state/type_environment.rs) | Stable API | `TypeEvaluator` construction and environment evaluation helpers | Architecture tests direct checker code here for evaluation. |
| [`state/type_resolution.rs`](../../crates/tsz-checker/src/query_boundaries/state/type_resolution.rs) | Stable API | type-resolution helper queries | Preferred state/type-resolution boundary. |
| [`type_computation/mod.rs`](../../crates/tsz-checker/src/query_boundaries/type_computation/mod.rs) | Stable API policy | type-computation boundary module tree | Groups access, complex, and core expression computation boundaries. |
| [`type_computation/access.rs`](../../crates/tsz-checker/src/query_boundaries/type_computation/access.rs) | Stable API | computed access helpers | Preferred access-computation boundary. |
| [`type_computation/complex.rs`](../../crates/tsz-checker/src/query_boundaries/type_computation/complex.rs) | Stable API | complex type computation helpers | Preferred complex-computation boundary. |
| [`type_computation/core.rs`](../../crates/tsz-checker/src/query_boundaries/type_computation/core.rs) | Stable API | core expression type computation helpers | Preferred type-computation boundary. |

## Quarantine List

Current direct `tsz_solver::type_queries::data::*` wrappers live only inside
`query_boundaries`, as enforced by
[`architecture_contract_tests.rs`](../../crates/tsz-checker/tests/architecture_contract_tests.rs).
The remaining quarantine modules are:

- `common.rs`: broad data-layer wrapper set for object symbols, constructor
  shapes, callable shapes, raw property types, and callable property collection.
- `assignability.rs`: direct intersection-member access during object-property
  classification.
- `class.rs`: valid base-type predicates.
- `checkers/call.rs`: overload call-signature extraction.
- `checkers/constructor.rs`: constructor return type extraction.
- `checkers/generic.rs`: call type-parameter extraction and overload type
  parameter counts.

New checker code should prefer stable APIs above. If a quarantine helper is
needed, add a narrow stable wrapper first or include a follow-up issue naming
the solver query helper that will retire the direct data accessor.
