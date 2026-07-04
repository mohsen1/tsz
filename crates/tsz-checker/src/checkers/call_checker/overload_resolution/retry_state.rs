//! Shared retry-state helpers for overload resolution.

use crate::context::speculation::FullSpeculationSnapshot;

use super::super::{OverloadResolution, SelectedTypePredicate};

pub(super) type NoReturnContextFallback = (
    Vec<tsz_solver::TypeId>,
    tsz_solver::TypeId,
    SelectedTypePredicate,
    FullSpeculationSnapshot,
);

pub(super) type BestTypeMismatch = (
    OverloadResolution,
    crate::context::NodeTypeCache,
    Vec<crate::diagnostics::Diagnostic>,
);
