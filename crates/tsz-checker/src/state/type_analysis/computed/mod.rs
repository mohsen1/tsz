mod builtin_iterator_return_alias;

mod jsx_runtime_bridge;

mod simple_local_interface;

mod type_alias_merged_value;

mod type_alias_variable_alias;

use crate::query_boundaries::common::{contains_infer_types, contains_type_parameters};

struct SymbolAliasCtx<'a> {
    sym_id: SymbolId,
    flags: u32,
    value_decl: NodeIndex,
    declarations: &'a [NodeIndex],
    import_module: &'a Option<String>,
    import_name: &'a Option<String>,
    escaped_name: &'a str,
    factory: &'a tsz_solver::construction::TypeFactory<'a>,
}

use crate::query_boundaries::state::type_environment;

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_common::ModuleKind;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::{PropertyInfo, TypeId, Visibility};

include!("mod_parts/part1.rs");
include!("mod_parts/part2.rs");

#[cfg(test)]
mod tests;
