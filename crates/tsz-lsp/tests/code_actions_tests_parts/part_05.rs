#[test]
fn test_codefix_registry_import_error_2304() {
    // Cannot find name '{0}'. (2304)
    let fixes = CodeFixRegistry::fixes_for_error_code(2304);
    assert!(!fixes.is_empty(), "Should return fixes for error 2304");
    let fix_names: Vec<&str> = fixes.iter().map(|f| f.0).collect();
    assert!(fix_names.contains(&"import"), "Should contain import fix");
}

#[test]
fn test_codefix_registry_unused_identifier_6133() {
    // '{0}' is declared but its value is never read. (6133)
    let fixes = CodeFixRegistry::fixes_for_error_code(6133);
    assert!(!fixes.is_empty(), "Should return fixes for error 6133");
    assert_eq!(fixes[0].0, "unusedIdentifier");
    assert_eq!(fixes[0].1, "unusedIdentifier_delete");
}

#[test]
fn test_codefix_registry_unused_identifier_6196() {
    // '{0}' is declared but never used. (6196)
    let fixes = CodeFixRegistry::fixes_for_error_code(6196);
    assert!(!fixes.is_empty(), "Should return fixes for error 6196");
    assert_eq!(fixes[0].0, "unusedIdentifier");
}

#[test]
fn test_codefix_registry_add_missing_member_2339() {
    // Property '{0}' does not exist on type '{1}'. (2339)
    let fixes = CodeFixRegistry::fixes_for_error_code(2339);
    assert!(!fixes.is_empty(), "Should return fixes for error 2339");
    let fix_names: Vec<&str> = fixes.iter().map(|f| f.0).collect();
    assert!(
        fix_names.contains(&"addMissingMember"),
        "Should contain addMissingMember fix"
    );
}

#[test]
fn test_codefix_registry_await_in_sync_1308() {
    // 'await' expressions are only allowed within async functions (1308)
    let fixes = CodeFixRegistry::fixes_for_error_code(1308);
    assert!(!fixes.is_empty(), "Should return fixes for error 1308");
    assert_eq!(fixes[0].0, "fixAwaitInSyncFunction");
    assert_eq!(fixes[0].1, "fixAwaitInSyncFunction");

    // Also check 1359 variant
    let fixes_1359 = CodeFixRegistry::fixes_for_error_code(1359);
    assert!(!fixes_1359.is_empty(), "Should return fixes for error 1359");
    assert_eq!(fixes_1359[0].0, "fixAwaitInSyncFunction");
}

#[test]
fn test_codefix_registry_override_modifier_4114() {
    // This member cannot have an 'override' modifier (4114)
    let fixes = CodeFixRegistry::fixes_for_error_code(4114);
    assert!(!fixes.is_empty(), "Should return fixes for error 4114");
    assert_eq!(fixes[0].0, "fixOverrideModifier");
}

#[test]
fn test_codefix_registry_class_implements_interface_2420() {
    // Class '{0}' incorrectly implements interface '{1}'. (2420)
    let fixes = CodeFixRegistry::fixes_for_error_code(2420);
    assert!(!fixes.is_empty(), "Should return fixes for error 2420");
    assert_eq!(fixes[0].0, "fixClassIncorrectlyImplementsInterface");
    assert_eq!(fixes[0].1, "fixClassIncorrectlyImplementsInterface");
}

#[test]
fn test_codefix_registry_unreachable_code_7027() {
    // Unreachable code detected (7027)
    let fixes = CodeFixRegistry::fixes_for_error_code(7027);
    assert!(!fixes.is_empty(), "Should return fixes for error 7027");
    assert_eq!(fixes[0].0, "fixUnreachableCode");
}

#[test]
fn test_codefix_registry_unknown_error_returns_empty() {
    // Unknown error code should return empty
    let fixes = CodeFixRegistry::fixes_for_error_code(99999);
    assert!(
        fixes.is_empty(),
        "Should return no fixes for unknown error code"
    );
}

#[test]
fn test_codefix_registry_supported_error_codes_not_empty() {
    let codes = CodeFixRegistry::supported_error_codes();
    assert!(!codes.is_empty(), "Should return supported error codes");
    assert!(
        codes.contains(&2304),
        "Should contain 2304 (Cannot find name)"
    );
    assert!(
        codes.contains(&2339),
        "Should contain 2339 (Property does not exist)"
    );
    assert!(
        codes.contains(&6133),
        "Should contain 6133 (Unused identifier)"
    );
    assert!(codes.contains(&2552), "Should contain 2552 (Did you mean)");
}

