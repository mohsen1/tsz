use crate::context::TypingRequest;

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

use crate::state::CheckerState;

use crate::state_checking::readonly::ReadonlyAssignmentDiagnostic;

use tsz_binder::symbol_flags;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("assignment_ops_parts/part1.rs");
include!("assignment_ops_parts/part2.rs");
