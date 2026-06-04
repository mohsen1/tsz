use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;

use crate::query_boundaries::common as query_common;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::computation::ContextualTypeContext;

include!("elaboration_object_properties_parts/part1.rs");
include!("elaboration_object_properties_parts/part2.rs");
