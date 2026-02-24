//! Type checking queries for `CheckerState`.
//!
//! This module group handles type-checking query methods:
//! - `core` — modifier, member access, and general query methods
//! - `binding` — type inference from binding patterns
//! - `class` — type parameter scope, function implementation, class member analysis
//! - `lib` — library type resolution, namespace/alias utilities
//! - `lib_prime` — supplementary lib type resolution helpers
//! - `lib_resolution` — lib interface heritage resolution
//! - `type_only` — type-only symbol detection

pub(crate) mod binding;
pub(crate) mod class;
pub(crate) mod core;
pub(crate) mod lib;
pub(crate) mod lib_prime;
pub(crate) mod lib_resolution;
pub(crate) mod type_only;
