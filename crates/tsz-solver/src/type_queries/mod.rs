//! Type Query Functions
//!
//! This module provides high-level query functions for inspecting type characteristics.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for the checker to query type properties.
//!
//! # Design Principles
//!
//! - **Abstraction**: Checker code should use these functions instead of matching on `TypeData`
//! - **TypeDatabase-based**: All queries work through the `TypeDatabase` trait
//! - **Comprehensive**: Covers all common type checking scenarios
//! - **Efficient**: Simple lookups with minimal overhead
//!
//! # Usage
//!
//! ```text
//! use crate::type_queries::*;
//!
//! // Check if a type is callable
//! if is_callable_type(&db, type_id) {
//!     // Handle callable type
//! }
//!
//! // Check if a type is a tuple
//! if is_tuple_type(&db, type_id) {
//!     // Handle tuple type
//! }
//! ```

pub mod assertion_overlap;
pub mod classifiers;
pub mod conditional_infer_alias;
mod core;
pub mod data;
mod declaration_walk_guard_state;
pub mod declaration_walks;
pub mod extended;
pub mod extended_constructors;
pub mod flow;
pub mod global_interfaces;
pub mod iterable;
pub mod mapped;
pub mod mapped_declaration_surface;
pub mod mapped_display_order;
pub mod predicate_narrowing;
pub mod shape_queries;
pub mod traversal;
pub mod ts7_sort_order;

// Re-export shared predicates from visitor_predicates to avoid duplication.
// These are the canonical implementations; type_queries re-exports them so
// callers can use a single `type_queries::*` import.
pub use crate::visitors::visitor_predicates::{
    contains_any_type, is_array_type, is_conditional_type, is_empty_object_type, is_function_type,
    is_generic_application, is_index_access_type, is_intersection_type, is_literal_type,
    is_mapped_type, is_object_like_type, is_primitive_type, is_template_literal_type,
    is_tuple_type, is_type_query_type, is_union_type,
};

// Re-export sub-module items so callers can use `type_queries::*`
pub use classifiers::{
    AssignabilityEvalKind, AugmentationTargetKind, ConstructorAccessKind, ExcessPropertiesKind,
    InterfaceMergeKind, classify_for_assignability_eval, classify_for_augmentation,
    classify_for_constructor_access, classify_for_excess_properties, classify_for_interface_merge,
    get_application_lazy_def_id, get_conditional_type_id, get_keyof_type, get_lazy_def_id,
    get_mapped_type_id, get_type_query_symbol_ref, indexed_access_self_keyof,
    is_compound_this_relative_surface_type, is_deferred_indexed_access_or_intersection_with_one,
    is_deferred_lazy_or_indexed_access, is_deferred_type_operation,
    is_distributive_conditional_with_deferred_check, is_indexed_access, is_only_false_or_never,
};
pub use conditional_infer_alias::*;
// `get_def_id` is an alias for `get_lazy_def_id` (identical semantics).
pub use classifiers::get_lazy_def_id as get_def_id;
pub use extended::get_application_info;
pub use extended::{
    ArrayLikeKind, CallSignaturesKind, ContextualLiteralAllowKind, ElementIndexableKind,
    IndexKeyKind, LazyTypeKind, LiteralKeyKind, LiteralTypeKind, MappedConstraintKind,
    NamespaceMemberKind, PromiseTypeKind, PropertyAccessResolutionKind, StringLiteralKeyKind,
    TypeArgumentExtractionKind, TypeQueryKind, TypeResolutionKind, are_same_base_literal_kind,
    classify_array_like, classify_element_indexable, classify_element_indexable_with_resolver,
    classify_for_call_signatures, classify_for_contextual_literal, classify_for_lazy_resolution,
    classify_for_property_access_resolution, classify_for_string_literal_keys,
    classify_for_type_argument_extraction, classify_for_type_resolution, classify_index_key,
    classify_literal_key, classify_literal_type, classify_mapped_constraint,
    classify_namespace_member, classify_promise_type, classify_type_query,
    create_number_literal_type, create_string_literal_type, get_application_base,
    get_invalid_index_type_member, get_invalid_index_type_member_strict, get_literal_property_name,
    get_number_literal_value, get_string_literal_value, get_tuple_list_id, is_literal_enum_member,
    is_number_literal, is_object_with_index_type, is_string_like_type, is_string_literal,
    is_type_or_base_identical, key_matches_number_index, key_matches_string_index,
    source_is_or_base_identical_to_fixed, string_like_type_for_type, type_contains_string_literal,
    type_contains_unit_literal, widen_literal_to_primitive,
};
pub use extended_constructors::{
    AbstractClassCheckKind, AbstractConstructorAnchor, BaseInstanceMergeKind, ClassDeclTypeKind,
    ConstructorCheckKind, ConstructorReturnMergeKind, InstanceTypeKind,
    classify_for_abstract_check, classify_for_base_instance_merge, classify_for_class_decl,
    classify_for_constructor_check, classify_for_constructor_return_merge,
    classify_for_instance_type, get_base_construct_return_type,
    resolve_abstract_constructor_anchor,
};

pub use data::*;
pub use declaration_walks::*;
pub use flow::*;
pub use global_interfaces::*;
pub use iterable::*;
pub use mapped::*;
pub use mapped_declaration_surface::*;
pub use mapped_display_order::*;
pub use predicate_narrowing::*;
pub use shape_queries::*;
pub use traversal::*;

// Re-export core implementation
pub use self::core::*;
