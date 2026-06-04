//! Continuation of `compute_type_of_symbol`: type alias, class property, variable,
//! and alias symbol resolution.

include!(
    "type_alias_variable_alias_large_methods/compute_type_of_symbol_type_alias_variable_alias_12_0.rs"
);

use super::SymbolAliasCtx;
use crate::query_boundaries::common::{array_element_type, is_generic_type};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::type_environment;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, Visibility};

include!("type_alias_variable_alias_helpers.rs");

impl<'a> CheckerState<'a> {
    __tsz_split_type_alias_variable_alias_compute_type_of_symbol_type_alias_variable_alias_12_0!();
}
