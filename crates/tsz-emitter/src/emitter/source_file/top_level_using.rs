use super::super::Printer;

use super::top_level_using_decorated::{export_decorate_assignment, strip_decorate_export_prefix};

use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter, emit_utils};

use rustc_hash::FxHashSet;

use tsz_common::common::ModuleKind;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::NodeList;

use tsz_parser::parser::node::{ClassData, Node, NodeAccess};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::syntax::transform_utils::is_private_identifier;

use tsz_scanner::SyntaxKind;

#[path = "top_level_using_analysis.rs"]
mod top_level_using_analysis;

include!("top_level_using_parts/part1.rs");
include!("top_level_using_parts/part2.rs");
