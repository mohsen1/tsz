#[test]
fn test_destructuring_object_literal_excess_properties() {
    let source = "const { x, y, z } : { x: number, y: number } = { x: 1, y: 2, z: 3 };";
    assert_conformance!(
        source,
        Diagnostic::error(Span::new(0, 61), diagnostic_codes::Object_literal_may_only_specify_known_properties),
    );
}