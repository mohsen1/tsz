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
mod call_errors;
mod core;
mod generics;
mod name_resolution;
mod operator_errors;
mod properties;
mod suggestions;
mod type_value;

// Re-export known-global classifier used by types/computation/identifier.rs
pub(crate) use name_resolution::is_known_dom_global;
