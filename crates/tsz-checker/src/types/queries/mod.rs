//! Type checking queries for `CheckerState`.
//!
//! This module group handles type-checking query methods:
//! - `binding` — type inference from binding patterns
//! - `callable_truthiness` — TS2774/TS2872/TS2873 truthiness and callable checks
//! - `class` — type parameter scope, function implementation, class member analysis
//! - `core` — modifier, member access, and general query methods
//! - `lib` — library type resolution, namespace/alias utilities
//! - `lib_prime` — supplementary lib type resolution helpers
//! - `lib_resolution` — lib interface heritage resolution
//! - `type_only` — type-only symbol detection

pub(crate) mod binding;
pub(crate) mod callable_truthiness;
pub(crate) mod class;
pub(crate) mod core;
pub(crate) mod lib;
mod lib_name_text;
pub(crate) mod lib_prime;
pub(crate) mod lib_resolution;
pub(crate) mod lib_scoped_heritage;
pub(crate) mod type_only;
