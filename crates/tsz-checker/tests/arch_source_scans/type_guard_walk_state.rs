#[test]
fn type_guard_tree_walks_use_named_state() {
    let type_guards = include_str!("../../src/flow/control_flow/type_guards.rs");
    let walk_state = include_str!("../../src/flow/control_flow/type_guard_walk.rs");

    assert!(
        !type_guards.contains("MAX_TREE_WALK_ITERATIONS"),
        "type guard walks should route through named walk state instead of raw limits"
    );
    assert!(
        type_guards.contains("TypeGuardWalk"),
        "type guard extraction should use the named walk helper"
    );
    assert!(
        walk_state.contains("TypeGuardWalkState::Exhausted"),
        "the helper should name exhausted walks explicitly"
    );
    assert!(
        walk_state.contains("TypeGuardWalkState::Finished"),
        "the helper should distinguish normal termination from exhaustion"
    );
}
