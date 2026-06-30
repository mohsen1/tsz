use super::*;
use crate::construction::TypeInterner;
use crate::types::TypeId;
use rustc_hash::FxHashSet;

#[test]
fn alias_def_reach_visit_state_names_intrinsic_skip() {
    let mut visited = FxHashSet::default();

    assert_eq!(
        AliasDefReachVisitState::enter(TypeId::STRING, &mut visited),
        AliasDefReachVisitState::Intrinsic
    );
    assert!(visited.is_empty());
}

#[test]
fn alias_def_reach_visit_state_names_entered_and_revisit() {
    let interner = TypeInterner::new();
    let type_id = interner.object(vec![]);
    let mut visited = FxHashSet::default();

    assert_eq!(
        AliasDefReachVisitState::enter(type_id, &mut visited),
        AliasDefReachVisitState::Entered
    );
    assert_eq!(
        AliasDefReachVisitState::enter(type_id, &mut visited),
        AliasDefReachVisitState::AlreadyVisited
    );
}
