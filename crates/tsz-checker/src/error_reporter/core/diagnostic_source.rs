mod assignment_formatting;

mod assignment_source_preservation;

mod compound_assignment_context;

mod computed_index_source_display;

mod contextual_index_display;

mod generic_source_display;

mod literal_surface;

mod literal_widening_helpers;

mod literal_widening_policy;

mod object_literal_targets;

mod recursive_alias_display;

mod static_schema;

mod tuple_source_display;

mod type_query_alias;

mod wrapper_provenance;

use crate::diagnostics::diagnostic_codes;

use crate::query_boundaries::diagnostics as diagnostic_query;

use crate::state::CheckerState;

use crate::types_domain::type_node_helpers::type_node_includes_explicit_undefined;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

include!("diagnostic_source_parts/part1.rs");
include!("diagnostic_source_parts/part2.rs");

/// Strip TS-family file extensions from module specifiers for display while
/// preserving JS-family extensions in `typeof import("mod")` output.
/// Element-access diagnostics can opt into raw namespace display earlier.
pub(crate) fn strip_module_specifier_extension(module_name: &str) -> &str {
    tsz_common::file_extensions::strip_ts_extension(module_name)
}
