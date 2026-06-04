use crate::query_boundaries::checkers::jsx as jsx_boundary;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_solver::TypeId;

use tsz_solver::computation::TypeResolver;

include!("extraction_parts/part1.rs");
include!("extraction_parts/part2.rs");
