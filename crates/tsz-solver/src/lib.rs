//! Query-Based Structural Solver
//!
//! This module implements a declarative, query-based type solver architecture.
//! It uses:
//!
//! - **Ena**: For unification (Union-Find) in generic type inference
//! - **Custom `TypeData`**: Structural type representation with interning
//! - **Cycle Detection**: Coinductive semantics for recursive types
//!
//! Key benefits:
//! - O(1) type equality via interning (`TypeId` comparison)
//! - Automatic cycle handling via coinductive semantics
//! - Lazy evaluation - only compute types that are queried
mod apparent;
mod application;
pub mod binary_ops;
pub mod canonicalize;
mod class_hierarchy;
pub mod compat;
mod contextual;
mod db;
pub mod def;
mod diagnostics;
pub mod element_access;
mod evaluate;
pub mod evaluate_rules;
pub mod expression_ops;
mod flow_analysis;
mod format;
pub mod freshness;
mod index_signatures;
mod infer;
pub mod inheritance;
mod instantiate;
mod intern;
pub mod judge;
mod lawyer;
mod narrowing;
mod object_literal;
pub mod objects;
pub mod operations;
pub mod operations_property;
mod query_trace;
pub mod recursion;
pub mod relation_queries;
pub mod sound;
mod subtype;
mod subtype_rules;
pub mod tracer;
mod type_factory;
pub mod type_queries;
pub mod type_queries_extended;
pub mod types;
pub mod unsoundness_audit;
pub mod utils;
pub mod variance;
pub mod visitor;
pub mod widening;
pub use intern::TypeInterner;
pub use operations::infer_generic_function;
pub use visitor::{
    ConstAssertionVisitor, ObjectTypeKind, RecursiveTypeCollector, TypeCollectorVisitor, TypeKind,
    TypeKindVisitor, TypePredicateVisitor, TypeVisitor, application_id, array_element_type,
    bound_parameter_index, callable_shape_id, classify_object_type, collect_all_types,
    collect_enum_def_ids, collect_lazy_def_ids, collect_referenced_types, collect_type_queries,
    conditional_type_id, contains_error_type, contains_infer_types, contains_this_type,
    contains_type_matching, contains_type_parameters, enum_components, for_each_child,
    function_shape_id, index_access_parts, intersection_list_id, intrinsic_kind, is_array_type,
    is_conditional_type, is_empty_object_type, is_empty_object_type_db, is_enum_type,
    is_error_type, is_function_type, is_function_type_db, is_generic_application,
    is_index_access_type, is_intersection_type, is_literal_type, is_literal_type_db,
    is_mapped_type, is_module_namespace_type, is_module_namespace_type_db, is_object_like_type,
    is_object_like_type_db, is_primitive_type, is_template_literal_type, is_this_type,
    is_tuple_type, is_type_kind, is_type_parameter, is_type_reference, is_union_type, is_unit_type,
    keyof_inner_type, lazy_def_id, literal_number, literal_string, literal_value, mapped_type_id,
    module_namespace_symbol_ref, no_infer_inner_type, object_shape_id, object_with_index_shape_id,
    readonly_inner_type, recursive_index, ref_symbol, string_intrinsic_components,
    template_literal_id, test_type, tuple_list_id, type_param_info, type_query_symbol,
    union_list_id, unique_symbol_ref, walk_referenced_types,
};

pub use apparent::{
    ApparentMemberKind, apparent_object_member_kind, apparent_primitive_member_kind,
    apparent_primitive_members,
};
pub use application::*;
pub use binary_ops::*;
pub use canonicalize::*;
pub use class_hierarchy::*;
pub use compat::*;
pub use contextual::{ContextualTypeContext, apply_contextual_type};
pub use db::{QueryCache, QueryDatabase, RelationCacheProbe, RelationCacheStats, TypeDatabase};
pub use def::*;
pub use diagnostics::SubtypeFailureReason;
pub use diagnostics::{
    DiagnosticArg, DiagnosticBuilder, DiagnosticCollector, DiagnosticSeverity, PendingDiagnostic,
    PendingDiagnosticBuilder, SourceLocation, SourceSpan, SpannedDiagnosticBuilder,
};
pub use element_access::*;
pub use evaluate::*;
pub use flow_analysis::*;
pub use format::TypeFormatter;
pub use freshness::*;
pub use index_signatures::*;
pub use infer::*;
pub use inheritance::*;
pub use instantiate::{
    MAX_INSTANTIATION_DEPTH, TypeInstantiator, TypeSubstitution, instantiate_type,
    substitute_this_type,
};
pub use judge::*;
pub use lawyer::AnyPropagationRules;
pub use narrowing::*;
pub use object_literal::ObjectLiteralBuilder;
pub use objects::*;
pub use operations::{
    AssignabilityChecker, CallEvaluator, CallResult, MAX_CONSTRAINT_RECURSION_DEPTH,
};
pub use relation_queries::*;
pub use sound::*;
pub use subtype::{
    AnyPropagationMode, SubtypeChecker, SubtypeResult, TypeEnvironment, TypeResolver,
    are_types_structurally_identical, is_subtype_of,
};
pub use type_factory::*;
pub use types::{
    CallSignature, CallableShapeId, IntrinsicKind, LiteralValue, MappedModifier, ObjectShapeId,
    PropertyInfo, PropertyLookup, SymbolRef, TypeApplication, TypeApplicationId, TypeData, TypeId,
    TypeListId, Visibility, is_compiler_managed_type,
};
pub use types::{
    CallableShape, ConditionalType, FunctionShape, FunctionShapeId, IndexSignature, MappedType,
    MappedTypeId, ObjectFlags, ObjectShape, OrderedFloat, ParamInfo, RelationCacheKey,
    TemplateSpan, TupleElement, TupleListId, TypeParamInfo, TypePredicate, TypePredicateTarget,
};
pub use unsoundness_audit::*;
pub use variance::*;
pub use widening::*;

// Test modules: Most are loaded by their source files via #[path = "tests/..."] declarations.
// Only include modules here that aren't loaded elsewhere to avoid duplicate_mod warnings.
#[cfg(test)]
#[path = "../tests/bidirectional_tests.rs"]
mod bidirectional_tests;
// callable_tests: loaded from subtype.rs
// compat_tests: loaded from compat.rs
// contextual_tests: loaded from contextual.rs
// db_tests: loaded from db.rs
// diagnostics_tests: loaded from diagnostics.rs
// evaluate_tests: loaded from evaluate.rs
// index_signature_tests: loaded from subtype.rs
// infer_tests: loaded from infer.rs
// instantiate_tests: loaded from instantiate.rs
#[cfg(test)]
#[path = "../tests/integration_tests.rs"]
mod integration_tests;
// intern_tests: loaded from intern.rs
#[cfg(test)]
#[path = "../tests/enum_nominality.rs"]
mod enum_nominality;
#[cfg(test)]
#[path = "../tests/intersection_union_tests.rs"]
mod intersection_union_tests;
// lawyer_tests: loaded from lawyer.rs
#[cfg(test)]
#[path = "../tests/mapped_key_remap_tests.rs"]
mod mapped_key_remap_tests;
// narrowing_tests: loaded from narrowing.rs
// operations_tests: loaded from operations.rs
// subtype_tests: loaded from subtype.rs
#[cfg(test)]
#[path = "../tests/intersection_distributivity_tests.rs"]
mod intersection_distributivity_tests;
#[cfg(test)]
#[path = "../tests/intersection_type_param_tests.rs"]
mod intersection_type_param_tests;
#[cfg(test)]
#[path = "../tests/template_expansion_tests.rs"]
mod template_expansion_tests;
#[cfg(test)]
#[path = "../tests/template_literal_comprehensive_test.rs"]
mod template_literal_comprehensive_test;
#[cfg(test)]
#[path = "../tests/template_literal_subtype_tests.rs"]
mod template_literal_subtype_tests;
// tracer_tests: tests are in tracer.rs module
#[cfg(test)]
#[path = "../tests/type_law_tests.rs"]
mod type_law_tests;
// types_tests: loaded from types.rs
// union_tests: loaded from subtype.rs
#[cfg(test)]
#[path = "../tests/isomorphism_tests.rs"]
mod isomorphism_tests;
#[cfg(test)]
#[path = "../tests/isomorphism_validation.rs"]
mod isomorphism_validation;
// solver_refactoring_tests: kept in root crate (depends on checker types)
#[cfg(test)]
#[path = "../tests/conditional_comprehensive_tests.rs"]
mod conditional_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/function_comprehensive_tests.rs"]
mod function_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/interface_comprehensive_tests.rs"]
mod interface_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/keyof_comprehensive_tests.rs"]
mod keyof_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/mapped_comprehensive_tests.rs"]
mod mapped_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/tuple_comprehensive_tests.rs"]
mod tuple_comprehensive_tests;
#[cfg(test)]
#[path = "tests/type_queries_property_names_tests.rs"]
mod type_queries_property_names_tests;
#[cfg(test)]
#[path = "tests/typedata_contract_tests.rs"]
mod typedata_contract_tests;
#[cfg(test)]
#[path = "../tests/union_intersection_comprehensive_tests.rs"]
mod union_intersection_comprehensive_tests;
#[cfg(test)]
#[path = "tests/visitor_tests.rs"]
mod visitor_tests;
