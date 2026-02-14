use super::*;

#[test]
fn test_decorator_target_class() {
    let arena = NodeArena::new();
    let checker = DecoratorChecker::new(&arena);

    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::CLASS_DECLARATION),
        Some(DecoratorTarget::Class)
    );
    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::CLASS_EXPRESSION),
        Some(DecoratorTarget::Class)
    );
}

#[test]
fn test_decorator_target_method() {
    let arena = NodeArena::new();
    let checker = DecoratorChecker::new(&arena);

    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::METHOD_DECLARATION),
        Some(DecoratorTarget::Method)
    );
}

#[test]
fn test_decorator_target_accessor() {
    let arena = NodeArena::new();
    let checker = DecoratorChecker::new(&arena);

    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::GET_ACCESSOR),
        Some(DecoratorTarget::Accessor)
    );
    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::SET_ACCESSOR),
        Some(DecoratorTarget::Accessor)
    );
}

#[test]
fn test_decorator_target_property() {
    let arena = NodeArena::new();
    let checker = DecoratorChecker::new(&arena);

    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::PROPERTY_DECLARATION),
        Some(DecoratorTarget::Property)
    );
}

#[test]
fn test_decorator_target_parameter() {
    let arena = NodeArena::new();
    let checker = DecoratorChecker::new(&arena);

    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::PARAMETER),
        Some(DecoratorTarget::Parameter)
    );
}

#[test]
fn test_decorator_invalid_target() {
    let arena = NodeArena::new();
    let checker = DecoratorChecker::new(&arena);

    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::FUNCTION_DECLARATION),
        None
    );
    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::VARIABLE_STATEMENT),
        None
    );
    assert_eq!(
        checker.get_decorator_target(syntax_kind_ext::INTERFACE_DECLARATION),
        None
    );
}
