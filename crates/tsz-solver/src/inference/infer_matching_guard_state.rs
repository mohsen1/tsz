//! Named guard states for structural inference matching.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InferMatchEntryState {
    Entered { depth: u32 },
    DepthExceeded,
    AlreadyVisited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppExpansionState {
    Entered { depth: u32 },
    DepthExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetParamVisitState {
    Entered,
    AlreadyVisited { fallback: bool },
}

pub(crate) const fn infer_match_entry_state(
    depth: u32,
    max_depth: u32,
    inserted_visit: bool,
) -> InferMatchEntryState {
    if depth >= max_depth {
        InferMatchEntryState::DepthExceeded
    } else if !inserted_visit {
        InferMatchEntryState::AlreadyVisited
    } else {
        InferMatchEntryState::Entered { depth: depth + 1 }
    }
}

pub(crate) const fn app_expansion_state(depth: u32, max_depth: u32) -> AppExpansionState {
    if depth >= max_depth {
        AppExpansionState::DepthExceeded
    } else {
        AppExpansionState::Entered { depth: depth + 1 }
    }
}

pub(crate) const fn target_param_visit_state(inserted_visit: bool) -> TargetParamVisitState {
    if inserted_visit {
        TargetParamVisitState::Entered
    } else {
        TargetParamVisitState::AlreadyVisited { fallback: false }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppExpansionState, InferMatchEntryState, TargetParamVisitState, app_expansion_state,
        infer_match_entry_state, target_param_visit_state,
    };

    #[test]
    fn infer_match_entry_state_names_depth_and_revisit_cutoffs() {
        assert_eq!(
            infer_match_entry_state(20, 20, true),
            InferMatchEntryState::DepthExceeded
        );
        assert_eq!(
            infer_match_entry_state(0, 20, false),
            InferMatchEntryState::AlreadyVisited
        );
        assert_eq!(
            infer_match_entry_state(0, 20, true),
            InferMatchEntryState::Entered { depth: 1 }
        );
    }

    #[test]
    fn app_expansion_state_names_depth_cutoff() {
        assert_eq!(app_expansion_state(8, 8), AppExpansionState::DepthExceeded);
        assert_eq!(
            app_expansion_state(7, 8),
            AppExpansionState::Entered { depth: 8 }
        );
    }

    #[test]
    fn target_param_visit_state_names_cycle_cutoff() {
        assert_eq!(
            target_param_visit_state(true),
            TargetParamVisitState::Entered
        );
        assert_eq!(
            target_param_visit_state(false),
            TargetParamVisitState::AlreadyVisited { fallback: false }
        );
    }
}
