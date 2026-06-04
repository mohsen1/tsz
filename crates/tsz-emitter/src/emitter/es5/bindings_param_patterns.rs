use super::super::{ParamTransformPlan, Printer};

use super::bindings_patterns::ES5RestProp;

use crate::transforms::emit_utils;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::{BindingElementData, ForInOfData, Node, NodeAccess};

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

include!("bindings_param_patterns_parts/part1.rs");
include!("bindings_param_patterns_parts/part2.rs");
