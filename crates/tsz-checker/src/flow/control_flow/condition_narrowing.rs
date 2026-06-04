use super::FlowAnalyzer;

use crate::query_boundaries::flow as flow_boundary;

use crate::query_boundaries::flow_analysis::{
    empty_object_type, is_union_type, is_unit_type, is_unknown_narrowing_literal,
};

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_binder::{FlowNodeId, SymbolId, symbol_flags};

use tsz_parser::parser::node::BinaryExprData;

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::narrowing::{GuardSense, NarrowingContext, TypeGuard, TypeofKind};

include!("condition_narrowing_parts/part1.rs");
include!("condition_narrowing_parts/part2.rs");
