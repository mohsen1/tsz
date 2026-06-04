use crate::query_boundaries::checkers::call as call_checker;

use crate::query_boundaries::common;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

use tsz_solver::computation::{ContextualTypeContext, TypeSubstitution};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextualPropertyPresence {
    Present,
    Absent,
    Unknown,
}

include!("object_literal_context_parts/part1.rs");
include!("object_literal_context_parts/part2.rs");
