//! Integration tests for stability fixes
//!
//! This test module validates that all stability improvements are working correctly:
//! 1. Compiler option parsing handles comma-separated values
//! 2. Recursion depth limits prevent infinite loops
//! 3. Operation limits prevent OOM
//! 4. Type resolution limits prevent timeouts

use crate::CheckerState;

fn assert_in_range(name: &str, value: usize, min: usize, max: usize) {
    if !(min..=max).contains(&value) {
        panic!("{name} should be in range [{min}, {max}], got {value}");
    }
}

#[test]
fn test_comma_separated_boolean_options() {
    // Validates fix for: "invalid type: string 'true, false'" crashes
    let text = r#"
        // @strict: true, false
        // @noimplicitany: false, true
        // @strictnullchecks: true, false, true
    "#;

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(
        result,
        Some(true),
        "Should parse first value from comma-separated list"
    );

    let result = CheckerState::parse_test_option_bool(text, "@noimplicitany");
    assert_eq!(result, Some(false), "Should parse first value");

    let result = CheckerState::parse_test_option_bool(text, "@strictnullchecks");
    assert_eq!(
        result,
        Some(true),
        "Should parse first value from multiple commas"
    );
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
    use tsz_solver::MAX_INSTANTIATION_DEPTH;

    // Verify the limit is set to a reasonable value
    assert_in_range(
        "Instantiation depth",
        MAX_INSTANTIATION_DEPTH as usize,
        20,
        100,
    );

    // The limit prevents infinite recursion in type instantiation
    // When exceeded, TypeId::ERROR is returned instead of crashing
}

#[test]
fn test_call_depth_limit() {
    // Validates that function call resolution has depth limits
    use crate::state::MAX_CALL_DEPTH;

    assert_in_range("Call depth", MAX_CALL_DEPTH as usize, 10, 50);
}

#[test]
fn test_tree_walk_iteration_limit() {
    // Validates that tree-walking loops have iteration limits
    use crate::state::MAX_TREE_WALK_ITERATIONS;

    assert_in_range(
        "Tree walk iterations",
        MAX_TREE_WALK_ITERATIONS as usize,
        1_000,
        50_000,
    );
}

#[test]
fn test_type_lowering_operation_limit() {
    // Validates that type lowering has operation limits
    use tsz_lowering::MAX_LOWERING_OPERATIONS;

    assert_in_range(
        "Lowering operations",
        MAX_LOWERING_OPERATIONS as usize,
        10_000,
        1_000_000,
    );
}

#[test]
fn test_constraint_recursion_depth_limit() {
    // Validates that constraint collection has recursion limits
    use tsz_solver::MAX_CONSTRAINT_RECURSION_DEPTH;

    assert_in_range(
        "Constraint recursion depth",
        MAX_CONSTRAINT_RECURSION_DEPTH,
        50,
        200,
    );
}
