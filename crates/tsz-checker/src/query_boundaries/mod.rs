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
// Allow dead code and related lints in scaffolding modules that define
// the unified relation boundary API (NORTH_STAR.md §22). These types
// will be wired in as checker paths migrate to the boundary API.
#[allow(dead_code, clippy::missing_const_for_fn, clippy::match_same_arms)]
pub(crate) mod assignability;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod capabilities;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod checkers;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod class;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod class_type;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod common;
pub(crate) mod construct_signatures;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod definite_assignment;
pub(crate) mod definition_identity;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod diagnostics;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod dispatch;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
#[allow(private_interfaces)]
pub(crate) mod environment;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod flow;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod flow_analysis;
#[allow(dead_code)]
pub(crate) mod index_signature;
#[allow(dead_code)]
pub(crate) mod inference;
pub(crate) mod intersection_display;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod js_exports;
pub(crate) mod key_constraints;
#[allow(dead_code)]
pub(crate) mod name_resolution;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod property_access;
pub(crate) mod recursive_alias;
#[allow(dead_code, clippy::missing_const_for_fn, clippy::match_same_arms)]
pub(crate) mod relation_types;
pub(crate) mod spread;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod state;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_checking;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_checking_utilities;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_computation;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_construction;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_defaults;
pub(crate) mod type_origin;
pub(crate) mod type_parameter_identity;
pub(crate) mod type_predicates;
#[allow(dead_code, clippy::missing_const_for_fn, clippy::match_same_arms)]
pub(crate) mod type_rewrite;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod variance;
#[allow(dead_code)]
pub(crate) mod widening;
