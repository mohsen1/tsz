use super::*;

#[test]
fn test_assignment_state_merge() {
    // DefinitelyAssigned merge DefinitelyAssigned = DefinitelyAssigned
    assert_eq!(
        AssignmentState::DefinitelyAssigned.merge(AssignmentState::DefinitelyAssigned),
        AssignmentState::DefinitelyAssigned
    );

    // DefinitelyAssigned merge Unassigned = MaybeAssigned
    assert_eq!(
        AssignmentState::DefinitelyAssigned.merge(AssignmentState::Unassigned),
        AssignmentState::MaybeAssigned
    );

    // Unassigned merge Unassigned = Unassigned
    assert_eq!(
        AssignmentState::Unassigned.merge(AssignmentState::Unassigned),
        AssignmentState::Unassigned
    );

    // MaybeAssigned merge anything = MaybeAssigned
    assert_eq!(
        AssignmentState::MaybeAssigned.merge(AssignmentState::DefinitelyAssigned),
        AssignmentState::MaybeAssigned
    );
    assert_eq!(
        AssignmentState::MaybeAssigned.merge(AssignmentState::Unassigned),
        AssignmentState::MaybeAssigned
    );
}

#[test]
fn test_assignment_state_map() {
    let mut map = AssignmentStateMap::new();
    let var1 = NodeIndex(1);
    let var2 = NodeIndex(2);

    // Initially unassigned
    assert_eq!(map.get(var1), AssignmentState::Unassigned);

    // Mark as assigned
    map.mark_assigned(var1);
    assert_eq!(map.get(var1), AssignmentState::DefinitelyAssigned);

    // Merge with another map
    let mut map2 = AssignmentStateMap::new();
    map2.set(var2, AssignmentState::DefinitelyAssigned);
    map2.set(var1, AssignmentState::Unassigned);

    map.merge(&map2);
    // var1: DefinitelyAssigned merge Unassigned = MaybeAssigned
    assert_eq!(map.get(var1), AssignmentState::MaybeAssigned);
    // var2: Unassigned merge DefinitelyAssigned = MaybeAssigned
    assert_eq!(map.get(var2), AssignmentState::MaybeAssigned);
}

#[test]
fn test_definite_assignment_result() {
    let mut states = AssignmentStateMap::new();
    let var1 = NodeIndex(1);
    let var2 = NodeIndex(2);

    states.mark_assigned(var1);
    states.set(var2, AssignmentState::MaybeAssigned);

    let result = DefiniteAssignmentResult { states };

    assert!(result.is_definitely_assigned(var1));
    assert!(!result.is_definitely_assigned(var2));
    assert!(result.is_maybe_assigned(var1));
    assert!(result.is_maybe_assigned(var2));
}

#[test]
fn test_merge_assignment_states() {
    let mut state1 = AssignmentStateMap::new();
    let state2 = AssignmentStateMap::new();
    let var1 = NodeIndex(1);

    state1.mark_assigned(var1);
    // state2 doesn't have var1 (implicitly Unassigned)

    let merged = merge_assignment_states(&[state1, state2]);
    assert_eq!(merged.get(var1), AssignmentState::MaybeAssigned);
}
