use super::super::Printer;

use super::super::get_trailing_comment_ranges;

use super::super::hoist_anchor::HoistAnchor;

use crate::safe_slice;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::Node;

use tsz_parser::parser::node_flags;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
