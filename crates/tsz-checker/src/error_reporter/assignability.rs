use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};

use crate::error_reporter::assignability_literal_display::display_has_boolean_member_literal_assignability;

use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};

use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;

use crate::state::CheckerState;

use tracing::{Level, trace};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

pub(crate) use super::assignability_type_helpers::{
    display_is_literal_value, is_primitive_type_name, is_reserved_type_name,
};

pub(super) use super::assignability_type_helpers::{
    has_own_signature_type_params, is_builtin_wrapper_name, is_callable_application_type,
    is_function_like_for_literal_member_widening, is_object_prototype_method,
    is_object_prototype_method_for_array_target,
};

include!("assignability_parts/part1.rs");
include!("assignability_parts/part2.rs");
