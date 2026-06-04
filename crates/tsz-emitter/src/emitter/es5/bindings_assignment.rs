use super::super::{Printer, is_valid_identifier_name};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::Node;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

enum AssignmentRestProp {
    Static(String),
    Dynamic(String),
}

include!("bindings_assignment_parts/part1.rs");
include!("bindings_assignment_parts/part2.rs");
