use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

mod indexed_access_helpers;

mod mapped_key_check;

use indexed_access_helpers::{
    generic_constrained_index, indexed_access_object_alias_application_exceeds_depth,
    is_broad_index_type, remapped_mapped_type_template_index_should_report_ts2536,
    same_object_key_space, same_type_param_name,
};

include!("indexed_access_parts/part1.rs");
include!("indexed_access_parts/part2.rs");
