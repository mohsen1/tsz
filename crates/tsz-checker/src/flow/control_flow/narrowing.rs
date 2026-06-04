use tsz_binder::symbol_flags;

use tsz_common::interner::Atom;

use tsz_parser::parser::node::CallExprData;

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::narrowing::{GuardSense, TypeGuard};

use tsz_solver::{ParamInfo, TypeId, TypePredicate, TypePredicateTarget};

use super::{FlowAnalyzer, PredicateSignature};

use crate::query_boundaries::flow as flow_boundary;

use crate::query_boundaries::flow_analysis::{
    self as flow_query, PredicateSignatureKind, classify_for_predicate_signature,
    is_narrowing_literal, stringify_literal_type, union_members_for_type,
};

include!("narrowing_parts/part1.rs");
include!("narrowing_parts/part2.rs");
