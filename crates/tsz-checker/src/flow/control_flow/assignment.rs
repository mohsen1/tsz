use super::{FlowAnalyzer, PropertyKey};

use crate::query_boundaries::common::{
    is_assignment_operator as boundary_is_assignment_operator, is_compound_assignment_operator,
    is_logical_compound_assignment_operator, map_compound_assignment_to_binary,
};

use crate::query_boundaries::flow_analysis::{
    array_type, fallback_compound_assignment_result, get_array_element_type,
    tuple_elements_for_type, union_members_for_type, widen_literal_to_primitive,
};

use crate::query_boundaries::type_computation::core::BinaryOpResult;

use rustc_hash::FxHashSet;

use tsz_common::interner::Atom;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::{TupleElement, TypeId};

#[derive(Clone, Copy, Debug)]
struct DestructuringSource {
    node: NodeIndex,
    ty: TypeId,
}

include!("assignment_parts/part1.rs");
include!("assignment_parts/part2.rs");
