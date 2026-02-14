use super::*;
use tsz_solver::TypeInterner;

#[test]
fn test_nullish_coalescing_with_null_left() {
    let types = TypeInterner::new();

    // null ?? string should be string
    let result = get_nullish_coalescing_type(&types, SolverTypeId::NULL, SolverTypeId::STRING);
    assert_eq!(result, SolverTypeId::STRING);
}

#[test]
fn test_nullish_coalescing_with_undefined_left() {
    let types = TypeInterner::new();

    // undefined ?? number should be number
    let result = get_nullish_coalescing_type(&types, SolverTypeId::UNDEFINED, SolverTypeId::NUMBER);
    assert_eq!(result, SolverTypeId::NUMBER);
}

#[test]
fn test_nullish_coalescing_non_nullish_left() {
    let types = TypeInterner::new();

    // string ?? number should be string (string is never nullish)
    let result = get_nullish_coalescing_type(&types, SolverTypeId::STRING, SolverTypeId::NUMBER);
    assert_eq!(result, SolverTypeId::STRING);
}

#[test]
fn test_nullish_coalescing_any_left() {
    let types = TypeInterner::new();

    // any ?? number should be any
    let result = get_nullish_coalescing_type(&types, SolverTypeId::ANY, SolverTypeId::NUMBER);
    assert_eq!(result, SolverTypeId::ANY);
}

#[test]
fn test_precedence_check() {
    let arena = NodeArena::new();
    // Test with empty node
    let result = check_nullish_coalescing_precedence(&arena, NodeIndex::NONE);
    assert!(result.is_none());
}
