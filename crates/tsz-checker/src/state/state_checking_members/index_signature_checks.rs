use crate::query_boundaries::flow_analysis as flow_query;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

include!("index_signature_checks_parts/part1.rs");
include!("index_signature_checks_parts/part2.rs");
