use super::CallableContext;

use crate::computation::complex::is_contextually_sensitive;

use crate::context::TypingRequest;

use crate::context::speculation::DiagnosticSpeculationSnapshot;

use crate::diagnostics::diagnostic_codes;

use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, contains_index_access_with_type_parameter_object,
    contains_index_access_with_variadic_tuple_object, is_type_parameter_type,
    tuple_elements_for_type, tuple_slice_variable_rest_offset,
};

use crate::query_boundaries::common::ContextualTypeContext;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::Node;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::{TupleElement, TypeId};

const SPREAD_ARGUMENT_MARKER_NAME: &str = "__tsz_spread_argument__";

include!("candidate_collection_parts/part1.rs");
include!("candidate_collection_parts/part2.rs");

#[cfg(test)]
#[path = "candidate_collection_tests.rs"]
mod tests;
