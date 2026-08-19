//! Checker-facing query boundaries over solver semantics.
//!
//! Checker code should call these modules when it needs semantic facts from the
//! solver. The checker owns source context, request construction, diagnostics
//! orchestration, and spans; the solver owns low-level type representation,
//! relation policy, evaluation, and semantic caches.
//!
//! Boundary modules should expose stable, request-shaped APIs where possible.
//! Compatibility shims may remain while callers migrate, but temporary wrappers
//! around `tsz_solver::type_queries::data::*` are quarantine helpers: do not add
//! new direct data access unless the PR also names the stable solver query that
//! will replace it. The current module inventory and quarantine list live in
//! `docs/architecture/QUERY_BOUNDARY_INVENTORY.md`.
//!
pub(crate) mod apparent_type;
pub(crate) mod application_keyof;
pub(crate) mod assignability;
pub(crate) mod assignability_alias_display;
pub(crate) mod assignability_did_you_mean;
pub(crate) mod assignability_suppression;
pub(crate) mod binding_patterns;
pub(crate) mod capabilities;
pub(crate) mod checkers;
pub(crate) mod class;
pub(crate) mod class_type;
pub(crate) mod common;
pub(crate) mod comparability;
pub(crate) mod conditional;
pub(crate) mod conditional_constraints;
pub(crate) mod conditional_infer_alias;
pub(crate) mod construct_signatures;
pub(crate) mod containment_queries;
pub(crate) mod declaration_exports;
pub(crate) mod definite_assignment;
pub(crate) mod definition_identity;
pub(crate) mod diagnostics;
pub(crate) mod dispatch;
pub(crate) mod enum_analysis;
pub(crate) mod environment;
pub(crate) mod exact_rewrite;
pub(crate) mod flow;
pub(crate) mod flow_analysis;
pub(crate) mod function_returns;
pub(crate) mod generic_instantiation;
pub(crate) mod import_attributes;
pub(crate) mod index_signature;
pub(crate) mod indexed_access_key_space;
pub(crate) mod inference;
pub(crate) mod interface_merge;
pub(crate) mod intersection_display;
pub(crate) mod js_exports;
mod js_exports_json;
pub(crate) mod js_exports_named_class;
pub(crate) mod jsdoc_construction;
pub(crate) mod key_constraints;
pub(crate) mod lib_augmentations;
pub(crate) mod module_augmentation;
pub(crate) mod name_resolution;
pub(crate) mod object_literal_context;
pub(crate) mod operator_wrappers;
pub(crate) mod optional_chain;
pub(crate) mod property_access;
pub(crate) mod recursive_alias;
pub(crate) mod relation_policy;
pub(crate) mod relation_request;
pub(crate) mod relation_types;
pub(crate) mod shape_predicates;
pub(crate) mod signature_building;
pub(crate) mod spread;
pub(crate) mod state;
pub(crate) mod type_checking;
pub(crate) mod type_checking_utilities;
pub(crate) mod type_computation;
pub(crate) mod type_construction;
pub(crate) mod type_defaults;
pub(crate) mod type_origin;
pub(crate) mod type_parameter_identity;
pub(crate) mod type_predicates;
pub(crate) mod type_query_construction;
pub(crate) mod type_rewrite;
pub(crate) mod variance;
pub(crate) mod widening;
