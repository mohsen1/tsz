//! Assignability, assignment, and subtype/identity checking.
//!
//! This module groups the three related concerns:
//! - `assignability_checker` — type assignability and excess property checking
//! - `assignment_checker` — assignment expression checking (=, +=, etc.)
//! - `subtype_identity_checker` — subtype, identity, and redeclaration compat

mod application_keyof_helpers;
pub mod assignability_checker;
mod assignability_diagnostics;
mod assignability_eval;
mod assignability_relation;
mod assignability_type_param_helpers;
pub mod assignment_checker;
mod awaited_variance_normalization;
mod cached_constraint_relation_helpers;
mod callable_union_relation;
pub(crate) mod compound_assignment;
mod conditional_infer_alias_helpers;
mod constrained_type_param_assertion;
mod failure_memo;
mod generic_mapped_alias_helpers;
mod index_access_normalization;
mod nullish_error_targets;
mod overload_subtype_pass;
mod polymorphic_this_diagnostics;
mod provisional_rest_union;
mod readonly_tuple_diagnostics;
mod relation_outcome_helpers;
pub mod subtype_identity_checker;
mod typeof_this_guard;
