use super::super::Printer;

use crate::transforms::private_fields_es5::get_private_field_name;

use tsz_common::common::ModuleKind;

use tsz_parser::parser::{NodeIndex, node::Node, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

include!("call_parts/part1.rs");
include!("call_parts/part2.rs");
