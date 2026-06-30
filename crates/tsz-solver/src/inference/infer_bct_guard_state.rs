//! Named guard states for best-common-type inference.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExtendsWalkState {
    Continue,
    DepthExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClassHierarchyVisitState {
    Entered,
    AlreadyVisited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActiveSubtypePairState {
    Entered,
    AlreadyActive { fallback: bool },
}

pub(crate) const fn extends_walk_state(depth: u32, max_depth: u32) -> ExtendsWalkState {
    if depth >= max_depth {
        ExtendsWalkState::DepthExceeded
    } else {
        ExtendsWalkState::Continue
    }
}

pub(crate) const fn class_hierarchy_visit_state(inserted_visit: bool) -> ClassHierarchyVisitState {
    if inserted_visit {
        ClassHierarchyVisitState::Entered
    } else {
        ClassHierarchyVisitState::AlreadyVisited
    }
}

pub(crate) const fn active_subtype_pair_state(already_active: bool) -> ActiveSubtypePairState {
    if already_active {
        ActiveSubtypePairState::AlreadyActive { fallback: true }
    } else {
        ActiveSubtypePairState::Entered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveSubtypePairState, ClassHierarchyVisitState, ExtendsWalkState,
        active_subtype_pair_state, class_hierarchy_visit_state, extends_walk_state,
    };

    #[test]
    fn extends_walk_state_names_continue_and_depth_cutoff() {
        assert_eq!(extends_walk_state(0, 20), ExtendsWalkState::Continue);
        assert_eq!(extends_walk_state(20, 20), ExtendsWalkState::DepthExceeded);
    }

    #[test]
    fn class_hierarchy_visit_state_names_revisit_cutoff() {
        assert_eq!(
            class_hierarchy_visit_state(true),
            ClassHierarchyVisitState::Entered
        );
        assert_eq!(
            class_hierarchy_visit_state(false),
            ClassHierarchyVisitState::AlreadyVisited
        );
    }

    #[test]
    fn active_subtype_pair_state_names_coinductive_fallback() {
        assert_eq!(
            active_subtype_pair_state(false),
            ActiveSubtypePairState::Entered
        );
        assert_eq!(
            active_subtype_pair_state(true),
            ActiveSubtypePairState::AlreadyActive { fallback: true }
        );
    }
}
