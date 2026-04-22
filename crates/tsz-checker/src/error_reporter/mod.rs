//! Error reporting (`error_*` for emission, `report_*` for higher-level wrappers).
//! This module is split into focused submodules for maintainability.

/// Whether a type-only symbol came from `import type` or `export type`.
#[derive(Debug)]
pub(crate) enum TypeOnlyKind {
    Import,
    Export,
}

// Submodules
pub(crate) mod assignability;
mod assignability_helpers;
mod call_errors;
mod call_errors_anchors;
mod core;
mod core_formatting;
mod emitters;
mod fingerprint_policy;
mod generics;
mod name_resolution;
mod operator_errors;
mod properties;
mod render_failure;
mod suggestions;
mod type_display_policy;
mod type_value;

pub(crate) use fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
    ResolvedDiagnosticAnchor,
};

#[cfg(test)]
#[path = "render_request_tests.rs"]
mod render_request_tests;
