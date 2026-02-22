//! Assignability, assignment, and subtype/identity checking.
//!
//! This module groups the three related concerns:
//! - `assignability_checker` — type assignability and excess property checking
//! - `assignment_checker` — assignment expression checking (=, +=, etc.)
//! - `subtype_identity_checker` — subtype, identity, and redeclaration compat

pub mod assignability_checker;
pub mod assignment_checker;
pub mod subtype_identity_checker;
