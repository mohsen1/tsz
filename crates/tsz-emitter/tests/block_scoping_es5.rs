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
fn generated_block_scope_names_are_reserved_after_scope_exit() {
    let mut state = BlockScopeState::new();

    state.enter_function_scope();
    assert_eq!(state.register_var_declaration("x"), "x");

    state.enter_scope();
    assert_eq!(state.register_variable("x"), "x_1");
    state.exit_scope();

    state.enter_scope();
    assert_eq!(state.register_variable("x"), "x_2");
    state.exit_scope();
    state.exit_scope();
}

#[test]
fn unrenamed_block_scope_names_can_reuse_after_scope_exit() {
    let mut state = BlockScopeState::new();

    state.enter_function_scope();

    state.enter_scope();
    assert_eq!(state.register_variable("x"), "x");
    state.exit_scope();

    state.enter_scope();
    assert_eq!(state.register_variable("x"), "x");
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

#[test]
fn nested_function_scope_restores_outer_var_registrations() {
    let mut state = BlockScopeState::new();

    state.enter_function_scope();
    assert_eq!(state.register_var_declaration("x"), "x");

    state.enter_function_scope();
    assert_eq!(state.register_var_declaration("x"), "x");
    state.exit_scope();

    assert_eq!(
        state.register_var_declaration("x"),
        "x",
        "Returning to the outer function should reuse its original var binding"
    );
    state.exit_scope();
}

#[test]
fn class_decl_at_loop_capture_function_level_keeps_local_name() {
    let mut state = BlockScopeState::new();

    state.enter_scope();
    assert_eq!(state.register_variable("row"), "row");

    state.enter_function_scope();
    state.register_function_parameter("row");
    assert_eq!(
        state.register_block_scoped_class("RowClass", false),
        "RowClass"
    );
    state.enter_scope();
    // A non-self-referencing nested-block class with no name collision keeps
    // its original name (tsc emits `var NestedClass`, not `NestedClass_1`).
    assert_eq!(
        state.register_block_scoped_class("NestedClass", false),
        "NestedClass"
    );
    // A self-referencing nested-block class still renames its outer binding.
    assert_eq!(
        state.register_block_scoped_class("SelfRefClass", true),
        "SelfRefClass_1"
    );
    state.exit_scope();
    state.exit_scope();

    assert_eq!(state.get_emitted_name("row"), Some("row".to_string()));
    state.exit_scope();
}

#[test]
fn function_level_shadowed_names_stay_local_nested_blocks_rename() {
    let mut state = BlockScopeState::new();

    state.enter_function_scope();
    state.register_function_scope_shadowed_name("value");

    assert_eq!(
        state.register_variable("value"),
        "value",
        "Direct function-body block-scoped declarations lower to local var bindings"
    );

    state.enter_scope();
    assert_eq!(
        state.register_variable("value"),
        "value_1",
        "Nested block-scoped declarations must rename so the hoisted var does not capture later references"
    );
    state.exit_scope();
    state.exit_scope();
}

#[test]
fn nested_block_scoped_class_without_collision_keeps_name() {
    // A non-self-referencing block-scoped class in a nested block keeps its
    // original name; tsc emits `var Widget`, not `var Widget_1`. Name choice is
    // irrelevant — the rule is structural.
    for name in ["Widget", "Box", "Shape"] {
        let mut state = BlockScopeState::new();
        state.enter_function_scope();
        state.enter_scope(); // nested block, e.g. inside `if (b) { ... }`

        assert_eq!(
            state.register_block_scoped_class(name, false),
            name,
            "non-colliding, non-self-referencing block class reuses its name"
        );

        state.exit_scope();
        state.exit_scope();
    }
}

#[test]
fn nested_block_scoped_class_self_reference_renames() {
    // A self-referencing block-scoped class forces the hoisted self-alias
    // pattern, so its outer binding renames to `Name_1` even without an outer
    // collision (tsc: `var Foo_1` for `class Foo { static f(): Foo {...} }`).
    for name in ["Foo", "Node", "Tree"] {
        let mut state = BlockScopeState::new();
        state.enter_function_scope();
        state.enter_scope();

        assert_eq!(
            state.register_block_scoped_class(name, true),
            format!("{name}_1"),
            "self-referencing block class renames its outer binding"
        );

        state.exit_scope();
        state.exit_scope();
    }
}

#[test]
fn sibling_block_scoped_classes_share_name() {
    // Two same-named, non-self-referencing classes in disjoint sibling blocks
    // both keep the original name — tsc emits `var C` twice, relying on plain
    // `var` function-scoping rather than synthesizing `C_1`/`C_2`.
    for name in ["C", "Item", "Cell"] {
        let mut state = BlockScopeState::new();
        state.enter_function_scope();

        state.enter_scope();
        assert_eq!(state.register_block_scoped_class(name, false), name);
        state.exit_scope();

        state.enter_scope();
        assert_eq!(
            state.register_block_scoped_class(name, false),
            name,
            "second sibling-block class reuses the name; sibling scope was popped"
        );
        state.exit_scope();

        state.exit_scope();
    }
}

#[test]
fn nested_block_scoped_class_renames_on_outer_var_collision() {
    // When an enclosing scope already binds the name (e.g. `var C = 1`), the
    // lowered class must rename to avoid clashing with that hoisted var.
    for name in ["C", "Color", "Group"] {
        let mut state = BlockScopeState::new();
        state.enter_function_scope();
        // Outer binding the lowered class would otherwise clash with.
        state.register_var_declaration(name);

        state.enter_scope();
        assert_eq!(
            state.register_block_scoped_class(name, false),
            format!("{name}_1"),
            "block class renames when an outer same-name binding exists"
        );
        state.exit_scope();

        state.exit_scope();
    }
}

#[test]
fn top_level_block_scoped_class_redeclaration_reuses_name() {
    // At module/script top level (function-scope mark), a same-name class
    // redeclaration reuses the name (tsc emits `var C` for both of two
    // `export default class C`), instead of renaming the second to `C_1`.
    for name in ["C", "Default", "Main"] {
        let mut state = BlockScopeState::new();
        state.enter_function_scope();

        assert_eq!(state.register_block_scoped_class(name, false), name);
        assert_eq!(
            state.register_block_scoped_class(name, false),
            name,
            "top-level class redeclaration reuses the name like `var`"
        );

        state.exit_scope();
    }
}
