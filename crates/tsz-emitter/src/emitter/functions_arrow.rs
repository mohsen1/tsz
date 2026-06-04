use super::Printer;

use tsz_common::ScriptTarget;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::{Node, NodeAccess};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::syntax::transform_utils::{
    contains_arguments_reference, contains_this_reference, is_private_identifier,
};

use tsz_scanner::SyntaxKind;

enum NativeArrowParamPrologueEntry {
    Default {
        name: String,
        initializer: NodeIndex,
    },
    Binding {
        pattern: NodeIndex,
        temp_name: String,
        initializer: NodeIndex,
    },
}

#[derive(Clone, Copy)]
struct AsyncArrowGeneratorHoistStart {
    anchor: super::hoist_anchor::HoistAnchor,
    assignment_start: usize,
    for_of_start: usize,
    value_start: usize,
}

include!("functions_arrow_parts/part1.rs");
include!("functions_arrow_parts/part2.rs");
