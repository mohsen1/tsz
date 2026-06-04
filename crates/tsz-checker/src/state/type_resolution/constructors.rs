use crate::query_boundaries::state::type_resolution as query;

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol;

use tsz_common::interner::Atom;

use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::computation::TypeSubstitution;

mod callable_type_arguments;

mod heritage_call_returns;

pub(super) const fn should_cache_base_expr_result(
    type_argument_count: usize,
    has_active_type_parameter_scope: bool,
) -> bool {
    type_argument_count == 0 && !has_active_type_parameter_scope
}

include!("constructors_parts/part1.rs");
include!("constructors_parts/part2.rs");
