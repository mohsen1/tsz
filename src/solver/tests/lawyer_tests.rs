//! Tests for the Lawyer layer (Any propagation rules).

use super::*;
use crate::solver::AnyPropagationMode;

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
// TypeScriptQuirks Tests
// =============================================================================

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
