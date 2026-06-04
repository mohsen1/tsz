use super::super::{Printer, get_trailing_comment_ranges};

use tsz_parser::parser::node::Node;

use tsz_parser::parser::node_flags;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

struct RecoveredSwitchClass {
    header: String,
    inline_body: Option<String>,
}

include!("control_flow_parts/part1.rs");
include!("control_flow_parts/part2.rs");
