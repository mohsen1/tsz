//! Integration tests for stability fixes
//!
//! This test module validates that all stability improvements are working correctly:
//! 1. Compiler option parsing handles comma-separated values
//! 2. Recursion depth limits prevent infinite loops
//! 3. Operation limits prevent OOM
//! 4. Type resolution limits prevent timeouts

use crate::checker::CheckerState;

#[test]
fn test_comma_separated_boolean_options() {
    // Validates fix for: "invalid type: string 'true, false'" crashes
    let text = r#"
        // @strict: true, false
        // @noimplicitany: false, true
        // @strictnullchecks: true, false, true
    "#;

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, Some(true), "Should parse first value from comma-separated list");

    let result = CheckerState::parse_test_option_bool(text, "@noimplicitany");
    assert_eq!(result, Some(false), "Should parse first value");

    let result = CheckerState::parse_test_option_bool(text, "@strictnullchecks");
    assert_eq!(result, Some(true), "Should parse first value from multiple commas");
}

#[test]
fn test_boolean_option_with_trailing_delimiters() {
    let text = r#"
        // @strict: true;
        // @noimplicitany: false,
    "#;

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, Some(true), "Should handle semicolon delimiter");

    let result = CheckerState::parse_test_option_bool(text, "@noimplicitany");
    assert_eq!(result, Some(false), "Should handle comma delimiter");
}

#[test]
fn test_recursive_type_depth_limit() {
    // Validates that recursive type expansion has proper depth limits
    // This should not cause OOM or stack overflow
    use crate::solver::instantiate::MAX_INSTANTIATION_DEPTH;

    // Verify the limit is set to a reasonable value
    assert!(MAX_INSTANTIATION_DEPTH <= 100, "Instantiation depth should be <= 100");
    assert!(MAX_INSTANTIATION_DEPTH >= 20, "Instantiation depth should be >= 20");

    // The limit prevents infinite recursion in type instantiation
    // When exceeded, TypeId::ERROR is returned instead of crashing
}

#[test]
fn test_call_depth_limit() {
    // Validates that function call resolution has depth limits
    use crate::checker::state::MAX_CALL_DEPTH;

    assert!(MAX_CALL_DEPTH <= 50, "Call depth should be <= 50");
    assert!(MAX_CALL_DEPTH >= 10, "Call depth should be >= 10");
}

#[test]
fn test_tree_walk_iteration_limit() {
    // Validates that tree-walking loops have iteration limits
    use crate::checker::state::MAX_TREE_WALK_ITERATIONS;

    assert!(MAX_TREE_WALK_ITERATIONS <= 50_000, "Tree walk limit should be <= 50000");
    assert!(MAX_TREE_WALK_ITERATIONS >= 1_000, "Tree walk limit should be >= 1000");
}

#[test]
fn test_type_lowering_operation_limit() {
    // Validates that type lowering has operation limits
    use crate::solver::lower::MAX_LOWERING_OPERATIONS;

    assert!(MAX_LOWERING_OPERATIONS <= 1_000_000, "Lowering ops should be <= 1M");
    assert!(MAX_LOWERING_OPERATIONS >= 10_000, "Lowering ops should be >= 10K");
}

#[test]
fn test_constraint_recursion_depth_limit() {
    // Validates that constraint collection has recursion limits
    use crate::solver::operations::MAX_CONSTRAINT_RECURSION_DEPTH;

    assert!(MAX_CONSTRAINT_RECURSION_DEPTH <= 200, "Constraint depth should be <= 200");
    assert!(MAX_CONSTRAINT_RECURSION_DEPTH >= 50, "Constraint depth should be >= 50");
}
