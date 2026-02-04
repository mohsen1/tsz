//! Tests for the Lawyer layer (Any propagation rules and CompatChecker).

use super::*;
use crate::solver::AnyPropagationMode;
use crate::solver::compat::CompatChecker;
use crate::solver::intern::TypeInterner;
use crate::solver::{LiteralValue, PropertyInfo, TypeId, Visibility};

// =============================================================================
// AnyPropagationRules Tests
// =============================================================================

#[test]
fn test_any_propagation_rules_default() {
    let rules = AnyPropagationRules::new();

    // Default: allow suppression is true
    assert!(rules.allow_any_suppression);
    assert_eq!(rules.any_propagation_mode(), AnyPropagationMode::All);
}

#[test]
fn test_any_propagation_rules_strict() {
    let rules = AnyPropagationRules::strict();

    // Strict: allow suppression is false
    assert!(!rules.allow_any_suppression);
    assert_eq!(
        rules.any_propagation_mode(),
        AnyPropagationMode::TopLevelOnly
    );
}

#[test]
fn test_set_allow_any_suppression() {
    let mut rules = AnyPropagationRules::new();

    // Default is true
    assert!(rules.allow_any_suppression);
    assert_eq!(rules.any_propagation_mode(), AnyPropagationMode::All);

    // Set to false
    rules.set_allow_any_suppression(false);
    assert!(!rules.allow_any_suppression);
    assert_eq!(
        rules.any_propagation_mode(),
        AnyPropagationMode::TopLevelOnly
    );

    // Set back to true
    rules.set_allow_any_suppression(true);
    assert!(rules.allow_any_suppression);
    assert_eq!(rules.any_propagation_mode(), AnyPropagationMode::All);
}

// =============================================================================
// CompatChecker Tests (The Lawyer)
// =============================================================================

#[test]
fn test_compat_checker_any_propagation() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // `any` is assignable to everything (TypeScript compatibility)
    assert!(checker.is_assignable(TypeId::ANY, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::NUMBER, TypeId::ANY));
    assert!(checker.is_assignable(TypeId::ANY, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::STRING, TypeId::ANY));
}

#[test]
fn test_compat_checker_strict_null_checks() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // With strict_null_checks (default), null is NOT assignable to number
    checker.set_strict_null_checks(true);
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));

    // Without strict_null_checks, null IS assignable to number (legacy TS)
    checker.set_strict_null_checks(false);
    assert!(checker.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
}

#[test]
fn test_compat_checker_empty_object_target() {
    let interner = TypeInterner::new();

    // Create an empty object type
    let empty_obj = interner.object(vec![]);

    // Create some test types
    let num_type = TypeId::NUMBER;
    let str_type = TypeId::STRING;

    let mut checker = CompatChecker::new(&interner);

    // Empty object accepts all non-nullish, non-any/unknown values
    assert!(checker.is_assignable(num_type, empty_obj));
    assert!(checker.is_assignable(str_type, empty_obj));

    // But null/undefined are NOT assignable to empty object
    assert!(!checker.is_assignable(TypeId::NULL, empty_obj));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, empty_obj));

    // void is NOT assignable to empty object
    assert!(!checker.is_assignable(TypeId::VOID, empty_obj));

    // any/never are assignable
    assert!(checker.is_assignable(TypeId::ANY, empty_obj));
    assert!(checker.is_assignable(TypeId::NEVER, empty_obj));
}

// =============================================================================
// TypeScriptQuirks Tests
// =============================================================================

// NOTE: Function variance test is omitted - it requires deeper investigation
// into the bivariance implementation. The current behavior may differ from
// TypeScript's legacy mode due to complex interactions between function
// parameter variance and function type checking rules.
// TODO: Add comprehensive function variance tests once implementation is verified

#[test]
fn test_compat_checker_weak_type_detection() {
    let interner = TypeInterner::new();

    // Create a weak type (all optional properties)
    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");

    let weak_type = interner.object(vec![
        PropertyInfo {
            name: name_atom,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: age_atom,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ]);

    // Empty object should be assignable to weak type
    let empty_obj = interner.object(vec![]);
    let mut checker = CompatChecker::new(&interner);
    assert!(checker.is_assignable(empty_obj, weak_type));

    // Object with unrelated properties should NOT be assignable
    let unrelated_atom = interner.intern_string("unrelated");
    let unrelated_obj = interner.object(vec![PropertyInfo {
        name: unrelated_atom,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);
    assert!(!checker.is_assignable(unrelated_obj, weak_type));

    // Object with at least one common property should be assignable
    let matching_obj = interner.object(vec![PropertyInfo {
        name: name_atom,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);
    assert!(checker.is_assignable(matching_obj, weak_type));
}

// =============================================================================
// TypeScriptQuirks Tests
// =============================================================================

// NOTE: Function variance test is omitted - it requires deeper investigation
// into the bivariance implementation. The current behavior may differ from
// TypeScript's legacy mode due to complex interactions between function
// parameter variance and function type checking rules.
// TODO: Add comprehensive function variance tests once implementation is verified

#[test]
fn test_typescript_quirks_list() {
    let quirks = TypeScriptQuirks::QUIRKS;
    assert!(
        quirks.len() >= 9,
        "Should have at least 9 documented quirks"
    );
    let quirk_names: Vec<&str> = quirks.iter().map(|(name, _)| *name).collect();
    assert!(quirk_names.contains(&"any-propagation"));
    assert!(quirk_names.contains(&"function-bivariance"));
    assert!(quirk_names.contains(&"method-bivariance"));
    assert!(quirk_names.contains(&"void-return"));
    assert!(quirk_names.contains(&"weak-types"));
    assert!(quirk_names.contains(&"freshness"));
}
