//! Shared retry-state helpers for overload resolution.

use crate::context::speculation::FullSpeculationSnapshot;
use crate::query_boundaries::common::CallResult;
use crate::{context::NodeTypeCache, diagnostics::Diagnostic};
use tsz_solver::TypeId;

use super::super::{OverloadResolution, SelectedTypePredicate};

pub(super) type NoReturnContextFallback = (
    Vec<tsz_solver::TypeId>,
    tsz_solver::TypeId,
    SelectedTypePredicate,
    FullSpeculationSnapshot,
);

pub(super) type BestTypeMismatch = (OverloadResolution, NodeTypeCache, Vec<Diagnostic>);

pub(super) const fn best_this_type_mismatch(
    arg_types: Vec<TypeId>,
    expected_this: TypeId,
    actual_this: TypeId,
    emit_not_callable: bool,
    node_types: NodeTypeCache,
) -> BestTypeMismatch {
    (
        OverloadResolution {
            arg_types,
            result: CallResult::ThisTypeMismatch {
                expected_this,
                actual_this,
                emit_not_callable,
            },
            selected_type_predicate: None,
        },
        node_types,
        Vec::new(),
    )
}
