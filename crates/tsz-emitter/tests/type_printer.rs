use super::*;

#[test]
fn test_primitive_types() {
    // For now we can't easily test without a real TypeInterner
    // In the future we'll need to set up a mock or test fixture
    assert!(TypeId::STRING.is_intrinsic());
    assert!(TypeId::NUMBER.is_intrinsic());
    assert!(TypeId::BOOLEAN.is_intrinsic());
}
