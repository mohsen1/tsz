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
mod assignability_generic;
mod assignability_helpers;
mod assignability_keyof_alias_display;
mod assignability_literal_display;
mod assignability_missing_property_satisfaction;
mod assignability_normalized_union;
mod assignability_numeric_display;
mod assignability_satisfies;
mod assignability_type_helpers;
mod assignability_type_parameter_target;
mod async_suggestion;
mod call_errors;
mod call_errors_anchors;
mod conditional_alias_display;
mod core;
mod core_alias_display;
mod core_formatting;
pub(crate) mod display_budget;
mod emitters;
mod enum_nominal_name_display;
mod expected_type_from_property;
mod expected_type_from_return;
mod fingerprint_policy;
mod generic_display_helpers;
mod generics;
mod intersection_never_elaboration;
mod literal_alias_display;
mod literal_alias_rewrites;
mod missing_property_declared_here;
mod name_resolution;
mod noinfer_diagnostic_display;
pub(crate) mod operator_errors;
mod primitive_intersection_display;
mod properties;
mod property_receiver_formatting;
mod recursive_alias_display;
mod render_failure;
mod suggestions;
mod token_anchors;
mod ts2820_display;
pub(crate) mod type_display_policy;
mod type_query_alias_display;
mod type_value;

pub(crate) use fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
    ResolvedDiagnosticAnchor,
};

#[cfg(test)]
#[path = "fingerprint_policy_tests.rs"]
mod fingerprint_policy_tests;

#[cfg(test)]
#[path = "render_request_tests.rs"]
mod render_request_tests;

#[cfg(test)]
#[path = "tuple_annotation_display_tests.rs"]
mod tuple_annotation_display_tests;
