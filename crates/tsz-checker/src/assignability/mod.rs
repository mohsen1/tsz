//! Assignability, assignment, and subtype/identity checking.
//!
//! This module groups the three related concerns:
//! - `assignability_checker` — type assignability and excess property checking
//! - `assignment_checker` — assignment expression checking (=, +=, etc.)
//! - `subtype_identity_checker` — subtype, identity, and redeclaration compat

mod application_keyof_helpers;
pub mod assignability_checker;
mod assignability_diagnostics;
mod assignability_type_param_helpers;
pub mod assignment_checker;
mod awaited_variance_normalization;
pub(crate) mod compound_assignment;
mod index_access_normalization;
mod nullish_error_targets;
mod polymorphic_this_diagnostics;
mod readonly_tuple_diagnostics;
pub mod subtype_identity_checker;
