use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy, Debug)]
struct SuperInitFlowState {
    super_called: bool,
    reachable: bool,
}

include!("scope_finder_contexts_parts/part1.rs");
include!("scope_finder_contexts_parts/part2.rs");
