use super::{HoverInfo, HoverProvider, format};

use crate::jsdoc::{format_inline_code, inline_links, jsdoc_for_node, parse_jsdoc};

use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};

use crate::utils::{
    find_symbol_query_node_at_or_before, is_comment_context, should_backtrack_to_previous_symbol,
};

use tsz_checker::state::CheckerState;

use tsz_common::position::Range;

use tsz_parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");

/// Pick a fence length for an `@example` code block. `CommonMark` §4.5 requires
/// the closing fence to be at least as long as the opening fence, so the
/// returned length must exceed every backtick-only line prefix inside `text`.
/// The minimum is three to match the conventional ` ``` ` fence.
fn pick_example_fence_length(text: &str) -> usize {
    let longest_inner_fence = text
        .lines()
        .map(|line| line.chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or(0);
    (longest_inner_fence + 1).max(3)
}
