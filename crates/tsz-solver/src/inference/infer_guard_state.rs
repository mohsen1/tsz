//! Named guard states for inference constraint walks.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TypeGraphVisitState {
    Entered,
    AlreadyVisited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParamDependencyState {
    TargetReached,
    Entered,
    AlreadyVisited,
}

pub(crate) const fn type_graph_visit_state(inserted_visit: bool) -> TypeGraphVisitState {
    if inserted_visit {
        TypeGraphVisitState::Entered
    } else {
        TypeGraphVisitState::AlreadyVisited
    }
}

pub(crate) const fn param_dependency_state(
    is_target: bool,
    inserted_visit: bool,
) -> ParamDependencyState {
    if is_target {
        ParamDependencyState::TargetReached
    } else if inserted_visit {
        ParamDependencyState::Entered
    } else {
        ParamDependencyState::AlreadyVisited
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ParamDependencyState, TypeGraphVisitState, param_dependency_state, type_graph_visit_state,
    };

    #[test]
    fn type_graph_visit_state_names_revisit_cutoff() {
        assert_eq!(type_graph_visit_state(true), TypeGraphVisitState::Entered);
        assert_eq!(
            type_graph_visit_state(false),
            TypeGraphVisitState::AlreadyVisited
        );
    }

    #[test]
    fn param_dependency_state_prioritizes_target_before_revisit() {
        assert_eq!(
            param_dependency_state(true, false),
            ParamDependencyState::TargetReached
        );
        assert_eq!(
            param_dependency_state(false, true),
            ParamDependencyState::Entered
        );
        assert_eq!(
            param_dependency_state(false, false),
            ParamDependencyState::AlreadyVisited
        );
    }
}
