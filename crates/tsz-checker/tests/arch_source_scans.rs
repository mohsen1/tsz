//! Architecture source-scan ratchets, declared as an explicit `[[test]]`
//! integration target so the checker-integration CI lane (which enumerates
//! `cargo metadata` targets with `kind == "test"`) builds and runs them.
//!
//! These ratchets previously lived as `#[cfg(test)]` lib-test mounts in
//! `src/lib.rs`, but CI never builds the checker lib-test binary (it exceeds
//! what the `32 GiB` runners can link; see `run_checker_integration_tests`
//! in `scripts/ci/full-ci.sh`), so the invariants were silently
//! unenforced.
//!
//! All scans are pure source-text checks over `$CARGO_MANIFEST_DIR/src` with
//! no dependency on crate internals:
//! - `relation_routing_residual_arch_tests`: diagnostic-bearing relation
//!   probes in production checker code must route through named
//!   `*_relation_outcome` helpers at the assignability boundary
//!   (issues #8227 / #12949).
//! - `common_boundary_export_ratchets`: the `query_boundaries/common.rs`
//!   `pub(crate) fn` surface only changes with an explicit allowlist update
//!   (issue #12948).
//! - `assignability_surface_construction_boundary_scans`: assignability
//!   relation-preparation callers route transient tuple/object/property/union
//!   construction through `query_boundaries::assignability::construction`.
//! - `construction_boundary_signature_scans`: the issue #13022 module set
//!   constructs signature-bearing solver types only through
//!   `query_boundaries::construct_signatures`, never via inline shape
//!   literals or direct interning calls.
//! - `diagnostic_construction_boundary_scans`: diagnostic reporters route
//!   display-only solver shape construction through
//!   `query_boundaries::diagnostics`.
//! - `excess_property_construction_boundary_scans`: excess-property checking
//!   routes object shape construction through query boundaries.
//! - `class_instance_walk_state_scans`: class instance base traversal uses a
//!   named checker-owned walk state instead of paired raw visited sets.
//! - `cross_arena_delegation_scope_scans`: cross-arena delegation depth uses a
//!   scoped guard instead of manual enter/leave pairs.
//! - `index_signature_boundary_scans`: production checker index-signature
//!   queries go through `query_boundaries::index_signature` rather than
//!   constructing the raw solver resolver at call sites.
//! - `indexed_access_key_space_construction_boundary_scans`: indexed-access
//!   validation routes key-space/value-surface construction through
//!   `query_boundaries::indexed_access_key_space`.
//! - `jsdoc_construction_boundary_scans`: JSDoc type-resolution callers route
//!   solver shape construction through `query_boundaries::jsdoc_construction`.
//! - `jsx_construction_boundary_scans`: JSX checker callers route object and
//!   function shape construction through `query_boundaries::checkers::jsx`.
//! - `js_class_property_construction_boundary_scans`: JS class-property
//!   scanning routes type-parameter, array/union, property, callable, and object
//!   construction through `query_boundaries::checkers::class_properties`.
//! - `strict_bind_call_apply_construction_boundary_scans`: property-access
//!   helpers route strict bind/call/apply signature construction through
//!   `query_boundaries::property_access`.
//! - `property_access_result_construction_boundary_scans`: property-access
//!   environment resolution routes optional/union/intersection result
//!   construction through `query_boundaries::property_access`.
//! - `declaration_export_construction_boundary_scans`: namespace/module
//!   declaration checkers route export-surface construction through
//!   `query_boundaries::declaration_exports`.
//! - `decorator_construction_boundary_scans`: class-member decorator
//!   signature checking routes semantic helper-type construction through
//!   `query_boundaries::checkers::decorators`.
//! - `import_attribute_construction_boundary_scans`: static and dynamic import
//!   callers route import-attribute object construction through
//!   `query_boundaries::import_attributes`.
//! - `commonjs_json_export_surface_construction_boundary_scans`: JSON module
//!   and current-file CommonJS namespace surfaces route solver construction
//!   through `query_boundaries::js_exports`.
//! - `commonjs_resolution_export_surface_construction_boundary_scans`: CommonJS
//!   resolution/collection routes descriptor, overlay, expando, constructor,
//!   and imported module value-surface construction through
//!   `query_boundaries::js_exports`.
//! - `binding_pattern_construction_boundary_scans`: binding/destructuring
//!   pattern callers route contextual tuple/object/property/union construction
//!   through `query_boundaries::binding_patterns`.
//! - `type_query_construction_boundary_scans`: const value/type-query callers
//!   route literal/property/object/tuple construction through
//!   `query_boundaries::type_query_construction`.
//! - `type_checking_surface_construction_boundary_scans`: type-checking
//!   helpers route temporary type surfaces and user type-parameter
//!   construction through `query_boundaries::type_checking`.
//! - `type_node_fallback_construction_boundary_scans`: type-node resolution
//!   routes intersection, callable/function, and object fallback construction
//!   through construction query boundaries.
//! - `class_partial_constructor_construction_boundary_scans`: class
//!   constructor-part helpers route partial static-constructor solver
//!   construction through `query_boundaries::class_type`.
//! - `class_surface_construction_boundary_scans`: class instance/interface and
//!   final constructor surface construction routes through
//!   `query_boundaries::class_type`.
//! - `class_member_surface_construction_boundary_scans`: class instance and
//!   constructor member/in-progress surfaces route through
//!   `query_boundaries::class_type`.
//! - `class_constructor_return_construction_boundary_scans`: class constructor
//!   checking routes return/intersection/static-property construction through
//!   `query_boundaries::checkers::constructor`.
//! - `await_promise_construction_boundary_scans`: await checking routes
//!   contextual promise operand and `Awaited<T>` join construction through
//!   `query_boundaries::checkers::promise`.
//! - `call_inference_construction_boundary_scans`: call inference routes
//!   partial object/function/tuple inference construction through
//!   `query_boundaries::checkers::call`.
//! - `call_result_construction_boundary_scans`: call result handling routes
//!   correlated unions, optional-chain returns, and recursive fallback result
//!   construction through `query_boundaries::checkers::call`.
//! - `excess_property_nested_target_construction_boundary_scans`: nested
//!   excess-property target and annotation intersections route through
//!   `query_boundaries::state::checking`.
//! - `expression_result_construction_boundary_scans`: expression computation
//!   routes selected result-shape construction through
//!   `query_boundaries::type_computation::expression_results`.
//! - `object_literal_context_construction_boundary_scans`: object-literal
//!   contextual typing routes union/intersection rebuilds through
//!   `query_boundaries::object_literal_context`.
//! - `object_literal_result_construction_boundary_scans`: object-literal
//!   result construction routes final object/index/union/intersection/mapped
//!   surfaces through `query_boundaries::type_computation::object_literals`.
//! - `module_type_analysis_surface_construction_boundary_scans`: module and
//!   namespace type-analysis surfaces route through
//!   `query_boundaries::state::type_analysis`.
//! - `module_augmentation_surface_construction_boundary_scans`: module
//!   augmentation member and augmented base surface construction routes through
//!   `query_boundaries::module_augmentation`.
//! - `interface_type_literal_surface_construction_boundary_scans`: interface
//!   and type-literal own member surfaces route through
//!   `query_boundaries::type_construction` and
//!   `query_boundaries::construct_signatures`.
//! - `interface_merge_surface_construction_boundary_scans`: interface merge
//!   reconstruction routes final callable/object/index/intersection surfaces
//!   through `query_boundaries::interface_merge`.
//! - `js_constructor_surface_construction_boundary_scans`: checked-JS
//!   constructor/prototype instance surfaces route through
//!   `query_boundaries::type_computation::complex`.
//! - `yield_context_construction_boundary_scans`: yield dispatch routes
//!   `yield*` contextual generator/array construction through
//!   `query_boundaries::dispatch`.

#[path = "arch_source_scans/assignability_surface_construction_boundary_scans.rs"]
mod assignability_surface_construction_boundary_scans;
#[path = "arch_source_scans/await_promise_construction_boundary_scans.rs"]
mod await_promise_construction_boundary_scans;
#[path = "arch_source_scans/binding_pattern_construction_boundary_scans.rs"]
mod binding_pattern_construction_boundary_scans;
#[path = "arch_source_scans/call_inference_construction_boundary_scans.rs"]
mod call_inference_construction_boundary_scans;
#[path = "arch_source_scans/call_result_construction_boundary_scans.rs"]
mod call_result_construction_boundary_scans;
#[path = "arch_source_scans/class_constructor_return_construction_boundary_scans.rs"]
mod class_constructor_return_construction_boundary_scans;
#[path = "arch_source_scans/class_instance_walk_state_scans.rs"]
mod class_instance_walk_state_scans;
#[path = "arch_source_scans/class_member_surface_construction_boundary_scans.rs"]
mod class_member_surface_construction_boundary_scans;
#[path = "arch_source_scans/class_partial_constructor_construction_boundary_scans.rs"]
mod class_partial_constructor_construction_boundary_scans;
#[path = "arch_source_scans/class_surface_construction_boundary_scans.rs"]
mod class_surface_construction_boundary_scans;
#[path = "arch_source_scans/common_boundary_export_ratchets.rs"]
mod common_boundary_export_ratchets;
#[path = "arch_source_scans/commonjs_json_export_surface_construction_boundary_scans.rs"]
mod commonjs_json_export_surface_construction_boundary_scans;
#[path = "arch_source_scans/commonjs_resolution_export_surface_construction_boundary_scans.rs"]
mod commonjs_resolution_export_surface_construction_boundary_scans;
#[path = "arch_source_scans/construction_boundary_signature_scans.rs"]
mod construction_boundary_signature_scans;
#[path = "arch_source_scans/cross_arena_delegation_scope_scans.rs"]
mod cross_arena_delegation_scope_scans;
#[path = "arch_source_scans/declaration_export_construction_boundary_scans.rs"]
mod declaration_export_construction_boundary_scans;
#[path = "arch_source_scans/decorator_construction_boundary_scans.rs"]
mod decorator_construction_boundary_scans;
#[path = "arch_source_scans/diagnostic_construction_boundary_scans.rs"]
mod diagnostic_construction_boundary_scans;
#[path = "arch_source_scans/excess_property_construction_boundary_scans.rs"]
mod excess_property_construction_boundary_scans;
#[path = "arch_source_scans/excess_property_nested_target_construction_boundary_scans.rs"]
mod excess_property_nested_target_construction_boundary_scans;
#[path = "arch_source_scans/expression_result_construction_boundary_scans.rs"]
mod expression_result_construction_boundary_scans;
#[path = "arch_source_scans/import_attribute_construction_boundary_scans.rs"]
mod import_attribute_construction_boundary_scans;
#[path = "arch_source_scans/index_signature_boundary_scans.rs"]
mod index_signature_boundary_scans;
#[path = "arch_source_scans/indexed_access_key_space_construction_boundary_scans.rs"]
mod indexed_access_key_space_construction_boundary_scans;
#[path = "arch_source_scans/interface_merge_surface_construction_boundary_scans.rs"]
mod interface_merge_surface_construction_boundary_scans;
#[path = "arch_source_scans/interface_type_literal_surface_construction_boundary_scans.rs"]
mod interface_type_literal_surface_construction_boundary_scans;
#[path = "arch_source_scans/js_class_property_construction_boundary_scans.rs"]
mod js_class_property_construction_boundary_scans;
#[path = "arch_source_scans/js_constructor_surface_construction_boundary_scans.rs"]
mod js_constructor_surface_construction_boundary_scans;
#[path = "arch_source_scans/jsdoc_construction_boundary_scans.rs"]
mod jsdoc_construction_boundary_scans;
#[path = "arch_source_scans/jsx_construction_boundary_scans.rs"]
mod jsx_construction_boundary_scans;
#[path = "arch_source_scans/lazy_resolution_session_scans.rs"]
mod lazy_resolution_session_scans;
#[path = "arch_source_scans/module_augmentation_surface_construction_boundary_scans.rs"]
mod module_augmentation_surface_construction_boundary_scans;
#[path = "arch_source_scans/module_type_analysis_surface_construction_boundary_scans.rs"]
mod module_type_analysis_surface_construction_boundary_scans;
#[path = "arch_source_scans/object_flags_boundary_scans.rs"]
mod object_flags_boundary_scans;
#[path = "arch_source_scans/object_literal_annotation_walker_scans.rs"]
mod object_literal_annotation_walker_scans;
#[path = "arch_source_scans/object_literal_context_construction_boundary_scans.rs"]
mod object_literal_context_construction_boundary_scans;
#[path = "arch_source_scans/object_literal_result_construction_boundary_scans.rs"]
mod object_literal_result_construction_boundary_scans;
#[path = "arch_source_scans/property_access_result_construction_boundary_scans.rs"]
mod property_access_result_construction_boundary_scans;
#[path = "arch_source_scans/relation_boundary_session_scans.rs"]
mod relation_boundary_session_scans;
#[path = "arch_source_scans/relation_routing_residual_arch_tests.rs"]
mod relation_routing_residual_arch_tests;
#[path = "arch_source_scans/spelling_suggestion_gateway_scans.rs"]
mod spelling_suggestion_gateway_scans;
#[path = "arch_source_scans/strict_bind_call_apply_construction_boundary_scans.rs"]
mod strict_bind_call_apply_construction_boundary_scans;
#[path = "arch_source_scans/type_checking_surface_construction_boundary_scans.rs"]
mod type_checking_surface_construction_boundary_scans;
#[path = "arch_source_scans/type_guard_walk_state.rs"]
mod type_guard_walk_state;
#[path = "arch_source_scans/type_node_fallback_construction_boundary_scans.rs"]
mod type_node_fallback_construction_boundary_scans;
#[path = "arch_source_scans/type_query_construction_boundary_scans.rs"]
mod type_query_construction_boundary_scans;
#[path = "arch_source_scans/type_reference_depth_session_scans.rs"]
mod type_reference_depth_session_scans;
#[path = "arch_source_scans/yield_context_construction_boundary_scans.rs"]
mod yield_context_construction_boundary_scans;
