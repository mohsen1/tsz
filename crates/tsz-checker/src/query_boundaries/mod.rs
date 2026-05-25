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
pub(crate) mod assignability;
pub(crate) mod assignability_alias_display;
pub(crate) mod capabilities;
pub(crate) mod checkers;
pub(crate) mod class;
pub(crate) mod class_type;
pub(crate) mod common;
pub(crate) mod construct_signatures;
pub(crate) mod definite_assignment;
pub(crate) mod definition_identity;
pub(crate) mod diagnostics;
pub(crate) mod dispatch;
pub(crate) mod enum_analysis;
pub(crate) mod environment;
pub(crate) mod flow;
pub(crate) mod flow_analysis;
pub(crate) mod function_returns;
pub(crate) mod index_signature;
pub(crate) mod inference;
pub(crate) mod intersection_display;
pub(crate) mod js_exports;
pub(crate) mod key_constraints;
pub(crate) mod name_resolution;
pub(crate) mod operator_wrappers;
pub(crate) mod property_access;
pub(crate) mod recursive_alias;
pub(crate) mod relation_types;
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
pub(crate) mod type_rewrite;
pub(crate) mod variance;
pub(crate) mod widening;
