use crate::query_boundaries::assignability::{
    AssignabilityEvalKind, classify_for_assignability_eval, contains_free_infer_types,
    get_keyof_type, get_string_literal_value, get_union_members, is_type_parameter_like,
    keyof_object_properties, map_compound_members,
};

use crate::query_boundaries::common::{collect_lazy_def_ids, collect_type_queries};

use crate::state::CheckerState;

use rustc_hash::FxHashSet;

use tsz_common::interner::Atom;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::narrowing::NarrowingContext;

include!("assignability_checker_parts/part1.rs");
include!("assignability_checker_parts/part2.rs");

/// A target signature can supply contextual types for `source_param_count`
/// callback parameters when it has a rest parameter (which absorbs any
/// trailing positions) or its fixed parameter list is at least that long.
fn signature_has_param_capacity(
    params: &[tsz_solver::ParamInfo],
    source_param_count: usize,
) -> bool {
    if params.iter().any(|p| p.rest) {
        return true;
    }
    params.len() >= source_param_count
}
