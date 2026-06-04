use crate::query_boundaries::checkers::jsx as jsx_query;

use crate::query_boundaries::common::{
    PropertyAccessResult, array_element_type, tuple_elements, unwrap_readonly,
};

use crate::state::CheckerState;

use crate::symbol_resolver::TypeSymbolResolution;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use rustc_hash::FxHashSet;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

include!("children_parts/part1.rs");
include!("children_parts/part2.rs");

#[cfg(test)]
mod tests {
    #[test]
    fn jsx_children_display_policy_avoids_formatted_type_name_decisions() {
        let source = include_str!("children.rs");
        for forbidden in [
            ["format_type(type_id)", " == ", "\"ReactChild\""].join(""),
            ["format_type(actual_child_type)", " == ", "\"Element\""].join(""),
        ] {
            assert!(
                !source.contains(&forbidden),
                "JSX children display policy must use TypeId/query facts, \
                 not formatted type-name comparisons: found {forbidden}"
            );
        }
    }
}
