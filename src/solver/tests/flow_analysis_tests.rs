//! Tests for flow analysis integration with the solver.
//!
//! These tests verify that:
//! - FlowFacts properly merge at control flow join points
//! - FlowTypeEvaluator correctly narrows types
//! - Definite assignment checking works properly
//! - TDZ violations are detected

use crate::solver::TypeInterner;
use crate::solver::flow_analysis::{FlowFacts, FlowTypeEvaluator};
use rustc_hash::{FxHashMap, FxHashSet};

#[test]
fn test_flow_facts_basic_operations() {
    let mut facts = FlowFacts::new();

    // Test definite assignment tracking
    assert!(!facts.is_definitely_assigned("x"));
    facts.mark_definitely_assigned("x".to_string());
    assert!(facts.is_definitely_assigned("x"));

    // Test type narrowing tracking
    assert!(facts.get_narrowed_type("y").is_none());
    facts.add_narrowing("y".to_string(), crate::solver::TypeId::STRING);
    assert_eq!(
        facts.get_narrowed_type("y"),
        Some(crate::solver::TypeId::STRING)
    );

    // Test TDZ violation tracking
    assert!(!facts.has_tdz_violation("z"));
    facts.mark_tdz_violation("z".to_string());
    assert!(facts.has_tdz_violation("z"));
}

#[test]
fn test_flow_facts_merge_intersection_for_narrowings() {
    let mut facts1 = FlowFacts::new();
    facts1.add_narrowing("x".to_string(), crate::solver::TypeId::STRING);
    facts1.add_narrowing("y".to_string(), crate::solver::TypeId::NUMBER);

    let mut facts2 = FlowFacts::new();
    facts2.add_narrowing("x".to_string(), crate::solver::TypeId::STRING);
    facts2.add_narrowing("z".to_string(), crate::solver::TypeId::BOOLEAN);

    let merged = facts1.merge(&facts2);

    // x should be present (same narrowing in both)
    assert_eq!(
        merged.get_narrowed_type("x"),
        Some(crate::solver::TypeId::STRING)
    );

    // y should not be present (only in facts1)
    assert!(merged.get_narrowed_type("y").is_none());

    // z should not be present (only in facts2)
    assert!(merged.get_narrowed_type("z").is_none());
}

#[test]
fn test_flow_facts_merge_conflicting_narrowings() {
    let mut facts1 = FlowFacts::new();
    facts1.add_narrowing("x".to_string(), crate::solver::TypeId::STRING);

    let mut facts2 = FlowFacts::new();
    facts2.add_narrowing("x".to_string(), crate::solver::TypeId::NUMBER);

    let merged = facts1.merge(&facts2);

    // x should not be present (different narrowings)
    assert!(merged.get_narrowed_type("x").is_none());
}

#[test]
fn test_flow_facts_merge_intersection_for_assignments() {
    let mut facts1 = FlowFacts::new();
    facts1.mark_definitely_assigned("x".to_string());
    facts1.mark_definitely_assigned("y".to_string());

    let mut facts2 = FlowFacts::new();
    facts2.mark_definitely_assigned("x".to_string());
    facts2.mark_definitely_assigned("z".to_string());

    let merged = facts1.merge(&facts2);

    // x should be assigned (in both)
    assert!(merged.is_definitely_assigned("x"));

    // y should not be assigned (only in facts1)
    assert!(!merged.is_definitely_assigned("y"));

    // z should not be assigned (only in facts2)
    assert!(!merged.is_definitely_assigned("z"));
}

#[test]
fn test_flow_facts_merge_intersection_for_tdz_violations() {
    let mut facts1 = FlowFacts::new();
    facts1.mark_tdz_violation("x".to_string());
    facts1.mark_tdz_violation("y".to_string());

    let mut facts2 = FlowFacts::new();
    facts2.mark_tdz_violation("x".to_string());
    facts2.mark_tdz_violation("z".to_string());

    let merged = facts1.merge(&facts2);

    // x should be present (in both)
    assert!(merged.has_tdz_violation("x"));
    // y should not be present (only in facts1)
    assert!(!merged.has_tdz_violation("y"));
    // z should not be present (only in facts2)
    assert!(!merged.has_tdz_violation("z"));
}

#[test]
fn test_flow_facts_merge_empty_with_populated() {
    let populated = {
        let mut facts = FlowFacts::new();
        facts.mark_definitely_assigned("x".to_string());
        facts.add_narrowing("y".to_string(), crate::solver::TypeId::STRING);
        facts.mark_tdz_violation("z".to_string());
        facts
    };

    let empty = FlowFacts::new();

    let merged1 = populated.merge(&empty);
    let merged2 = empty.merge(&populated);

    // Merging with empty should produce empty (intersection behavior)
    assert!(!merged1.is_definitely_assigned("x"));
    assert!(merged1.get_narrowed_type("y").is_none());
    assert!(!merged1.has_tdz_violation("z"));

    // Same result regardless of order
    assert!(!merged2.is_definitely_assigned("x"));
    assert!(merged2.get_narrowed_type("y").is_none());
    assert!(!merged2.has_tdz_violation("z"));
}

#[test]
fn test_flow_type_evaluator_definite_assignment() {
    let interner = TypeInterner::new();
    let evaluator = FlowTypeEvaluator::new(&interner);

    let mut facts = FlowFacts::new();
    facts.mark_definitely_assigned("x".to_string());
    facts.mark_definitely_assigned("y".to_string());

    assert!(evaluator.is_definitely_assigned("x", &facts));
    assert!(evaluator.is_definitely_assigned("y", &facts));
    assert!(!evaluator.is_definitely_assigned("z", &facts));
}

#[test]
fn test_flow_type_evaluator_tdz_checking() {
    let interner = TypeInterner::new();
    let evaluator = FlowTypeEvaluator::new(&interner);

    let mut facts = FlowFacts::new();
    facts.mark_tdz_violation("x".to_string());

    assert!(evaluator.has_tdz_violation("x", &facts));
    assert!(!evaluator.has_tdz_violation("y", &facts));
}

#[test]
fn test_flow_type_evaluator_compute_narrowed_type() {
    let interner = TypeInterner::new();
    let evaluator = FlowTypeEvaluator::new(&interner);

    let mut facts = FlowFacts::new();
    facts.add_narrowing("x".to_string(), crate::solver::TypeId::STRING);

    // Should return the narrowed type
    assert_eq!(
        evaluator.compute_narrowed_type(crate::solver::TypeId::ANY, &facts, "x"),
        crate::solver::TypeId::STRING
    );

    // Should return original type if no narrowing
    assert_eq!(
        evaluator.compute_narrowed_type(crate::solver::TypeId::NUMBER, &facts, "y"),
        crate::solver::TypeId::NUMBER
    );
}

#[test]
fn test_flow_type_evaluator_facts_from_assignments() {
    let interner = TypeInterner::new();
    let evaluator = FlowTypeEvaluator::new(&interner);

    let mut assignments = FxHashSet::default();
    assignments.insert("x".to_string());
    assignments.insert("y".to_string());

    let facts = evaluator.facts_from_assignments(assignments);

    assert!(facts.is_definitely_assigned("x"));
    assert!(facts.is_definitely_assigned("y"));
    assert!(!facts.is_definitely_assigned("z"));
}

#[test]
fn test_flow_type_evaluator_facts_from_narrowings() {
    let interner = TypeInterner::new();
    let evaluator = FlowTypeEvaluator::new(&interner);

    let mut narrowings = FxHashMap::default();
    narrowings.insert("x".to_string(), crate::solver::TypeId::STRING);
    narrowings.insert("y".to_string(), crate::solver::TypeId::NUMBER);

    let facts = evaluator.facts_from_narrowings(narrowings);

    assert_eq!(
        facts.get_narrowed_type("x"),
        Some(crate::solver::TypeId::STRING)
    );
    assert_eq!(
        facts.get_narrowed_type("y"),
        Some(crate::solver::TypeId::NUMBER)
    );
    assert!(facts.get_narrowed_type("z").is_none());
}

#[test]
fn test_flow_type_evaluator_narrow_by_typeof() {
    let interner = TypeInterner::new();
    let evaluator = FlowTypeEvaluator::new(&interner);

    // Test narrowing union to string
    let string_or_number =
        interner.union2(crate::solver::TypeId::STRING, crate::solver::TypeId::NUMBER);
    let narrowed = evaluator.narrow_by_typeof(string_or_number, "string");

    // Should narrow to string
    assert_eq!(narrowed, crate::solver::TypeId::STRING);
}

#[test]
fn test_flow_type_evaluator_narrow_excluding_type() {
    let interner = TypeInterner::new();
    let evaluator = FlowTypeEvaluator::new(&interner);

    // Test narrowing string | null to exclude null
    let string_or_null =
        interner.union2(crate::solver::TypeId::STRING, crate::solver::TypeId::NULL);
    let narrowed = evaluator.narrow_excluding_type(string_or_null, crate::solver::TypeId::NULL);

    // Should narrow to string
    assert_eq!(narrowed, crate::solver::TypeId::STRING);
}

#[test]
fn test_flow_facts_complex_merge() {
    // Test merging multiple flow paths
    let mut path1 = FlowFacts::new();
    path1.mark_definitely_assigned("a".to_string());
    path1.add_narrowing("x".to_string(), crate::solver::TypeId::STRING);

    let mut path2 = FlowFacts::new();
    path2.mark_definitely_assigned("a".to_string());
    path2.mark_definitely_assigned("b".to_string());
    path2.add_narrowing("x".to_string(), crate::solver::TypeId::STRING);

    let mut path3 = FlowFacts::new();
    path3.mark_definitely_assigned("a".to_string());
    path3.add_narrowing("x".to_string(), crate::solver::TypeId::STRING);
    path3.mark_tdz_violation("c".to_string());

    // Merge all three paths
    let merged = path1.merge(&path2).merge(&path3);

    // a should be definitely assigned (in all paths)
    assert!(merged.is_definitely_assigned("a"));

    // b should not be assigned (only in path2)
    assert!(!merged.is_definitely_assigned("b"));

    // x should keep its narrowing (same in all paths)
    assert_eq!(
        merged.get_narrowed_type("x"),
        Some(crate::solver::TypeId::STRING)
    );

    // c should NOT have TDZ violation (only in path3, intersection requires all paths)
    assert!(!merged.has_tdz_violation("c"));
}

#[test]
fn test_flow_facts_clone() {
    let mut facts1 = FlowFacts::new();
    facts1.mark_definitely_assigned("x".to_string());
    facts1.add_narrowing("y".to_string(), crate::solver::TypeId::STRING);
    facts1.mark_tdz_violation("z".to_string());

    let mut facts2 = facts1.clone();

    // Both should have the same data
    assert!(facts2.is_definitely_assigned("x"));
    assert_eq!(
        facts2.get_narrowed_type("y"),
        Some(crate::solver::TypeId::STRING)
    );
    assert!(facts2.has_tdz_violation("z"));

    // Modifying clone shouldn't affect original
    facts2.mark_definitely_assigned("a".to_string());
    assert!(!facts1.is_definitely_assigned("a"));
    assert!(facts2.is_definitely_assigned("a"));
}
