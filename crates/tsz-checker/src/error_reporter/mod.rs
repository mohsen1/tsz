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
mod assignability_alias_display;
mod assignability_anchor_helpers;
mod assignability_callable_suppression;
mod assignability_contextual_display;
mod assignability_enum_display;
mod assignability_exact_optional;
mod assignability_helpers;
mod assignability_keyof_alias_display;
mod assignability_literal_display;
mod assignability_normalized_union;
mod assignability_numeric_display;
mod assignability_type_helpers;
mod call_errors;
mod call_errors_anchors;
mod core;
mod core_formatting;
mod emitters;
mod fingerprint_policy;
mod generic_display_helpers;
mod generics;
mod literal_alias_display;
mod literal_alias_rewrites;
mod name_resolution;
mod operator_errors;
mod primitive_intersection_display;
mod properties;
mod property_receiver_formatting;
mod recursive_alias_display;
mod render_failure;
mod suggestions;
mod ts2820_display;
mod type_display_policy;
mod type_query_alias_display;
mod type_value;

pub(crate) use fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
    ResolvedDiagnosticAnchor,
};

#[cfg(test)]
#[path = "render_request_tests.rs"]
mod render_request_tests;
