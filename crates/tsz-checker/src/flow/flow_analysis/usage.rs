use crate::FlowAnalyzer;

use crate::control_flow::type_guards::reference_uses_outer_class_property_initializer_binding;

use crate::query_boundaries::definite_assignment::should_report_variable_use_before_assignment;

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};

use tsz_binder::SymbolId;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("usage_parts/part1.rs");
include!("usage_parts/part2.rs");
