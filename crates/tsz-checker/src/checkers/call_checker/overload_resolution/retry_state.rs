//! Shared retry-state helpers for overload resolution.

use crate::context::speculation::FullSnapshot;

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
