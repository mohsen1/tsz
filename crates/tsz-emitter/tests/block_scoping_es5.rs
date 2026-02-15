use super::*;

#[test]
fn test_block_scope_state() {
    let mut state = BlockScopeState::new();

    state.enter_scope();
    assert_eq!(state.register_variable("x"), "x");
    assert_eq!(state.get_emitted_name("x"), Some("x".to_string()));

    state.enter_scope();
    // Shadowing - should rename
    assert_eq!(state.register_variable("x"), "x_1");
    assert_eq!(state.get_emitted_name("x"), Some("x_1".to_string()));

    state.exit_scope();
    // Back to outer scope
    assert_eq!(state.get_emitted_name("x"), Some("x".to_string()));

    state.exit_scope();
}

#[test]
fn test_loop_function_names() {
    let mut state = BlockScopeState::new();

    assert_eq!(state.next_loop_function_name(), "_loop_1");
    assert_eq!(state.next_loop_function_name(), "_loop_2");
    assert_eq!(state.next_loop_function_name(), "_loop_3");
}

#[test]
fn test_reserved_names() {
    let mut state = BlockScopeState::new();

    // Reserve temp variable names
    state.reserve_name("_a".to_string());
    state.reserve_name("a_1".to_string());

    state.enter_scope();

    // First registration should not conflict
    assert_eq!(state.register_variable("a"), "a");

    state.enter_scope();
    // Shadowing 'a' should skip reserved 'a_1' and use 'a_2'
    let renamed = state.register_variable("a");
    assert_eq!(renamed, "a_2");

    state.exit_scope();
    state.exit_scope();
}

#[test]
fn test_reserved_names_multiple_conflicts() {
    let mut state = BlockScopeState::new();

    // Reserve several names in sequence
    state.reserve_name("v_1".to_string());
    state.reserve_name("v_2".to_string());
    state.reserve_name("v_4".to_string());

    state.enter_scope();
    state.register_variable("v");

    state.enter_scope();
    // Should skip v_1, v_2 and use v_3
    assert_eq!(state.register_variable("v"), "v_3");

    state.enter_scope();
    // Should skip v_4 and use v_5
    assert_eq!(state.register_variable("v"), "v_5");

    state.exit_scope();
    state.exit_scope();
    state.exit_scope();
}

#[test]
fn test_reset_clears_reserved_names() {
    let mut state = BlockScopeState::new();

    state.reserve_name("a_1".to_string());
    state.enter_scope();
    state.register_variable("a");

    state.reset();

    // After reset, reserved names should be cleared
    state.enter_scope();
    state.register_variable("a");
    state.enter_scope();
    // Should now use a_1 since it's no longer reserved
    assert_eq!(state.register_variable("a"), "a_1");

    state.exit_scope();
    state.exit_scope();
}

#[test]
fn test_var_redeclaration_same_scope() {
    let mut state = BlockScopeState::new();

    state.enter_scope();
    // First var declaration
    assert_eq!(state.register_var_declaration("cl"), "cl");
    // Redeclaration in same scope — should NOT rename
    assert_eq!(state.register_var_declaration("cl"), "cl");
    // Third redeclaration — still no rename
    assert_eq!(state.register_var_declaration("cl"), "cl");

    state.exit_scope();
}

#[test]
fn test_var_declaration_parent_scope_conflict() {
    let mut state = BlockScopeState::new();

    state.enter_scope();
    // Outer scope has `a`
    assert_eq!(state.register_variable("a"), "a");

    state.enter_scope();
    // Reserve a_1 (like for-of loop temp)
    state.reserve_name("a_1".to_string());

    // var a in inner scope — should rename to a_2 (skip reserved a_1)
    assert_eq!(state.register_var_declaration("a"), "a_2");

    state.exit_scope();
    state.exit_scope();
}

#[test]
fn test_var_declaration_parent_scope_conflict_without_reserved_name() {
    let mut state = BlockScopeState::new();

    state.enter_scope();
    state.register_variable("a");

    state.enter_scope();
    assert_eq!(state.register_var_declaration("a"), "a_1");

    state.exit_scope();
    state.exit_scope();
}

#[test]
fn test_var_declaration_no_parent_conflict() {
    let mut state = BlockScopeState::new();

    state.enter_scope();
    // No parent scope — should not rename
    assert_eq!(state.register_var_declaration("x"), "x");
    // Redeclaration in same scope — should not rename
    assert_eq!(state.register_var_declaration("x"), "x");

    state.exit_scope();
}
