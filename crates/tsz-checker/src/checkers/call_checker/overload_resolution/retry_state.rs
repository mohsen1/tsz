//! Shared retry-state helpers for overload resolution.

use crate::context::speculation::FullSnapshot;
use crate::state::CheckerState;

use super::super::{OverloadResolution, SelectedTypePredicate};

pub(super) type NoReturnContextFallback = (
    Vec<tsz_solver::TypeId>,
    tsz_solver::TypeId,
    SelectedTypePredicate,
    FullSnapshot,
);

pub(super) type BestTypeMismatch = (
    OverloadResolution,
    crate::context::NodeTypeCache,
    Vec<crate::diagnostics::Diagnostic>,
);

impl<'a> CheckerState<'a> {
    pub(super) fn snapshot_overload_retry_state(&mut self) -> FullSnapshot {
        self.ctx.snapshot_full()
    }

    pub(super) fn rollback_overload_retry_state(&mut self, snap: &FullSnapshot) {
        self.ctx.rollback_full(snap);
    }
}
