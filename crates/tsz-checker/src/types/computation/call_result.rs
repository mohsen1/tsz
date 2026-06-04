use crate::query_boundaries::assignability as assign_query;

use crate::query_boundaries::common;

use crate::query_boundaries::common::CallResult;

use crate::query_boundaries::type_computation::core as expr_ops;

use crate::state::CheckerState;

use rustc_hash::FxHashSet;

use tsz_common::diagnostics::{diagnostic_codes, format_message};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::computation::TypeResolver;

use tsz_solver::{ParamInfo, TupleElement, TypeId};

pub(super) struct CallResultContext<'a> {
    pub(super) callee_expr: NodeIndex,
    pub(super) call_idx: NodeIndex,
    pub(super) args: &'a [NodeIndex],
    pub(super) arg_types: &'a [TypeId],
    pub(super) callee_type: TypeId,
    pub(super) callee_has_declared_generic_signature: bool,
    pub(super) is_super_call: bool,
    pub(super) is_optional_chain: bool,
    pub(super) allow_contextual_mismatch_deferral: bool,
}

include!("call_result_parts/part1.rs");
include!("call_result_parts/part2.rs");
