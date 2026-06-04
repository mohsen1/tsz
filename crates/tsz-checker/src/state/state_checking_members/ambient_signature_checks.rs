use crate::context::{TypingRequest, speculation::DiagnosticSpeculationSnapshot};

use crate::query_boundaries::common::ContextualTypeContext;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

include!("ambient_signature_checks_parts/part1.rs");
include!("ambient_signature_checks_parts/part2.rs");
