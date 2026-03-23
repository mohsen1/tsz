//! Error reporting (`error_*` for emission, `report_*` for higher-level wrappers).
//! This module is split into focused submodules for maintainability.

/// Whether a type-only symbol came from `import type` or `export type`.
#[derive(Debug)]
pub(crate) enum TypeOnlyKind {
    Import,
    Export,
}

// Submodules
mod assignability;
mod assignability_helpers;
mod render_failure;
mod call_errors;
mod core;
mod core_formatting;
mod emitters;
mod fingerprint_policy;
mod generics;
mod name_resolution;
mod operator_errors;
mod properties;
mod suggestions;
mod type_value;

// Re-export known-global classifier from the canonical capabilities boundary.
pub(crate) use crate::query_boundaries::capabilities::is_known_dom_global;

#[cfg(test)]
#[path = "render_request_tests.rs"]
mod render_request_tests;
