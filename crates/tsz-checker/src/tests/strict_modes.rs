use super::*;

#[test]
fn test_function_type_result_eq() {
    assert_eq!(FunctionTypeResult::Compatible, FunctionTypeResult::Compatible);
    assert_ne!(FunctionTypeResult::Compatible, FunctionTypeResult::Incompatible);
}

#[test]
fn test_optional_property_result_eq() {
    assert_eq!(OptionalPropertyResult::Allowed, OptionalPropertyResult::Allowed);
    assert_ne!(
        OptionalPropertyResult::Allowed,
        OptionalPropertyResult::ExplicitUndefinedNotAllowed
    );
}

#[test]
fn test_strict_modes_checker_creation() {
    let arena = NodeArena::new();
    let types = TypeInterner::new();
    let checker = StrictModesChecker::new(
        &arena,
        &types,
        true, // strict_null_checks
        true, // strict_function_types
        true, // strict_bind_call_apply
        true, // strict_property_initialization
        true, // no_implicit_any
        true, // no_implicit_this
        true, // use_unknown_in_catch_variables
        true, // exact_optional_property_types
    );

    assert!(checker.requires_property_initialization());
    assert!(checker.should_report_implicit_this());
    assert_eq!(checker.get_catch_variable_type(), TypeId::UNKNOWN);
}

#[test]
fn test_catch_variable_type() {
    let arena = NodeArena::new();
    let types = TypeInterner::new();

    // With useUnknownInCatchVariables
    let checker = StrictModesChecker::new(
        &arena,
        &types,
        false,
        false,
        false,
        false,
        false,
        false,
        true,
        false,
    );
    assert_eq!(checker.get_catch_variable_type(), TypeId::UNKNOWN);

    // Without useUnknownInCatchVariables
    let checker = StrictModesChecker::new(
        &arena,
        &types,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert_eq!(checker.get_catch_variable_type(), TypeId::ANY);
}
