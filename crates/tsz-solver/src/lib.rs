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
//!
//! # API Organization
//!
//! The public API is organized into tiered modules:
//!
//! - [`type_handles`] — Identity types (`TypeId`, `TypeData`, shapes). Safe for all consumers.
//! - [`query`] — Read-only type visitors and inspectors. Safe for all consumers.
//! - [`computation`] — Type relations, evaluation, instantiation, inference.
//!   Should be accessed through `query_boundaries` in the checker.
//! - [`construction`] — Type building (`TypeInterner`, factories).
//!   Should be accessed through `query_boundaries` in the checker.
//!
//! Flat re-exports are preserved for backwards compatibility but consumers
//! should prefer the module-based imports for clarity.

mod caches;
pub mod canonicalize;
pub mod classes;
mod contextual;
pub mod def;
mod diagnostics;
pub mod evaluation;
#[cfg(test)]
mod flow_analysis;
mod inference;
mod instantiation;
mod intern;
pub mod judge {
    //! Re-exports from `relations::judge` for convenience.
    pub use crate::relations::judge::*;
}
mod narrowing;
pub mod objects;
pub mod operations;
pub mod recursion;
pub mod relations;
#[cfg(test)]
mod sound;
pub mod type_queries;
// type_resolver moved into def/resolver.rs
pub mod types;
pub mod unsoundness_audit;
pub mod utils;
pub mod visitor {
    //! Re-exports from `visitors::visitor` for convenience.
    pub use crate::visitors::visitor::*;
}
mod visitors;

// =============================================================================
// Tiered API modules — structured access to solver functionality
// =============================================================================

/// Tier 1: Type identity handles and structural shapes.
///
/// These are pure data types with no computation. Safe for any consumer
/// (checker, emitter, LSP) to import directly.
pub mod type_handles {
    pub use crate::diagnostics::builders::{
        DiagnosticBuilder, DiagnosticCollector, SourceLocation, SpannedDiagnosticBuilder,
    };
    pub use crate::diagnostics::format::TypeFormatter;
    pub use crate::diagnostics::{
        DiagnosticArg, DiagnosticSeverity, PendingDiagnostic, PendingDiagnosticBuilder, SourceSpan,
        SubtypeFailureReason,
    };
    pub use crate::types::{
        CallSignature, CallableShape, CallableShapeId, ConditionalType, FunctionShape,
        FunctionShapeId, IndexSignature, IntrinsicKind, LiteralValue, MappedModifier, MappedType,
        MappedTypeId, ObjectFlags, ObjectShape, ObjectShapeId, OrderedFloat, ParamInfo,
        PropertyInfo, PropertyLookup, SymbolRef, TemplateSpan, TupleElement, TupleListId,
        TypeApplication, TypeApplicationId, TypeData, TypeId, TypeListId, TypeParamInfo,
        TypePredicate, TypePredicateTarget, Visibility, is_compiler_managed_type,
    };
}

/// Tier 2: Read-only type visitors and inspectors.
///
/// These functions inspect types but don't modify or create them.
/// Safe for any consumer to import directly.
pub mod query {
    pub use crate::visitors::visitor::{
        application_id, array_element_type, bound_parameter_index, callable_shape_id,
        collect_enum_def_ids, collect_infer_bindings, collect_lazy_def_ids,
        collect_referenced_types, collect_type_queries, conditional_type_id, contains_error_type,
        contains_free_infer_types, contains_infer_types, contains_this_type,
        contains_type_matching, contains_type_parameter_named, contains_type_parameters,
        enum_components, for_each_child, for_each_child_by_id, function_shape_id,
        has_deferred_conditional_member, index_access_parts, intersection_list_id, intrinsic_kind,
        is_array_type, is_conditional_type, is_empty_object_type,
        is_empty_object_type_through_type_constraints, is_enum_type, is_error_type,
        is_function_type, is_function_type_through_type_constraints, is_generic_application,
        is_identity_comparable_type, is_index_access_type, is_intersection_type, is_lazy_type,
        is_literal_type, is_literal_type_through_type_constraints, is_mapped_type,
        is_module_namespace_type, is_object_like_type,
        is_object_like_type_through_type_constraints, is_primitive_type,
        is_structurally_deferred_type, is_template_literal_type, is_this_type, is_tuple_type,
        is_type_parameter, is_type_query_type, is_type_reference, is_union_type, keyof_inner_type,
        lazy_def_id, literal_number, literal_string, literal_value, mapped_type_id,
        module_namespace_symbol_ref, no_infer_inner_type, object_shape_id,
        object_with_index_shape_id, readonly_inner_type, recursive_index,
        resolve_default_type_args, string_intrinsic_components, template_literal_id, tuple_list_id,
        type_param_info, type_query_symbol, union_list_id, unique_symbol_ref,
        walk_referenced_types,
    };
}

/// Tier 3: Type computation — relations, evaluation, instantiation, inference.
///
/// These perform type computation and should be accessed through
/// `query_boundaries` in the checker crate, not imported directly.
pub mod computation {
    // Subtype/assignability relations
    pub use crate::relations::compat::CompatChecker;
    pub use crate::relations::lawyer::AnyPropagationRules;
    pub use crate::relations::subtype::{
        AnyPropagationMode, SubtypeChecker, SubtypeResult, TypeEnvironment, TypeResolver,
        are_types_structurally_identical, is_subtype_of,
    };

    // Evaluation
    pub use crate::evaluation::evaluate::evaluate_type;

    // Instantiation
    pub use crate::instantiation::instantiate::{
        MAX_INSTANTIATION_DEPTH, TypeInstantiator, TypeSubstitution, instantiate_generic,
        instantiate_type, instantiate_type_preserving_meta, instantiate_type_with_depth_status,
        substitute_this_type,
    };

    // Contextual typing
    pub use crate::contextual::{
        ContextualTypeContext, apply_contextual_type, rest_argument_element_type,
    };

    // Operations
    pub use crate::operations::infer_generic_function;
    pub use crate::operations::{
        AssignabilityChecker, BinaryOpEvaluator, BinaryOpResult, CallEvaluator, CallResult,
        MAX_CONSTRAINT_RECURSION_DEPTH, get_contextual_signature_for_arity_with_compat_checker,
        get_contextual_signature_with_compat_checker,
    };
}

/// Tier 4: Type construction — building new types.
///
/// These create or modify types via the interner. Should be accessed through
/// `query_boundaries` in the checker crate.
pub mod construction {
    pub use crate::caches::db::{QueryDatabase, TypeDatabase};
    pub use crate::caches::query_cache::QueryCache;
    pub use crate::intern::TypeInterner;
    pub use crate::intern::type_factory::*;
}
pub use intern::TypeInterner;
pub use intern::clear_thread_local_cache;
pub use operations::infer_generic_function;
pub use operations::widening;
pub use visitors::visitor::{
    application_id, array_element_type, bound_parameter_index, callable_shape_id,
    collect_enum_def_ids, collect_infer_bindings, collect_lazy_def_ids, collect_referenced_types,
    collect_type_queries, conditional_type_id, contains_error_type, contains_free_infer_types,
    contains_infer_types, contains_this_type, contains_type_by_id, contains_type_matching,
    contains_type_parameter_named, contains_type_parameters, enum_components, for_each_child,
    for_each_child_by_id, function_shape_id, has_deferred_conditional_member, index_access_parts,
    intersection_list_id, intrinsic_kind, is_array_type, is_conditional_type, is_empty_object_type,
    is_empty_object_type_through_type_constraints, is_enum_type, is_error_type, is_function_type,
    is_function_type_through_type_constraints, is_generic_application, is_identity_comparable_type,
    is_index_access_type, is_intersection_type, is_lazy_type, is_literal_type,
    is_literal_type_through_type_constraints, is_mapped_type, is_module_namespace_type,
    is_object_like_type, is_object_like_type_through_type_constraints, is_primitive_type,
    is_structurally_deferred_type, is_template_literal_type, is_this_type, is_tuple_type,
    is_type_parameter, is_type_query_type, is_type_reference, is_union_type, keyof_inner_type,
    lazy_def_id, literal_number, literal_string, literal_value, mapped_type_id,
    module_namespace_symbol_ref, no_infer_inner_type, object_shape_id, object_with_index_shape_id,
    readonly_inner_type, recursive_index, references_any_type_param_named,
    resolve_default_type_args, string_intrinsic_components, template_literal_id, tuple_list_id,
    type_param_info, type_query_symbol, union_list_id, unique_symbol_ref,
    unwrap_readonly_or_noinfer, walk_referenced_types,
};

pub use caches::db::{QueryDatabase, TypeDatabase};
pub use caches::query_cache::{
    QueryCache, QueryCacheStatistics, RelationCacheProbe, RelationCacheStats, SharedQueryCache,
};
pub use canonicalize::*;
pub use classes::inheritance::*;
pub use contextual::{ContextualTypeContext, apply_contextual_type, rest_argument_element_type};
pub use def::*;
pub use diagnostics::SubtypeFailureReason;
pub use diagnostics::builders::{
    DiagnosticBuilder, DiagnosticCollector, SourceLocation, SpannedDiagnosticBuilder,
};
pub use diagnostics::format::TypeFormatter;
pub use diagnostics::{
    DiagnosticArg, DiagnosticSeverity, PendingDiagnostic, PendingDiagnosticBuilder, SourceSpan,
};
pub use evaluation::evaluate::*;
pub use evaluation::session::EvaluationSession;
pub use instantiation::application::*;
pub use instantiation::instantiate::{
    MAX_INSTANTIATION_DEPTH, TypeInstantiator, TypeSubstitution, instantiate_generic,
    instantiate_type, instantiate_type_preserving_meta, instantiate_type_with_depth_status,
    substitute_this_type,
};
pub use intern::type_factory::*;
pub use narrowing::*;
pub use objects::*;
pub use operations::compound_assignment;
pub use operations::compound_assignment::*;
pub use operations::expression_ops;
pub use operations::expression_ops::*;
pub use operations::{
    AssignabilityChecker, BinaryOpEvaluator, BinaryOpResult, CallEvaluator, CallResult,
    MAX_CONSTRAINT_RECURSION_DEPTH, get_contextual_signature_for_arity_with_compat_checker,
    get_contextual_signature_with_compat_checker,
};
pub use relations::compat::*;
pub use relations::judge::*;
pub use relations::lawyer::AnyPropagationRules;
pub use relations::relation_queries::*;
pub use relations::subtype::{
    AnyPropagationMode, SubtypeChecker, SubtypeResult, TypeEnvironment, TypeResolver,
    are_types_structurally_identical, is_subtype_of, reset_subtype_thread_local_state,
};
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
// unsoundness_audit: accessed via tsz_solver::unsoundness_audit module path
pub use widening::*;

// Test modules: Most are loaded by their source files via #[path = "tests/..."] declarations.
// Only include modules here that aren't loaded elsewhere to avoid duplicate_mod warnings.
#[cfg(test)]
#[path = "../tests/bidirectional_tests.rs"]
mod bidirectional_tests;
// callable_tests: loaded from relations/subtype.rs
// compat_tests: loaded from relations/compat.rs
// contextual_tests: loaded from contextual/mod.rs
// db_tests: loaded from caches/db.rs
// diagnostics_tests: loaded from diagnostics.rs
// evaluate_tests: loaded from evaluation/evaluate.rs
// index_signature_tests: loaded from relations/subtype.rs
// infer_tests: loaded from inference/infer.rs
// instantiate_tests: loaded from instantiation/instantiate.rs
#[cfg(test)]
#[path = "../tests/integration_tests.rs"]
mod integration_tests;
// intern_tests: loaded from intern/mod.rs
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
// narrowing_tests: loaded from narrowing/mod.rs
// operations_tests: loaded from operations/mod.rs
// subtype_tests: loaded from relations/subtype.rs
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
#[cfg(test)]
#[path = "../tests/type_law_tests.rs"]
mod type_law_tests;
// types_tests: loaded from types.rs
// union_tests: loaded from relations/subtype.rs
#[cfg(test)]
#[path = "../tests/isomorphism_tests.rs"]
mod isomorphism_tests;
#[cfg(test)]
#[path = "../tests/isomorphism_validation.rs"]
mod isomorphism_validation;
// solver_refactoring_tests: kept in root crate (depends on checker types)
#[cfg(test)]
#[path = "../tests/array_comprehensive_tests.rs"]
mod array_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/async_promise_comprehensive_tests.rs"]
mod async_promise_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/class_comprehensive_tests.rs"]
mod class_comprehensive_tests;
// compound_assignment_tests: loaded from operations/compound_assignment.rs
#[cfg(test)]
#[path = "../tests/architecture_guards.rs"]
mod architecture_guards;
#[cfg(test)]
#[path = "../tests/bct_tests.rs"]
mod bct_tests;
#[cfg(test)]
#[path = "../tests/boxed_augmentation_tests.rs"]
mod boxed_augmentation_tests;
#[cfg(test)]
#[path = "tests/classify_array_like_tests.rs"]
mod classify_array_like_tests;
#[cfg(test)]
#[path = "tests/classify_index_key_tests.rs"]
mod classify_index_key_tests;
#[cfg(test)]
#[path = "tests/computed_prop_name_tests.rs"]
mod computed_prop_name_tests;
#[cfg(test)]
#[path = "../tests/conditional_comprehensive_tests.rs"]
mod conditional_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/constraint_tests.rs"]
mod constraint_tests;
#[cfg(test)]
#[path = "../tests/function_comprehensive_tests.rs"]
mod function_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/index_access_comprehensive_tests.rs"]
mod index_access_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/interface_comprehensive_tests.rs"]
mod interface_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/keyof_comprehensive_tests.rs"]
mod keyof_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/mapped_architecture_tests.rs"]
mod mapped_architecture_tests;
#[cfg(test)]
#[path = "../tests/mapped_comprehensive_tests.rs"]
mod mapped_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/matching_tests.rs"]
mod matching_tests;
#[cfg(test)]
#[path = "../tests/narrowing_comprehensive_tests.rs"]
mod narrowing_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/narrowing_discriminant_tests.rs"]
mod narrowing_discriminant_tests;
#[cfg(test)]
#[path = "../tests/property_helpers_tests.rs"]
mod property_helpers_tests;
#[cfg(test)]
#[path = "tests/solver_file_size_ceiling_tests.rs"]
mod solver_file_size_ceiling_tests;
#[cfg(test)]
#[path = "../tests/string_intrinsic_subtype_tests.rs"]
mod string_intrinsic_subtype_tests;
#[cfg(test)]
#[path = "../tests/subtype_cache_tests.rs"]
mod subtype_cache_tests;
#[cfg(test)]
#[path = "../tests/template_literal_comprehensive_tests.rs"]
mod template_literal_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/tuple_comprehensive_tests.rs"]
mod tuple_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/type_parameter_comprehensive_tests.rs"]
mod type_parameter_comprehensive_tests;
#[cfg(test)]
#[path = "tests/type_queries_contextual_structure_tests.rs"]
mod type_queries_contextual_structure_tests;
#[cfg(test)]
#[path = "tests/type_queries_function_rewrite_tests.rs"]
mod type_queries_function_rewrite_tests;
#[cfg(test)]
#[path = "tests/type_queries_mapped_context_tests.rs"]
mod type_queries_mapped_context_tests;
#[cfg(test)]
#[path = "tests/type_queries_property_names_tests.rs"]
mod type_queries_property_names_tests;
#[cfg(test)]
#[path = "tests/type_queries_spread_tests.rs"]
mod type_queries_spread_tests;
#[cfg(test)]
#[path = "tests/typedata_contract_tests.rs"]
mod typedata_contract_tests;
#[cfg(test)]
#[path = "../tests/union_intersection_comprehensive_tests.rs"]
mod union_intersection_comprehensive_tests;
#[cfg(test)]
#[path = "../tests/variance_tests.rs"]
mod variance_tests;
#[cfg(test)]
#[path = "tests/visitor_tests.rs"]
mod visitor_tests;
