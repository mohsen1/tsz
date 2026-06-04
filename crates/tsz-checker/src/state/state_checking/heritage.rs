use crate::query_boundaries::class_type as class_query;

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use rustc_hash::FxHashSet;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

include!("heritage_parts/part1.rs");
include!("heritage_parts/part2.rs");

const fn find_heritage_call_expression_type_argument_anchor_impl(
    call_expr_start: u32,
    explicit_type_arg_start: Option<u32>,
    fallback_start: u32,
) -> u32 {
    if explicit_type_arg_start.is_some() {
        call_expr_start
    } else {
        fallback_start
    }
}

#[cfg(test)]
mod tests {
    use super::find_heritage_call_expression_type_argument_anchor_impl;

    #[test]
    fn test_prefers_explicit_type_argument_node_start() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, Some(23), 5);
        assert_eq!(anchor, 15);
    }

    #[test]
    fn test_falls_back_to_call_start_when_source_text_missing() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(26, Some(2), 5);
        assert_eq!(anchor, 26);
    }

    #[test]
    fn test_falls_back_to_call_start_without_type_arguments() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, None, 7);
        assert_eq!(anchor, 7);
    }
}
