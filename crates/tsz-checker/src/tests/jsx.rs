use super::*;

#[test]
fn test_is_intrinsic_element() {
    assert!(JsxChecker::is_intrinsic_element("div"));
    assert!(JsxChecker::is_intrinsic_element("span"));
    assert!(JsxChecker::is_intrinsic_element("custom-element"));
    assert!(!JsxChecker::is_intrinsic_element("MyComponent"));
    assert!(!JsxChecker::is_intrinsic_element("App"));
    assert!(!JsxChecker::is_intrinsic_element(""));
}

#[test]
fn test_is_known_intrinsic() {
    assert!(JsxChecker::is_known_intrinsic("div"));
    assert!(JsxChecker::is_known_intrinsic("span"));
    assert!(JsxChecker::is_known_intrinsic("svg"));
    assert!(JsxChecker::is_known_intrinsic("circle"));
    assert!(!JsxChecker::is_known_intrinsic("foobar"));
    assert!(!JsxChecker::is_known_intrinsic("custom-element"));
}

#[test]
fn test_element_type() {
    let arena = NodeArena::new();
    let checker = JsxChecker::new(&arena);

    assert_eq!(
        checker.get_element_type("div"),
        JsxElementType::IntrinsicElement("div".to_string())
    );
    assert_eq!(
        checker.get_element_type("customtag"),
        JsxElementType::UnknownIntrinsic("customtag".to_string())
    );
    assert_eq!(
        checker.get_element_type("MyComponent"),
        JsxElementType::Component("MyComponent".to_string())
    );
}
